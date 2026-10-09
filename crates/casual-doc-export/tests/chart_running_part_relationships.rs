// SPDX-License-Identifier: Apache-2.0
//! A chart is savable on every surface that can hold one, not only in the body
//! (`109` HF-256, and HF-196's export half).
//!
//! # What was broken, measured on this branch
//!
//! `chart_part_writer.rs` closed HF-256 for the body: a chart the editor minted
//! got its `word/charts/chartN.xml`, its content-type `Override` and its
//! `document.xml.rels` entry. The relationship walk behind it visited
//! `document.body()` and nothing else, and the model admits a chart in a header,
//! a footer, a footnote, an endnote and a comment — `Document::visit_chart_object_ids`
//! walks all five for exactly that reason.
//!
//! So a chart on any of those surfaces was written as a `c:chart r:id` into a part
//! whose `_rels` companion **did not exist at all**. Probe output before the fix,
//! from a model holding one chart object in a header, a footnote and a comment:
//!
//! ```text
//! --- word/header1.xml
//!     has c:chart   : true
//!     rels has chart: false
//!     rels = <ABSENT>
//! --- word/footnotes.xml
//!     has c:chart   : true
//!     rels has chart: false
//!     rels = <ABSENT>
//! --- word/comments.xml
//!     has c:chart   : true
//!     rels has chart: false
//!     rels = <ABSENT>
//! ```
//!
//! An `r:id` that resolves to no relationship is the shape Word reports as
//! unreadable content — the same defect HF-256 names, on the surfaces its first
//! half did not reach.
//!
//! # What these guards assert
//!
//! The four OPC links, per surface, read out of the written FILE rather than the
//! model: the part carries the reference, the part's own `_rels` declares that
//! id, the target resolves to a part the package contains, and
//! `[Content_Types].xml` declares that part as a chart. The link numbers are in
//! the failure messages so a break says which one.
//!
//! The surfaces are enumerated rather than sampled (`SKILL` §9.3: absence from a
//! matrix is an overstatement by omission), and [`SURFACES`] is the list — a
//! surface that stops working names itself.

use std::collections::BTreeMap;

use casual_doc_export::{export_document, write_document, write_document_with_retained_parts};
use casual_doc_import::{RetainedPart, RetainedParts};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    Axis, AxisKind, AxisPosition, BarDirection, BarGrouping, BlockNode, Chart, ChartCoverage,
    ChartGroup, ChartGroupKind, ChartId, ChartText, ChartValue, Comment, CommentId, DataRange,
    Definitions, DisplayBlanks, Document, EmbeddedKind, EmbeddedObject, EmbeddedPart, Extent,
    ExternalTarget, GroupChild, GroupTextBox, GroupTransform, HeaderFooter, HeaderFooterId,
    HeaderFooterKind, HeaderFooterRef, Hyperlink, HyperlinkTarget, InlineNode, Legend,
    LegendPosition, Note, NoteId, PageMargins, PageSize, Paragraph, ParagraphProperties, PlotArea,
    PointEmu, Run, RunProperties, SectionBoundary, SectionColumns, SectionId, Series,
    ShapeGeometry, WordprocessingGroup,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const CHART_PART: &str = "word/charts/chart9.xml";
const CHART_REL_ID: &str = "rId1";
const CHART_CT: &str = "application/vnd.openxmlformats-officedocument.drawingml.chart+xml";
const CHART_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";

fn id(counter: u64) -> NodeId {
    NodeId::from_parts(1, counter).expect("a test node id")
}

/// The chart object's node id, and so the projection's anchor.
///
/// One constant rather than a per-surface seed: the projection resolves by
/// anchor, and a fixture whose object id did not match its projection's anchor
/// would exercise the no-projection path while claiming to test the writer. Each
/// document here holds the chart on exactly one surface, so one id is enough.
fn chart_anchor() -> NodeId {
    id(201)
}

