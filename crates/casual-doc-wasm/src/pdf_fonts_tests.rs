// SPDX-License-Identifier: Apache-2.0

//! `exportAs("application.pdf")` lays the document out with the faces the host
//! registered on THIS document (`docs/109` HF-293).
//!
//! The browser has no system fonts. It registers Roboto by name
//! (`registerFonts`) and Noto CJK as a script fallback (`registerFallbackFont`)
//! on the session's shaper. The PDF adapter used to lay the document out again on a fresh shaper, so
//! none of those reached the file: a Korean document exported every syllable as
//! `.notdef`, and the report was empty.
//!
//! The browser's position is reproduced natively by swapping in
//! [`ParleyShaper::without_system_fonts`] before anything is registered, which
//! is the shaper the `wasm32` build has; otherwise an installed system CJK face
//! would answer for Hangul and the guards would pass on a developer machine for
//! the wrong reason.

use casual_doc_export::write_document;
use casual_doc_layout::shape::ParleyShaper;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Definitions, Document, FontName, FontRef, InlineNode, Paragraph,
    ParagraphProperties, Run, RunProperties,
};
use casual_doc_pdf::inspect;

use crate::{WasmDocument, open_document};

/// Noto Sans CJK KR, subset to [`KOREAN`] (`fixtures/fonts/README.md`).
const KOREAN_FACE: &[u8] = include_bytes!("../../../fixtures/fonts/NotoSansCJKkr-TestSubset.otf");
const KOREAN: &str = "대한민국 국세청 한글 문서 내보내기 검사";
/// The family the Korean face's `name` table declares.
const KOREAN_FAMILY: &str = "Noto Sans CJK KR";
const PDF: &str = casual_doc_io::formats::PDF;

/// A one-paragraph DOCX whose run asks for `family`, when given, for both its
/// Latin and its East Asian text.
fn docx(text: &str, family: Option<&str>) -> Vec<u8> {
    let id = |n: u64| NodeId::from_parts(n, 1).unwrap();
    let named = family.map(|name| {
        FontRef::Named(FontName {
            name: name.to_owned(),
        })
    });
    let properties = RunProperties {
        font_ref: named.clone(),
        font_ref_east_asia: named,
        ..RunProperties::default()
    };
    let document = Document::new(
        id(1),
        vec![BlockNode::Paragraph(Paragraph {
            id: id(2),
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Run(Run {
                id: id(3),
                properties: properties.into(),
                text: text.to_owned(),
            })],
        })],
        Definitions::default(),
    )
    .expect("a valid document");
    write_document(&document, &Default::default()).expect("write the package")
}

/// Opens `bytes` on the browser's shaper: bundled faces only.
fn open_in_browser(bytes: &[u8]) -> WasmDocument {
    let mut doc = open_document(bytes).expect("open");
    doc.shaper = ParleyShaper::without_system_fonts();
    doc.repaginate();
    doc
}

struct Exported {
    text: String,
    faces: Vec<String>,
    report: String,
}

impl Exported {
    fn embeds(&self, postscript: &str) -> bool {
        self.faces
            .iter()
            .any(|face| face.ends_with(&format!("+{postscript}")))
    }

    fn reports_missing_glyphs(&self) -> bool {
        self.report.contains("\"pdf.font.missing_glyphs\"")
    }

    fn words(&self) -> Vec<&str> {
        self.text.split_whitespace().collect()
    }
}

/// `exportAs("application.pdf", "semantic")`, read back as a reader would.
fn export_pdf(doc: &mut WasmDocument) -> Exported {
    let artifact = doc.export_as_inner(PDF, "semantic").expect("export a PDF");
    let pdf = inspect::parse(&artifact.bytes).expect("the export parses");
    Exported {
        text: pdf
            .pages()
            .iter()
            .map(|page| pdf.page_contents(page).plain_text())
            .collect::<Vec<_>>()
            .join("\n"),
        faces: pdf
            .embedded_fonts()
            .into_iter()
            .map(|font| font.base_font)
            .collect(),
        report: artifact.report_json,
    }
}

#[test]
fn a_fallback_face_registered_on_the_document_is_the_one_its_pdf_embeds() {
    let text = format!("{KOREAN} PDF");
    let mut doc = open_in_browser(text.as_bytes());

    // Before the host's Korean download lands: drawn as `.notdef`, and said so.
    let before = export_pdf(&mut doc);
    assert!(before.reports_missing_glyphs(), "{}", before.report);
    assert!(
        !before.embeds("NotoSansCJKkr-Regular"),
        "{:?}",
        before.faces
    );

    doc.register_fallback_font(KOREAN_FACE, vec!["Hani".to_owned(), "Hang".to_owned()]);
    let after = export_pdf(&mut doc);
    assert!(
        after.embeds("NotoSansCJKkr-Regular"),
        "the registered face is embedded: {:?}",
        after.faces
    );
    assert!(!after.reports_missing_glyphs(), "{}", after.report);
    assert_eq!(after.words(), text.split_whitespace().collect::<Vec<_>>());
}

#[test]
fn a_named_face_registered_with_register_fonts_is_the_one_its_pdf_embeds() {
    // `registerFonts` is the named-family seam (Roboto in the browser). The run
    // asks for the Korean family by name; no fallback is wired, so only the
    // named lookup can find it.
    let mut doc = open_in_browser(&docx(KOREAN, Some(KOREAN_FAMILY)));
    assert!(export_pdf(&mut doc).reports_missing_glyphs());

    doc.register_fonts_inner(KOREAN_FACE, &[u32::try_from(KOREAN_FACE.len()).unwrap()])
        .expect("register the named face");
    let after = export_pdf(&mut doc);
    assert!(after.embeds("NotoSansCJKkr-Regular"), "{:?}", after.faces);
    assert!(!after.reports_missing_glyphs(), "{}", after.report);
    assert_eq!(after.words(), KOREAN.split_whitespace().collect::<Vec<_>>());
}

#[test]
fn a_face_registered_on_one_document_never_reaches_anothers_pdf() {
    let mut a = open_in_browser(KOREAN.as_bytes());
    let mut b = open_in_browser(KOREAN.as_bytes());
    a.register_fallback_font(KOREAN_FACE, vec!["Hani".to_owned(), "Hang".to_owned()]);

    let a_pdf = export_pdf(&mut a);
    let b_pdf = export_pdf(&mut b);
    assert!(a_pdf.embeds("NotoSansCJKkr-Regular"));
    assert!(!a_pdf.reports_missing_glyphs(), "{}", a_pdf.report);
    assert!(!b_pdf.embeds("NotoSansCJKkr-Regular"), "{:?}", b_pdf.faces);
    assert!(b_pdf.reports_missing_glyphs(), "{}", b_pdf.report);
}

#[test]
fn exporting_leaves_the_session_and_a_repeat_export_unchanged() {
    // The export reads the live shaper; it must not register anything, so the
    // page on screen and a second export are what they were.
    let mut doc = open_in_browser(KOREAN.as_bytes());
    doc.register_fallback_font(KOREAN_FACE, vec!["Hani".to_owned(), "Hang".to_owned()]);
    let page_before = doc.render_page_inner(0, 48.0).expect("render").rgba;
    let first = doc.export_as_inner(PDF, "semantic").expect("export").bytes;
    let second = doc.export_as_inner(PDF, "semantic").expect("export").bytes;
    let page_after = doc.render_page_inner(0, 48.0).expect("render").rgba;
    assert!(first == second, "the PDF is deterministic");
    assert!(
        page_before == page_after,
        "exporting did not change the page"
    );
}
