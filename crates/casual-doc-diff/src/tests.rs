//! Guards for the structural diff.
//!
//! Every guard here asserts the **family and the location** of a change, never
//! just that a count is non-zero. A diff test that counts is the exact trap
//! SKILL §4 warns about: it passes when the engine says "something changed"
//! about the wrong thing, and it passes when both inputs are identical.

use casual_doc_import::{ImportConfig, import_main_document_xml};
use casual_doc_layout::flow::node_plain_text;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, BorderEdge, Comment, Document, HeaderFooter, HeaderFooterId, HeaderFooterKind,
    HeaderFooterRef, InlineNode, Paragraph, ParagraphProperties, Run, RunProperties,
    SharedParagraphProperties, SharedRunProperties,
};

use crate::compare::{DEFINITION_FIELDS, STORY_FIELDS, differing_field_paths};
use crate::job::{DiffJob, DiffSides, Progress};
use crate::record::{
    DiffChange, DiffFamily, DiffKind, FindingCode, PathSegment, Story, VersionDiff,
};

// ── fixtures ────────────────────────────────────────────────────────────────

/// A document imported from a `w:body` fragment — the real parse path, because
/// the questions about node identity can only be answered by the importer.
fn import_body(body: &str) -> Document {
    let xml = format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
    );
    import_main_document_xml(xml.as_bytes(), ImportConfig::default())
        .expect("the fixture imports")
        .document
}

