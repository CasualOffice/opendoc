//! Built-in adapter over the existing DOCX pipeline.

use std::sync::Arc;

// Own line, kept out of any sorted block (the repo's parallel-PR rule).
use casual_doc_export::{DocumentStatistics, ExportOptions, PackageKind, export_package_with_options};
use casual_doc_import::{
    FeatureLocation as DocxFeatureLocation, ImportConfig, ImportMode,
    ModelOutcome as DocxModelOutcome, RetainedParts, RetentionOutcome as DocxRetentionOutcome,
    import_package,
};
// Own lines, kept out of any sorted block (the repo's parallel-PR rule).
use casual_doc_import::{RecoveryReport as DocxRecoveryReport, Severity as DocxSeverity};
use casual_doc_odf::{OdfImportLimits, OdfPackageLimits};
use casual_doc_ooxml::{DocxPackage, PackageLimits};
use casual_doc_ooxml::{PackageRepair, repair_archive};
use casual_doc_rtf::RtfLimits;

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, ExportArtifact,
    ExportMode, ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId,
    FormatImporter, FormatProfile, FormatRegistry, ImportArtifact, ImportRequest, ModelOutcome,
    NormalizedJsonAdapter, OdtAdapter, PlainTextAdapter, PlainTextLimits, ProbeRequest,
    ProbeResult, RetentionOutcome, SourceEnvelope, formats,
};
// Own line, kept out of any sorted block (the repo's parallel-PR rule).
use crate::{RecoveryReport, RepairSeverity, SourceRepair};

const DOCX_MIME: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";

#[derive(Debug)]
struct DocxSourceState {
    original_bytes: Option<Vec<u8>>,
    retained_parts: RetainedParts,
    /// The import findings whose `preserved` claim only the verbatim source
    /// snapshot licensed, restated as what a regenerating save does to them:
    /// `not-retained` (`109` FID-AT-07).
    ///
    /// The import report says `preserved` for these because an unchanged file
    /// saved exactly keeps them. A save that regenerates the parts from the
    /// model — every save except [`ExportMode::ExactIfUnchanged`] — does not,
    /// so that save's report names each one. Empty for a semantic import, whose
    /// report already says `not-retained` for the same detail.
    lost_on_regeneration: Vec<CompatibilityEntry>,
    /// The `docProps/app.xml` statistics as they were read, so a save of an
    /// EDITED document can leave out the ones that still hold them (`109`
    /// FID-AT-04). `None` when the source had no application properties.
    source_statistics: Option<DocumentStatistics>,
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

    /// # Detection of damaged input
    ///
    /// A strict admission is tried first, and a damaged package falls through to
    /// the same best-effort open [`FormatImporter::import`] performs. Without
    /// that second attempt the recovery below would be unreachable: detection
    /// runs before import, so a truncated `.docx` would be reported as an
    /// unrecognised format and never reach an importer that could recover it.
    ///
    /// It does not widen what this adapter claims. The best-effort open still
    /// has to find a part that could be a WordprocessingML main document, which
    /// no ODF, RTF or plain-text source has — so a damaged file of another
    /// format still gets `no_match` here and is still detected by its own
    /// adapter.
    fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult {
        if DocxPackage::open(request.bytes, self.package_limits).is_ok() {
            return ProbeResult::definite("docx.opc.office-document");
        }
        let repaired = repair_archive(request.bytes);
        let bytes = repaired.as_deref().unwrap_or(request.bytes);
        match DocxPackage::open_recovering(bytes, self.package_limits) {
            Ok(_) => ProbeResult::definite("docx.opc.office-document.recovered"),
            Err(_) => ProbeResult::no_match("docx.opc.not-admitted"),
        }
    }

