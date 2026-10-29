//! PDF import behind the registry seam (`docs/182` P1), the RTF adapter's
//! pattern applied to `casual-doc-pdf-import`.
//!
//! # The capability is explicit
//!
//! [`crate::builtin_registry`] keeps PDF **export-only**: its descriptor says
//! `can_import: false`, so detection never selects PDF and the editor's File ->
//! Open does not accept it (that is P2, with the staged job driven from the UI).
//! A headless host opts in twice, deliberately: it compiles the `pdf-import`
//! feature, and it calls [`enable_pdf_import`] (or builds with
//! [`builtin_registry_with_pdf_import`]), which replaces PDF's export-only
//! registration with [`PdfImportAdapter`], whose descriptor says
//! `can_import: true` and `exact_if_unchanged: true`. `opendoc-viewer` (feature
//! `pdf-import`) and `casual-doc-ai`'s `Session::open` (feature `pdf-import`,
//! which `opendoc-mcp` enables) do this.
//!
//! # What is retained
//!
//! With `retain_source`, the envelope holds the original bytes (shared with the
//! reader, not copied), and an `ExactIfUnchanged` export of an unchanged
//! session returns them byte for byte (MF-I4). Every other export is the
//! semantic writer (`docs/98`), except an edited session saved with
//! `PreserveWhenSafe` or `ExactIfUnchanged`: that appends an incremental update
//! to the original when it is safe (`crate::pdf_preserve`, `docs/182` P4), and
//! falls back to the semantic writer, reported, when it is not. A session whose
//! content equals the import's (an edit undone) writes the original back. No
//! PDF object reaches the model (MF-I1).
//!
//! # OCR is the host's
//!
//! [`PdfImportAdapter::new`] runs no OCR: a scanned page imports with its
//! picture and `pdf.import.ocr_unavailable`. A host that holds models builds
//! [`crate::PdfOcrOptions`] (for example `PdfOcrOptions::from_models` over a
//! [`crate::PdfOcrModels`]) and registers [`PdfImportAdapter::with_ocr`]
//! through [`enable_pdf_import_with_ocr`]. What read the pages, with the
//! SHA-256 of every model, is in [`pdf_ocr_summary`] of the import's source
//! envelope (PDFR-I8). Nothing here fetches a model.
//!
//! # A PDF opens as a flowing document
//!
//! [`PdfImportAdapter`] opens a PDF the way Word's PDF Reflow and Google Docs
//! do (`docs/182` §4): the page layout is evidence, and the document is an
//! ordinary flowing one ([`PdfLayout::Flowing`], `PdfImport::flowing`), whose
//! report lists what was reconstructed (`pdf.import.layout_inferred` and the
//! `*_inferred` rows). It saves as DOCX by default (its artifact's
//! [`crate::FormatProfile`] is DOCX); saved as PDF it is a new file. The
//! original is retained (with `retain_source`) only so the host can open the
//! PDF again. [`PdfImportAdapter::with_layout`] with [`PdfLayout::Fixed`]
//! ("Keep exact page layout") opens every page as drawn, in positioned
//! frames; only that layout saves back into the original (P4).
//!
//! # Reflow
//!
//! A fixed-layout envelope also keeps the import's reflow source (`docs/182` §3, the I4
//! sidecar: text blocks, rules, pictures by part name, vector art,
//! annotations, the structure tree), so [`pdf_reflow`] makes "Convert to
//! flowing document" from an open session without parsing the PDF again
//! (`docs/182` §18). Its memory is O(glyphs + path commands) for the
//! session's life.

use std::sync::Arc;

use casual_doc_pdf_import::{ImportJob, PdfImport, PdfImportError, Progress};
use casual_doc_pdf_import::{OcrOptions, OcrSummary};
use casual_doc_pdf_import::{PdfReadLimits, probe_pdf};

use crate::pdf_source::{
    PdfSavePlanner, PdfSourceState, SessionParts, export_pdf_session, pdf_retained_original,
    session_artifact,
};
// Own line: how a PDF opens (`docs/182` §4).
use crate::pdf_source::PdfLayout;
use crate::{
    AdapterError, DocumentResources, ExportArtifact, ExportRequest, FormatDescriptor,
    FormatExporter, FormatId, FormatImporter, FormatRegistry, ImportArtifact, ImportRequest,
    IoError, PdfAdapter, ProbeRequest, ProbeResult, SourceEnvelope, formats,
};

