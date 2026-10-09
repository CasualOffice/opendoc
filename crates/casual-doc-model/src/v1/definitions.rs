//! Style, numbering, section, theme, and media definition tables.

use serde::{Deserialize, Serialize};

use super::{
    AbstractNumberingId, BlockNode, BookmarkId, BorderEdge, ColorScheme, CommentId, DefinitionMap,
    FontDescriptor, FontScheme, FormatScheme, HeaderFooterId, MediaId, NoteId, NumberingInstanceId,
    ParagraphProperties, RunProperties, SectionId, ShapeStyleRef, StyleId, StyleKind,
    TableCellProperties, TableProperties, TableRowProperties, TextDirection,
};
// Separate `use` line (kept out of the sorted block above) to avoid import-list
// merge collisions with other agents editing this shared model file.
use super::PropChange;
// Same rule: the watermark types' own imports go on their own line.
use super::{FontName, Rgba};
// Same rule: the paragraph-spanning field range's own imports go on their own line.
use super::{FieldKind, FieldRangeId};
// Same rule: the field update attributes go on their own line.
use super::FieldUpdateState;
// Same rule: the one numbering resolver (`v1::numbering`) goes on its own line.
use super::NumberingResolver;
// Same rule: the typed chart projection's own imports go on their own line.
use super::{Chart, ChartId};
// Own line (anti-conflict): the drawing-name side table (`docs/109` HF-267).
use super::ObjectName;
// Own line (anti-conflict): the shape theme-style side table's key.
use crate::NodeId;

/// The table region a `w:tblStylePr` conditional format applies to
/// (`w:tblStylePr/@w:type`, ECMA-376 §17.7.6). Each region carries its own
/// pPr/rPr/tblPr/trPr/tcPr overrides (banding, first-row emphasis, corners).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TableStyleRegion {
    /// The whole table (`wholeTable`).
    WholeTable,
    /// The first (header) row (`firstRow`).
    FirstRow,
    /// The last (total) row (`lastRow`).
    LastRow,
    /// The first column (`firstCol`).
    FirstColumn,
    /// The last column (`lastCol`).
    LastColumn,
    /// Odd horizontal bands (`band1Horz`).
    Band1Horizontal,
    /// Even horizontal bands (`band2Horz`).
    Band2Horizontal,
    /// Odd vertical bands (`band1Vert`).
    Band1Vertical,
    /// Even vertical bands (`band2Vert`).
    Band2Vertical,
    /// The top-right (north-east) corner cell (`neCell`).
    NorthEastCell,
    /// The top-left (north-west) corner cell (`nwCell`).
    NorthWestCell,
    /// The bottom-right (south-east) corner cell (`seCell`).
    SouthEastCell,
    /// The bottom-left (south-west) corner cell (`swCell`).
    SouthWestCell,
}

/// One `w:tblStylePr` conditional-formatting block: the property overrides a
/// table style applies to a single [`TableStyleRegion`]. Each override reuses the
/// shared property types; every field is additive and omitted when absent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableStyleOverride {
    /// The region this override formats (`@w:type`).
    pub region: TableStyleRegion,
    /// Paragraph property overrides (`w:pPr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<ParagraphProperties>,
    /// Run property overrides (`w:rPr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<RunProperties>,
    /// Table property overrides (`w:tblPr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableProperties>,
    /// Table-row property overrides (`w:trPr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_row: Option<TableRowProperties>,
    /// Table-cell property overrides (`w:tcPr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_cell: Option<TableCellProperties>,
}

/// A style definition (its id is the map key). Fields follow `CT_Style`
/// (ECMA-376 §17.7.4.17) order; metadata beyond the id is modeled so it survives
/// an edit round trip. All fields past `kind` are additive: a style that predates
/// this shape (paragraph/character with just pPr/rPr) serializes byte-identically.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    /// Style kind (`@w:type`).
    pub kind: StyleKind,
    /// Whether this is the document default for its kind (`@w:default="1"`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub is_default: bool,
    /// Whether the style is the author's rather than one of the application's
    /// built-ins (`@w:customStyle="1"`).
    ///
    /// Not cosmetic, and not derivable from anything else here. A consumer uses
    /// it to tell a user-defined style from a built-in whose `w:styleId`
    /// happens to collide, and Word uses it to decide which styles a *style
    /// cleanup* or a template re-attach may replace: a built-in is re-derived
    /// from the attached template, a custom style is the author's and is kept.
    /// Dropping it therefore changes what a later edit in Word does to the
    /// document, not how this one looks.
    ///
    /// Measured in 274 `w:style` elements across twelve of the owner's nineteen
    /// documents, always `"1"` — the attribute is Word's marker and is simply
    /// omitted for a built-in, so `false` is the honest absence.
    ///
    /// ODF import leaves this `false`, deliberately: ODF has no built-in style
    /// table to be distinguished from, so every named style there is equally
    /// the producer's, and setting the flag would stamp `w:customStyle="1"` on a
    /// style called `Heading_20_1` that Word regards as its own built-in.
    /// `false` writes nothing, which is the status quo rather than a claim.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub custom_style: bool,
    /// Human-readable primary style name (`w:name`), retained as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Alternate style names (`w:aliases`), the comma-separated list retained
    /// verbatim as one string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases: Option<String>,
    /// Inherited style (`w:basedOn`); must share this style's kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub based_on: Option<StyleId>,
    /// The style applied to the following paragraph (`w:next`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<StyleId>,
    /// The companion style of the opposite kind (`w:link`), e.g. a paragraph
    /// style linked to its character style; the linked style is not kind-checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<StyleId>,
    /// Hidden from the style UI entirely (`w:hidden`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub hidden: bool,
    /// Sort priority in the style UI (`w:uiPriority`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_priority: Option<i32>,
    /// Hidden until used (`w:semiHidden`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub semi_hidden: bool,
    /// Reveal in the UI once used (`w:unhideWhenUsed`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub unhide_when_used: bool,
    /// Marked as a primary (quick) format (`w:qFormat`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub q_format: bool,
    /// Locked against use when document protection is active (`w:locked`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub locked: bool,
    /// Paragraph property overrides (`w:pPr`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<ParagraphProperties>,
    /// Run property overrides (`w:rPr`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunProperties>,
    /// Table property defaults (`w:tblPr`), for a table style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<TableProperties>,
    /// Table-row property defaults (`w:trPr`), for a table style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_row: Option<TableRowProperties>,
    /// Table-cell property defaults (`w:tcPr`), for a table style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table_cell: Option<TableCellProperties>,
    /// Per-region conditional formatting (`w:tblStylePr`), in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditional: Vec<TableStyleOverride>,
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

/// A level's number format (`w:numFmt/@w:val`, `ST_NumberFormat`) — the glyph or
/// numeral system the level renders with. The common vocabulary is modeled; any
/// other token is retained verbatim via [`NumberFormat::Other`] so no producer's
/// format is lost.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NumberFormat {
    /// Arabic numerals (`decimal`).
    Decimal,
    /// A bullet glyph (`bullet`); the glyph itself is the level text.
    Bullet,
    /// Lowercase Roman numerals (`lowerRoman`).
    LowerRoman,
    /// Uppercase Roman numerals (`upperRoman`).
    UpperRoman,
    /// Lowercase letters (`lowerLetter`).
    LowerLetter,
    /// Uppercase letters (`upperLetter`).
    UpperLetter,
    /// Ordinal numerals (`ordinal`, e.g. `1st`).
    Ordinal,
    /// Cardinal text (`cardinalText`, e.g. `One`).
    CardinalText,
    /// Ordinal text (`ordinalText`, e.g. `First`).
    OrdinalText,
    /// Arabic numerals with a leading zero (`decimalZero`).
    DecimalZero,
    /// No number (`none`).
    None,
    /// Any other `ST_NumberFormat` token, retained verbatim (non-empty, bounded).
    Other(String),
}

/// A level's justification (`w:lvlJc/@w:val`) — where the level text sits within
/// the number position. OOXML spells these `left`/`center`/`right`; the logical
/// (writing-direction-aware) `start`/`end` are accepted as synonyms on import.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LevelJustification {
    /// Leading edge (`left`/`start`).
    Start,
    /// Centered (`center`).
    Center,
    /// Trailing edge (`right`/`end`).
    End,
}

