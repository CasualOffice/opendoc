//! A PDF-origin session in the main module, which links no PDF reader, no
//! importer, no OCR and no save planner (`docs/182` §17a).
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
//! `export_pdf_session`: the retained original when unchanged, an incremental
//! update when that is safe, the semantic writer otherwise. What changed, and
//! whether it can be kept, is the PDF module's `planSave` (the import's
//! digests and the hayro half live there), which the webapp registers with
//! [`set_pdf_save_planner`] when it loads that module; this module lays out
//! the overlay and writes the update. With none registered, the save is
//! semantic and says `preserve_fallback.pdf_module_unavailable`.
//!
//! Why the save calls back rather than this module re-creating the reader's
//! state: re-reading the original's revision needs a PDF parser of hostile
//! input, and the only one this repository trusts is hayro (`docs/182` §19.1
//! rejected a second). Keeping it, and the digests, in one lazily loaded
//! module is the point of the split.
//!
//! **A PDF opens as a flowing document** (`docs/182` §4), as Word's PDF Reflow
//! and Google Docs open one: the PDF module finishes the staged import with
//! `finishReflowed`, and [`open_pdf_reflow`] opens that flowing document,
//! keeping the original only so the editor can open the PDF again ("Keep
//! exact page layout", "Recognise text"). It saves as DOCX by default; saved
//! as PDF it is a new file. [`open_pdf_import`] opens the fixed layout, the
//! explicit choice, and the only one whose save goes back into the original.
//! "Convert to flowing document" from a fixed-layout session is the same
//! finish over [`WasmDocument::pdf_original`].
//!
//! [`bundled_face`] is the other direction: the PDF module draws the standard
//! 14 with the Liberation faces this module ships, asked for by name.

use std::cell::RefCell;
use std::sync::Arc;

use casual_doc_io::pdf_wire::SaveAnswer;
use casual_doc_io::{PdfSavePlanner, import_pdf_package, import_pdf_reflow_package};
use casual_doc_model::SnapshotLimits;
use casual_doc_model::v1::Document;
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

/// Opens the PDF module's flowing package (the default open, and "Convert to
/// flowing document") as a flowing document. `original`, when given, must be
/// the PDF the package was made from; it is kept so the editor can open the
/// PDF again (in its exact layout, or with OCR), never written back: saved as
/// PDF the document is the semantic writer, reported. Without it the session
/// is not PDF-origin. Its import report is the whole import's, with
/// `pdf.import.layout_inferred`. O(document).
///
/// # Errors
///
/// A malformed package, an original that is not the converted file, or a
/// document the snapshot importer refuses.
#[wasm_bindgen(js_name = openPdfReflow)]
pub fn open_pdf_reflow(package: &[u8], original: Option<Vec<u8>>) -> Result<WasmDocument, JsValue> {
    open_reflow(package, original).map_err(to_js)
}

/// [`open_pdf_reflow`] without `JsValue`.
pub(crate) fn open_reflow(
    package: &[u8],
    original: Option<Vec<u8>>,
) -> Result<WasmDocument, String> {
    let imported = import_pdf_reflow_package(package, original, SnapshotLimits::default())
        .map_err(|error| format!("convert PDF: {error}"))?;
    session_from_import(imported, MAX_WHOLE_LAYOUT_BLOCKS)
}

#[wasm_bindgen]
impl WasmDocument {
    /// Whether this session was opened from a PDF and keeps its original:
    /// what "Convert to flowing document" and "Recognise text in scanned
    /// pages" are enabled by. O(1).
    #[wasm_bindgen(getter, js_name = pdfOrigin)]
    #[must_use]
    pub fn pdf_origin(&self) -> bool {
        self.format_state
            .source
            .as_ref()
            .is_some_and(|source| casual_doc_io::pdf_retained_original(source).is_some())
    }

