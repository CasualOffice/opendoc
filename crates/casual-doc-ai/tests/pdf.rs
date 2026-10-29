// SPDX-License-Identifier: Apache-2.0

//! PDF sessions (`docs/182` §18, §20a): a host's OCR reaches the session that
//! opens a scan, and "Convert to flowing document" opens a NEW session from a
//! PDF session. The provider is a scripted fake, so CI needs no model.
#![cfg(feature = "pdf-import")]

use std::sync::Arc;

use casual_doc_ai::{PdfLayout, Session, SessionConfig};
use casual_doc_io::pdf_ocr;

fn prose() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf/synthetic-prose.pdf"),
    )
    .expect("the prose fixture")
}

/// A one-page image-only PDF: a white 8 x 8 grey picture over the whole
/// page, uncompressed. A scan with nothing native on it.
fn blank_scan() -> Vec<u8> {
    let content = b"q 612 0 0 792 0 0 cm /Im0 Do Q";
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
        [
            format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(),
            content,
            b"\nendstream",
        ]
        .concat(),
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

/// Reads "Scanned" near the top left of whatever raster it is given.
struct Fake;

impl pdf_ocr::OcrProvider for Fake {
    fn manifest(&self) -> pdf_ocr::ProviderManifest {
        pdf_ocr::ProviderManifest {
            provider: "fake",
            version: "test",
            models: vec![("detection", "00".repeat(32))],
            decoding: "fixed",
            determinism: pdf_ocr::Determinism::Exact,
        }
    }

    fn recognize(
        &self,
        raster: &pdf_ocr::Raster,
        _limits: &pdf_ocr::OcrLimits,
        _interrupt: &dyn pdf_ocr::Interrupt,
    ) -> Result<pdf_ocr::OcrPage, pdf_ocr::OcrError> {
        let (w, h) = (raster.width() as f32, raster.height() as f32);
        let bounds = pdf_ocr::OcrBox {
            x0: w * 0.1,
            y0: h * 0.1,
            x1: w * 0.4,
            y1: h * 0.12,
        };
        let word = pdf_ocr::OcrWord {
            text: "Scanned".to_owned(),
            bounds,
            confidence: 0.99,
        };
        Ok(pdf_ocr::OcrPage {
            lines: vec![pdf_ocr::OcrLine::upright(vec![word], bounds)],
            ..pdf_ocr::OcrPage::default()
        })
    }
}

fn body(session: &Session) -> String {
    format!("{:?}", session.document().body())
}

#[test]
fn a_session_opened_with_the_hosts_ocr_reads_a_scan() {
    let scan = blank_scan();
    let plain = Session::open(&scan, "scan.pdf", SessionConfig::default()).expect("opens");
    assert!(!body(&plain).contains("Scanned"));
    assert_eq!(
        plain.pdf_ocr_summary().map(|s| s.provider.is_some()),
        Some(false)
    );
    let ocr = casual_doc_io::PdfOcrOptions {
        provider: Some(Arc::new(Fake)),
        ..casual_doc_io::PdfOcrOptions::default()
    };
    let read =
        Session::open_with_pdf_ocr(&scan, "scan.pdf", SessionConfig::default(), &ocr).unwrap();
    assert!(body(&read).contains("Scanned"), "{}", body(&read));
    let summary = read.pdf_ocr_summary().expect("a PDF session");
    assert_eq!(
        summary.provider.as_ref().map(|m| m.provider),
        Some("fake"),
        "the session records what read its pages"
    );
    // Any other format opens exactly as without OCR.
    let text =
        Session::open_with_pdf_ocr(b"plain words", "notes.txt", SessionConfig::default(), &ocr)
            .unwrap();
    assert!(text.pdf_ocr_summary().is_none());
}

/// A PDF opens as a flowing document by default (`docs/182` §4): no frames,
/// the text in body paragraphs, and it does not convert again. Mutation:
/// make `PdfLayout::default()` `Fixed` and the body holds `TextBox`.
#[test]
fn a_pdf_session_opens_as_a_flowing_document_by_default() {
    let pdf = Session::open(&prose(), "prose.pdf", SessionConfig::default()).expect("opens");
    assert!(!body(&pdf).contains("TextBox"), "{}", body(&pdf));
    assert!(body(&pdf).contains("quick brown fox"));
    assert_eq!(pdf.pdf_layout(), Some(PdfLayout::Flowing));
    assert_eq!(
        pdf.convert_pdf_to_flowing().unwrap_err().code,
        "already-flowing"
    );
    let fixed = Session::open_pdf(
        &prose(),
        "prose.pdf",
        SessionConfig::default(),
        None,
        PdfLayout::Fixed,
    )
    .expect("opens");
    assert!(body(&fixed).contains("TextBox"), "fixed layout: frames");
    assert_eq!(fixed.pdf_layout(), Some(PdfLayout::Fixed));
}

#[test]
fn converting_a_pdf_session_opens_a_new_flowing_session() {
    let pdf = Session::open_pdf(
        &prose(),
        "prose.pdf",
        SessionConfig::default(),
        None,
        PdfLayout::Fixed,
    )
    .expect("opens");
    let before = pdf.document().clone();
    assert!(body(&pdf).contains("TextBox"), "fixed layout: frames");
    let flowing = pdf.convert_pdf_to_flowing().expect("converts");
    assert!(!body(&flowing).contains("TextBox"), "{}", body(&flowing));
    assert!(body(&flowing).contains("quick brown fox"));
    assert_eq!(flowing.name(), "prose.pdf (flowing)");
    assert_eq!(pdf.document(), &before, "the PDF session is untouched");
    // A conversion is not a PDF, and neither is a text file.
    assert_eq!(
        flowing.convert_pdf_to_flowing().unwrap_err().code,
        "not-a-pdf"
    );
    let text = Session::open(b"plain words", "notes.txt", SessionConfig::default()).unwrap();
    assert_eq!(text.convert_pdf_to_flowing().unwrap_err().code, "not-a-pdf");
}
