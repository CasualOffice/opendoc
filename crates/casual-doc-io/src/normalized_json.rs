//! Built-in adapter for strict normalized schema-v1 JSON snapshots.

use casual_doc_model::{SnapshotLimits, v1::Document};

use crate::{
    AdapterError, CompatibilityEntry, CompatibilityReport, DocumentResources, ExportArtifact,
    ExportMode, ExportRequest, FeatureLocation, FormatDescriptor, FormatExporter, FormatId,
    FormatImporter, FormatProfile, ImportArtifact, ImportRequest, ModelOutcome, ProbeRequest,
    ProbeResult, RetentionOutcome, SourceEnvelope, formats,
};

const JSON_MIME: &str = "application/vnd.casualoffice.document+json";

#[derive(Debug)]
struct JsonSourceState {
    original_bytes: Option<Vec<u8>>,
}

/// Built-in strict normalized schema-v1 JSON adapter.
#[derive(Clone, Debug)]
pub struct NormalizedJsonAdapter {
    descriptor: FormatDescriptor,
    limits: SnapshotLimits,
}

impl NormalizedJsonAdapter {
    /// Creates an adapter with explicit normalized-snapshot limits.
    #[must_use]
    pub fn new(limits: SnapshotLimits) -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::NORMALIZED_JSON)
                    .expect("built-in normalized JSON format id is valid"),
                display_name: "OpenDoc Normalized JSON".to_owned(),
                mime_types: vec![JSON_MIME.to_owned(), "application/json".to_owned()],
                extensions: vec!["json".to_owned()],
                can_import: true,
                can_export: true,
                exact_if_unchanged: true,
                preserve_when_safe: false,
            },
            limits,
        }
    }
}

impl Default for NormalizedJsonAdapter {
    fn default() -> Self {
        Self::new(SnapshotLimits::default())
    }
}

impl FormatImporter for NormalizedJsonAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult {
        match Document::from_json(request.bytes, self.limits) {
            Ok(_) => ProbeResult::definite("normalized-json.schema-v1"),
            Err(_) => ProbeResult::no_match("normalized-json.not-valid-v1"),
        }
    }

    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifact, AdapterError> {
        let document = Document::from_json(request.bytes, self.limits)
            .map_err(|error| AdapterError::new(format!("snapshot validation: {error}")))?;
        Ok(ImportArtifact {
            document,
            resources: DocumentResources::default(),
            source: SourceEnvelope::new(
                self.descriptor.id.clone(),
                env!("CARGO_PKG_VERSION").to_owned(),
                JsonSourceState {
                    original_bytes: request.retain_source.then(|| request.bytes.to_vec()),
                },
            ),
            report: CompatibilityReport::default(),
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some("1".to_owned()),
            },
        })
    }
}

impl FormatExporter for NormalizedJsonAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError> {
        let matching_source = request
            .source
            .filter(|source| source.format() == &self.descriptor.id)
            .and_then(SourceEnvelope::state::<JsonSourceState>);
        let bytes = match request.mode {
            ExportMode::ExactIfUnchanged => matching_source
                .filter(|_| request.source_unchanged)
                .and_then(|source| source.original_bytes.clone())
                .ok_or_else(|| {
                    AdapterError::new(
                        "exact export requires matching retained source and an unchanged document",
                    )
                })?,
            ExportMode::Semantic | ExportMode::PreserveWhenSafe => {
                let bytes = request.document.to_json().map_err(|error| {
                    AdapterError::new(format!("snapshot serialization: {error}"))
                })?;
                Document::from_json(&bytes, self.limits).map_err(|error| {
                    AdapterError::new(format!("snapshot output limits: {error}"))
                })?;
                bytes
            }
        };

