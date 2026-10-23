//! The Viewer SDK's API guards (`docs/176` §11): every call a host makes, with
//! the property it promises, against the committed root `sample.docx` and small
//! packages built here.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::{Cursor, Write};

use opendoc_viewer::{ViewerDocument, ViewerError, ViewerOptions};

fn sample() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../sample.docx"))
        .expect("the committed root sample.docx")
}

/// A minimal `.docx` with a styles part, built in memory.
fn docx(body: &str, styles: &str) -> Vec<u8> {
    use zip::write::SimpleFileOptions;
    let parts: [(&str, String); 5] = [
        (
            "[Content_Types].xml",
            concat!(
                "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">",
                "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>",
                "<Default Extension=\"xml\" ContentType=\"application/xml\"/>",
                "<Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>",
                "<Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/>",
                "</Types>"
            )
            .to_owned(),
        ),
        (
            "_rels/.rels",
            concat!(
                "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
                "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>",
                "</Relationships>"
            )
            .to_owned(),
        ),
        (
            "word/_rels/document.xml.rels",
            concat!(
                "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
                "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>",
                "</Relationships>"
            )
            .to_owned(),
        ),
        (
            "word/document.xml",
            format!(
                "<?xml version=\"1.0\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
                 <w:body>{body}</w:body></w:document>"
            ),
        ),
        (
            "word/styles.xml",
            format!(
                "<?xml version=\"1.0\"?><w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">{styles}</w:styles>"
            ),
        ),
    ];
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in parts {
        writer
            .start_file(name, SimpleFileOptions::default())
            .expect("zip entry");
        writer.write_all(bytes.as_bytes()).expect("zip write");
    }
    writer.finish().expect("zip finish").into_inner()
}

fn digest(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[test]
fn a_docx_opens_paginates_and_renders_every_page() {
    let doc = ViewerDocument::open(&sample()).expect("open");
    assert!(doc.page_count() >= 1);
    assert_eq!(doc.page_sizes_twips().len(), doc.page_count());
    for index in 0..doc.page_count() {
        let (w, h) = doc.page_size_twips(index).expect("size");
        assert_eq!(doc.page_sizes_twips()[index], (w, h));
        let bitmap = doc.render_page(index, 48.0).expect("render");
        assert_eq!(bitmap.width_px, (w as f32 * 48.0 / 1440.0).ceil() as u32);
        assert_eq!(bitmap.height_px, (h as f32 * 48.0 / 1440.0).ceil() as u32);
        assert_eq!(
            bitmap.rgba.len(),
            (bitmap.width_px * bitmap.height_px * 4) as usize
        );
        // A rendered page is not blank: some pixel is not the white page.
        assert!(
            bitmap
                .rgba
                .chunks_exact(4)
                .any(|px| px[..3] != [255, 255, 255]),
            "page {index} rendered blank"
        );
    }
}

/// Determinism per page: two independent opens of the same bytes render every
/// page to the same pixels, and a page rendered twice from one open is the same
/// both times. This is the guarantee `docs/176` §2.1 sells.
#[test]
fn every_page_renders_identically_across_opens_and_repeats() {
    assert!(!ViewerOptions::default().system_fonts);
    let a = ViewerDocument::open(&sample()).expect("open");
    let b = ViewerDocument::open(&sample()).expect("open");
    assert_eq!(a.page_count(), b.page_count());
    for index in 0..a.page_count() {
        let first = digest(&a.render_page(index, 36.0).expect("a").rgba);
        let again = digest(&a.render_page(index, 36.0).expect("a again").rgba);
        let other = digest(&b.render_page(index, 36.0).expect("b").rgba);
        assert_eq!(first, again, "page {index} changed between two renders");
        assert_eq!(first, other, "page {index} differs between two opens");
    }
}

#[test]
fn a_page_past_the_end_is_refused_with_the_count() {
    let doc = ViewerDocument::open(&sample()).expect("open");
    let pages = doc.page_count();
    assert_eq!(
        doc.render_page(pages, 48.0),
        Err(ViewerError::NoSuchPage {
            index: pages,
            pages
        })
    );
    assert_eq!(doc.page_size_twips(pages), None);
    assert!(matches!(
        doc.page_text(pages),
        Err(ViewerError::NoSuchPage { .. })
    ));
}

#[test]
fn a_resolution_that_is_not_positive_is_refused() {
    let doc = ViewerDocument::open(&sample()).expect("open");
    for dpi in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(
            matches!(doc.render_page(0, dpi), Err(ViewerError::Surface(_))),
            "{dpi} dpi was accepted"
        );
    }
}