/// What OCR did during the import `source` came from, and the manifest of
/// the provider that read its pages (provider, version, SHA-256 of every
/// model): `None` when `source` is not an in-process PDF import (a session
/// opened from the editor's PDF module carries the reduced
/// [`crate::pdf_ocr_record`] instead). O(1).
#[must_use]
pub fn pdf_ocr_summary(source: &SourceEnvelope) -> Option<&OcrSummary> {
    source
        .state::<PdfSourceState>()
        .and_then(|state| state.ocr_detail.as_ref())
}

/// The planning half of the byte-preserving save, in process
/// (`docs/182` §17a): `casual-doc-pdf-save` compares the document with the
/// import's baseline, and the reader opens the original and prepares the
/// edited pages. What the browser's PDF module answers through the host.
#[derive(Clone, Copy, Debug, Default)]
pub struct InProcessSavePlanner;

impl PdfSavePlanner for InProcessSavePlanner {
    fn plan(
        &self,
        original: &Arc<Vec<u8>>,
        baseline: &[u8],
        document: &casual_doc_model::v1::Document,
    ) -> casual_doc_pdf_wire::SaveAnswer {
        let Ok(baseline) = casual_doc_pdf_wire::Baseline::decode(baseline) else {
            return casual_doc_pdf_wire::SaveAnswer::Refused(casual_doc_pdf_wire::Refusal {
                reason: "baseline_malformed".to_owned(),
                page: None,
            });
        };
        casual_doc_pdf_save::plan(document, &baseline, &|request| {
            casual_doc_pdf_read::revision::read_for_save(original.clone(), request)
        })
    }
}

/// PDF import and export: [`PdfAdapter`]'s writer plus the fixed-layout
/// importer, under one import-capable descriptor.
#[derive(Debug)]
pub struct PdfImportAdapter {
    descriptor: FormatDescriptor,
    writer: PdfAdapter,
    limits: PdfReadLimits,
    ocr: OcrOptions,
    layout: PdfLayout,
}

impl PdfImportAdapter {
    /// An adapter admitting PDFs under `limits` (`docs/21` PDF table), with
    /// no OCR.
    #[must_use]
    pub fn new(limits: PdfReadLimits) -> Self {
        Self::with_ocr(limits, OcrOptions::default())
    }

    /// An adapter that reads scanned pages and unrecoverable runs with the
    /// host's OCR (`docs/182` §20a). `OcrOptions::default()` is
    /// [`PdfImportAdapter::new`].
    #[must_use]
    pub fn with_ocr(limits: PdfReadLimits, ocr: OcrOptions) -> Self {
        let writer = PdfAdapter::default();
        let mut descriptor = FormatExporter::descriptor(&writer).clone();
        descriptor.can_import = true;
        // An unchanged session writes the retained original back exactly; an
        // edited one appends an incremental update when that is safe.
        descriptor.exact_if_unchanged = true;
        descriptor.preserve_when_safe = true;
        Self {
            descriptor,
            writer,
            limits,
            ocr,
            layout: PdfLayout::default(),
        }
    }

    /// The adapter opening PDFs in `layout`: [`PdfLayout::Flowing`] (the
    /// default) or [`PdfLayout::Fixed`] ("Keep exact page layout").
    #[must_use]
    pub fn with_layout(mut self, layout: PdfLayout) -> Self {
        self.layout = layout;
        self
    }

    /// How this adapter opens a PDF. O(1).
    #[must_use]
    pub fn layout(&self) -> PdfLayout {
        self.layout
    }

