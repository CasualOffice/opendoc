//! Built-in partial ODT importer/exporter over the bounded ODF pipeline.

use casual_doc_odf::{
    CompatibilityReport as OdfCompatibilityReport, ModelOutcome as OdfModelOutcome, ODT_MIME,
    OdfExportLimits, OdfImportLimits, OdfPackageLimits, OdtPackage,
    RetentionOutcome as OdfRetentionOutcome, write_odt,
};

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, ExportArtifact,
    ExportMode, ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId,
    FormatImporter, FormatProfile, ImportArtifact, ImportRequest, ModelOutcome, ProbeRequest,
    ProbeResult, RetentionOutcome, SourceEnvelope, formats,
};

#[derive(Debug)]
struct OdtSourceState {
    original_bytes: Option<Vec<u8>>,
    version: String,
}

/// Built-in ODT importer for the currently implemented bounded semantic subset.
#[derive(Clone, Debug)]
pub struct OdtAdapter {
    descriptor: FormatDescriptor,
    package_limits: OdfPackageLimits,
    import_limits: OdfImportLimits,
    export_limits: OdfExportLimits,
}

impl OdtAdapter {
    /// Creates an ODT adapter with explicit package and semantic-import limits.
    #[must_use]
    pub fn new(package_limits: OdfPackageLimits, import_limits: OdfImportLimits) -> Self {
        Self::with_limits(package_limits, import_limits, OdfExportLimits::default())
    }

    /// Creates an ODT adapter with explicit package, import, and export limits.
    #[must_use]
    pub fn with_limits(
        package_limits: OdfPackageLimits,
        import_limits: OdfImportLimits,
        export_limits: OdfExportLimits,
    ) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::ODT).expect("built-in ODT format id is valid"),
                display_name: "OpenDocument Text".to_owned(),
                mime_types: vec![ODT_MIME.to_owned()],
                extensions: vec!["odt".to_owned()],
                can_import: true,
                can_export: true,
                exact_if_unchanged: true,
                preserve_when_safe: false,
            },
            package_limits,
            import_limits,
            export_limits,
        }
    }
}

impl Default for OdtAdapter {
    fn default() -> Self {
        Self::new(OdfPackageLimits::default(), OdfImportLimits::default())
    }
}

impl FormatImporter for OdtAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult {
        match OdtPackage::open(request.bytes, self.package_limits) {
            Ok(_) => ProbeResult::definite("odt.odf.text-package"),
            Err(_) => ProbeResult::no_match("odt.odf.not-admitted"),
        }
    }

    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifact, AdapterError> {
        let mut package = OdtPackage::open(request.bytes, self.package_limits)
            .map_err(|error| AdapterError::new(format!("ODT package admission: {error}")))?;
        let version = package.version().as_str().to_owned();
        let imported = package
            .import_document(self.import_limits)
            .map_err(|error| AdapterError::new(format!("ODT semantic import: {error}")))?;
        Ok(ImportArtifact {
            document: imported.document,
            resources: DocumentResources::default(),
            source: SourceEnvelope::new(
                self.descriptor.id.clone(),
                env!("CARGO_PKG_VERSION").to_owned(),
                OdtSourceState {
                    original_bytes: request.retain_source.then(|| request.bytes.to_vec()),
                    version: version.clone(),
                },
            ),
            report: convert_report(&imported.report, request.retain_source),
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some(version),
            },
        })
    }
}

impl FormatExporter for OdtAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError> {
        self.export_limits
            .validate()
            .map_err(|error| AdapterError::new(format!("ODT export limits: {error}")))?;
        let matching_source = request
            .source
            .filter(|source| source.format() == &self.descriptor.id)
            .and_then(SourceEnvelope::state::<OdtSourceState>);

        let (bytes, mut report, version) = match request.mode {
            ExportMode::ExactIfUnchanged => {
                let source = matching_source
                    .filter(|_| request.source_unchanged)
                    .filter(|source| source.original_bytes.is_some())
                    .ok_or_else(|| {
                        AdapterError::new(
                            "exact export requires matching retained source and an unchanged document",
                        )
                    })?;
                let bytes = source.original_bytes.clone().expect("checked above");
                if bytes.len() > self.export_limits.max_package_bytes {
                    return Err(AdapterError::new(format!(
                        "ODT exact export exceeds package byte limit: observed {}, allowed {}",
                        bytes.len(),
                        self.export_limits.max_package_bytes
                    )));
                }
                (
                    bytes,
                    CompatibilityReport::default(),
                    source.version.clone(),
                )
            }
            ExportMode::Semantic | ExportMode::PreserveWhenSafe => {
                let exported = write_odt(request.document, self.export_limits)
                    .map_err(|error| AdapterError::new(format!("ODT semantic export: {error}")))?;
                let mut report = convert_report(&exported.report, false);
                if !request.resources.is_empty() {
                    report.entries.push(export_loss(
                        "odt.export.resources",
                        request.resources.as_map().len(),
                    ));
                }
                if request.mode == ExportMode::PreserveWhenSafe && request.source.is_some() {
                    report
                        .entries
                        .push(export_loss("odt.export.source_envelope", 1));
                }
                (exported.bytes, report, "1.4".to_owned())
            }
        };
        report.sort();
        Ok(ExportArtifact {
            bytes,
            report,
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some(version),
            },
            mime_type: ODT_MIME.to_owned(),
            suggested_extension: "odt".to_owned(),
        })
    }
}

