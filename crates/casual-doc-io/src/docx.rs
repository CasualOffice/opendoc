//! Built-in adapter over the existing DOCX pipeline.

use std::sync::Arc;

use casual_doc_export::{write_document, write_document_with_retained_parts};
use casual_doc_import::{
    ImportConfig, ImportMode, ModelOutcome as DocxModelOutcome, RetainedParts,
    RetentionOutcome as DocxRetentionOutcome, import_package,
};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, ExportArtifact,
    ExportMode, ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId,
    FormatImporter, FormatProfile, FormatRegistry, ImportArtifact, ImportRequest, ModelOutcome,
    ProbeRequest, ProbeResult, RetentionOutcome, SourceEnvelope, formats,
};

const DOCX_MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

#[derive(Debug)]
struct DocxSourceState {
    original_bytes: Option<Vec<u8>>,
    retained_parts: RetainedParts,
}

/// Built-in adapter that delegates to the existing bounded DOCX pipeline.
#[derive(Clone, Debug)]
pub struct DocxAdapter {
    descriptor: FormatDescriptor,
    package_limits: PackageLimits,
    import_config: ImportConfig,
}

impl DocxAdapter {
    /// Creates an adapter with explicit existing DOCX package/import limits.
    #[must_use]
    pub fn new(package_limits: PackageLimits, import_config: ImportConfig) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::DOCX).expect("built-in DOCX format id is valid"),
                display_name: "Office Open XML Document".to_owned(),
                mime_types: vec![DOCX_MIME.to_owned()],
                extensions: vec!["docx".to_owned()],
                can_import: true,
                can_export: true,
                exact_if_unchanged: true,
                preserve_when_safe: true,
            },
            package_limits,
            import_config,
        }
    }
}

impl Default for DocxAdapter {
    fn default() -> Self {
        Self::new(PackageLimits::default(), ImportConfig::default())
    }
}

impl FormatImporter for DocxAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult {
        match DocxPackage::open(request.bytes, self.package_limits) {
            Ok(_) => ProbeResult::definite("docx.opc.office-document"),
            Err(_) => ProbeResult::no_match("docx.opc.not-admitted"),
        }
    }

    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifact, AdapterError> {
        let mut package = DocxPackage::open(request.bytes, self.package_limits)
            .map_err(|error| AdapterError::new(format!("package admission: {error}")))?;
        let mut config = self.import_config;
        config.mode = if request.retain_source {
            ImportMode::Retention
        } else {
            ImportMode::Semantic
        };
        let imported = import_package(&mut package, config)
            .map_err(|error| AdapterError::new(format!("semantic import: {error}")))?;

        let mut resources = DocumentResources::default();
        for (_, reference) in imported.document.definitions().media.iter() {
            if let Ok(bytes) = package.read_part(&reference.part_name) {
                resources.insert(reference.part_name.clone(), bytes);
            }
        }
        let report = convert_report(&imported.report);
        let source = SourceEnvelope::new(
            self.descriptor.id.clone(),
            env!("CARGO_PKG_VERSION").to_owned(),
            DocxSourceState {
                original_bytes: request.retain_source.then(|| request.bytes.to_vec()),
                retained_parts: imported.retained_parts,
            },
        );
        Ok(ImportArtifact {
            document: imported.document,
            resources,
            source,
            report,
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: None,
            },
        })
    }
}

impl FormatExporter for DocxAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError> {
        let empty_retained = RetainedParts::default();
        let matching_source = request
            .source
            .filter(|source| source.format() == &self.descriptor.id)
            .and_then(SourceEnvelope::state::<DocxSourceState>);

        let (bytes, mut report) = match request.mode {
            ExportMode::Semantic => (
                write_document(request.document, request.resources.as_map())
                    .map_err(|error| AdapterError::new(format!("semantic writer: {error}")))?,
                CompatibilityReport::default(),
            ),
            ExportMode::PreserveWhenSafe => {
                let retained = matching_source
                    .map(|source| &source.retained_parts)
                    .unwrap_or(&empty_retained);
                let bytes = write_document_with_retained_parts(
                    request.document,
                    request.resources.as_map(),
                    retained,
                )
                .map_err(|error| AdapterError::new(format!("semantic writer: {error}")))?;
                let mut report = CompatibilityReport::default();
                if request.source.is_some() && matching_source.is_none() {
                    report.entries.push(CompatibilityEntry {
                        feature: "source_envelope".to_owned(),
                        occurrences: 1,
                        location: FeatureLocation::default(),
                        model_outcome: ModelOutcome::Omitted,
                        retention_outcome: RetentionOutcome::NotRetained,
                    });
                }
                (bytes, report)
            }
            ExportMode::ExactIfUnchanged => {
                let bytes = matching_source
                    .filter(|_| request.source_unchanged)
                    .and_then(|source| source.original_bytes.clone())
                    .ok_or_else(|| {
                        AdapterError::new(
                            "exact export requires matching retained source and an unchanged document",
                        )
                    })?;
                (bytes, CompatibilityReport::default())
            }
        };
        report.sort();
        Ok(ExportArtifact {
            bytes,
            report,
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: None,
            },
            mime_type: DOCX_MIME.to_owned(),
            suggested_extension: "docx".to_owned(),
        })
    }
}