/// The object the projection anchors to, shaped like `insertChart`'s.
///
/// The relationship id is deliberately `rId1`, the first id a `RelBuilder` would
/// mint: a surface that does not reserve the embedded ids hands that id to the
/// first hyperlink it writes, and the chart's `r:id` then resolves to the
/// hyperlink. `a_running_part_does_not_hand_a_charts_id_to_a_hyperlink` is that
/// case.
fn chart_object(object: NodeId) -> EmbeddedObject {
    EmbeddedObject {
        id: object,
        kind: EmbeddedKind::Chart,
        part: EmbeddedPart {
            relationship_id: CHART_REL_ID.to_owned(),
            relationship_type: CHART_REL_TYPE.to_owned(),
            part_name: CHART_PART.to_owned(),
        },
        extra_parts: Vec::new(),
        preview: None,
        extent: Extent {
            width_emu: 4_572_000,
            height_emu: 2_743_200,
        },
        prog_id: None,
    }
}

/// One paragraph holding the chart object, plus whatever `extra` inlines the
/// caller wants beside it.
fn chart_paragraph(seed: u64, extra: Vec<InlineNode>) -> BlockNode {
    let mut inlines = vec![InlineNode::EmbeddedObject(Box::new(chart_object(
        chart_anchor(),
    )))];
    inlines.extend(extra);
    BlockNode::Paragraph(Paragraph {
        id: id(seed),
        properties: ParagraphProperties::default().into(),
        inlines,
    })
}

/// A complete, regenerable projection anchored to `object`: one clustered column
/// series with cached categories, a title and a legend — the shape the row calls
/// for ("its type, series, categories, title and legend intact").
fn projection(object: NodeId, coverage: ChartCoverage) -> Chart {
    Chart {
        font: None,
        chart_retained: Default::default(),
        namespaces: Default::default(),
        space_retained: Default::default(),
        object,
        coverage,
        title: None,
        auto_title_deleted: true,
        plot_area: PlotArea {
            retained: Default::default(),
            groups: vec![ChartGroup {
                retained: Default::default(),
                kind: ChartGroupKind::Bar {
                    direction: BarDirection::Column,
                    grouping: BarGrouping::Clustered,
                    gap_width: 150,
                    overlap: -27,
                },
                series: vec![Series {
                    index: 0,
                    order: 0,
                    name: Some(ChartText {
                        text: "Series 1".to_owned(),
                        formula: None,
                    }),
                    categories: Some(DataRange {
                        formula: None,
                        point_count: 2,
                        points: vec![
                            (0, ChartValue::Text("1st Qtr".to_owned())),
                            (1, ChartValue::Text("2nd Qtr".to_owned())),
                        ],
                        number_format: None,
                    }),
                    values: DataRange {
                        formula: None,
                        point_count: 2,
                        points: vec![
                            (0, ChartValue::Number("4.3".to_owned())),
                            (1, ChartValue::Number("2.5".to_owned())),
                        ],
                        number_format: None,
                    },
                    ..Series::default()
                }],
                axis_ids: vec![1, 2],
                vary_colors: false,
            }],
            axes: vec![
                Axis {
                    id: 1,
                    kind: AxisKind::Category,
                    position: Some(AxisPosition::Bottom),
                    cross_axis_id: Some(2),
                    ..Axis::default()
                },
                Axis {
                    id: 2,
                    kind: AxisKind::Value,
                    position: Some(AxisPosition::Left),
                    cross_axis_id: Some(1),
                    ..Axis::default()
                },
            ],
        },
        legend: Some(Legend {
            font: None,
            retained: Default::default(),
            position: LegendPosition::Bottom,
            overlay: false,
        }),
        plot_visible_only: true,
        display_blanks_as: DisplayBlanks::Gap,
        vary_colors: false,
        external_data: None,
        dirty: false,
    }
}

