//! Deterministic adapter registration, detection, and dispatch.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::{
    AdapterError, ExportArtifact, ExportRequest, FormatDescriptor, FormatId, ImportArtifact,
    ImportRequest, IoError,
};

/// Confidence returned by a bounded byte/package probe.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProbeConfidence {
    /// The input does not match this format.
    NoMatch,
    /// The input has non-authoritative evidence for this format.
    Possible,
    /// The input contains authoritative format evidence.
    Definite,
}

/// Deterministic result of one adapter probe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProbeResult {
    /// Match confidence.
    pub confidence: ProbeConfidence,
    /// Stable adapter-defined evidence code; never source text.
    pub evidence: &'static str,
}

impl ProbeResult {
    /// Creates a no-match result.
    #[must_use]
    pub const fn no_match(evidence: &'static str) -> Self {
        Self {
            confidence: ProbeConfidence::NoMatch,
            evidence,
        }
    }

    /// Creates a possible-match result.
    #[must_use]
    pub const fn possible(evidence: &'static str) -> Self {
        Self {
            confidence: ProbeConfidence::Possible,
            evidence,
        }
    }

    /// Creates a definite-match result.
    #[must_use]
    pub const fn definite(evidence: &'static str) -> Self {
        Self {
            confidence: ProbeConfidence::Definite,
            evidence,
        }
    }
}

/// Bounded probe input. Adapters must not mutate external state while probing.
#[derive(Clone, Copy, Debug)]
pub struct ProbeRequest<'a> {
    /// Untrusted input bytes. Adapter-level admission limits still apply.
    pub bytes: &'a [u8],
}

/// Import adapter contract.
pub trait FormatImporter: Send + Sync {
    /// Returns this adapter's stable descriptor.
    fn descriptor(&self) -> &FormatDescriptor;
    /// Performs a deterministic, read-only format probe.
    fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult;
    /// Imports bytes after this adapter has been selected.
    fn import(&self, request: ImportRequest<'_>) -> Result<ImportArtifact, AdapterError>;
}

/// Export adapter contract.
pub trait FormatExporter: Send + Sync {
    /// Returns this adapter's stable descriptor.
    fn descriptor(&self) -> &FormatDescriptor;
    /// Exports one immutable normalized document snapshot.
    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError>;
}

/// Caller selection policy for import.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum FormatSelection {
    /// Detect the format from authoritative byte/package evidence.
    #[default]
    Auto,
    /// Use exactly one registered adapter, which must still validate the bytes.
    Explicit(FormatId),
}

/// Inputs to deterministic format detection.
#[derive(Clone, Debug)]
pub struct DetectionRequest<'a> {
    /// Untrusted source bytes.
    pub bytes: &'a [u8],
    /// Selection policy.
    pub selection: FormatSelection,
    /// Optional filename hint used only to resolve possible-match ties.
    pub file_name_hint: Option<&'a str>,
    /// Optional MIME hint used only to resolve possible-match ties.
    pub mime_hint: Option<&'a str>,
}

#[derive(Default)]
struct RegisteredFormat {
    descriptor: Option<FormatDescriptor>,
    importer: Option<Arc<dyn FormatImporter>>,
    exporter: Option<Arc<dyn FormatExporter>>,
}

/// Deterministically ordered importer/exporter registry.
#[derive(Default)]
pub struct FormatRegistry {
    formats: BTreeMap<FormatId, RegisteredFormat>,
}

