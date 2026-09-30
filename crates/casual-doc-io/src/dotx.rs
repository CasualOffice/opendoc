//! Built-in export-only Word template (`.dotx`) adapter.
//!
//! `docs/153` `shell.export-dotx`. Their Download-as grid offers DOTX; ours had
//! no way to save a document as a template at all, so a house style had to be
//! kept as a `.docx` somebody remembered not to type into.
//!
//! # Why this is eight lines of logic and not a writer
//!
//! ECMA-376 gives a template its own main-part content type and changes nothing
//! else: the parts, the relationships, the styles and the body are identical to
//! a document's, and Word decides "open a copy of this" from that one string.
//! So this adapter asks the DOCX writer for the same package with
//! [`PackageKind::Template`] rather than serializing a second time. Two writers
//! for one rule diverge (`SKILL` §8).
//!
//! # Export only
//!
//! `can_import` is false. A `.dotx` would import perfectly well — it is an OPC
//! package the reader already understands — but the registry must not claim an
//! importer that is not registered, and opening a template as a template (a new
//! unnamed copy rather than the file itself) is a host behaviour this crate has
//! no way to express. Reported rather than half-built.

use casual_doc_export::{PackageKind, export_package};
use casual_doc_import::RetainedParts;

use crate::{
    AdapterError, CompatibilityReport, ExportArtifact, ExportMode, ExportRequest, FormatDescriptor,
    FormatExporter, FormatId, FormatProfile, formats,
};

/// The media type registered for a Word template.
pub const DOTX_MIME: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.template";

/// Built-in export-only `.dotx` adapter.
#[derive(Clone, Debug)]
pub struct DotxAdapter {
    descriptor: FormatDescriptor,
}

impl Default for DotxAdapter {
    fn default() -> Self {
        Self {
            descriptor: FormatDescriptor {
                id: FormatId::new(formats::DOTX).expect("built-in dotx id is valid"),
                display_name: "Word Template".to_owned(),
                mime_types: vec![DOTX_MIME.to_owned()],
                extensions: vec!["dotx".to_owned()],
                can_import: false,
                can_export: true,
                exact_if_unchanged: false,
                preserve_when_safe: false,
            },
        }
    }
}

impl FormatExporter for DotxAdapter {
    fn descriptor(&self) -> &FormatDescriptor {
        &self.descriptor
    }

