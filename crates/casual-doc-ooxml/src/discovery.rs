//! Main-document discovery and bounded package-metadata XML streaming.

use casual_doc_package::{BoundedPackage, CancellationToken};
use quick_xml::Reader;
use quick_xml::events::Event;

use crate::contenttypes::ContentTypes;
use crate::error::PackageError;
// Own line, kept out of any sorted block (the repo's parallel-PR rule).
use crate::recover::PackageRepair;
use crate::package::ROOT_RELATIONSHIPS_PART;
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
const PART_RELS_LABEL: &str = "<part>/_rels/*.rels";

pub(crate) fn discover_main_document(
    relationships_bytes: &[u8],
    content_types: &ContentTypes,
    package: &BoundedPackage<'_>,
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
    if !package.contains_part(&resolved) {
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

/// Conventional main-document part names, tried in order when nothing in the
/// package points at one. `document2.xml` is Word's own second choice, written
/// when a `document.xml` entry already exists in the archive.
const CONVENTIONAL_MAIN_DOCUMENTS: [&str; 2] = ["word/document.xml", "word/document2.xml"];

/// Locates the main document in a damaged package, recording what it had to
/// assume.
///
/// Every refusal the strict discovery raises becomes a repair here, because none
/// of them is a fact about document *content*:
///
/// - **No relationships part, or an unreadable one.** The index is plumbing a
///   writer regenerates; losing it loses no text.
/// - **No `officeDocument` relationship, or one whose target is not in the
///   package.** The document part is still in the archive under a conventional
///   name, or is still the only part declaring a WordprocessingML content type.
/// - **More than one.** Picking the first in relationship order is deterministic,
///   and is strictly better than showing the reader nothing.
/// - **A content type that is absent or contradicts the part.** A content type is
///   a label; the markup inside the part is the fact. If the markup is not
///   WordprocessingML the import says so with its own finding — refusing here
///   would stop the file opening because of its label.
///
/// `Err(MissingMainDocument)` is still possible, and it is the honest one: a
/// package with no candidate part at all holds no Word document, and the caller
/// renders that as a sentence rather than retrying.
pub(crate) fn discover_main_document_recovering(
    relationships_bytes: Option<&[u8]>,
    content_types: &ContentTypes,
    package: &BoundedPackage<'_>,
    repairs: &mut Vec<PackageRepair>,
) -> Result<String, PackageError> {
    let relationships = match relationships_bytes {
        None => {
            repairs.push(PackageRepair::PackageRelationshipsMissing);
            Vec::new()
        }
        Some(bytes) => match parse_relationships(bytes, ROOT_RELATIONSHIPS_PART) {
            Ok(relationships) => relationships,
            Err(_) => {
                repairs.push(PackageRepair::PackageRelationshipsUnreadable);
                Vec::new()
            }
        },
    };
    let office: Vec<&Relationship> = relationships
        .iter()
        .filter(|relationship| {
            !relationship.external && is_office_document_type(&relationship.rel_type)
        })
        .collect();
    let declared = office
        .first()
        .and_then(|relationship| resolve_relative_target(&[], &relationship.target))
        .filter(|part| package.contains_part(part));
    let (part, located_by_name) = match declared {
        Some(part) => (part, false),
        None => (conventional_main_document(content_types, package)?, true),
    };
    if located_by_name {
        repairs.push(PackageRepair::MainDocumentLocatedByName { part: part.clone() });
    } else if office.len() > 1 {
        repairs.push(PackageRepair::MainDocumentAmbiguous { part: part.clone() });
    }
    let content_type = content_types.content_type_of(&part);
    if !content_type.is_some_and(|declared| MAIN_DOCUMENT_CONTENT_TYPES.contains(&declared)) {
        repairs.push(PackageRepair::MainDocumentContentTypeIgnored {
            declared: content_type.map(str::to_owned),
        });
    }
    Ok(part)
}

/// The best candidate main document in a package that does not point at one:
/// the first part declaring a WordprocessingML main-document type, then the
/// conventional names, then any `word/document*.xml`.
fn conventional_main_document(
    content_types: &ContentTypes,
    package: &BoundedPackage<'_>,
) -> Result<String, PackageError> {
    // Entries are already ordered by normalized part name, so every branch below
    // is deterministic.
    if let Some(entry) = package.entries().iter().find(|entry| {
        content_types
            .content_type_of(&entry.part_name)
            .is_some_and(|declared| MAIN_DOCUMENT_CONTENT_TYPES.contains(&declared))
    }) {
        return Ok(entry.part_name.clone());
    }
    for candidate in CONVENTIONAL_MAIN_DOCUMENTS {
        if package.contains_part(candidate) {
            return Ok(candidate.to_owned());
        }
    }
    if let Some(name) = package
        .entries()
        .iter()
        .map(|entry| entry.part_name.as_str())
        .find(|name| name.starts_with("word/document") && name.ends_with(".xml"))
    {
        return Ok(name.to_owned());
    }
    // Last resort: the conventional name even though no such part exists. A
    // package holding `word/styles.xml` and `word/numbering.xml` but no
    // `word/document.xml` is a Word document whose text is gone, and naming the
    // part that should have been there lets the import open an empty document
    // with that file's styles, properties and headers around it, saying plainly
    // that the text could not be read. Refusing instead shows the reader nothing
    // about a file they can still partly recover.
    if looks_like_wordprocessingml(content_types, package) {
        return Ok(CONVENTIONAL_MAIN_DOCUMENTS[0].to_owned());
    }
    Err(PackageError::MissingMainDocument)
}

/// Whether a package identifies itself as WordprocessingML even though it holds
/// no main document.
///
/// This is what stops the fallback above turning every ZIP into an empty Word
/// document. Two independent signals, either of which is enough: a part under
/// `word/` (the directory ECMA-376 and every producer use for this format), or a
/// declared content type in the WordprocessingML family. Both are properties of
/// the *package* rather than of a name this engine chose, so a package of another
/// format cannot satisfy either by accident — an ODF or a JAR is still refused
/// here, and is still detected by its own adapter.
fn looks_like_wordprocessingml(content_types: &ContentTypes, package: &BoundedPackage<'_>) -> bool {
    package.entries().iter().any(|entry| {
        entry.part_name.starts_with("word/")
            || content_types
                .content_type_of(&entry.part_name)
                .is_some_and(|declared| declared.contains("wordprocessingml"))
    })
}

/// Resolves any part's part-level relationships, classifying each as internal
/// (with a resolved normalized part name) or external (never fetched). Targets
/// resolve relative to the part's own directory, so an extra part (header,
/// footer, footnotes) resolves its own media/hyperlink references. A part with
/// no `_rels` part has no relationships.
pub(crate) fn resolve_part_relationships(
    package: &mut BoundedPackage<'_>,
    part: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<DocumentRelationship>, PackageError> {
    let rels_part = relationship_part_name(part);
    if !package.contains_part(&rels_part) {
        return Ok(Vec::new());
    }
    let bytes = package
        .read_part_with_cancellation(&rels_part, cancellation)
        .map_err(PackageError::from)?;
    let relationships = parse_relationships(&bytes, PART_RELS_LABEL)?;
    let base = parent_segments(part);
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
                // OPC attribute values (notably a relationship `Target` URL) may
                // carry XML character references (`&amp;` in a query string), so
                // unescape them; entity-free values are unchanged. Malformed
                // UTF-8 or a bad entity fails closed.
                let raw = core::str::from_utf8(attribute.value.as_ref())
                    .map_err(|_| PackageError::MalformedPackageXml { part })?;
                let value = quick_xml::escape::unescape(raw)
                    .map_err(|_| PackageError::MalformedPackageXml { part })?;
                sink(attribute.key.local_name().as_ref(), value.as_ref());
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