#[test]
fn a_thumbnail_is_a_png() {
    let doc = ViewerDocument::open(&sample()).expect("open");
    let png = doc.render_page_png(0, 24.0).expect("png");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}

/// Text per page and whole: the pages' texts, concatenated, are the body text,
/// and each page carries its own share of it.
#[test]
fn the_pages_texts_concatenate_to_the_body_text() {
    let doc = ViewerDocument::open(&sample()).expect("open");
    let whole = doc.plain_text();
    assert!(whole.lines().any(|line| !line.trim().is_empty()));
    let pages: Vec<String> = (0..doc.page_count())
        .map(|index| doc.page_text(index).expect("page text"))
        .collect();
    assert!(
        doc.page_count() < 2 || pages[0] != pages[1],
        "two pages read the same text"
    );
    assert_eq!(pages.concat(), whole);
}

#[test]
fn page_text_follows_a_page_break() {
    let doc = ViewerDocument::open(&docx(
        concat!(
            "<w:p><w:r><w:t>first page</w:t></w:r></w:p>",
            "<w:p><w:r><w:br w:type=\"page\"/><w:t>second page</w:t></w:r></w:p>",
        ),
        "",
    ))
    .expect("open");
    assert_eq!(doc.page_count(), 2);
    // The break itself is read where it is: at the end of the page it ends.
    assert_eq!(doc.page_text(0).expect("p0"), "first page\n\n");
    assert_eq!(doc.page_text(1).expect("p1"), "second page\n");
    assert!(
        doc.page_text(1).expect("p1").contains("second page"),
        "{:?}",
        doc.page_text(1)
    );
    assert!(!doc.page_text(0).expect("p0").contains("second"));
}

#[test]
fn the_outline_lists_headings_with_level_and_page() {
    let styles = concat!(
        "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/></w:style>",
        "<w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/></w:style>",
    );
    let doc = ViewerDocument::open(&docx(
        concat!(
            "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Introduction</w:t></w:r></w:p>",
            "<w:p><w:r><w:t>Body text.</w:t></w:r></w:p>",
            "<w:p><w:pPr><w:pStyle w:val=\"Heading2\"/></w:pPr><w:r><w:t>Scope</w:t></w:r></w:p>",
            "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:br w:type=\"page\"/><w:t>Results</w:t></w:r></w:p>",
        ),
        styles,
    ))
    .expect("open");
    let outline: Vec<(u8, String, Option<usize>)> = doc
        .outline()
        .into_iter()
        .map(|entry| (entry.level, entry.text, entry.page))
        .collect();
    assert_eq!(
        outline,
        vec![
            (1, "Introduction".to_owned(), Some(0)),
            (2, "Scope".to_owned(), Some(0)),
            (1, "Results".to_owned(), Some(1)),
        ]
    );
}

#[cfg(not(feature = "formats"))]
#[test]
fn bytes_that_are_not_a_docx_are_refused() {
    assert!(matches!(
        ViewerDocument::open(b"not a document"),
        Err(ViewerError::Open(_))
    ));
}

/// With every importer registered, unstructured bytes are a plain-text
/// document, exactly as the editor opens them.
#[cfg(feature = "formats")]
#[test]
fn bytes_that_are_not_a_package_open_as_plain_text() {
    let doc = ViewerDocument::open(b"not a document").expect("plain text");
    assert_eq!(doc.plain_text(), "not a document\n");
}