fn export_loss(feature: &str, occurrences: usize) -> CompatibilityEntry {
    CompatibilityEntry {
        feature: feature.to_owned(),
        occurrences: u32::try_from(occurrences).unwrap_or(u32::MAX),
        location: FeatureLocation::default(),
        model_outcome: ModelOutcome::Omitted,
        retention_outcome: RetentionOutcome::NotRetained,
    }
}

fn convert_report(report: &OdfCompatibilityReport, retained_source: bool) -> CompatibilityReport {
    let mut converted = CompatibilityReport {
        entries: report
            .entries
            .iter()
            .map(|entry| CompatibilityEntry {
                feature: entry.feature.clone(),
                occurrences: entry.occurrences,
                location: FeatureLocation {
                    part_name: Some(casual_doc_odf::CONTENT_PART.to_owned()),
                    namespace: feature_namespace(&entry.feature).map(str::to_owned),
                    local_name: entry.feature.rsplit('.').next().map(str::to_owned),
                },
                model_outcome: match entry.model_outcome {
                    OdfModelOutcome::Mapped => ModelOutcome::Mapped,
                    OdfModelOutcome::Degraded => ModelOutcome::Degraded,
                    OdfModelOutcome::Omitted => ModelOutcome::Omitted,
                },
                retention_outcome: match entry.retention_outcome {
                    OdfRetentionOutcome::Preserved => RetentionOutcome::Preserved,
                    OdfRetentionOutcome::NotRetained if retained_source => {
                        RetentionOutcome::Preserved
                    }
                    OdfRetentionOutcome::NotRetained => RetentionOutcome::NotRetained,
                    OdfRetentionOutcome::Blocked => RetentionOutcome::Blocked,
                    OdfRetentionOutcome::Rejected => RetentionOutcome::Rejected,
                    OdfRetentionOutcome::NotApplicable => RetentionOutcome::NotApplicable,
                },
            })
            .collect(),
    };
    converted.sort();
    converted
}

