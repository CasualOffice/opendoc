// SPDX-License-Identifier: Apache-2.0

//! Legacy VML across a save (`docs/109` HF-390, HF-395; `docs/08` ADR-072).
//!
//! The importer normalises a `w:pict` into DrawingML model nodes and keeps the
//! source beside them. These tests hold the three promises that makes:
//!
//! 1. an unedited document saves byte-identical (the retention path);
//! 2. an edited document writes every UNTOUCHED pict back as the VML it was —
//!    including a text box whose text was edited, whose content is regenerated
//!    into the kept source — and reports nothing for it;
//! 3. a pict whose projection WAS edited is written as DrawingML and named in
//!    the export report, never silently.
//!
//! And the import report's half: drawn VML is "shown approximately" and
//! preserved, never "kept, not shown".
//!
//! Complexity: one or two imports and exports of one small fixture per test.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use casual_doc_export::{export_document_with_retained_parts, write_package};
use casual_doc_import::{
    Disposition, ImportConfig, ImportMode, ModelOutcome, RetentionOutcome, import_package,
};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{BlockNode, Document, HorizontalPosition, InlineNode, TextBox};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const VML_SHAPES_DOCX: &[u8] = include_bytes!("../../../fixtures/generated/vml-shapes.docx");

struct Imported {
    document: Document,
    media: BTreeMap<String, Vec<u8>>,
    retained: casual_doc_import::RetainedParts,
    report: casual_doc_import::CompatibilityReport,
}

fn import(bytes: &[u8]) -> Imported {
    let mut package = DocxPackage::open(bytes, PackageLimits::default()).expect("admitted");
    let import = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("imports");
    Imported {
        document: import.document,
        media: parts(bytes, "word/media/"),
        retained: import.retained_parts,
        report: import.report,
    }
}

fn parts(bytes: &[u8], prefix: &str) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes.to_vec())).expect("a zip");
    let names: Vec<String> = archive
        .file_names()
        .map(|name| name.expect("a valid ZIP entry name").into_owned())
        .filter(|name| name.starts_with(prefix))
        .collect();
    names
        .into_iter()
        .map(|name| {
            let mut buffer = Vec::new();
            archive
                .by_name(&name)
                .expect("listed")
                .read_to_end(&mut buffer)
                .expect("readable");
            (name, buffer)
        })
        .collect()
}

fn part_text(bytes: &[u8], name: &str) -> String {
    String::from_utf8(
        parts(bytes, name)
            .remove(name)
            .expect("the part is written"),
    )
    .expect("utf-8")
}

/// The lead node id of the kept pict whose source contains `marker`.
fn lead_of(document: &Document, marker: &str) -> NodeId {
    document
        .definitions()
        .legacy_vml
        .iter()
        .find(|(_, entry)| entry.source.iter().any(|piece| piece.contains(marker)))
        .map(|(lead, _)| *lead)
        .unwrap_or_else(|| panic!("no kept pict carries {marker}"))
}

/// Every inline node of the body, at any depth of paragraph nesting the
/// fixture uses (top-level paragraphs only).
fn body_inlines_mut(document: &mut Document) -> impl Iterator<Item = &mut InlineNode> {
    document
        .body_mut()
        .iter_mut()
        .flat_map(|block| match block {
            BlockNode::Paragraph(paragraph) => paragraph.inlines.iter_mut(),
            _ => [].iter_mut(),
        })
}

fn save(imported: &Imported) -> casual_doc_export::DocxExport {
    export_document_with_retained_parts(&imported.document, &imported.media, &imported.retained)
        .expect("writable")
}

/// 1. An unedited document saves byte-identical: every part of the source, VML
///    included, comes back unchanged.
#[test]
fn an_unedited_vml_document_saves_byte_identical() {
    let mut package = DocxPackage::open(VML_SHAPES_DOCX, PackageLimits::default()).unwrap();
    let import = import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Retention,
            ..ImportConfig::default()
        },
    )
    .expect("imports");
    let written = write_package(import.retained_source.as_ref().expect("retained")).unwrap();
    assert_eq!(parts(&written, ""), parts(VML_SHAPES_DOCX, ""));
}

