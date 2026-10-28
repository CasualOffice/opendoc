//! A PDF-origin session in the main module, which links no PDF reader
//! (`docs/182` §17a).
//!
//! The import runs in the lazily loaded PDF module (`casual-doc-wasm-pdf`):
//! the webapp fetches it the first time a PDF is opened, drives its staged job
//! (`docs/182` §6) and hands this module two things — the job's package and
//! the original bytes it already holds. [`open_pdf_import`] turns them into a
//! session through the same `session_from_import` every other open uses. The
//! package names the original's length and hash, so a session is never built
//! over the wrong file.
//!
//! **Saving** a PDF-origin session as PDF is `casual-doc-io`'s
//! `export_pdf_session`: the retained original when unchanged or undone, an
//! incremental update when that is safe, the semantic writer otherwise. The
//! update's planner and writer are here (they need the layout engine); the
//! hayro half (open the last revision, read the edited pages, remove their
//! shows) is the PDF module's `readRevision`, which the webapp registers with
//! [`set_pdf_revision_reader`] when it loads that module. With none
//! registered, the save is semantic and says `preserve_fallback.
//! pdf_module_unavailable`.
//!
//! Why the save calls back rather than this module re-creating the reader's
//! state: re-reading the original's revision needs a PDF parser of hostile
//! input, and the only one this repository trusts is hayro (`docs/182` §19.1
//! rejected a second). Keeping it in one module is the point of the split.
//!
//! [`bundled_face`] is the other direction: the PDF module draws the standard
//! 14 with the Liberation faces this module ships, asked for by name.

use std::cell::RefCell;
use std::sync::Arc;

use casual_doc_io::pdf_wire::{RevisionParts, RevisionRequest, decode_revision};
use casual_doc_io::{PdfRevisionReader, import_pdf_package};
use casual_doc_model::SnapshotLimits;
use wasm_bindgen::prelude::*;

use crate::{MAX_WHOLE_LAYOUT_BLOCKS, WasmDocument, session_from_import, to_js};

/// Opens the PDF module's finished import as an editing session. `original`
/// is the file that was imported; it is retained for the exact save.
///
/// O(document): the snapshot parse and the layout, the cost of any open.
///
/// # Errors
///
/// A malformed package, an original that is not the imported file, or a
/// document the snapshot importer refuses.
#[wasm_bindgen(js_name = openPdfImport)]
pub fn open_pdf_import(package: &[u8], original: Vec<u8>) -> Result<WasmDocument, JsValue> {
    open(package, original).map_err(to_js)
}

/// [`open_pdf_import`] without `JsValue`.
pub(crate) fn open(package: &[u8], original: Vec<u8>) -> Result<WasmDocument, String> {
    let imported = import_pdf_package(package, original, SnapshotLimits::default())
        .map_err(|error| format!("open PDF: {error}"))?;
    session_from_import(imported, MAX_WHOLE_LAYOUT_BLOCKS)
}

#[wasm_bindgen]
extern "C" {
    /// The host's bridge to the PDF module: `readRevision(original, request)`
    /// returns that module's answer bytes.
    pub type PdfRevisionHost;

    #[wasm_bindgen(method, catch, js_name = readRevision)]
    fn read_revision(
        this: &PdfRevisionHost,
        original: &[u8],
        request: &[u8],
    ) -> Result<Vec<u8>, JsValue>;
}

impl PdfRevisionReader for PdfRevisionHost {
    fn read(
        &self,
        original: &Arc<Vec<u8>>,
        request: &RevisionRequest,
    ) -> Result<RevisionParts, casual_doc_io::pdf_wire::Refusal> {
        let failed = |reason: &str| casual_doc_io::pdf_wire::Refusal {
            reason: reason.to_owned(),
            page: None,
        };
        let bytes = self
            .read_revision(original, &request.encode())
            .map_err(|_| failed("pdf_module_failed"))?;
        decode_revision(&bytes).map_err(|_| failed("pdf_module_failed"))?
    }
}