/// A `<w:p>` holding one run of text.
fn xml_paragraph(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

/// A constructed paragraph, for the fixtures the XML entry point cannot reach
/// (headers, comments, sections).
fn paragraph(id: u128, text: &str) -> BlockNode {
    BlockNode::Paragraph(Paragraph {
        id: NodeId::new(id).expect("non-zero"),
        properties: SharedParagraphProperties::new(ParagraphProperties::default()),
        inlines: vec![InlineNode::Run(Run {
            id: NodeId::new(id + 1_000_000).expect("non-zero"),
            properties: SharedRunProperties::new(RunProperties::default()),
            text: text.to_owned(),
        })],
    })
}

/// A one-paragraph imported document whose definitions a fixture then fills in.
///
/// Built by parsing rather than by `Document::new` so the section boundary, the
/// ids and the validation are the importer's real output; only the definition a
/// test is actually about is added by hand.
fn base_document() -> Document {
    import_body(&format!("{}<w:sectPr/>", xml_paragraph("body")))
}

/// The diff of two documents, run to completion.
fn diff(left: &Document, right: &Document) -> VersionDiff {
    DiffJob::run(DiffSides {
        left,
        right,
        left_digests: None,
        right_digests: None,
    })
    .expect("the job completes")
}

/// Every change of one family and kind.
fn of(diff: &VersionDiff, family: DiffFamily, kind: DiffKind) -> Vec<&DiffChange> {
    diff.changes
        .iter()
        .filter(|change| change.family == family && change.kind == kind)
        .collect()
}

/// The `Block { index }` values of an anchor path.
fn block_path(path: &[PathSegment]) -> Vec<u32> {
    path.iter()
        .map(|segment| match segment {
            PathSegment::Block { index }
            | PathSegment::Row { index }
            | PathSegment::Cell { index } => *index,
        })
        .collect()
}

/// Whether a finding was reported.
fn has_finding(diff: &VersionDiff, code: FindingCode, construct: &str) -> bool {
    diff.findings
        .iter()
        .any(|finding| finding.code == code && finding.construct == construct)
}

// ── what stable NodeIds are and are not good for ────────────────────────────

/// **The load-bearing fact this whole design rests on.**
///
/// `docs/140` §11.2 step 2 says to match stable `NodeId` identities. Across two
/// independently parsed checkpoints there is nothing to match: the importer mints
/// ids from a counter that restarts at one, so the *same* id names a *different*
/// paragraph on the two sides as soon as anything is inserted above it.
///
/// This test imports two real fragments and shows the coincidence directly, then
/// shows the diff getting the answer right anyway — because it aligns on content
/// hashes, not on ids.
#[test]
fn node_ids_are_ordinal_across_two_parses_so_they_are_anchors_and_not_match_keys() {
    let left = import_body(&format!(
        "{}{}{}",
        xml_paragraph("alpha"),
        xml_paragraph("beta"),
        xml_paragraph("gamma")
    ));
    let right = import_body(&format!(
        "{}{}{}{}",
        xml_paragraph("inserted"),
        xml_paragraph("alpha"),
        xml_paragraph("beta"),
        xml_paragraph("gamma")
    ));

    let left_ids: Vec<NodeId> = left
        .body()
        .iter()
        .map(|block| match block {
            BlockNode::Paragraph(paragraph) => paragraph.id,
            _ => unreachable!("the fixture is paragraphs"),
        })
        .collect();
    let right_ids: Vec<NodeId> = right
        .body()
        .iter()
        .map(|block| match block {
            BlockNode::Paragraph(paragraph) => paragraph.id,
            _ => unreachable!("the fixture is paragraphs"),
        })
        .collect();

    // The first paragraph of each side carries the SAME id and DIFFERENT text.
    assert_eq!(
        left_ids[0], right_ids[0],
        "two parses hand out the same first id, which is why id equality is not identity"
    );
    let left_first = match &left.body()[0] {
        BlockNode::Paragraph(paragraph) => node_plain_text(&paragraph.inlines),
        _ => unreachable!(),
    };
    let right_first = match &right.body()[0] {
        BlockNode::Paragraph(paragraph) => node_plain_text(&paragraph.inlines),
        _ => unreachable!(),
    };
    assert_ne!(
        left_first, right_first,
        "the same id names different content, so matching on it would misalign everything below"
    );

    // And the diff is right anyway: exactly one inserted block, at index 0, and
    // nothing said about alpha/beta/gamma.
    let diff = diff(&left, &right);
    let insertions = of(&diff, DiffFamily::Block, DiffKind::Insertion);
    assert_eq!(insertions.len(), 1, "one insertion, got {:?}", diff.changes);
    assert_eq!(
        block_path(&insertions[0].right.as_ref().expect("a right anchor").path),
        vec![0],
        "the insertion is at the head of the body"
    );
    assert!(
        of(&diff, DiffFamily::Text, DiffKind::Insertion).is_empty()
            && of(&diff, DiffFamily::Text, DiffKind::Deletion).is_empty(),
        "no paragraph's text changed; got {:?}",
        diff.changes
    );
}

// ── block families ──────────────────────────────────────────────────────────

/// A paragraph removed from the middle is one deletion, at its own index.
#[test]
fn a_removed_paragraph_is_one_deletion_at_its_own_index() {
    let left = import_body(&format!(
        "{}{}{}",
        xml_paragraph("one"),
        xml_paragraph("two"),
        xml_paragraph("three")
    ));
    let right = import_body(&format!(
        "{}{}",
        xml_paragraph("one"),
        xml_paragraph("three")
    ));
    let diff = diff(&left, &right);
    let deletions = of(&diff, DiffFamily::Block, DiffKind::Deletion);
    assert_eq!(deletions.len(), 1, "one deletion, got {:?}", diff.changes);
    let deletion = deletions[0];
    assert_eq!(
        block_path(&deletion.left.as_ref().expect("a left anchor").path),
        vec![1],
        "the second paragraph is the one that went"
    );
    assert_eq!(deletion.left_text.as_deref(), Some("two"));
    assert!(deletion.right.is_none(), "a deletion has no right side");
}

/// A moved paragraph is a move PAIR, not a deletion and an insertion, and the
/// two halves point at each other.
#[test]
fn a_moved_paragraph_is_a_move_pair_whose_halves_reference_each_other() {
    let left = import_body(&format!(
        "{}{}{}",
        xml_paragraph("first"),
        xml_paragraph("second"),
        xml_paragraph("third")
    ));
    let right = import_body(&format!(
        "{}{}{}",
        xml_paragraph("second"),
        xml_paragraph("third"),
        xml_paragraph("first")
    ));
    let diff = diff(&left, &right);
    let from = of(&diff, DiffFamily::Block, DiffKind::MoveFrom);
    let to = of(&diff, DiffFamily::Block, DiffKind::MoveTo);
    assert_eq!(from.len(), 1, "one move source, got {:?}", diff.changes);
    assert_eq!(to.len(), 1, "one move destination, got {:?}", diff.changes);
    assert_eq!(from[0].left_text.as_deref(), Some("first"));
    assert_eq!(
        block_path(&from[0].left.as_ref().expect("a left anchor").path),
        vec![0]
    );
    assert_eq!(
        block_path(&to[0].right.as_ref().expect("a right anchor").path),
        vec![2]
    );
    assert_eq!(from[0].paired_with.as_deref(), Some(to[0].id.as_str()));
    assert_eq!(to[0].paired_with.as_deref(), Some(from[0].id.as_str()));
    assert!(
        of(&diff, DiffFamily::Block, DiffKind::Deletion).is_empty(),
        "a move is not a deletion; got {:?}",
        diff.changes
    );
}

/// Content that is not unique on both sides is NOT called a move. It is a
/// deletion plus an insertion, and the reader is told why.
#[test]
fn an_ambiguous_reordering_is_reported_as_ambiguous_and_never_as_a_move() {
    let left = import_body(&format!(
        "{}{}{}",
        xml_paragraph("dup"),
        xml_paragraph("dup"),
        xml_paragraph("anchor")
    ));
    let right = import_body(&format!(
        "{}{}{}",
        xml_paragraph("anchor"),
        xml_paragraph("dup"),
        xml_paragraph("dup")
    ));
    let diff = diff(&left, &right);
    assert!(
        of(&diff, DiffFamily::Block, DiffKind::MoveFrom).is_empty(),
        "two identical candidates are not a move; got {:?}",
        diff.changes
    );
    assert!(
        has_finding(&diff, FindingCode::AmbiguousMatch, "blockMoveCandidate"),
        "the ambiguity is reported; got {:?}",
        diff.findings
    );
    assert!(
        !diff.complete,
        "a diff with a finding is not a complete diff"
    );
}

// ── text ────────────────────────────────────────────────────────────────────

/// An edited word inside a paragraph is a text change whose byte offsets bracket
/// the changed word on each side — not a deleted paragraph and a new one.
#[test]
fn an_edited_word_is_a_text_change_whose_offsets_bracket_the_word() {
    let left = import_body(&xml_paragraph("the quick brown fox"));
    let right = import_body(&xml_paragraph("the quick red fox"));
    let diff = diff(&left, &right);
    assert!(
        of(&diff, DiffFamily::Block, DiffKind::Deletion).is_empty(),
        "an edited paragraph is not a deleted one; got {:?}",
        diff.changes
    );
    let text: Vec<&DiffChange> = diff
        .changes
        .iter()
        .filter(|change| change.family == DiffFamily::Text)
        .collect();
    assert_eq!(text.len(), 1, "one text change, got {:?}", diff.changes);
    let change = text[0];
    let left_anchor = change.left.as_ref().expect("a left anchor");
    let right_anchor = change.right.as_ref().expect("a right anchor");
    assert_eq!(
        &"the quick brown fox"[left_anchor.start as usize..left_anchor.end as usize],
        "brown",
        "the left offsets bracket the word that went"
    );
    assert_eq!(
        &"the quick red fox"[right_anchor.start as usize..right_anchor.end as usize],
        "red",
        "the right offsets bracket the word that arrived"
    );
    assert_eq!(change.left_text.as_deref(), Some("brown"));
    assert_eq!(change.right_text.as_deref(), Some("red"));
    assert_eq!(left_anchor.story, Story::Body);
}

/// A grapheme cluster is never split. Changing the skin tone of an emoji reports
/// the whole cluster, not a lone modifier byte range.
#[test]
fn an_emoji_is_reported_as_one_cluster_and_never_split() {
    let left = import_body(&xml_paragraph("hi \u{1f44d}\u{1f3fb} there"));
    let right = import_body(&xml_paragraph("hi \u{1f44d}\u{1f3fe} there"));
    let diff = diff(&left, &right);
    let text: Vec<&DiffChange> = diff
        .changes
        .iter()
        .filter(|change| change.family == DiffFamily::Text)
        .collect();
    assert_eq!(text.len(), 1, "one text change, got {:?}", diff.changes);
    let anchor = text[0].right.as_ref().expect("a right anchor");
    let slice = &"hi \u{1f44d}\u{1f3fe} there"[anchor.start as usize..anchor.end as usize];
    assert_eq!(
        slice, "\u{1f44d}\u{1f3fe}",
        "the whole cluster is the change, not half of it"
    );
}

// ── formatting ──────────────────────────────────────────────────────────────

/// Identical text, different alignment: a formatting change that NAMES the
/// property, and no text change at all.
#[test]
fn a_paragraph_property_change_is_formatting_and_names_the_property() {
    let left = import_body("<w:p><w:r><w:t>same</w:t></w:r></w:p>");
    let right =
        import_body("<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t>same</w:t></w:r></w:p>");
    let diff = diff(&left, &right);
    let formatting = of(&diff, DiffFamily::Formatting, DiffKind::Formatting);
    assert_eq!(
        formatting.len(),
        1,
        "one formatting change, got {:?}",
        diff.changes
    );
    assert!(
        formatting[0].fields.contains(&"alignment".to_owned()),
        "the changed property is named; got {:?}",
        formatting[0].fields
    );
    assert!(
        diff.changes
            .iter()
            .all(|change| change.family != DiffFamily::Text),
        "the text did not change; got {:?}",
        diff.changes
    );
}

/// A nested property is named by its FULL path, which is what proves the field
/// names are reflected from the model type rather than listed by hand.
#[test]
fn a_nested_property_is_named_by_its_full_path() {
    let left = import_body("<w:p><w:r><w:t>same</w:t></w:r></w:p>");
    let right = import_body(
        "<w:p><w:pPr><w:spacing w:before=\"240\"/></w:pPr><w:r><w:t>same</w:t></w:r></w:p>",
    );
    let diff = diff(&left, &right);
    let formatting = of(&diff, DiffFamily::Formatting, DiffKind::Formatting);
    assert_eq!(formatting.len(), 1, "got {:?}", diff.changes);
    assert!(
        formatting[0]
            .fields
            .iter()
            .any(|field| field == "spacing.beforeTwips"),
        "the nested field is named in full, not as its container; got {:?}",
        formatting[0].fields
    );
}

/// Character formatting is named per run, with the run's position.
#[test]
fn a_run_property_change_names_the_run_and_the_property() {
    let left = import_body("<w:p><w:r><w:t>same</w:t></w:r></w:p>");
    let right = import_body("<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>same</w:t></w:r></w:p>");
    let diff = diff(&left, &right);
    let formatting = of(&diff, DiffFamily::Formatting, DiffKind::Formatting);
    assert_eq!(formatting.len(), 1, "got {:?}", diff.changes);
    assert!(
        formatting[0]
            .fields
            .iter()
            .any(|field| field == "runProperties[0].bold"),
        "the run and the property are both named; got {:?}",
        formatting[0].fields
    );
}

/// The reflected comparator names a property no line of this crate mentions.
#[test]
fn the_field_comparator_names_a_property_this_crate_never_spells_out() {
    let edge = |size| BorderEdge {
        style: "single".to_owned(),
        size_eighth_points: Some(size),
        color: None,
        space_points: None,
    };
    let mut left = ParagraphProperties::default();
    let mut right = ParagraphProperties::default();
    left.borders.top = Some(edge(4));
    right.borders.top = Some(edge(12));
    let (fields, truncated) = differing_field_paths(&left, &right);
    assert!(!truncated);
    assert_eq!(
        fields,
        vec!["borders.top.sizeEighthPoints".to_owned()],
        "a two-level-deep property is named exactly, with no help from a list"
    );
}

// ── tables ──────────────────────────────────────────────────────────────────

/// A row added to a table is a table-family insertion whose path names the row.
#[test]
fn a_row_added_to_a_table_is_a_table_insertion_at_its_row_index() {
    let row =
        |text: &str| format!("<w:tr><w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr>");
    let left = import_body(&format!("<w:tbl>{}{}</w:tbl>", row("a"), row("b")));
    let right = import_body(&format!(
        "<w:tbl>{}{}{}</w:tbl>",
        row("a"),
        row("b"),
        row("c")
    ));
    let diff = diff(&left, &right);
    let insertions = of(&diff, DiffFamily::Table, DiffKind::Insertion);
    assert_eq!(
        insertions.len(),
        1,
        "one row inserted, got {:?}",
        diff.changes
    );
    let path = &insertions[0].right.as_ref().expect("a right anchor").path;
    assert!(
        matches!(path.last(), Some(PathSegment::Row { index: 2 })),
        "the path names row 2; got {path:?}"
    );
}

/// A cell's text edit is located by table, row and cell — and the rest of the
/// table costs nothing, which is the hash-tree short-circuit.
#[test]
fn a_cell_edit_is_located_by_row_and_cell_and_the_rest_of_the_table_is_skipped() {
    let cells = |first: &str| {
        format!(
            "<w:tbl>{}</w:tbl>",
            (0..20)
                .map(|index| {
                    let text = if index == 7 {
                        first.to_owned()
                    } else {
                        format!("cell{index}")
                    };
                    format!("<w:tr><w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr>")
                })
                .collect::<String>()
        )
    };
    let left = import_body(&cells("before"));
    let right = import_body(&cells("after"));
    let diff = diff(&left, &right);
    let text: Vec<&DiffChange> = diff
        .changes
        .iter()
        .filter(|change| change.family == DiffFamily::Text)
        .collect();
    assert_eq!(text.len(), 1, "one cell changed, got {:?}", diff.changes);
    let path = &text[0].right.as_ref().expect("a right anchor").path;
    assert_eq!(
        block_path(path),
        vec![0, 7, 0, 0],
        "table 0, row 7, cell 0, paragraph 0; got {path:?}"
    );
}

// ── stories ─────────────────────────────────────────────────────────────────

/// A header is diffed by the same pipeline as the body, and its change is
/// located in the header story of its own section and page type — not by an id
/// that means nothing across two parses.
#[test]
fn a_header_edit_is_located_in_the_header_story_of_its_section_and_page_type() {
    let build = |text: &str| {
        let mut document = base_document();
        assert_eq!(
            document.definitions().sections.len(),
            1,
            "the importer produced exactly one section to hang the header on"
        );
        let header_id = HeaderFooterId::new(NodeId::new(900).expect("non-zero"));
        let definitions = document.definitions_mut();
        definitions.headers.insert(
            header_id,
            HeaderFooter {
                blocks: vec![paragraph(10, text)],
            },
        );
        definitions.sections[0].headers.push(HeaderFooterRef {
            kind: HeaderFooterKind::Default,
            reference: header_id,
        });
        document
    };
    let diff = diff(&build("old header"), &build("new header"));
    let text: Vec<&DiffChange> = diff
        .changes
        .iter()
        .filter(|change| change.family == DiffFamily::Text)
        .collect();
    assert_eq!(text.len(), 1, "one header edit, got {:?}", diff.changes);
    assert_eq!(
        text[0].right.as_ref().expect("a right anchor").story,
        Story::Header {
            section: 0,
            page: "default".to_owned()
        },
        "the change is in the default header of section 0"
    );
}

/// A comment is matched across two parses by the durable id the package carries,
/// which is the one construct in this model with a genuinely stable cross-file
/// identity.
#[test]
fn a_comment_is_matched_by_its_durable_id_and_not_by_its_position() {
    let build = |first: &str, second: &str| {
        let mut document = base_document();
        let definitions = document.definitions_mut();
        definitions.comments.insert(
            casual_doc_model::v1::CommentId::new(NodeId::new(701).expect("non-zero")),
            Comment {
                blocks: vec![paragraph(30, first)],
                durable_id: Some("AAAA0001".to_owned()),
                ..Comment::default()
            },
        );
        definitions.comments.insert(
            casual_doc_model::v1::CommentId::new(NodeId::new(702).expect("non-zero")),
            Comment {
                blocks: vec![paragraph(40, second)],
                durable_id: Some("AAAA0002".to_owned()),
                ..Comment::default()
            },
        );
        document
    };
    let diff = diff(&build("keep", "edit me"), &build("keep", "edited"));
    let text: Vec<&DiffChange> = diff
        .changes
        .iter()
        .filter(|change| change.family == DiffFamily::Text)
        .collect();
    assert_eq!(text.len(), 1, "one comment edited, got {:?}", diff.changes);
    assert_eq!(
        text[0].right.as_ref().expect("a right anchor").story,
        Story::Comment {
            id: "AAAA0002".to_owned()
        },
        "the edited comment is the one with that durable id"
    );
}

// ── definitions ─────────────────────────────────────────────────────────────

/// A style redefinition is keyed by the style's NAME, which the source writes,
/// and names the property that moved.
#[test]
fn a_redefined_style_is_keyed_by_name_and_names_the_changed_property() {
    // Deserialized rather than constructed: `Style` has twenty fields and this
    // test is about the KEY it is matched on, not about its shape.
    let build = |priority: i32| {
        let style: casual_doc_model::v1::Style = serde_json::from_value(serde_json::json!({
            "kind": "paragraph",
            "name": "heading 1",
            "uiPriority": priority,
        }))
        .expect("the style deserializes");
        let mut document = base_document();
        document.definitions_mut().styles.insert(
            casual_doc_model::v1::StyleId::new(NodeId::new(500).expect("non-zero")),
            style,
        );
        document
    };
    let diff = diff(&build(9), &build(3));
    let changes = of(&diff, DiffFamily::Definition, DiffKind::Property);
    assert_eq!(
        changes.len(),
        1,
        "one style changed, got {:?}",
        diff.changes
    );
    assert_eq!(
        changes[0].fields[0], "styles.Paragraph/heading 1",
        "the record names the style by kind and name; got {:?}",
        changes[0].fields
    );
    assert!(
        changes[0].fields.iter().any(|field| field == "uiPriority"),
        "and names what moved; got {:?}",
        changes[0].fields
    );
}

/// A chart that changed is **located and reported as not characterised**.
///
/// `charts` is in `OPAQUE_CONSTRUCTS`, so the contract is two things at once: a
/// change record naming the construct, *and* a `NotCompared` finding saying the
/// engine could not say what inside it changed. Asserting only the change record
/// would pass while the honesty half silently disappeared — which is the whole
/// reason that finding exists.
///
/// Also the other half: two documents whose charts are identical must produce
/// neither, or the finding fires on every healthy comparison and gets filtered out.
#[test]
fn a_changed_chart_is_located_and_reported_as_not_characterised() {
    use casual_doc_model::v1::{
        BarDirection, BarGrouping, Chart, ChartCoverage, ChartGroup, ChartGroupKind, ChartId,
        ChartValue, DataRange, DisplayBlanks, EmbeddedKind, EmbeddedObject, EmbeddedPart, Extent,
        PlotArea, Series,
    };

    /// A one-paragraph document holding one embedded chart object and a
    /// projection of it whose single cached value is `value`.
    fn document_with_chart(value: &str) -> Document {
        let mut document = base_document();
        let object_id = NodeId::new(900_001).expect("non-zero");
        let BlockNode::Paragraph(first) = &mut document.body_mut()[0] else {
            panic!("the base fixture starts with a paragraph");
        };
        first
            .inlines
            .push(InlineNode::EmbeddedObject(Box::new(EmbeddedObject {
                id: object_id,
                kind: EmbeddedKind::Chart,
                part: EmbeddedPart {
                    relationship_id: "rId5".to_owned(),
                    relationship_type:
                        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart"
                            .to_owned(),
                    part_name: "word/charts/chart1.xml".to_owned(),
                },
                extra_parts: Vec::new(),
                preview: None,
                extent: Extent {
                    width_emu: 5_486_400,
                    height_emu: 3_200_400,
                },
                prog_id: None,
            })));
        document.definitions_mut().charts.insert(
            ChartId::new(NodeId::new(900_002).expect("non-zero")),
            Chart {
                object: object_id,
                coverage: ChartCoverage::Complete,
                title: None,
                auto_title_deleted: false,
                plot_area: PlotArea {
                    groups: vec![ChartGroup {
                        kind: ChartGroupKind::Bar {
                            direction: BarDirection::Column,
                            grouping: BarGrouping::Clustered,
                            gap_width: 150,
                            overlap: 0,
                        },
                        series: vec![Series {
                            values: DataRange {
                                formula: Some("Sheet1!$B$2".to_owned()),
                                point_count: 1,
                                points: vec![(0, ChartValue::Number(value.to_owned()))],
                                number_format: None,
                            },
                            ..Series::default()
                        }],
                        axis_ids: vec![1, 2],
                        vary_colors: false,
                    }],
                    axes: Vec::new(),
                },
                legend: None,
                plot_visible_only: true,
                display_blanks_as: DisplayBlanks::Gap,
                vary_colors: false,
                external_data: None,
            },
        );
        document
            .validate()
            .expect("the fixture is a valid document");
        document
    }

    let left = document_with_chart("4.30");
    let right = document_with_chart("9.90");
    let changed = diff(&left, &right);
    assert!(
        of(&changed, DiffFamily::Definition, DiffKind::Property)
            .iter()
            .any(|change| change.fields.first().map(String::as_str) == Some("charts")),
        "a changed chart must be located; got {:?}",
        changed
            .changes
            .iter()
            .map(|change| change.fields.clone())
            .collect::<Vec<_>>()
    );
    assert!(
        has_finding(&changed, FindingCode::NotCompared, "charts"),
        "and the engine must say it could not characterise the change; findings: {:?}",
        changed.findings
    );

    // The other half: identical charts raise nothing at all.
    let unchanged = diff(&document_with_chart("4.30"), &document_with_chart("4.30"));
    assert!(
        !unchanged
            .changes
            .iter()
            .any(|change| change.fields.first().map(String::as_str) == Some("charts")),
        "two identical charts must not be reported as a change"
    );
    assert!(
        !has_finding(&unchanged, FindingCode::NotCompared, "charts"),
        "nor raise the honesty finding on a healthy comparison"
    );
}

/// Adding a field to `Definitions` must not silently add an uncompared
/// construct. The field list is read from the model's own source, so a new field
/// fails this until [`crate::compare`] says what happens to it.
///
/// Reading the model source is unusual and deliberate: the alternative is a
/// hand-maintained list, and SKILL §8 is explicit that hand-maintained counts
/// drift into false claims. `Definitions` cannot be reflected at runtime
/// (`skip_serializing_if` hides every empty field, and serializing a populated
/// one would be an O(document) encode), so the source is the only derivation
/// available.
#[test]
fn every_definitions_field_is_either_compared_or_a_story() {
    const SOURCE: &str = include_str!("../../casual-doc-model/src/v1/definitions.rs");
    // Normalised first, because the delimiter below is a literal newline and a
    // Windows checkout (`core.autocrlf=true`, which is the default there) hands
    // this file over with CRLF. `find("\n}\n")` then matches nothing and the
    // guard panics — on Windows only, which is exactly where it did: the macOS
    // and Linux jobs were green and `platform (Windows-x64)` failed with "the
    // struct closes". A guard that reads source has to read it the same way on
    // every platform, or it reports the checkout rather than the model.
    let source = SOURCE.replace("\r\n", "\n");
    let start = source
        .find("pub struct Definitions {")
        .expect("the struct is in that file");
    let body = &source[start..];
    let end = body.find("\n}\n").expect("the struct closes");
    let fields: Vec<String> = body[..end]
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("pub ")?;
            let name = rest.split(':').next()?;
            name.chars()
                .all(|character| character.is_ascii_lowercase() || character == '_')
                .then(|| camel_case(name))
        })
        .collect();
    assert!(
        fields.len() >= 20,
        "the parse found the fields, not something else: {fields:?}"
    );
    for field in &fields {
        assert!(
            DEFINITION_FIELDS.contains(&field.as_str()) || STORY_FIELDS.contains(&field.as_str()),
            "`Definitions::{field}` is neither compared by `compare_definitions` nor handled as a \
             story. Add it to one of the two lists in `compare.rs` and say what happens to it."
        );
    }
    for field in DEFINITION_FIELDS.iter().chain(STORY_FIELDS.iter()) {
        assert!(
            fields.contains(&(*field).to_owned()),
            "`{field}` is claimed as compared but is not a field of `Definitions` any more"
        );
    }
}