/// A section that references `header`, so the header part is written at all (an
/// unreferenced running body is dropped, `docx.export.header.unreferenced_dropped`).
fn section(header: Option<HeaderFooterId>, footer: Option<HeaderFooterId>) -> SectionBoundary {
    SectionBoundary {
        id: SectionId::new(id(90)),
        page_size: PageSize {
            width_twips: 12240,
            height_twips: 15840,
        },
        page_margins: PageMargins {
            top_twips: 1440,
            bottom_twips: 1440,
            start_twips: 1440,
            end_twips: 1440,
            header_twips: Some(720),
            footer_twips: Some(720),
            gutter_twips: None,
        },
        columns: SectionColumns {
            count: 1,
            space_twips: None,
            separator: None,
            equal_width: None,
            columns: Vec::new(),
        },
        headers: header
            .map(|reference| {
                vec![HeaderFooterRef {
                    kind: HeaderFooterKind::Default,
                    reference,
                }]
            })
            .unwrap_or_default(),
        footers: footer
            .map(|reference| {
                vec![HeaderFooterRef {
                    kind: HeaderFooterKind::Default,
                    reference,
                }]
            })
            .unwrap_or_default(),
        section_type: None,
        title_page: None,
        vertical_alignment: None,
        page_numbering: Default::default(),
        doc_grid: Default::default(),
        orientation: None,
        paper_source: Default::default(),
        page_borders: Default::default(),
        line_numbering: Default::default(),
        watermark: None,
        footnote_props: Default::default(),
        endnote_props: Default::default(),
        text_direction: None,
        bidi: false,
        section_change: None,
    }
}

/// One empty body paragraph: the body is not the surface under test here, and a
/// chart left in it would make every assertion pass through `document.xml`.
fn empty_body() -> Vec<BlockNode> {
    vec![BlockNode::Paragraph(Paragraph {
        id: id(2),
        properties: ParagraphProperties::default().into(),
        inlines: Vec::new(),
    })]
}

/// Where a chart can live, and which package part then has to declare its
/// relationship.
struct Surface {
    /// What the failure message calls it.
    name: &'static str,
    /// The part that carries the `c:chart r:id`.
    part: &'static str,
    /// Places the chart paragraph on this surface and returns the document.
    place: fn(Chart, Vec<InlineNode>) -> Document,
}

/// Every block container the model admits a chart in, which is every container
/// `Document::visit_chart_object_ids` walks. Enumerated, not sampled: a surface
/// missing from here is a surface nothing checks.
const SURFACES: [Surface; 5] = [
    Surface {
        name: "a header",
        part: "word/header1.xml",
        place: place_in_header,
    },
    Surface {
        name: "a footer",
        part: "word/footer1.xml",
        place: place_in_footer,
    },
    Surface {
        name: "a footnote",
        part: "word/footnotes.xml",
        place: place_in_footnote,
    },
    Surface {
        name: "an endnote",
        part: "word/endnotes.xml",
        place: place_in_endnote,
    },
    Surface {
        name: "a comment",
        part: "word/comments.xml",
        place: place_in_comment,
    },
];

fn document_from(definitions: Definitions) -> Document {
    Document::new(id(1), empty_body(), definitions)
        .expect("a chart object plus its projection is a valid document")
}

fn with_projection(projection: Chart) -> Definitions {
    let mut definitions = Definitions::default();
    definitions.charts.insert(ChartId::new(id(4)), projection);
    definitions
}

fn place_in_header(projection: Chart, extra: Vec<InlineNode>) -> Document {
    let header = HeaderFooterId::new(id(100));
    let mut definitions = with_projection(projection);
    definitions.headers.insert(
        header,
        HeaderFooter {
            blocks: vec![chart_paragraph(200, extra)],
        },
    );
    definitions.sections = vec![section(Some(header), None)];
    document_from(definitions)
}

fn place_in_footer(projection: Chart, extra: Vec<InlineNode>) -> Document {
    let footer = HeaderFooterId::new(id(101));
    let mut definitions = with_projection(projection);
    definitions.footers.insert(
        footer,
        HeaderFooter {
            blocks: vec![chart_paragraph(210, extra)],
        },
    );
    definitions.sections = vec![section(None, Some(footer))];
    document_from(definitions)
}

fn place_in_footnote(projection: Chart, extra: Vec<InlineNode>) -> Document {
    let mut definitions = with_projection(projection);
    definitions.footnotes.insert(
        NoteId::new(id(102)),
        Note {
            blocks: vec![chart_paragraph(220, extra)],
        },
    );
    document_from(definitions)
}