/// The character following a level's number (`w:suff/@w:val`) before the
/// paragraph text.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LevelSuffix {
    /// A tab (`tab`, the default).
    Tab,
    /// A single space (`space`).
    Space,
    /// Nothing (`nothing`).
    Nothing,
}

/// One abstract numbering level.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingLevel {
    /// Level index.
    pub level: u8,
    /// Starting value.
    pub start: u16,
    /// Number format (`w:numFmt`) — the glyph/numeral system. Additive: omitted
    /// when absent so pre-existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_fmt: Option<NumberFormat>,
    /// Level text template (`w:lvlText`, e.g. `%1.`) — the placeholder-bearing
    /// string the level renders. Bounded to 255 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lvl_text: Option<String>,
    /// Level justification (`w:lvlJc`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lvl_jc: Option<LevelJustification>,
    /// Suffix between the number and the paragraph text (`w:suff`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suff: Option<LevelSuffix>,
    /// Display this level's number using Arabic numerals (`w:isLgl`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub is_lgl: bool,
    /// Per-level paragraph properties (`w:pPr`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph_properties: Option<ParagraphProperties>,
    /// Per-level run properties (`w:rPr`) applied to the number itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_properties: Option<RunProperties>,
    /// Optional character style reference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style_ref: Option<StyleId>,
    /// The higher level whose increment restarts this level's counter
    /// (`w:lvlRestart`). `None` = the default (restart when any higher level
    /// advances); `Some(0)` = never restart. Needed to reproduce multilevel
    /// restart behavior.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lvl_restart: Option<u8>,
    /// The paragraph style this level binds to (`w:lvl/w:pStyle`) — how a
    /// numbered Heading 1/2/3 list ties each level to its heading style.
    /// Resolved from the referenced style's id during import; re-emitted using
    /// that style's id token on export. Distinct from `style_ref` (a char-style
    /// placeholder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pstyle: Option<StyleId>,
    /// The level's list-template code (`w:lvl/@w:tplc`, `ST_LongHexNumber`):
    /// the key Word uses to tie the level back to an entry in the reader's List
    /// Library. One to eight hexadecimal digits, kept as written. Additive
    /// (`109` FID-AT-16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_code: Option<String>,
    /// `w:lvl/@w:tentative`: Word created this level as a placeholder and may
    /// discard it if it is never used. Dropping it made a tentative level
    /// permanent on reopen. Additive (`109` FID-AT-16).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub tentative: bool,
}

impl NumberingLevel {
    /// Whether `code` is a storable `w:tplc`: one to eight hexadecimal digits.
    #[must_use]
    pub fn is_valid_template_code(code: &str) -> bool {
        (1..=8).contains(&code.len()) && code.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

/// An abstract numbering definition (its id is the map key).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AbstractNumbering {
    /// Ordered levels.
    pub levels: Vec<NumberingLevel>,
    /// The list's overall shape (`w:multiLevelType`) — Word emits it on nearly
    /// every abstract definition; additive, omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multi_level_type: Option<MultiLevelType>,
    /// A reusable "List Style" whose numbering this abstract defers to
    /// (`w:numStyleLink`) — this definition points at a numbering-style paragraph
    /// style for its actual levels. Resolved to/from the referenced style's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_style_link: Option<StyleId>,
    /// The numbering-style paragraph style this abstract *is* the definition for
    /// (`w:styleLink`) — the back-link from an abstract to its owning List Style.
    /// Resolved to/from the referenced style's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_link: Option<StyleId>,
}

/// The overall structure of an abstract numbering definition (`w:multiLevelType`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MultiLevelType {
    /// A single-level list (`singleLevel`).
    SingleLevel,
    /// A multi-level list where every level shares one format (`multilevel`).
    Multilevel,
    /// A multi-level list assembled from mixed level formats (`hybridMultilevel`).
    HybridMultilevel,
}

/// A per-instance numbering level override (`w:num/w:lvlOverride`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingOverride {
    /// Level index (`w:lvlOverride@w:ilvl`).
    pub level: u8,
    /// Overriding start value (`w:startOverride`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u16>,
    /// A full per-instance level redefinition (`w:lvlOverride/w:lvl`): when
    /// present, this instance replaces the abstract level's format, text, style,
    /// and properties for [`level`](Self::level), not just its start value.
    /// Additive: omitted when absent so pre-existing snapshots serialize
    /// byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition: Option<NumberingLevel>,
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
    /// Distance from the top edge of the page to the top of the header
    /// (`w:pgMar/@w:header`). Word nests the header band inside the top margin,
    /// so this — not the top margin — anchors the header. `None` when the
    /// attribute is absent (Word defaults to 720 twips); additive in schema v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_twips: Option<i32>,
    /// Distance from the bottom edge of the page to the bottom of the footer
    /// (`w:pgMar/@w:footer`). Word nests the footer band inside the bottom
    /// margin, so this — not the bottom margin — anchors the footer. `None` when
    /// the attribute is absent (Word defaults to 720 twips); additive in schema v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer_twips: Option<i32>,
    /// Binding (gutter) margin added on the inner edge for two-sided printing
    /// (`w:pgMar/@w:gutter`). `None` when absent (Word defaults to 0); additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gutter_twips: Option<i32>,
}

/// One explicit column's geometry inside a `w:cols` (`w:col`). Word writes these
/// (with `w:cols/@w:equalWidth="0"`) when the columns are unequal; each carries the
/// column's own width and the space that follows it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColumnDef {
    /// This column's width in twips (`w:col/@w:w`).
    pub width_twips: i32,
    /// The space in twips following this column (`w:col/@w:space`); absent on the
    /// last column (and any column that omits it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_twips: Option<i32>,
}

/// Section column layout (`w:cols`).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
    /// Whether the columns are of equal width (`w:cols/@w:equalWidth`). `None` when
    /// the attribute is absent (Word's default is `true`); `Some(false)` signals
    /// per-column widths carried in [`columns`](Self::columns). Additive in schema v1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equal_width: Option<bool>,
    /// Explicit per-column geometries (`w:col`), in column order. Empty when the
    /// columns are equal-width (the geometry is then derived from `count`/`space`).
    /// Additive in schema v1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<ColumnDef>,
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
    /// Number format (`w:fmt`, `ST_NumberFormat`); the common vocabulary is typed
    /// and any other token is retained verbatim via [`NumberFormat::Other`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<NumberFormat>,
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

/// Page orientation (`w:pgSz/@w:orient`, `ST_PageOrientation`). Word derives the
/// printed orientation from this flag; the page size itself stays as written.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PageOrientation {
    /// Portrait (`portrait`).
    Portrait,
    /// Landscape (`landscape`).
    Landscape,
}

/// Which pages a section's page border is drawn on (`w:pgBorders/@w:display`,
/// `ST_PageBorderDisplay`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PageBorderDisplay {
    /// Every page (`allPages`).
    AllPages,
    /// The first page only (`firstPage`).
    FirstPage,
    /// Every page except the first (`notFirstPage`).
    NotFirstPage,
}

/// What a section's page border is measured from (`w:pgBorders/@w:offsetFrom`,
/// `ST_PageBorderOffset`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PageBorderOffset {
    /// Offset from the page edge (`page`).
    Page,
    /// Offset from the text extent (`text`).
    Text,
}

/// Section page borders (`w:pgBorders`) — the four page edges plus where they are
/// drawn (`display`) and measured from (`offsetFrom`). Each edge reuses the shared
/// [`BorderEdge`] value type. Page borders use `w:left`/`w:right`, mapped to
/// `start`/`end` here.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageBorders {
    /// Which pages the border is drawn on (`@w:display`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<PageBorderDisplay>,
    /// What the border offset is measured from (`@w:offsetFrom`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_from: Option<PageBorderOffset>,
    /// Top edge (`w:top`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top: Option<BorderEdge>,
    /// Bottom edge (`w:bottom`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottom: Option<BorderEdge>,
    /// Leading (start) edge (`w:left`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<BorderEdge>,
    /// Trailing (end) edge (`w:right`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<BorderEdge>,
}

