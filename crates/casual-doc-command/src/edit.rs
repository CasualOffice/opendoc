// SPDX-License-Identifier: Apache-2.0

//! The edit vocabulary as types (`docs/181` §2.1).
//!
//! An edit is what an agent sends inside an edit plan: a tagged object whose
//! `op` names a command in [`crate::COMMANDS`]. These types are the single
//! source of the vocabulary: `casual-doc-ai` deserialises them, the JSON Schema
//! every front door publishes is derived from them ([`crate::schema`]), and a
//! test holds the set of `op`s here equal to the registry's ids.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which side of the target an insertion goes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Before the quote (or at the paragraph's start).
    Before,
    /// After the quote (or at the paragraph's end). The default.
    #[default]
    After,
}

/// A byte range a program passes back exactly as `read`/`search` returned it.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PosRange {
    /// Start, UTF-8 bytes into the node's text.
    pub start: u32,
    /// End, exclusive.
    pub end: u32,
}

/// A pointer at text: a paragraph id plus an optional quote, matched inside
/// that paragraph only. Never a document offset.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    /// A paragraph id returned by `get_outline`, `read_blocks` or `search`.
    pub node: String,
    /// The quoted text, matched inside `node` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exact: Option<String>,
    /// Text immediately before `exact`, to disambiguate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    /// Text immediately after `exact`, to disambiguate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suffix: Option<String>,
    /// 1-based occurrence of `exact` among the matches that remain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub occurrence: Option<u32>,
    /// Offsets for programs only; never computed by a model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<PosRange>,
    /// For an insertion point: before or after the target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<Side>,
}

/// Whether edits become tracked suggestions or land directly.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Tracked changes authored by the session's identity. The default.
    #[default]
    Suggest,
    /// Untracked edits; only under a profile that grants `edit`.
    Direct,
}

/// One new paragraph for `insert_paragraphs`.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NewParagraph {
    /// Its text.
    #[serde(default)]
    pub text: String,
    /// A paragraph style, by name (for example `Heading 2`) or id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
}

/// Character formatting an agent may set.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TextFormat {
    /// Bold on or off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    /// Italic on or off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    /// Underline on or off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    /// Strikethrough on or off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    /// Size in points (half-point precision), 1 to 1638.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    /// Colour, `#RRGGBB`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Highlight name (`yellow`, `green`, `cyan`, `magenta`, `blue`, `red`, …, `none`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<String>,
    /// Font family.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

/// A cell by its 0-based row and its 0-based cell within that row, as
/// `read_table` lists them.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CellRef {
    /// 0-based row.
    pub row: u32,
    /// 0-based cell in that row.
    pub column: u32,
}

/// A cell's vertical text alignment.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CellAlignment {
    /// Top.
    Top,
    /// Centred.
    Center,
    /// Bottom.
    Bottom,
}

/// Which way a cell's text runs (`w:textDirection`).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CellTextDirection {
    /// Left to right, the default.
    Horizontal,
    /// Rotated 90 degrees clockwise: read top to bottom.
    RotateDown,
    /// Rotated 90 degrees counter-clockwise: read bottom to top.
    RotateUp,
}

impl CellTextDirection {
    /// The name `table_tools::text_direction_named` reads.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Horizontal => "horizontal",
            Self::RotateDown => "rotate_down",
            Self::RotateUp => "rotate_up",
        }
    }
}

/// Horizontal paragraph alignment.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AlignmentName {
    /// Left in a left-to-right paragraph.
    Start,
    /// Centred.
    Center,
    /// Right in a left-to-right paragraph.
    End,
    /// Justified.
    Justify,
}

/// How a line-spacing value is read.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LineSpacingRule {
    /// `value` is a percentage of single spacing (100 single, 150, 200 double).
    Multiple,
    /// `value` is an exact line height in twips (1/20 pt).
    Exact,
    /// `value` is a minimum line height in twips.
    AtLeast,
}

/// Which list a paragraph belongs to.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ListKind {
    /// A bulleted list.
    Bullet,
    /// A decimal numbered list.
    Numbered,
    /// No list: the paragraph leaves its list.
    None,
}

/// A common field.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FieldName {
    /// The current page number (`PAGE`), recomputed at pagination.
    Page,
    /// The page count (`NUMPAGES`), recomputed at pagination.
    Numpages,
    /// The date (`DATE`); `result` is the date as it should read.
    Date,
    /// The time (`TIME`); `result` is the time as it should read.
    Time,
    /// The file name (`FILENAME`); `result` is the name as it should read.
    Filename,
    /// The author (`AUTHOR`); `result` is the name as it should read.
    Author,
}

impl FieldName {
    /// The editor's field-kind token. O(1).
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Page => "page",
            Self::Numpages => "numpages",
            Self::Date => "date",
            Self::Time => "time",
            Self::Filename => "filename",
            Self::Author => "author",
        }
    }
}