    /// How this PDF-origin session was opened: `"flowing"` (the default open)
    /// or `"fixed"` ("Keep exact page layout"); empty for a session that is
    /// not PDF-origin. O(1).
    #[wasm_bindgen(getter, js_name = pdfLayout)]
    #[must_use]
    pub fn pdf_layout(&self) -> String {
        self.format_state
            .source
            .as_ref()
            .filter(|source| casual_doc_io::pdf_retained_original(source).is_some())
            .and_then(casual_doc_io::pdf_layout)
            .map_or_else(String::new, |layout| layout.as_str().to_owned())
    }

    /// The PDF this session was opened from, byte for byte, for the PDF
    /// module to import again (to reflow it, or to read its scans).
    /// `undefined` when the session is not PDF-origin. O(original): a copy.
    #[wasm_bindgen(js_name = pdfOriginal)]
    #[must_use]
    pub fn pdf_original(&self) -> Option<Vec<u8>> {
        self.format_state
            .source
            .as_ref()
            .and_then(casual_doc_io::pdf_retained_original)
            .map(<[u8]>::to_vec)
    }

    /// What OCR did when the PDF this session came from was imported, as JSON
    /// (`rasterPages`, `hybridPages`, `regionsRecognized`, `regionsSkipped`,
    /// `wordsKept`, `provider`); empty for a session not opened from a PDF.
    /// O(1).
    #[wasm_bindgen(getter, js_name = pdfOcrSummaryJson)]
    #[must_use]
    pub fn pdf_ocr_summary_json(&self) -> String {
        let Some(ocr) = self
            .format_state
            .source
            .as_ref()
            .and_then(casual_doc_io::pdf_ocr_record)
        else {
            return String::new();
        };
        serde_json::json!({
            "rasterPages": ocr.raster_pages,
            "hybridPages": ocr.hybrid_pages,
            "regionsRecognized": ocr.regions_recognized,
            "regionsSkipped": ocr.regions_skipped,
            "wordsKept": ocr.words_kept,
            "provider": ocr.provider,
        })
        .to_string()
    }
}

#[wasm_bindgen]
extern "C" {
    /// The host's bridge to the PDF module: `planSave(original, baseline,
    /// documentJson)` returns that module's `SaveAnswer` bytes.
    pub type PdfSaveHost;

    #[wasm_bindgen(method, catch, js_name = planSave)]
    fn plan_save(
        this: &PdfSaveHost,
        original: &[u8],
        baseline: &[u8],
        document_json: &[u8],
    ) -> Result<Vec<u8>, JsValue>;
}

impl PdfSavePlanner for PdfSaveHost {
    fn plan(&self, original: &Arc<Vec<u8>>, baseline: &[u8], document: &Document) -> SaveAnswer {
        let failed = || {
            SaveAnswer::Refused(casual_doc_io::pdf_wire::Refusal {
                reason: "pdf_module_failed".to_owned(),
                page: None,
            })
        };
        // The snapshot serializer the editor already links; the PDF module
        // parses it with the same limits as any shared snapshot.
        let Ok(json) = document.to_json() else {
            return failed();
        };
        let Ok(bytes) = self.plan_save(original, baseline, &json) else {
            return failed();
        };
        SaveAnswer::decode(&bytes).unwrap_or_else(|_| failed())
    }
}

thread_local! {
    /// The registered planner. One per module instance: the webapp loads one
    /// PDF module and registers it once.
    static PLANNER: RefCell<Option<Box<dyn PdfSavePlanner>>> = const { RefCell::new(None) };
}

/// Registers the PDF module's half of the byte-preserving save.
#[wasm_bindgen(js_name = setPdfSavePlanner)]
pub fn set_pdf_save_planner(host: PdfSaveHost) {
    install_planner(Box::new(host));
}

/// [`set_pdf_save_planner`] for any planner (the native tests' in-process
/// one).
pub(crate) fn install_planner(planner: Box<dyn PdfSavePlanner>) {
    PLANNER.with(|slot| *slot.borrow_mut() = Some(planner));
}