    /// The original bytes `source` retained, when it is a PDF envelope made
    /// with `retain_source`.
    #[must_use]
    pub fn retained_source_bytes<'a>(&self, source: &'a crate::SourceEnvelope) -> Option<&'a [u8]> {
        pdf_retained_original(source)
    }

    /// Starts a staged import (`docs/182` §6): admission now, then
    /// [`PdfStagedImport::step`] a page budget at a time.
    ///
    /// # Errors
    ///
    /// The reader's refusal, as an [`AdapterError`].
    pub fn begin(
        &self,
        bytes: &[u8],
        retain_source: bool,
    ) -> Result<PdfStagedImport, AdapterError> {
        let job = ImportJob::begin_with_ocr(bytes.to_vec(), self.limits, self.ocr.clone())
            .map_err(adapter_error)?;
        Ok(PdfStagedImport {
            job,
            retain_source,
            format: self.descriptor.id.clone(),
            layout: self.layout,
        })
    }
}

impl Default for PdfImportAdapter {
    fn default() -> Self {
        Self::new(PdfReadLimits::default())
    }
}

/// A staged PDF import: `step` / `cancel` / `finish`, the shape of
/// `casual-doc-diff`'s job. Nothing is published until `finish` (PDFR-I3).
#[derive(Debug)]
pub struct PdfStagedImport {
    job: ImportJob,
    retain_source: bool,
    format: FormatId,
    layout: PdfLayout,
}

impl PdfStagedImport {
    /// Pages in the document, known from admission, so a host can show
    /// "page 0 of N" before the first step. O(1).
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.job.page_count()
    }

    /// Does at most `budget` pages. O(budget).
    pub fn step(&mut self, budget: usize) -> Progress {
        self.job.step(budget)
    }

    /// Stops; `finish` then fails and nothing is produced.
    pub fn cancel(&mut self) {
        self.job.cancel();
    }

    /// The artifact, atomically.
    ///
    /// # Errors
    ///
    /// Cancellation, a page that broke a limit, or an invalid model.
    pub fn finish(self) -> Result<ImportArtifact, AdapterError> {
        let imported = self.job.finish().map_err(adapter_error)?;
        Ok(match self.layout {
            PdfLayout::Fixed => artifact(imported, self.retain_source, self.format),
            PdfLayout::Flowing => flowing_artifact(imported, self.retain_source),
        })
    }
}

fn adapter_error(error: PdfImportError) -> AdapterError {
    AdapterError::new(format!("PDF import: {error}"))
}

/// "Convert to flowing document" (`docs/182` §4.2, §18) for the PDF import
/// `source` came from: a **new** import artifact with its own document,
/// resources (the pictures and font programs it references, copied from
/// `resources`, the open document's) and report (`pdf.import.*` reflow
/// families). The open document and its envelope are not touched; the new
/// envelope retains no original bytes, so the reflowed document never writes
/// the PDF back. `None` when `source` is not an in-process PDF import (a
/// session opened from the editor's PDF module reflows there,
/// `finishReflowed`). O(document log document).
///
/// # Errors
///
/// [`AdapterError`] when the reflowed document fails validation (an importer
/// defect, never a property of the input).
pub fn pdf_reflow(
    source: &SourceEnvelope,
    resources: &DocumentResources,
) -> Option<Result<ImportArtifact, AdapterError>> {
    let state = source.state::<PdfSourceState>()?;
    let (reflow_state, namespace) = state.reflow.as_ref()?;
    Some(
        casual_doc_pdf_import::reflow::build_for_import(
            reflow_state,
            resources.as_map(),
            *namespace,
        )
        .map_err(adapter_error)
        .map(|reflowed| {
            let findings = casual_doc_pdf_import::wire::findings(&reflowed.report);
            let mut resources = DocumentResources::default();
            for (name, bytes) in reflowed.resources {
                resources.insert(name, bytes);
            }
            session_artifact(SessionParts {
                document: reflowed.document,
                resources,
                findings: &findings,
                baseline: None,
                original: None,
                ocr: state.ocr.clone(),
                ocr_detail: state.ocr_detail.clone(),
                reflow: None,
                layout: PdfLayout::Flowing,
            })
        }),
    )
}