/// A break.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BreakName {
    /// A line break inside the paragraph (Shift+Enter).
    Line,
    /// A page break; body paragraphs only.
    Page,
    /// A column break; body paragraphs only.
    Column,
    /// Two page breaks, leaving one empty page; body paragraphs only.
    BlankPage,
}

/// Where a new section starts.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SectionStart {
    /// On the next page.
    NextPage,
    /// On the same page.
    Continuous,
    /// On the next even page.
    EvenPage,
    /// On the next odd page.
    OddPage,
}

impl SectionStart {
    /// The editor's token. O(1).
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::NextPage => "nextPage",
            Self::Continuous => "continuous",
            Self::EvenPage => "evenPage",
            Self::OddPage => "oddPage",
        }
    }
}

/// Page orientation.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OrientationName {
    /// Taller than wide.
    Portrait,
    /// Wider than tall.
    Landscape,
}

/// Page margins in twips; each one omitted keeps its value.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Margins {
    /// Top margin, twips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 31680))]
    pub top: Option<i32>,
    /// Bottom margin, twips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 31680))]
    pub bottom: Option<i32>,
    /// Left (start) margin, twips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 31680))]
    pub left: Option<i32>,
    /// Right (end) margin, twips.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 31680))]
    pub right: Option<i32>,
}

/// A page-number format.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PageNumberFormat {
    /// 1, 2, 3.
    Decimal,
    /// i, ii, iii.
    LowerRoman,
    /// I, II, III.
    UpperRoman,
    /// a, b, c.
    LowerLetter,
    /// A, B, C.
    UpperLetter,
}

/// When margin line numbers start again.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum LineNumberRestartName {
    /// On every page.
    EachPage,
    /// At every section.
    EachSection,
    /// Never.
    Continuous,
}

/// The angle of a watermark.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WatermarkAngle {
    /// Rising across the page (Word's default).
    Diagonal,
    /// Level with the text.
    Horizontal,
}

/// Which pages of a section a header or footer is for.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PageKind {
    /// Every page that has no first-page or even-page variant (the default).
    #[default]
    Default,
    /// The section's first page (needs a different first page).
    First,
    /// Even pages (needs different odd and even pages).
    Even,
}

/// A header or a footer.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    /// The header.
    Header,
    /// The footer.
    Footer,
}

/// Where across a header or footer a page number goes.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PageNumberSide {
    /// At the start edge (left in left-to-right text).
    Left,
    /// Centred.
    Center,
    /// At the end edge (right in left-to-right text).
    Right,
}

/// How text wraps around a floating object.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WrapName {
    /// Around its bounding box.
    Square,
    /// Around its outline.
    Tight,
    /// Through its outline's gaps.
    Through,
    /// Above and below only.
    TopAndBottom,
    /// No wrap, behind the text.
    Behind,
    /// No wrap, over the text.
    Front,
}

impl WrapName {
    /// The editor's token. O(1).
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Square => "square",
            Self::Tight => "tight",
            Self::Through => "through",
            Self::TopAndBottom => "topAndBottom",
            Self::Behind => "behind",
            Self::Front => "front",
        }
    }
}

/// A chart family this build paints.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
pub enum ChartKind {
    /// Vertical bars side by side.
    #[serde(rename = "column")]
    Column,
    /// Vertical bars stacked.
    #[serde(rename = "column-stacked")]
    ColumnStacked,
    /// Vertical bars stacked to 100%.
    #[serde(rename = "column-percent")]
    ColumnPercent,
    /// Horizontal bars side by side.
    #[serde(rename = "bar")]
    Bar,
    /// Horizontal bars stacked.
    #[serde(rename = "bar-stacked")]
    BarStacked,
    /// Horizontal bars stacked to 100%.
    #[serde(rename = "bar-percent")]
    BarPercent,
    /// Lines.
    #[serde(rename = "line")]
    Line,
    /// Lines with markers.
    #[serde(rename = "line-markers")]
    LineMarkers,
    /// Stacked lines.
    #[serde(rename = "line-stacked")]
    LineStacked,
    /// Lines stacked to 100%.
    #[serde(rename = "line-percent")]
    LinePercent,
    /// Areas.
    #[serde(rename = "area")]
    Area,
    /// Stacked areas.
    #[serde(rename = "area-stacked")]
    AreaStacked,
    /// Areas stacked to 100%.
    #[serde(rename = "area-percent")]
    AreaPercent,
    /// X/Y points.
    #[serde(rename = "scatter")]
    Scatter,
    /// X/Y points joined by smooth lines.
    #[serde(rename = "scatter-smooth")]
    ScatterSmooth,
    /// A pie.
    #[serde(rename = "pie")]
    Pie,
    /// A doughnut.
    #[serde(rename = "doughnut")]
    Doughnut,
}