/// `snake_case` to `camelCase`, matching the model's `rename_all`.
fn camel_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = false;
    for character in name.chars() {
        if character == '_' {
            upper = true;
        } else if upper {
            out.push(character.to_ascii_uppercase());
            upper = false;
        } else {
            out.push(character);
        }
    }
    out
}

// ── honesty about what is not compared ──────────────────────────────────────

/// An inline object this engine does not characterise is LOCATED and declared
/// uncharacterised, and the diff refuses to call itself complete.
#[test]
fn an_uncharacterised_inline_object_is_located_and_declared_not_compared() {
    let symbol = |code: &str| {
        format!(
            "<w:p><w:r><w:t>text</w:t></w:r><w:r><w:sym w:font=\"Wingdings\" w:char=\"{code}\"/></w:r></w:p>"
        )
    };
    let left = import_body(&symbol("F0FC"));
    let right = import_body(&symbol("F0A7"));
    let diff = diff(&left, &right);
    let objects = of(&diff, DiffFamily::Object, DiffKind::Property);
    assert_eq!(
        objects.len(),
        1,
        "the object change is located, got {:?}",
        diff.changes
    );
    assert_eq!(
        block_path(&objects[0].right.as_ref().expect("a right anchor").path),
        vec![0],
        "at the paragraph that holds it"
    );
    assert!(
        has_finding(&diff, FindingCode::NotCompared, "inlineObject"),
        "and the reader is told it is not characterised; got {:?}",
        diff.findings
    );
    assert!(
        !diff.complete,
        "a diff that could not characterise something is not complete"
    );
}

