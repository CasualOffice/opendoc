//! The staged PDF import, as the editor drives it (`docs/182` §6, P2).
//!
//! `open` is synchronous: one call parses, builds and lays out the document, so
//! a 300-page PDF would hold the main thread for the whole import. A PDF is
//! therefore opened through this binding instead, the **resumable, budgeted
//! job** pattern `casual-doc-diff`'s job already uses:
//!
//! 1. [`begin_pdf_import`] admits the file (size, header, encryption refused,
//!    the bounded-inflate pre-pass, the active-content inventory). O(document)
//!    in bytes, done once, and the only step that cannot be split.
//! 2. [`WasmPdfImport::step`] imports at most `budget` pages and returns.
//!    O(budget). The webapp calls it from `requestIdleCallback`-sized slices
//!    (`webapp/src/pdf_import_scheduler.mjs`), so no task exceeds the
//!    `docs/116` budget and Cancel can land between two pages.
//! 3. [`WasmPdfImport::cancel`] drops the partial build. Nothing was published.
//! 4. [`WasmPdfImport::finish`] validates the model and turns it into an
//!    editing session through the same `session_from_import` every other open
//!    uses. It is **atomic** (PDFR-I3): a cancelled or failed job returns no
//!    document, so the host replaces the open document only on success.
//!
//! The binding exists in every build so the webapp's import of it always
//! links; without the `pdf-import` feature [`begin_pdf_import`] refuses with a
//! sentence instead of a missing export.
//!
//! What a host may not do: open a PDF through the synchronous `open`. The
//! built-in registry stays export-only for PDF (`casual-doc-io`'s
//! `the_builtin_registry_still_never_imports_pdf`), so a host that forgets the
//! staged path gets an "unsupported format" refusal, never a frozen tab.

use wasm_bindgen::prelude::*;

use crate::{WasmDocument, to_js};
use casual_doc_io::SourceEnvelope;

#[cfg(feature = "pdf-import")]
use crate::{MAX_WHOLE_LAYOUT_BLOCKS, session_from_import};
#[cfg(feature = "pdf-import")]
use casual_doc_io::{PdfImportAdapter, PdfImportProgress, PdfStagedImport};

/// Where a staged import is. (Without the feature no job ever starts, so only
/// the type exists.)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(feature = "pdf-import"), allow(dead_code))]
enum State {
    Running,
    Done,
    Cancelled,
    Failed,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "pending",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }
}

/// A PDF import in progress. See the module documentation for the protocol.
#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmPdfImport {
    #[cfg(feature = "pdf-import")]
    job: Option<PdfStagedImport>,
    pages_done: usize,
    page_count: usize,
    state: State,
}

/// Admits `bytes` as a PDF and returns the job, or the reader's refusal.
///
/// O(document) in bytes (admission); no page is built yet.
#[wasm_bindgen(js_name = beginPdfImport)]
pub fn begin_pdf_import(bytes: &[u8]) -> Result<WasmPdfImport, JsValue> {
    begin(bytes).map_err(to_js)
}

/// [`begin_pdf_import`] without `JsValue`, so native tests run the same path.
#[cfg(feature = "pdf-import")]
pub(crate) fn begin(bytes: &[u8]) -> Result<WasmPdfImport, String> {
    // `retain_source`: the original bytes stay with the session, so an
    // unchanged PDF saves back exactly (`docs/182` §7).
    let job = PdfImportAdapter::default()
        .begin(bytes, true)
        .map_err(|error| error.to_string())?;
    Ok(WasmPdfImport {
        page_count: job.page_count(),
        job: Some(job),
        pages_done: 0,
        state: State::Running,
    })
}

/// Without the feature: a sentence, not a missing export.
#[cfg(not(feature = "pdf-import"))]
pub(crate) fn begin(_bytes: &[u8]) -> Result<WasmPdfImport, String> {
    Err("This build of the editor cannot open PDF files.".to_owned())
}