impl ChartKind {
    /// The editor's token (`casual_doc_command::charts::KINDS`). O(1).
    #[must_use]
    pub const fn token(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::ColumnStacked => "column-stacked",
            Self::ColumnPercent => "column-percent",
            Self::Bar => "bar",
            Self::BarStacked => "bar-stacked",
            Self::BarPercent => "bar-percent",
            Self::Line => "line",
            Self::LineMarkers => "line-markers",
            Self::LineStacked => "line-stacked",
            Self::LinePercent => "line-percent",
            Self::Area => "area",
            Self::AreaStacked => "area-stacked",
            Self::AreaPercent => "area-percent",
            Self::Scatter => "scatter",
            Self::ScatterSmooth => "scatter-smooth",
            Self::Pie => "pie",
            Self::Doughnut => "doughnut",
        }
    }
}

/// Which update a table of contents gets.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TocUpdateName {
    /// Keep the entries; re-read every entry's page number.
    PageNumbers,
    /// Rebuild the entries from the current headings.
    Entire,
}

impl TocUpdateName {
    /// The lowering's mode. O(1).
    #[must_use]
    pub const fn mode(self) -> crate::references::TocUpdate {
        match self {
            Self::PageNumbers => crate::references::TocUpdate::PageNumbers,
            Self::Entire => crate::references::TocUpdate::Entire,
        }
    }
}

/// Where a caption goes relative to what it captions.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CaptionPlacement {
    /// Above it.
    Above,
    /// Below it (the default).
    #[default]
    Below,
}

impl CaptionPlacement {
    /// The engine's position. O(1).
    #[must_use]
    pub const fn position(self) -> casual_doc_edit::references::CaptionPosition {
        match self {
            Self::Above => casual_doc_edit::references::CaptionPosition::Above,
            Self::Below => casual_doc_edit::references::CaptionPosition::Below,
        }
    }
}

/// How a caption's number is written.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CaptionNumbering {
    /// 1, 2, 3 (the default).
    #[default]
    Arabic,
    /// I, II, III.
    UpperRoman,
    /// i, ii, iii.
    LowerRoman,
    /// A, B, C.
    UpperLetter,
    /// a, b, c.
    LowerLetter,
}

impl CaptionNumbering {
    /// The engine's number format. O(1).
    #[must_use]
    pub const fn format(self) -> casual_doc_edit::references::CaptionNumberFormat {
        use casual_doc_edit::references::CaptionNumberFormat as F;
        match self {
            Self::Arabic => F::Arabic,
            Self::UpperRoman => F::UpperRoman,
            Self::LowerRoman => F::LowerRoman,
            Self::UpperLetter => F::UpperLetter,
            Self::LowerLetter => F::LowerLetter,
        }
    }
}

/// The separator between a chapter number and a caption number.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChapterSeparator {
    /// `1-1` (the default, as Word's).
    #[default]
    Hyphen,
    /// `1.1`.
    Period,
    /// `1:1`.
    Colon,
    /// `1—1`.
    EmDash,
    /// `1–1`.
    EnDash,
}

impl ChapterSeparator {
    /// The engine's separator. O(1).
    #[must_use]
    pub const fn separator(self) -> casual_doc_edit::references::CaptionSeparator {
        use casual_doc_edit::references::CaptionSeparator as S;
        match self {
            Self::Hyphen => S::Hyphen,
            Self::Period => S::Period,
            Self::Colon => S::Colon,
            Self::EmDash => S::EmDash,
            Self::EnDash => S::EnDash,
        }
    }
}

/// What a cross-reference shows.
///
/// Word's paragraph-number kinds (`\r`, `\n`, `\w`) are not offered: the
/// shared lowering caches the paragraph's text as their result rather than its
/// list number, which a reader would see until the field is updated
/// (`docs/181` §12, lane references).
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    /// A caption's label, number and text.
    EntireCaption,
    /// A caption's label and number only (`Figure 2`).
    LabelAndNumber,
    /// A caption's text after its number.
    CaptionText,
    /// The page label of the page the target is on.
    PageNumber,
    /// The word `above` or `below`, from where the reference sits.
    AboveBelow,
    /// The target paragraph's text (a heading's, a bookmarked paragraph's).
    ParagraphText,
}

impl ReferenceKind {
    /// The engine's reference kind. O(1).
    #[must_use]
    pub const fn reference_to(self) -> casual_doc_edit::references::ReferenceTo {
        use casual_doc_edit::references::ReferenceTo as R;
        match self {
            Self::EntireCaption => R::EntireCaption,
            Self::LabelAndNumber => R::LabelAndNumber,
            Self::CaptionText => R::CaptionTextOnly,
            Self::PageNumber => R::PageNumber,
            Self::AboveBelow => R::AboveBelow,
            Self::ParagraphText => R::ParagraphText,
        }
    }
}

/// Accept or reject.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, JsonSchema, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    /// Keep the change.
    Accept,
    /// Undo the change.
    Reject,
}