impl PageBorders {
    /// Whether nothing is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Where line numbers restart within a section (`w:lnNumType/@w:restart`,
/// `ST_LineNumberRestart`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineNumberRestart {
    /// Restart on every new page (`newPage`).
    NewPage,
    /// Restart at the section start (`newSection`).
    NewSection,
    /// Never restart (`continuous`).
    Continuous,
}

/// Section line numbering (`w:lnNumType`) — margin line numbers and their step,
/// origin, gap, and restart policy.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LineNumbering {
    /// Number every Nth line (`@w:countBy`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count_by: Option<i32>,
    /// First line number (`@w:start`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i32>,
    /// Distance from text to the numbers in twips (`@w:distance`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<i32>,
    /// Restart policy (`@w:restart`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart: Option<LineNumberRestart>,
}

impl LineNumbering {
    /// Whether nothing is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The longest watermark text, in bytes. Word's own dialog caps its combo box
/// well below this; the bound exists so a document cannot carry an unbounded
/// string into the layout pass, not to express a product limit.
pub const MAX_WATERMARK_TEXT_BYTES: usize = 512;

/// How a watermark is angled on the page — Word's Layout radio pair.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WatermarkLayout {
    /// Rising left-to-right across the page, Word's default and the angle every
    /// "DRAFT" stamp is recognised by. Word writes this as a fixed
    /// `rotation:315` on the shape — not an angle derived from the page — so the
    /// model names the choice and layout applies that one angle.
    #[default]
    Diagonal,
    /// Level with the text.
    Horizontal,
}

/// A text watermark ("DRAFT", "CONFIDENTIAL", a case number).
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WatermarkText {
    /// The words to stamp (non-empty, at most [`MAX_WATERMARK_TEXT_BYTES`]).
    pub text: String,
    /// The face to stamp them in. `None` takes the document's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<FontName>,
    /// Size in half-points. `None` is Word's "Auto", which scales the text to
    /// the page rather than picking a number — so it stays automatic here too
    /// and is resolved by layout, where the page width is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_half_points: Option<u32>,
    /// The ink.
    pub color: Rgba,
    /// Bold (`w:b` on the shape's run).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bold: bool,
    /// Italic (`w:i`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub italic: bool,
}

/// A picture watermark: an image stamped behind the text, usually washed out.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WatermarkPicture {
    /// The image, in `Definitions::media`.
    pub media: MediaId,
    /// Scale as a percentage of the image's natural size. `None` is Word's
    /// "Auto" — fit the page — and is resolved by layout for the same reason
    /// [`WatermarkText::size_half_points`] is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale_percent: Option<u32>,
    /// Wash the image out so text stays readable over it (Word's "Washout",
    /// checked by default). Painted through the existing picture-opacity seam.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub washout: bool,
}

/// A section's watermark (Word's Design ▸ Watermark): text or a picture, stamped
/// behind the body on every page of the section.
///
/// # Why this is a section property and not a header shape
///
/// Word has no watermark element. It stores one as a floating VML shape in each
/// of a section's headers — `v:shape` with a `v:textpath` for text, `v:imagedata`
/// with `gain`/`blacklevel` for a picture — and recognises it again by the
/// `PowerPlusWaterMarkObject`/`WordPictureWatermark` shape id. That is a
/// serialization detail of one producer, and modeling it literally would mean
/// every consumer of this model re-deriving "is this shape a watermark" from a
/// name, and a watermark silently becoming three unrelated shapes the moment a
/// section has first/even/odd headers.
///
/// So the model says what the user asked for, once per section, and import lifts
/// it out of the header while export puts it back in the shape Word expects.
/// Round-tripping through Word is a mapping problem at the edges; it is not a
/// reason for the middle of the system to hold a warped-text shape.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WatermarkContent {
    /// Stamped words.
    Text(WatermarkText),
    /// A stamped image.
    Picture(WatermarkPicture),
}

/// A section's watermark and how it sits on the page.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Watermark {
    /// What is stamped.
    pub content: WatermarkContent,
    /// The angle.
    #[serde(default)]
    pub layout: WatermarkLayout,
    /// Draw it faintly so the body stays readable (Word's "Semitransparent",
    /// checked by default for text). Applied on top of the content's own alpha.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub semi_transparent: bool,
}

/// Where a section's footnotes or endnotes are placed (`w:footnotePr`/`w:endnotePr`
/// `w:pos`). The union of `ST_FtnPos` (footnotes) and `ST_EdnPos` (endnotes) is
/// modeled; an unknown token is reported by the importer, never silently kept.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NotePosition {
    /// The bottom of the page (`pageBottom`, footnotes).
    PageBottom,
    /// Immediately beneath the text (`beneathText`, footnotes).
    BeneathText,
    /// The end of the section (`sectEnd`).
    SectionEnd,
    /// The end of the document (`docEnd`).
    DocumentEnd,
}

/// A section's note numbering restart policy (`w:numRestart/@w:val`,
/// `ST_RestartNumber`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoteNumberRestart {
    /// Never restart (`continuous`).
    Continuous,
    /// Restart each section (`eachSect`).
    EachSection,
    /// Restart each page (`eachPage`).
    EachPage,
}

/// Per-section footnote or endnote properties (`w:footnotePr`/`w:endnotePr`):
/// placement plus the numbering format/origin/restart overrides that apply within
/// the section. The numbering format token (`w:numFmt`) is kept opaque and bounded
/// like [`PageNumbering::format`]; every field is additive.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteProperties {
    /// Placement of the notes (`w:pos`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<NotePosition>,
    /// Number format (`w:numFmt/@w:val`, `ST_NumberFormat`); the common vocabulary
    /// is typed and any other token is retained verbatim via [`NumberFormat::Other`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_format: Option<NumberFormat>,
    /// Starting number (`w:numStart/@w:val`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_start: Option<i32>,
    /// Numbering restart policy (`w:numRestart/@w:val`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number_restart: Option<NoteNumberRestart>,
}

impl NoteProperties {
    /// Whether nothing is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Section paper source bins (`w:paperSrc`) — the printer tray for the first page
/// and for all other pages.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PaperSource {
    /// First-page paper bin (`@w:first`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first: Option<i32>,
    /// Paper bin for all other pages (`@w:other`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<i32>,
}

impl PaperSource {
    /// Whether nothing is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
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
    /// Page orientation (`w:pgSz/@w:orient`). Additive: omitted when absent so
    /// pre-existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<PageOrientation>,
    /// Paper source bins (`w:paperSrc`). Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "PaperSource::is_empty")]
    pub paper_source: PaperSource,
    /// Page borders (`w:pgBorders`). Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "PageBorders::is_empty")]
    pub page_borders: PageBorders,
    /// Line numbering (`w:lnNumType`). Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "LineNumbering::is_empty")]
    pub line_numbering: LineNumbering,
    /// The section's watermark, stamped behind the body on every page. Word has
    /// no watermark element and keeps one as a VML shape in each header; see
    /// [`Watermark`] for why the model states it once instead. Additive: omitted
    /// when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watermark: Option<Watermark>,
    /// Per-section footnote properties (`w:footnotePr`). Additive: omitted when
    /// empty.
    #[serde(default, skip_serializing_if = "NoteProperties::is_empty")]
    pub footnote_props: NoteProperties,
    /// Per-section endnote properties (`w:endnotePr`). Additive: omitted when
    /// empty.
    #[serde(default, skip_serializing_if = "NoteProperties::is_empty")]
    pub endnote_props: NoteProperties,
    /// Text flow direction for the section (`w:textDirection`). Additive: omitted
    /// when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_direction: Option<TextDirection>,
    /// Right-to-left section layout (`w:bidi`). Additive: omitted when false.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub bidi: bool,
    /// Section-properties format-change revision (`w:sectPrChange`): the change
    /// metadata (author/date/id) plus a full prior `w:sectPr` snapshot, reusing the
    /// same [`PropChange`] shape as `w:pPrChange`/`w:tblPrChange`. The prior snapshot
    /// never itself carries a further change. Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_change: Option<PropChange<SectionBoundary>>,
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
///
/// Review threading, resolved-state, and durable identity are carried in the
/// companion parts (`commentsExtended.xml`, `commentsIds.xml`, `people.xml`).
/// They join to the base comment on `para_id` — the durable id (`w14:paraId`) of
/// the comment's last paragraph — so a threaded conversation survives a semantic
/// edit->save instead of collapsing to flat comments.
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
    /// The durable paragraph id (`w14:paraId`) of the comment's last paragraph —
    /// the join key for the companion parts. `<= 64` bytes when present.
    /// Additive: omitted when absent so pre-threading snapshots stay identical.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub para_id: Option<String>,
    /// The `para_id` of the parent comment when this comment is a reply
    /// (`commentsExtended.xml` `w15:paraIdParent`); this is the thread edge.
    /// `<= 64` bytes when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_para_id: Option<String>,
    /// Resolved/done state (`commentsExtended.xml` `w15:done`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub done: bool,
    /// The durable id (`commentsIds.xml` `w16cid:durableId`) bound to this
    /// comment's `para_id`. `<= 64` bytes when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durable_id: Option<String>,
    /// The collaborator identity (an entry in [`Definitions::people`], keyed by
    /// author name) matching this comment's author, when `people.xml` declares
    /// one. `<= 255` bytes when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub person: Option<String>,
}

