// SPDX-License-Identifier: Apache-2.0

//! The editor's PDF module (`docs/182` §17a): loaded by the webapp the first
//! time a PDF is opened, never by a DOCX open.
//!
//! The main editor module (`casual-doc-wasm`) links no hayro, no reader and no
//! importer. This module is all three plus OCR, and it talks to the main
//! module only in `casual-doc-pdf-wire` bytes, through the host's JavaScript:
//!
//! 1. **Import.** [`begin_pdf_import`] admits the file; [`PdfImportJob::step`]
//!    imports a page budget at a time (`docs/182` §6); [`PdfImportJob::cancel`]
//!    drops the partial build; [`PdfImportJob::finish`] returns an
//!    `ImportPackage`: the normalized JSON snapshot, media, findings, the
//!    source map and OCR's summary. The main module's `openPdfImport` turns it
//!    and the original bytes into a session. Nothing is published before the
//!    package exists (PDFR-I3).
//! 2. **The hayro half of the byte-preserving save.** [`read_revision`]
//!    answers the main module's `RevisionRequest`: it opens the original's last
//!    revision, reads the edited pages' parts and removes their shows. The main
//!    module plans the save and writes the update; see `casual-doc-io`'s
//!    `pdf_preserve`. The boundary is drawn there because both halves already
//!    existed: the planner needs the layout engine (only the main module has
//!    it), the reader needs hayro (only this module has it).
//! 3. **Provisioning.** hayro's bundled standard-14 faces and CMaps are not in
//!    this module (`casual-doc-pdf-read`'s `provision`). [`set_face_host`]
//!    hands it a host object whose `face(family, bold, italic)` returns the
//!    bytes of a face the main module ships (Liberation); [`provide_cmap`]
//!    registers a predefined CMap the host fetched.
//! 4. **OCR.** [`PdfOcrModels`] carries model bytes the host obtained (never
//!    bundled, never fetched here); [`PdfOcrEngine::load`] builds the PP-OCRv5
//!    provider once; [`begin_pdf_import_with_ocr`] runs the import with it.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::sync::Arc;

use casual_doc_ocr::{CancelToken, PaddleOcr, RecognitionPack};
use casual_doc_pdf_import::{
    ImportJob, OcrOptions, PdfImportError, PdfReadError, PdfReadLimits, Progress,
};
use casual_doc_pdf_read::provision;
use casual_doc_pdf_wire as wire;
use wasm_bindgen::prelude::*;

/// This module's build, for the host's diagnostics and the size guard.
#[wasm_bindgen(js_name = pdfModuleVersion)]
#[must_use]
pub fn pdf_module_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

/// A PDF import in progress.
#[wasm_bindgen]
pub struct PdfImportJob {
    job: Option<ImportJob>,
    cancel: CancelToken,
    pages_done: usize,
    page_count: usize,
    state: State,
    needed_cmaps: Vec<String>,
    ocr_summary: Option<wire::OcrSummary>,
}

impl std::fmt::Debug for PdfImportJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfImportJob")
            .field("pages_done", &self.pages_done)
            .field("page_count", &self.page_count)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// Admits `bytes` as a PDF and returns the job, or the reader's refusal.
/// O(document) in bytes (admission).
///
/// # Errors
///
/// The reader's refusal (encrypted, a limit, not a PDF), as a sentence.
#[wasm_bindgen(js_name = beginPdfImport)]
pub fn begin_pdf_import(bytes: Vec<u8>) -> Result<PdfImportJob, JsValue> {
    begin(bytes, None).map_err(|e| JsValue::from_str(&e))
}

/// [`begin_pdf_import`] with OCR: scanned pages and unmapped runs are read by
/// `engine` (`docs/182` P5).
///
/// # Errors
///
/// As [`begin_pdf_import`].
#[wasm_bindgen(js_name = beginPdfImportWithOcr)]
pub fn begin_pdf_import_with_ocr(
    bytes: Vec<u8>,
    engine: &PdfOcrEngine,
) -> Result<PdfImportJob, JsValue> {
    begin(bytes, Some(engine.provider.clone())).map_err(|e| JsValue::from_str(&e))
}

/// The native path of both `begin`s.
///
/// # Errors
///
/// The reader's refusal.
pub fn begin(bytes: Vec<u8>, ocr: Option<Arc<PaddleOcr>>) -> Result<PdfImportJob, String> {
    let cancel = CancelToken::new();
    let options = OcrOptions {
        provider: ocr.map(|p| p as Arc<dyn casual_doc_ocr::OcrProvider>),
        interrupt: Arc::new(cancel.clone()),
        ..OcrOptions::default()
    };
    let job = ImportJob::begin_with_ocr(bytes, PdfReadLimits::default(), options)
        .map_err(|error| error.to_string())?;
    Ok(PdfImportJob {
        page_count: job.page_count(),
        job: Some(job),
        cancel,
        pages_done: 0,
        state: State::Running,
        needed_cmaps: Vec::new(),
        ocr_summary: None,
    })
}

