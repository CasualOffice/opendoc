//! Saving a document keeps its emoji, scalar for scalar, in every container.
//!
//! "On download the doc" was the second half of the report, and it is a separate
//! risk from painting: an emoji is a surrogate pair in UTF-16 and several
//! scalars in one user-perceived character, so a `w:t` whose escaping or
//! whitespace handling is slightly off turns it into a question mark, or drops
//! the joiner and leaves a woman and a laptop where a person-with-laptop was.
//! Neither shows up until someone reopens the file somewhere else.
//!
//! Before this there were **zero** tests in `casual-doc-export` or
//! `casual-doc-io` that mentioned emoji at all.
//!
//! The assertion is on the reopened MODEL's code points, not on the written XML:
//! a writer that escapes correctly and an importer that mangles on the way back
//! would pass an XML assertion and still lose the user's document. The input is
//! the committed `emoji-containers.docx`, so the body paragraph, the table cell,
//! the running header and the footnote body are all covered by construction —
//! the same fixture the rendering guard uses.

use std::collections::BTreeMap;

use casual_doc_export::export_document;
use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_model::v1::{BlockNode, Document, InlineNode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const EMOJI_CONTAINERS_DOCX: &[u8] =
    include_bytes!("../../../fixtures/generated/emoji-containers.docx");

/// The five hard shapes the fixture carries, spelled out: a lone scalar, a ZWJ
/// sequence, a scalar plus a skin-tone modifier, an emoji-presentation sequence,
/// and a regional-indicator pair. Between them they exercise every way an emoji
/// is more than one code point.
const EMOJI: &str =
    "\u{1f600}\u{1f469}\u{200d}\u{1f4bb}\u{1f44d}\u{1f3fd}\u{2764}\u{fe0f}\u{1f1ec}\u{1f1e7}";

fn open(bytes: &[u8]) -> Document {
    let mut package =
        DocxPackage::open(bytes, PackageLimits::default()).expect("the package opens");
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the package imports")
    .document
}

fn collect(blocks: &[BlockNode], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                for inline in &paragraph.inlines {
                    if let InlineNode::Run(run) = inline {
                        out.push(run.text.clone());
                    }
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        collect(&cell.blocks, out);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Every run's text, from the body, its tables, the running parts and the
/// footnote bodies — the containers the fixture uses, each labelled in its own
/// text so a failure says which one lost the emoji.
fn all_text(document: &Document) -> Vec<String> {
    let mut out = Vec::new();
    collect(document.body(), &mut out);
    let definitions = document.definitions();
    for (_, header) in definitions.headers.iter() {
        collect(&header.blocks, &mut out);
    }
    for (_, footer) in definitions.footers.iter() {
        collect(&footer.blocks, &mut out);
    }
    for (_, note) in definitions.footnotes.iter() {
        collect(&note.blocks, &mut out);
    }
    out.sort();
    out
}

/// Export → reopen keeps every scalar of every emoji, in every container.
#[test]
fn a_saved_document_carries_the_exact_code_points_back() {
    let original = open(EMOJI_CONTAINERS_DOCX);
    let before = all_text(&original);
    assert!(
        before.iter().filter(|text| text.contains(EMOJI)).count() >= 4,
        "the fixture should carry the emoji in four containers before the save; got {before:?}",
    );

    let exported = export_document(&original, &BTreeMap::new()).expect("the document exports");
    let after = all_text(&open(&exported.bytes));

    assert_eq!(
        before, after,
        "the saved document does not read back with the same text",
    );
    for text in &after {
        if !text.contains('\u{1f600}') {
            continue;
        }
        for ch in EMOJI.chars() {
            assert!(
                text.contains(ch),
                // The joiner is named separately because it is the scalar most
                // likely to be lost and the least visible when it is: drop
                // U+200D and a person-with-laptop silently becomes two pictures.
                "U+{:04X} was lost on save (from {text:?})",
                ch as u32,
            );
        }
        assert!(
            text.contains(EMOJI),
            "the emoji survived as scalars but not in order: {text:?}",
        );
    }
}
