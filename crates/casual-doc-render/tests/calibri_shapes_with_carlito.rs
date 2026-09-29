//! Which face actually paints a `Calibri` run — measured, not read.
//!
//! # Why this exists
//!
//! A page-count divergence against LibreOffice on a real document was
//! attributed to font provisioning: `pdffonts` showed LibreOffice embedding
//! Carlito (its metric-compatible Calibri clone), and the conclusion drawn was
//! that we substitute something with different metrics. The code contradicts
//! that — `font_substitution` maps `calibri -> CARLITO`, and `resolve` goes
//! through `substitute` unconditionally — but reading a resolver is not
//! measuring it, so this reads the face off the glyph runs the compositor
//! emits. On the machine this was written on (no Calibri installed) a Calibri
//! run paints with `FontId(8)`, which is Carlito: the same face LibreOffice
//! embedded. The font hypothesis is therefore not the cause of that drift.
//!
//! # What is asserted, and why it is not a host-dependent assertion
//!
//! Face selection legitimately depends on the host: `ParleyShaper::pick_family`
//! keeps a genuinely INSTALLED family (a real macOS Arial) and only substitutes
//! when the requested family is absent, which is correct and is why the answer
//! differs between this laptop and a Linux runner. So the guard is written
//! against the part that must hold everywhere:
//!
//! - a named family is never painted with the bundled DEFAULT (Roboto). That is
//!   precisely the failure the other lane hypothesised — substitution not being
//!   consulted, so a run gets the wrong advances and the page count drifts;
//! - when it does land on a bundled face, it is the documented metric partner.
//!
//! A host-resolved face is reported as a dynamic id (`>= DYNAMIC_FONT_BASE`),
//! which is distinguishable from Roboto — `fonts::family_name` is not, since it
//! answers "Roboto" for every id it does not recognise, including every host
//! face. That fallback is why a naive reading of the painted family name looks
//! like a substitution bug when it is not, and it is worth knowing before the
//! next investigation starts from the same signal.

use casual_doc_import::ImportConfig;
use casual_doc_import::ImportMode;
use casual_doc_import::import_package;
use casual_doc_layout::compose::compose_page;
use casual_doc_layout::display::PaintItem;
use casual_doc_layout::document_layout::paginate_document;
use casual_doc_layout::font_registry::DYNAMIC_FONT_BASE;
use casual_doc_layout::fonts::family_name;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_ooxml::DocxPackage;
use casual_doc_ooxml::PackageLimits;
use std::collections::BTreeSet;
use std::io::Cursor;
use std::io::Write;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

/// A one-paragraph document whose single run names `family`.
fn document_xml(family: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:rPr><w:rFonts w:ascii="{family}" w:hAnsi="{family}"/><w:sz w:val="22"/></w:rPr><w:t>The quick brown fox</w:t></w:r></w:p>
<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr>
</w:body></w:document>"#
    )
}

fn package(document: &str) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, body) in [
        ("[Content_Types].xml", CONTENT_TYPES),
        ("_rels/.rels", ROOT_RELS),
        ("word/document.xml", document),
    ] {
        zip.start_file(name, options).expect("a fresh zip entry");
        zip.write_all(body.as_bytes()).expect("writing a zip entry");
    }
    zip.finish().expect("finishing the zip");
    buffer.into_inner()
}

/// How the faces painted on page 1 are named, one entry per distinct face.
///
/// A bundled face is named by its family; a face the HOST supplied (the shaper
/// kept a genuinely installed family) is named `"<host face>"`, because
/// `fonts::family_name` cannot name one and answers "Roboto" for it.
///
/// Complexity: linear in the page's paint items, over a one-run document.
fn painted_faces(family: &str) -> BTreeSet<&'static str> {
    let bytes = package(&document_xml(family));
    let mut docx = DocxPackage::open(&bytes, PackageLimits::default()).expect("a valid package");
    let imported = import_package(
        &mut docx,
        ImportConfig {
            mode: ImportMode::Semantic,
            ..ImportConfig::default()
        },
    )
    .expect("the fixture imports");
    let shaper = ParleyShaper::new();
    let pages = paginate_document(&imported.document, &shaper);
    let page = pages.pages.first().expect("one page");
    compose_page(page)
        .items
        .iter()
        .filter_map(|item| match item {
            PaintItem::Glyphs { run } if run.font.0 >= DYNAMIC_FONT_BASE => Some("<host face>"),
            PaintItem::Glyphs { run } => Some(family_name(run.font)),
            _ => None,
        })
        .collect()
}

/// The five families a Word document reaches for, and the bundled face each must
/// land on when the host does not have it. `"<host face>"` is also acceptable —
/// an installed face has the true metrics and is the better answer — but the
/// bundled DEFAULT never is.
const EXPECTED: [(&str, &str); 5] = [
    ("Calibri", "Carlito"),
    ("Cambria", "Caladea"),
    ("Arial", "Liberation Sans"),
    ("Times New Roman", "Liberation Serif"),
    ("Courier New", "Liberation Mono"),
];

/// The measurement the font hypothesis stands or falls on.
#[test]
fn a_calibri_run_is_never_painted_with_the_default_face() {
    for (requested, partner) in EXPECTED {
        let faces = painted_faces(requested);
        assert_eq!(faces.len(), 1, "{requested} paints one face, got {faces:?}");
        let face = *faces.iter().next().expect("one face");
        assert!(
            face == partner || face == "<host face>",
            "{requested} must paint with its metric-compatible partner \
             ({partner}) or with the host's own installed face — painting it \
             with {face} means substitution was not consulted, which is the \
             one thing that would change line breaking and the page count"
        );
    }
}