#[wasm_bindgen]
impl PdfImportJob {
    /// Pages in the PDF. O(1).
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

    /// The predefined CMaps the file names that no source has: a host fetches
    /// these ([`provide_cmap`]) before the first step, or after a finish that
    /// failed for want of one ([`Self::needed_cmaps`]). O(fonts).
    #[wasm_bindgen(getter, js_name = missingCMaps)]
    #[must_use]
    pub fn missing_cmaps(&self) -> Vec<String> {
        self.job
            .as_ref()
            .map(ImportJob::missing_cmaps)
            .unwrap_or_default()
    }

    /// After a finish that failed because a page needed CMaps nobody had:
    /// their names, so the host can fetch them and import again. O(1).
    #[wasm_bindgen(getter, js_name = neededCMaps)]
    #[must_use]
    pub fn needed_cmaps(&self) -> Vec<String> {
        self.needed_cmaps.clone()
    }

    /// After a finish: what OCR did, as JSON (`rasterPages`, `hybridPages`,
    /// `regionsRecognized`, `regionsSkipped`, `wordsKept`, `provider`), for the
    /// summary's "Recognise text" offer. Empty before. O(1).
    #[wasm_bindgen(getter, js_name = ocrSummaryJson)]
    #[must_use]
    pub fn ocr_summary_json(&self) -> String {
        let Some(ocr) = &self.ocr_summary else {
            return String::new();
        };
        // Provider ids are crate constants (`pp-ocrv5`), never document text.
        let provider = ocr.provider.as_ref().map_or_else(
            || "null".to_owned(),
            |p| format!("\"{}\"", p.replace(['"', '\\'], "")),
        );
        format!(
            "{{\"rasterPages\":{},\"hybridPages\":{},\"regionsRecognized\":{},\"regionsSkipped\":{},\"wordsKept\":{},\"provider\":{}}}",
            ocr.raster_pages,
            ocr.hybrid_pages,
            ocr.regions_recognized,
            ocr.regions_skipped,
            ocr.words_kept,
            provider
        )
    }

    /// Imports at most `budget` pages (at least one). O(budget pages).
    pub fn step(&mut self, budget: u32) -> String {
        if self.state == State::Running
            && let Some(job) = self.job.as_mut()
        {
            match job.step(usize::try_from(budget).unwrap_or(usize::MAX)) {
                Progress::Pending { page, .. } => self.pages_done = page,
                Progress::Done => {
                    self.pages_done = self.page_count;
                    self.state = State::Done;
                }
                Progress::Cancelled => self.state = State::Cancelled,
                Progress::Failed => self.state = State::Failed,
            }
        }
        self.status()
    }

    /// Stops the job and drops the partial build. Idempotent.
    pub fn cancel(&mut self) {
        self.cancel.cancel();
        if let Some(job) = self.job.as_mut() {
            job.cancel();
        }
        if matches!(self.state, State::Running | State::Done) {
            self.state = State::Cancelled;
        }
    }

    /// The finished import as a package for the main module's
    /// `openPdfImport`. Runs any pages left. O(document).
    ///
    /// # Errors
    ///
    /// Cancellation, a page that failed (a CMap nobody had sets
    /// [`Self::needed_cmaps`]), or an invalid model.
    pub fn finish(&mut self) -> Result<Vec<u8>, JsValue> {
        self.finish_package().map_err(|e| JsValue::from_str(&e))
    }
}

impl PdfImportJob {
    /// [`Self::finish`] without `JsValue`.
    ///
    /// # Errors
    ///
    /// As [`Self::finish`].
    pub fn finish_package(&mut self) -> Result<Vec<u8>, String> {
        if self.state == State::Cancelled {
            return Err("the import was cancelled".to_owned());
        }
        let job = self
            .job
            .take()
            .ok_or_else(|| "the import has no job".to_owned())?;
        match job.finish() {
            Ok(imported) => {
                self.state = State::Done;
                let package = casual_doc_pdf_import::wire::package(&imported)?;
                self.ocr_summary = Some(package.ocr.clone());
                Ok(package.encode())
            }
            Err(error) => {
                self.state = State::Failed;
                if let PdfImportError::Read(PdfReadError::CMapUnavailable(names)) = &error {
                    self.needed_cmaps.clone_from(names);
                }
                Err(error.to_string())
            }
        }
    }
}

/// Registers a predefined CMap's file the host fetched (`UniJIS-UCS2-H`, …).
/// Kept for the module's lifetime; a name that is not predefined is ignored.
#[wasm_bindgen(js_name = provideCMap)]
pub fn provide_cmap(name: &str, bytes: Vec<u8>) {
    provision::provide_cmap(name, bytes);
}