    /// # Opening damaged input
    ///
    /// A damaged `.docx` **opens**, and what was repaired is reported in
    /// [`ImportArtifact::recovery`]. The ladder is: strict admission; a rebuilt
    /// ZIP directory ([`repair_archive`]) for a truncated file; a best-effort
    /// OPC open that infers a missing content-type manifest or relationship
    /// index; then the importer's own recovery
    /// ([`ImportConfig::recover`]) for damaged XML inside the parts.
    ///
    /// Each rung runs only after the one above it fails, so a well-formed file
    /// takes exactly the path it took before any of this existed.
    ///
    /// Three refusals remain, and each is a sentence rather than a retry: a
    /// macro project part (undecided policy, not an oversight), an encrypted
    /// entry (there is no password to try), and bytes holding nothing that could
    /// be a Word document.
    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifact, AdapterError> {
        let strict = DocxPackage::open(request.bytes, self.package_limits);
        let repaired_archive = match &strict {
            Ok(_) => None,
            Err(_) => repair_archive(request.bytes),
        };
        let bytes = repaired_archive.as_deref().unwrap_or(request.bytes);
        let mut package_repairs: Vec<PackageRepair> = Vec::new();
        let mut package = match strict {
            Ok(package) => package,
            Err(_) => {
                // The one refusal left, and the sentence a reader gets for it:
                // `PackageError::summary` rather than its `Display`, because a
                // host renders this verbatim and "DOCX ZIP structure is
                // malformed" in front of someone who double-clicked a file is
                // the same defect as showing them an internal error name.
                let (package, repairs) = DocxPackage::open_recovering(bytes, self.package_limits)
                    .map_err(|error| AdapterError::new(error.summary()))?;
                if repaired_archive.is_some() {
                    package_repairs.push(PackageRepair::ArchiveDirectoryRebuilt {
                        entries: u32::try_from(package.entries().len()).unwrap_or(u32::MAX),
                    });
                }
                package_repairs.extend(repairs);
                package
            }
        };
        let mut config = self.import_config;
        config.mode = if request.retain_source {
            ImportMode::Retention
        } else {
            ImportMode::Semantic
        };
        // A package that needed repairing is a file whose parts are likely
        // damaged too, so the importer recovers from the start rather than
        // refusing once and being retried.
        config.recover = true;
        let imported = import_package(&mut package, config)
            .map_err(|error| AdapterError::new(error.summary()))?;

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
                    attribute_name: None,
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
                    attribute_name: None,
                },
                model_outcome: ModelOutcome::Omitted,
                retention_outcome: RetentionOutcome::NotRetained,
            });
        }
        report.sort();
        let lost_on_regeneration = lost_on_regeneration(&imported.report, &imported.ledger);
        let source_statistics = imported
            .document
            .properties()
            .map(|properties| DocumentStatistics::of(&properties.app));
        let source = SourceEnvelope::new(
            self.descriptor.id.clone(),
            env!("CARGO_PKG_VERSION").to_owned(),
            DocxSourceState {
                original_bytes: request.retain_source.then(|| request.bytes.to_vec()),
                retained_parts: imported.retained_parts,
                lost_on_regeneration,
                source_statistics,
            },
        );
        Ok(ImportArtifact {
            document: imported.document,
            resources,
            source,
            report,
            recovery: convert_recovery(&package_repairs, &imported.recovery),
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

        // An edited document's source statistics describe the text it no longer
        // has (`109` FID-AT-04); every regenerating save is told which they were.
        let options = match matching_source.and_then(|source| source.source_statistics) {
            Some(statistics) if !request.source_unchanged => {
                ExportOptions::default().statistics_stale_since(statistics)
            }
            _ => ExportOptions::default(),
        };
        let (bytes, mut report) = match request.mode {
            // Semantic: regenerate everything from the model and carry no opaque
            // part. The writer's own findings now travel with the bytes instead of
            // being thrown away behind a `CompatibilityReport::default()`
            // (FID-R-01), and the side-table this mode deliberately does not carry
            // is named too, because "the user asked for a semantic save" does not
            // make the dropped parts less dropped.
            ExportMode::Semantic => {
                let exported = export_package_with_options(
                    request.document,
                    request.resources.as_map(),
                    &empty_retained,
                    PackageKind::Document,
                    options,
                )
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
                if let Some(source) = matching_source {
                    report
                        .entries
                        .extend(source.lost_on_regeneration.iter().cloned());
                }
                (exported.bytes, report)
            }
            ExportMode::PreserveWhenSafe => {
                let retained = matching_source
                    .map(|source| &source.retained_parts)
                    .unwrap_or(&empty_retained);
                // A retained part DERIVED from the content (the thumbnail,
                // Word 2010's `stylesWithEffects.xml`) contradicts an edited
                // document, so after an edit it is left behind — with its
                // relationship — and named, never carried stale and never dropped
                // in silence (`105` FID-R-05). An unedited document keeps it: the
                // picture is still of this document.
                let (edited_retained, invalidated) = if request.source_unchanged {
                    (None, Vec::new())
                } else {
                    let (kept, invalidated) = retained.invalidated_by_edit();
                    (Some(kept), invalidated)
                };
                let exported = export_package_with_options(
                    request.document,
                    request.resources.as_map(),
                    edited_retained.as_ref().unwrap_or(retained),
                    PackageKind::Document,
                    options,
                )
                .map_err(|error| AdapterError::new(format!("semantic writer: {error}")))?;
                let mut report = convert_export_report(&exported.report);
                for part in invalidated {
                    report.entries.push(CompatibilityEntry {
                        feature: part.feature.to_owned(),
                        occurrences: 1,
                        location: FeatureLocation {
                            part_name: Some(part.part_name),
                            namespace: None,
                            local_name: None,
                            attribute_name: None,
                        },
                        model_outcome: ModelOutcome::Omitted,
                        retention_outcome: RetentionOutcome::NotRetained,
                    });
                }
                // This save regenerated every consumed part from the model, so
                // the detail only the source snapshot held is not in it — edited
                // or not, because `source_unchanged` decides which DERIVED parts
                // travel, not whether the body is regenerated. The import report
                // called that detail `preserved`; this is where the reader learns
                // it was not (`109` FID-AT-07).
                if let Some(source) = matching_source {
                    report
                        .entries
                        .extend(source.lost_on_regeneration.iter().cloned());
                }
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
    builtin_registry_with_limits(package_limits, PlainTextLimits::default())
}

/// Creates the built-in registry with BOTH admission policies a host sets: the
/// ZIP policy the DOCX and ODT package adapters share, and the plain-text
/// policy.
///
/// The text policy was previously always [`PlainTextLimits::default`], which is
/// sized for a 64-bit native host. The browser build therefore admitted a text
/// file with up to two million paragraphs, and a 2 MB one was enough to exhaust
/// wasm32 linear memory and abort the module (`docs/104` HF-158).
pub fn builtin_registry_with_limits(
    package_limits: PackageLimits,
    text_limits: PlainTextLimits,
) -> FormatRegistry {
    builtin_registry_with_format_limits(package_limits, text_limits, RtfLimits::default())
}

/// Creates the built-in registry with every host-selected admission policy:
/// the ZIP policy DOCX and ODT share, the plain-text policy, and the RTF
/// policy.
///
/// [`RtfLimits::default`] is already sized for a 32-bit browser host, so
/// [`builtin_registry_with_limits`] stays correct without this; a native host
/// that wants larger RTF documents than the browser can hold raises them here.
pub fn builtin_registry_with_format_limits(
    package_limits: PackageLimits,
    text_limits: PlainTextLimits,
    rtf_limits: RtfLimits,
) -> FormatRegistry {
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
    let adapter = Arc::new(PlainTextAdapter::new(text_limits));
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
    // Import only: no exporter is registered, so `.rtf` never appears in
    // `export_formats()` and the UI cannot offer a save format the engine
    // cannot write (`docs/110` §11 records what a writer would add).
    registry
        .register_importer(Arc::new(crate::RtfAdapter::new(rtf_limits)))
        .expect("built-in RTF importer registration is unique");
    // Export only, and registered here rather than left to each host to opt
    // into: a capability a host must remember to switch on is a capability
    // some host will ship without, which is the "built but not reachable"
    // pattern `docs/105` §9 rule 4 names as the most expensive recurring
    // defect here — the RTF importer landed exactly that way. PDF has no
    // importer, so it never becomes a format the picker offers to open.
    crate::register_pdf_exporter(&mut registry)
        .expect("built-in PDF exporter registration is unique");
    // Export only, and built in for the same reason PDF is: `docs/153`
    // `shell.export-markdown`. ONLYOFFICE's Download-as grid offers `MD`; ours
    // did not. Registered here rather than behind an opt-in so the capability is
    // reachable in every host rather than in the ones that remembered.
    registry
        .register_exporter(Arc::new(crate::MarkdownAdapter::default()))
        .expect("built-in Markdown exporter registration is unique");
    // Export only, built in for the same reason: `docs/153` `shell.export-html`.
    // Theirs is a zip of markup plus sibling image files; ours is one file with
    // the pictures inside it, which is the local-first argument applied to an
    // export.
    registry
        .register_exporter(Arc::new(crate::HtmlAdapter::default()))
        .expect("built-in HTML exporter registration is unique");
    // Export only, built in for the same reason: `docs/153` `shell.export-dotx`.
    registry
        .register_exporter(Arc::new(crate::DotxAdapter::default()))
        .expect("built-in DOTX exporter registration is unique");
    registry
}

/// Lifts a DOCX-layer location into the format-neutral one.
///
/// `fallback_local_name` supplies a local name for a finding the DOCX layer
/// located only by part or not at all — the stable `docx.export.*` id's last
/// dotted segment, which is what the adapter surfaced before the DOCX layer had
/// an element/attribute vocabulary at all (FID-R-03). Where the DOCX layer now
/// names the element, that name wins, because it is the real one.
fn convert_location(
    location: &DocxFeatureLocation,
    fallback_local_name: Option<String>,
) -> FeatureLocation {
    FeatureLocation {
        part_name: location.part_name.clone(),
        namespace: None,
        local_name: location.element.clone().or(fallback_local_name),
        attribute_name: location.attribute.clone(),
    }
}

fn convert_model_outcome(outcome: DocxModelOutcome) -> ModelOutcome {
    match outcome {
        DocxModelOutcome::Mapped => ModelOutcome::Mapped,
        DocxModelOutcome::Degraded => ModelOutcome::Degraded,
        DocxModelOutcome::Omitted => ModelOutcome::Omitted,
    }
}

fn convert_retention_outcome(outcome: DocxRetentionOutcome) -> RetentionOutcome {
    match outcome {
        DocxRetentionOutcome::Preserved => RetentionOutcome::Preserved,
        DocxRetentionOutcome::NotRetained => RetentionOutcome::NotRetained,
        DocxRetentionOutcome::Blocked => RetentionOutcome::Blocked,
        DocxRetentionOutcome::Rejected => RetentionOutcome::Rejected,
        DocxRetentionOutcome::NotApplicable => RetentionOutcome::NotApplicable,
    }
}

/// Lifts the DOCX writer's findings into the format-neutral report.
///
/// Exists because the two layers keep separate types on purpose: the adapter
/// vocabulary is shared by every format, the writer's is DOCX-specific. The
/// disposition itself is not re-decided here — both axes come from the writer's
/// `Disposition` — so a finding cannot mean one thing to the writer and another
/// to the caller.
pub(crate) fn convert_export_report(
    report: &casual_doc_export::CompatibilityReport,
) -> CompatibilityReport {
    let mut converted = CompatibilityReport {
        entries: report
            .entries
            .iter()
            .map(|entry| CompatibilityEntry {
                feature: entry.feature.clone(),
                occurrences: entry.occurrences,
                location: convert_location(
                    &entry.location,
                    entry.feature.rsplit('.').next().map(str::to_owned),
                ),
                model_outcome: convert_model_outcome(entry.model_outcome()),
                retention_outcome: convert_retention_outcome(entry.retention_outcome()),
            })
            .collect(),
    };
    converted.sort();
    converted
}

/// The import findings a save that regenerates the package from the model does
/// not deliver, as that save reports them (`109` FID-AT-07).
///
/// Exactly the entries [`casual_doc_import::CompatibilityReport::held_only_by_source_snapshot`]
/// returns, with the model outcome the import stated and the retention outcome
/// the save produces. The feature identifier and the location are the import's
/// own, so a host that already describes a finding in words describes the save's
/// statement of it the same way.
///
/// Complexity: O(import entries), once per import.
fn lost_on_regeneration(
    report: &casual_doc_import::CompatibilityReport,
    ledger: &casual_doc_import::PreservationLedger,
) -> Vec<CompatibilityEntry> {
    report
        .held_only_by_source_snapshot(ledger)
        .map(|entry| CompatibilityEntry {
            feature: entry.feature.clone(),
            occurrences: entry.occurrences,
            location: convert_location(&entry.location, None),
            model_outcome: convert_model_outcome(entry.model_outcome()),
            retention_outcome: RetentionOutcome::NotRetained,
        })
        .collect()
}

fn convert_report(report: &casual_doc_import::CompatibilityReport) -> CompatibilityReport {
    let mut converted = CompatibilityReport {
        entries: report
            .entries
            .iter()
            .map(|entry| CompatibilityEntry {
                feature: entry.feature.clone(),
                occurrences: entry.occurrences,
                location: convert_location(&entry.location, None),
                model_outcome: convert_model_outcome(entry.model_outcome()),
                retention_outcome: convert_retention_outcome(entry.retention_outcome()),
            })
            .collect(),
    };
    converted.sort();
    converted
}

/// Lifts the package-level and importer-level repairs into one format-neutral
/// recovery report.
///
/// Both halves render their own sentence, because both own their vocabulary: the
/// package layer knows what a rebuilt ZIP directory means and the importer knows
/// what a truncated body means, and a translation table in this crate would be a
/// third copy of each fact that could drift from either.
///
/// Order is package repairs first, then importer repairs. That is the order the
/// damage occurred in, and it is the order a reader needs: "this file's index was
/// rebuilt" explains why the part after it was missing.
fn convert_recovery(
    package_repairs: &[PackageRepair],
    recovery: &DocxRecoveryReport,
) -> RecoveryReport {
    let mut repairs: Vec<SourceRepair> = package_repairs
        .iter()
        .map(|repair| SourceRepair {
            token: repair.token().to_owned(),
            summary: repair.summary(),
            // Every package-level repair is plumbing: the archive index and the
            // OPC manifests carry no document meaning, so rebuilding them puts
            // none at risk. What the rebuild may not have *found* is reported by
            // the importer, as a missing part.
            severity: RepairSeverity::Structural,
            part_name: None,
            occurrences: 1,
        })
        .collect();
    repairs.extend(recovery.repairs().iter().map(|repair| SourceRepair {
        token: repair.kind.token().to_owned(),
        summary: repair.summary(),
        severity: convert_severity(repair.severity()),
        part_name: repair.part.clone(),
        occurrences: repair.occurrences,
    }));
    RecoveryReport { repairs }
}

const fn convert_severity(severity: DocxSeverity) -> RepairSeverity {
    match severity {
        DocxSeverity::Structural => RepairSeverity::Structural,
        DocxSeverity::ContentDropped => RepairSeverity::ContentDropped,
        DocxSeverity::BodyLost => RepairSeverity::BodyLost,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DetectionRequest, ExportRequest, FormatSelection};

    const MINIMAL_DOCX: &[u8] = include_bytes!("../../../fixtures/generated/minimal-valid.docx");

    /// The attribute axis must survive the lift into the format-neutral report
    /// (FID-R-03). A capability that stops at a crate boundary is not reachable,
    /// and this is the boundary every host reads the report through.
    #[test]
    fn an_attribute_level_finding_keeps_its_attribute_at_the_adapter_boundary() {
        use std::io::{Cursor, Write as _};
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipWriter};

        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        // One paragraph carrying the durable identity a semantic save drops.
        let document = br#"<w:document xmlns:w="urn:w" xmlns:w14="urn:w14"><w:body>
            <w:p w14:paraId="0A0A0A0A"><w:r><w:t>x</w:t></w:r></w:p>
        </w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;

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
                    file_name_hint: Some("para-id.docx"),
                    mime_hint: None,
                },
                false,
            )
            .expect("the document is valid");
        let entry = imported
            .report
            .entries
            .iter()
            .find(|entry| entry.feature == "p/@paraId")
            .unwrap_or_else(|| {
                panic!(
                    "the attribute finding reaches the adapter: {:?}",
                    imported
                        .report
                        .entries
                        .iter()
                        .map(|entry| entry.feature.as_str())
                        .collect::<Vec<_>>()
                )
            });
        assert_eq!(entry.location.local_name.as_deref(), Some("p"));
        assert_eq!(entry.location.attribute_name.as_deref(), Some("paraId"));
        assert_eq!(entry.model_outcome, ModelOutcome::Degraded);
    }

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

    /// The part a finding came from survives the lift into the format-neutral
    /// report, which is what every host — and the webapp's findings dialog, via
    /// `importReportJson` — reads (`109` HF-047). The importer splitting a count
    /// by part is worth nothing if this boundary collapses or blanks it.
    #[test]
    fn a_findings_part_survives_the_adapter_boundary() {
        use std::io::{Cursor, Write as _};
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipWriter};

        let lost = r#"<w:r><w:drawing><wp:inline><wp:extent cx="9525" cy="9525"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed="rIdGone"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#;
        let ns = r#"xmlns:w="urn:w" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="urn:wp" xmlns:a="urn:a" xmlns:pic="urn:pic""#;
        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = format!(
            r#"<w:document {ns}><w:body><w:p>{lost}</w:p><w:p>{lost}</w:p><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr></w:body></w:document>"#
        );
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#;
        let header = format!(r#"<w:hdr {ns}><w:p>{lost}</w:p></w:hdr>"#);

        let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", root_rels.as_slice()),
            ("word/document.xml", document.as_bytes()),
            ("word/_rels/document.xml.rels", doc_rels.as_slice()),
            ("word/header1.xml", header.as_bytes()),
        ] {
            zw.start_file(name, opts).unwrap();
            zw.write_all(bytes).unwrap();
        }
        let imported = import_docx(&zw.finish().unwrap().into_inner());
        let mut drawings: Vec<(Option<String>, u32)> = imported
            .report
            .entries
            .iter()
            .filter(|entry| entry.feature == "drawing")
            .map(|entry| (entry.location.part_name.clone(), entry.occurrences))
            .collect();
        drawings.sort();
        assert_eq!(
            drawings,
            vec![
                (Some("word/document.xml".to_owned()), 2),
                (Some("word/header1.xml".to_owned()), 1),
            ],
            "each part's count, named by part, at the boundary hosts read"
        );
    }

    /// A retained part DERIVED from the content is left behind once the content
    /// changes, and the loss is named; an independent one is still carried
    /// (`105` FID-R-05).
    ///
    /// Before this, the side-table carried every retained part through every
    /// save: an edited document went out with a thumbnail of the text it no
    /// longer had — the picture a file browser or document library shows AS the
    /// document — and with Word 2010's `stylesWithEffects.xml`, which Word 2010
    /// reads in preference to the regenerated `styles.xml`.
    #[test]
    fn an_edited_document_leaves_its_stale_derived_parts_behind_and_says_so() {
        use std::io::{Cursor, Read as _, Write as _};
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipArchive, ZipWriter};

        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="jpeg" ContentType="image/jpeg"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/stylesWithEffects.xml" ContentType="application/vnd.ms-word.stylesWithEffects+xml"/><Override PartName="/customXml/item1.xml" ContentType="application/xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail" Target="docProps/thumbnail.jpeg"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.microsoft.com/office/2007/relationships/stylesWithEffects" Target="stylesWithEffects.xml"/><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml" Target="../customXml/item1.xml"/></Relationships>"#;
        let styles_with_effects = br#"<w:styles xmlns:w="urn:w"><w:style w:type="paragraph" w:styleId="Normal"/></w:styles>"#;
        let custom_xml = br#"<root><independent>value</independent></root>"#;
        let thumbnail = b"\xFF\xD8\xFF\xE0 a picture of page one as it was";

        let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", root_rels.as_slice()),
            ("word/document.xml", document.as_slice()),
            ("word/_rels/document.xml.rels", doc_rels.as_slice()),
            ("word/stylesWithEffects.xml", styles_with_effects.as_slice()),
            ("customXml/item1.xml", custom_xml.as_slice()),
            ("docProps/thumbnail.jpeg", thumbnail.as_slice()),
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
                    file_name_hint: Some("derived.docx"),
                    mime_hint: None,
                },
                false,
            )
            .expect("the package opens");
        let save = |source_unchanged: bool| {
            registry
                .export(
                    &FormatId::new(formats::DOCX).unwrap(),
                    ExportRequest {
                        document: &imported.document,
                        resources: &imported.resources,
                        source: Some(&imported.source),
                        source_unchanged,
                        mode: ExportMode::PreserveWhenSafe,
                    },
                )
                .expect("it exports")
        };
        let parts = |bytes: &[u8]| -> std::collections::BTreeMap<String, String> {
            let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("a ZIP");
            (0..archive.len())
                .map(|index| {
                    let mut file = archive.by_index(index).unwrap();
                    let mut text = Vec::new();
                    file.read_to_end(&mut text).unwrap();
                    (
                        file.name().to_owned(),
                        String::from_utf8_lossy(&text).into_owned(),
                    )
                })
                .collect()
        };

        // Unedited: the picture is still of this document, so everything stays.
        let unedited = save(true);
        let unedited_parts = parts(&unedited.bytes);
        for name in [
            "docProps/thumbnail.jpeg",
            "word/stylesWithEffects.xml",
            "customXml/item1.xml",
        ] {
            assert!(
                unedited_parts.contains_key(name),
                "an unedited save keeps {name}"
            );
        }
        assert!(
            unedited.report.entries.is_empty(),
            "nothing is lost from an unedited save, got {:?}",
            unedited.report.entries
        );

        // Edited: the two derived parts leave WITH their relationships, the
        // independent store stays, and each departure is named by part.
        let edited = save(false);
        let edited_parts = parts(&edited.bytes);
        assert!(
            !edited_parts.contains_key("docProps/thumbnail.jpeg"),
            "an edited save must not carry a thumbnail of the text it replaced"
        );
        assert!(
            !edited_parts.contains_key("word/stylesWithEffects.xml"),
            "an edited save must not carry a style sheet Word 2010 prefers over the \
             regenerated one"
        );
        assert!(
            edited_parts.contains_key("customXml/item1.xml"),
            "an independent retained part is still carried"
        );
        let root = &edited_parts["_rels/.rels"];
        let document_rels = &edited_parts["word/_rels/document.xml.rels"];
        let manifest = &edited_parts["[Content_Types].xml"];
        assert!(
            !root.contains("thumbnail"),
            "no relationship is left pointing at the thumbnail: {root}"
        );
        assert!(
            !document_rels.contains("stylesWithEffects"),
            "no relationship is left pointing at stylesWithEffects: {document_rels}"
        );
        assert!(
            !manifest.contains("stylesWithEffects"),
            "no content type is declared for a part the package lacks: {manifest}"
        );
        assert!(
            document_rels.contains("customXml/item1.xml"),
            "the independent part keeps its relationship: {document_rels}"
        );
        let mut stale: Vec<(String, Option<String>, ModelOutcome, RetentionOutcome)> = edited
            .report
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.feature.clone(),
                    entry.location.part_name.clone(),
                    entry.model_outcome,
                    entry.retention_outcome,
                )
            })
            .collect();
        stale.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            stale,
            vec![
                (
                    casual_doc_import::STALE_STYLES_WITH_EFFECTS.to_owned(),
                    Some("word/stylesWithEffects.xml".to_owned()),
                    ModelOutcome::Omitted,
                    RetentionOutcome::NotRetained,
                ),
                (
                    casual_doc_import::STALE_THUMBNAIL.to_owned(),
                    Some("docProps/thumbnail.jpeg".to_owned()),
                    ModelOutcome::Omitted,
                    RetentionOutcome::NotRetained,
                ),
            ],
            "each part left behind is named, by part, and nothing else is reported"
        );

        // And the result is a package this engine opens again.
        let reopened = registry.import(
            DetectionRequest {
                bytes: &edited.bytes,
                selection: FormatSelection::Auto,
                file_name_hint: Some("derived.docx"),
                mime_hint: None,
            },
            false,
        );
        assert!(reopened.is_ok(), "the edited save reopens");
    }

    /// `docProps/app.xml`'s statistics are derived from the content, so an
    /// edited save leaves out the ones still holding the source's values and
    /// names them, while one a host refreshed is written (`109` FID-AT-04).
    ///
    /// Before, an edited save wrote the page and word counts the file had when
    /// it was opened — what a file browser or document library shows AS the
    /// document — exactly as it carried a stale thumbnail before FID-R-05.
    #[test]
    fn an_edited_save_leaves_its_stale_statistics_out_and_says_so() {
        use std::io::{Cursor, Read as _, Write as _};
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipArchive, ZipWriter};

        let content_types = br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#;
        let root_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#;
        let document = br#"<w:document xmlns:w="urn:w"><w:body><w:p><w:r><w:t>x</w:t></w:r></w:p></w:body></w:document>"#;
        let doc_rels = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>"#;
        let app = br#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Template>Normal.dotm</Template><Pages>3</Pages><Words>1200</Words><Characters>6400</Characters><Lines>90</Lines><Paragraphs>30</Paragraphs><CharactersWithSpaces>7500</CharactersWithSpaces><Application>Microsoft Office Word</Application></Properties>"#;
        let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
        let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (name, bytes) in [
            ("[Content_Types].xml", content_types.as_slice()),
            ("_rels/.rels", root_rels.as_slice()),
            ("word/document.xml", document.as_slice()),
            ("word/_rels/document.xml.rels", doc_rels.as_slice()),
            ("docProps/app.xml", app.as_slice()),
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
                    file_name_hint: Some("statistics.docx"),
                    mime_hint: None,
                },
                true,
            )
            .expect("the package opens");
        let save = |document: &casual_doc_model::v1::Document, source_unchanged: bool| {
            registry
                .export(
                    &FormatId::new(formats::DOCX).unwrap(),
                    ExportRequest {
                        document,
                        resources: &imported.resources,
                        source: Some(&imported.source),
                        source_unchanged,
                        mode: ExportMode::PreserveWhenSafe,
                    },
                )
                .expect("it exports")
        };
        let app_xml = |bytes: &[u8]| {
            let mut archive = ZipArchive::new(Cursor::new(bytes)).expect("a ZIP");
            let mut text = String::new();
            archive
                .by_name("docProps/app.xml")
                .expect("app.xml is written")
                .read_to_string(&mut text)
                .unwrap();
            text
        };
        let names_stale = |report: &CompatibilityReport| {
            report.entries.iter().any(|entry| {
                entry.feature == casual_doc_export::STALE_STATISTICS
                    && entry.retention_outcome == RetentionOutcome::NotRetained
            })
        };
        let statistics = [
            "<Pages>3</Pages>",
            "<Words>1200</Words>",
            "<Characters>6400</Characters>",
            "<Lines>90</Lines>",
            "<Paragraphs>30</Paragraphs>",
            "<CharactersWithSpaces>7500</CharactersWithSpaces>",
        ];

        // Unedited: the counts still describe the document.
        let unedited = save(&imported.document, true);
        let written = app_xml(&unedited.bytes);
        for statistic in statistics {
            assert!(written.contains(statistic), "an unedited save keeps {statistic}");
        }
        assert!(!names_stale(&unedited.report));

        // Edited: every count that still holds the source's value is left out,
        // the rest of the part is written, and the save says so.
        let edited = save(&imported.document, false);
        let written = app_xml(&edited.bytes);
        for statistic in statistics {
            assert!(
                !written.contains(statistic),
                "an edited save leaves out {statistic}: {written}"
            );
        }
        assert!(written.contains("<Application>Microsoft Office Word</Application>"));
        assert!(
            names_stale(&edited.report),
            "the save names the left-out statistics: {:?}",
            edited.report.entries
        );

        // A host that refreshed a count gets it written.
        let mut refreshed = imported.document.clone();
        refreshed.properties_mut().app.words = Some(1201);
        let written = app_xml(&save(&refreshed, false).bytes);
        assert!(
            written.contains("<Words>1201</Words>") && !written.contains("<Pages>3</Pages>"),
            "a refreshed count is written, a stale one is not: {written}"
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