/// Liberation Mono with its family renamed to `OpendocTestFace` (same
/// length, every `name` record), so it is a face no build bundles, under a
/// name with no class hint (`Serif`, `Mono`) that would make the substitute
/// the same face.
fn renamed_face() -> Vec<u8> {
    let mut bytes = casual_doc_layout::fonts::LIBERATION_MONO_REGULAR.to_vec();
    let (from, to) = ("Liberation Mono", "OpendocTestFace");
    assert_eq!(from.len(), to.len());
    let utf16 = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let mut replaced = 0;
    for (needle, with) in [
        (from.as_bytes().to_vec(), to.as_bytes().to_vec()),
        (utf16(from), utf16(to)),
    ] {
        let mut at = 0;
        while let Some(found) = bytes[at..]
            .windows(needle.len())
            .position(|window| window == needle.as_slice())
        {
            let start = at + found;
            bytes[start..start + needle.len()].copy_from_slice(&with);
            at = start + needle.len();
            replaced += 1;
        }
    }
    assert!(replaced > 0, "the face carries no family name to rename");
    bytes
}

/// The host font hook end to end: the document asks for a family the build
/// does not have, the viewer says so (with the substitute it used), the host
/// registers the face, and the next layout uses it — the need is gone and the
/// page's pixels change.
#[test]
fn a_missing_family_is_reported_and_a_host_face_fills_it() {
    let body = concat!(
        "<w:p><w:r><w:rPr><w:rFonts w:ascii=\"OpendocTestFace\" w:hAnsi=\"OpendocTestFace\"/></w:rPr>",
        "<w:t>The quick brown fox jumps over the lazy dog.</w:t></w:r></w:p>",
    );
    let bytes = docx(body, "");
    let mut doc = ViewerDocument::open(&bytes).expect("open");
    let need = doc
        .font_needs()
        .iter()
        .find(|need| need.requested == "OpendocTestFace")
        .unwrap_or_else(|| panic!("no need reported: {:?}", doc.font_needs()))
        .clone();
    assert!(need.substitute.is_some(), "{need:?}");
    assert!(need.substitute_available, "{need:?}");
    let before = digest(&doc.render_page(0, 48.0).expect("before").rgba);

    assert_eq!(doc.register_fonts([renamed_face()]), 1);
    assert!(
        doc.font_needs()
            .iter()
            .all(|need| need.requested != "OpendocTestFace"),
        "the registered face did not satisfy the need: {:?}",
        doc.font_needs()
    );
    let after = digest(&doc.render_page(0, 48.0).expect("after").rgba);
    assert_ne!(before, after, "the host face did not change the page");

    // Registering up front gives the same pages in one layout.
    let preloaded =
        ViewerDocument::open_with(&bytes, &ViewerOptions::default().with_font(renamed_face()))
            .expect("open with font");
    assert!(
        preloaded.font_needs().is_empty(),
        "{:?}",
        preloaded.font_needs()
    );
    assert_eq!(
        digest(&preloaded.render_page(0, 48.0).expect("preloaded").rgba),
        after
    );
}

#[test]
fn bytes_that_are_not_a_font_register_nothing_and_keep_the_layout() {
    let mut doc = ViewerDocument::open(&sample()).expect("open");
    let before = digest(&doc.render_page(0, 24.0).expect("before").rgba);
    assert_eq!(doc.register_fonts([b"not a font".to_vec()]), 0);
    assert_eq!(
        digest(&doc.render_page(0, 24.0).expect("after").rgba),
        before
    );
}

#[cfg(feature = "pdf")]
#[test]
fn the_pdf_has_one_page_per_laid_out_page() {
    let doc = ViewerDocument::open(&sample()).expect("open");
    let pdf = doc.to_pdf().expect("pdf");
    assert!(pdf.starts_with(b"%PDF-"));
    // Each page object is written as `<</Type/Page/Parent …` (lib.rs of
    // casual-doc-pdf); the page tree is `/Type/Pages`, which this skips.
    let marker = b"/Type/Page/Parent";
    let count = pdf.windows(marker.len()).filter(|w| w == marker).count();
    assert_eq!(count, doc.page_count());
}

/// The same document converts to the same PDF bytes: no clock, no random id.
#[cfg(feature = "pdf")]
#[test]
fn the_pdf_is_deterministic_for_the_same_document() {
    let a = ViewerDocument::open(&sample())
        .expect("open")
        .to_pdf()
        .expect("a");
    let b = ViewerDocument::open(&sample())
        .expect("open")
        .to_pdf()
        .expect("b");
    assert_eq!(digest(&a), digest(&b));
}