thread_local! {
    /// The registered reader. One per module instance: the webapp loads one PDF
    /// module and registers it once.
    static READER: RefCell<Option<Box<dyn PdfRevisionReader>>> = const { RefCell::new(None) };
}

/// Registers the PDF module's half of the byte-preserving save.
#[wasm_bindgen(js_name = setPdfRevisionReader)]
pub fn set_pdf_revision_reader(host: PdfRevisionHost) {
    install_reader(Box::new(host));
}

/// [`set_pdf_revision_reader`] for any reader (the native tests' in-process
/// one).
pub(crate) fn install_reader(reader: Box<dyn PdfRevisionReader>) {
    READER.with(|slot| *slot.borrow_mut() = Some(reader));
}

/// Runs `f` with the registered reader, if any.
pub(crate) fn with_reader<R>(f: impl FnOnce(Option<&dyn PdfRevisionReader>) -> R) -> R {
    READER.with(|slot| f(slot.borrow().as_deref()))
}

/// The bytes of a face this module ships, for the PDF module's standard-14
/// resolver (`casual-doc-pdf-read` `provision`): `Liberation Sans`, `Serif` or
/// `Mono` in any style. `None` for anything else. O(face bytes): a copy.
///
/// Read through the layout crate's own functions (`fonts::family_name`,
/// `fonts::face_bytes`), never by naming a `fonts` constant here: a `const`
/// holding `include_bytes!` (or a table of them, `FAMILIES`) is emitted again
/// in every crate that names it. Naming the twelve `LIBERATION_*` constants
/// linked a second 4.35 MB copy; naming `FAMILIES` a second copy of every face
/// (measured, `docs/182` §17a).
#[wasm_bindgen(js_name = bundledFace)]
#[must_use]
pub fn bundled_face(family: &str, bold: bool, italic: bool) -> Option<Vec<u8>> {
    use casual_doc_layout::fonts;
    use casual_doc_layout::text::FontId;
    if !matches!(
        family,
        "Liberation Sans" | "Liberation Serif" | "Liberation Mono"
    ) {
        return None;
    }
    // A family's block is four ids from its regular face; the bundle is a few
    // dozen ids, so this is O(1).
    let base = (0..256).find(|id| fonts::family_name(FontId(*id)) == Some(family))?;
    let id = FontId(base + u32::from(bold) + (u32::from(italic) << 1));
    let bytes = fonts::face_bytes(id);
    // `face_bytes` answers the default face for an id it does not hold; a
    // viewer build's placeholder is empty.
    (fonts::family_name(id) == Some(family) && !bytes.is_empty()).then(|| bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_edit::surface_block_lists;
    use casual_doc_model::v1::BlockNode;

    const PROSE: &[u8] = include_bytes!("../../../fixtures/pdf/synthetic-prose.pdf");
    const PDF: &str = casual_doc_io::formats::PDF;
    const DOCX: &str = casual_doc_io::formats::DOCX;

    /// A real import through the PDF module's own code, then this module's open.
    fn imported(bytes: &[u8]) -> WasmDocument {
        let mut job = casual_doc_wasm_pdf::begin(bytes.to_vec(), None).expect("begin");
        while job.step(1) == "pending" {}
        let package = job.finish_package().expect("finish");
        open(&package, bytes.to_vec()).expect("open")
    }

    /// The first paragraph inside the first text box: a PDF frame.
    fn first_frame_paragraph(doc: &WasmDocument) -> String {
        let lists = surface_block_lists(&doc.document);
        lists[1..]
            .iter()
            .flat_map(|list| list.iter())
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
                _ => None,
            })
            .expect("a PDF page imports as frames holding paragraphs")
    }

    #[test]
    fn the_pdf_modules_package_opens_into_a_session() {
        let doc = imported(PROSE);
        assert_eq!(doc.source_format(), PDF);
        assert_eq!(doc.page_count(), 2, "one PDF page is one editor page");
        assert!(
            doc.import_report_json()
                .contains("pdf.import.font_substituted"),
            "the import report reaches the session"
        );
        assert!(first_frame_paragraph(&doc).len() > 1);
    }

    #[test]
    fn a_package_is_refused_over_a_different_original() {
        let mut job = casual_doc_wasm_pdf::begin(PROSE.to_vec(), None).expect("begin");
        let package = job.finish_package().expect("finish");
        let mut other = PROSE.to_vec();
        other.push(b'\n');
        let error = open(&package, other).unwrap_err();
        assert!(error.contains("different file"), "{error}");
    }

    #[test]
    fn the_synchronous_open_still_refuses_a_pdf() {
        let error = crate::open_document(PROSE).unwrap_err();
        assert!(error.contains("import document"), "{error}");
    }

    #[test]
    fn an_unchanged_pdf_saves_back_byte_for_byte_and_an_edited_one_is_an_incremental_update() {
        install_reader(Box::new(casual_doc_io::InProcessRevisionReader));
        let mut doc = imported(PROSE);
        let exact = doc
            .export_as_inner(PDF, "exact_if_unchanged")
            .expect("export");
        assert!(
            exact.bytes == PROSE,
            "an unchanged session is the original file ({} bytes written, {} read)",
            exact.bytes.len(),
            PROSE.len()
        );
        let semantic = doc.export_as_inner(PDF, "semantic").expect("export");
        assert_ne!(semantic.bytes, PROSE);

        // Edit a frame: the save appends an update to the original (P4)
        // through the registered reader, rather than regenerating the file.
        let target = first_frame_paragraph(&doc);
        doc.insert_text(&target, 0, "Edited ".to_owned())
            .expect("type into a frame");
        let edited = doc
            .export_as_inner(PDF, "exact_if_unchanged")
            .expect("export");
        assert!(
            edited.bytes.starts_with(PROSE) && edited.bytes.len() > PROSE.len(),
            "an edited session is the original plus an incremental update: {}",
            edited.report_json
        );
        assert!(
            edited.report_json.contains("pdf.export.incremental_update"),
            "{}",
            edited.report_json
        );
        // The edit is drawn by the update's overlay form (`docs/182` §19.2).
        // The writer's inspector reads page streams, not form XObjects, so the
        // text itself is checked by `casual-doc-io`'s `pdf_preserve` guards,
        // which render the saved file with hayro.
        let update = &edited.bytes[PROSE.len()..];
        assert!(
            update.windows(11).any(|w| w == b"OpenDocEdit"),
            "the update carries the overlay form"
        );
    }

    #[test]
    fn with_no_pdf_module_an_edited_save_is_semantic_and_says_why() {
        READER.with(|slot| *slot.borrow_mut() = None);
        let mut doc = imported(PROSE);
        let target = first_frame_paragraph(&doc);
        doc.insert_text(&target, 0, "Edited ".to_owned())
            .expect("type into a frame");
        let edited = doc
            .export_as_inner(PDF, "exact_if_unchanged")
            .expect("export");
        assert!(!edited.bytes.starts_with(PROSE));
        assert!(
            edited
                .report_json
                .contains("pdf.export.preserve_fallback.pdf_module_unavailable"),
            "{}",
            edited.report_json
        );
    }

    #[test]
    fn an_imported_pdf_saves_as_docx_with_its_frames() {
        let mut doc = imported(PROSE);
        let target = first_frame_paragraph(&doc);
        doc.insert_text(&target, 0, "Edited ".to_owned())
            .expect("type into a frame");
        let docx = doc.export_as_inner(DOCX, "semantic").expect("export docx");
        let reopened = crate::open_document(&docx.bytes).expect("the DOCX reopens");
        assert!(surface_block_lists(&reopened.document).len() > 1);
    }

    #[test]
    fn the_standard_fourteen_faces_are_shipped_here() {
        for family in ["Liberation Sans", "Liberation Serif", "Liberation Mono"] {
            for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
                assert!(bundled_face(family, bold, italic).is_some(), "{family}");
            }
        }
        assert!(bundled_face("Foxit Sans", false, false).is_none());
    }
}
