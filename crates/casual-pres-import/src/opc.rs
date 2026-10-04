// SPDX-License-Identifier: Apache-2.0

//! The OPC layer a `.pptx` needs: content types, per-part relationships, and
//! discovery of the presentation part.
//!
//! # Why this is here and not reused
//!
//! `casual-doc-ooxml` already does bounded OPC inspection, and reusing it was the
//! intent (`docs/156` §8 says `casual-pres-import` runs "over the parameterised
//! OPC layer"). It is not parameterised yet. Every piece of it —
//! `ContentTypes`, `parse_relationships`, `resolve_relative_target`,
//! `relationship_part_name`, `for_each_metadata_element` — is `pub(crate)`, and
//! the one public entry point, `DocxPackage::open`, discovers the main document
//! by checking its content type against two **WordprocessingML** constants and
//! fails `UnsupportedMainDocumentType` on a presentation. So from outside that
//! crate there is nothing a `.pptx` can call.
//!
//! What *is* reused is the part that matters most: the ZIP container is admitted
//! by `casual_doc_package::BoundedPackage`, which is the security boundary — the
//! zip-bomb ratio, the entry ceiling, the path normalization, the overlap and
//! symlink refusals, the cancellation. There is exactly one of those in the
//! repository and this does not add a second.
//!
//! This module is therefore a second *OPC* reader, which is a duplication worth
//! naming rather than hiding: the right fix is to lift the OPC layer out of
//! `casual-doc-ooxml` the way the loss taxonomy was lifted into
//! `casual-doc-loss`, and delete this. That is a change to a crate this lane does
//! not own.

use std::collections::BTreeMap;

use casual_doc_package::{BoundedPackage, CancellationToken};

use crate::ImportError;

/// The fixed name of the content-types part. OPC fixes this and `_rels/.rels`,
/// and nothing else about a package's layout — which is why the presentation part
/// is discovered by relationship type rather than assumed to be
/// `ppt/presentation.xml`.
pub(crate) const CONTENT_TYPES_PART: &str = "[Content_Types].xml";

/// The fixed name of the package-root relationships part.
pub(crate) const ROOT_RELATIONSHIPS_PART: &str = "_rels/.rels";

/// The OPC `officeDocument` relationship type, transitional namespace.
const OFFICE_DOCUMENT_TRANSITIONAL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
/// The same relationship type in the ISO/IEC 29500 Strict namespace.
const OFFICE_DOCUMENT_STRICT: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";

/// The `slideLayout` relationship type.
///
/// The `slide`, `slideMaster` and `image` types have no constant here on
/// purpose: a slide and a master are reached through the relationship **id** that
/// `p:sldIdLst`/`p:sldMasterIdLst` names, never by scanning for a type, because
/// the id is what carries the order. An image is reached through its
/// `a:blip@r:embed` id for the same reason — scanning by type would register
/// every image in the part's relationships whether a shape references it or not.
pub(crate) const SLIDE_LAYOUT_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";

/// The PresentationML main-part content types this build accepts.
///
/// All four, not one: a `.pptx`, a template (`.potx`), a macro-free show
/// (`.ppsx`) and a slideshow differ in their main part's content type and in
/// nothing this importer reads. Accepting only the first would refuse three file
/// kinds that are the same document.
const PRESENTATION_CONTENT_TYPES: [&str; 4] = [
    "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
    "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml",
    "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml",
    "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml",
];

/// One resolved OPC relationship.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Relationship {
    /// The relationship type URI.
    pub(crate) relationship_type: String,
    /// The normalized part name for an internal, in-package target. `None` for an
    /// external target (never fetched) or an internal one that escapes the root.
    pub(crate) resolved_part: Option<String>,
}

/// A part's relationships, keyed by `Id`.
pub(crate) type Relationships = BTreeMap<String, Relationship>;

/// `[Content_Types].xml`'s default and override mappings.
#[derive(Debug, Default)]
pub(crate) struct ContentTypes {
    defaults: BTreeMap<String, String>,
    overrides: BTreeMap<String, String>,
}