#[wasm_bindgen]
extern "C" {
    /// The host's face source: `face(family, bold, italic)` returns the bytes
    /// of a face the main module ships, or `undefined`.
    pub type PdfFaceHost;

    #[wasm_bindgen(method, catch)]
    fn face(
        this: &PdfFaceHost,
        family: &str,
        bold: bool,
        italic: bool,
    ) -> Result<Option<Vec<u8>>, JsValue>;
}

thread_local! {
    static FACE_HOST: RefCell<Option<PdfFaceHost>> = const { RefCell::new(None) };
}

/// Installs the host's face source for the standard 14 (`provision`). Each
/// face is asked for once and kept.
#[wasm_bindgen(js_name = setFaceHost)]
pub fn set_face_host(host: PdfFaceHost) {
    FACE_HOST.with(|slot| *slot.borrow_mut() = Some(host));
    provision::set_face_source(Arc::new(|request: &provision::FaceRequest| {
        let bytes = FACE_HOST.with(|slot| {
            let slot = slot.borrow();
            slot.as_ref()?
                .face(request.family, request.bold, request.italic)
                .ok()
                .flatten()
        })?;
        Some(Arc::new(bytes) as provision::FaceBytes)
    }));
}

/// The hayro half of the byte-preserving save: `request` is the main module's
/// encoded `RevisionRequest`; the answer is `encode_revision`'s bytes, parts
/// or a refusal. O(original) plus the requested pages' content.
#[wasm_bindgen(js_name = readRevision)]
#[must_use]
pub fn read_revision(original: Vec<u8>, request: &[u8]) -> Vec<u8> {
    let answer = match wire::RevisionRequest::decode(request) {
        Ok(request) => casual_doc_pdf_read::revision::read_for_save(Arc::new(original), &request),
        Err(_) => Err(wire::Refusal {
            reason: "request_malformed".to_owned(),
            page: None,
        }),
    };
    wire::encode_revision(&answer)
}

/// OCR model bytes the host obtained (a cache, its own fetch): a detection
/// model and one or more recognition packs (`docs/182` §20).
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct PdfOcrModels {
    detection: Vec<u8>,
    packs: Vec<RecognitionPack>,
}

#[wasm_bindgen]
impl PdfOcrModels {
    /// Starts a model set with the PP-OCRv5 detection model's ONNX bytes.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new(detection: Vec<u8>) -> Self {
        Self {
            detection,
            packs: Vec::new(),
        }
    }

    /// Adds a recognition pack: its name, ONNX bytes and dictionary text.
    #[wasm_bindgen(js_name = addPack)]
    pub fn add_pack(&mut self, name: String, model: Vec<u8>, dictionary: String) {
        self.packs.push(RecognitionPack {
            name,
            model,
            dictionary,
        });
    }
}

/// A loaded PP-OCRv5 provider, built once and reused by every import.
#[wasm_bindgen]
#[derive(Debug)]
pub struct PdfOcrEngine {
    provider: Arc<PaddleOcr>,
}

#[wasm_bindgen]
impl PdfOcrEngine {
    /// Loads the models. O(model bytes).
    ///
    /// # Errors
    ///
    /// A model that does not load, or no pack.
    pub fn load(models: PdfOcrModels) -> Result<PdfOcrEngine, JsValue> {
        PaddleOcr::from_model_bytes(models.detection, models.packs)
            .map(|provider| Self {
                provider: Arc::new(provider),
            })
            .map_err(|error| JsValue::from_str(&error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROSE: &[u8] = include_bytes!("../../../fixtures/pdf/synthetic-prose.pdf");

    #[test]
    fn a_staged_import_finishes_into_a_package_that_names_its_original() {
        let mut job = begin(PROSE.to_vec(), None).expect("begin");
        assert_eq!((job.page_count(), job.pages_done()), (2, 0));
        assert_eq!(job.step(1), "pending");
        assert_eq!(job.step(1), "done");
        let package = wire::ImportPackage::decode(&job.finish_package().expect("finish")).unwrap();
        assert_eq!(package.original_len, PROSE.len() as u64);
        assert_eq!(package.original_hash, wire::fnv1a64(PROSE));
        assert_eq!(package.sources.pages.len(), 2);
        assert!(
            package
                .findings
                .iter()
                .any(|f| f.feature == "pdf.import.font_substituted")
        );
    }

    #[test]
    fn a_cancelled_import_produces_no_package() {
        let mut job = begin(PROSE.to_vec(), None).expect("begin");
        assert_eq!(job.step(1), "pending");
        job.cancel();
        assert_eq!(job.step(1), "cancelled");
        assert!(job.finish_package().is_err());
    }

    #[test]
    fn a_malformed_revision_request_is_refused_not_trapped() {
        let answer = wire::decode_revision(&read_revision(PROSE.to_vec(), b"junk")).unwrap();
        assert_eq!(answer.unwrap_err().reason, "request_malformed");
    }
}