/// With no media digests, a replaced image under the same part name cannot be
/// seen — and that is reported rather than absorbed.
#[test]
fn media_bytes_without_digests_are_reported_as_a_missing_resource() {
    let build = |text: &str| {
        let mut document = import_body(&format!("{}<w:sectPr/>", xml_paragraph(text)));
        document.definitions_mut().media.insert(
            casual_doc_model::v1::MediaId::new(NodeId::new(600).expect("non-zero")),
            casual_doc_model::v1::MediaReference {
                relationship_id: "rId4".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "/word/media/image1.png".to_owned(),
            },
        );
        document
    };
    let left = build("a");
    let right = build("b");
    let diff = diff(&left, &right);
    assert!(
        has_finding(&diff, FindingCode::MissingResource, "mediaBytes"),
        "the gap is named; got {:?}",
        diff.findings
    );
}

/// Given digests, a replaced image under the SAME part name is detected and named
/// as a byte change — the one thing the engine crate cannot see on its own,
/// because a `Document` models a media reference and not its bytes.
#[test]
fn a_replaced_image_is_named_as_a_byte_change_when_digests_are_supplied() {
    let build = || {
        let mut document = base_document();
        document.definitions_mut().media.insert(
            casual_doc_model::v1::MediaId::new(NodeId::new(600).expect("non-zero")),
            casual_doc_model::v1::MediaReference {
                relationship_id: "rId4".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "/word/media/image1.png".to_owned(),
            },
        );
        document
    };
    let left = build();
    let right = build();
    let mut left_digests = crate::MediaDigests::new();
    left_digests.insert("/word/media/image1.png".to_owned(), 111);
    let mut right_digests = crate::MediaDigests::new();
    right_digests.insert("/word/media/image1.png".to_owned(), 222);
    let diff = DiffJob::run(DiffSides {
        left: &left,
        right: &right,
        left_digests: Some(&left_digests),
        right_digests: Some(&right_digests),
    })
    .expect("the job completes");
    let resources = of(&diff, DiffFamily::Resource, DiffKind::Property);
    assert_eq!(
        resources.len(),
        1,
        "the replaced part is one record, got {:?}",
        diff.changes
    );
    assert_eq!(
        resources[0].fields,
        vec![
            "media./word/media/image1.png".to_owned(),
            "bytes".to_owned()
        ],
        "named by part and by what changed"
    );
    assert!(
        !has_finding(&diff, FindingCode::MissingResource, "mediaBytes"),
        "and no gap is reported, because the digests answered the question"
    );
    assert!(
        diff.complete,
        "so the diff is complete; got {:?}",
        diff.findings
    );
}

