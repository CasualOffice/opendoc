//! OPC relationship parsing and target resolution.

use crate::discovery::for_each_metadata_element;
use crate::error::PackageError;

/// OPC `officeDocument` relationship type in the transitional namespace.
pub(crate) const OFFICE_DOCUMENT_REL_TRANSITIONAL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
/// OPC `officeDocument` relationship type in the ISO/IEC 29500 Strict namespace.
pub(crate) const OFFICE_DOCUMENT_REL_STRICT: &str =
    "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument";

/// Whether a relationship target is inside the package or an external URI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TargetMode {
    /// The target resolves to a part inside the package.
    Internal,
    /// The target is an external URI and is never fetched during import.
    External,
}

/// One resolved relationship declared by the main document part.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentRelationship {
    /// Relationship identifier (`r:id`), empty if the source omitted it.
    pub id: String,
    /// Relationship type URI.
    pub relationship_type: String,
    /// Raw target as declared in the relationships part.
    pub target: String,
    /// Whether the target is internal or external.
    pub target_mode: TargetMode,
    /// Normalized package part name for a resolvable internal target; `None`
    /// for external targets or internal targets that escape the package root.
    pub resolved_part: Option<String>,
}

/// One parsed OPC relationship (only the fields the reader needs).
#[derive(Debug)]
pub(crate) struct Relationship {
    pub(crate) id: String,
    pub(crate) rel_type: String,
    pub(crate) target: String,
    pub(crate) external: bool,
}

pub(crate) fn parse_relationships(
    bytes: &[u8],
    part: &'static str,
) -> Result<Vec<Relationship>, PackageError> {
    let mut out = Vec::new();
    for_each_metadata_element(bytes, part, |name, attribute| {
        if name == b"Relationship" {
            let mut id = None;
            let mut rel_type = None;
            let mut target = None;
            let mut external = false;
            attribute(&mut |key, value| match key {
                b"Id" => id = Some(value.to_owned()),
                b"Type" => rel_type = Some(value.to_owned()),
                b"Target" => target = Some(value.to_owned()),
                b"TargetMode" => external = value.eq_ignore_ascii_case("External"),
                _ => {}
            })?;
            if let (Some(rel_type), Some(target)) = (rel_type, target) {
                out.push(Relationship {
                    id: id.unwrap_or_default(),
                    rel_type,
                    target,
                    external,
                });
            }
        }
        Ok(())
    })?;
    Ok(out)
}

pub(crate) fn is_office_document_type(rel_type: &str) -> bool {
    rel_type == OFFICE_DOCUMENT_REL_TRANSITIONAL || rel_type == OFFICE_DOCUMENT_REL_STRICT
}

/// Resolves an OPC relationship target against a base directory (empty for the
/// package root), returning a normalized part name, or `None` if the target is
/// external-shaped, empty, or escapes the package root.
pub(crate) fn resolve_relative_target(base: &[String], target: &str) -> Option<String> {
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

/// Returns the directory segments of a normalized part name (empty at root).
pub(crate) fn parent_segments(part: &str) -> Vec<String> {
    match part.rsplit_once('/') {
        Some((directory, _)) => directory.split('/').map(str::to_owned).collect(),
        None => Vec::new(),
    }
}

/// Returns the `_rels` part name that carries a part's relationships.
pub(crate) fn relationship_part_name(part: &str) -> String {
    let (directory, name) = match part.rsplit_once('/') {
        Some((directory, name)) => (Some(directory), name),
        None => (None, part),
    };
    match directory {
        Some(directory) => format!("{directory}/_rels/{name}.rels"),
        None => format!("_rels/{name}.rels"),
    }
}