/// One edit. The `op` tag is a registry id (`crate::COMMANDS`); a test holds
/// the two sets equal, so adding a variant without registering it (or the
/// reverse) fails.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Edit {
    /// Insert text at an anchor.
    InsertText {
        /// Where.
        at: Anchor,
        /// What.
        text: String,
    },
    /// Replace the quoted text.
    ReplaceText {
        /// The text to replace.
        target: Anchor,
        /// Its replacement.
        text: String,
    },
    /// Delete the quoted text.
    DeleteText {
        /// The text to delete.
        target: Anchor,
    },
    /// Insert new paragraphs after an anchor's paragraph.
    InsertParagraphs {
        /// The paragraph they follow.
        after: Anchor,
        /// The new paragraphs, in order.
        paragraphs: Vec<NewParagraph>,
    },
    /// Apply a paragraph style.
    ApplyParagraphStyle {
        /// The paragraph.
        target: Anchor,
        /// The style, by name or id.
        style: String,
    },
    /// Set character formatting on the quoted text.
    SetTextFormat {
        /// The text.
        target: Anchor,
        /// What to set.
        format: TextFormat,
    },
    /// Remove direct character formatting from the quoted text; the
    /// paragraph's style stays.
    ClearFormatting {
        /// The text.
        target: Anchor,
    },
    /// Rewrite one paragraph from Markdown in the dialect `read_blocks`
    /// emits; the engine diffs it word by word against the paragraph and
    /// changes only what differs.
    RewriteBlock {
        /// The paragraph (its `node`; a quote is not used).
        target: Anchor,
        /// The paragraph's new content: an optional `{#id}` prefix (it must
        /// name `target`), an optional `#`..`######` heading marker, then the
        /// text, on one line.
        markdown: String,
    },
    /// Set a paragraph's horizontal alignment.
    SetAlignment {
        /// The paragraph.
        target: Anchor,
        /// The alignment.
        alignment: AlignmentName,
    },
    /// Set a paragraph's line spacing.
    SetLineSpacing {
        /// The paragraph.
        target: Anchor,
        /// How `value` is read.
        rule: LineSpacingRule,
        /// Percent of single for `multiple`; twips for `exact` and `at_least`.
        #[schemars(range(min = 0, max = 31680))]
        value: i32,
    },
    /// Set the space before and/or after a paragraph.
    SetParagraphSpacing {
        /// The paragraph.
        target: Anchor,
        /// Twips before; negative clears it back to the style's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        before: Option<i32>,
        /// Twips after; negative clears it back to the style's.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<i32>,
    },
    /// Set a paragraph's indents.
    SetIndent {
        /// The paragraph.
        target: Anchor,
        /// Start (left) indent in twips, at least 0.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<i32>,
        /// End (right) indent in twips, at least 0.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<i32>,
        /// First-line indent in twips: positive indents, negative hangs, 0
        /// clears both.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        first_line: Option<i32>,
    },
    /// Set the text of one table cell.
    SetCellText {
        /// The table id.
        table: String,
        /// 0-based row.
        row: u32,
        /// 0-based cell in that row.
        column: u32,
        /// The new text.
        text: String,
    },
    /// Insert an empty row so it lands at `index`.
    TableInsertRow {
        /// The table id.
        table: String,
        /// 0 = above the first row, the row count = below the last.
        index: u32,
    },
    /// Delete one row.
    TableDeleteRow {
        /// The table id.
        table: String,
        /// 0-based row.
        index: u32,
    },
    /// Insert an empty table after an anchor's top-level block.
    InsertTable {
        /// A paragraph; the table goes after the body block holding it.
        after: Anchor,
        /// Rows, 1 to 50.
        #[schemars(range(min = 1, max = 50))]
        rows: u32,
        /// Columns, 1 to 20.
        #[schemars(range(min = 1, max = 20))]
        columns: u32,
    },
    /// Insert an empty column so it lands at grid column `index`.
    TableInsertColumn {
        /// The table id.
        table: String,
        /// 0 = left of the first column, the column count = right of the last.
        index: u32,
    },
    /// Delete one grid column.
    TableDeleteColumn {
        /// The table id.
        table: String,
        /// 0-based grid column.
        index: u32,
    },
    /// Merge the rectangle of cells between two cells into one.
    TableMergeCells {
        /// The table id.
        table: String,
        /// One corner.
        from: CellRef,
        /// The opposite corner.
        to: CellRef,
    },
    /// Unmerge the merged cell a cell belongs to.
    TableSplitCell {
        /// The table id.
        table: String,
        /// A cell of the merged cell.
        cell: CellRef,
    },
    /// Delete a whole table.
    TableDelete {
        /// The table id.
        table: String,
    },
    /// Set the shading and/or vertical alignment of one cell or a rectangle.
    TableSetCellFormat {
        /// The table id.
        table: String,
        /// The cell, or one corner of the rectangle.
        from: CellRef,
        /// The opposite corner; omitted for one cell.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<CellRef>,
        /// Fill `#RRGGBB`, or `none` to clear.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shading: Option<String>,
        /// Vertical alignment of the text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        vertical_alignment: Option<CellAlignment>,
        /// Which way the text runs: horizontal, rotated down (read top to
        /// bottom) or rotated up (read bottom to top).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text_direction: Option<CellTextDirection>,
    },
    /// Put a paragraph in a bulleted or numbered list, or take it out.
    SetList {
        /// The paragraph.
        target: Anchor,
        /// Which list.
        kind: ListKind,
    },
    /// Move a list item down (`delta` > 0) or up a level.
    ChangeListLevel {
        /// The list item.
        target: Anchor,
        /// Levels to move, clamped to levels 0..=8.
        #[schemars(range(min = -8, max = 8))]
        delta: i32,
    },
    /// Restart numbering at a numbered list item.
    RestartNumbering {
        /// The list item.
        target: Anchor,
        /// The number it starts at, 1 to 32767 (default 1).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1, max = 32767))]
        value: Option<u16>,
    },
    /// Continue numbering from the nearest earlier list at the item's level.
    ContinueNumbering {
        /// The list item.
        target: Anchor,
    },
    /// Link the quoted text to a URL or a bookmark.
    SetHyperlink {
        /// The text to link (a non-empty quote).
        target: Anchor,
        /// An `http`, `https`, `mailto`, `ftp` or `tel` URL, or `#` and a
        /// bookmark name.
        url: String,
        /// A tooltip shown on hover.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tooltip: Option<String>,
    },
    /// Remove the hyperlink whose text is exactly the quote; the text stays.
    RemoveHyperlink {
        /// The linked text, exactly.
        target: Anchor,
    },
    /// Bookmark the quoted text (or, with no quote, a point in the paragraph).
    CreateBookmark {
        /// What the bookmark spans.
        target: Anchor,
        /// Its name: a letter first, then letters, digits or `_`, at most 40
        /// characters, as Word requires.
        name: String,
    },
    /// Rename a bookmark.
    RenameBookmark {
        /// Its current name.
        name: String,
        /// Its new name.
        new_name: String,
    },
    /// Delete a bookmark; the text it spans stays.
    DeleteBookmark {
        /// Its name.
        name: String,
    },
    /// Insert a field at an anchor.
    InsertField {
        /// Where.
        at: Anchor,
        /// Which field.
        kind: FieldName,
        /// The displayed value of a date, time, file-name or author field
        /// (the engine reads no clock or file system); ignored for page
        /// numbers, which are computed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        result: Option<String>,
    },
    /// Insert a picture at an anchor.
    InsertImage {
        /// Where.
        at: Anchor,
        /// The image bytes, standard base64, at most 8 MiB decoded.
        data: String,
        /// Its MIME type: image/png, image/jpeg, image/gif, image/bmp,
        /// image/tiff or image/webp; the bytes must be that format.
        content_type: String,
        /// Width in points; with `height` omitted too, the picture's own
        /// size at 96 DPI (PNG, GIF, JPEG and BMP state it).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width_pt: Option<f64>,
        /// Height in points; omitted, it keeps the picture's proportions.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height_pt: Option<f64>,
        /// Alt text for readers who cannot see it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        alt_text: Option<String>,
    },
    /// Set or clear an object's alt text.
    SetAltText {
        /// The object id `read_blocks` lists.
        object: String,
        /// The alt text; omitted or empty clears it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// Resize a picture, shape or text box.
    ResizeObject {
        /// The object id.
        object: String,
        /// Width in points.
        width_pt: f64,
        /// Height in points.
        height_pt: f64,
    },
    /// Set how text wraps around a floating object.
    SetWrap {
        /// The object id.
        object: String,
        /// The wrap.
        wrap: WrapName,
    },
    /// Crop a picture: the fraction (0 to 1) of each edge to hide; all zero
    /// removes the crop.
    CropImage {
        /// The picture id.
        object: String,
        /// Left edge.
        #[schemars(range(min = 0, max = 1))]
        left: f64,
        /// Top edge.
        #[schemars(range(min = 0, max = 1))]
        top: f64,
        /// Right edge.
        #[schemars(range(min = 0, max = 1))]
        right: f64,
        /// Bottom edge.
        #[schemars(range(min = 0, max = 1))]
        bottom: f64,
    },
    /// Delete a picture, shape, text box or group.
    DeleteObject {
        /// The object id.
        object: String,
    },
    /// Set a shape's fill.
    SetShapeFill {
        /// The shape id.
        shape: String,
        /// `#RRGGBB`, or `none` to remove the fill.
        color: String,
    },
    /// Set a shape's or a picture's outline colour and/or weight.
    SetShapeOutline {
        /// The shape or picture id.
        shape: String,
        /// `#RRGGBB`, or `none` to remove the outline.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        /// Weight in points.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 0, max = 1584))]
        width_pt: Option<f64>,
    },
    /// Insert a footnote at an anchor, with its text.
    InsertFootnote {
        /// Where the reference mark goes.
        at: Anchor,
        /// The note's text.
        text: String,
    },
    /// Insert an endnote at an anchor, with its text.
    InsertEndnote {
        /// Where the reference mark goes.
        at: Anchor,
        /// The note's text.
        text: String,
    },
    /// Insert a line, page or column break, or a blank page, at an anchor.
    InsertBreak {
        /// Where.
        at: Anchor,
        /// Which break.
        kind: BreakName,
    },
    /// Start a new section at an anchor; it inherits the current section's
    /// page setup, headers and footers.
    InsertSectionBreak {
        /// Where; a top-level body paragraph.
        at: Anchor,
        /// Where the new section starts.
        start: SectionStart,
    },
    /// Change a section's page size, orientation, margins or columns.
    SetPageSetup {
        /// The 0-based section (`get_outline` lists them); omitted, every
        /// section.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<u32>,
        /// Page width, twips (Letter is 12240, A4 11906).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1440, max = 31680))]
        width: Option<i32>,
        /// Page height, twips (Letter is 15840, A4 16838).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1440, max = 31680))]
        height: Option<i32>,
        /// Orientation; a change swaps width and height unless both are given.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        orientation: Option<OrientationName>,
        /// Margins, twips.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        margins: Option<Margins>,
        /// Number of text columns, 1 to 45.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1, max = 45))]
        columns: Option<u16>,
        /// Space between columns, twips.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 0, max = 31680))]
        column_spacing: Option<i32>,
    },
    /// Number the lines of a section in the margin, or stop.
    SetLineNumbering {
        /// The 0-based section; omitted, every section.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<u32>,
        /// Number every Nth line; 0 turns numbering off.
        #[schemars(range(min = 0, max = 100))]
        count_by: i32,
        /// The first number (default 1).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1, max = 32767))]
        start: Option<i32>,
        /// When numbering restarts (default each page).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        restart: Option<LineNumberRestartName>,
    },
    /// Set how a section's pages are numbered.
    SetPageNumbering {
        /// The 0-based section; omitted, every section.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<u32>,
        /// The format; omitted, decimal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        format: Option<PageNumberFormat>,
        /// Restart at this number; omitted, continue from the previous
        /// section.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 0, max = 32767))]
        start: Option<i32>,
    },
    /// Stamp a text watermark on every page, or remove it.
    SetWatermark {
        /// The words; omitted or empty removes the watermark.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        /// `#RRGGBB` (default light grey).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
        /// The angle (default diagonal).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        layout: Option<WatermarkAngle>,
        /// The font family (default the document's).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        font: Option<String>,
    },
    /// Add an empty header to a section, ready for insert_text.
    InsertHeader {
        /// The 0-based section (default 0).
        #[serde(default)]
        section: u32,
        /// Which pages it is for (default every page).
        #[serde(default)]
        pages: PageKind,
    },
    /// Add an empty footer to a section, ready for insert_text.
    InsertFooter {
        /// The 0-based section (default 0).
        #[serde(default)]
        section: u32,
        /// Which pages it is for (default every page).
        #[serde(default)]
        pages: PageKind,
    },
    /// Give a section's first page its own header and footer, or not.
    SetDifferentFirstPage {
        /// The 0-based section.
        section: u32,
        /// On or off.
        enabled: bool,
    },
    /// Give even and odd pages their own headers and footers, or not
    /// (document-wide).
    SetDifferentOddEven {
        /// On or off.
        enabled: bool,
    },
    /// Link a section's header or footer to the previous section's (true), or
    /// give the section its own copy of it to change on its own (false).
    LinkToPrevious {
        /// The 0-based section; never 0, which has no previous section.
        section: u32,
        /// The header or the footer.
        band: Band,
        /// Which pages (default every page).
        #[serde(default)]
        pages: PageKind,
        /// Linked or not.
        linked: bool,
    },
    /// Remove the header or footer a section shows, from that section only.
    RemoveHeaderFooter {
        /// The 0-based section.
        section: u32,
        /// The header or the footer.
        band: Band,
        /// Which pages (default every page).
        #[serde(default)]
        pages: PageKind,
    },
    /// Put a page number in a section's header or footer, left, centred or
    /// right; the band is created when there is none, and its content is kept.
    InsertPageNumber {
        /// The 0-based section.
        section: u32,
        /// The header or the footer.
        band: Band,
        /// Which pages (default every page).
        #[serde(default)]
        pages: PageKind,
        /// Where across the band.
        side: PageNumberSide,
    },
    /// Insert a chart with sample data at an anchor.
    InsertChart {
        /// Where.
        at: Anchor,
        /// The chart family.
        kind: ChartKind,
    },
    /// Insert a table of contents before a body paragraph: one entry per
    /// heading, with the page it is on, as one field an update finds again.
    InsertTableOfContents {
        /// The body paragraph the table goes before (its id; a quote is not
        /// needed).
        before: Anchor,
        /// The deepest heading level listed, 1 to 9; 3 when omitted.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1, max = 9))]
        levels: Option<u8>,
    },
    /// Update a table of contents.
    UpdateTableOfContents {
        /// `page_numbers` keeps the entries and re-reads their pages;
        /// `entire` rebuilds the entries from the headings.
        mode: TocUpdateName,
        /// The table's field id, as `read_blocks` lists it; omitted, the
        /// document's only table of contents.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        field: Option<String>,
    },
    /// Insert a numbered caption above or below a table, picture or
    /// paragraph in the body; later captions of the same label renumber.
    InsertCaption {
        /// The id of the table, picture or paragraph the caption is for.
        target: String,
        /// The label and numbering sequence: `Figure`, `Table`, `Equation` or
        /// any label the document already uses.
        label: String,
        /// The caption's own text after the number.
        #[serde(default, skip_serializing_if = "String::is_empty")]
        text: String,
        /// Above or below the target; below when omitted.
        #[serde(default)]
        position: CaptionPlacement,
        /// Leave the label out and show only the number.
        #[serde(default)]
        exclude_label: bool,
        /// How the number is written; arabic when omitted.
        #[serde(default)]
        number_format: CaptionNumbering,
        /// Include the chapter number, counted from headings of this level
        /// (1 to 9).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1, max = 9))]
        chapter_level: Option<u8>,
        /// The separator after the chapter number; a hyphen when omitted.
        #[serde(default)]
        separator: ChapterSeparator,
    },
    /// Renumber every caption whose number is stale (Word's update fields
    /// over the captions).
    UpdateCaptionNumbers {},
    /// Insert a cross-reference at an anchor: a field showing a heading's,
    /// caption's or bookmarked paragraph's text, number or page.
    InsertCrossReference {
        /// Where the reference goes.
        at: Anchor,
        /// The paragraph referred to: a heading, a caption, a bookmarked
        /// paragraph or a note's first paragraph.
        target: String,
        /// What the reference shows.
        show: ReferenceKind,
        /// Make the reference a link to its target.
        #[serde(default)]
        hyperlink: bool,
        /// Follow it with `above` or `below`, as Word's checkbox does.
        #[serde(default)]
        include_above_below: bool,
    },
    /// Comment on the quoted text, or reply to a comment.
    Comment {
        /// The text commented on.
        target: Anchor,
        /// The comment.
        text: String,
        /// A comment id to reply to.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to: Option<String>,
    },
    /// Resolve or reopen a comment thread.
    ResolveComment {
        /// The comment id.
        id: String,
        /// `true` (the default) resolves, `false` reopens.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolved: Option<bool>,
    },
    /// Accept or reject tracked changes.
    Review {
        /// Accept or reject.
        decision: Decision,
        /// Revision ids.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        revisions: Vec<String>,
        /// Every revision in the document.
        #[serde(default)]
        all: bool,
    },
}