fn feature_namespace(feature: &str) -> Option<&'static str> {
    if feature.contains(".office.") {
        Some("urn:oasis:names:tc:opendocument:xmlns:office:1.0")
    } else if feature.contains(".text.") {
        Some("urn:oasis:names:tc:opendocument:xmlns:text:1.0")
    } else if feature.contains(".script.") {
        Some("urn:oasis:names:tc:opendocument:xmlns:script:1.0")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::CompressionMethod;
    use zip::write::{SimpleFileOptions, ZipWriter};

    use super::*;
    use crate::{DetectionRequest, FormatSelection, IoError, builtin_registry};

    const CONTENT: &[u8] = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.4"><office:body><office:text><text:p text:style-name="Body">Hello ODT</text:p></office:text></office:body></office:document-content>"#;

    fn odt_bytes() -> Vec<u8> {
        let manifest = format!(
            r#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="{ODT_MIME}" manifest:version="1.4"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#
        );
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "mimetype",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(ODT_MIME.as_bytes()).unwrap();
        writer
            .start_file(
                "META-INF/manifest.xml",
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(manifest.as_bytes()).unwrap();
        writer
            .start_file(
                "content.xml",
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .unwrap();
        writer.write_all(CONTENT).unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn builtin_registry_detects_and_imports_odt_despite_misleading_hints() {
        let bytes = odt_bytes();
        let registry = builtin_registry();
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: &bytes,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("misleading.docx"),
                    mime_hint: Some(
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                    ),
                },
                true,
            )
            .unwrap();
        assert_eq!(imported.format.format.as_str(), formats::ODT);
        assert_eq!(imported.format.version.as_deref(), Some("1.4"));
        assert!(imported.resources.is_empty());
        assert_eq!(imported.document.body().len(), 1);
        assert_eq!(imported.source.format().as_str(), formats::ODT);
        assert_eq!(
            imported
                .source
                .state::<OdtSourceState>()
                .and_then(|state| state.original_bytes.as_deref()),
            Some(bytes.as_slice())
        );
        assert!(imported.report.entries.iter().any(|entry| {
            entry.feature == "odf.attribute.text.style-name"
                && entry.location.part_name.as_deref() == Some("content.xml")
                && entry.location.namespace.as_deref()
                    == Some("urn:oasis:names:tc:opendocument:xmlns:text:1.0")
                && entry.retention_outcome == RetentionOutcome::Preserved
        }));
    }

    #[test]
    fn odt_capabilities_advertise_semantic_and_exact_export() {
        let registry = builtin_registry();
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id.as_str() == formats::ODT)
            .unwrap();
        assert!(descriptor.can_import);
        assert!(descriptor.can_export);
        assert!(descriptor.exact_if_unchanged);
        assert!(!descriptor.preserve_when_safe);
        assert!(
            registry
                .export_formats()
                .iter()
                .any(|id| id.as_str() == formats::ODT)
        );
    }

    #[test]
    fn semantic_odt_export_reopens_with_the_same_core_model() {
        let bytes = odt_bytes();
        let registry = builtin_registry();
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: &bytes,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                false,
            )
            .unwrap();
        let expected = imported.document.clone();
        let exported = registry
            .export(
                &FormatId::new(formats::ODT).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: None,
                    source_unchanged: false,
                    mode: ExportMode::Semantic,
                },
            )
            .unwrap();
        assert_eq!(exported.format.version.as_deref(), Some("1.4"));
        assert_eq!(exported.mime_type, ODT_MIME);
        assert_eq!(exported.suggested_extension, "odt");
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
        assert_eq!(reopened.document, expected);
    }

    #[test]
    fn preserving_export_reports_resources_and_unretained_source_state() {
        let bytes = odt_bytes();
        let registry = builtin_registry();
        let mut imported = registry
            .import(
                DetectionRequest {
                    bytes: &bytes,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                true,
            )
            .unwrap();
        imported
            .resources
            .insert("Pictures/example.png".to_owned(), vec![1, 2, 3]);
        let exported = registry
            .export(
                &FormatId::new(formats::ODT).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: false,
                    mode: ExportMode::PreserveWhenSafe,
                },
            )
            .unwrap();
        assert!(exported.report.entries.iter().any(|entry| {
            entry.feature == "odt.export.resources"
                && entry.occurrences == 1
                && entry.model_outcome == ModelOutcome::Omitted
                && entry.retention_outcome == RetentionOutcome::NotRetained
        }));
        assert!(exported.report.entries.iter().any(|entry| {
            entry.feature == "odt.export.source_envelope"
                && entry.model_outcome == ModelOutcome::Omitted
                && entry.retention_outcome == RetentionOutcome::NotRetained
        }));
    }

    #[test]
    fn exact_odt_export_requires_retained_unchanged_source() {
        let bytes = odt_bytes();
        let registry = builtin_registry();
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: &bytes,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                true,
            )
            .unwrap();
        let exact = registry
            .export(
                &FormatId::new(formats::ODT).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: true,
                    mode: ExportMode::ExactIfUnchanged,
                },
            )
            .unwrap();
        assert_eq!(exact.bytes, bytes);
        assert_eq!(exact.format.version.as_deref(), Some("1.4"));

        let error = registry
            .export(
                &FormatId::new(formats::ODT).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: false,
                    mode: ExportMode::ExactIfUnchanged,
                },
            )
            .unwrap_err();
        assert!(matches!(error, IoError::ExportFailed { .. }));

        let without_retention = registry
            .import(
                DetectionRequest {
                    bytes: &bytes,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                false,
            )
            .unwrap();
        let error = registry
            .export(
                &FormatId::new(formats::ODT).unwrap(),
                ExportRequest {
                    document: &without_retention.document,
                    resources: &without_retention.resources,
                    source: Some(&without_retention.source),
                    source_unchanged: true,
                    mode: ExportMode::ExactIfUnchanged,
                },
            )
            .unwrap_err();
        assert!(matches!(error, IoError::ExportFailed { .. }));

        let bounded_adapter = OdtAdapter::with_limits(
            OdfPackageLimits::default(),
            OdfImportLimits::default(),
            OdfExportLimits {
                max_package_bytes: bytes.len() - 1,
                ..OdfExportLimits::default()
            },
        );
        let bounded_import = bounded_adapter
            .import(ImportRequest {
                bytes: &bytes,
                retain_source: true,
            })
            .unwrap();
        let error = bounded_adapter
            .export(ExportRequest {
                document: &bounded_import.document,
                resources: &bounded_import.resources,
                source: Some(&bounded_import.source),
                source_unchanged: true,
                mode: ExportMode::ExactIfUnchanged,
            })
            .unwrap_err();
        assert!(error.message().contains("package byte limit"));
    }

    #[test]
    fn explicit_odt_selection_still_requires_full_admission() {
        let registry = builtin_registry();
        let error = registry
            .import(
                DetectionRequest {
                    bytes: b"not an ODT",
                    selection: FormatSelection::Explicit(FormatId::new(formats::ODT).unwrap()),
                    file_name_hint: Some("document.odt"),
                    mime_hint: Some(ODT_MIME),
                },
                false,
            )
            .unwrap_err();
        assert!(matches!(error, IoError::ImportFailed { .. }));
    }
}
