//! Built-in adapter over the existing DOCX pipeline.

use std::sync::Arc;

use casual_doc_export::{write_document, write_document_with_retained_parts};
use casual_doc_import::{
    ImportConfig, ImportMode, ModelOutcome as DocxModelOutcome, RetainedParts,
    RetentionOutcome as DocxRetentionOutcome, import_package,
};
use casual_doc_odf::{OdfImportLimits, OdfPackageLimits};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, ExportArtifact,
    ExportMode, ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId,
    FormatImporter, FormatProfile, FormatRegistry, ImportArtifact, ImportRequest, ModelOutcome,
    NormalizedJsonAdapter, OdtAdapter, PlainTextAdapter, ProbeRequest, ProbeResult,
    RetentionOutcome, SourceEnvelope, formats,
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
        // A media part the package cannot hand back was previously dropped by an
        // `if let Ok(..)` with no else arm, and the writer then emitted an EMPTY
        // part alongside a perfectly valid /image relationship and content-type.
        // The saved file therefore advertised a picture it could not supply: Word
        // drew a broken-image box, and the export reported no loss whatsoever, so
        // the user only discovered it on reopening. This repository's contract is
        // that unsupported or unreadable data is preserved where safe or reported
        // explicitly — never silently discarded.
        let mut unreadable: Vec<String> = Vec::new();
        for (_, reference) in imported.document.definitions().media.iter() {
            match package.read_part(&reference.part_name) {
                Ok(bytes) => {
                    resources.insert(reference.part_name.clone(), bytes);
                }
                // One part can be reached through several relationships, so report
                // the PART once rather than once per reference to it.
                Err(_) if !unreadable.contains(&reference.part_name) => {
                    unreadable.push(reference.part_name.clone());
                }
                Err(_) => {}
            }
        }
        let mut report = convert_report(&imported.report);
        for part_name in unreadable {
            report.entries.push(CompatibilityEntry {
                feature: "docx.media.unreadable-part".to_owned(),
                occurrences: 1,
                location: FeatureLocation {
                    part_name: Some(part_name),
                    namespace: None,
                    local_name: None,
                },
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: RetentionOutcome::NotRetained,
            });
        }
        report.sort();
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
    builtin_registry_with_package_limits(PackageLimits::default())
}

/// Creates the built-in registry with one host-selected ZIP admission policy
/// shared by the DOCX and ODT package adapters.
pub fn builtin_registry_with_package_limits(package_limits: PackageLimits) -> FormatRegistry {
    let mut registry = FormatRegistry::new();
    let adapter = Arc::new(DocxAdapter::new(package_limits, ImportConfig::default()));
    registry
        .register_importer(adapter.clone())
        .expect("built-in DOCX importer registration is unique");
    registry
        .register_exporter(adapter)
        .expect("built-in DOCX exporter registration is unique");
    let adapter = Arc::new(NormalizedJsonAdapter::default());
    registry
        .register_importer(adapter.clone())
        .expect("built-in normalized JSON importer registration is unique");
    registry
        .register_exporter(adapter)
        .expect("built-in normalized JSON exporter registration is unique");
    let adapter = Arc::new(PlainTextAdapter::default());
    registry
        .register_importer(adapter.clone())
        .expect("built-in text importer registration is unique");
    registry
        .register_exporter(adapter)
        .expect("built-in text exporter registration is unique");
    let adapter = Arc::new(OdtAdapter::new(
        OdfPackageLimits {
            package: package_limits,
            ..OdfPackageLimits::default()
        },
        OdfImportLimits::default(),
    ));
    registry
        .register_importer(adapter.clone())
        .expect("built-in ODT importer registration is unique");
    registry
        .register_exporter(adapter)
        .expect("built-in ODT exporter registration is unique");
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

    /// A media part named by a relationship but absent from (or unreadable in)
    /// the package must be REPORTED, not silently skipped. Before this, the
    /// reference survived into the model, the bytes did not, and the writer
    /// emitted an empty part beside a valid /image relationship — a file that
    /// advertises a picture it cannot supply, with an empty loss report.
    #[test]
    fn an_unreadable_media_part_is_reported_as_lost() {
        use std::io::{Cursor, Write as _};
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipWriter};

        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic"><w:body>
            <w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="685800"/>
                <a:graphic><a:graphicData><pic:pic><pic:blipFill>
                    <a:blip r:embed="rId7"/></pic:blipFill></pic:pic></a:graphicData></a:graphic>
            </wp:inline></w:drawing></w:r></w:p>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;

        // Note what is NOT in this archive: `word/media/image1.png`. The
        // relationship names it, so the model carries the reference; the bytes
        // cannot be read.
        let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", root_rels.as_slice()),
            ("word/document.xml", document.as_slice()),
            ("word/_rels/document.xml.rels", doc_rels.as_slice()),
        ] {
            zw.start_file(name, opts).unwrap();
            zw.write_all(bytes).unwrap();
        }
        let source = zw.finish().unwrap().into_inner();

        let registry = builtin_registry();
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: &source,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("missing-media.docx"),
                    mime_hint: None,
                },
                false,
            )
            .expect("the document itself is valid and must still open");

        assert!(
            !imported.document.definitions().media.is_empty(),
            "the reference survives; only its bytes are missing"
        );
        let reported = imported
            .report
            .entries
            .iter()
            .find(|entry| entry.feature == "docx.media.unreadable-part")
            .expect("an unreadable media part must appear in the compatibility report");
        assert_eq!(
            reported.location.part_name.as_deref(),
            Some("word/media/image1.png"),
            "the report names the part that was lost"
        );
        assert_eq!(reported.model_outcome, ModelOutcome::Omitted);
        assert_eq!(reported.retention_outcome, RetentionOutcome::NotRetained);
    }

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
    fn an_invalid_zip_is_not_selected_as_docx_by_suffix() {
        let registry = builtin_registry();
        let detected = registry
            .detect(DetectionRequest {
                bytes: b"PK-not-a-valid-package",
                selection: FormatSelection::Auto,
                file_name_hint: Some("document.docx"),
                mime_hint: Some(DOCX_MIME),
            })
            .unwrap();
        assert_eq!(detected.as_str(), formats::TEXT);

        let error = registry
            .import(
                DetectionRequest {
                    bytes: b"PK-not-a-valid-package",
                    selection: FormatSelection::Explicit(FormatId::new(formats::DOCX).unwrap()),
                    file_name_hint: Some("document.docx"),
                    mime_hint: Some(DOCX_MIME),
                },
                false,
            )
            .unwrap_err();
        assert!(matches!(error, crate::IoError::ImportFailed { .. }));
    }
}