impl fmt::Debug for FormatRegistry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FormatRegistry")
            .field("formats", &self.formats.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl FormatRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an importer, rejecting duplicates and descriptor conflicts.
    pub fn register_importer(&mut self, importer: Arc<dyn FormatImporter>) -> Result<(), IoError> {
        let descriptor = importer.descriptor().clone();
        let id = descriptor.id.clone();
        let registered = self.formats.entry(id.clone()).or_default();
        check_descriptor(registered, &descriptor)?;
        if registered.importer.is_some() {
            return Err(IoError::DuplicateAdapter {
                format: id,
                capability: "import",
            });
        }
        registered.descriptor = Some(descriptor);
        registered.importer = Some(importer);
        Ok(())
    }

    /// Registers an exporter, rejecting duplicates and descriptor conflicts.
    pub fn register_exporter(&mut self, exporter: Arc<dyn FormatExporter>) -> Result<(), IoError> {
        let descriptor = exporter.descriptor().clone();
        let id = descriptor.id.clone();
        let registered = self.formats.entry(id.clone()).or_default();
        check_descriptor(registered, &descriptor)?;
        if registered.exporter.is_some() {
            return Err(IoError::DuplicateAdapter {
                format: id,
                capability: "export",
            });
        }
        registered.descriptor = Some(descriptor);
        registered.exporter = Some(exporter);
        Ok(())
    }

    /// Returns all registered descriptors in ascending format-id order.
    #[must_use]
    pub fn descriptors(&self) -> Vec<&FormatDescriptor> {
        self.formats
            .values()
            .filter_map(|registered| registered.descriptor.as_ref())
            .collect()
    }

    /// Returns exporter format IDs in ascending order.
    #[must_use]
    pub fn export_formats(&self) -> Vec<&FormatId> {
        self.formats
            .iter()
            .filter_map(|(id, registered)| registered.exporter.as_ref().map(|_| id))
            .collect()
    }

    /// Selects an importer deterministically without importing the document.
    pub fn detect(&self, request: DetectionRequest<'_>) -> Result<FormatId, IoError> {
        if let FormatSelection::Explicit(format) = request.selection {
            return self
                .formats
                .get(&format)
                .and_then(|registered| registered.importer.as_ref())
                .map(|_| format.clone())
                .ok_or(IoError::UnsupportedFormat {
                    requested: Some(format),
                });
        }

        let mut matches = Vec::new();
        for (id, registered) in &self.formats {
            let Some(importer) = &registered.importer else {
                continue;
            };
            let probe = importer.probe(ProbeRequest {
                bytes: request.bytes,
            });
            if probe.confidence != ProbeConfidence::NoMatch {
                matches.push((id, registered, probe.confidence));
            }
        }
        let Some(highest) = matches.iter().map(|(_, _, confidence)| *confidence).max() else {
            return Err(IoError::UnsupportedFormat { requested: None });
        };
        let top: Vec<_> = matches
            .into_iter()
            .filter(|(_, _, confidence)| *confidence == highest)
            .collect();
        if top.len() == 1 {
            return Ok(top[0].0.clone());
        }
        if highest == ProbeConfidence::Possible {
            let hinted: Vec<_> = top
                .iter()
                .filter(|(_, registered, _)| {
                    registered.descriptor.as_ref().is_some_and(|descriptor| {
                        descriptor.matches_hint(request.file_name_hint, request.mime_hint)
                    })
                })
                .collect();
            if hinted.len() == 1 {
                return Ok(hinted[0].0.clone());
            }
        }
        Err(IoError::AmbiguousFormat {
            candidates: top.into_iter().map(|(id, _, _)| id.clone()).collect(),
        })
    }

    /// Detects and imports a document atomically.
    pub fn import(
        &self,
        detection: DetectionRequest<'_>,
        retain_source: bool,
    ) -> Result<ImportArtifact, IoError> {
        let format = self.detect(detection.clone())?;
        let importer = self.formats[&format]
            .importer
            .as_ref()
            .expect("detected importer must remain registered");
        importer
            .import(ImportRequest {
                bytes: detection.bytes,
                retain_source,
            })
            .map_err(|source| IoError::ImportFailed { format, source })
    }

    /// Exports through one explicitly selected target format.
    pub fn export(
        &self,
        format: &FormatId,
        request: ExportRequest<'_>,
    ) -> Result<ExportArtifact, IoError> {
        let exporter = self
            .formats
            .get(format)
            .and_then(|registered| registered.exporter.as_ref())
            .ok_or_else(|| IoError::UnsupportedFormat {
                requested: Some(format.clone()),
            })?;
        exporter
            .export(request)
            .map_err(|source| IoError::ExportFailed {
                format: format.clone(),
                source,
            })
    }
}

