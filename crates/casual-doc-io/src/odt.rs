//! Built-in partial ODT importer over the bounded ODF pipeline.

use casual_doc_odf::{
    CompatibilityReport as OdfCompatibilityReport, ModelOutcome as OdfModelOutcome, ODT_MIME,
    OdfImportLimits, OdfPackageLimits, OdtPackage, RetentionOutcome as OdfRetentionOutcome,
};

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, FeatureLocation,
    FormatDescriptor, FormatId, FormatImporter, FormatProfile, ImportArtifact, ImportRequest,
    ModelOutcome, ProbeRequest, ProbeResult, RetentionOutcome, SourceEnvelope, formats,
};

/// Built-in ODT importer for the currently implemented bounded semantic subset.
#[derive(Clone, Debug)]
pub struct OdtAdapter {
    descriptor: FormatDescriptor,
    package_limits: OdfPackageLimits,
    import_limits: OdfImportLimits,
}

impl OdtAdapter {
    /// Creates an ODT adapter with explicit package and semantic-import limits.
    #[must_use]
    pub fn new(package_limits: OdfPackageLimits, import_limits: OdfImportLimits) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::ODT).expect("built-in ODT format id is valid"),
                display_name: "OpenDocument Text".to_owned(),
                mime_types: vec![ODT_MIME.to_owned()],
                extensions: vec!["odt".to_owned()],
                can_import: true,
                can_export: false,
                exact_if_unchanged: false,
                preserve_when_safe: false,
            },
            package_limits,
            import_limits,
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
                request.retain_source.then(|| request.bytes.to_vec()),
            ),
            report: convert_report(&imported.report, request.retain_source),
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some(version),
            },
        })
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
                .state::<Option<Vec<u8>>>()
                .and_then(Option::as_deref),
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
    fn odt_capabilities_are_import_only_until_the_writer_lands() {
        let registry = builtin_registry();
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id.as_str() == formats::ODT)
            .unwrap();
        assert!(descriptor.can_import);
        assert!(!descriptor.can_export);
        assert!(!descriptor.exact_if_unchanged);
        assert!(!descriptor.preserve_when_safe);
        assert!(
            !registry
                .export_formats()
                .iter()
                .any(|id| id.as_str() == formats::ODT)
        );
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
