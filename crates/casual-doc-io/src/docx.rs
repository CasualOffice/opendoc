//! Built-in adapter over the existing DOCX pipeline.

use std::sync::Arc;

use casual_doc_export::{
    Disposition as ExportDisposition, ModelOutcome as ExportModelOutcome,
    RetentionOutcome as ExportRetentionOutcome, export_document,
    export_document_with_retained_parts,
};
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
        // Embedded font faces (`.odttf`). The model carries only the reference
        // metadata (font key, relationship, part name), exactly as it does for
        // images, so the obfuscated bytes must be carried alongside or the
        // document's own faces are unreachable: layout de-obfuscates them from
        // here (`casual-doc-layout` `font_registry::register_embedded_fonts`),
        // and the semantic writer re-emits the `.odttf` parts from the same map
        // — without this it wrote empty parts.
        let mut unreadable_fonts: Vec<String> = Vec::new();
        for font in &imported.document.definitions().font_table {
            for (_, face) in font.embedded.faces() {
                if resources.get(&face.part_name).is_some() {
                    continue;
                }
                match package.read_part(&face.part_name) {
                    Ok(bytes) => {
                        resources.insert(face.part_name.clone(), bytes);
                    }
                    Err(_) if !unreadable_fonts.contains(&face.part_name) => {
                        unreadable_fonts.push(face.part_name.clone());
                    }
                    Err(_) => {}
                }
            }
        }
        let mut report = convert_report(&imported.report);
        for part_name in unreadable_fonts {
            report.entries.push(CompatibilityEntry {
                feature: "docx.font.embedded.unreadable-part".to_owned(),
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
            // Semantic: regenerate everything from the model and carry no opaque
            // part. The writer's own findings now travel with the bytes instead of
            // being thrown away behind a `CompatibilityReport::default()`
            // (FID-R-01), and the side-table this mode deliberately does not carry
            // is named too, because "the user asked for a semantic save" does not
            // make the dropped parts less dropped.
            ExportMode::Semantic => {
                let exported = export_document(request.document, request.resources.as_map())
                    .map_err(|error| AdapterError::new(format!("semantic writer: {error}")))?;
                let mut report = convert_export_report(&exported.report);
                let dropped = matching_source
                    .map(|source| source.retained_parts.parts.len())
                    .unwrap_or(0);
                if dropped != 0 {
                    report.entries.push(CompatibilityEntry {
                        feature: "docx.export.retained_parts".to_owned(),
                        occurrences: u32::try_from(dropped).unwrap_or(u32::MAX),
                        location: FeatureLocation::default(),
                        model_outcome: ModelOutcome::Omitted,
                        retention_outcome: RetentionOutcome::NotRetained,
                    });
                }
                (exported.bytes, report)
            }
            ExportMode::PreserveWhenSafe => {
                let retained = matching_source
                    .map(|source| &source.retained_parts)
                    .unwrap_or(&empty_retained);
                let exported = export_document_with_retained_parts(
                    request.document,
                    request.resources.as_map(),
                    retained,
                )
                .map_err(|error| AdapterError::new(format!("semantic writer: {error}")))?;
                let mut report = convert_export_report(&exported.report);
                if request.source.is_some() && matching_source.is_none() {
                    report.entries.push(CompatibilityEntry {
                        feature: "source_envelope".to_owned(),
                        occurrences: 1,
                        location: FeatureLocation::default(),
                        model_outcome: ModelOutcome::Omitted,
                        retention_outcome: RetentionOutcome::NotRetained,
                    });
                }
                (exported.bytes, report)
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

/// Lifts the DOCX writer's findings into the format-neutral report.
///
/// Exists because the two layers keep separate types on purpose: the adapter
/// vocabulary is shared by every format, the writer's is DOCX-specific. The
/// disposition itself is not re-decided here — both axes come from the writer's
/// [`ExportDisposition`], so a finding cannot mean one thing to the writer and
/// another to the caller.
fn convert_export_report(report: &casual_doc_export::CompatibilityReport) -> CompatibilityReport {
    let mut converted = CompatibilityReport {
        entries: report
            .entries
            .iter()
            .map(|entry| CompatibilityEntry {
                feature: entry.feature.clone(),
                occurrences: entry.occurrences,
                location: FeatureLocation {
                    part_name: entry.part_name.clone(),
                    namespace: None,
                    local_name: entry.feature.rsplit('.').next().map(str::to_owned),
                },
                model_outcome: convert_model_outcome(entry.disposition),
                retention_outcome: convert_retention_outcome(entry.disposition),
            })
            .collect(),
    };
    converted.sort();
    converted
}

fn convert_model_outcome(disposition: ExportDisposition) -> ModelOutcome {
    match disposition.model_outcome() {
        ExportModelOutcome::Mapped => ModelOutcome::Mapped,
        ExportModelOutcome::Degraded => ModelOutcome::Degraded,
        ExportModelOutcome::Omitted => ModelOutcome::Omitted,
    }
}

fn convert_retention_outcome(disposition: ExportDisposition) -> RetentionOutcome {
    match disposition.retention_outcome() {
        ExportRetentionOutcome::Preserved => RetentionOutcome::Preserved,
        ExportRetentionOutcome::NotRetained => RetentionOutcome::NotRetained,
        ExportRetentionOutcome::Blocked => RetentionOutcome::Blocked,
        ExportRetentionOutcome::Rejected => RetentionOutcome::Rejected,
        ExportRetentionOutcome::NotApplicable => RetentionOutcome::NotApplicable,
    }
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

    /// Builds a package whose `fontTable.xml` embeds one `.odttf` face.
    /// `include_face` controls whether the `.odttf` part is actually in the
    /// archive, so the same builder produces the present and the missing case.
    fn package_with_embedded_font(include_face: bool, face_bytes: &[u8]) -> Vec<u8> {
        use std::io::{Cursor, Write as _};
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipWriter};

        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="odttf" ContentType="application/vnd.openxmlformats-officedocument.obfuscatedFont"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/fontTable.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.fontTable+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:rPr><w:rFonts w:ascii="Embedded Probe"/></w:rPr><w:t>probe</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/fontTable" Target="fontTable.xml"/></Relationships>"#;
        let font_table = br#"<w:fonts xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:font w:name="Embedded Probe"><w:embedRegular r:id="rIdF1" w:fontKey="{3EEE3167-E5B8-4798-AE48-EA6B71E31D4D}"/></w:font></w:fonts>"#;
        let font_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdF1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/font" Target="fonts/font1.odttf"/></Relationships>"#;

        let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let mut parts: Vec<(&str, &[u8])> = vec![
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", root_rels.as_slice()),
            ("word/document.xml", document.as_slice()),
            ("word/_rels/document.xml.rels", doc_rels.as_slice()),
            ("word/fontTable.xml", font_table.as_slice()),
            ("word/_rels/fontTable.xml.rels", font_rels.as_slice()),
        ];
        if include_face {
            parts.push(("word/fonts/font1.odttf", face_bytes));
        }
        for (name, bytes) in parts {
            zw.start_file(name, opts).unwrap();
            zw.write_all(bytes).unwrap();
        }
        zw.finish().unwrap().into_inner()
    }

    fn import_docx(bytes: &[u8]) -> crate::ImportArtifact {
        builtin_registry()
            .import(
                DetectionRequest {
                    bytes,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("embedded-font.docx"),
                    mime_hint: None,
                },
                false,
            )
            .expect("the package is valid and must open")
    }

    /// An embedded font's obfuscated bytes must be carried out of the package
    /// alongside the reference metadata. The model holds only the part name, so
    /// without this the document's own faces are unreachable: layout cannot
    /// de-obfuscate them (FID-L-01) and the semantic writer re-emits an EMPTY
    /// `.odttf` part.
    #[test]
    fn an_embedded_font_part_is_carried_into_the_resources() {
        let face = b"OBFUSCATED-FACE-BYTES-0123456789abcdef".as_slice();
        let imported = import_docx(&package_with_embedded_font(true, face));

        let font = imported
            .document
            .definitions()
            .font_table
            .iter()
            .find(|font| font.name == "Embedded Probe")
            .expect("the font table entry is modeled");
        let embedded = font
            .embedded
            .regular
            .as_ref()
            .expect("the embedded regular face is modeled");
        assert_eq!(embedded.part_name, "word/fonts/font1.odttf");
        assert_eq!(
            imported.resources.get(&embedded.part_name),
            Some(face),
            "the obfuscated .odttf bytes travel with the import",
        );
        assert!(
            imported
                .report
                .entries
                .iter()
                .all(|entry| !entry.feature.starts_with("docx.font.embedded.")),
            "a readable face raises no finding",
        );
    }

    /// A declared-but-absent `.odttf` is reported rather than silently dropped:
    /// the run will render in a substitute and the host must be able to say so.
    #[test]
    fn a_missing_embedded_font_part_is_reported() {
        let imported = import_docx(&package_with_embedded_font(false, b""));
        assert!(
            imported.resources.get("word/fonts/font1.odttf").is_none(),
            "there are no bytes to carry",
        );
        let reported = imported
            .report
            .entries
            .iter()
            .find(|entry| entry.feature == "docx.font.embedded.unreadable-part")
            .expect("an unreadable embedded font part must be reported");
        assert_eq!(
            reported.location.part_name.as_deref(),
            Some("word/fonts/font1.odttf"),
        );
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

    /// The adapter must hand back what the writer reported, and must report
    /// nothing for a document the writer emits in full.
    ///
    /// Both halves matter. Before this, `ExportMode::Semantic` returned
    /// `CompatibilityReport::default()` unconditionally (FID-R-01), so export loss
    /// on the primary format could not be surfaced at all; a report that instead
    /// fires on every healthy save is the failure in the other direction, and one
    /// callers learn to ignore.
    #[test]
    fn an_ordinary_document_exports_through_the_adapter_with_an_empty_report() {
        const RICH: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");
        let registry = builtin_registry();
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: RICH,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("rich.docx"),
                    mime_hint: None,
                },
                false,
            )
            .expect("an ordinary Word document opens");
        assert!(
            imported.resources.get("word/media/image1.png").is_some(),
            "the fixture must carry its picture bytes, or an empty report proves nothing"
        );
        let exported = registry
            .export(
                &FormatId::new(formats::DOCX).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: false,
                    mode: ExportMode::Semantic,
                },
            )
            .expect("it exports");
        assert!(
            exported.report.entries.is_empty(),
            "an ordinary document must export with no findings, got {:?}",
            exported.report.entries
        );
    }

    /// Media bytes the caller cannot supply are reported on the EXPORT side too,
    /// with the part named, and the package does not pretend otherwise.
    ///
    /// The import half of this already reported (`docx.media.unreadable-part`);
    /// the export half returned a default report while writing a zero-byte part
    /// behind a live `/image` relationship (FID-R-06).
    #[test]
    fn media_bytes_the_caller_cannot_supply_are_reported_on_export() {
        const RICH: &[u8] = include_bytes!("../../../fixtures/corpus/real-producer-rich.docx");
        let registry = builtin_registry();
        let imported = registry
            .import(
                DetectionRequest {
                    bytes: RICH,
                    selection: FormatSelection::Auto,
                    file_name_hint: Some("rich.docx"),
                    mime_hint: None,
                },
                false,
            )
            .expect("an ordinary Word document opens");
        // The picture is still declared by the model; its bytes are not offered.
        let exported = registry
            .export(
                &FormatId::new(formats::DOCX).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &DocumentResources::default(),
                    source: None,
                    source_unchanged: false,
                    mode: ExportMode::Semantic,
                },
            )
            .expect("it still exports");
        let entry = exported
            .report
            .entries
            .iter()
            .find(|entry| entry.feature == "docx.export.media.missing_bytes")
            .expect("the export must report the picture it could not write");
        assert_eq!(
            entry.location.part_name.as_deref(),
            Some("word/media/image1.png"),
            "the finding names the part"
        );
        assert_eq!(entry.model_outcome, ModelOutcome::Omitted);
        assert_eq!(entry.retention_outcome, RetentionOutcome::NotRetained);
    }

    /// A semantic save drops the opaque parts the preserving save carries, and
    /// must say so.
    ///
    /// "The caller asked for `ExportMode::Semantic`" explains the drop; it does
    /// not make the dropped `customXml` less gone. Doc 35 admits `not-retained`
    /// only when a report records it, so the same export that silently shed the
    /// side-table now names it — and the preserving mode, which really does carry
    /// the part, stays silent.
    #[test]
    fn a_semantic_save_reports_the_opaque_parts_a_preserving_save_would_carry() {
        use std::io::{Cursor, Write as _};
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipWriter};

        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/customXml/item1.xml" ContentType="application/xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="customXml/item1.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
        let custom_xml = br#"<root><bound>value</bound></root>"#;

        let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", root_rels.as_slice()),
            ("word/document.xml", document.as_slice()),
            ("word/_rels/document.xml.rels", doc_rels.as_slice()),
            ("customXml/item1.xml", custom_xml.as_slice()),
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
                    file_name_hint: Some("bound.docx"),
                    mime_hint: None,
                },
                false,
            )
            .expect("the package opens");

        let semantic = registry
            .export(
                &FormatId::new(formats::DOCX).unwrap(),
                ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: false,
                    mode: ExportMode::Semantic,
                },
            )
            .expect("it exports");
        let entry = semantic
            .report
            .entries
            .iter()
            .find(|entry| entry.feature == "docx.export.retained_parts")
            .expect("a semantic save must report the side-table it drops");
        assert_eq!(entry.occurrences, 1, "one opaque part was dropped");
        assert_eq!(entry.model_outcome, ModelOutcome::Omitted);
        assert_eq!(entry.retention_outcome, RetentionOutcome::NotRetained);

        let preserving = registry
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
            .expect("it exports");
        assert!(
            preserving.report.entries.is_empty(),
            "the preserving save carries the part, so it must report nothing, got {:?}",
            preserving.report.entries
        );
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
