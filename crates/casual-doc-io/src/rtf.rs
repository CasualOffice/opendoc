//! Built-in Rich Text Format importer over the bounded RTF pipeline.
//!
//! Import only. The descriptor advertises `can_export: false` and no exporter
//! is registered, so `.rtf` never appears in
//! [`FormatRegistry::export_formats`](crate::FormatRegistry::export_formats).
//! `docs/110-RTF-IMPORT-PROFILE.md` §11 records what a writer would add.

use casual_doc_rtf::RTF_MIME;
use casual_doc_rtf::RtfCompatibilityReport as RtfReport;
use casual_doc_rtf::RtfLimits;
use casual_doc_rtf::RtfModelOutcome;
use casual_doc_rtf::RtfRetentionOutcome;
use casual_doc_rtf::import_rtf;
use casual_doc_rtf::probe_rtf;

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, FeatureLocation,
    FormatDescriptor, FormatId, FormatImporter, FormatProfile, ImportArtifact, ImportRequest,
    ModelOutcome, ProbeRequest, ProbeResult, RetentionOutcome, SourceEnvelope, formats,
};

/// Source bytes retained so a future RTF writer can offer exact export.
#[derive(Debug)]
struct RtfSourceState {
    original_bytes: Option<Vec<u8>>,
}

/// Built-in RTF importer for the bounded subset in doc 110.
#[derive(Clone, Debug)]
pub struct RtfAdapter {
    descriptor: FormatDescriptor,
    limits: RtfLimits,
}

impl RtfAdapter {
    /// Creates an adapter with an explicit admission policy.
    #[must_use]
    pub fn new(limits: RtfLimits) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::RTF).expect("built-in RTF format id is valid"),
                display_name: "Rich Text Format".to_owned(),
                mime_types: vec![RTF_MIME.to_owned(), "text/rtf".to_owned()],
                extensions: vec!["rtf".to_owned()],
                can_import: true,
                can_export: false,
                // Both stay false until a writer exists. Advertising an export
                // capability the registry cannot satisfy would make the
                // descriptor a claim rather than a contract.
                exact_if_unchanged: false,
                preserve_when_safe: false,
            },
            limits,
        }
    }
}

impl RtfAdapter {
    /// Returns the original bytes an import retained, when `source` is an RTF
    /// envelope produced with `retain_source`.
    ///
    /// RTF has no writer yet, so nothing inside the registry consumes these
    /// bytes. They are still worth keeping — and worth exposing — because a
    /// host that offers "download the original file" needs them, and a
    /// retention nothing can read is a retention nobody can prove happened.
    /// When the writer lands it reads the same envelope for
    /// [`ExportMode::ExactIfUnchanged`](crate::ExportMode::ExactIfUnchanged).
    /// A non-RTF envelope yields `None`. There is deliberately no
    /// format-identifier comparison here: the envelope's payload is keyed by
    /// concrete type and `RtfSourceState` is private to this module, so the
    /// downcast is what refuses a foreign envelope. A second check on top of
    /// it would be a branch no input can reach — and an untestable branch is
    /// the kind of "defence" that gets cited as evidence and is not one.
    #[must_use]
    pub fn retained_source_bytes<'a>(&self, source: &'a SourceEnvelope) -> Option<&'a [u8]> {
        source.state::<RtfSourceState>()?.original_bytes.as_deref()
    }
}

impl Default for RtfAdapter {
    fn default() -> Self {
        Self::new(RtfLimits::default())
    }
}

