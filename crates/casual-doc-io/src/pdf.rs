//! Built-in export-only PDF adapter.
//!
//! PDF joins DOCX, ODT, plain text and normalized JSON as a registered
//! [`FormatExporter`], so a host reaches it through the same
//! `available_export_formats()` / `export_as(format_id, mode)` seam and needs
//! no PDF-specific code path (`docs/104` HF-030 "Decision": a PDF writer in the
//! export registry, not a browser Save-as-PDF).
//!
//! # Import is deliberately absent
//!
//! No importer is registered, so `.pdf` never appears in a detection result and
//! the UI cannot offer a format the engine cannot read. PDF *editing* is out of
//! scope for the product (`AGENTS.md`).
//!
//! # Fonts and the parity question
//!
//! The adapter lays the document out with the shared pagination pass and embeds
//! the faces **that pass** resolved, so the file is self-consistent: the glyph
//! ids in the content stream address the faces the same pass measured with.
//!
//! What it cannot yet do is reuse a *host's* live font registry. A browser
//! viewer that fetched a CJK face at runtime has ids in its own registry, and
//! those ids mean nothing to a fresh pagination pass, so handing them over
//! would embed the wrong face rather than a fallback. A host in that position
//! should drive `casual_doc_pdf::write_pdf` with the display lists and the
//! font source it already holds. Closing the gap for the registry path needs
//! the session's registry to seed the export's pagination, which is recorded
//! as remaining work in `docs/98`.

use std::sync::Arc;

use casual_doc_pdf::PdfError;
use casual_doc_pdf::PdfExportOptions;
use casual_doc_pdf::PdfMediaSource;
use casual_doc_pdf::PdfSeverity;
use casual_doc_pdf::export_document;

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, ExportArtifact,
    ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId, FormatProfile,
    FormatRegistry, IoError, ModelOutcome, RetentionOutcome, formats,
};

const PDF_MIME: &str = "application/pdf";
/// The emitted profile: the PDF version this writer targets.
const PDF_PROFILE: &str = "1.7";

/// Export-only PDF adapter.
#[derive(Debug)]
pub struct PdfAdapter {
    descriptor: FormatDescriptor,
    options: PdfExportOptions,
}

impl Default for PdfAdapter {
    fn default() -> Self {
        Self::new(PdfExportOptions::with_metadata())
    }
}

impl PdfAdapter {
    /// An adapter with the given export options.
    #[must_use]
    pub fn new(options: PdfExportOptions) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::PDF).expect("built-in PDF format id is valid"),
                display_name: "PDF".to_owned(),
                mime_types: vec![PDF_MIME.to_owned()],
                extensions: vec!["pdf".to_owned()],
                can_import: false,
                can_export: true,
                // A PDF is never the source, so neither preservation mode can
                // ever apply to it.
                exact_if_unchanged: false,
                preserve_when_safe: false,
            },
            options,
        }
    }
}

/// Serves the export's pictures out of the request's [`DocumentResources`].
struct ResourceMedia<'a>(&'a DocumentResources);

impl PdfMediaSource for ResourceMedia<'_> {
    fn media_bytes(&self, media: &str) -> Option<&[u8]> {
        self.0.get(media)
    }
}

impl FormatExporter for PdfAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError> {
        // `mode` selects how much of the SOURCE format is preserved. A PDF is
        // never the source, so every mode produces the same semantic export;
        // it is accepted rather than refused so a host that always asks for
        // `ExactIfUnchanged` still gets a PDF instead of an error.
        let media = ResourceMedia(request.resources);
        let export = export_document(request.document, &media, &self.options)
            .map_err(|error: PdfError| AdapterError::new(format!("PDF export failed: {error}")))?;

        let mut report = CompatibilityReport::default();
        for finding in export.findings {
            report.entries.push(CompatibilityEntry {
                feature: finding.code,
                occurrences: u32::try_from(finding.occurrences).unwrap_or(u32::MAX),
                location: FeatureLocation::default(),
                model_outcome: match finding.severity {
                    PdfSeverity::Note => ModelOutcome::Mapped,
                    PdfSeverity::Degraded => ModelOutcome::Degraded,
                },
                retention_outcome: match finding.severity {
                    PdfSeverity::Note => RetentionOutcome::NotApplicable,
                    PdfSeverity::Degraded => RetentionOutcome::NotRetained,
                },
            });
        }
        report.sort();