/// The default open: the import reconstructed as a flowing document
/// (`PdfImport::flowing`). Should the reconstruction fail validation (an
/// importer defect, never a property of the input), the PDF still opens, in
/// its exact layout, and the report says so
/// (`pdf.import.flowing_unavailable`). O(document log document).
fn flowing_artifact(imported: PdfImport, retain_source: bool) -> ImportArtifact {
    let reflowed = match imported.flowing() {
        Ok(reflowed) => reflowed,
        Err(_) => {
            let mut fixed = artifact(
                imported,
                retain_source,
                FormatId::new(formats::PDF).expect("valid"),
            );
            fixed.report.entries.push(crate::CompatibilityEntry {
                feature: casual_doc_pdf_import::report::FLOWING_UNAVAILABLE.to_owned(),
                occurrences: 1,
                location: crate::FeatureLocation::default(),
                model_outcome: crate::ModelOutcome::Degraded,
                retention_outcome: crate::RetentionOutcome::NotApplicable,
            });
            fixed.report.sort();
            return fixed;
        }
    };
    let findings = casual_doc_pdf_import::wire::findings(&reflowed.report);
    let ocr = casual_doc_pdf_import::wire::ocr(&imported.ocr);
    let mut resources = DocumentResources::default();
    for (name, bytes) in reflowed.resources {
        resources.insert(name, bytes);
    }
    session_artifact(SessionParts {
        document: reflowed.document,
        resources,
        findings: &findings,
        baseline: None,
        original: retain_source.then_some(imported.original),
        ocr,
        ocr_detail: Some(imported.ocr),
        reflow: None,
        layout: PdfLayout::Flowing,
    })
}

fn artifact(imported: PdfImport, retain_source: bool, _format: FormatId) -> ImportArtifact {
    let findings = casual_doc_pdf_import::wire::findings(&imported.report);
    // The baseline is only worth computing when the original is kept for it.
    let baseline = retain_source
        .then(|| {
            casual_doc_pdf_save::baseline(
                &imported.document,
                casual_doc_pdf_import::wire::sources(&imported.sources),
            )
        })
        .flatten()
        .map(|baseline| baseline.encode());
    let ocr = casual_doc_pdf_import::wire::ocr(&imported.ocr);
    let mut resources = DocumentResources::default();
    for (name, bytes) in imported.resources {
        resources.insert(name, bytes);
    }
    session_artifact(SessionParts {
        document: imported.document,
        resources,
        findings: &findings,
        baseline,
        original: retain_source.then_some(imported.original),
        ocr,
        ocr_detail: Some(imported.ocr),
        reflow: Some((imported.source, imported.namespace)),
        layout: PdfLayout::Fixed,
    })
}

impl FormatImporter for PdfImportAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult {
        if probe_pdf(request.bytes) {
            ProbeResult::definite("pdf.header")
        } else {
            ProbeResult::no_match("pdf.no-header")
        }
    }

    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifact, AdapterError> {
        self.begin(request.bytes, request.retain_source)?.finish()
    }
}

impl FormatExporter for PdfImportAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError> {
        export_pdf_session(request, Some(&InProcessSavePlanner), &|request| {
            self.writer.export(request)
        })
    }
}

/// Replaces `registry`'s PDF registration with the import-capable adapter.
///
/// # Errors
///
/// Never in practice: re-registration into a cleared slot cannot collide.
pub fn enable_pdf_import(
    registry: &mut FormatRegistry,
    limits: PdfReadLimits,
) -> Result<(), IoError> {
    enable_pdf_import_with_ocr(registry, limits, OcrOptions::default())
}

/// [`enable_pdf_import`] with the host's OCR: scanned pages and runs whose
/// Unicode is unrecoverable are read by `ocr`'s provider and merged
/// native-first (`docs/182` §20).
///
/// # Errors
///
/// Never in practice: re-registration into a cleared slot cannot collide.
pub fn enable_pdf_import_with_ocr(
    registry: &mut FormatRegistry,
    limits: PdfReadLimits,
    ocr: OcrOptions,
) -> Result<(), IoError> {
    enable_pdf_import_with_layout(registry, limits, ocr, PdfLayout::default())
}