impl FormatImporter for RtfAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult {
        // Definite, not possible: the plain-text adapter reports `Possible` for
        // any valid UTF-8, and an RTF stream is valid UTF-8, so a weaker
        // confidence here would open every `.rtf` as prose full of control
        // words.
        if probe_rtf(request.bytes) {
            ProbeResult::definite("rtf.signature")
        } else {
            ProbeResult::no_match("rtf.no-signature")
        }
    }

    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifact, AdapterError> {
        let imported = import_rtf(request.bytes, self.limits)
            .map_err(|error| AdapterError::new(format!("RTF import: {error}")))?;
        let mut resources = DocumentResources::default();
        for (name, bytes) in imported.resources {
            resources.insert(name, bytes);
        }
        let version = format!("rtf{}", imported.version);
        Ok(ImportArtifact {
            document: imported.document,
            resources,
            source: SourceEnvelope::new(
                self.descriptor.id.clone(),
                env!("CARGO_PKG_VERSION").to_owned(),
                RtfSourceState {
                    original_bytes: request.retain_source.then(|| request.bytes.to_vec()),
                },
            ),
            report: convert_report(&imported.report),
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some(version),
            },
        })
    }
}

fn convert_report(report: &RtfReport) -> CompatibilityReport {
    let mut converted = CompatibilityReport {
        entries: report
            .entries
            .iter()
            .map(|entry| CompatibilityEntry {
                feature: entry.feature.clone(),
                occurrences: entry.occurrences,
                location: FeatureLocation {
                    part_name: None,
                    namespace: None,
                    local_name: Some(
                        entry
                            .control_word
                            .clone()
                            .unwrap_or_else(|| entry.feature.clone()),
                    ),
                    attribute_name: None,
                },
                model_outcome: match entry.model_outcome {
                    RtfModelOutcome::Mapped => ModelOutcome::Mapped,
                    RtfModelOutcome::Degraded => ModelOutcome::Degraded,
                    RtfModelOutcome::Omitted => ModelOutcome::Omitted,
                },
                retention_outcome: match entry.retention_outcome {
                    RtfRetentionOutcome::Preserved => RetentionOutcome::Preserved,
                    RtfRetentionOutcome::NotRetained => RetentionOutcome::NotRetained,
                    RtfRetentionOutcome::NotApplicable => RetentionOutcome::NotApplicable,
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
    use crate::{DetectionRequest, FormatSelection, IoError, PlainTextAdapter, builtin_registry};

    // Note the doubled space after `\b0`: a control word swallows exactly one
    // following space as its delimiter, so `\b0 x` is "x" and `\b0  x` is " x".
    const SAMPLE: &[u8] = br"{\rtf1\ansi\ansicpg1252 Hello \b world\b0  \'93quoted\'94\par}";

    #[test]
    fn the_registry_detects_and_imports_rtf() {
        let registry = builtin_registry();
        let detected = registry
            .detect(DetectionRequest {
                bytes: SAMPLE,
                selection: FormatSelection::Auto,
                file_name_hint: Some("memo.rtf"),
                mime_hint: None,
            })
            .expect("RTF is detected from its signature alone");
        assert_eq!(detected.as_str(), formats::RTF);

        let artifact = registry
            .import(
                DetectionRequest {
                    bytes: SAMPLE,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                false,
            )
            .expect("import succeeds");
        assert_eq!(artifact.format.format.as_str(), formats::RTF);
        let text = artifact
            .document
            .body()
            .iter()
            .filter_map(|block| match block {
                casual_doc_model::v1::BlockNode::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })
            .flat_map(|paragraph| paragraph.inlines.iter())
            .filter_map(|inline| match inline {
                casual_doc_model::v1::InlineNode::Run(run) => Some(run.text.as_str()),
                _ => None,
            })
            .collect::<String>();
        assert_eq!(text, "Hello world \u{201c}quoted\u{201d}");
    }

    #[test]
    fn source_bytes_are_retained_only_when_asked_for() {
        // The envelope holds the original bytes so a future writer can offer
        // `ExactIfUnchanged` without any import-side change (`docs/110` §11).
        // Nothing reads them yet, so without this guard the retention could be
        // quietly wrong — or quietly absent — until the writer landed.
        let adapter = RtfAdapter::default();
        let retained = adapter
            .import(ImportRequest {
                bytes: SAMPLE,
                retain_source: true,
            })
            .expect("import succeeds");
        assert_eq!(
            adapter.retained_source_bytes(&retained.source),
            Some(SAMPLE)
        );

        let plain = adapter
            .import(ImportRequest {
                bytes: SAMPLE,
                retain_source: false,
            })
            .expect("import succeeds");
        assert_eq!(
            adapter.retained_source_bytes(&plain.source),
            None,
            "retention is opt-in; holding a copy of every opened file is not free"
        );

        // A foreign envelope must not be mined for bytes just because its
        // adapter-private payload happens to have a compatible shape.
        let text = PlainTextAdapter::default()
            .import(ImportRequest {
                bytes: b"not rtf",
                retain_source: true,
            })
            .expect("text import succeeds");
        assert_eq!(adapter.retained_source_bytes(&text.source), None);
    }

    #[test]
    fn rtf_beats_plain_text_because_its_probe_is_authoritative() {
        // The sample is valid UTF-8, so the text adapter matches it too. If the
        // RTF probe were merely `Possible`, detection would be ambiguous and
        // the file would fail to open at all, or open as control-word soup.
        let registry = builtin_registry();
        assert_eq!(
            registry
                .detect(DetectionRequest {
                    bytes: SAMPLE,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                })
                .unwrap()
                .as_str(),
            formats::RTF
        );
    }

    #[test]
    fn rtf_advertises_import_only() {
        let registry = builtin_registry();
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id.as_str() == formats::RTF)
            .expect("the RTF descriptor is registered");
        assert!(descriptor.can_import);
        assert!(!descriptor.can_export);
        assert!(!descriptor.exact_if_unchanged);
        assert!(!descriptor.preserve_when_safe);
        assert!(
            !registry
                .export_formats()
                .iter()
                .any(|id| id.as_str() == formats::RTF),
            "no RTF exporter exists yet, so the registry must not offer one"
        );
    }

    #[test]
    fn explicitly_selecting_rtf_for_non_rtf_bytes_fails_loudly() {
        let registry = builtin_registry();
        let rtf = FormatId::new(formats::RTF).unwrap();
        let error = registry
            .import(
                DetectionRequest {
                    bytes: b"this is not rtf",
                    selection: FormatSelection::Explicit(rtf.clone()),
                    file_name_hint: None,
                    mime_hint: None,
                },
                false,
            )
            .unwrap_err();
        assert!(matches!(error, IoError::ImportFailed { format, .. } if format == rtf));
    }

    #[test]
    fn dropped_constructs_reach_the_format_neutral_report() {
        let registry = builtin_registry();
        let source = br"{\rtf1\ansi{\header hidden}{\footnote note}body\par}";
        let artifact = registry
            .import(
                DetectionRequest {
                    bytes: source,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                false,
            )
            .expect("import succeeds");
        for feature in ["rtf.section.header-footer", "rtf.note"] {
            let entry = artifact
                .report
                .entries
                .iter()
                .find(|entry| entry.feature == feature)
                .unwrap_or_else(|| panic!("{feature} must be reported, not silently dropped"));
            assert_eq!(entry.model_outcome, ModelOutcome::Omitted);
            assert_eq!(entry.retention_outcome, RetentionOutcome::NotRetained);
        }
    }

    #[test]
    fn picture_bytes_reach_the_artifact_resources() {
        // The ODT adapter once collected image bytes into a format-private
        // side table and left `resources` empty, so cross-format export wrote
        // zero-byte image parts. This asserts RTF does not repeat that.
        let registry = builtin_registry();
        let source = br"{\rtf1\ansi{\pict\pngblip\picwgoal1440\pichgoal1440 89504e47}\par}";
        let artifact = registry
            .import(
                DetectionRequest {
                    bytes: source,
                    selection: FormatSelection::Auto,
                    file_name_hint: None,
                    mime_hint: None,
                },
                false,
            )
            .expect("import succeeds");
        assert_eq!(artifact.resources.as_map().len(), 1);
        let (name, bytes) = artifact.resources.as_map().iter().next().unwrap();
        assert_eq!(bytes.as_slice(), &[0x89, 0x50, 0x4e, 0x47]);
        assert!(
            artifact
                .document
                .definitions()
                .media
                .iter()
                .any(|(_, media)| &media.part_name == name),
            "the model must reference the resource the adapter emitted"
        );
    }
}