/// A collaborator identity from `word/people.xml` (`w15:person`): an author
/// display name plus optional presence-provider info. A [`Comment`] whose author
/// matches `author` references it through [`Comment::person`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Person {
    /// The author display name (`w15:author`) — the identity key (non-empty, at
    /// most 255 bytes).
    pub author: String,
    /// Presence-provider info (`w15:presenceInfo`), if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence: Option<PresenceInfo>,
}

/// Presence-provider info (`w15:presenceInfo`) for a [`Person`]: the presence
/// provider and the provider-scoped user id.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresenceInfo {
    /// The presence provider (`w15:providerId`, e.g. `AD`, `Windows Live`), at
    /// most 255 bytes.
    pub provider_id: String,
    /// The provider-scoped user id (`w15:userId`), at most 255 bytes.
    pub user_id: String,
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

/// A paragraph-spanning complex field (its id is the map key).
///
/// In OOXML a complex field is a **range, not a container**: `w:fldChar` markers
/// are run-level elements, so a field's `begin` and `end` may sit in different
/// paragraphs. This definition is the shared payload of one such range; its
/// extent is delimited by a `FieldRangeStart`/`FieldRangeEnd` marker pair in body
/// flow, and everything between the markers is the field's cached result as
/// ordinary block and inline content.
///
/// A field whose markers fall in the **same** paragraph is an inline `Field`
/// instead, unchanged — the contained encoding carries the stronger invariant and
/// is available whenever the field fits in a paragraph. See `docs/128` §2c.
///
/// There is no `separate` boundary here for the same reason the inline `Field`
/// has none: the instruction is a string rather than retained instruction runs,
/// so the boundary between instruction and result has nothing left to delimit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldRange {
    /// The field instruction (non-empty, at most `MAX_FIELD_INSTRUCTION_BYTES`),
    /// as the producer wrote it. Authoritative for export.
    pub instruction: String,
    /// The typed field-kind projection derived from `instruction`, by the same
    /// best-effort `FieldKind::parse` the inline field uses. `instruction` stays
    /// authoritative; this is a convenience for consumers.
    #[serde(default)]
    pub kind: FieldKind,
    /// The `w:fldLock` / `w:dirty` update attributes, read from the range's
    /// `w:fldChar` markers. The same type the inline field carries, so a field
    /// promoted from inline to range (or read back either way) cannot change its
    /// update semantics by changing its encoding.
    #[serde(default, skip_serializing_if = "FieldUpdateState::is_empty")]
    pub update: FieldUpdateState,
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

/// A `w:proofState` spelling/grammar checking state.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProofState {
    /// Checked and up to date (`clean`).
    Clean,
    /// Needs (re)checking (`dirty`).
    Dirty,
}

/// The document's spelling/grammar proof state (`w:proofState`). Each dimension is
/// independent and omitted when unset.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProofStateSettings {
    /// Spelling state (`w:spelling`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spelling: Option<ProofState>,
    /// Grammar state (`w:grammar`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grammar: Option<ProofState>,
}

impl ProofStateSettings {
    /// Whether nothing is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The editing restriction imposed by `w:documentProtection/@w:edit`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentProtectionEdit {
    /// No editing restriction (`none`).
    None,
    /// The document is read-only (`readOnly`).
    ReadOnly,
    /// Only comments may be inserted (`comments`).
    Comments,
    /// Only tracked changes are allowed (`trackedChanges`).
    TrackedChanges,
    /// Only form fields may be edited (`forms`).
    Forms,
}

/// Editing/formatting protection (`w:documentProtection`): the three policy
/// attributes, and the password verifier Word stored with them.
///
/// **The verifier is kept, not checked** (ADR-052, updated 2026-10-09). The
/// sixteen password attributes (`AG_Password`'s `w:hash`, `w:salt`,
/// `w:cryptProviderType`, … and `AG_TransitionalPassword`'s `w:algorithmName`,
/// `w:hashValue`, `w:saltValue`, `w:spinCount`) are carried verbatim in
/// [`DocumentProtection::password`] and written back, so a document an author
/// protected with a password in Word still asks Word for that password after it
/// is edited and saved here. Until then they were reported and dropped: the
/// restriction survived and became liftable in Word with no password at all,
/// which silently weakened the author's own deterrent. Nothing here verifies the
/// verifier, and nothing claims the restriction is a security boundary; a
/// reader who lifts or changes the restriction here installs a new value with no
/// verifier, which removes the password (Word would have asked for it first —
/// the host says so), and Undo puts the old value, verifier and all, back.
///
/// Not `Copy` since the verifier was added: it owns its strings.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentProtection {
    /// The editing restriction (`w:edit`).
    pub edit: DocumentProtectionEdit,
    /// Whether the restriction is enforced (`w:enforcement`).
    ///
    /// The source attribute has three states and this field is the **resolved**
    /// answer, so a reader never has to know which of them produced it: an
    /// explicit `"0"` is `false`, an explicit `"1"` is `true`, and an **absent**
    /// attribute is `true` — MS-OI29500 Part 1 §17.15.1.29 records that "Word
    /// enforces protection when this attribute is missing". Export writes the
    /// attribute explicitly in both directions for the same reason.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub enforcement: bool,
    /// Whether style formatting is also locked (`w:formatting`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub formatting: bool,
    /// The password verifier the file carried with the restriction, verbatim;
    /// `None` when it had none. See the type's documentation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<PasswordVerifier>,
}

/// One attribute of a protection's password verifier: `AG_Password`'s twelve
/// (the legacy form Word has always written) and `AG_TransitionalPassword`'s
/// four (the ISO form Office writes instead when `UseIsoPasswordVerifier` is
/// set), ECMA-376 Part 1 §17.15.1.29 and §17.15.1.93.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PasswordAttribute {
    /// `w:algorithmName`.
    AlgorithmName,
    /// `w:hashValue`.
    HashValue,
    /// `w:saltValue`.
    SaltValue,
    /// `w:spinCount`.
    SpinCount,
    /// `w:cryptProviderType`.
    CryptProviderType,
    /// `w:cryptAlgorithmClass`.
    CryptAlgorithmClass,
    /// `w:cryptAlgorithmType`.
    CryptAlgorithmType,
    /// `w:cryptAlgorithmSid`.
    CryptAlgorithmSid,
    /// `w:cryptSpinCount`.
    CryptSpinCount,
    /// `w:cryptProvider`.
    CryptProvider,
    /// `w:algIdExt`.
    AlgIdExt,
    /// `w:algIdExtSource`.
    AlgIdExtSource,
    /// `w:cryptProviderTypeExt`.
    CryptProviderTypeExt,
    /// `w:cryptProviderTypeExtSource`.
    CryptProviderTypeExtSource,
    /// `w:hash`.
    Hash,
    /// `w:salt`.
    Salt,
}

