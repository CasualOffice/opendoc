//! Editing a field's cached result, driven against the repository's real
//! `sample.docx` rather than a constructed fixture.
//!
//! The defect these guard was reported from the running editor: editing the
//! **total page count** in a footer answered "That edit isn't supported for this
//! selection yet" — the host's one generic sentence for any refusal the engine
//! did not explain. `sample.docx`'s three footers each carry a `PAGE` and a
//! `NUMPAGES` field in the four-part complex spelling (`fldChar begin` /
//! `instrText` / `separate` / `end`, all inside a single `w:r`), which import
//! reads as an inline [`InlineNode::Field`] — not a `FieldRangeStart`/`End` pair,
//! because both markers are in the same paragraph.
//!
//! Three gestures on the same two bytes behaved three different ways before this:
//!
//! | gesture | before | after |
//! | --- | --- | --- |
//! | Backspace over the last digit | `Unsupported` → generic sentence | refused, and says the count is calculated |
//! | type with the caret between the digits | **appended at the paragraph end** | refused, same sentence |
//! | select both digits and type | field replaced with the typed text | unchanged |
//!
//! The middle row is the one that mattered most: it reported success and put the
//! character somewhere the reader did not ask for.
//!
//! The fixture is loaded from the committed copy at the repository root, never
//! `webapp/sample.docx` — that one is staged by `webapp/build.sh` and is not in
//! the repository, so a test reading it passes only on a machine that has built
//! the webapp.

use std::str::FromStr as _;

use casual_doc_edit::{EditError, FieldRefusal, Operation, Pos, Range, apply, find_paragraph_any};
use casual_doc_import::ImportConfig;
use casual_doc_model::v1::{BlockNode, Document, Field, FieldKind, InlineNode};
use casual_doc_model::{IdGenerator, NodeId};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const SAMPLE: &[u8] = include_bytes!("../../../sample.docx");

fn sample() -> Document {
    let mut package = DocxPackage::open(SAMPLE, PackageLimits::default()).expect("open package");
    casual_doc_import::import_package(&mut package, ImportConfig::default())
        .expect("import sample.docx")
        .document
}

/// The projected text of an inline list, descending into the wrappers that
/// contribute text — enough for `sample.docx`'s footers (a run, a field, a run, a
/// field) and its TOC paragraph (one field). Deliberately local to this test: it
/// asserts what the READER sees, and must not be able to agree with the engine by
/// sharing the engine's own accounting.
fn projected(inlines: &[InlineNode]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => out.push_str(&run.text),
            InlineNode::Tab(_) => out.push('\t'),
            InlineNode::Field(field) => out.push_str(&projected(&field.inlines)),
            InlineNode::Hyperlink(link) => out.push_str(&projected(&link.inlines)),
            _ => {}
        }
    }
    out
}

fn paragraph_text(document: &Document, node: NodeId) -> String {
    projected(
        &find_paragraph_any(document, node)
            .expect("paragraph is in the document")
            .inlines,
    )
}

/// The paragraph on any surface whose projected text is `wanted`, plus its length.
fn paragraph_with_text(document: &Document, wanted: &str) -> (NodeId, u32) {
    let node = all_paragraph_ids(document)
        .into_iter()
        .find(|id| paragraph_text(document, *id) == wanted)
        .unwrap_or_else(|| panic!("no paragraph reads {wanted:?}"));
    let len = paragraph_text(document, node).len() as u32;
    (node, len)
}

/// The instruction of every field directly in paragraph `node`.
fn field_instructions(document: &Document, node: NodeId) -> Vec<String> {
    find_paragraph_any(document, node)
        .expect("paragraph")
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Field(field) => Some(field.instruction.clone()),
            _ => None,
        })
        .collect()
}