#[wasm_bindgen]
impl WasmPdfImport {
    /// Pages in the PDF, known from admission. O(1).
    #[wasm_bindgen(getter, js_name = pageCount)]
    #[must_use]
    pub fn page_count(&self) -> u32 {
        u32::try_from(self.page_count).unwrap_or(u32::MAX)
    }

    /// Pages imported so far. O(1).
    #[wasm_bindgen(getter, js_name = pagesDone)]
    #[must_use]
    pub fn pages_done(&self) -> u32 {
        u32::try_from(self.pages_done).unwrap_or(u32::MAX)
    }

    /// `pending`, `done`, `cancelled` or `failed`. O(1).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn status(&self) -> String {
        self.state.as_str().to_owned()
    }

    /// Imports at most `budget` pages (at least one) and returns the status.
    /// O(budget pages), each bounded by the reader's per-page limits.
    pub fn step(&mut self, budget: u32) -> String {
        self.step_pages(usize::try_from(budget).unwrap_or(usize::MAX));
        self.status()
    }

    /// Stops the job and drops the partial build. Idempotent.
    pub fn cancel(&mut self) {
        #[cfg(feature = "pdf-import")]
        if let Some(job) = self.job.as_mut() {
            job.cancel();
        }
        if self.state == State::Running || self.state == State::Done {
            self.state = State::Cancelled;
        }
    }

    /// The editing session, atomically: runs any pages left, validates the
    /// model and lays it out. A cancelled or failed job is an error and
    /// produces nothing.
    ///
    /// O(document): the layout is the open's own cost.
    pub fn finish(self) -> Result<WasmDocument, JsValue> {
        self.finish_session().map_err(to_js)
    }
}

impl WasmPdfImport {
    #[cfg(feature = "pdf-import")]
    fn step_pages(&mut self, budget: usize) {
        if self.state != State::Running {
            return;
        }
        let Some(job) = self.job.as_mut() else {
            return;
        };
        match job.step(budget) {
            PdfImportProgress::Pending { page, .. } => self.pages_done = page,
            PdfImportProgress::Done => {
                self.pages_done = self.page_count;
                self.state = State::Done;
            }
            PdfImportProgress::Cancelled => self.state = State::Cancelled,
            PdfImportProgress::Failed => self.state = State::Failed,
        }
    }

    #[cfg(not(feature = "pdf-import"))]
    fn step_pages(&mut self, _budget: usize) {}

    /// [`Self::finish`] without `JsValue`.
    #[cfg(feature = "pdf-import")]
    pub(crate) fn finish_session(mut self) -> Result<WasmDocument, String> {
        if self.state == State::Cancelled {
            return Err("the import was cancelled".to_owned());
        }
        let job = self
            .job
            .take()
            .ok_or_else(|| "the import has no job".to_owned())?;
        let imported = job.finish().map_err(|error| error.to_string())?;
        session_from_import(imported, MAX_WHOLE_LAYOUT_BLOCKS)
    }

    #[cfg(not(feature = "pdf-import"))]
    pub(crate) fn finish_session(self) -> Result<WasmDocument, String> {
        Err("This build of the editor cannot open PDF files.".to_owned())
    }
}

/// The original PDF bytes a session retained, when it was opened from a PDF
/// through this binding. `None` for every other source.
#[cfg(feature = "pdf-import")]
pub(crate) fn retained_original(source: Option<&SourceEnvelope>) -> Option<Vec<u8>> {
    PdfImportAdapter::default()
        .retained_source_bytes(source?)
        .map(<[u8]>::to_vec)
}

/// Without the feature no session can have been opened from a PDF.
#[cfg(not(feature = "pdf-import"))]
pub(crate) fn retained_original(_source: Option<&SourceEnvelope>) -> Option<Vec<u8>> {
    None
}