impl PasswordAttribute {
    /// Every attribute, in the order they are written: the ISO four first, then
    /// the legacy twelve in the schema's own order.
    pub const ALL: [Self; 16] = [
        Self::AlgorithmName,
        Self::HashValue,
        Self::SaltValue,
        Self::SpinCount,
        Self::CryptProviderType,
        Self::CryptAlgorithmClass,
        Self::CryptAlgorithmType,
        Self::CryptAlgorithmSid,
        Self::CryptSpinCount,
        Self::CryptProvider,
        Self::AlgIdExt,
        Self::AlgIdExtSource,
        Self::CryptProviderTypeExt,
        Self::CryptProviderTypeExtSource,
        Self::Hash,
        Self::Salt,
    ];

    /// The attribute's local name in the `w:` namespace.
    #[must_use]
    pub const fn local_name(self) -> &'static str {
        match self {
            Self::AlgorithmName => "algorithmName",
            Self::HashValue => "hashValue",
            Self::SaltValue => "saltValue",
            Self::SpinCount => "spinCount",
            Self::CryptProviderType => "cryptProviderType",
            Self::CryptAlgorithmClass => "cryptAlgorithmClass",
            Self::CryptAlgorithmType => "cryptAlgorithmType",
            Self::CryptAlgorithmSid => "cryptAlgorithmSid",
            Self::CryptSpinCount => "cryptSpinCount",
            Self::CryptProvider => "cryptProvider",
            Self::AlgIdExt => "algIdExt",
            Self::AlgIdExtSource => "algIdExtSource",
            Self::CryptProviderTypeExt => "cryptProviderTypeExt",
            Self::CryptProviderTypeExtSource => "cryptProviderTypeExtSource",
            Self::Hash => "hash",
            Self::Salt => "salt",
        }
    }

    /// The attribute a local name denotes, if it is one of the sixteen.
    #[must_use]
    pub fn from_local_name(name: &[u8]) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|attribute| attribute.local_name().as_bytes() == name)
    }
}

/// A protection's password verifier, kept verbatim so a save writes back what
/// the file said (ADR-052). Never verified: see [`DocumentProtection`].
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PasswordVerifier {
    /// The attributes the file stated, at most one per name, in
    /// [`PasswordAttribute::ALL`] order, each non-empty and at most
    /// [`PasswordVerifier::MAX_VALUE_LEN`] bytes.
    pub attributes: Vec<(PasswordAttribute, String)>,
}

impl PasswordVerifier {
    /// The longest value kept. A SHA-512 hash in base64 is 88 bytes; this bounds
    /// a hostile part without refusing anything Word or LibreOffice writes.
    pub const MAX_VALUE_LEN: usize = 1024;

    /// Whether `value` may be stored for an attribute: non-empty and bounded.
    #[must_use]
    pub fn is_storable(value: &str) -> bool {
        !value.is_empty() && value.len() <= Self::MAX_VALUE_LEN
    }

    /// Sets `attribute`, keeping [`Self::attributes`] in `ALL` order and one per
    /// name. A value [`Self::is_storable`] refuses is not stored, and the call
    /// says so. O(16).
    pub fn set(&mut self, attribute: PasswordAttribute, value: String) -> bool {
        if !Self::is_storable(&value) {
            return false;
        }
        match self
            .attributes
            .binary_search_by(|(name, _)| name.cmp(&attribute))
        {
            Ok(index) => self.attributes[index].1 = value,
            Err(index) => self.attributes.insert(index, (attribute, value)),
        }
        true
    }

    /// The value stated for `attribute`, if any. O(16).
    #[must_use]
    pub fn get(&self, attribute: PasswordAttribute) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(name, _)| *name == attribute)
            .map(|(_, value)| value.as_str())
    }

    /// Whether nothing was stated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty()
    }
}

/// Write protection (`w:writeProtection`) — the document is recommended or
/// required to be opened read-only. Presence (`Some`) is itself load-bearing.
///
/// `CT_WriteProtection` carries the same sixteen password attributes as
/// [`DocumentProtection`], kept the same way and for the same reason: a
/// document Word opens read-only behind a password keeps that password through
/// a save here.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteProtection {
    /// Whether opening read-only is merely recommended (`w:recommended`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub recommended: bool,
    /// The password verifier the file carried, verbatim; `None` when it had none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<PasswordVerifier>,
}

/// The view magnification mode (`w:zoom/@w:val`, `ST_Zoom`).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ZoomMode {
    /// No preset mode (`none`); use the explicit percent.
    None,
    /// Fit the whole page (`fullPage`).
    FullPage,
    /// Best fit (`bestFit`).
    BestFit,
    /// Fit the text width (`textFit`).
    TextFit,
}

/// The document's view magnification (`w:zoom`). The mode and the explicit percent
/// are independent (Word writes either or both); omitted when neither is set.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Zoom {
    /// The preset mode (`w:val`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<ZoomMode>,
    /// The explicit magnification percent (`w:percent`), 1..=1000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percent: Option<u16>,
}

impl Zoom {
    /// Whether nothing is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The view a document opens in (`w:view/@w:val`, `ST_View`).
///
/// Modeled so it survives a save: before `109` FID-AT-01 it was reported and
/// dropped, so a document its author had left in Web Layout or Outline came back
/// in Word's default view after any edit here. This engine has one paged layout
/// and one reflow layout and does not switch between them on this value.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentView {
    /// No view stated (`none`): the application's default.
    None,
    /// Print Layout (`print`).
    Print,
    /// Outline (`outline`).
    Outline,
    /// Master document view (`masterPages`).
    MasterPages,
    /// Draft, Word's old "Normal" view (`normal`).
    Normal,
    /// Web Layout (`web`).
    Web,
}

/// The languages a theme font reference resolves against (`w:themeFontLang`).
///
/// A run whose font is a theme slot (`+mn-ea`, `+mj-cs`, …) is resolved by Word
/// through the theme's per-script font list (`a:font script="Jpan"`), and THIS is
/// what says which script each slot means: `w:eastAsia="ja-JP"` makes the East
/// Asian minor font the theme's Japanese face, `zh-CN` its Simplified Chinese
/// one. Dropping it hands Word the language of whatever machine opens the file, so
/// the same document picks a different East Asian face on a different computer.
///
/// Each language is a BCP 47 tag as the producer wrote it, non-empty and bounded
/// to 255 bytes. An empty attribute says nothing, and is read as absent: a
/// producer that writes `w:val=""` on all three (LibreOffice does) has stated no
/// language at all, which is the state an absent element leaves too.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ThemeFontLanguages {
    /// The language for the Latin slots (`w:val`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latin: Option<String>,
    /// The language for the East Asian slots (`w:eastAsia`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub east_asia: Option<String>,
    /// The language for the complex-script slots (`w:bidi`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bidi: Option<String>,
}

impl ThemeFontLanguages {
    /// Whether no language is stated (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.latin.is_none() && self.east_asia.is_none() && self.bidi.is_none()
    }
}

/// One `w:compatSetting` — a named compatibility flag scoped by a URI, carrying an
/// opaque value. The triple is retained verbatim (bounded) so a producer's
/// compatibility contract survives the semantic round trip.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatSetting {
    /// The setting name (`w:name`), non-empty and bounded to 255 bytes.
    pub name: String,
    /// The owning namespace URI (`w:uri`), non-empty and bounded to 255 bytes.
    pub uri: String,
    /// The setting value (`w:val`), bounded to 255 bytes.
    pub val: String,
}