fn place_in_endnote(projection: Chart, extra: Vec<InlineNode>) -> Document {
    let mut definitions = with_projection(projection);
    definitions.endnotes.insert(
        NoteId::new(id(103)),
        Note {
            blocks: vec![chart_paragraph(230, extra)],
        },
    );
    document_from(definitions)
}

fn place_in_comment(projection: Chart, extra: Vec<InlineNode>) -> Document {
    let mut definitions = with_projection(projection);
    definitions.comments.insert(
        CommentId::new(id(104)),
        Comment {
            blocks: vec![chart_paragraph(240, extra)],
            ..Comment::default()
        },
    );
    document_from(definitions)
}

/// Every `r:id` a part's `c:chart` elements carry, read out of the written XML.
///
/// Reads the FILE, not the model: the question is what a consumer sees, and a
/// helper that consulted the model could not tell the two apart.
fn chart_reference_ids(part: &[u8]) -> Vec<String> {
    use quick_xml::Reader;
    use quick_xml::events::Event;

    let mut reader = Reader::from_reader(part);
    let mut buffer = Vec::new();
    let mut ids = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Eof) => break,
            Ok(Event::Start(element) | Event::Empty(element)) => {
                if element.name().as_ref() == b"c:chart" {
                    for attribute in element.attributes().flatten() {
                        if attribute.key.as_ref() == b"r:id" {
                            ids.push(String::from_utf8_lossy(&attribute.value).into_owned());
                        }
                    }
                }
            }
            Ok(_) => {}
            Err(error) => panic!("the written part must parse: {error}"),
        }
        buffer.clear();
    }
    ids
}

/// What the generated part has to carry, by the element or value that carries
/// it: the row's own list, each one named so a failure says which went missing.
///
/// The body's guards compare the whole reconstructed projection
/// (`chart_part_writer.rs`), which is stronger. That is not available here: the
/// importer passes an empty embedded index to every running part, so a chart in
/// a header does not come back as a projection at all (reported, not fixed in
/// this crate). Until it does, these are the facts a reader needs and the part
/// can be read for directly.
const CHART_CONTENT: [(&str, &str); 5] = [
    ("chart family", "<c:barChart>"),
    ("series", "<c:ser>"),
    ("series name", "Series 1"),
    ("cached categories", "1st Qtr"),
    ("legend", "<c:legend>"),
];

/// Walks the four OPC links from `part`'s own reference to the chart part, the
/// way a consumer does, and panics naming the link that broke.
fn assert_chart_resolves_from(package: &mut DocxPackage, surface: &str, part: &str) {
    let bytes = package
        .read_part(part)
        .unwrap_or_else(|_| panic!("{surface}: {part} must be in the package"));

    // Link 1: the reference the part itself carries.
    let referenced = chart_reference_ids(&bytes);
    assert_eq!(
        referenced.len(),
        1,
        "link 1 ({surface}): {part} must carry exactly one `c:chart r:id`, got {referenced:?}"
    );
    let rel_id = &referenced[0];

    // Link 2: OPC resolution through the PART's own relationships, not the
    // document's — that distinction is the whole defect.
    let relationships = package
        .part_relationships(part)
        .unwrap_or_else(|error| panic!("link 2 ({surface}): {part}'s rels must resolve: {error}"));
    let resolved = relationships
        .iter()
        .find(|rel| &rel.id == rel_id)
        .unwrap_or_else(|| {
            panic!(
                "link 2 ({surface}): {part} references {rel_id}, which its own relationship part \
                 does not declare — an `r:id` resolving to nothing is what Word reports as \
                 unreadable content (109 HF-256). Declared: {:?}",
                relationships.iter().map(|rel| &rel.id).collect::<Vec<_>>()
            )
        });
    assert_eq!(
        resolved.relationship_type, CHART_REL_TYPE,
        "link 2 ({surface}): {rel_id} must be a chart relationship, not whatever else took the id"
    );
    let part_name = resolved.resolved_part.clone().unwrap_or_else(|| {
        panic!("link 2 ({surface}): the target must resolve inside the package")
    });
    assert_eq!(
        part_name, CHART_PART,
        "link 2 ({surface}): the relationship must resolve to the part name the model named, so \
         the part written and the part referenced cannot disagree"
    );

    // Link 3: the part is there, and is the CHART — not an empty chart space.
    //
    // `starts_with("<c:chartSpace")` alone would pass on a part holding nothing,
    // and "a part exists" is not the guarantee. The row names what has to
    // survive — type, series, categories, title and legend — so each of those is
    // looked for by the element or value that carries it. An empty shell that
    // validates is exactly the outcome a reference-only check cannot tell from a
    // working chart.
    let chart_part = package
        .read_part(&part_name)
        .unwrap_or_else(|_| panic!("link 3 ({surface}): {part_name} must be in the package"));
    let chart_text = String::from_utf8_lossy(&chart_part);
    assert!(
        chart_text.starts_with("<c:chartSpace"),
        "link 3 ({surface}): the resolved part must be a chart space"
    );
    for (what, needle) in CHART_CONTENT {
        assert!(
            chart_text.contains(needle),
            "link 3 ({surface}): the written chart part carries no {what} — `{needle}` is \
             absent, so the part validates while the chart is empty. Part:\n{chart_text}"
        );
    }

    // Link 4: the content type, through the package's own override handling.
    assert_eq!(
        package.content_type(&part_name),
        Some(CHART_CT),
        "link 4 ({surface}): without the Override the part falls through the `.xml` Default to \
         application/xml and Word refuses the package"
    );
}

