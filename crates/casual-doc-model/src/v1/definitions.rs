//! Style, numbering, section, theme, and media definition tables.

use serde::{Deserialize, Serialize};

use super::{
    AbstractNumberingId, BlockNode, CommentId, DefinitionMap, FontName, HeaderFooterId, MediaId,
    NoteId, NumberingInstanceId, ParagraphProperties, RunProperties, SectionId, StyleId, StyleKind,
};

/// A style definition (its id is the map key).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    /// Style kind.
    pub kind: StyleKind,
    /// Inherited style.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub based_on: Option<StyleId>,
    /// Paragraph property overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<ParagraphProperties>,
    /// Run property overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunProperties>,
}

/// Document-wide default properties.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentDefaults {
    /// Default paragraph properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<ParagraphProperties>,
    /// Default run properties.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunProperties>,
}

/// One abstract numbering level.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingLevel {
    /// Level index.
    pub level: u8,
    /// Starting value.
    pub start: u16,
    /// Optional character style reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<StyleId>,
}

/// An abstract numbering definition (its id is the map key).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AbstractNumbering {
    /// Ordered levels.
    pub levels: Vec<NumberingLevel>,
}

/// A per-instance numbering level override.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingOverride {
    /// Level index.
    pub level: u8,
    /// Overriding start value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u16>,
}

/// A numbering instance (its id is the map key).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingInstance {
    /// The abstract definition this instance uses.
    pub abstract_ref: AbstractNumberingId,
    /// Per-level overrides.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<NumberingOverride>,
}

/// Page size in twips.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSize {
    /// Width in twips.
    pub width_twips: i32,
    /// Height in twips.
    pub height_twips: i32,
}

/// Page margins in twips.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageMargins {
    /// Top margin.
    pub top_twips: i32,
    /// Bottom margin.
    pub bottom_twips: i32,
    /// Leading margin.
    pub start_twips: i32,
    /// Trailing margin.
    pub end_twips: i32,
}

/// Section column layout.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionColumns {
    /// Column count.
    pub count: u16,
}

/// Which page type a header or footer applies to.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HeaderFooterKind {
    /// The default header/footer.
    Default,
    /// The first-page header/footer.
    First,
    /// The even-page header/footer.
    Even,
}

/// A section's reference to a header or footer definition for a page type.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderFooterRef {
    /// The page type this reference applies to.
    pub kind: HeaderFooterKind,
    /// The referenced header/footer (resolves in `Definitions`).
    pub reference: HeaderFooterId,
}

/// One ordered section boundary.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionBoundary {
    /// Stable section identity.
    pub id: SectionId,
    /// Page size.
    pub page_size: PageSize,
    /// Page margins.
    pub page_margins: PageMargins,
    /// Column layout.
    pub columns: SectionColumns,
    /// Header references by page type (additive; omitted when empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<HeaderFooterRef>,
    /// Footer references by page type (additive; omitted when empty).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub footers: Vec<HeaderFooterRef>,
}

/// A header or footer definition (its id is the map key). Its content reuses the
/// recursive block model; `blocks` may be empty.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderFooter {
    /// The header/footer's block content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockNode>,
}

/// Semantic theme references retained without embedding the theme.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeReferences {
    /// Major (heading) font family, if identified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub major_font: Option<FontName>,
    /// Minor (body) font family, if identified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minor_font: Option<FontName>,
}

/// A footnote or endnote definition (its id is the map key). Its content reuses
/// the recursive block model, so a note may hold paragraphs, tables, and text
/// boxes. `blocks` may be empty.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Note {
    /// The note's block content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockNode>,
}

/// A comment definition (its id is the map key). Its content reuses the recursive
/// block model; `blocks` may be empty. Author/date/initials are retained as the
/// producer wrote them (opaque, bounded).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Comment {
    /// The comment's block content.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<BlockNode>,
    /// The comment author, if declared (at most 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The author's initials, if declared (at most 255 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initials: Option<String>,
    /// The comment date as written (ISO-8601 string), if declared (<= 64 bytes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

/// A media reference (its id is the map key).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MediaReference {
    /// Source relationship id.
    pub relationship_id: String,
    /// Media (content) type.
    pub media_type: String,
    /// Package part name.
    pub part_name: String,
}

/// The document definition tables.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Definitions {
    /// Style definitions by id.
    #[serde(default)]
    pub styles: DefinitionMap<StyleId, Style>,
    /// Abstract numbering by id.
    #[serde(default)]
    pub abstract_numbering: DefinitionMap<AbstractNumberingId, AbstractNumbering>,
    /// Numbering instances by id.
    #[serde(default)]
    pub numbering: DefinitionMap<NumberingInstanceId, NumberingInstance>,
    /// Ordered section boundaries.
    #[serde(default)]
    pub sections: Vec<SectionBoundary>,
    /// Media references by id.
    #[serde(default)]
    pub media: DefinitionMap<MediaId, MediaReference>,
    /// Footnote definitions by id. Additive: omitted when empty so existing
    /// snapshots (which predate notes) serialize byte-identically.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub footnotes: DefinitionMap<NoteId, Note>,
    /// Endnote definitions by id. Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub endnotes: DefinitionMap<NoteId, Note>,
    /// Header definitions by id. Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub headers: DefinitionMap<HeaderFooterId, HeaderFooter>,
    /// Footer definitions by id. Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub footers: DefinitionMap<HeaderFooterId, HeaderFooter>,
    /// Comment definitions by id. Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub comments: DefinitionMap<CommentId, Comment>,
    /// Document-wide defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_defaults: Option<DocumentDefaults>,
}