/// Document-wide settings (`word/settings.xml`). The load-bearing settings are
/// modeled; every other setting is reported (never silently dropped) by the
/// importer. The struct is additive and grows as more settings are mapped;
/// serialized only when non-default so snapshots that predate a field stay
/// byte-identical.
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
    /// `w:evenAndOddHeaders` — distinct headers/footers on even vs. odd pages.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub even_and_odd_headers: bool,
    /// `w:mirrorMargins` — mirror inner/outer margins for two-sided printing.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub mirror_margins: bool,
    /// `w:trackRevisions` — revision tracking is on. (The field keeps its
    /// snapshot name; the importer read a `w:trackChanges` element that is not
    /// in the schema until `109` FID-AT-11.)
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub track_changes: bool,
    /// `w:updateFields` — recalculate all fields (TOC, page numbers, refs) when
    /// the document is opened. Common on generated/templated documents.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub update_fields: bool,
    /// `w:defaultTabStop` — the default tab-stop interval in twips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_tab_stop: Option<i32>,
    /// `w:proofState` — the spelling/grammar checking state.
    #[serde(default, skip_serializing_if = "ProofStateSettings::is_empty")]
    pub proof_state: ProofStateSettings,
    /// `w:documentProtection` — the editing/formatting restriction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_protection: Option<DocumentProtection>,
    /// `w:writeProtection` — the read-only-open recommendation/requirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write_protection: Option<WriteProtection>,
    /// `w:defaultTableStyle` — the style applied to tables with no explicit
    /// style, bounded to 255 bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_table_style: Option<String>,
    /// `w:zoom` — the view magnification.
    #[serde(default, skip_serializing_if = "Zoom::is_empty")]
    pub zoom: Zoom,
    /// `w:footnotePr` — the document-default footnote properties (numbering
    /// format/origin/restart and placement) that apply where a section does not
    /// override them. Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "NoteProperties::is_empty")]
    pub footnote_props: NoteProperties,
    /// `w:endnotePr` — the document-default endnote properties. Additive: omitted
    /// when empty.
    #[serde(default, skip_serializing_if = "NoteProperties::is_empty")]
    pub endnote_props: NoteProperties,
    /// `w:compat`/`w:compatSetting` — the modeled compatibility-setting triples,
    /// in document order. Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compat: Vec<CompatSetting>,
    /// `w:compat/w:adjustLineHeightInTable` — apply a section document grid's
    /// line pitch inside table cells. OOXML defaults this compatibility switch
    /// off, so table text normally ignores the section line grid.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub adjust_line_height_in_table: bool,
    /// `w:autoHyphenation` — automatically hyphenate the document.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub auto_hyphenation: bool,
    /// `w:hyphenationZone` — the hyphenation zone in twips (the maximum space left
    /// at a line's end before a word is hyphenated).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyphenation_zone: Option<i32>,
    /// `w:consecutiveHyphenLimit` — the maximum number of consecutive lines that
    /// may end in a hyphen (`0` = no limit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consecutive_hyphen_limit: Option<i32>,
    /// `w:doNotHyphenateCaps` — do not hyphenate words in all capitals.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub do_not_hyphenate_caps: bool,
    /// `w:displayBackgroundShape` — render the document's page background
    /// (`w:background`). When off, the modeled background is not painted.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub display_background_shape: bool,
    /// `w:view` — the view the document opens in. Additive: omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<DocumentView>,
    /// `w:themeFontLang` — the languages theme font references resolve against.
    /// Additive: omitted when no language is stated.
    #[serde(default, skip_serializing_if = "ThemeFontLanguages::is_empty")]
    pub theme_font_languages: ThemeFontLanguages,
    /// `w:savePreviewPicture` — store a picture of the first page with the
    /// document, for file browsers. Additive (`109` FID-AT-10).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub save_preview_picture: bool,
    /// `w:compat/w:useFELayout` — lay out East Asian text with Word's
    /// East Asian rules regardless of the run's language. Additive.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub use_fe_layout: bool,
    /// `w:doNotAutoCompressPictures` — do not recompress pictures on save.
    /// Additive.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub do_not_auto_compress_pictures: bool,
    /// `w:decimalSymbol` — the decimal separator field codes and table
    /// formulas use (`.` in `en-US`, `,` in most of Europe). Non-empty and
    /// bounded to [`MAX_SETTINGS_TOKEN_BYTES`]. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decimal_symbol: Option<String>,
    /// `w:listSeparator` — the list separator field codes and table formulas
    /// use (`,` in `en-US`, `;` where the decimal symbol is `,`). Non-empty and
    /// bounded to [`MAX_SETTINGS_TOKEN_BYTES`]. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_separator: Option<String>,
    /// `w14:docId` — the document's Word 2010 identity, eight hexadecimal
    /// digits (`ST_LongHexNumber`). Word keeps it across saves; a save that
    /// dropped it made the edited file a different document to Word. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id_w14: Option<String>,
    /// `w15:docId` — the document's Word 2013 identity, a braced GUID.
    /// Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id_w15: Option<String>,
    /// `w14:defaultImageDpi` — the resolution pictures are compressed to when
    /// they are compressed (`220`, `150`, `96`, …), bounded 1..=32,767. Word's
    /// "High fidelity" (do not compress) is stored as `32767`, which a bound of
    /// 10,000 refused and reported until the owner's documents showed it.
    /// Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_image_dpi: Option<u32>,
    /// `m:mathPr` — the document's equation defaults (math font, break rules,
    /// margins, limit placement), retained VERBATIM as one serialized element.
    ///
    /// Verbatim rather than typed because nothing in this engine consumes it
    /// yet and fourteen typed fields would be modelling for its own sake; the
    /// one thing that matters is that a save keeps it. Written back between
    /// `w:compat` and `w:themeFontLang`, where `CT_Settings` puts it. Bounded to
    /// [`MAX_SETTINGS_FRAGMENT_BYTES`] and checked again by the writer, which
    /// refuses a fragment that is not one well-formed `m:mathPr` element in
    /// the four namespaces it declares. Additive (`109` FID-AT-10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub math_properties_xml: Option<String>,
    /// `w:shapeDefaults` — the VML defaults for new shapes (`o:shapedefaults`,
    /// `o:shapelayout`), retained VERBATIM as one serialized element, for the
    /// reason and under the bounds [`DocumentSettings::math_properties_xml`]
    /// states. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_defaults_xml: Option<String>,
    /// `w:hdrShapeDefaults` — the same VML defaults for shapes in headers and
    /// footers, retained VERBATIM like [`DocumentSettings::shape_defaults_xml`].
    /// Word writes one into most documents it saves (`spidmax`, the highest
    /// shape id used). Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_shape_defaults_xml: Option<String>,
    /// `w:drawingGrid*` and `w:displayHorizontal/VerticalDrawingGridEvery` —
    /// Word's drawing grid (Layout ▸ Align ▸ Grid Settings). Additive.
    #[serde(default, skip_serializing_if = "DrawingGrid::is_empty")]
    pub drawing_grid: DrawingGrid,
    /// The legacy `w:compat` switches that are on, by local name, in
    /// [`LEGACY_COMPAT_OPTIONS`] order — every `CT_Compat` on/off child except
    /// the two typed ones above (`w:adjustLineHeightInTable`, `w:useFELayout`).
    ///
    /// Kept and written back so an edited save does not change how Word lays
    /// the document out; the layout engine here does not interpret them (most
    /// apply only to documents in an older compatibility mode). A switch stated
    /// off is the default and is not stored. Additive.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compat_options: Vec<String>,
    /// `w:attachedTemplate` — the target of the template the document was based
    /// on (`Normal.dotm`, or a path or URL), from `settings.xml.rels`. Word keeps
    /// it across saves; nothing here opens it. At most
    /// [`MAX_ATTACHED_TEMPLATE_BYTES`]. Additive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_template: Option<String>,
}

/// The longest attached-template target kept ([`DocumentSettings::attached_template`]).
pub const MAX_ATTACHED_TEMPLATE_BYTES: usize = 2048;