fn check_descriptor(
    registered: &RegisteredFormat,
    descriptor: &FormatDescriptor,
) -> Result<(), IoError> {
    if registered
        .descriptor
        .as_ref()
        .is_some_and(|existing| existing != descriptor)
    {
        return Err(IoError::DescriptorConflict {
            format: descriptor.id.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::FormatId;

    struct ProbeImporter {
        descriptor: FormatDescriptor,
        result: ProbeResult,
    }

    impl FormatImporter for ProbeImporter {
        fn descriptor(&self) -> &FormatDescriptor {
            &self.descriptor
        }

        fn probe(&self, _request: ProbeRequest<'_>) -> ProbeResult {
            self.result
        }

        fn import(&self, _request: ImportRequest<'_>) -> Result<ImportArtifact, AdapterError> {
            Err(AdapterError::new("probe-only test adapter"))
        }
    }

    fn descriptor(id: &str, extension: &str) -> FormatDescriptor {
        FormatDescriptor {
            id: FormatId::new(id).unwrap(),
            display_name: id.to_owned(),
            mime_types: vec![format!("application/x-{extension}")],
            extensions: vec![extension.to_owned()],
            can_import: true,
            can_export: false,
            exact_if_unchanged: false,
            preserve_when_safe: false,
        }
    }

    fn register_probe(
        registry: &mut FormatRegistry,
        id: &str,
        extension: &str,
        result: ProbeResult,
    ) {
        registry
            .register_importer(Arc::new(ProbeImporter {
                descriptor: descriptor(id, extension),
                result,
            }))
            .unwrap();
    }

    #[test]
    fn definite_ties_are_ambiguous_and_stably_sorted() {
        let mut registry = FormatRegistry::new();
        register_probe(&mut registry, "z.format", "z", ProbeResult::definite("z"));
        register_probe(&mut registry, "a.format", "a", ProbeResult::definite("a"));
        let error = registry
            .detect(DetectionRequest {
                bytes: b"same",
                selection: FormatSelection::Auto,
                file_name_hint: Some("document.z"),
                mime_hint: None,
            })
            .unwrap_err();
        assert_eq!(
            error,
            IoError::AmbiguousFormat {
                candidates: vec![
                    FormatId::new("a.format").unwrap(),
                    FormatId::new("z.format").unwrap(),
                ],
            }
        );
    }

    #[test]
    fn hints_only_break_possible_ties() {
        let mut registry = FormatRegistry::new();
        register_probe(&mut registry, "a.format", "a", ProbeResult::possible("a"));
        register_probe(&mut registry, "b.format", "b", ProbeResult::possible("b"));
        let detected = registry
            .detect(DetectionRequest {
                bytes: b"same",
                selection: FormatSelection::Auto,
                file_name_hint: Some("document.b"),
                mime_hint: None,
            })
            .unwrap();
        assert_eq!(detected, FormatId::new("b.format").unwrap());
    }

    #[test]
    fn explicit_selection_neither_probes_nor_accepts_unknown_formats() {
        let mut registry = FormatRegistry::new();
        register_probe(
            &mut registry,
            "a.format",
            "a",
            ProbeResult::no_match("never"),
        );
        let requested = FormatId::new("a.format").unwrap();
        assert_eq!(
            registry
                .detect(DetectionRequest {
                    bytes: b"not-a",
                    selection: FormatSelection::Explicit(requested.clone()),
                    file_name_hint: None,
                    mime_hint: None,
                })
                .unwrap(),
            requested
        );
        let unknown = FormatId::new("missing.format").unwrap();
        assert_eq!(
            registry
                .detect(DetectionRequest {
                    bytes: b"anything",
                    selection: FormatSelection::Explicit(unknown.clone()),
                    file_name_hint: None,
                    mime_hint: None,
                })
                .unwrap_err(),
            IoError::UnsupportedFormat {
                requested: Some(unknown)
            }
        );
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut registry = FormatRegistry::new();
        register_probe(&mut registry, "a.format", "a", ProbeResult::definite("a"));
        let duplicate = Arc::new(ProbeImporter {
            descriptor: descriptor("a.format", "a"),
            result: ProbeResult::definite("a"),
        });
        assert!(matches!(
            registry.register_importer(duplicate),
            Err(IoError::DuplicateAdapter {
                capability: "import",
                ..
            })
        ));
    }
}
