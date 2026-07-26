//! The opaque part side-table (P1F-2): admitted package parts the semantic
//! model does not consume, carried verbatim so a semantic edit→save preserves
//! them instead of regenerating them away.
//!
//! This is **preservation-sidecar data**, not part of the semantic `v1` node
//! model (doc-45 invariant I4: derived/opaque bytes never live in the OOXML
//! model). It travels alongside the model from the importer to the semantic
//! writer, which re-emits each part byte-for-byte, merges its content-type into
//! the generated `[Content_Types].xml`, preserves its owned `_rels`, and re-adds
//! the root/document relationship that targets it so the part stays reachable.
//!
//! Digital signatures (`_xmlsignatures/*` and signature relationships) are
//! deliberately **excluded** — editing invalidates a signature, so a preserved
//! signature over regenerated content would be misleading. They are dropped and
//! reported (`not-retained`) instead.
//!
//! Scope: a part referenced only from the document *body* survives as bytes and,
//! when a first-class node re-references it (a chart/diagram/OLE embedded-object
//! node, P1F-26/27), the writer emits its relationship from that node — so the
//! importer excludes such a part from the orphan-rel set here (its bytes stay
//! preserved, but its relationship is NOT re-added, avoiding a double-emit). A
//! body-referenced part that no node re-references still has its relationship
//! re-added (keeping the part in the package graph) though the body no longer
//! names the id. Root-referenced parts (customXml, thumbnail, docProps-like)
//! keep their referencing rels and remain fully reachable.

/// Which regenerated relationships part carries a retained relationship.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RelationshipOwner {
    /// The package root `_rels/.rels`.
    Root,
    /// The main document's `word/_rels/document.xml.rels`.
    Document,
}

/// A part's own `_rels` companion, retained verbatim (e.g. a chart's
/// `word/charts/_rels/chart1.xml.rels`, a customXml item's
/// `customXml/_rels/item1.xml.rels`), so parts it references stay reachable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedRels {
    /// Normalized `_rels` part name.
    pub part_name: String,
    /// Verbatim relationships-part bytes.
    pub bytes: Vec<u8>,
}

/// One admitted-but-unconsumed part, retained verbatim.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedPart {
    /// Normalized package part name (e.g. `customXml/item1.xml`).
    pub part_name: String,
    /// Declared content type, if the package declared one (emitted as a
    /// content-type `Override` on write).
    pub content_type: Option<String>,
    /// Verbatim part bytes.
    pub bytes: Vec<u8>,
    /// The part's own `_rels` companion, if any.
    pub rels: Option<RetainedRels>,
}

/// A root/document relationship targeting a retained part, re-emitted (with a
/// fresh id) so the part stays reachable in the regenerated package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedRelationship {
    /// Which regenerated relationships part carries it.
    pub owner: RelationshipOwner,
    /// Relationship type URI.
    pub relationship_type: String,
    /// Raw target as declared in the source relationships part (kept verbatim so
    /// the relative path is exactly reproduced).
    pub target: String,
    /// Whether the target is external (`TargetMode="External"`). Internal
    /// (in-package) targets omit the attribute.
    pub external: bool,
}

/// The opaque part side-table: unconsumed admitted parts carried verbatim
/// through the semantic writer, plus the root/document relationships that keep
/// them reachable. Empty for the XML-only import entry point (no package) and
/// when a package has no unconsumed parts.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RetainedParts {
    /// Retained parts, ordered by part name (deterministic).
    pub parts: Vec<RetainedPart>,
    /// Root/document relationships targeting a retained part, ordered
    /// deterministically.
    pub relationships: Vec<RetainedRelationship>,
}

impl RetainedParts {
    /// Whether the side-table carries no parts (and thus no relationships).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// Aggregate retained byte count (part bytes plus their owned `_rels`),
    /// used to bound the side-table against the parser byte ceiling.
    #[must_use]
    pub fn total_bytes(&self) -> usize {
        self.parts
            .iter()
            .map(|part| {
                part.bytes
                    .len()
                    .saturating_add(part.rels.as_ref().map_or(0, |rels| rels.bytes.len()))
            })
            .fold(0_usize, usize::saturating_add)
    }
}

/// Whether a part is a digital-signature part (excluded from preservation:
/// editing invalidates a signature). Matches the `_xmlsignatures/` origin/parts
/// and any digital-signature content type.
pub(crate) fn is_signature_part(part_name: &str, content_type: Option<&str>) -> bool {
    part_name.starts_with("_xmlsignatures/")
        || content_type.is_some_and(|ct| ct.contains("digital-signature"))
}

/// Whether a relationship type points at the digital-signature machinery (so its
/// referencing relationship is not re-added on the semantic path).
pub(crate) fn is_signature_relationship(relationship_type: &str) -> bool {
    relationship_type.contains("/digital-signature/")
}