    fn export(&self, request: ExportRequest<'_>) -> Result<ExportArtifact, AdapterError> {
        request
            .document
            .validate()
            .map_err(|error| AdapterError::new(format!("normalized model: {error}")))?;
        if request.mode == ExportMode::ExactIfUnchanged {
            return Err(AdapterError::new(
                "a template is written from the model, so there are no retained source bytes \
                 to return exactly; ask for the semantic mode",
            ));
        }
        // A template deliberately carries no retained side-table. Retention
        // re-emits the parts of the package a document was IMPORTED from, and a
        // template is a new file made from the model; carrying another
        // document's opaque parts into it would put a glossary, a thumbnail and
        // somebody else's custom XML into the house style. The loss is reported
        // below rather than left for the reader to discover.
        let exported = export_package(
            request.document,
            request.resources.as_map(),
            &RetainedParts::default(),
            PackageKind::Template,
        )
        .map_err(|error| AdapterError::new(format!("template writer: {error}")))?;

        let mut report = crate::docx::convert_export_report(&exported.report);
        if request.source.is_some() {
            report.entries.push(crate::CompatibilityEntry {
                feature: "dotx.export.retained_parts".to_owned(),
                occurrences: 1,
                location: crate::FeatureLocation {
                    local_name: Some("dotx.export.retained_parts".to_owned()),
                    ..crate::FeatureLocation::default()
                },
                model_outcome: crate::ModelOutcome::Omitted,
                retention_outcome: crate::RetentionOutcome::NotRetained,
            });
        }
        report.sort();
        let report: CompatibilityReport = report;

        Ok(ExportArtifact {
            bytes: exported.bytes,
            report,
            format: FormatProfile {
                format: self.descriptor.id.clone(),
                version: Some("ecma-376".to_owned()),
            },
            mime_type: DOTX_MIME.to_owned(),
            suggested_extension: "dotx".to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocumentResources, builtin_registry};
    use casual_doc_model::v1::Document;
    use std::collections::BTreeMap;

    /// The main-part content type a `.docx` declares, for the contrast the whole
    /// format rests on.
    const DOCUMENT_CT: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";
    const TEMPLATE_CT: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml";

    fn document() -> Document {
        casual_doc_import::import_main_document_xml(
            b"<w:document xmlns:w=\"urn:w\"><w:body><w:p><w:r><w:t>House style</w:t></w:r>\
              </w:p></w:body></w:document>",
            Default::default(),
        )
        .expect("the body imports")
        .document
    }

    fn content_types(bytes: &[u8]) -> String {
        let mut archive =
            zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("the package is a ZIP");
        let mut part = archive
            .by_name("[Content_Types].xml")
            .expect("every OPC package has one");
        let mut text = String::new();
        std::io::Read::read_to_string(&mut part, &mut text).expect("content types are UTF-8");
        text
    }

    fn export(mode: ExportMode) -> Result<ExportArtifact, AdapterError> {
        let source = document();
        let resources = DocumentResources::default();
        DotxAdapter::default().export(ExportRequest {
            document: &source,
            resources: &resources,
            source: None,
            source_unchanged: false,
            mode,
        })
    }

    #[test]
    fn a_template_declares_the_template_main_part_and_a_document_does_not() {
        // This is the entire difference between the two formats, so it is the
        // whole guard: the same bytes with one content type swapped.
        let template = export(ExportMode::Semantic).expect("template export");
        let types = content_types(&template.bytes);
        assert!(types.contains(TEMPLATE_CT), "{types}");
        assert!(
            !types.contains(DOCUMENT_CT),
            "a template must not also declare the document type: {types}"
        );
        assert_eq!(template.mime_type, DOTX_MIME);
        assert_eq!(template.suggested_extension, "dotx");
    }

    #[test]
    fn the_document_writer_is_unchanged_by_the_template_one_existing() {
        // The regression this pair guards against: threading a package kind
        // through the writer must not have altered what a `.docx` declares.
        let source = document();
        let media = BTreeMap::new();
        let docx = casual_doc_export::export_document(&source, &media).expect("docx export");
        let types = content_types(&docx.bytes);
        assert!(types.contains(DOCUMENT_CT), "{types}");
        assert!(!types.contains(TEMPLATE_CT), "{types}");
    }

    #[test]
    fn a_template_carries_the_same_body_as_the_document() {
        let source = document();
        let media = BTreeMap::new();
        let docx = casual_doc_export::export_document(&source, &media).expect("docx export");
        let template = export(ExportMode::Semantic).expect("template export");
        let mut docx_zip =
            zip::ZipArchive::new(std::io::Cursor::new(&docx.bytes)).expect("docx is a ZIP");
        let mut template_zip =
            zip::ZipArchive::new(std::io::Cursor::new(&template.bytes)).expect("dotx is a ZIP");
        let mut left = Vec::new();
        let mut right = Vec::new();
        std::io::Read::read_to_end(
            &mut docx_zip.by_name("word/document.xml").expect("main part"),
            &mut left,
        )
        .expect("read");
        std::io::Read::read_to_end(
            &mut template_zip
                .by_name("word/document.xml")
                .expect("main part"),
            &mut right,
        )
        .expect("read");
        assert_eq!(left, right, "only the content type differs");
    }

    #[test]
    fn the_exact_mode_is_refused_with_a_sentence() {
        let error = export(ExportMode::ExactIfUnchanged).expect_err("no source bytes exist");
        assert!(
            format!("{error}").contains("no retained source bytes"),
            "{error}"
        );
    }

    #[test]
    fn the_builtin_registry_offers_a_template_to_save_and_never_to_open() {
        let registry = builtin_registry();
        let exports: Vec<&str> = registry
            .export_formats()
            .into_iter()
            .map(FormatId::as_str)
            .collect();
        assert!(exports.contains(&formats::DOTX), "{exports:?}");
        let descriptor = registry
            .descriptors()
            .into_iter()
            .find(|descriptor| descriptor.id.as_str() == formats::DOTX)
            .expect("the DOTX descriptor is registered");
        assert!(!descriptor.can_import);
        assert!(!descriptor.exact_if_unchanged);
        assert_eq!(descriptor.extensions, ["dotx"]);
        assert_eq!(descriptor.display_name, "Word Template");
    }
}