/// Creates the built-in registry for the currently implemented formats.
pub fn builtin_registry() -> FormatRegistry {
    let adapter = Arc::new(DocxAdapter::default());
    let mut registry = FormatRegistry::new();
    registry
        .register_importer(adapter.clone())
        .expect("built-in DOCX importer registration is unique");
    registry
        .register_exporter(adapter)
        .expect("built-in DOCX exporter registration is unique");
    registry
}

fn convert_report(report: &casual_doc_import::CompatibilityReport) -> CompatibilityReport {
    let mut converted = CompatibilityReport {
        entries: report
            .entries
            .iter()
            .map(|entry| CompatibilityEntry {
                feature: entry.feature.clone(),
                occurrences: entry.occurrences,
                location: FeatureLocation {
                    part_name: entry.part.as_ref().map(|part| part.part_name.clone()),
                    namespace: None,
                    local_name: entry.part.is_none().then(|| entry.feature.clone()),
                },
                model_outcome: match entry.model_outcome {
                    DocxModelOutcome::Mapped => ModelOutcome::Mapped,
                    DocxModelOutcome::Degraded => ModelOutcome::Degraded,
                    DocxModelOutcome::Omitted => ModelOutcome::Omitted,
                },
                retention_outcome: match entry.retention_outcome {
                    DocxRetentionOutcome::Preserved => RetentionOutcome::Preserved,
                    DocxRetentionOutcome::NotRetained => RetentionOutcome::NotRetained,
                    DocxRetentionOutcome::Blocked => RetentionOutcome::Blocked,
                    DocxRetentionOutcome::Rejected => RetentionOutcome::Rejected,
                    DocxRetentionOutcome::NotApplicable => RetentionOutcome::NotApplicable,
                },
            })
            .collect(),
    };
    converted.sort();
    converted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DetectionRequest, ExportRequest, FormatSelection};

    const MINIMAL_DOCX: &[u8] = include_bytes!("../../../fixtures/generated/minimal-valid.docx");

    #[test]
    fn builtin_registry_detects_imports_exports_and_reopens_docx() {
        let registry = builtin_registry();
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: MINIMAL_DOCX,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("misleading.odt"),
                    mime_hint: Some("application/vnd.oasis.opendocument.text"),
                },
                true,
            )
            .unwrap();
        assert_eq!(imported.format.format.as_str(), formats::DOCX);
        let original_model = imported.document.clone();
        let exported = registry
            .export(
                &FormatId::new(formats::DOCX).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: false,
                    mode: ExportMode::PreserveWhenSafe,
                },
            )
            .unwrap();
        let reopened = registry
            .import(
                DetectionRequest {
                    bytes: &exported.bytes,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                false,
            )
            .unwrap();
        assert_eq!(reopened.document, original_model);
    }

    #[test]
    fn exact_unchanged_export_returns_original_bytes_only_when_authorized() {
        let registry = builtin_registry();
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: MINIMAL_DOCX,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                true,
            )
            .unwrap();
        let exact = registry
            .export(
                &FormatId::new(formats::DOCX).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: true,
                    mode: ExportMode::ExactIfUnchanged,
                },
            )
            .unwrap();
        assert_eq!(exact.bytes, MINIMAL_DOCX);

        let error = registry
            .export(
                &FormatId::new(formats::DOCX).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: false,
                    mode: ExportMode::ExactIfUnchanged,
                },
            )
            .unwrap_err();
        assert!(matches!(error, crate::IoError::ExportFailed { .. }));
    }

    #[test]
    fn an_invalid_zip_is_not_selected_by_suffix() {
        let registry = builtin_registry();
        assert!(matches!(
            registry.detect(DetectionRequest {
                bytes: b"PK-not-a-valid-package",
                selection: FormatSelection::Auto,
                file_name_hint: Some("document.docx"),
                mime_hint: Some(DOCX_MIME),
            }),
            Err(crate::IoError::UnsupportedFormat { requested: None })
        ));
    }
}