/// Every paragraph id on every surface this fixture has — body, headers, footers.
fn all_paragraph_ids(document: &Document) -> Vec<NodeId> {
    fn walk(blocks: &[BlockNode], out: &mut Vec<NodeId>) {
        for block in blocks {
            match block {
                BlockNode::Paragraph(paragraph) => out.push(paragraph.id),
                BlockNode::Table(table) => {
                    for cell in table.rows.iter().flat_map(|row| &row.cells) {
                        walk(&cell.blocks, out);
                    }
                }
                BlockNode::Sdt(sdt) => walk(&sdt.blocks, out),
                BlockNode::AltChunk(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(document.body(), &mut out);
    for (_, header) in document.definitions().headers.iter() {
        walk(&header.blocks, &mut out);
    }
    for (_, footer) in document.definitions().footers.iter() {
        walk(&footer.blocks, &mut out);
    }
    out
}

fn ids() -> IdGenerator {
    // A namespace no node in `sample.docx` occupies, so a minted run id cannot
    // collide with an imported one.
    IdGenerator::new(0x5150)
}

/// The footer paragraph carrying `PAGE of NUMPAGES`, and the byte range its
/// `NUMPAGES` result occupies.
fn footer_page_count(document: &Document) -> (NodeId, u32, u32, u32) {
    let (node, len) = paragraph_with_text(document, "OpenDoc by CasualOffice   •   1 of 14");
    assert_eq!(
        field_instructions(document, node),
        vec![" PAGE ".to_owned(), " NUMPAGES ".to_owned()],
        "the fixture's footer really carries the two complex fields this is about",
    );
    let paragraph = find_paragraph_any(document, node).expect("paragraph");
    let mut cum = 0u32;
    let mut result = None;
    for inline in &paragraph.inlines {
        let start = cum;
        let width = projected(std::slice::from_ref(inline)).len() as u32;
        cum = start + width;
        if let InlineNode::Field(field) = inline
            && field.instruction.contains("NUMPAGES")
        {
            result = Some((start, cum));
        }
    }
    let (start, end) = result.expect("the NUMPAGES field is in the footer");
    assert_eq!(
        (end - start),
        2,
        "the cached result is the two bytes \"14\", which is what makes a PARTIAL \
         edit inside it expressible at all",
    );
    (node, len, start, end)
}

fn refusal(error: EditError) -> FieldRefusal {
    match error {
        EditError::FieldResult(refusal) => refusal,
        other => panic!("expected a field-result refusal, got {other:?}"),
    }
}

#[test]
fn a_backspace_inside_the_footers_page_count_says_the_count_is_calculated() {
    let mut document = sample();
    let (node, len, start, end) = footer_page_count(&document);
    assert_eq!(len, end, "the NUMPAGES result ends the footer paragraph");

    // Backspace with the caret at the end of the footer: delete the last byte of
    // "14". This is the reported gesture.
    let error = apply(
        &mut document,
        &mut ids(),
        &Operation::DeleteText {
            range: Range {
                start: Pos::new(node, end - 1),
                end: Pos::new(node, end),
            },
        },
    )
    .expect_err("a partial edit of a calculated result is refused");
    assert_eq!(refusal(error), FieldRefusal::PageCountIsCalculated);

    // The sentence the reader is shown — not an engine error name, and not the
    // host's generic "not supported for this selection", which would send them
    // hunting a selection that will never work.
    let sentence = error
        .reason()
        .expect("the refusal carries its own sentence");
    assert!(
        sentence.starts_with("refused: "),
        "the host passes an already-explained refusal through verbatim on this \
         marker (webapp/src/edit_errors.mjs): {sentence}",
    );
    assert!(
        sentence.contains("total page count is calculated"),
        "the sentence names the calculated value: {sentence}",
    );
    assert!(
        sentence.contains("select the whole number"),
        "and names the gesture that DOES work: {sentence}",
    );

    // Refused means refused: nothing moved.
    assert_eq!(
        paragraph_text(&document, node),
        "OpenDoc by CasualOffice   •   1 of 14",
    );
    assert_eq!(
        field_instructions(&document, node),
        vec![" PAGE ".to_owned(), " NUMPAGES ".to_owned()],
        "and the field is still a field",
    );
    let _ = start;
}

#[test]
fn typing_between_the_page_counts_digits_is_refused_not_moved_to_the_paragraph_end() {
    let mut document = sample();
    let (node, _, start, end) = footer_page_count(&document);
    let inside = start + 1;
    assert!(inside > start && inside < end, "strictly inside the result");

    let error = apply(
        &mut document,
        &mut ids(),
        &Operation::InsertText {
            at: Pos::new(node, inside),
            text: "9".to_owned(),
        },
    )
    .expect_err("typing inside a calculated result is refused");
    assert_eq!(refusal(error), FieldRefusal::PageCountIsCalculated);

    // The guarantee, and the reason this test exists: the character is NOT
    // silently relocated. Before this, `insert_text` fell through to its
    // "insert a fresh run at the matching top-level position" fallback, whose
    // loop cannot match an offset interior to an inline, so the footer read
    // "…1 of 149" — the typed character after the field, and a reported success.
    assert_eq!(
        paragraph_text(&document, node),
        "OpenDoc by CasualOffice   •   1 of 14",
        "no character landed anywhere, least of all at the paragraph end",
    );
}

#[test]
fn selecting_the_whole_page_count_and_typing_replaces_the_field_with_fixed_text() {
    // The gesture the refusal above tells the reader to use, and Word's and
    // Docs' answer for "I want a fixed number here". It must actually work, or
    // the sentence is advice the product does not honour.
    let mut document = sample();
    let (node, _, start, end) = footer_page_count(&document);

    apply(
        &mut document,
        &mut ids(),
        &Operation::DeleteText {
            range: Range {
                start: Pos::new(node, start),
                end: Pos::new(node, end),
            },
        },
    )
    .expect("a range covering the whole field replaces it");
    apply(
        &mut document,
        &mut ids(),
        &Operation::InsertText {
            at: Pos::new(node, start),
            text: "20".to_owned(),
        },
    )
    .expect("and the fixed text goes in");

    assert_eq!(
        paragraph_text(&document, node),
        "OpenDoc by CasualOffice   •   1 of 20",
        "the footer's page-count text reads what the user typed",
    );
    assert_eq!(
        field_instructions(&document, node),
        vec![" PAGE ".to_owned()],
        "the NUMPAGES field is gone — that is what \"fixed text\" means — while \
         the PAGE field beside it is untouched",
    );
}

#[test]
fn a_toc_fields_result_is_edited_in_place_rather_than_at_the_paragraph_end() {
    // The other half of the class. Every field that is NOT `PAGE`/`NUMPAGES`
    // has a cached result that IS the text the page shows (`casual-doc-layout`'s
    // `flow.rs` flows it as ordinary inline content), so editing it is
    // meaningful — and is what Word does, which is why "edit your table of
    // contents and lose it on the next update" is a documented Word gotcha
    // rather than a refusal.
    //
    // `sample.docx`'s TOC paragraph is ONE `TOC` field whose whole cached result
    // is a single run, so every interior offset in that paragraph is interior to
    // a field: before this change, typing at offset 6 appended the character at
    // the paragraph END instead.
    let mut document = sample();
    let (node, _) = paragraph_with_text(
        &document,
        "Update this field in Word to generate the table of contents.",
    );
    assert_eq!(
        field_instructions(&document, node).len(),
        1,
        "the whole paragraph is one field's cached result",
    );

    apply(
        &mut document,
        &mut ids(),
        &Operation::InsertText {
            at: Pos::new(node, 6),
            text: "d".to_owned(),
        },
    )
    .expect("a passthrough field's result takes an edit");
    assert_eq!(
        paragraph_text(&document, node),
        "Updated this field in Word to generate the table of contents.",
        "the character landed AT offset 6, not appended after the field",
    );
    assert_eq!(
        field_instructions(&document, node).len(),
        1,
        "and it landed inside the field, which is still a field",
    );

    apply(
        &mut document,
        &mut ids(),
        &Operation::DeleteText {
            range: Range {
                start: Pos::new(node, 0),
                end: Pos::new(node, 8),
            },
        },
    )
    .expect("and a partial delete inside it");
    assert_eq!(
        paragraph_text(&document, node),
        "this field in Word to generate the table of contents.",
    );
}

#[test]
fn a_paragraph_break_inside_a_field_result_is_refused_with_the_reason() {
    // Still refused — a complex field is one `fldChar begin … end` span and
    // cannot straddle two `w:p` — but it no longer arrives as the generic
    // sentence. This is the one place the refusal is about the GESTURE rather
    // than about the field's value, so it gets its own words.
    let mut document = sample();
    let (node, _) = paragraph_with_text(
        &document,
        "Update this field in Word to generate the table of contents.",
    );
    let error = apply(
        &mut document,
        &mut ids(),
        &Operation::SplitParagraph {
            at: Pos::new(node, 6),
            new_id: NodeId::from_str("00000000000051500000000000000001").unwrap(),
            properties: None,
        },
    )
    .expect_err("a field's result cannot be split");
    assert_eq!(refusal(error), FieldRefusal::FieldCannotBeSplit);
    let sentence = error.reason().expect("explained");
    assert!(
        sentence.contains("cannot be split across two paragraphs"),
        "{sentence}",
    );
    assert_eq!(
        paragraph_text(&document, node),
        "Update this field in Word to generate the table of contents.",
        "and the document is unchanged",
    );
}

/// A `Field` is what `sample.docx`'s footer imports as — asserted here because
/// the whole analysis depends on it, and #637 added a second representation
/// (`FieldRangeStart`/`FieldRangeEnd` markers with the content flat between
/// them) that a field outliving its paragraph is promoted to. A run between two
/// markers is TOP-LEVEL, so every conclusion above would be wrong for it.
#[test]
fn the_footers_page_fields_import_as_inline_fields_not_as_a_range_pair() {
    let document = sample();
    assert!(
        document.definitions().field_ranges.is_empty(),
        "nothing in the fixture is promoted to a field range",
    );
    let (node, ..) = footer_page_count(&document);
    let paragraph = find_paragraph_any(&document, node).expect("paragraph");
    let kinds: Vec<&str> = paragraph
        .inlines
        .iter()
        .map(|inline| match inline {
            InlineNode::Run(_) => "run",
            InlineNode::Field(_) => "field",
            InlineNode::FieldRangeStart(_) => "range-start",
            InlineNode::FieldRangeEnd(_) => "range-end",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["run", "field", "run", "field"]);

    // And each field's result is a run nested INSIDE it, which is why no
    // top-level run segment ever covered those bytes.
    let nested: Vec<usize> = paragraph
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Field(field) => Some(field.inlines.len()),
            _ => None,
        })
        .collect();
    assert_eq!(nested, vec![1, 1]);

    // Neither field is locked or flagged stale, so `w:dirty` is not what is
    // stopping the edit: the restamp is unconditional. `docs/128`.
    let dirty: Vec<bool> = paragraph
        .inlines
        .iter()
        .filter_map(|inline| match inline {
            InlineNode::Field(field) => Some(field.update.dirty || field.update.locked),
            _ => None,
        })
        .collect();
    assert_eq!(dirty, vec![false, false]);
}

/// Every non-`PAGE`/`NUMPAGES` field's result is editable and those two are not —
/// asserted through the public refusal rather than the private predicate, so the
/// guard cannot pass by agreeing with an implementation detail.
#[test]
fn only_the_pagination_dependent_fields_refuse_an_interior_edit() {
    for (instruction, expected) in [
        (" PAGE ", Some(FieldRefusal::PageNumberIsCalculated)),
        (" NUMPAGES ", Some(FieldRefusal::PageCountIsCalculated)),
        (
            "PAGE \\* MERGEFORMAT",
            Some(FieldRefusal::PageNumberIsCalculated),
        ),
        (" TOC \\o \"1-3\" ", None),
        (" SEQ Figure ", None),
        (" REF _Ref1 ", None),
        (" STYLEREF 1 \\s ", None),
        (" PAGEREF _Ref1 ", None),
    ] {
        let mut document = one_field_document(instruction, "AB");
        let node = all_paragraph_ids(&document)[0];
        let outcome = apply(
            &mut document,
            &mut ids(),
            &Operation::InsertText {
                at: Pos::new(node, 1),
                text: "x".to_owned(),
            },
        );
        match expected {
            Some(wanted) => {
                let error = outcome
                    .err()
                    .unwrap_or_else(|| panic!("{instruction:?} must refuse an interior edit"));
                assert_eq!(refusal(error), wanted, "for {instruction:?}");
                assert_eq!(paragraph_text(&document, node), "AB", "for {instruction:?}");
            }
            None => {
                outcome.unwrap_or_else(|error| {
                    panic!("{instruction:?} must accept an interior edit: {error:?}")
                });
                assert_eq!(
                    paragraph_text(&document, node),
                    "AxB",
                    "for {instruction:?}"
                );
            }
        }
    }
}

/// One paragraph holding one field whose cached result is `result`.
fn one_field_document(instruction: &str, result: &str) -> Document {
    use casual_doc_model::v1::{Definitions, Paragraph, ParagraphProperties, Run, RunProperties};
    let id = |counter: u64| NodeId::from_parts(9, counter).unwrap();
    let field = Field {
        id: id(3),
        instruction: instruction.to_owned(),
        // Deliberately the real projection, not a default: the engine reads the
        // INSTRUCTION (`kind` is documented as best-effort and defaults to
        // `Other` on legacy payloads), and a fixture that defaulted it would
        // leave that unproven.
        kind: FieldKind::parse(instruction),
        inlines: vec![InlineNode::Run(Run {
            id: id(4),
            properties: RunProperties::default().into(),
            text: result.to_owned(),
        })],
        form: None,
        update: casual_doc_model::v1::FieldUpdateState::default(),
    };
    Document::new(
        id(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: id(2),
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Field(Box::new(field))],
        })],
        Definitions::default(),
    )
    .expect("valid document")
}