/// Every on/off child of `w:compat` (`CT_Compat`, ECMA-376 Part 1 §17.15.3), in
/// schema order, except `w:compatSetting`. [`DocumentSettings::compat_options`]
/// holds the ones that are on, other than the two typed fields.
pub const LEGACY_COMPAT_OPTIONS: [&str; 65] = [
    "useSingleBorderforContiguousCells",
    "wpJustification",
    "noTabHangInd",
    "noLeading",
    "spaceForUL",
    "noColumnBalance",
    "balanceSingleByteDoubleByteWidth",
    "noExtraLineSpacing",
    "doNotLeaveBackslashAlone",
    "ulTrailSpace",
    "doNotExpandShiftReturn",
    "spacingInWholePoints",
    "lineWrapLikeWord6",
    "printBodyTextBeforeHeader",
    "printColBlack",
    "wpSpaceWidth",
    "showBreaksInFrames",
    "subFontBySize",
    "suppressBottomSpacing",
    "suppressTopSpacing",
    "suppressSpacingAtTopOfPage",
    "suppressTopSpacingWP",
    "suppressSpBfAfterPgBrk",
    "swapBordersFacingPages",
    "convMailMergeEsc",
    "truncateFontHeightsLikeWP6",
    "mwSmallCaps",
    "usePrinterMetrics",
    "doNotSuppressParagraphBorders",
    "wrapTrailSpaces",
    "footnoteLayoutLikeWW8",
    "shapeLayoutLikeWW8",
    "alignTablesRowByRow",
    "forgetLastTabAlignment",
    "adjustLineHeightInTable",
    "autoSpaceLikeWord95",
    "noSpaceRaiseLower",
    "doNotUseHTMLParagraphAutoSpacing",
    "layoutRawTableWidth",
    "layoutTableRowsApart",
    "useWord97LineBreakRules",
    "doNotBreakWrappedTables",
    "doNotSnapToGridInCell",
    "selectFldWithFirstOrLastChar",
    "applyBreakingRules",
    "doNotWrapTextWithPunct",
    "doNotUseEastAsianBreakRules",
    "useWord2002TableStyleRules",
    "growAutofit",
    "useFELayout",
    "useNormalStyleForList",
    "doNotUseIndentAsNumberingTabStop",
    "useAltKinsokuLineBreakRules",
    "allowSpaceOfSameStyleInTable",
    "doNotSuppressIndentation",
    "doNotAutofitConstrainedTables",
    "autofitToFirstFixedWidthCell",
    "underlineTabInNumList",
    "displayHangulFixedWidth",
    "splitPgBreakAndParaMark",
    "doNotVertAlignCellWithSp",
    "doNotBreakConstrainedForcedTable",
    "doNotVertAlignInTxbx",
    "useAnsiKerningPairs",
    "cachedColBalance",
];

/// Word's drawing grid (`w:drawingGridHorizontalSpacing` and its six siblings,
/// ECMA-376 Part 1 §17.15.1.43–48). Each value is what the file stated; `None`
/// and `false` are the schema's absent. Spacings and origins are twips, bounded
/// like a tab stop (0..=31,680); the "every" counts are 0..=32,767.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DrawingGrid {
    /// `w:drawingGridHorizontalSpacing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal_spacing: Option<u32>,
    /// `w:drawingGridVerticalSpacing`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_spacing: Option<u32>,
    /// `w:displayHorizontalDrawingGridEvery`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_horizontal_every: Option<u32>,
    /// `w:displayVerticalDrawingGridEvery`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_vertical_every: Option<u32>,
    /// `w:doNotUseMarginsForDrawingGridOrigin`.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub do_not_use_margins_for_origin: bool,
    /// `w:drawingGridHorizontalOrigin`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal_origin: Option<u32>,
    /// `w:drawingGridVerticalOrigin`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_origin: Option<u32>,
}

impl DrawingGrid {
    /// The largest spacing or origin kept, in twips (22 inches).
    pub const MAX_TWIPS: u32 = 31_680;
    /// The largest "display every" count kept.
    pub const MAX_EVERY: u32 = 32_767;

    /// Whether the file stated nothing about the grid.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Maximum UTF-8 length of a settings token (`w:decimalSymbol`,
/// `w:listSeparator`).
pub const MAX_SETTINGS_TOKEN_BYTES: usize = 255;

/// Maximum UTF-8 length of a verbatim settings fragment
/// ([`DocumentSettings::math_properties_xml`],
/// [`DocumentSettings::shape_defaults_xml`]). Word's own are a few hundred
/// bytes; this bounds a hostile snapshot.
pub const MAX_SETTINGS_FRAGMENT_BYTES: usize = 64 * 1024;

impl DocumentSettings {
    /// True when no setting departs from the default (so the part is omitted).
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Whether `token` is a storable `w:decimalSymbol`/`w:listSeparator`
    /// value: non-empty and within [`MAX_SETTINGS_TOKEN_BYTES`].
    #[must_use]
    pub fn is_valid_token(token: &str) -> bool {
        !token.is_empty() && token.len() <= MAX_SETTINGS_TOKEN_BYTES
    }