/// An identical pair produces no changes, no findings, and calls itself
/// complete. The guard exists because "both inputs were identical" is the way a
/// diff test passes while testing nothing — so it is asserted deliberately here
/// and contradicted by every other guard in this file.
#[test]
fn an_identical_pair_is_empty_and_complete() {
    let left = import_body(&format!("{}{}", xml_paragraph("a"), xml_paragraph("b")));
    let right = import_body(&format!("{}{}", xml_paragraph("a"), xml_paragraph("b")));
    let diff = diff(&left, &right);
    assert!(diff.changes.is_empty(), "got {:?}", diff.changes);
    assert!(diff.findings.is_empty(), "got {:?}", diff.findings);
    assert!(diff.complete);
}

// ── the vocabulary agrees with review ───────────────────────────────────────

/// The kind strings are the ones the review surface already emits
/// (`casual-doc-wasm`'s `reviewChanges`), so the product has ONE way of saying
/// "inserted". `property` is the one addition, for a change review markup has no
/// inline form for.
#[test]
fn the_change_kinds_are_the_review_vocabulary() {
    let rendered: Vec<String> = [
        DiffKind::Insertion,
        DiffKind::Deletion,
        DiffKind::MoveFrom,
        DiffKind::MoveTo,
        DiffKind::Formatting,
        DiffKind::Property,
    ]
    .iter()
    .map(|kind| serde_json::to_string(kind).expect("a plain enum serializes"))
    .collect();
    assert_eq!(
        rendered,
        vec![
            "\"insertion\"",
            "\"deletion\"",
            "\"move_from\"",
            "\"move_to\"",
            "\"formatting\"",
            "\"property\"",
        ],
        "these strings are a compatibility surface shared with the review projection"
    );
}

