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
//! The same reasoning applies, one step later, to the two retained parts that are
//! *derived* from the content — the thumbnail and Word 2010's
//! `stylesWithEffects.xml`. They are carried while the document is unedited and
//! left behind, by name, once it is not: `RetainedParts::invalidated_by_edit`
//! (`105` FID-R-05).
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
    /// The source theme part, carried verbatim while the model's theme is
    /// unchanged (`109` FID-AT-03). `None` when the package had no theme, when
    /// it could not be read whole, or when it owns relationships of its own.
    pub theme: Option<RetainedTheme>,
}

/// The source theme part, held so a save can write it back byte for byte
/// instead of regenerating it — copy-on-write, the pattern a chart part
/// already follows until `Chart::dirty` says the model changed it (#811).
///
/// # Why (`109` FID-AT-03)
///
/// The model carries the theme's font scheme, colour scheme and format scheme,
/// which is what resolves `w:themeColor` and `+mn-lt`. It does not carry the
/// theme's NAME, its font scheme's name, populated object defaults, custom
/// colours or extensions, and the regenerated part dropped them all on every
/// save. A theme nobody edited is the overwhelming case, so the source bytes
/// are kept beside what they parsed to, and the writer emits them for as long
/// as the model's theme still equals that — [`RetainedTheme::still_describes`].
/// Equality of values rather than a dirty flag, because nothing then has to
/// remember to set the flag: any path that changes the theme, today or later,
/// makes the comparison fail and the theme regenerate.
///
/// When it does regenerate, the detail only these bytes held is gone, and the
/// writer names it from [`RetainedTheme::unmodeled`] — the findings the import
/// raised against the part, which the import report called `preserved`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetainedTheme {
    /// The source part name (`word/theme/theme1.xml` from Word).
    pub part_name: String,
    /// The source part, byte for byte.
    pub bytes: Vec<u8>,
    /// What the part parsed to: the model's theme at import.
    pub font_scheme: Option<casual_doc_model::v1::FontScheme>,
    /// See [`RetainedTheme::font_scheme`].
    pub color_scheme: Option<casual_doc_model::v1::ColorScheme>,
    /// See [`RetainedTheme::font_scheme`].
    pub format_scheme: Option<casual_doc_model::v1::FormatScheme>,
    /// See [`RetainedTheme::font_scheme`].
    pub format_scheme_xml: Option<String>,
    /// The import findings charged to the part: what the model does not carry
    /// and a regenerated theme therefore loses.
    pub unmodeled: Vec<crate::CompatibilityEntry>,
}

impl RetainedTheme {
    /// Whether the model's theme is still the one these bytes parsed to, so
    /// writing them back describes the document.
    ///
    /// Complexity: O(theme) — a field-by-field comparison of the three
    /// schemes, once per save.
    #[must_use]
    pub fn still_describes(&self, definitions: &casual_doc_model::v1::Definitions) -> bool {
        definitions.font_scheme == self.font_scheme
            && definitions.color_scheme == self.color_scheme
            && definitions.format_scheme == self.format_scheme
            && definitions.format_scheme_xml == self.format_scheme_xml
    }
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

