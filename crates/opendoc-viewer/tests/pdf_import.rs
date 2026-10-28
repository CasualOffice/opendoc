//! The Viewer SDK opens a PDF when built with `pdf-import` (`docs/182` P1):
//! the same calls a host makes for a DOCX, with no PDF-specific code.
#![cfg(feature = "pdf-import")]

use opendoc_viewer::ViewerDocument;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf")
            .join(name),
    )
    .expect(name)
}

#[test]
fn a_pdf_opens_paginates_renders_and_exports() {
    let document = ViewerDocument::open(&fixture("synthetic-prose.pdf")).expect("opens");
    assert_eq!(document.page_count(), 2, "one page per PDF page");
    assert_eq!(document.page_size_twips(0), Some((612 * 20, 792 * 20)));
    let text = document.plain_text();
    assert!(text.contains("Synthetic prose"), "{text}");
    assert!(text.contains("quick brown fox"), "{text}");
    let png = document.render_page_png(0, 36.0).expect("renders");
    assert!(png.starts_with(b"\x89PNG"));
}

#[test]
fn an_embedded_program_reaches_the_viewer_document() {
    // Registration itself is not observable here: Caladea is also bundled,
    // so a substitute and the embedded program draw the same face.
    let document = ViewerDocument::open(&fixture("synthetic-embedded-truetype.pdf")).unwrap();
    let programs = &document.document().definitions().font_programs;
    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].family, "Caladea");
}

#[test]
fn an_encrypted_pdf_is_refused() {
    let error = ViewerDocument::open(&fixture("hostile/hostile-encrypted.pdf")).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("pdf.import.encrypted_unsupported"),
        "{error}"
    );
}