/// A chart the editor minted on each non-body surface saves as a chart.
///
/// This is HF-256's own claim — a part, its content type and a relationship that
/// resolves — asserted on the five surfaces the body-only walk never visited.
#[test]
fn a_chart_on_every_block_container_resolves_to_the_part_the_export_generated() {
    for surface in &SURFACES {
        let document = (surface.place)(
            projection(chart_anchor(), ChartCoverage::Complete),
            Vec::new(),
        );
        let written = write_document(&document, &BTreeMap::new())
            .unwrap_or_else(|error| panic!("{}: the document writes: {error}", surface.name));
        let mut package = DocxPackage::open(&written, PackageLimits::default())
            .unwrap_or_else(|error| panic!("{}: the package opens: {error}", surface.name));
        assert_chart_resolves_from(&mut package, surface.name, surface.part);
    }
}

/// A chart whose part the package will NOT contain leaves no reference behind on
/// a running surface either.
///
/// The twin of the body guard. `Partial` coverage refuses regeneration, nothing
/// is retained, so the part cannot be written — and the surface must then drop
/// the object rather than name a part that is not there. A valid package that
/// reports the loss beats a corrupt one that hides it.
#[test]
fn a_chart_with_no_part_leaves_no_reference_on_any_block_container() {
    for surface in &SURFACES {
        let document = (surface.place)(
            projection(chart_anchor(), ChartCoverage::Partial),
            Vec::new(),
        );
        let export = export_document(&document, &BTreeMap::new())
            .unwrap_or_else(|error| panic!("{}: the document writes: {error}", surface.name));
        let mut package = DocxPackage::open(&export.bytes, PackageLimits::default())
            .unwrap_or_else(|error| panic!("{}: the package opens: {error}", surface.name));
        let bytes = package
            .read_part(surface.part)
            .unwrap_or_else(|_| panic!("{}: {} must be written", surface.name, surface.part));
        assert!(
            chart_reference_ids(&bytes).is_empty(),
            "{}: {} must not reference a chart part the package does not contain — that \
             dangling `r:id` is HF-256 itself",
            surface.name,
            surface.part
        );
        let declared = package
            .part_relationships(surface.part)
            .map(|rels| {
                rels.iter()
                    .any(|rel| rel.relationship_type == CHART_REL_TYPE)
            })
            .unwrap_or(false);
        assert!(
            !declared,
            "{}: {} must not declare a chart relationship whose part is absent",
            surface.name, surface.part
        );
        let reported: Vec<&str> = export
            .report
            .entries
            .iter()
            .map(|entry| entry.feature.as_str())
            .collect();
        assert!(
            reported.contains(&"docx.export.chart.partial_coverage_not_regenerated"),
            "{}: the refusal to regenerate must be REPORTED, not silent; got {reported:?}",
            surface.name
        );
    }
}

