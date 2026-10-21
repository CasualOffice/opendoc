// SPDX-License-Identifier: Apache-2.0

//! PDF export embeds fonts registered on the browser session.

use casual_doc_layout::shape::ParleyShaper;
use casual_doc_pdf::inspect;

use crate::{WasmDocument, open_document};

const CJK_FACE: &[u8] = include_bytes!("../../../fixtures/fonts/NotoSansCJK-TestSubset.otf");
const CJK_POSTSCRIPT: &str = "NotoSansCJKsc-Regular";
const TEXT: &str = "한글 PDF";
const PDF: &str = casual_doc_io::formats::PDF;

fn open_in_browser(bytes: &[u8]) -> WasmDocument {
    let mut doc = open_document(bytes).expect("open");
    doc.shaper = ParleyShaper::without_system_fonts();
    doc.repaginate();
    doc
}

fn pdf_embeds_registered_face(doc: &mut WasmDocument) -> bool {
    let bytes = doc
        .export_as_inner(PDF, "semantic")
        .expect("export a PDF")
        .bytes;
    inspect::parse(&bytes)
        .expect("the export parses")
        .embedded_fonts()
        .iter()
        .any(|font| {
            font.base_font.ends_with(&format!("+{CJK_POSTSCRIPT}"))
                && font.program_bytes.is_some_and(|bytes| bytes > 0)
        })
}

#[test]
fn a_fallback_face_registered_on_the_session_is_embedded_in_its_pdf() {
    let mut doc = open_in_browser(TEXT.as_bytes());
    assert!(
        !pdf_embeds_registered_face(&mut doc),
        "nothing registered yet"
    );
    doc.register_fallback_font(CJK_FACE, vec!["Hani".to_owned(), "Hang".to_owned()]);
    assert!(
        pdf_embeds_registered_face(&mut doc),
        "the registered fallback is embedded"
    );
}