/// [`enable_pdf_import_with_ocr`] opening PDFs in `layout`: the host's
/// "Keep exact page layout" ([`PdfLayout::Fixed`]) or the default flowing
/// document.
///
/// # Errors
///
/// Never in practice: re-registration into a cleared slot cannot collide.
pub fn enable_pdf_import_with_layout(
    registry: &mut FormatRegistry,
    limits: PdfReadLimits,
    ocr: OcrOptions,
    layout: PdfLayout,
) -> Result<(), IoError> {
    let id = FormatId::new(formats::PDF).expect("built-in PDF format id is valid");
    registry.unregister(&id);
    let adapter = Arc::new(PdfImportAdapter::with_ocr(limits, ocr).with_layout(layout));
    registry.register_importer(adapter.clone())?;
    registry.register_exporter(adapter)
}

/// [`crate::builtin_registry`] with PDF import enabled under the default
/// limits: what a headless host that opens PDFs uses.
#[must_use]
pub fn builtin_registry_with_pdf_import() -> FormatRegistry {
    let mut registry = crate::builtin_registry();
    enable_pdf_import(&mut registry, PdfReadLimits::default())
        .expect("re-registering PDF into a cleared slot cannot collide");
    registry
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DetectionRequest, ExportMode, FormatSelection, ModelOutcome, builtin_registry};

    const PROSE: &[u8] = include_bytes!("../../../fixtures/pdf/synthetic-prose.pdf");

    fn detect(registry: &FormatRegistry, bytes: &[u8]) -> Result<FormatId, IoError> {
        registry.detect(DetectionRequest {
            bytes,
            selection: FormatSelection::Auto,
            file_name_hint: Some("file.pdf"),
            mime_hint: None,
        })
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

    impl crate::pdf_ocr::OcrProvider for Fake {
        fn manifest(&self) -> crate::pdf_ocr::ProviderManifest {
            crate::pdf_ocr::ProviderManifest {
                provider: "fake",
                version: "test",
                models: vec![("detection", "00".repeat(32))],
                decoding: "fixed",
                determinism: crate::pdf_ocr::Determinism::Exact,
            }
        }

        fn recognize(
            &self,
            raster: &crate::pdf_ocr::Raster,
            _limits: &crate::pdf_ocr::OcrLimits,
            _interrupt: &dyn crate::pdf_ocr::Interrupt,
        ) -> Result<crate::pdf_ocr::OcrPage, crate::pdf_ocr::OcrError> {
            let (w, h) = (raster.width() as f32, raster.height() as f32);
            let bounds = crate::pdf_ocr::OcrBox {
                x0: w * 0.1,
                y0: h * 0.1,
                x1: w * 0.4,
                y1: h * 0.12,
            };
            let word = crate::pdf_ocr::OcrWord {
                text: "Scanned".to_owned(),
                bounds,
                confidence: 0.99,
            };
            Ok(crate::pdf_ocr::OcrPage {
                lines: vec![crate::pdf_ocr::OcrLine::upright(vec![word], bounds)],
                ..crate::pdf_ocr::OcrPage::default()
            })
        }
    }

    fn import_text(registry: &FormatRegistry, bytes: &[u8]) -> (String, ImportArtifact) {
        let artifact = registry
            .import(
                DetectionRequest {
                    bytes,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("scan.pdf"),
                    mime_hint: None,
                },
                false,
            )
            .expect("imports");
        (format!("{:?}", artifact.document.body()), artifact)
    }

    #[test]
    fn a_host_with_ocr_reads_a_scan_and_the_import_records_what_read_it() {
        let scan = blank_scan();
        let plain = builtin_registry_with_pdf_import();
        let (text, artifact) = import_text(&plain, &scan);
        assert!(!text.contains("Scanned"));
        assert!(
            artifact
                .report
                .entries
                .iter()
                .any(|e| e.feature == "pdf.import.ocr_unavailable")
        );
        assert_eq!(
            pdf_ocr_summary(&artifact.source).map(|s| s.provider.is_some()),
            Some(false)
        );

        let mut registry = builtin_registry();
        enable_pdf_import_with_ocr(
            &mut registry,
            PdfReadLimits::default(),
            crate::PdfOcrOptions {
                provider: Some(Arc::new(Fake)),
                ..crate::PdfOcrOptions::default()
            },
        )
        .unwrap();
        let (text, artifact) = import_text(&registry, &scan);
        assert!(
            text.contains("Scanned"),
            "the scan's text is in the document"
        );
        let features: Vec<&str> = artifact
            .report
            .entries
            .iter()
            .map(|e| e.feature.as_str())
            .collect();
        assert!(features.contains(&"pdf.import.ocr_applied"), "{features:?}");
        assert!(
            !features.contains(&"pdf.import.ocr_unavailable"),
            "{features:?}"
        );
        let summary = pdf_ocr_summary(&artifact.source).expect("a PDF import");
        let manifest = summary.provider.as_ref().expect("the provider is recorded");
        assert_eq!(manifest.provider, "fake");
        assert_eq!(manifest.models, vec![("detection", "00".repeat(32))]);
        assert_eq!(summary.words_kept, 1);
    }

    #[test]
    fn the_builtin_registry_still_never_imports_pdf() {
        // The P2 boundary: File -> Open must not accept PDF until the editor
        // drives the staged job. Compiling the feature changes nothing here.
        let registry = builtin_registry();
        assert!(matches!(
            detect(&registry, PROSE),
            Err(IoError::UnsupportedFormat { .. })
        ));
    }

    /// The default open is the flowing document (`docs/182` §4): paragraphs
    /// in the body, not one positioned frame per text block, saved as DOCX
    /// by default, its report saying the layout was inferred. Mutation:
    /// make `PdfLayout::default()` `Fixed` and `body` holds `TextBox`.
    #[test]
    fn a_pdf_opens_as_a_flowing_document_by_default() {
        let registry = builtin_registry_with_pdf_import();
        let artifact = registry
            .import(
                DetectionRequest {
                    bytes: PROSE,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("prose.pdf"),
                    mime_hint: None,
                },
                true,
            )
            .expect("import");
        let body = format!("{:?}", artifact.document.body());
        assert!(!body.contains("TextBox"), "{body}");
        assert!(body.contains("Paragraph"), "{body}");
        assert_eq!(artifact.format.format.as_str(), formats::DOCX);
        let features: Vec<&str> = artifact
            .report
            .entries
            .iter()
            .map(|e| e.feature.as_str())
            .collect();
        for wanted in [
            "pdf.import.layout_inferred",
            "pdf.import.exact_save_fixed_layout_only",
            "pdf.import.paragraph_boundary_inferred",
            // What reading the pages found is in the flowing report too.
            "pdf.import.font_substituted",
        ] {
            assert!(features.contains(&wanted), "{wanted}: {features:?}");
        }
        assert_eq!(
            crate::pdf_layout(&artifact.source),
            Some(PdfLayout::Flowing)
        );
        // A flowing document does not convert again.
        assert!(pdf_reflow(&artifact.source, &artifact.resources).is_none());
    }

    #[test]
    fn an_opted_in_registry_detects_imports_and_reports() {
        let mut registry = crate::builtin_registry();
        enable_pdf_import_with_layout(
            &mut registry,
            PdfReadLimits::default(),
            OcrOptions::default(),
            PdfLayout::Fixed,
        )
        .unwrap();
        assert_eq!(detect(&registry, PROSE).unwrap().as_str(), formats::PDF);
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|d| d.id.as_str() == formats::PDF)
            .unwrap();
        assert!(descriptor.can_import && descriptor.can_export && descriptor.exact_if_unchanged);
        assert!(descriptor.preserve_when_safe);
        let artifact = registry
            .import(
                DetectionRequest {
                    bytes: PROSE,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                true,
            )
            .expect("import");
        assert_eq!(artifact.document.definitions().sections.len(), 2);
        let substituted = artifact
            .report
            .entries
            .iter()
            .find(|e| e.feature == "pdf.import.font_substituted")
            .expect("the substitution is reported");
        assert_eq!(substituted.model_outcome, ModelOutcome::Degraded);
        assert!(
            registry
                .export_formats()
                .iter()
                .any(|id| id.as_str() == formats::PDF)
        );
    }

    #[test]
    fn an_unchanged_session_writes_the_original_back_exactly() {
        let adapter = PdfImportAdapter::default().with_layout(PdfLayout::Fixed);
        let artifact = adapter
            .import(ImportRequest {
                bytes: PROSE,
                retain_source: true,
            })
            .unwrap();
        assert_eq!(adapter.retained_source_bytes(&artifact.source), Some(PROSE));
        let request = |unchanged, mode| ExportRequest {
            document: &artifact.document,
            resources: &artifact.resources,
            source: Some(&artifact.source),
            source_unchanged: unchanged,
            mode,
        };
        let exact = adapter
            .export(request(true, ExportMode::ExactIfUnchanged))
            .unwrap();
        assert_eq!(exact.bytes, PROSE);
        // The watermark moved but the content is the import's (an edit
        // undone): the original comes back, reported, rather than a re-render.
        let undone = adapter
            .export(request(false, ExportMode::ExactIfUnchanged))
            .unwrap();
        assert_eq!(undone.bytes, PROSE);
        assert!(
            undone
                .report
                .entries
                .iter()
                .any(|e| e.feature == "pdf.export.original_written_back")
        );
        // Asked for a semantic write: the writer renders the model.
        let semantic = adapter
            .export(request(false, ExportMode::Semantic))
            .unwrap();
        assert_ne!(semantic.bytes, PROSE);
        assert!(semantic.bytes.starts_with(b"%PDF-"));
        // Without retention there is nothing to write back.
        let plain = adapter
            .import(ImportRequest {
                bytes: PROSE,
                retain_source: false,
            })
            .unwrap();
        assert_eq!(adapter.retained_source_bytes(&plain.source), None);
    }

    #[test]
    fn the_staged_job_cancels_without_producing_anything() {
        let adapter = PdfImportAdapter::default().with_layout(PdfLayout::Fixed);
        let mut job = adapter.begin(PROSE, false).unwrap();
        assert_eq!(job.step(1), Progress::Pending { page: 1, of: 2 });
        job.cancel();
        assert!(job.finish().is_err());
        let mut job = adapter.begin(PROSE, false).unwrap();
        while job.step(1) != Progress::Done {}
        assert_eq!(
            job.finish().unwrap().document.definitions().sections.len(),
            2
        );
    }

    #[test]
    fn an_open_pdf_converts_to_a_new_flowing_document_from_its_envelope() {
        let adapter = PdfImportAdapter::default().with_layout(PdfLayout::Fixed);
        let artifact = adapter
            .import(ImportRequest {
                bytes: PROSE,
                retain_source: true,
            })
            .unwrap();
        let before = artifact.document.clone();
        let reflowed = pdf_reflow(&artifact.source, &artifact.resources)
            .expect("a PDF import keeps its reflow source")
            .expect("reflows");
        // The same document the importer's own reflow makes: no second parse,
        // the same ids.
        let direct = casual_doc_pdf_import::import_pdf(PROSE.to_vec(), PdfReadLimits::default())
            .unwrap()
            .reflow()
            .unwrap();
        assert_eq!(reflowed.document, direct.document);
        assert_eq!(artifact.document, before, "the open document is untouched");
        // Flowing: paragraphs in the body, not one page-anchored frame per block.
        let body = format!("{:?}", reflowed.document.body());
        assert!(!body.contains("TextBox"), "{body}");
        assert!(
            reflowed
                .report
                .entries
                .iter()
                .any(|e| e.feature == "pdf.import.paragraph_boundary_inferred")
        );
        // The new document is not the PDF: it cannot write the original back,
        // and it does not convert again.
        assert_eq!(adapter.retained_source_bytes(&reflowed.source), None);
        assert!(pdf_reflow(&reflowed.source, &reflowed.resources).is_none());
        // Nor does a document that never was a PDF.
        let text = crate::builtin_registry()
            .import(
                DetectionRequest {
                    bytes: b"plain words",
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("notes.txt"),
                    mime_hint: None,
                },
                false,
            )
            .unwrap();
        assert!(pdf_reflow(&text.source, &text.resources).is_none());
    }

    #[test]
    fn an_encrypted_pdf_is_refused_through_the_registry() {
        let registry = builtin_registry_with_pdf_import();
        let error = registry
            .import(
                DetectionRequest {
                    bytes: include_bytes!("../../../fixtures/pdf/hostile/hostile-encrypted.pdf"),
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                false,
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("pdf.import.encrypted_unsupported"),
            "{error}"
        );
    }
}
