//! `docs/142` LST-10 / LST-34 — a Word **List Style** list must arrive with its
//! markers.
//!
//! # The shape under test
//!
//! Word's List Styles (Home ▸ Multilevel List ▸ Define New List Style) write a
//! numbering part that no other producer shape resembles:
//!
//! - `w:abstractNum` **0** carries `<w:numStyleLink w:val="MyListStyle"/>` and
//!   **no `w:lvl` at all**;
//! - `w:abstractNum` **1** carries `<w:styleLink w:val="MyListStyle"/>` and the
//!   real levels;
//! - `w:num` **1** points at abstract 0 — and it is the one body paragraphs use;
//! - `w:num` **2** points at abstract 1, and the paragraph style `MyListStyle`
//!   names *it* in its own `w:pPr/w:numPr`, which is the hop that closes the loop.
//!
//! So the levels a body paragraph needs are two indirections away from the
//! abstract its instance names. The importer used to require the level to be a
//! `w:lvl` child of that abstract, found none, and dropped the reference — every
//! paragraph of the list opened as plain text, with no marker and no finding.
//!
//! # Why this guard paginates instead of inspecting the model
//!
//! A marker is not in the model: it is produced by the layout engine from the
//! resolved level. A guard that stops at "the `NumberingRef` is `Some`" cannot
//! distinguish a fixed import from one that resolves to the wrong level, and it
//! is exactly the kind of assertion that let this defect live — `SKILL.md` §4:
//! assert the guarantee, not the mechanism. So this test lays the imported
//! document out through the real paginator and reads the page.

use std::io::{Cursor, Write};

use casual_doc_import::{ImportConfig, ImportMode, import_package};
use casual_doc_layout::block::BlockFragment;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::numbering::NumberingState;
use casual_doc_layout::page::PlacedFragment;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::v1::{BlockNode, Document, InlineNode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

const CONTENT_TYPES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/></Types>"#;
const ROOT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
const DOCUMENT_RELS: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/></Relationships>"#;

/// The three body paragraphs, all on `w:numId="1"` — the instance whose abstract
/// carries only the `w:numStyleLink`.
const DOCUMENT: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
    <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>
        <w:r><w:t>Alpha</w:t></w:r></w:p>
    <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>
        <w:r><w:t>Bravo</w:t></w:r></w:p>
    <w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr>
        <w:r><w:t>Charlie</w:t></w:r></w:p>
</w:body></w:document>"#;

/// The List Style itself: a paragraph style whose `w:pPr/w:numPr` names the
/// instance that carries the levels. This is the hop `w:numStyleLink` follows.
const STYLES: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:style w:type="paragraph" w:styleId="MyListStyle">
        <w:name w:val="My List Style"/>
        <w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr>
    </w:style>
</w:styles>"#;

/// The numbering part in Word's List-Style shape: a level-less abstract that
/// defers through `w:numStyleLink`, and the abstract that answers it.
const NUMBERING: &[u8] = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
    <w:abstractNum w:abstractNumId="0">
        <w:multiLevelType w:val="multilevel"/>
        <w:numStyleLink w:val="MyListStyle"/>
    </w:abstractNum>
    <w:abstractNum w:abstractNumId="1">
        <w:multiLevelType w:val="multilevel"/>
        <w:styleLink w:val="MyListStyle"/>
        <w:lvl w:ilvl="0" w:tplc="04090001" w:tentative="1">
            <w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/>
            <w:lvlJc w:val="left"/>
            <w:pPr><w:ind w:start="720" w:hanging="360"/></w:pPr>
        </w:lvl>
        <w:lvl w:ilvl="1">
            <w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%2)"/>
            <w:lvlJc w:val="left"/>
            <w:pPr><w:ind w:start="1440" w:hanging="360"/></w:pPr>
        </w:lvl>
    </w:abstractNum>
    <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>
    <w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>
</w:numbering>"#;

fn zip_package(parts: &[(&str, &[u8])]) -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn list_style_package() -> Vec<u8> {
    zip_package(&[
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", DOCUMENT),
        ("word/_rels/document.xml.rels", DOCUMENT_RELS),
        ("word/styles.xml", STYLES),
        ("word/numbering.xml", NUMBERING),
    ])
}

fn import(bytes: &[u8]) -> casual_doc_import::Import {
    let mut package = DocxPackage::open(bytes, PackageLimits::default()).unwrap();
    import_package(
        &mut package,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .unwrap()
}

/// The paragraph's own characters, concatenated from its runs.
fn paragraph_text(block: &BlockNode) -> String {
    match block {
        BlockNode::Paragraph(paragraph) => paragraph
            .inlines
            .iter()
            .filter_map(|inline| match inline {
                InlineNode::Run(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect(),
        _ => String::new(),
    }
}

/// The document as a reader sees it: one line per body paragraph, the marker the
/// engine resolved for it followed by its text.
///
/// Built from the **layout's** numbering engine walked in document order, which
/// is the same state the paginator advances, so an unmarked line here is an
/// unmarked line on the page.
fn document_transcript(document: &Document) -> String {
    let mut state = NumberingState::new();
    let mut out = String::new();
    for block in document.body() {
        let marker = match block {
            BlockNode::Paragraph(paragraph) => paragraph
                .properties
                .numbering
                .as_ref()
                .and_then(|reference| state.resolve(document.definitions(), reference))
                .map(|resolved| resolved.text),
            _ => None,
        };
        out.push_str(&format!(
            "{} {}\n",
            marker.as_deref().unwrap_or("<no marker>"),
            paragraph_text(block)
        ));
    }
    out
}

/// Every laid-out paragraph on every page, as `(text-ish key, marker glyph
/// count)`: how many glyphs the paginator actually put in a marker run on the
/// paragraph's first line. `0` means the page shows no marker.
fn marker_glyphs_per_paragraph(placed: &[PlacedFragment]) -> Vec<usize> {
    placed
        .iter()
        .filter_map(|fragment| match &fragment.fragment {
            BlockFragment::Paragraph { lines, .. } => {
                let line = lines.lines.first()?;
                Some(
                    line.runs
                        .iter()
                        .filter(|run| run.is_marker)
                        .map(|run| run.glyphs.len())
                        .sum(),
                )
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_word_list_style_list_arrives_with_its_markers() {
    let import = import(&list_style_package());
    let document = &import.document;

    // The model must ACCEPT the document. This is LST-34: a fix that taught the
    // importer to follow the link without moving the model's validator would
    // trade a silently unmarked list for a refused document, which is worse.
    document
        .validate()
        .expect("a List-Style list must validate, not be rejected");

    // The guarantee, stated as the document a reader sees.
    assert_eq!(
        document_transcript(document),
        "1. Alpha\n2. Bravo\na) Charlie\n",
        "a Word List-Style list must open with its markers"
    );

    // And the markers reach the PAGE, not just the resolver: the paginator put a
    // marker run with glyphs on the first line of all three paragraphs.
    let layout = paginate_document(document, &ParleyShaper::new());
    let glyphs: Vec<usize> = layout
        .pages
        .iter()
        .flat_map(|page| marker_glyphs_per_paragraph(&page.placed))
        .collect();
    assert_eq!(
        glyphs.len(),
        3,
        "three body paragraphs were laid out; got {glyphs:?}"
    );
    assert!(
        glyphs.iter().all(|count| *count > 0),
        "every list paragraph must carry a marker run on the page; marker glyph \
         counts were {glyphs:?} for the transcript:\n{}",
        document_transcript(document)
    );
}