/// Every record carries an anchor in the shape the review surface uses, so the
/// panel can hand it to the same navigation code.
#[test]
fn every_change_carries_a_node_anchored_position() {
    let left = import_body(&xml_paragraph("the quick brown fox"));
    let right = import_body(&xml_paragraph("the quick red fox"));
    let diff = diff(&left, &right);
    let json: serde_json::Value =
        serde_json::from_str(&diff.to_json().expect("serializes")).expect("valid JSON");
    let anchor = &json["changes"][0]["right"];
    assert!(
        anchor["node"].is_string(),
        "the anchor names a node; got {anchor}"
    );
    assert!(anchor["start"].is_number() && anchor["end"].is_number());
}

// ── the job: budget, progress, cancellation ─────────────────────────────────

/// Driving the job one unit at a time produces the SAME sidecar as running it in
/// one call. If it did not, the budgeted path would be a second implementation
/// of the diff with its own bugs.
#[test]
fn a_one_unit_budget_produces_the_same_diff_as_one_blocking_call() {
    let left = import_body(&format!(
        "{}{}{}",
        xml_paragraph("one"),
        xml_paragraph("two"),
        xml_paragraph("three")
    ));
    let right = import_body(&format!(
        "{}{}{}",
        xml_paragraph("one"),
        xml_paragraph("two edited"),
        xml_paragraph("three")
    ));
    let sides = DiffSides {
        left: &left,
        right: &right,
        left_digests: None,
        right_digests: None,
    };
    let mut job = DiffJob::new();
    let mut steps = 0;
    let mut saw_progress = false;
    loop {
        steps += 1;
        assert!(steps < 10_000, "the job terminates");
        match job.step(sides, 1) {
            Progress::Complete => break,
            Progress::Cancelled => unreachable!("nothing cancelled it"),
            Progress::Working { done, .. } => {
                if done > 0 {
                    saw_progress = true;
                }
            }
        }
    }
    assert!(steps > 3, "a one-unit budget took several steps, not one");
    assert!(saw_progress, "progress was reported while it worked");
    let stepped = job.take().expect("a result");
    let blocking = diff(&left, &right);
    let stepped_ids: Vec<&str> = stepped
        .changes
        .iter()
        .map(|change| change.id.as_str())
        .collect();
    let blocking_ids: Vec<&str> = blocking
        .changes
        .iter()
        .map(|change| change.id.as_str())
        .collect();
    assert_eq!(
        stepped_ids, blocking_ids,
        "the budgeted path and the blocking path agree, change for change"
    );
}

