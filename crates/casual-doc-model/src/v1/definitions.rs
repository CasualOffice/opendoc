//! Style, numbering, section, theme, and media definition tables.

use serde::{Deserialize, Serialize};

use super::{
    AbstractNumberingId, BlockNode, BookmarkId, ColorScheme, CommentId, DefinitionMap,
    FontDescriptor, FontScheme, HeaderFooterId, MediaId, NoteId, NumberingInstanceId,
    ParagraphProperties, RunProperties, SectionId, StyleId, StyleKind,
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

/// Editing/formatting protection (`w:documentProtection`). The crypto attributes
/// (`w:cryptProviderType`, `w:hash`, `w:salt`, …) are the byte-floor's concern;
/// only the load-bearing policy attributes are modeled.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentProtection {
    /// The editing restriction (`w:edit`).
    pub edit: DocumentProtectionEdit,
    /// Whether the restriction is enforced (`w:enforcement`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub enforcement: bool,
    /// Whether style formatting is also locked (`w:formatting`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub formatting: bool,
}

/// Write protection (`w:writeProtection`) — the document is recommended or
/// required to be opened read-only. Presence (`Some`) is itself load-bearing; the
/// crypto attributes are the byte-floor's concern.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteProtection {
    /// Whether opening read-only is merely recommended (`w:recommended`).
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub recommended: bool,
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
    /// `w:trackChanges` — revision tracking is on.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub track_changes: bool,
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
    /// `w:compat`/`w:compatSetting` — the modeled compatibility-setting triples,
    /// in document order. Additive: omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compat: Vec<CompatSetting>,
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
    /// Theme color scheme (`theme1.xml` `a:clrScheme`) against which `w:themeColor`
    /// references resolve. Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_scheme: Option<ColorScheme>,
    /// Theme format scheme (`theme1.xml` `a:fmtScheme`), retained verbatim as an
    /// opaque XML subtree so its fill/line/effect style lists round-trip without
    /// full DrawingML modeling. Additive: omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_scheme_xml: Option<String>,
    /// Document-wide settings (`word/settings.xml`). Additive: omitted when
    /// default so existing snapshots serialize byte-identically.
    #[serde(default, skip_serializing_if = "DocumentSettings::is_default")]
    pub settings: DocumentSettings,
}