/// Runs `f` with the registered planner, if any.
pub(crate) fn with_planner<R>(f: impl FnOnce(Option<&dyn PdfSavePlanner>) -> R) -> R {
    PLANNER.with(|slot| f(slot.borrow().as_deref()))
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
        install_planner(Box::new(casual_doc_io::InProcessSavePlanner));
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
        // through the registered planner, rather than regenerating the file.
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
        PLANNER.with(|slot| *slot.borrow_mut() = None);
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
    fn a_session_says_it_is_pdf_origin_and_hands_back_its_original() {
        let mut doc = imported(PROSE);
        assert!(doc.pdf_origin());
        assert_eq!(doc.pdf_original().as_deref(), Some(PROSE));
        assert!(doc.pdf_ocr_summary_json().contains("\"rasterPages\":0"));
        let docx = crate::open_document(&doc.export_as_inner(DOCX, "semantic").unwrap().bytes)
            .expect("the DOCX reopens");
        assert!(!docx.pdf_origin(), "a DOCX is not a PDF session");
        assert_eq!(docx.pdf_original(), None);
        assert_eq!(docx.pdf_ocr_summary_json(), "");
    }

    /// The default open (`docs/182` §4): the flowing package with its
    /// original is a PDF-origin flowing session that saves as DOCX by
    /// default and, saved as PDF, writes a new file rather than the original.
    /// Mutation: make the flowing branch of `export_pdf_session` fall through
    /// and the unchanged save is `PROSE` again.
    #[test]
    fn a_pdf_opens_as_a_flowing_session_that_never_writes_the_original_back() {
        install_planner(Box::new(casual_doc_io::InProcessSavePlanner));
        let mut job = casual_doc_wasm_pdf::begin(PROSE.to_vec(), None).expect("begin");
        let package = job.finish_reflowed_package().expect("reflow");
        let mut doc = open_reflow(&package, Some(PROSE.to_vec())).expect("open");
        assert_eq!(
            doc.source_format(),
            DOCX,
            "a flowing document saves as DOCX"
        );
        assert_eq!(doc.pdf_layout(), "flowing");
        assert!(doc.pdf_origin(), "the original is kept to open it again");
        assert_eq!(doc.pdf_original().as_deref(), Some(PROSE));
        assert!(
            doc.import_report_json()
                .contains("pdf.import.layout_inferred"),
            "{}",
            doc.import_report_json()
        );
        let pdf = doc
            .export_as_inner(PDF, "exact_if_unchanged")
            .expect("export");
        assert!(pdf.bytes != PROSE && !pdf.bytes.starts_with(PROSE));
        assert!(
            pdf.report_json
                .contains("pdf.export.preserve_fallback.flowing_layout"),
            "{}",
            pdf.report_json
        );
        // Saved as DOCX the PDF's envelope is not offered to the DOCX writer.
        let docx = doc
            .export_as_inner(DOCX, "preserve_when_safe")
            .expect("docx");
        assert!(
            !docx.report_json.contains("source_envelope"),
            "{}",
            docx.report_json
        );
        // Another file's package is refused.
        let mut other = PROSE.to_vec();
        other.push(b'\n');
        assert!(open_reflow(&package, Some(other)).is_err());
    }

    #[test]
    fn a_reflowed_pdf_opens_as_a_new_document_that_is_not_a_pdf_session() {
        let mut job = casual_doc_wasm_pdf::begin(PROSE.to_vec(), None).expect("begin");
        let package = job.finish_reflowed_package().expect("reflow");
        let doc = open_reflow(&package, None).expect("open");
        assert!(!doc.pdf_origin(), "a converted document keeps no original");
        assert_eq!(doc.pdf_original(), None);
        assert_eq!(doc.pdf_layout(), "");
        assert!(
            doc.import_report_json().contains("pdf.import."),
            "the reflow's inferences are its report: {}",
            doc.import_report_json()
        );
        // Flowing: paragraphs in the body, not frames on page anchors.
        assert!(
            surface_block_lists(&doc.document)[0].len() > 2,
            "the body holds the reflowed paragraphs"
        );
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