/// Cancelling stops the job and produces nothing. A half-finished diff is worse
/// than no diff, because a reader cannot tell it is half-finished.
#[test]
fn cancelling_a_job_yields_no_result_and_stays_cancelled() {
    let left = import_body(&xml_paragraph("a"));
    let right = import_body(&xml_paragraph("b"));
    let sides = DiffSides {
        left: &left,
        right: &right,
        left_digests: None,
        right_digests: None,
    };
    let mut job = DiffJob::new();
    assert!(matches!(job.step(sides, 1), Progress::Working { .. }));
    job.cancel();
    assert!(job.is_cancelled());
    assert!(matches!(job.step(sides, 1_000_000), Progress::Cancelled));
    assert!(job.take().is_none(), "a cancelled job produces nothing");
}

// ── complexity, guarded by doubling ─────────────────────────────────────────

/// A body of `n` paragraphs where EVERY paragraph was rewritten costs work
/// proportional to `n`, not to `n²`.
///
/// This is the shape that tempts a quadratic: a region with no unique common key
/// where every deletion is a candidate for every insertion. The measured quantity
/// is the comparison counter, not a wall clock — a clock cannot tell a slow
/// constant from a quadratic (SKILL §8).
#[test]
fn rewriting_every_paragraph_costs_linear_work_not_quadratic() {
    let build = |count: usize, suffix: &str| {
        import_body(
            &(0..count)
                .map(|index| xml_paragraph(&format!("line {index}{suffix}")))
                .collect::<String>(),
        )
    };
    let small = comparisons(&build(200, ""), &build(200, " rewritten"));
    let large = comparisons(&build(400, ""), &build(400, " rewritten"));
    let ratio = large as f64 / small as f64;
    assert!(
        (1.5..3.0).contains(&ratio),
        "doubling the document roughly doubled the work; ratio was {ratio:.2} \
         ({small} -> {large}). A quadratic would be near 4."
    );
}