    /// The side-table a save of an **edited** document may carry, and the parts
    /// it must leave behind because they describe the document as it was.
    ///
    /// # Why (`105` FID-R-05)
    ///
    /// The side-table carries a part verbatim because the model does not consume
    /// it — and most such parts are independent of the text (a glossary, a
    /// customXml store, web settings). Two are not: they are *derived* from the
    /// content the model does consume, so after an edit they contradict it.
    ///
    /// - **The thumbnail** (`_rels/.rels`, `…/metadata/thumbnail`): a picture of
    ///   page one as it was at import. File browsers, search indexers and
    ///   document libraries show it as the document. Word drops it on save when it
    ///   is not regenerating one, which is what this engine can do; it cannot
    ///   rasterise a replacement at save time.
    /// - **`stylesWithEffects.xml`** (`…/stylesWithEffects`): Word 2010's second
    ///   copy of the style sheet, which Word 2010 reads in preference to
    ///   `styles.xml`. The semantic writer regenerates `styles.xml` from the model,
    ///   so a style changed here would be shown in Word 2010 at its OLD
    ///   definition. Word 2013 and later stop writing it; dropping it hands every
    ///   reader the regenerated `styles.xml`, which is the current one.
    ///
    /// Both leave the package together with the relationship that reaches them,
    /// so no relationship is left pointing at nothing; each is named in the
    /// returned list so the caller can report it — a part invalidated in silence
    /// would be the loss this module exists to prevent.
    ///
    /// Deliberately NOT here: a `customXml` store a content control is bound to
    /// (`w:dataBinding`). Its staleness depends on whether the bound control's
    /// TEXT changed, which only an XPath evaluation against the store can say;
    /// dropping the store would unbind every control in the document. Recorded in
    /// `109` as its own row.
    ///
    /// # Target resolution
    ///
    /// A root relationship's target resolves against the package root and a
    /// document relationship's against `word/`, the directory the semantic writer
    /// places the main document in and so the base those relationships are
    /// re-emitted against.
    ///
    /// # Complexity
    ///
    /// O(relationships + parts), once per save.
    #[must_use]
    pub fn invalidated_by_edit(&self) -> (RetainedParts, Vec<InvalidatedPart>) {
        let mut stale: Vec<(String, &'static str)> = Vec::new();
        for relationship in &self.relationships {
            if relationship.external {
                continue;
            }
            if let Some(feature) = stale_relationship_feature(&relationship.relationship_type) {
                stale.push((
                    resolve_retained_target(relationship.owner, &relationship.target),
                    feature,
                ));
            }
        }
        for part in &self.parts {
            if part.content_type.as_deref() == Some(STYLES_WITH_EFFECTS_CONTENT_TYPE)
                && !stale.iter().any(|(name, _)| name == &part.part_name)
            {
                stale.push((part.part_name.clone(), STALE_STYLES_WITH_EFFECTS));
            }
        }
        let is_stale = |name: &str| stale.iter().any(|(stale_name, _)| stale_name == name);
        let mut invalidated: Vec<InvalidatedPart> = self
            .parts
            .iter()
            .filter(|part| is_stale(&part.part_name))
            .map(|part| InvalidatedPart {
                part_name: part.part_name.clone(),
                feature: stale
                    .iter()
                    .find(|(name, _)| name == &part.part_name)
                    .map_or(STALE_THUMBNAIL, |(_, feature)| feature),
            })
            .collect();
        invalidated.sort_by(|left, right| left.part_name.cmp(&right.part_name));
        let kept = RetainedParts {
            // The theme is not derived from the text, so an edit leaves it as
            // it was; whether it still describes the model is the writer's
            // check (`RetainedTheme::still_describes`).
            theme: self.theme.clone(),
            parts: self
                .parts
                .iter()
                .filter(|part| !is_stale(&part.part_name))
                .cloned()
                .collect(),
            relationships: self
                .relationships
                .iter()
                .filter(|relationship| {
                    relationship.external
                        || !is_stale(&resolve_retained_target(
                            relationship.owner,
                            &relationship.target,
                        ))
                })
                .cloned()
                .collect(),
        };
        (kept, invalidated)
    }
}

/// A retained part [`RetainedParts::invalidated_by_edit`] left behind, and the
/// stable feature identifier that names why.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidatedPart {
    /// The normalized part name that was not carried into the save.
    pub part_name: String,
    /// The report feature naming the reason: [`STALE_THUMBNAIL`] or
    /// [`STALE_STYLES_WITH_EFFECTS`].
    pub feature: &'static str,
}

/// Report feature for a thumbnail an edit made stale.
pub const STALE_THUMBNAIL: &str = "docx.export.stale.thumbnail";

/// Report feature for a `stylesWithEffects.xml` an edit made stale.
pub const STALE_STYLES_WITH_EFFECTS: &str = "docx.export.stale.styles_with_effects";

/// The content type Word declares for `word/stylesWithEffects.xml`.
const STYLES_WITH_EFFECTS_CONTENT_TYPE: &str = "application/vnd.ms-word.stylesWithEffects+xml";

/// Which stale class a relationship type reaches, if any.
fn stale_relationship_feature(relationship_type: &str) -> Option<&'static str> {
    if relationship_type.ends_with("/metadata/thumbnail") {
        Some(STALE_THUMBNAIL)
    } else if relationship_type.ends_with("/stylesWithEffects") {
        Some(STALE_STYLES_WITH_EFFECTS)
    } else {
        None
    }
}

/// The normalized part name a retained relationship's target names, resolved
/// against its owner's base (`""` for the root, `word/` for the document).
fn resolve_retained_target(owner: RelationshipOwner, target: &str) -> String {
    let (base, target) = match target.strip_prefix('/') {
        Some(absolute) => ("", absolute),
        None => match owner {
            RelationshipOwner::Root => ("", target),
            RelationshipOwner::Document => ("word/", target),
        },
    };
    let mut segments: Vec<&str> = base.split('/').filter(|s| !s.is_empty()).collect();
    for segment in target.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
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