        Ok(ExportArtifact {
            bytes: export.bytes,
            report,
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some(PDF_PROFILE.to_owned()),
            },
            mime_type: PDF_MIME.to_owned(),
            suggested_extension: "pdf".to_owned(),
        })
    }
}

/// Registers the PDF exporter into `registry`.
///
/// This is **opt-in** rather than part of [`crate::builtin_registry`] so a host
/// chooses when PDF appears in its save-as list, and so the browser build can
/// keep it behind the wiring that gives the export the session's own fonts.
///
/// # Errors
///
/// Returns [`IoError::DuplicateAdapter`] if a PDF exporter is already
/// registered.
pub fn register_pdf_exporter(registry: &mut FormatRegistry) -> Result<(), IoError> {
    registry.register_exporter(Arc::new(PdfAdapter::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ExportMode, FormatImporter, ImportRequest, PlainTextAdapter, PlainTextLimits,
        builtin_registry,
    };
    use casual_doc_model::v1::Document;

    /// A one-paragraph document, built through the plain-text importer so the
    /// test does not hand-assemble a model.
    fn document(text: &str) -> Document {
        PlainTextAdapter::new(PlainTextLimits::default())
            .import(ImportRequest {
                bytes: text.as_bytes(),
                retain_source: false,
            })
            .expect("import plain text")
            .document
    }

    #[test]
    fn pdf_is_export_only_and_never_offered_as_an_import_format() {
        let mut registry = builtin_registry();
        register_pdf_exporter(&mut registry).expect("register");
        let pdf = FormatId::new(formats::PDF).unwrap();
        assert!(registry.export_formats().contains(&&pdf));
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id == pdf)
            .expect("descriptor");
        assert!(!descriptor.can_import);
        assert!(descriptor.can_export);
    }

    #[test]
    fn exporting_through_the_registry_produces_a_pdf_with_the_documents_text() {
        let mut registry = builtin_registry();
        register_pdf_exporter(&mut registry).expect("register");
        let document = document("Registry reaches the writer");
        let resources = DocumentResources::default();
        let artifact = registry
            .export(
                &FormatId::new(formats::PDF).unwrap(),
                ExportRequest {
                    document: &document,
                    resources: &resources,
                    source: None,
                    source_unchanged: false,
                    mode: ExportMode::Semantic,
                },
            )
            .expect("export through the registry");
        assert_eq!(artifact.mime_type, PDF_MIME);
        assert_eq!(artifact.suggested_extension, "pdf");
        assert!(artifact.bytes.starts_with(b"%PDF-"));

        // The bytes carry real text, not a picture of it.
        let pdf = casual_doc_pdf::inspect::parse(&artifact.bytes).expect("parse");
        let page = pdf.pages().remove(0);
        let contents = pdf.page_contents(page);
        assert!(contents.images.is_empty());
        assert_eq!(contents.plain_text(), "Registry reaches the writer");
    }

    #[test]
    fn a_preservation_mode_still_produces_a_pdf_rather_than_an_error() {
        // A PDF is never the source, so `ExactIfUnchanged` has nothing to
        // preserve. Refusing would turn a host's default mode into a dead
        // control.
        let adapter = PdfAdapter::default();
        let document = document("Mode is irrelevant here");
        let resources = DocumentResources::default();
        for mode in [
            ExportMode::Semantic,
            ExportMode::PreserveWhenSafe,
            ExportMode::ExactIfUnchanged,
        ] {
            let artifact = adapter
                .export(ExportRequest {
                    document: &document,
                    resources: &resources,
                    source: None,
                    source_unchanged: true,
                    mode,
                })
                .expect("export");
            assert!(artifact.bytes.starts_with(b"%PDF-"), "{mode:?}");
        }
    }
}
