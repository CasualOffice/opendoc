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

/// A one-page image-only PDF: a white 8 x 8 grey picture over the whole
/// page, uncompressed. A scan with nothing native on it.
fn blank_scan() -> Vec<u8> {
    let objects: [Vec<u8>; 5] = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
        [
            b"<< /Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 64 >>\nstream\n".as_slice(),
            &[255u8; 64],
            b"\nendstream",
        ]
        .concat(),
        {
            let content = b"q 612 0 0 792 0 0 cm /Im0 Do Q";
            [
                format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(),
                content,
                b"\nendstream",
            ]
            .concat()
        },
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// Reads "Scanned" near the top left of whatever raster it is given; no
/// model.
struct Fake;

impl casual_doc_io::pdf_ocr::OcrProvider for Fake {
    fn manifest(&self) -> casual_doc_io::pdf_ocr::ProviderManifest {
        casual_doc_io::pdf_ocr::ProviderManifest {
            provider: "fake",
            version: "test",
            models: vec![("detection", "00".repeat(32))],
            decoding: "fixed",
            determinism: casual_doc_io::pdf_ocr::Determinism::Exact,
        }
    }

    fn recognize(
        &self,
        raster: &casual_doc_io::pdf_ocr::Raster,
        _limits: &casual_doc_io::pdf_ocr::OcrLimits,
        _interrupt: &dyn casual_doc_io::pdf_ocr::Interrupt,
    ) -> Result<casual_doc_io::pdf_ocr::OcrPage, casual_doc_io::pdf_ocr::OcrError> {
        let (w, h) = (raster.width() as f32, raster.height() as f32);
        let bounds = casual_doc_io::pdf_ocr::OcrBox {
            x0: w * 0.1,
            y0: h * 0.1,
            x1: w * 0.4,
            y1: h * 0.12,
        };
        let word = casual_doc_io::pdf_ocr::OcrWord {
            text: "Scanned".to_owned(),
            bounds,
            confidence: 0.99,
        };
        Ok(casual_doc_io::pdf_ocr::OcrPage {
            lines: vec![casual_doc_io::pdf_ocr::OcrLine::upright(vec![word], bounds)],
            ..casual_doc_io::pdf_ocr::OcrPage::default()
        })
    }
}

#[test]
fn a_scan_opens_with_its_text_when_the_host_supplies_ocr() {
    let scan = blank_scan();
    let plain = ViewerDocument::open(&scan).expect("opens");
    assert!(!plain.plain_text().contains("Scanned"));
    let options =
        opendoc_viewer::ViewerOptions::default().with_pdf_ocr(casual_doc_io::PdfOcrOptions {
            provider: Some(std::sync::Arc::new(Fake)),
            ..casual_doc_io::PdfOcrOptions::default()
        });
    let read = ViewerDocument::open_with(&scan, &options).expect("opens");
    let text = read.plain_text();
    assert!(text.contains("Scanned"), "{text}");
}

#[test]
fn the_reflow_option_opens_a_pdf_as_a_flowing_document() {
    let bytes = fixture("synthetic-prose.pdf");
    let fixed = ViewerDocument::open(&bytes).expect("opens");
    let frames =
        |document: &ViewerDocument| format!("{:?}", document.document().body()).contains("TextBox");
    assert!(frames(&fixed), "the default is the fixed layout's frames");
    let options = opendoc_viewer::ViewerOptions::default().with_pdf_reflow(true);
    let flowing = ViewerDocument::open_with(&bytes, &options).expect("opens");
    assert!(!frames(&flowing), "reflowed: paragraphs, not frames");
    let text = flowing.plain_text();
    assert!(text.contains("quick brown fox"), "{text}");
    assert!(flowing.page_count() >= 1);
    // The option means nothing to a format that is not PDF.
    let docx =
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../sample.docx"))
            .expect("sample.docx");
    let plain = ViewerDocument::open(&docx).unwrap();
    let asked = ViewerDocument::open_with(&docx, &options).unwrap();
    assert_eq!(plain.page_count(), asked.page_count());
    assert_eq!(plain.plain_text(), asked.plain_text());
}