        let mut report = CompatibilityReport::default();
        if request.mode != ExportMode::ExactIfUnchanged && !request.resources.is_empty() {
            report.entries.push(loss(
                "binary_resources",
                u32::try_from(request.resources.as_map().len()).unwrap_or(u32::MAX),
            ));
        }
        if request.mode != ExportMode::ExactIfUnchanged
            && request.source.is_some()
            && matching_source.is_none()
        {
            report.entries.push(loss("source_envelope", 1));
        }
        report.sort();
        Ok(ExportArtifact {
            bytes,
            report,
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some("1".to_owned()),
            },
            mime_type: JSON_MIME.to_owned(),
            suggested_extension: "json".to_owned(),
        })
    }
}

fn loss(feature: &str, occurrences: u32) -> CompatibilityEntry {
    CompatibilityEntry {
        feature: feature.to_owned(),
        occurrences,
        location: FeatureLocation {
            local_name: Some(feature.to_owned()),
            ..FeatureLocation::default()
        },
        model_outcome: ModelOutcome::Omitted,
        retention_outcome: RetentionOutcome::NotRetained,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_V1: &[u8] = br#"{
        "schemaVersion": 1,
        "documentId": "00000000000000010000000000000001",
        "body": [{
            "type": "paragraph",
            "id": "00000000000000010000000000000002",
            "properties": {},
            "inlines": [{
                "type": "run",
                "id": "00000000000000010000000000000003",
                "properties": {},
                "text": "hello"
            }]
        }],
        "definitions": {}
    }"#;

    #[test]
    fn strict_v1_import_and_canonical_export_are_deterministic() {
        let adapter = NormalizedJsonAdapter::default();
        assert_eq!(
            adapter.probe(ProbeRequest { bytes: MINIMAL_V1 }),
            ProbeResult::definite("normalized-json.schema-v1")
        );
        let imported = adapter
            .import(ImportRequest {
                bytes: MINIMAL_V1,
                retain_source: true,
            })
            .unwrap();
        let semantic = adapter
            .export(ExportRequest {
                document: &imported.document,
                resources: &imported.resources,
                source: Some(&imported.source),
                source_unchanged: true,
                mode: ExportMode::Semantic,
            })
            .unwrap();
        assert_eq!(
            Document::from_json(&semantic.bytes, SnapshotLimits::default()).unwrap(),
            imported.document
        );
        assert_eq!(
            adapter
                .export(ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: Some(&imported.source),
                    source_unchanged: true,
                    mode: ExportMode::ExactIfUnchanged,
                })
                .unwrap()
                .bytes,
            MINIMAL_V1
        );
    }

    #[test]
    fn strict_schema_and_limits_fail_closed() {
        let adapter = NormalizedJsonAdapter::new(SnapshotLimits {
            max_input_bytes: MINIMAL_V1.len() - 1,
            ..SnapshotLimits::default()
        });
        assert_eq!(
            adapter.probe(ProbeRequest { bytes: MINIMAL_V1 }),
            ProbeResult::no_match("normalized-json.not-valid-v1")
        );
        assert!(
            adapter
                .import(ImportRequest {
                    bytes: MINIMAL_V1,
                    retain_source: false,
                })
                .is_err()
        );
        let unknown = br#"{"schemaVersion":1,"unknown":true}"#;
        assert_eq!(
            NormalizedJsonAdapter::default().probe(ProbeRequest { bytes: unknown }),
            ProbeResult::no_match("normalized-json.not-valid-v1")
        );

        let imported = NormalizedJsonAdapter::default()
            .import(ImportRequest {
                bytes: MINIMAL_V1,
                retain_source: false,
            })
            .unwrap();
        let output_limited = NormalizedJsonAdapter::new(SnapshotLimits {
            max_input_bytes: 1,
            ..SnapshotLimits::default()
        });
        assert!(
            output_limited
                .export(ExportRequest {
                    document: &imported.document,
                    resources: &imported.resources,
                    source: None,
                    source_unchanged: false,
                    mode: ExportMode::Semantic,
                })
                .is_err()
        );
    }
}
