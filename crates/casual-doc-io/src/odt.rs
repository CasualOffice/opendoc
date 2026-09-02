//! Built-in partial ODT importer/exporter over the bounded ODF pipeline.

use casual_doc_odf::{
    CompatibilityReport as OdfCompatibilityReport, ModelOutcome as OdfModelOutcome, ODT_MIME,
    OdfExportLimits, OdfImportLimits, OdfPackageLimits, OdfRetainedParts, OdtPackage,
    RetentionOutcome as OdfRetentionOutcome, referenced_retained_parts, write_odt,
    write_odt_with_retained_parts,
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
    /// Source parts retained for edit-tolerant preservation (empty unless the
    /// import requested retention).
    retained: OdfRetainedParts,
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
                preserve_when_safe: true,
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
        // Picture bytes, always — not only when the source is being retained.
        //
        // These were collected into the ODT-private retained side-table and the
        // artifact's `resources` was left empty, so the bytes existed but nothing
        // outside the ODT round-trip could see them. Exporting an .odt full of
        // photographs as .docx then wrote every image as a ZERO-BYTE part beside
        // a perfectly valid relationship, with no entry in the compatibility
        // report — on a headline capability of the format registry.
        //
        // The same collection is reused rather than a second one written: it
        // already applies the manifest lookup, the reserved/active-content
        // exclusions and the size budgets.
        let media_parts = package
            .retained_media_parts(&imported.document, self.import_limits)
            .map_err(|error| AdapterError::new(format!("ODT media: {error}")))?;
        let mut resources = DocumentResources::default();
        for (name, part) in &media_parts.parts {
            resources.insert(name.clone(), part.bytes.clone());
        }
        let retained = if request.retain_source {
            media_parts
        } else {
            OdfRetainedParts::default()
        };
        Ok(ImportArtifact {
            document: imported.document,
            resources,
            source: SourceEnvelope::new(
                self.descriptor.id.clone(),
                env!("CARGO_PKG_VERSION").to_owned(),
                OdtSourceState {
                    original_bytes: request.retain_source.then(|| request.bytes.to_vec()),
                    version: version.clone(),
                    retained,
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
                let preserving = request.mode == ExportMode::PreserveWhenSafe;
                let retained = matching_source
                    .map(|source| &source.retained)
                    .filter(|r| !r.is_empty());
                let exported = if preserving && let Some(retained) = retained {
                    write_odt_with_retained_parts(request.document, retained, self.export_limits)
                        .map_err(|error| {
                            AdapterError::new(format!("ODT preserving export: {error}"))
                        })?
                } else {
                    write_odt(request.document, self.export_limits).map_err(|error| {
                        AdapterError::new(format!("ODT semantic export: {error}"))
                    })?
                };
                let mut report = convert_report(&exported.report, false);
                if !request.resources.is_empty() {
                    report.entries.push(export_loss(
                        "odt.export.resources",
                        request.resources.as_map().len(),
                    ));
                }
                if preserving {
                    // Report exactly how many parts were repackaged (the subset
                    // the current document references), not the whole source set.
                    let carried = retained
                        .map(|retained| referenced_retained_parts(request.document, retained).len())
                        .unwrap_or(0);
                    if carried != 0 {
                        report
                            .entries
                            .push(export_preserved("odt.export.retained_parts", carried));
                    } else if request.source.is_some() {
                        report
                            .entries
                            .push(export_loss("odt.export.source_envelope", 1));
                    }
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

fn export_preserved(feature: &str, occurrences: usize) -> CompatibilityEntry {
    CompatibilityEntry {
        feature: feature.to_owned(),
        occurrences: u32::try_from(occurrences).unwrap_or(u32::MAX),
        location: FeatureLocation::default(),
        model_outcome: ModelOutcome::Mapped,
        retention_outcome: RetentionOutcome::Preserved,
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
    } else if feature.contains(".xlink.") {
        Some("http://www.w3.org/1999/xlink")
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
    use crate::{
        DetectionRequest, FormatSelection, IoError, builtin_registry,
        builtin_registry_with_package_limits,
    };
    use casual_doc_ooxml::PackageLimits;

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

    fn odt_bytes_with_image() -> Vec<u8> {
        let content = br#"<office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" xmlns:xlink="http://www.w3.org/1999/xlink" office:version="1.4"><office:body><office:text><text:p><draw:frame svg:width="2cm" svg:height="2cm"><draw:image xlink:href="Pictures/img.png"/></draw:frame></text:p></office:text></office:body></office:document-content>"#;
        let manifest = format!(
            r#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.4"><manifest:file-entry manifest:full-path="/" manifest:media-type="{ODT_MIME}" manifest:version="1.4"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="Pictures/img.png" manifest:media-type="image/png"/></manifest:manifest>"#
        );
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        writer.start_file("mimetype", stored).unwrap();
        writer.write_all(ODT_MIME.as_bytes()).unwrap();
        writer
            .start_file("META-INF/manifest.xml", deflated)
            .unwrap();
        writer.write_all(manifest.as_bytes()).unwrap();
        writer.start_file("content.xml", deflated).unwrap();
        writer.write_all(content).unwrap();
        writer.start_file("Pictures/img.png", deflated).unwrap();
        writer.write_all(b"\x89PNG\r\nIMG").unwrap();
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn retained_source_captures_referenced_image_bytes() {
        let bytes = odt_bytes_with_image();
        let adapter = OdtAdapter::default();
        let imported = adapter
            .import(ImportRequest {
                bytes: &bytes,
                retain_source: true,
            })
            .unwrap();
        let state = imported.source.state::<OdtSourceState>().unwrap();
        let part = state
            .retained
            .parts
            .get("Pictures/img.png")
            .expect("retained image part");
        assert_eq!(part.media_type, "image/png");
        assert_eq!(part.bytes, b"\x89PNG\r\nIMG");

        // PreserveWhenSafe re-emits the image via the semantic (edit-tolerant)
        // path and repackages the retained bytes.
        let exported = adapter
            .export(ExportRequest {
                document: &imported.document,
                resources: &imported.resources,
                source: Some(&imported.source),
                source_unchanged: false,
                mode: ExportMode::PreserveWhenSafe,
            })
            .unwrap();
        assert!(exported.report.entries.iter().any(|entry| {
            entry.feature == "odt.export.retained_parts"
                && entry.occurrences == 1
                && entry.retention_outcome == RetentionOutcome::Preserved
        }));

        // The written package reopens with the image reference intact...
        let reopened = adapter
            .import(ImportRequest {
                bytes: &exported.bytes,
                retain_source: true,
            })
            .unwrap();
        assert_eq!(reopened.document.definitions().media.len(), 1);
        // ...and re-preserving it is byte-identical (fixed point).
        let reexported = adapter
            .export(ExportRequest {
                document: &reopened.document,
                resources: &reopened.resources,
                source: Some(&reopened.source),
                source_unchanged: false,
                mode: ExportMode::PreserveWhenSafe,
            })
            .unwrap();
        assert_eq!(reexported.bytes, exported.bytes);

        // Without retention nothing is captured.
        let plain = adapter
            .import(ImportRequest {
                bytes: &bytes,
                retain_source: false,
            })
            .unwrap();
        assert!(
            plain
                .source
                .state::<OdtSourceState>()
                .unwrap()
                .retained
                .is_empty()
        );
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
            entry.feature == "odf.style.unresolved"
                && entry.location.part_name.as_deref() == Some("content.xml")
                && entry.location.namespace.is_none()
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
        assert!(descriptor.preserve_when_safe);
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

    #[test]
    fn shared_host_package_limit_applies_to_odt_admission() {
        let bytes = odt_bytes();
        let registry = builtin_registry_with_package_limits(PackageLimits {
            max_input_bytes: bytes.len() - 1,
            ..PackageLimits::default()
        });
        let error = registry
            .import(
                DetectionRequest {
                    bytes: &bytes,
                    selection: FormatSelection::Explicit(FormatId::new(formats::ODT).unwrap()),
                    file_name_hint: None,
                    mime_hint: None,
                },
                false,
            )
            .unwrap_err();
        assert!(matches!(error, IoError::ImportFailed { .. }));
    }

    /// Converting an .odt to .docx must carry the picture BYTES across.
    ///
    /// The importer collected them into the ODT-private retained side-table and
    /// left the artifact's `resources` empty, so the bytes existed but nothing
    /// outside the ODT round-trip could reach them. The DOCX writer then hit
    /// `media.get(..).unwrap_or_default()` and emitted a ZERO-BYTE part beside a
    /// valid relationship: every image destroyed, silently, with no entry in the
    /// compatibility report — on a headline capability of the format registry.
    #[test]
    fn odt_pictures_survive_conversion_to_docx() {
        const RICH_ODT: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.odt");

        let registry = builtin_registry();
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: RICH_ODT,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("pictures.odt"),
                    mime_hint: None,
                },
                false,
            )
            .expect("the fixture imports");

        assert!(
            !imported.document.definitions().media.is_empty(),
            "the fixture must actually carry a picture"
        );
        for (_, reference) in imported.document.definitions().media.iter() {
            let bytes = imported
                .resources
                .get(&reference.part_name)
                .unwrap_or_else(|| panic!("no bytes for {}", reference.part_name));
            assert!(
                !bytes.is_empty(),
                "{} came across empty — the export would write a zero-byte image",
                reference.part_name
            );
        }
    }
}