/// A retained chart part is still copied verbatim when the chart lives on a
/// running surface, and the surface declares the relationship for it.
///
/// Import keeps a chart's source bytes, and retention must win over regeneration
/// on every surface — otherwise the fix above would quietly rewrite every
/// imported header chart as a lossy regeneration of itself.
#[test]
fn a_retained_chart_part_on_a_running_surface_is_copied_and_still_referenced() {
    let verbatim = br#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:layout/></c:plotArea></c:chart><!-- retained --></c:chartSpace>"#;
    let retained = RetainedParts {
        // A chart's part, not a theme: no verbatim theme is carried.
        theme: None,
        parts: vec![RetainedPart {
            part_name: CHART_PART.to_owned(),
            content_type: Some(CHART_CT.to_owned()),
            bytes: verbatim.to_vec(),
            rels: None,
        }],
        ..RetainedParts::default()
    };
    for surface in &SURFACES {
        let document = (surface.place)(
            projection(chart_anchor(), ChartCoverage::Complete),
            Vec::new(),
        );
        let written = write_document_with_retained_parts(&document, &BTreeMap::new(), &retained)
            .unwrap_or_else(|error| panic!("{}: the document writes: {error}", surface.name));
        let mut package = DocxPackage::open(&written, PackageLimits::default())
            .unwrap_or_else(|error| panic!("{}: the package opens: {error}", surface.name));
        assert_eq!(
            package.read_part(CHART_PART).expect("the chart part reads"),
            verbatim.to_vec(),
            "{}: a retained chart part must come back byte-identical, not regenerated",
            surface.name
        );
        // And the reference still resolves: retention supplies the bytes, the
        // surface still owes the relationship.
        let bytes = package.read_part(surface.part).expect("the surface reads");
        assert_eq!(
            chart_reference_ids(&bytes).len(),
            1,
            "{}: a retained chart is still referenced from its surface",
            surface.name
        );
        assert!(
            package
                .part_relationships(surface.part)
                .expect("the surface's rels resolve")
                .iter()
                .any(|rel| rel.relationship_type == CHART_REL_TYPE),
            "{}: the surface must declare the relationship for a retained chart too",
            surface.name
        );
    }
}

