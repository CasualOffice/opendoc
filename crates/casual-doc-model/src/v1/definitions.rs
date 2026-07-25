//! Style, numbering, section, theme, and media definition tables.

use serde::{Deserialize, Serialize};

use super::{
    AbstractNumberingId, BlockNode, BookmarkId, CommentId, DefinitionMap, FontDescriptor,
    FontScheme, HeaderFooterId, MediaId, NoteId, NumberingInstanceId, ParagraphProperties,
    RunProperties, SectionId, StyleId, StyleKind,
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

/// Section column layout (`w:cols`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionColumns {
    /// Column count (`w:num`).
    pub count: u16,
    /// Spacing between columns in twips (`w:space`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_twips: Option<i32>,
    /// Draw a line between columns (`w:sep`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separator: Option<bool>,
}

/// A section break's type (`w:type/@w:val`) — where the new section begins.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SectionType {
    /// Begin on the next page (`nextPage`, the default).
    NextPage,
    /// Continue on the same page (`continuous`).
    Continuous,
    /// Begin on the next even page (`evenPage`).
    EvenPage,
    /// Begin on the next odd page (`oddPage`).
    OddPage,
    /// Begin in the next column (`nextColumn`).
    NextColumn,
}

/// Vertical alignment of content on the page (`w:vAlign/@w:val`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PageVerticalAlignment {
    /// Top-aligned.
    Top,
    /// Centered.
    Center,
    /// Justified (`both`).
    Both,
    /// Bottom-aligned.
    Bottom,
}

/// Page numbering for a section (`w:pgNumType`).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageNumbering {
    /// Number format token (`w:fmt`, e.g. `decimal`/`lowerRoman`); the
    /// `ST_NumberFormat` vocabulary is kept opaque, bounded to 32 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Starting page number (`w:start`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i32>,
}

impl PageNumbering {
    /// Whether nothing is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Document-grid type (`w:docGrid/@w:type`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DocGridType {
    /// No grid (`default`).
    Default,
    /// Line grid only (`lines`).
    Lines,
    /// Line and character grid (`linesAndChars`).
    LinesAndChars,
    /// Snap to characters (`snapToChars`).
    SnapToChars,
}

/// Document grid for a section (`w:docGrid`), used mainly for East-Asian layout.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocGrid {
    /// Grid type (`w:type`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid_type: Option<DocGridType>,
    /// Line pitch in twips (`w:linePitch`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_pitch: Option<i32>,
    /// Character spacing (`w:charSpace`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub char_space: Option<i32>,
}

impl DocGrid {
    /// Whether nothing is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
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
    /// Where this section begins (`w:type`). Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_type: Option<SectionType>,
    /// Use a distinct first-page header/footer (`w:titlePg`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_page: Option<bool>,
    /// Vertical alignment of content on the page (`w:vAlign`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_alignment: Option<PageVerticalAlignment>,
    /// Page numbering (`w:pgNumType`).
    #[serde(default, skip_serializing_if = "PageNumbering::is_empty")]
    pub page_numbering: PageNumbering,
    /// Document grid (`w:docGrid`).
    #[serde(default, skip_serializing_if = "DocGrid::is_empty")]
    pub doc_grid: DocGrid,
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

/// A bookmark definition (its id is the map key). A bookmark is a named range;
/// its extent is delimited by a `BookmarkStart`/`BookmarkEnd` marker pair in body
/// flow, and only its name is a definition-level property.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Bookmark {
    /// The bookmark name as written (non-empty, at most 255 bytes).
    pub name: String,
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

/// Document-wide settings (`word/settings.xml`). Only the semantically load-
/// bearing flags are modeled today; the struct is additive and grows as more
/// settings are mapped. Serialized only when non-default so snapshots that
/// predate it stay byte-identical.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentSettings {
    /// `w:embedTrueTypeFonts` — Word requires this flag before it will honor the
    /// embedded font faces carried in `fontTable.xml`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub embed_true_type_fonts: bool,
    /// `w:embedSystemFonts` — embed fonts that ship with the operating system.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub embed_system_fonts: bool,
    /// `w:saveSubsetFonts` — the embedded faces are subsetted to used glyphs.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub save_subset_fonts: bool,
}

impl DocumentSettings {
    /// True when no setting departs from the default (so the part is omitted).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
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
    /// Bookmark definitions by id. Additive: omitted when empty so existing
    /// snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub bookmarks: DefinitionMap<BookmarkId, Bookmark>,
    /// Document-wide defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_defaults: Option<DocumentDefaults>,
    /// Font-table descriptors (`word/fontTable.xml`), in document order.
    /// Additive: omitted when empty so existing snapshots serialize
    /// byte-identically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_table: Vec<FontDescriptor>,
    /// Theme font scheme (`theme1.xml` `a:fontScheme`) against which theme font
    /// slots resolve. Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_scheme: Option<FontScheme>,
    /// Document-wide settings (`word/settings.xml`). Additive: omitted when
    /// default so existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "DocumentSettings::is_default")]
    pub settings: DocumentSettings,
}
