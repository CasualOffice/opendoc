//! Saving a document whose styles part a Google Docs export wrote
//! (`docs/109` HF-401, HF-402, HF-406).
//!
//! The import now folds repeated `w:style` definitions into one and reads
//! twips written with a decimal tail (`w:w="144.0"`). Two save guarantees
//! follow, and this file holds both:
//!
//! 1. **Unedited, the save is the source file, byte for byte.** Folding the
//!    duplicates changes the model, not the file; a save with nothing edited
//!    must not regenerate `styles.xml` because the model's view of it moved.
//! 2. **Edited, the regenerated styles part carries what the import modelled.**
//!    The table style's indent and cell margins, and the folded `Heading1`'s
//!    outline level, are written back and survive a reopen; the one style the
//!    import could not fold (two copies disagreeing) is named by the save's
//!    report rather than vanishing.
//!
//! The fixture is the importer's `tests/table_style_cascade.rs` fixture,
//! trimmed to one table: an integration test cannot share another crate's
//! constants.

use std::io::{Cursor, Read as _, Write as _};

use casual_doc_io::{
    CompatibilityReport, DetectionRequest, ExportMode, ExportRequest, FormatId, FormatSelection,
    ImportArtifact, RetentionOutcome, builtin_registry, formats,
};
use casual_doc_model::v1::{BlockNode, InlineNode, Style};

const CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#;
const ROOT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
const DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#;

const STYLES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:name w:val="normal"/></w:style>
  <w:style w:type="table" w:styleId="TableNormal" w:default="1"><w:name w:val="Normal Table"/></w:style>
  <w:style w:type="paragraph" w:styleId="Normal" w:default="1"><w:name w:val="Normal"/><w:qFormat w:val="1"/></w:style>
  <w:style w:type="table" w:styleId="TableNormal" w:default="1"><w:name w:val="Normal Table"/><w:tblPr><w:tblInd w:w="0.0" w:type="dxa"/><w:tblCellMar><w:top w:w="0.0" w:type="dxa"/><w:left w:w="108.0" w:type="dxa"/><w:bottom w:w="0.0" w:type="dxa"/><w:right w:w="108.0" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>
  <w:style w:type="table" w:styleId="Padded"><w:name w:val="Padded"/><w:basedOn w:val="TableNormal"/><w:tblPr><w:tblInd w:w="360.0" w:type="dxa"/><w:tblCellMar><w:top w:w="144.0" w:type="dxa"/><w:left w:w="216" w:type="dxa"/><w:bottom w:w="72.0" w:type="dxa"/><w:right w:w="216.0" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="1F4E79"/></w:tcPr></w:tblStylePr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="600"/></w:pPr></w:style>
  <w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:qFormat w:val="1"/><w:pPr><w:spacing w:before="600"/><w:outlineLvl w:val="0"/></w:pPr></w:style>
  <w:style w:type="character" w:styleId="Clash"><w:name w:val="Clash"/><w:rPr><w:sz w:val="20"/></w:rPr></w:style>
  <w:style w:type="character" w:styleId="Clash"><w:name w:val="Clash"/><w:rPr><w:sz w:val="28"/></w:rPr></w:style>
</w:styles>"#;

const DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
  <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p>
  <w:tbl>
    <w:tblPr><w:tblStyle w:val="Padded"/><w:tblW w:w="4000" w:type="dxa"/><w:tblLook w:firstRow="1" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="1" w:noVBand="1"/></w:tblPr>
    <w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>
    <w:tr><w:tc><w:p><w:r><w:t>Head</w:t></w:r></w:p></w:tc></w:tr>
    <w:tr><w:tc><w:p><w:r><w:t>Body</w:t></w:r></w:p></w:tc></w:tr>
  </w:tbl>
  <w:p/>
</w:body></w:document>"#;

fn source() -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", DOCUMENT),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/styles.xml", STYLES),
    ] {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn open(bytes: &[u8]) -> ImportArtifact {
    builtin_registry()
        .import(
            DetectionRequest {
                bytes,
                selection: FormatSelection::Auto,
                file_name_hint: Some("google.docx"),
                mime_hint: None,
            },
            // The webapp's open: `retain_source`, so `ImportMode::Retention`.
            true,
        )
        .expect("the fixture opens")
}

fn save(
    imported: &ImportArtifact,
    mode: ExportMode,
    source_unchanged: bool,
) -> (Vec<u8>, CompatibilityReport) {
    let artifact = builtin_registry()
        .export(
            &FormatId::new(formats::DOCX).expect("the DOCX id"),
            ExportRequest {
                document: &imported.document,
                resources: &imported.resources,
                source: Some(&imported.source),
                source_unchanged,
                mode,
            },
        )
        .expect("the document saves");
    (artifact.bytes, artifact.report)
}

fn style<'a>(imported: &'a ImportArtifact, name: &str) -> &'a Style {
    imported
        .document
        .definitions()
        .styles
        .iter()
        .map(|(_, style)| style)
        .find(|style| style.name.as_deref() == Some(name))
        .unwrap_or_else(|| panic!("style {name}"))
}

fn styles_part(bytes: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a ZIP package");
    let mut part = archive.by_name("word/styles.xml").expect("a styles part");
    let mut text = String::new();
    part.read_to_string(&mut text).expect("UTF-8 styles");
    text
}

#[test]
fn an_unedited_save_is_the_source_file_byte_for_byte() {
    let bytes = source();
    let imported = open(&bytes);
    let (saved, report) = save(&imported, ExportMode::ExactIfUnchanged, true);
    assert!(
        saved == bytes,
        "an unedited save returns the original bytes"
    );
    assert!(report.entries.is_empty(), "and loses nothing: {report:?}");
}

#[test]
fn an_edited_save_writes_back_what_the_import_modelled_and_names_the_rest() {
    let bytes = source();
    let mut imported = open(&bytes);
    // A real edit: one character typed into the first body run.
    'edit: for block in imported.document.body_mut().iter_mut() {
        if let BlockNode::Paragraph(paragraph) = block {
            for inline in &mut paragraph.inlines {
                if let InlineNode::Run(run) = inline {
                    run.text.push('!');
                    break 'edit;
                }
            }
        }
    }
    let (saved, report) = save(&imported, ExportMode::PreserveWhenSafe, false);

    let styles = styles_part(&saved);
    assert_eq!(
        styles.matches("<w:name w:val=\"heading 1\"/>").count(),
        1,
        "the folded heading is written once: {styles}"
    );

    let reopened = open(&saved);
    let padded = style(&reopened, "Padded").table.as_ref().expect("tblPr");
    assert_eq!(
        padded.indent_twips,
        Some(360),
        "the style's indent survives"
    );
    assert_eq!(padded.cell_margins.top_twips, Some(144));
    assert_eq!(padded.cell_margins.end_twips, Some(216));
    let heading = style(&reopened, "heading 1");
    assert_eq!(
        heading.paragraph.as_ref().and_then(|p| p.outline_level),
        Some(0),
        "the later copy's outline level survives"
    );
    assert!(heading.q_format);

    // The one disagreement the import could not fold is named, not dropped.
    assert!(
        report
            .entries
            .iter()
            .any(|entry| entry.feature == "docx.style.duplicate.character"
                && entry.retention_outcome == RetentionOutcome::NotRetained),
        "the regenerating save names the dropped duplicate: {report:#?}"
    );
}
