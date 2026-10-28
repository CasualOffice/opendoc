//! P1 exit gate (`docs/182` §10): edit a frame of an imported PDF, save DOCX,
//! reopen it, and find the same text boxes, pictures and shapes, the edit, and
//! no loss the writer did not report.

use casual_doc_io::{
    DetectionRequest, DocumentResources, ExportMode, ExportRequest, FormatId, FormatSelection,
    ModelOutcome, builtin_registry, formats,
};
use casual_doc_model::v1::{BlockNode, Document, InlineNode};
use casual_doc_pdf_import::{PdfReadLimits, import_pdf};

#[derive(Debug, Default, PartialEq)]
struct Census {
    text_boxes: usize,
    pictures: usize,
    crops: usize,
    groups: usize,
    shapes: usize,
    text: String,
}

fn census(document: &Document) -> Census {
    let mut out = Census::default();
    for block in document.body() {
        let BlockNode::Paragraph(paragraph) = block else {
            continue;
        };
        for inline in &paragraph.inlines {
            match inline {
                InlineNode::TextBox(text_box) => {
                    out.text_boxes += 1;
                    for block in &text_box.blocks {
                        if let BlockNode::Paragraph(p) = block {
                            for inline in &p.inlines {
                                if let InlineNode::Run(run) = inline {
                                    out.text.push_str(&run.text);
                                }
                            }
                        }
                    }
                }
                InlineNode::AnchoredDrawing(drawing) => {
                    out.pictures += 1;
                    out.crops += usize::from(drawing.crop.is_some());
                }
                InlineNode::Group(group) => {
                    out.groups += 1;
                    out.shapes += group.children.len();
                }
                _ => {}
            }
        }
    }
    out
}

fn edit_first_frame(document: &mut Document, text: &str) {
    for block in document.body_mut() {
        let BlockNode::Paragraph(paragraph) = block else {
            continue;
        };
        for inline in &mut paragraph.inlines {
            if let InlineNode::TextBox(text_box) = inline
                && let Some(BlockNode::Paragraph(p)) = text_box.blocks.first_mut()
                && let Some(InlineNode::Run(run)) = p.inlines.first_mut()
            {
                run.text = text.to_owned();
                return;
            }
        }
    }
    panic!("no frame to edit");
}

fn round_trip(name: &str) {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf")
            .join(name),
    )
    .unwrap();
    let mut imported = import_pdf(bytes, PdfReadLimits::default()).expect("import");
    edit_first_frame(&mut imported.document, "Edited in the frame");
    imported
        .document
        .validate()
        .expect("the edit keeps the model valid");
    let before = census(&imported.document);
    assert!(before.text.contains("Edited in the frame"));

    let mut resources = DocumentResources::default();
    for (part, bytes) in &imported.resources {
        resources.insert(part.clone(), bytes.clone());
    }
    let registry = builtin_registry();
    let docx = registry
        .export(
            &FormatId::new(formats::DOCX).unwrap(),
            ExportRequest {
                document: &imported.document,
                resources: &resources,
                source: None,
                source_unchanged: false,
                mode: ExportMode::Semantic,
            },
        )
        .expect("DOCX export");
    let lost: Vec<_> = docx
        .report
        .entries
        .iter()
        .filter(|e| e.model_outcome != ModelOutcome::Mapped)
        .map(|e| e.feature.clone())
        .collect();
    assert!(
        lost.is_empty(),
        "{name}: the DOCX writer reported losses {lost:?}"
    );

    let reopened = registry
        .import(
            DetectionRequest {
                bytes: &docx.bytes,
                selection: FormatSelection::Auto,
                file_name_hint: None,
                mime_hint: None,
            },
            false,
        )
        .expect("reopen");
    let after = census(&reopened.document);
    // No unreported loss: every frame, picture and shape survives, with the
    // edit, and the pictures' bytes came back too.
    assert_eq!(after, before, "{name}");
    assert_eq!(
        reopened.resources.as_map().len(),
        imported.resources.len(),
        "{name}: picture bytes"
    );
    assert_eq!(
        reopened.document.definitions().sections.len(),
        imported.document.definitions().sections.len(),
        "{name}: one section per page"
    );
}

#[test]
fn an_edited_prose_pdf_saves_to_docx_and_reopens_with_its_frames() {
    round_trip("synthetic-prose.pdf");
}

#[test]
fn pictures_and_crops_survive_the_docx_round_trip() {
    round_trip("synthetic-images.pdf");
}

#[test]
fn shapes_survive_the_docx_round_trip() {
    round_trip("writer-real-producer-table-merges.pdf");
}