/// 2. A semantic save of the unedited model writes every drawn pict back as its
///    VML source, word for word, and reports no regeneration.
#[test]
fn a_semantic_save_writes_every_untouched_pict_back_as_vml() {
    let imported = import(VML_SHAPES_DOCX);
    let saved = save(&imported);
    let document_xml = part_text(&saved.bytes, "word/document.xml");
    for id in [
        "vmlRect",
        "vmlRoundRect",
        "vmlOval",
        "vmlLine",
        "vmlTriangle",
        "vmlArrow",
        "vmlGroup",
        "vmlTextBox",
        "vmlPictureFill",
        "vmlPicture",
    ] {
        assert!(
            document_xml.contains(&format!("id=\"{id}\"")),
            "{id} is written back as VML"
        );
    }
    // Word for word: the arrow's own path, as authored.
    assert!(document_xml.contains(r#"path="m,25l140,25,140,,200,50,140,100,140,75,,75xe""#));
    assert!(part_text(&saved.bytes, "word/header1.xml").contains("id=\"vmlHeaderText\""));
    assert!(
        !saved
            .report
            .entries
            .iter()
            .any(|entry| entry.feature == "docx.export.vml.regenerated"),
        "nothing was regenerated"
    );
    // And the written package reads back to the same drawing.
    let again = import(&saved.bytes);
    assert_eq!(
        again.document.definitions().legacy_vml.len(),
        imported.document.definitions().legacy_vml.len()
    );
}

/// 2b. Typing in a VML text box keeps the box VML: the text-box content is a
///     hole in the kept source, filled from the model on save.
#[test]
fn editing_a_vml_text_boxs_text_keeps_it_vml_with_the_new_text() {
    let mut imported = import(VML_SHAPES_DOCX);
    let lead = lead_of(&imported.document, "id=\"vmlTextBox\"");
    let mut edited = false;
    for inline in body_inlines_mut(&mut imported.document) {
        if let InlineNode::TextBox(text_box) = inline
            && text_box.id == lead
        {
            set_first_run_text(text_box, "Edited inside the VML text box");
            edited = true;
        }
    }
    assert!(edited, "the fixture's text box is a top-level text box");
    let saved = save(&imported);
    let document_xml = part_text(&saved.bytes, "word/document.xml");
    let start = document_xml.find("id=\"vmlTextBox\"").expect("still VML");
    let rest = &document_xml[start..];
    let end = rest.find("</w:pict>").expect("closed");
    assert!(
        rest[..end].contains("Edited inside the VML text box"),
        "{}",
        &rest[..end]
    );
    assert!(!rest[..end].contains("flows through the shared pipeline"));
}

fn set_first_run_text(text_box: &mut TextBox, text: &str) {
    for block in &mut text_box.blocks {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &mut paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    run.text = text.to_owned();
                    return;
                }
            }
        }
    }
    panic!("the text box has a run");
}

/// 3. Moving a VML group changes its projection: the save writes it as
///    DrawingML at its new place and names the regeneration. The other picts
///    stay VML.
#[test]
fn moving_a_vml_group_writes_drawingml_and_says_so() {
    let mut imported = import(VML_SHAPES_DOCX);
    let lead = lead_of(&imported.document, "id=\"vmlGroup\"");
    let mut moved = false;
    for inline in body_inlines_mut(&mut imported.document) {
        if let InlineNode::Group(group) = inline
            && group.id == lead
        {
            let anchor = group.anchor.as_mut().expect("a floating group");
            anchor.horizontal.position = HorizontalPosition::Offset(914_400);
            moved = true;
        }
    }
    assert!(moved, "the fixture's group is one anchored group");
    let saved = save(&imported);
    let document_xml = part_text(&saved.bytes, "word/document.xml");
    assert!(
        !document_xml.contains("id=\"vmlGroup\""),
        "the moved group is not its old VML"
    );
    assert!(
        document_xml.contains("<wpg:wgp"),
        "it is written as a DrawingML group"
    );
    assert!(
        document_xml.contains("id=\"vmlRect\""),
        "untouched picts stay VML"
    );
    let regenerated: Vec<_> = saved
        .report
        .entries
        .iter()
        .filter(|entry| entry.feature == "docx.export.vml.regenerated")
        .collect();
    assert_eq!(regenerated.len(), 1, "{:?}", saved.report.entries);
    assert_eq!(regenerated[0].occurrences, 1);
    assert_eq!(regenerated[0].disposition, Disposition::DegradedNotRetained);
}

/// HF-390: drawn VML is "shown approximately" and preserved — on the semantic
/// path too, because its source is in the model — and never "kept, not shown".
/// A shapetype is a consumed definition and is not a finding at all.
#[test]
fn drawn_vml_is_reported_shown_approximately_and_preserved() {
    let imported = import(VML_SHAPES_DOCX);
    let vml = [
        "rect",
        "roundrect",
        "oval",
        "line",
        "shape",
        "group",
        "fill",
        "stroke",
        "textbox",
        "imagedata",
        "wrap",
    ];
    for name in vml {
        let entries: Vec<_> = imported
            .report
            .entries
            .iter()
            .filter(|entry| entry.feature == name)
            .collect();
        assert!(!entries.is_empty(), "{name} is reported");
        for entry in entries {
            assert_eq!(
                (
                    entry.disposition.model_outcome(),
                    entry.disposition.retention_outcome()
                ),
                (ModelOutcome::Degraded, RetentionOutcome::Preserved),
                "{name} in {:?}",
                entry.location.part_name
            );
        }
    }
    for consumed in ["shapetype", "formulas", "f", "lock"] {
        assert!(
            !imported
                .report
                .entries
                .iter()
                .any(|entry| entry.feature == consumed),
            "{consumed} is part of a consumed definition"
        );
    }
}