#[cfg(feature = "pdf-import")]
#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_edit::surface_block_lists;
    use casual_doc_model::v1::BlockNode;

    const PROSE: &[u8] = include_bytes!("../../../fixtures/pdf/synthetic-prose.pdf");
    const ENCRYPTED: &[u8] = include_bytes!("../../../fixtures/pdf/hostile/hostile-encrypted.pdf");
    const PDF: &str = casual_doc_io::formats::PDF;
    const DOCX: &str = casual_doc_io::formats::DOCX;

    fn imported(bytes: &[u8]) -> WasmDocument {
        let mut job = begin(bytes).expect("begin");
        while job.step(1) == "pending" {}
        job.finish_session().expect("finish")
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
    fn a_staged_import_steps_page_by_page_and_finishes_into_a_session() {
        let mut job = begin(PROSE).expect("begin");
        // Known at admission, before any page is built: the progress bar's total.
        assert_eq!((job.page_count(), job.pages_done()), (2, 0));
        assert_eq!(job.step(1), "pending");
        assert_eq!(job.pages_done(), 1);
        assert_eq!(job.step(1), "done");
        assert_eq!(job.pages_done(), 2);
        let doc = job.finish_session().expect("finish");
        assert_eq!(doc.source_format(), PDF);
        assert_eq!(doc.page_count(), 2, "one PDF page is one editor page");
        assert!(
            doc.import_report_json()
                .contains("pdf.import.font_substituted"),
            "the import report reaches the session"
        );
        // Frames hold the PDF's text.
        assert!(first_frame_paragraph(&doc).len() > 1);
    }

    #[test]
    fn a_cancelled_import_produces_no_document() {
        let mut job = begin(PROSE).expect("begin");
        assert_eq!(job.step(1), "pending");
        job.cancel();
        assert_eq!(job.status(), "cancelled");
        // A step after Cancel does nothing.
        assert_eq!(job.step(1), "cancelled");
        assert_eq!(job.pages_done(), 1);
        assert!(job.finish_session().is_err());
    }

    #[test]
    fn an_encrypted_pdf_is_refused_at_begin_with_its_finding_id() {
        let error = begin(ENCRYPTED).unwrap_err();
        assert!(
            error.contains("pdf.import.encrypted_unsupported"),
            "{error}"
        );
    }

    #[test]
    fn the_synchronous_open_still_refuses_a_pdf() {
        // The staged path is the only way in: a host calling `open` on a PDF gets
        // a refusal, not a main-thread import (`docs/182` §6).
        let error = crate::open_document(PROSE).unwrap_err();
        assert!(error.contains("import document"), "{error}");
    }

    #[test]
    fn an_unchanged_pdf_saves_back_byte_for_byte_and_an_edited_one_is_regenerated() {
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
        // Asked for a semantic write, the writer renders the model even unchanged.
        let semantic = doc.export_as_inner(PDF, "semantic").expect("export");
        assert_ne!(semantic.bytes, PROSE);
        assert!(semantic.bytes.starts_with(b"%PDF-"));

        // Edit a frame: the exact path no longer applies, and the text layer of
        // the regenerated PDF holds the edit.
        let target = first_frame_paragraph(&doc);
        doc.insert_text(&target, 0, "Edited ".to_owned())
            .expect("type into a frame");
        let edited = doc
            .export_as_inner(PDF, "exact_if_unchanged")
            .expect("export");
        assert_ne!(edited.bytes, PROSE, "an edited session is regenerated");
        let pdf = casual_doc_pdf::inspect::parse(&edited.bytes).expect("the export parses");
        let text: String = pdf
            .pages()
            .into_iter()
            .map(|page| pdf.page_contents(page).plain_text())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("Edited"),
            "the edit is in the text layer: {text}"
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
        let frames = surface_block_lists(&reopened.document).len();
        assert!(frames > 1, "the frames reopen as text boxes");
        let edited = surface_block_lists(&reopened.document)[1..]
            .iter()
            .flat_map(|list| list.iter())
            .any(|block| match block {
                BlockNode::Paragraph(paragraph) => paragraph_text(paragraph).starts_with("Edited "),
                _ => false,
            });
        assert!(edited, "the edited frame keeps its edit through DOCX");
    }

    fn paragraph_text(paragraph: &casual_doc_model::v1::Paragraph) -> String {
        paragraph
            .inlines
            .iter()
            .filter_map(|inline| match inline {
                casual_doc_model::v1::InlineNode::Run(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect()
    }
}