impl ContentTypes {
    /// The declared content type of a normalized part, overrides before
    /// extension defaults — which is the resolution order OPC specifies, and
    /// getting it backwards means a `.xml` slide resolves as `application/xml`.
    pub(crate) fn content_type_of(&self, part_name: &str) -> Option<&str> {
        let absolute = format!("/{part_name}");
        if let Some(content_type) = self.overrides.get(&absolute) {
            return Some(content_type);
        }
        let extension = part_name.rsplit_once('.').map(|(_, extension)| extension)?;
        self.defaults
            .get(&extension.to_ascii_lowercase())
            .map(String::as_str)
    }
}

/// An admitted `.pptx` package: the ZIP, its content types, and the presentation
/// part discovered through `_rels/.rels`.
#[derive(Debug)]
pub(crate) struct PresentationPackage<'a> {
    package: BoundedPackage<'a>,
    content_types: ContentTypes,
    presentation_part: String,
}

impl<'a> PresentationPackage<'a> {
    /// Admits `package` as a presentation, discovering the presentation part.
    ///
    /// # Complexity
    ///
    /// O(entries) for admission plus two part reads. Not for a keystroke.
    pub(crate) fn open(mut package: BoundedPackage<'a>) -> Result<Self, ImportError> {
        for required in [CONTENT_TYPES_PART, ROOT_RELATIONSHIPS_PART] {
            if !package.contains_part(required) {
                return Err(ImportError::MissingRequiredPart { part: required });
            }
        }
        let content_types_bytes = package.read_part(CONTENT_TYPES_PART)?;
        let content_types = parse_content_types(&content_types_bytes)?;
        let root_bytes = package.read_part(ROOT_RELATIONSHIPS_PART)?;
        let root = parse_relationships(&root_bytes, ROOT_RELATIONSHIPS_PART, &[])?;

        let mut office: Vec<&Relationship> = root
            .values()
            .filter(|relationship| {
                is_office_document(&relationship.relationship_type)
                    && relationship.resolved_part.is_some()
            })
            .collect();
        let presentation_part = match office.len() {
            0 => return Err(ImportError::MissingPresentationPart),
            1 => office
                .pop()
                .and_then(|relationship| relationship.resolved_part.clone())
                .ok_or(ImportError::MissingPresentationPart)?,
            _ => return Err(ImportError::AmbiguousPresentationPart),
        };
        if !package.contains_part(&presentation_part) {
            return Err(ImportError::MissingPresentationPart);
        }
        let content_type = content_types.content_type_of(&presentation_part);
        if !content_type.is_some_and(|declared| PRESENTATION_CONTENT_TYPES.contains(&declared)) {
            return Err(ImportError::NotAPresentation {
                content_type: content_type.map(str::to_owned),
            });
        }
        Ok(Self {
            package,
            content_types,
            presentation_part,
        })
    }

    /// The discovered presentation part's normalized name.
    pub(crate) fn presentation_part(&self) -> &str {
        &self.presentation_part
    }

    /// A part's declared content type, if `[Content_Types].xml` resolves one.
    pub(crate) fn content_type_of(&self, part_name: &str) -> Option<&str> {
        self.content_types.content_type_of(part_name)
    }

    /// Whether a normalized part was admitted.
    pub(crate) fn contains_part(&self, part_name: &str) -> bool {
        self.package.contains_part(part_name)
    }

    /// Reads and verifies one admitted part.
    pub(crate) fn read_part(&mut self, part_name: &str) -> Result<Vec<u8>, ImportError> {
        Ok(self.package.read_part(part_name)?)
    }

    /// Every admitted part name, ordered by normalized name, so a whole-part
    /// disposition can enumerate what import did not consume.
    pub(crate) fn part_names(&self) -> Vec<String> {
        self.package
            .entries()
            .iter()
            .map(|entry| entry.part_name.clone())
            .collect()
    }

    /// A part's own relationships, resolved against its directory.
    ///
    /// A part with no `_rels` companion has none, which is not an error: a slide
    /// layout that references nothing writes no relationships part.
    pub(crate) fn relationships_of(
        &mut self,
        part_name: &str,
    ) -> Result<Relationships, ImportError> {
        let rels_part = relationship_part_name(part_name);
        if !self.package.contains_part(&rels_part) {
            return Ok(Relationships::new());
        }
        let bytes = self
            .package
            .read_part_with_cancellation(&rels_part, &CancellationToken::default())?;
        let base = parent_segments(part_name);
        parse_relationships(&bytes, &rels_part, &base)
    }
}