    /// Whether `id` is a `w14:docId` value: `ST_LongHexNumber`, one to eight
    /// hexadecimal digits (Word writes eight).
    #[must_use]
    pub fn is_valid_document_id_w14(id: &str) -> bool {
        (1..=8).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    /// Whether `id` is a `w15:docId` value: a braced GUID,
    /// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`.
    #[must_use]
    pub fn is_valid_document_id_w15(id: &str) -> bool {
        let Some(inner) = id.strip_prefix('{').and_then(|rest| rest.strip_suffix('}')) else {
            return false;
        };
        let groups: Vec<&str> = inner.split('-').collect();
        groups.len() == 5
            && groups.iter().zip([8, 4, 4, 4, 12]).all(|(group, length)| {
                group.len() == length && group.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
    }

    /// Whether `dpi` is a storable `w14:defaultImageDpi`: 1..=32,767, the top
    /// being Word's "High fidelity".
    #[must_use]
    pub const fn is_valid_image_dpi(dpi: u32) -> bool {
        dpi >= 1 && dpi <= 32_767
    }

    /// Whether `name` is a `w:compat` switch [`Self::compat_options`] may hold.
    #[must_use]
    pub fn is_compat_option(name: &str) -> bool {
        name != "adjustLineHeightInTable"
            && name != "useFELayout"
            && LEGACY_COMPAT_OPTIONS.contains(&name)
    }
}

/// Maximum number of `w:lsdException` entries retained from a latent-styles
/// block. Word's default template declares a few hundred; this bounds a hostile
/// part.
pub const MAX_LATENT_STYLE_EXCEPTIONS: usize = 4096;

/// Maximum length, in UTF-8 bytes, of a latent-style exception name.
pub const MAX_LATENT_STYLE_NAME_BYTES: usize = 255;

/// The `w:latentStyles` block from `word/styles.xml`.
///
/// Word emits this block to declare the UI defaults (sort priority, hidden /
/// quick-format state, …) it applies to built-in styles that are *not*
/// explicitly defined in the part, plus per-style [`LsdException`] overrides.
/// Modeled as typed defaults so the block round-trips without being dropped.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LatentStyles {
    /// `w:defLockedState` — the default locked state for latent styles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_locked_state: Option<bool>,
    /// `w:defUIPriority` — the default UI sort priority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_ui_priority: Option<i32>,
    /// `w:defSemiHidden` — the default semi-hidden state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_semi_hidden: Option<bool>,
    /// `w:defUnhideWhenUsed` — the default unhide-when-used state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_unhide_when_used: Option<bool>,
    /// `w:defQFormat` — the default primary (quick-format) state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_q_format: Option<bool>,
    /// `w:count` — the number of built-in styles Word declares defaults for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<i32>,
    /// Per-style overrides (`w:lsdException`), in document order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exceptions: Vec<LsdException>,
}

/// A `w:lsdException`: a per-style override of the latent-style UI defaults.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LsdException {
    /// `w:name` — the built-in style's primary name (non-empty, bounded).
    pub name: String,
    /// `w:locked`, if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    /// `w:uiPriority`, if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_priority: Option<i32>,
    /// `w:semiHidden`, if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semi_hidden: Option<bool>,
    /// `w:unhideWhenUsed`, if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unhide_when_used: Option<bool>,
    /// `w:qFormat`, if declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub q_format: Option<bool>,
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
    /// Paragraph-spanning complex field definitions by id — the instruction of
    /// each `FieldRangeStart`/`FieldRangeEnd` pair in body flow. Additive:
    /// omitted when empty so existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub field_ranges: DefinitionMap<FieldRangeId, FieldRange>,
    /// Typed chart projections by id — a **read projection of a retained part**,
    /// never a replacement for it (`docs/155` §6.1). A `Chart` names the
    /// `EmbeddedObject` node it describes through `Chart::object`; the chart
    /// part's bytes stay in the import side-table and are re-emitted verbatim.
    ///
    /// A side table rather than a field on `EmbeddedObject` for two reasons:
    /// derived data about an opaque part does not belong in the node model
    /// (`docs/45` invariants I3/I4), and a new field on that struct is a breaking
    /// change to every one of its literals across the workspace with nothing for
    /// a merge to conflict on (`SKILL` §5a shape 1). Additive: omitted when empty
    /// so existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub charts: DefinitionMap<ChartId, Chart>,
    /// Document-wide defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_defaults: Option<DocumentDefaults>,
    /// The `w:latentStyles` block (built-in-style UI defaults). Additive:
    /// omitted when absent so existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latent_styles: Option<LatentStyles>,
    /// Font-table descriptors (`word/fontTable.xml`), in document order.
    /// Additive: omitted when empty so existing snapshots serialize
    /// byte-identically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub font_table: Vec<FontDescriptor>,
    /// Theme font scheme (`theme1.xml` `a:fontScheme`) against which theme font
    /// slots resolve. Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_scheme: Option<FontScheme>,
    /// Theme color scheme (`theme1.xml` `a:clrScheme`) against which `w:themeColor`
    /// references resolve. Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_scheme: Option<ColorScheme>,
    /// Theme format scheme (`theme1.xml` `a:fmtScheme`), retained verbatim as an
    /// opaque XML subtree so its fill/line/effect style lists round-trip without
    /// full DrawingML modeling. Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_scheme_xml: Option<String>,
    /// The modeled subset of that same format scheme, parsed for RESOLUTION while
    /// `format_scheme_xml` stays the source of truth for export.
    ///
    /// Two representations of one part is deliberate and the division is strict:
    /// the verbatim XML is what gets written back, so adding this changed no output
    /// byte, while the typed form is what a shape's `wps:style` reference resolves
    /// against. Parsing for export instead would have put every unmodeled entry —
    /// gradients, patterns, effect styles — at risk of being rewritten as something
    /// it is not. Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_scheme: Option<FormatScheme>,
    /// Shape theme-style references (`wps:style`), keyed by the shape's node id.
    ///
    /// A side table rather than a field on `GroupShape`: this is authored content and
    /// would sit naturally on the shape, but that struct has 23 literal construction
    /// sites across six crates and a new field breaks every one with nothing for a
    /// merge to conflict on (`SKILL` §5a shape 1) — the same reason `charts` is a side
    /// table. Additive: omitted when empty so existing snapshots serialize
    /// byte-identically.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub shape_styles: DefinitionMap<NodeId, ShapeStyleRef>,
    /// Drawing object names and titles (`wp:docPr`/`*:cNvPr` `@name`/`@title`),
    /// keyed by the object's node id — a side table for the reason
    /// [`ObjectName`] gives. Additive: omitted when empty so existing snapshots
    /// serialize byte-identically.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub object_names: DefinitionMap<NodeId, ObjectName>,
    /// Each section's `w:formProt` — whether the section is protected when the
    /// document's forms protection is in force — keyed by the section's id, for
    /// the sections whose `w:sectPr` states it (`109` FID-AT-06).
    ///
    /// A side table rather than a field on `SectionBoundary`, for the reason
    /// [`ObjectName`] gives: that struct has 78 literal construction sites
    /// across eight crates, and a new field breaks every one with nothing for a
    /// merge to conflict on (`SKILL` §5a shape 1). Read
    /// [`Definitions::section_form_protection`]. An absent entry is an absent
    /// element, which is NOT the same statement as `false`: with
    /// `w:documentProtection w:edit="forms"` enforced, a section without
    /// `w:formProt` is protected and one with `w:formProt w:val="false"` is not.
    ///
    /// Keys are not validated against `sections`: an edit that removes a
    /// section does not know this table, and refusing the edited document over
    /// an entry nothing reads would be worse than the entry. The writer looks
    /// each section up, so an orphan is simply never written.
    /// Additive: omitted when empty so existing snapshots serialize
    /// byte-identically.
    #[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
    pub form_protection: DefinitionMap<SectionId, bool>,
    /// Document-wide settings (`word/settings.xml`). Additive: omitted when
    /// default so existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "DocumentSettings::is_default")]
    pub settings: DocumentSettings,
    /// Collaborator identities (`word/people.xml`), keyed by author name; a
    /// `Comment` links to one through [`Comment::person`]. Additive: omitted when
    /// empty so existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<Person>,
}

impl Definitions {
    /// The single numbering resolver over this document's styles, numbering
    /// instances and abstract definitions — the one answer to "which level does
    /// this `w:numPr` paint?" that validation, layout and import all consult.
    ///
    /// Complexity: O(1); it only borrows three maps.
    #[must_use]
    pub fn numbering_resolver(&self) -> NumberingResolver<'_> {
        NumberingResolver::new(&self.styles, &self.numbering, &self.abstract_numbering)
    }

    /// Whether a resize of drawing object `id` must keep its aspect ratio —
    /// DrawingML's `noChangeAspect`, on the object's frame or on the object
    /// itself (`ObjectLocks::locks_aspect_ratio`, `109` FID-AT-09).
    ///
    /// This is the flag Word uses to make a corner drag of a picture
    /// proportional; Word writes it on every picture it inserts. `id` is the
    /// drawing node's id (`Drawing::id`, `AnchoredDrawing::id`, a group's or a
    /// group child's id). An object with no recorded locks is unlocked.
    ///
    /// Complexity: one side-table lookup, O(log n) in named or locked objects —
    /// fit for a per-interaction call.
    #[must_use]
    pub fn locks_aspect_ratio(&self, id: NodeId) -> bool {
        self.object_names
            .get(&id)
            .is_some_and(|entry| entry.locks.locks_aspect_ratio())
    }

    /// `section`'s `w:formProt` (`109` FID-AT-06): `Some(true)` or
    /// `Some(false)` where the section states it, `None` where it does not.
    ///
    /// `None` is not `Some(false)`. With `w:documentProtection w:edit="forms"`
    /// enforced, Word protects a section that states nothing and leaves one
    /// stating `false` editable, so a forms-protection check (`docs/165` M6)
    /// reads this per section rather than treating `forms` as document-wide.
    ///
    /// Complexity: one side-table lookup, O(log n) in sections.
    #[must_use]
    pub fn section_form_protection(&self, section: SectionId) -> Option<bool> {
        self.form_protection.get(&section).copied()
    }

    /// Whether `style` is **locked** — `w:locked`, ECMA-376 §17.7.4.6 "Style
    /// Cannot Be Applied".
    ///
    /// # What the attribute means, and the one condition it depends on
    ///
    /// `w:locked` is not an unconditional lock. It takes effect only while
    /// document protection is enforced **and** the formatting restriction
    /// (`w:documentProtection/@w:formatting`) is on; outside that, every style
    /// is applicable. This function answers only "is the flag set", because the
    /// condition belongs to the caller that holds the protection —
    /// `casual_doc_edit::protection`, which is this method's reason to exist.
    ///
    /// # Why it is a method here and not a field read at the call site
    ///
    /// Because the answer for a style the table does **not** define is not on
    /// any `Style`. `w:latentStyles/@w:defLockedState` is the declared default
    /// `w:locked` for the built-in styles a part leaves latent, so a style id
    /// that resolves to nothing inherits it. A caller reading `style.locked`
    /// directly would get `false` for that case by not looking, which is the
    /// shape of answer that reads as a decision and is an omission.
    ///
    /// `w:lsdException/@w:locked` — the per-style latent override — is
    /// deliberately **not** consulted, and this is a recorded limit rather than
    /// an oversight: an `LsdException` is keyed by the built-in style's
    /// `w:name` ("heading 1"), a [`StyleId`] carries a [`NodeId`] and no name,
    /// and the `w:name` that would bridge them lives on the `Style` that is by
    /// definition absent in exactly this case. Guessing that an id spells its
    /// own name would be a heuristic in an access decision, so the block's
    /// declared default is used and the exception list is not searched.
    ///
    /// # Complexity
    ///
    /// O(log n) in the style table — one `BTreeMap` lookup, no document walk.
    /// It runs on the edit path, so it may not scan (`docs/107` §4 B1); the
    /// exception list is never traversed, which is also why.
    #[must_use]
    pub fn style_locked(&self, style: StyleId) -> bool {
        match self.styles.get(&style) {
            Some(defined) => defined.locked,
            None => self
                .latent_styles
                .as_ref()
                .and_then(|latent| latent.default_locked_state)
                .unwrap_or(false),
        }
    }
}