/// Reordering a whole body — so that nothing aligns and every block is a move
/// candidate — also costs work proportional to `n`, because move detection is a
/// hash join and not a pairwise search.
#[test]
fn detecting_moves_in_a_reordered_body_costs_linear_work_not_quadratic() {
    let build = |count: usize, reversed: bool| {
        let mut indices: Vec<usize> = (0..count).collect();
        if reversed {
            indices.reverse();
        }
        import_body(
            &indices
                .iter()
                .map(|index| xml_paragraph(&format!("block {index}")))
                .collect::<String>(),
        )
    };
    let small = comparisons(&build(200, false), &build(200, true));
    let large = comparisons(&build(400, false), &build(400, true));
    let ratio = large as f64 / small as f64;
    assert!(
        (1.5..3.0).contains(&ratio),
        "doubling the reordered document roughly doubled the work; ratio was {ratio:.2} \
         ({small} -> {large}). A pairwise move search would be near 4."
    );
}

/// An unchanged subtree is settled by ONE key comparison, so a document whose
/// only change is in the first paragraph costs far less than its block count.
#[test]
fn an_unchanged_table_is_settled_without_walking_its_cells() {
    let build = |first: &str| {
        let rows: String = (0..50)
            .map(|index| {
                format!(
                    "<w:tr><w:tc><w:p><w:r><w:t>r{index}c0</w:t></w:r></w:p></w:tc>\
                     <w:tc><w:p><w:r><w:t>r{index}c1</w:t></w:r></w:p></w:tc></w:tr>"
                )
            })
            .collect();
        import_body(&format!("{}<w:tbl>{rows}</w:tbl>", xml_paragraph(first)))
    };
    let diff = diff(&build("before"), &build("after"));
    // 1 paragraph + 1 table + 50 rows + 100 cells + 100 paragraphs = 252 blocks
    // per side. Settling the table takes one comparison, so the whole diff must
    // cost far less than one comparison per block.
    assert!(
        diff.left.blocks > 250,
        "the fixture really is that big: {} blocks",
        diff.left.blocks
    );
    assert!(
        diff.diagnostics.comparisons < 40,
        "the unchanged table was skipped, not walked: {} comparisons for {} blocks",
        diff.diagnostics.comparisons,
        diff.left.blocks
    );
    assert_eq!(
        diff.changes.len(),
        1,
        "and the one real change is still found; got {:?}",
        diff.changes
    );
}

/// Property hashing is memoized by the model's own flyweight, so a document of
/// identically-formatted paragraphs serializes ONE property value, not one per
/// paragraph.
#[test]
fn property_hashing_is_memoized_by_the_flyweight_not_repeated_per_paragraph() {
    let body: String = (0..300)
        .map(|index| xml_paragraph(&format!("p{index}")))
        .collect();
    let left = import_body(&body);
    let right = import_body(&format!("{body}{}", xml_paragraph("extra")));
    let diff = diff(&left, &right);
    assert!(
        diff.left.blocks >= 300,
        "the fixture is that big: {}",
        diff.left.blocks
    );
    assert!(
        diff.diagnostics.value_hashes < 40,
        "601 paragraphs with one distinct property value cost {} serializations",
        diff.diagnostics.value_hashes
    );
}

/// The comparison counter for one pair.
fn comparisons(left: &Document, right: &Document) -> u64 {
    diff(left, right).diagnostics.comparisons
}