fn is_office_document(relationship_type: &str) -> bool {
    relationship_type == OFFICE_DOCUMENT_TRANSITIONAL || relationship_type == OFFICE_DOCUMENT_STRICT
}

/// The `_rels` part name carrying a part's relationships.
fn relationship_part_name(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((directory, name)) => format!("{directory}/_rels/{name}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// A normalized part name's directory segments (empty at the package root).
fn parent_segments(part: &str) -> Vec<String> {
    match part.rsplit_once('/') {
        Some((directory, _)) => directory.split('/').map(str::to_owned).collect(),
        None => Vec::new(),
    }
}

/// Resolves a relationship target against a base directory.
///
/// `None` for an empty target, a backslash- or NUL-bearing one, or one that
/// escapes the package root with `..` — every case in which there is no part to
/// name. Refusing the escape here rather than at the read is what keeps a
/// traversal out of the resolved-part map entirely.
fn resolve_target(base: &[String], target: &str) -> Option<String> {
    if target.is_empty() || target.contains('\\') || target.contains('\0') {
        return None;
    }
    let (mut segments, body) = match target.strip_prefix('/') {
        Some(absolute) => (Vec::new(), absolute),
        None => (base.to_vec(), target),
    };
    for segment in body.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            other => segments.push(other.to_owned()),
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(segments.join("/"))
}

/// Bound on element count for a package-metadata part, which is small by
/// construction: a relationships part has one element per reference.
const MAX_METADATA_ELEMENTS: u64 = 50_000;
/// Bound on element nesting for a package-metadata part, which is two deep.
const MAX_METADATA_DEPTH: u32 = 16;

fn metadata_limits() -> crate::limits::ImportLimits {
    crate::limits::ImportLimits {
        max_part_elements: MAX_METADATA_ELEMENTS,
        max_part_depth: MAX_METADATA_DEPTH,
        ..crate::limits::ImportLimits::default()
    }
}

fn parse_content_types(bytes: &[u8]) -> Result<ContentTypes, ImportError> {
    let mut types = ContentTypes::default();
    let part = CONTENT_TYPES_PART;
    let mut cursor = crate::xml::Cursor::new(bytes, part, metadata_limits());
    cursor.root()?;
    crate::xml::children(&mut cursor, |cursor, element, _empty| {
        match crate::xml::local_name(element) {
            b"Default" => {
                let extension = crate::xml::attribute(element, b"Extension", cursor.part())?;
                let content_type = crate::xml::attribute(element, b"ContentType", cursor.part())?;
                if let (Some(extension), Some(content_type)) = (extension, content_type) {
                    types
                        .defaults
                        .insert(extension.to_ascii_lowercase(), content_type);
                }
            }
            b"Override" => {
                let part_name = crate::xml::attribute(element, b"PartName", cursor.part())?;
                let content_type = crate::xml::attribute(element, b"ContentType", cursor.part())?;
                if let (Some(part_name), Some(content_type)) = (part_name, content_type) {
                    types.overrides.insert(part_name, content_type);
                }
            }
            _ => {}
        }
        Ok(false)
    })?;
    Ok(types)
}

fn parse_relationships(
    bytes: &[u8],
    part: &str,
    base: &[String],
) -> Result<Relationships, ImportError> {
    let mut relationships = Relationships::new();
    let mut cursor = crate::xml::Cursor::new(bytes, part, metadata_limits());
    cursor.root()?;
    crate::xml::children(&mut cursor, |cursor, element, _empty| {
        if crate::xml::local_name(element) != b"Relationship" {
            return Ok(false);
        }
        let id = crate::xml::attribute(element, b"Id", cursor.part())?;
        let relationship_type = crate::xml::attribute(element, b"Type", cursor.part())?;
        let target = crate::xml::attribute(element, b"Target", cursor.part())?;
        let external = crate::xml::attribute(element, b"TargetMode", cursor.part())?
            .is_some_and(|mode| mode.eq_ignore_ascii_case("External"));
        if let (Some(id), Some(relationship_type), Some(target)) = (id, relationship_type, target) {
            let resolved_part = if external {
                None
            } else {
                resolve_target(base, &target)
            };
            relationships.insert(
                id,
                Relationship {
                    relationship_type,
                    resolved_part,
                },
            );
        }
        Ok(false)
    })?;
    Ok(relationships)
}