/// A chart's verbatim relationship id is reserved against hyperlink minting on a
/// running surface, exactly as it is in the body.
///
/// A header's `_rels` mints ids for the hyperlinks inside it starting at `rId1`.
/// The chart's id is written VERBATIM from the model, so unless it is reserved
/// the hyperlink takes it and the chart's `r:id` resolves to the hyperlink — the
/// same collision that once made a header's `r:embed` resolve to a hyperlink.
/// The chart fixture here holds `rId1` on purpose so the collision is real.
#[test]
fn a_running_part_does_not_hand_a_charts_id_to_a_hyperlink() {
    let link = InlineNode::Hyperlink(Box::new(Hyperlink {
        id: id(300),
        target: HyperlinkTarget::External(ExternalTarget {
            url: "https://example.invalid/".to_owned(),
            anchor: None,
        }),
        tooltip: None,
        // A hyperlink with no content is not a valid model (`EmptyHyperlink`),
        // and the point of this fixture is a REAL hyperlink competing for the id.
        inlines: vec![InlineNode::Run(Run {
            id: id(301),
            properties: RunProperties::default().into(),
            text: "link".to_owned(),
        })],
    }));
    for surface in &SURFACES {
        let document = (surface.place)(
            projection(chart_anchor(), ChartCoverage::Complete),
            vec![link.clone()],
        );
        let written = write_document(&document, &BTreeMap::new())
            .unwrap_or_else(|error| panic!("{}: the document writes: {error}", surface.name));
        let mut package = DocxPackage::open(&written, PackageLimits::default())
            .unwrap_or_else(|error| panic!("{}: the package opens: {error}", surface.name));
        let relationships = package
            .part_relationships(surface.part)
            .expect("the surface's rels resolve");
        // The duplicate-id check comes FIRST, because it is the failure a
        // find-the-first-match assertion cannot see: with the chart declared
        // before the hyperlink, an unreserved id puts rId1 in the part TWICE and
        // every reader that takes the first match looks satisfied. Two
        // relationships with one Id is not a valid OPC part.
        let mut ids: Vec<&String> = relationships.iter().map(|rel| &rel.id).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(
            ids.len(),
            total,
            "{}: {} declares a relationship id twice — the chart writes its id \
             verbatim and the hyperlink minted the same one. Declared: {:?}",
            surface.name,
            surface.part,
            relationships
                .iter()
                .map(|rel| (&rel.id, &rel.relationship_type))
                .collect::<Vec<_>>()
        );
        let holder = relationships
            .iter()
            .find(|rel| rel.id == CHART_REL_ID)
            .unwrap_or_else(|| panic!("{}: {CHART_REL_ID} must be declared", surface.name));
        assert_eq!(
            holder.relationship_type, CHART_REL_TYPE,
            "{}: {CHART_REL_ID} is the chart's verbatim id; a hyperlink minted into it makes the \
             chart's `r:id` resolve to the hyperlink",
            surface.name
        );
        // And the hyperlink is still there, under some other id — reserving must
        // not drop it.
        assert!(
            relationships
                .iter()
                .any(|rel| rel.relationship_type.ends_with("/hyperlink")),
            "{}: the hyperlink must still be declared, under a different id",
            surface.name
        );
        assert_chart_resolves_from(&mut package, surface.name, surface.part);
    }
}

/// A chart inside a GROUPED text box declares its relationship (`109` HF-196).
///
/// `write_group_text_box` writes the blocks of a grouped shape's text box, so a
/// chart in one is written with its `c:chart r:id`. The relationship walk did not
/// descend into a group — `collect_group_media` always has — which is what HF-196
/// reported as a dangling relationship and Word offering to repair the file.
///
/// Measured before this change, the symptom had already MOVED: the availability
/// gate added with HF-256's first half meant the undeclared part was not
/// available, so the object was dropped from the body instead of dangling. The
/// pre-fix failure of this guard was `link 1: word/document.xml must carry
/// exactly one c:chart r:id, got []` — a silent loss rather than a corrupt file.
/// Both are data loss; only writing the part and declaring it is a chart.
#[test]
fn a_chart_inside_a_grouped_text_box_declares_its_relationship() {
    let extent = Extent {
        width_emu: 4_572_000,
        height_emu: 2_743_200,
    };
    let group = WordprocessingGroup {
        id: id(400),
        anchor: None,
        relative_height: None,
        extent,
        transform: GroupTransform {
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            child_offset: PointEmu { x_emu: 0, y_emu: 0 },
            child_extent: extent,
            rotation: None,
            flip_h: false,
            flip_v: false,
        },
        hyperlink: None,
        children: vec![GroupChild::TextBox(GroupTextBox {
            id: id(401),
            offset: PointEmu { x_emu: 0, y_emu: 0 },
            extent,
            rotation: None,
            flip_h: false,
            flip_v: false,
            geometry: ShapeGeometry::Rectangle,
            preset: None,
            adjustments: Vec::new(),
            fill: None,
            border: None,
            body_properties: Default::default(),
            hyperlink: None,
            blocks: vec![chart_paragraph(410, Vec::new())],
        })],
    };
    let mut definitions = with_projection(projection(chart_anchor(), ChartCoverage::Complete));
    definitions.sections = vec![section(None, None)];
    let document = Document::new(
        id(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: id(2),
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Group(Box::new(group))],
        })],
        definitions,
    )
    .expect("a grouped text box holding a chart is a valid document");
    let written = write_document(&document, &BTreeMap::new()).expect("the document writes");
    let mut package =
        DocxPackage::open(&written, PackageLimits::default()).expect("the package opens");
    assert_chart_resolves_from(&mut package, "a grouped text box", "word/document.xml");
}
