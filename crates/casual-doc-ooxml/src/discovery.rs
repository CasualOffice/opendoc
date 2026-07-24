//! Main-document discovery and bounded package-metadata XML streaming.

use std::collections::BTreeMap;
use std::io::Cursor;

use quick_xml::Reader;
use quick_xml::events::Event;
use zip::ZipArchive;

use crate::contenttypes::ContentTypes;
use crate::error::PackageError;
use crate::package::{CancellationToken, ROOT_RELATIONSHIPS_PART, read_indexed};
use crate::relationships::{
    DocumentRelationship, Relationship, TargetMode, is_office_document_type, parent_segments,
    parse_relationships, relationship_part_name, resolve_relative_target,
};

/// Accepted WordprocessingML main-document content types (document and template).
const MAIN_DOCUMENT_CONTENT_TYPES: [&str; 2] = [
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
];
/// Bound on element count for package-metadata XML (relationships, content types).
const MAX_METADATA_XML_ELEMENTS: u64 = 10_000;
/// Bound on element nesting depth for package-metadata XML.
const MAX_METADATA_XML_DEPTH: u64 = 64;

/// Static diagnostic label for the main document's relationships part.
const MAIN_DOCUMENT_RELS_LABEL: &str = "<main-document>/_rels/*.rels";

pub(crate) fn discover_main_document(
    relationships_bytes: &[u8],
    content_types: &ContentTypes,
    archive_indexes: &BTreeMap<String, usize>,
) -> Result<String, PackageError> {
    let relationships = parse_relationships(relationships_bytes, ROOT_RELATIONSHIPS_PART)?;
    let office: Vec<&Relationship> = relationships
        .iter()
        .filter(|relationship| {
            !relationship.external && is_office_document_type(&relationship.rel_type)
        })
        .collect();
    match office.len() {
        0 => return Err(PackageError::MissingMainDocument),
        1 => {}
        _ => return Err(PackageError::AmbiguousMainDocument),
    }
    let resolved =
        resolve_relative_target(&[], &office[0].target).ok_or(PackageError::UnsafePartName)?;
    if !archive_indexes.contains_key(&resolved) {
        return Err(PackageError::MissingMainDocument);
    }
    let content_type = content_types
        .content_type_of(&resolved)
        .ok_or(PackageError::UnsupportedMainDocumentType)?;
    if !MAIN_DOCUMENT_CONTENT_TYPES.contains(&content_type) {
        return Err(PackageError::UnsupportedMainDocumentType);
    }
    Ok(resolved)
}

/// Resolves the main document's part-level relationships, classifying each as
/// internal (with a resolved normalized part name) or external (never fetched).
/// A main document with no `_rels` part has no relationships.
pub(crate) fn resolve_main_document_relationships(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    archive_indexes: &BTreeMap<String, usize>,
    main_document_part: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<DocumentRelationship>, PackageError> {
    let rels_part = relationship_part_name(main_document_part);
    let Some(&index) = archive_indexes.get(&rels_part) else {
        return Ok(Vec::new());
    };
    let bytes = read_indexed(archive, index, cancellation)?;
    let relationships = parse_relationships(&bytes, MAIN_DOCUMENT_RELS_LABEL)?;
    let base = parent_segments(main_document_part);
    let mut resolved: Vec<DocumentRelationship> = relationships
        .into_iter()
        .map(|relationship| {
            let (target_mode, resolved_part) = if relationship.external {
                (TargetMode::External, None)
            } else {
                (
                    TargetMode::Internal,
                    resolve_relative_target(&base, &relationship.target),
                )
            };
            DocumentRelationship {
                id: relationship.id,
                relationship_type: relationship.rel_type,
                target: relationship.target,
                target_mode,
                resolved_part,
            }
        })
        .collect();
    resolved.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(resolved)
}

/// Streams bounded, namespace-agnostic package-metadata XML, invoking `visit`
/// for each element with its local name and an attribute accessor. DTDs and any
/// non-predefined entity are rejected; depth and element counts are bounded.
pub(crate) fn for_each_metadata_element(
    bytes: &[u8],
    part: &'static str,
    mut visit: impl FnMut(
        &[u8],
        &mut dyn FnMut(&mut dyn FnMut(&[u8], &str)) -> Result<(), PackageError>,
    ) -> Result<(), PackageError>,
) -> Result<(), PackageError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut elements = 0_u64;
    let mut depth = 0_u64;
    let mut handle = |element: &quick_xml::events::BytesStart<'_>,
                      elements: &mut u64|
     -> Result<(), PackageError> {
        *elements += 1;
        if *elements > MAX_METADATA_XML_ELEMENTS {
            return Err(PackageError::LimitExceeded {
                limit: "metadata_xml_elements",
                observed: *elements,
                allowed: MAX_METADATA_XML_ELEMENTS,
            });
        }
        let local_name = element.local_name();
        let mut read_attributes = |sink: &mut dyn FnMut(&[u8], &str)| -> Result<(), PackageError> {
            for attribute in element.attributes() {
                let attribute =
                    attribute.map_err(|_| PackageError::MalformedPackageXml { part })?;
                // OPC relationship and content-type attribute values are plain
                // ASCII/UTF-8 with no character entities; a value that fails
                // UTF-8 or carries an unexpected entity fails closed downstream.
                let value = core::str::from_utf8(attribute.value.as_ref())
                    .map_err(|_| PackageError::MalformedPackageXml { part })?;
                sink(attribute.key.local_name().as_ref(), value);
            }
            Ok(())
        };
        visit(local_name.as_ref(), &mut read_attributes)
    };
    loop {
        let event = reader
            .read_event_into(&mut buffer)
            .map_err(|_| PackageError::MalformedPackageXml { part })?;
        match event {
            Event::Eof => break,
            Event::DocType(_) => return Err(PackageError::MalformedPackageXml { part }),
            Event::Start(element) => {
                depth += 1;
                if depth > MAX_METADATA_XML_DEPTH {
                    return Err(PackageError::MalformedPackageXml { part });
                }
                handle(&element, &mut elements)?;
            }
            Event::Empty(element) => {
                handle(&element, &mut elements)?;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(())
}