impl Edit {
    /// Its `op`, the registry id. Exhaustive. O(1).
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::InsertText { .. } => "insert_text",
            Self::ReplaceText { .. } => "replace_text",
            Self::DeleteText { .. } => "delete_text",
            Self::InsertParagraphs { .. } => "insert_paragraphs",
            Self::ApplyParagraphStyle { .. } => "apply_paragraph_style",
            Self::SetTextFormat { .. } => "set_text_format",
            Self::ClearFormatting { .. } => "clear_formatting",
            Self::RewriteBlock { .. } => "rewrite_block",
            Self::SetAlignment { .. } => "set_alignment",
            Self::SetLineSpacing { .. } => "set_line_spacing",
            Self::SetParagraphSpacing { .. } => "set_paragraph_spacing",
            Self::SetIndent { .. } => "set_indent",
            Self::SetCellText { .. } => "set_cell_text",
            Self::TableInsertRow { .. } => "table_insert_row",
            Self::TableDeleteRow { .. } => "table_delete_row",
            Self::InsertTable { .. } => "insert_table",
            Self::TableInsertColumn { .. } => "table_insert_column",
            Self::TableDeleteColumn { .. } => "table_delete_column",
            Self::TableMergeCells { .. } => "table_merge_cells",
            Self::TableSplitCell { .. } => "table_split_cell",
            Self::TableDelete { .. } => "table_delete",
            Self::TableSetCellFormat { .. } => "table_set_cell_format",
            Self::SetList { .. } => "set_list",
            Self::ChangeListLevel { .. } => "change_list_level",
            Self::RestartNumbering { .. } => "restart_numbering",
            Self::ContinueNumbering { .. } => "continue_numbering",
            Self::SetHyperlink { .. } => "set_hyperlink",
            Self::RemoveHyperlink { .. } => "remove_hyperlink",
            Self::CreateBookmark { .. } => "create_bookmark",
            Self::RenameBookmark { .. } => "rename_bookmark",
            Self::DeleteBookmark { .. } => "delete_bookmark",
            Self::InsertField { .. } => "insert_field",
            Self::InsertImage { .. } => "insert_image",
            Self::SetAltText { .. } => "set_alt_text",
            Self::ResizeObject { .. } => "resize_object",
            Self::SetWrap { .. } => "set_wrap",
            Self::CropImage { .. } => "crop_image",
            Self::DeleteObject { .. } => "delete_object",
            Self::SetShapeFill { .. } => "set_shape_fill",
            Self::SetShapeOutline { .. } => "set_shape_outline",
            Self::InsertFootnote { .. } => "insert_footnote",
            Self::InsertEndnote { .. } => "insert_endnote",
            Self::InsertBreak { .. } => "insert_break",
            Self::InsertSectionBreak { .. } => "insert_section_break",
            Self::SetPageSetup { .. } => "set_page_setup",
            Self::SetLineNumbering { .. } => "set_line_numbering",
            Self::SetPageNumbering { .. } => "set_page_numbering",
            Self::SetWatermark { .. } => "set_watermark",
            Self::InsertHeader { .. } => "insert_header",
            Self::InsertFooter { .. } => "insert_footer",
            Self::SetDifferentFirstPage { .. } => "set_different_first_page",
            Self::SetDifferentOddEven { .. } => "set_different_odd_even",
            Self::LinkToPrevious { .. } => "link_to_previous",
            Self::RemoveHeaderFooter { .. } => "remove_header_footer",
            Self::InsertPageNumber { .. } => "insert_page_number",
            Self::InsertChart { .. } => "insert_chart",
            Self::InsertTableOfContents { .. } => "insert_table_of_contents",
            Self::UpdateTableOfContents { .. } => "update_table_of_contents",
            Self::InsertCaption { .. } => "insert_caption",
            Self::UpdateCaptionNumbers { .. } => "update_caption_numbers",
            Self::InsertCrossReference { .. } => "insert_cross_reference",
            Self::Comment { .. } => "comment",
            Self::ResolveComment { .. } => "resolve_comment",
            Self::Review { .. } => "review",
        }
    }

    /// Bytes of text this edit inserts, for the plan's ceiling. O(text).
    #[must_use]
    pub fn inserted_bytes(&self) -> usize {
        match self {
            Self::InsertText { text, .. }
            | Self::ReplaceText { text, .. }
            | Self::SetCellText { text, .. }
            | Self::InsertFootnote { text, .. }
            | Self::InsertEndnote { text, .. }
            | Self::Comment { text, .. } => text.len(),
            Self::SetHyperlink { url, tooltip, .. } => {
                url.len() + tooltip.as_ref().map_or(0, String::len)
            }
            Self::CreateBookmark { name, .. } => name.len(),
            Self::RenameBookmark { new_name, .. } => new_name.len(),
            Self::InsertField { result, .. } => result.as_ref().map_or(0, String::len),
            Self::InsertCaption { label, text, .. } => label.len() + text.len(),
            Self::SetAltText { text, .. } => text.as_ref().map_or(0, String::len),
            Self::SetWatermark { text, .. } => text.as_ref().map_or(0, String::len),
            // A picture's bytes are bounded per edit (`objects::MAX_IMAGE_BYTES`),
            // not by the plan's text ceiling; its alt text is text.
            Self::InsertImage { alt_text, .. } => alt_text.as_ref().map_or(0, String::len),
            Self::RewriteBlock { markdown, .. } => markdown.len(),
            Self::InsertParagraphs { paragraphs, .. } => paragraphs
                .iter()
                .map(|paragraph| paragraph.text.len())
                .sum(),
            Self::DeleteText { .. }
            | Self::ApplyParagraphStyle { .. }
            | Self::SetTextFormat { .. }
            | Self::ClearFormatting { .. }
            | Self::SetAlignment { .. }
            | Self::SetLineSpacing { .. }
            | Self::SetParagraphSpacing { .. }
            | Self::SetIndent { .. }
            | Self::TableInsertRow { .. }
            | Self::TableDeleteRow { .. }
            | Self::InsertTable { .. }
            | Self::TableInsertColumn { .. }
            | Self::TableDeleteColumn { .. }
            | Self::TableMergeCells { .. }
            | Self::TableSplitCell { .. }
            | Self::TableDelete { .. }
            | Self::TableSetCellFormat { .. }
            | Self::SetList { .. }
            | Self::ChangeListLevel { .. }
            | Self::RestartNumbering { .. }
            | Self::ContinueNumbering { .. }
            | Self::RemoveHyperlink { .. }
            | Self::DeleteBookmark { .. }
            | Self::ResizeObject { .. }
            | Self::SetWrap { .. }
            | Self::CropImage { .. }
            | Self::DeleteObject { .. }
            | Self::SetShapeFill { .. }
            | Self::SetShapeOutline { .. }
            | Self::InsertBreak { .. }
            | Self::InsertSectionBreak { .. }
            | Self::SetPageSetup { .. }
            | Self::SetLineNumbering { .. }
            | Self::SetPageNumbering { .. }
            | Self::InsertHeader { .. }
            | Self::InsertFooter { .. }
            | Self::SetDifferentFirstPage { .. }
            | Self::SetDifferentOddEven { .. }
            | Self::LinkToPrevious { .. }
            | Self::RemoveHeaderFooter { .. }
            | Self::InsertPageNumber { .. }
            | Self::InsertChart { .. }
            | Self::InsertTableOfContents { .. }
            | Self::UpdateTableOfContents { .. }
            | Self::UpdateCaptionNumbers { .. }
            | Self::InsertCrossReference { .. }
            | Self::ResolveComment { .. }
            | Self::Review { .. } => 0,
        }
    }
}
