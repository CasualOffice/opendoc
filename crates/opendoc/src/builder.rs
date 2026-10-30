// SPDX-License-Identifier: Apache-2.0

//! The declarative builder: a [`DocumentSpec`] in, a WordprocessingML package
//! out, which then opens through the engine like any other DOCX.
//!
//! The spec's shape is docx-js's (`docs/176` §12.2): `sections[].children` of
//! paragraphs and tables, `TextRun` options named as docx-js names them
//! (`bold`, `italics`, `size` in half-points, `color` as hex), headers and
//! footers per section, `numbering.config`, `styles.paragraphStyles`,
//! `comments.children` and `footnotes`. A docx-js user's object literal is,
//! field for field, a valid spec. The JavaScript package's classes
//! (`new Paragraph(...)`) serialise to exactly this JSON.
//!
//! **Why a package, not model structs.** Creation is an *open* of a package
//! the SDK authored, so the result is a real DOCX that Word, LibreOffice and
//! every other reader accept, and the engine's model is the one its importer
//! builds, which is the model every other path is tested on. Edits to the
//! created document then go through the operation set like any other edit.
//!
//! **Complexity.** O(spec) to write, plus one import.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::{Cursor, Write as _};

use serde::Deserialize;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

use crate::error::{Error, ErrorCode, Result};

// --- the spec ---------------------------------------------------------------

/// A whole document, docx-js-shaped.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentSpec {
    /// `dc:creator`.
    #[serde(default)]
    pub creator: Option<String>,
    /// `dc:title`.
    #[serde(default)]
    pub title: Option<String>,
    /// `dc:subject`.
    #[serde(default)]
    pub subject: Option<String>,
    /// `dc:description`.
    #[serde(default)]
    pub description: Option<String>,
    /// `cp:keywords`.
    #[serde(default)]
    pub keywords: Option<String>,
    /// `cp:lastModifiedBy`.
    #[serde(default)]
    pub last_modified_by: Option<String>,
    /// Styles to define or override.
    #[serde(default)]
    pub styles: Option<StylesSpec>,
    /// Numbering definitions (`numbering.config`).
    #[serde(default)]
    pub numbering: Option<NumberingSpec>,
    /// Comment bodies, referenced by id from the text.
    #[serde(default)]
    pub comments: Option<CommentsSpec>,
    /// Footnote bodies by id (`"1"`, `"2"`...).
    #[serde(default)]
    pub footnotes: BTreeMap<String, StorySpec>,
    /// Endnote bodies by id.
    #[serde(default)]
    pub endnotes: BTreeMap<String, StorySpec>,
    /// Document-level switches.
    #[serde(default)]
    pub features: Option<FeaturesSpec>,
    /// The sections, in order. An empty list writes one empty section.
    #[serde(default)]
    pub sections: Vec<SectionSpec>,
}

/// Document-level switches.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeaturesSpec {
    /// Turn on Track Changes (`w:trackRevisions`).
    #[serde(default)]
    pub track_revisions: bool,
    /// Ask Word to update fields (a table of contents) when it opens the file.
    #[serde(default)]
    pub update_fields: bool,
}

/// One section.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionSpec {
    /// Page setup.
    #[serde(default)]
    pub properties: Option<SectionPropertiesSpec>,
    /// Headers: `default`, `first`, `even`.
    #[serde(default)]
    pub headers: Option<HeaderFooterGroupSpec>,
    /// Footers: `default`, `first`, `even`.
    #[serde(default)]
    pub footers: Option<HeaderFooterGroupSpec>,
    /// The section's blocks.
    #[serde(default)]
    pub children: Vec<BlockSpec>,
}

/// Page setup for a section.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SectionPropertiesSpec {
    /// Page size, margins and numbering.
    #[serde(default)]
    pub page: Option<PageSpec>,
    /// A distinct first-page header and footer.
    #[serde(default)]
    pub title_page: bool,
    /// `"nextPage"` (default), `"continuous"`, `"evenPage"`, `"oddPage"`, `"nextColumn"`.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// Text columns.
    #[serde(default)]
    pub column: Option<ColumnSpec>,
}

/// Text columns.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ColumnSpec {
    /// Column count.
    #[serde(default)]
    pub count: Option<u16>,
    /// Space between columns, twips.
    #[serde(default)]
    pub space: Option<i32>,
    /// A line between columns.
    #[serde(default)]
    pub separate: bool,
}

/// Page size, margins and numbering, in twips (docx-js's unit).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSpec {
    /// Size.
    #[serde(default)]
    pub size: Option<PageSizeSpec>,
    /// Margins.
    #[serde(default)]
    pub margin: Option<MarginSpec>,
    /// Page numbering.
    #[serde(default)]
    pub page_numbers: Option<PageNumbersSpec>,
}

/// Page size.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageSizeSpec {
    /// Width, twips. Default A4 (11906) or Letter by `orientation`'s sibling.
    #[serde(default)]
    pub width: Option<i32>,
    /// Height, twips.
    #[serde(default)]
    pub height: Option<i32>,
    /// `"portrait"` or `"landscape"` (swaps the sides, as docx-js does).
    #[serde(default)]
    pub orientation: Option<String>,
}

/// Margins, twips.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarginSpec {
    /// Top.
    #[serde(default)]
    pub top: Option<i32>,
    /// Right.
    #[serde(default)]
    pub right: Option<i32>,
    /// Bottom.
    #[serde(default)]
    pub bottom: Option<i32>,
    /// Left.
    #[serde(default)]
    pub left: Option<i32>,
    /// Header distance.
    #[serde(default)]
    pub header: Option<i32>,
    /// Footer distance.
    #[serde(default)]
    pub footer: Option<i32>,
    /// Gutter.
    #[serde(default)]
    pub gutter: Option<i32>,
}

/// Page numbering.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageNumbersSpec {
    /// The first page's number.
    #[serde(default)]
    pub start: Option<u32>,
    /// `"decimal"`, `"upperRoman"`, `"lowerRoman"`, `"upperLetter"`, `"lowerLetter"`.
    #[serde(default)]
    pub format_type: Option<String>,
}

/// The headers or footers of a section.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HeaderFooterGroupSpec {
    /// Every page (or odd pages when `even` is set).
    #[serde(default)]
    pub default: Option<StorySpec>,
    /// The first page, with `titlePage`.
    #[serde(default)]
    pub first: Option<StorySpec>,
    /// Even pages.
    #[serde(default)]
    pub even: Option<StorySpec>,
}

/// A story: a header, a footer, a note or a comment body.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorySpec {
    /// Its blocks.
    #[serde(default)]
    pub children: Vec<BlockSpec>,
}

/// A block.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BlockSpec {
    /// A paragraph.
    Paragraph(ParagraphSpec),
    /// A table.
    Table(TableSpec),
    /// A table of contents over the document's headings.
    TableOfContents(TocSpec),
}

/// Paragraph formatting, shared by paragraphs and paragraph styles.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphFormatSpec {
    /// `"left"`, `"start"`, `"center"`, `"right"`, `"end"`, `"justified"`/`"both"`.
    #[serde(default)]
    pub alignment: Option<String>,
    /// Spacing, twips.
    #[serde(default)]
    pub spacing: Option<SpacingSpec>,
    /// Indentation, twips.
    #[serde(default)]
    pub indent: Option<IndentSpec>,
    /// Keep with the next paragraph.
    #[serde(default)]
    pub keep_next: bool,
    /// Keep the lines together.
    #[serde(default)]
    pub keep_lines: bool,
    /// A page break before it.
    #[serde(default)]
    pub page_break_before: bool,
    /// Outline level, 0-based.
    #[serde(default)]
    pub outline_level: Option<u8>,
    /// Shading.
    #[serde(default)]
    pub shading: Option<ShadingSpec>,
    /// A bottom border line (docx-js `thematicBreak`).
    #[serde(default)]
    pub thematic_break: bool,
    /// Suppress spacing between paragraphs of the same style.
    #[serde(default)]
    pub contextual_spacing: bool,
}

/// A paragraph.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParagraphSpec {
    /// Shorthand for one plain run.
    #[serde(default)]
    pub text: Option<String>,
    /// The inline content.
    #[serde(default)]
    pub children: Vec<InlineSpec>,
    /// `"Heading1"`..`"Heading6"`, `"Title"` (docx-js `HeadingLevel`).
    #[serde(default)]
    pub heading: Option<String>,
    /// A paragraph style id or name.
    #[serde(default)]
    pub style: Option<String>,
    /// A numbering reference.
    #[serde(default)]
    pub numbering: Option<NumberingRefSpec>,
    /// A bullet at a level (the built-in bullet list).
    #[serde(default)]
    pub bullet: Option<BulletSpec>,
    /// A bookmark around the whole paragraph, by name.
    #[serde(default)]
    pub bookmark: Option<String>,
    /// Formatting.
    #[serde(flatten)]
    pub format: ParagraphFormatSpec,
}

/// A bullet.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BulletSpec {
    /// The level, 0-based.
    #[serde(default)]
    pub level: u8,
}

/// A reference to a `numbering.config` entry.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingRefSpec {
    /// The config's `reference`.
    pub reference: String,
    /// The level, 0-based.
    #[serde(default)]
    pub level: u8,
    /// A separate instance that restarts the count.
    #[serde(default)]
    pub instance: Option<u32>,
}

/// Paragraph spacing, twips (`line` in 240ths of a line for `auto`).
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpacingSpec {
    /// Before.
    #[serde(default)]
    pub before: Option<i32>,
    /// After.
    #[serde(default)]
    pub after: Option<i32>,
    /// Line.
    #[serde(default)]
    pub line: Option<i32>,
    /// `"auto"`, `"exact"`, `"atLeast"`.
    #[serde(default)]
    pub line_rule: Option<String>,
}

/// Indentation, twips.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IndentSpec {
    /// Left (start).
    #[serde(default, alias = "start")]
    pub left: Option<i32>,
    /// Right (end).
    #[serde(default, alias = "end")]
    pub right: Option<i32>,
    /// First-line indent.
    #[serde(default)]
    pub first_line: Option<i32>,
    /// Hanging indent.
    #[serde(default)]
    pub hanging: Option<i32>,
}

/// Shading.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShadingSpec {
    /// Fill colour, hex.
    #[serde(default)]
    pub fill: Option<String>,
    /// Pattern colour, hex.
    #[serde(default)]
    pub color: Option<String>,
    /// Pattern (`"clear"`, `"solid"`, ...).
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

/// Inline content.
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InlineSpec {
    /// A run of text.
    Text(TextRunSpec),
    /// A picture.
    Image(ImageSpec),
    /// A link to a URL.
    Hyperlink(HyperlinkSpec),
    /// A link to a bookmark.
    InternalHyperlink(InternalHyperlinkSpec),
    /// A bookmark around inline content.
    Bookmark(BookmarkSpec),
    /// A page break.
    PageBreak,
    /// A column break.
    ColumnBreak,
    /// A line break.
    LineBreak,
    /// A tab.
    Tab,
    /// A footnote reference by id.
    FootnoteReference(NoteRefSpec),
    /// An endnote reference by id.
    EndnoteReference(NoteRefSpec),
    /// The start of a comment's range.
    CommentRangeStart(CommentRefSpec),
    /// The end of a comment's range.
    CommentRangeEnd(CommentRefSpec),
    /// The comment's reference mark.
    CommentReference(CommentRefSpec),
    /// A field (`PAGE`, `NUMPAGES`, `DATE \@ "yyyy-MM-dd"`, ...).
    Field(FieldSpec),
    /// Insertion tracked as a change (docx-js `InsertedTextRun`).
    InsertedText(TrackedRunSpec),
    /// Deletion tracked as a change (docx-js `DeletedTextRun`).
    DeletedText(TrackedRunSpec),
}

/// Run formatting, shared by runs and styles.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunFormatSpec {
    /// Bold.
    #[serde(default)]
    pub bold: Option<bool>,
    /// Italic (docx-js spells it `italics`).
    #[serde(default, alias = "italic")]
    pub italics: Option<bool>,
    /// Underline: `true`, or `{ type, color }`.
    #[serde(default)]
    pub underline: Option<UnderlineSpec>,
    /// Strike-through.
    #[serde(default)]
    pub strike: Option<bool>,
    /// Double strike-through.
    #[serde(default)]
    pub double_strike: Option<bool>,
    /// Colour, hex (`"FF0000"`).
    #[serde(default)]
    pub color: Option<String>,
    /// Size in half-points (docx-js: `size: 28` is 14 pt).
    #[serde(default)]
    pub size: Option<u32>,
    /// Font family.
    #[serde(default)]
    pub font: Option<String>,
    /// Highlight colour name (`"yellow"`).
    #[serde(default)]
    pub highlight: Option<String>,
    /// Superscript.
    #[serde(default)]
    pub super_script: Option<bool>,
    /// Subscript.
    #[serde(default)]
    pub sub_script: Option<bool>,
    /// All capitals.
    #[serde(default)]
    pub all_caps: Option<bool>,
    /// Small capitals.
    #[serde(default)]
    pub small_caps: Option<bool>,
    /// Character spacing, twips.
    #[serde(default)]
    pub character_spacing: Option<i32>,
    /// Shading.
    #[serde(default)]
    pub shading: Option<ShadingSpec>,
}

/// An underline: `true`, or a typed one.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum UnderlineSpec {
    /// On (single) or off.
    On(bool),
    /// A typed underline.
    Typed {
        /// `"single"`, `"double"`, `"dotted"`, `"dash"`, `"wave"`, ...
        #[serde(default, rename = "type")]
        kind: Option<String>,
        /// Colour, hex.
        #[serde(default)]
        color: Option<String>,
    },
}

/// A text run.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRunSpec {
    /// The text. `\n` is a line break and `\t` a tab.
    #[serde(default)]
    pub text: String,
    /// A character style id or name.
    #[serde(default)]
    pub style: Option<String>,
    /// Line breaks before the text (docx-js `break`).
    #[serde(default, rename = "break")]
    pub breaks: Option<u32>,
    /// Formatting.
    #[serde(flatten)]
    pub format: RunFormatSpec,
}

/// A tracked insertion or deletion.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedRunSpec {
    /// The text.
    #[serde(default)]
    pub text: String,
    /// Change id (optional; assigned when absent).
    #[serde(default)]
    pub id: Option<u32>,
    /// Author.
    #[serde(default)]
    pub author: Option<String>,
    /// Date, ISO 8601.
    #[serde(default)]
    pub date: Option<String>,
    /// Formatting.
    #[serde(flatten)]
    pub format: RunFormatSpec,
}

/// A picture.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImageSpec {
    /// The bytes, base64 (the JSON boundary's form).
    #[serde(default)]
    pub data: Option<String>,
    /// Or an index into the media list passed beside the spec.
    #[serde(default)]
    pub media: Option<usize>,
    /// `"png"`, `"jpg"`, `"gif"`, `"bmp"`; sniffed when absent.
    #[serde(default, alias = "type")]
    pub image_type: Option<String>,
    /// Size in pixels at 96 dpi; read from the picture when absent.
    #[serde(default)]
    pub transformation: Option<TransformationSpec>,
    /// Alternative text.
    #[serde(default)]
    pub alt_text: Option<AltTextSpec>,
}

/// A picture's size, pixels.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TransformationSpec {
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// Alternative text.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AltTextSpec {
    /// Name.
    #[serde(default)]
    pub name: Option<String>,
    /// Title.
    #[serde(default)]
    pub title: Option<String>,
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
}

/// A link to a URL.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HyperlinkSpec {
    /// The URL.
    pub link: String,
    /// The linked content.
    #[serde(default)]
    pub children: Vec<InlineSpec>,
}

/// A link to a bookmark.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InternalHyperlinkSpec {
    /// The bookmark name.
    pub anchor: String,
    /// The linked content.
    #[serde(default)]
    pub children: Vec<InlineSpec>,
}

/// A bookmark.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BookmarkSpec {
    /// Its name.
    pub id: String,
    /// The bookmarked content.
    #[serde(default)]
    pub children: Vec<InlineSpec>,
}

/// A note reference.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteRefSpec {
    /// The note id (a key of `footnotes` / `endnotes`).
    pub id: String,
}

/// A comment marker.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentRefSpec {
    /// The comment id.
    pub id: u32,
}

/// A field.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FieldSpec {
    /// The instruction.
    pub instruction: String,
    /// The cached result shown until the field is updated.
    #[serde(default)]
    pub text: Option<String>,
}

/// A table.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableSpec {
    /// The rows.
    pub rows: Vec<RowSpec>,
    /// Total width.
    #[serde(default)]
    pub width: Option<WidthSpec>,
    /// Grid column widths, twips.
    #[serde(default)]
    pub column_widths: Vec<i32>,
    /// A table style id or name (default `TableGrid`).
    #[serde(default)]
    pub style: Option<String>,
    /// `"left"`, `"center"`, `"right"`.
    #[serde(default)]
    pub alignment: Option<String>,
    /// `"fixed"` or `"autofit"`.
    #[serde(default)]
    pub layout: Option<String>,
}

/// A width.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WidthSpec {
    /// The size, in the unit `type` names (twips for `dxa`, fiftieths of a
    /// percent for `pct`, or a `"50%"` string).
    pub size: serde_json::Value,
    /// `"dxa"`, `"pct"`, `"auto"`.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
}

/// A table row.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowSpec {
    /// The cells.
    pub children: Vec<CellSpec>,
    /// Repeat as a header row.
    #[serde(default)]
    pub table_header: bool,
    /// Do not split across pages.
    #[serde(default)]
    pub cant_split: bool,
    /// Height.
    #[serde(default)]
    pub height: Option<RowHeightSpec>,
}

/// A row height.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RowHeightSpec {
    /// Twips.
    pub value: i32,
    /// `"atLeast"`, `"exact"`, `"auto"`.
    #[serde(default)]
    pub rule: Option<String>,
}

/// A table cell.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellSpec {
    /// Its blocks. Empty writes one empty paragraph (a cell must hold one).
    #[serde(default)]
    pub children: Vec<BlockSpec>,
    /// Grid columns spanned.
    #[serde(default)]
    pub column_span: Option<u32>,
    /// Rows spanned; the cells below are written as merge continuations.
    #[serde(default)]
    pub row_span: Option<u32>,
    /// Width.
    #[serde(default)]
    pub width: Option<WidthSpec>,
    /// Shading.
    #[serde(default)]
    pub shading: Option<ShadingSpec>,
    /// `"top"`, `"center"`, `"bottom"`.
    #[serde(default)]
    pub vertical_align: Option<String>,
}

/// A table of contents.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TocSpec {
    /// A heading paragraph above it.
    #[serde(default)]
    pub title: Option<String>,
    /// Heading levels, `"1-3"`.
    #[serde(default)]
    pub heading_style_range: Option<String>,
    /// Entries link to their headings.
    #[serde(default)]
    pub hyperlink: Option<bool>,
}

/// Styles.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StylesSpec {
    /// Document defaults.
    #[serde(default)]
    pub default: Option<DefaultStylesSpec>,
    /// Paragraph styles to add, or to override by id.
    #[serde(default)]
    pub paragraph_styles: Vec<StyleSpec>,
    /// Character styles.
    #[serde(default)]
    pub character_styles: Vec<StyleSpec>,
}

/// Document defaults.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DefaultStylesSpec {
    /// The document's base formatting.
    #[serde(default)]
    pub document: Option<StyleFormatSpec>,
}

/// Run and paragraph formatting for a style or the defaults.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleFormatSpec {
    /// Run formatting.
    #[serde(default)]
    pub run: Option<RunFormatSpec>,
    /// Paragraph formatting.
    #[serde(default)]
    pub paragraph: Option<ParagraphFormatSpec>,
}

/// A style.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StyleSpec {
    /// Style id.
    pub id: String,
    /// Display name (defaults to the id).
    #[serde(default)]
    pub name: Option<String>,
    /// Parent style id.
    #[serde(default)]
    pub based_on: Option<String>,
    /// Next paragraph's style id.
    #[serde(default)]
    pub next: Option<String>,
    /// Show in the Quick Styles gallery.
    #[serde(default)]
    pub quick_format: bool,
    /// Run formatting.
    #[serde(default)]
    pub run: Option<RunFormatSpec>,
    /// Paragraph formatting.
    #[serde(default)]
    pub paragraph: Option<ParagraphFormatSpec>,
}

/// Numbering definitions.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingSpec {
    /// The definitions.
    #[serde(default)]
    pub config: Vec<NumberingConfigSpec>,
}

/// One numbering definition.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NumberingConfigSpec {
    /// The name paragraphs refer to it by.
    pub reference: String,
    /// Its levels.
    pub levels: Vec<LevelSpec>,
}

/// One numbering level.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LevelSpec {
    /// 0-based level.
    pub level: u8,
    /// `"decimal"`, `"bullet"`, `"upperRoman"`, `"lowerRoman"`, `"upperLetter"`,
    /// `"lowerLetter"`, `"decimalZero"`, `"none"`.
    #[serde(default)]
    pub format: Option<String>,
    /// The level text (`"%1."`).
    #[serde(default)]
    pub text: Option<String>,
    /// `"start"`, `"center"`, `"end"` (`"left"`/`"right"` accepted).
    #[serde(default)]
    pub alignment: Option<String>,
    /// First number.
    #[serde(default)]
    pub start: Option<u32>,
    /// The level's paragraph and run formatting.
    #[serde(default)]
    pub style: Option<StyleFormatSpec>,
}

/// Comment bodies.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentsSpec {
    /// The comments.
    #[serde(default)]
    pub children: Vec<CommentSpec>,
}

/// A comment body.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommentSpec {
    /// The id the markers use.
    pub id: u32,
    /// Author.
    #[serde(default)]
    pub author: Option<String>,
    /// Initials.
    #[serde(default)]
    pub initials: Option<String>,
    /// Date, ISO 8601.
    #[serde(default)]
    pub date: Option<String>,
    /// Its paragraphs.
    #[serde(default)]
    pub children: Vec<BlockSpec>,
}

// --- the writer -------------------------------------------------------------

const W_NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture""#;
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const EMU_PER_PX: f64 = 9_525.0;

/// Escapes text for XML content and attributes.
fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // XML 1.0 forbids most C0 controls; drop them rather than write an
            // unreadable part.
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => out.push(c),
        }
    }
    out
}

/// A hex colour, validated (`#` stripped, `auto` kept).
fn hex(color: &str) -> Result<String> {
    let c = color.trim().trim_start_matches('#');
    if c.eq_ignore_ascii_case("auto") {
        return Ok("auto".to_owned());
    }
    if c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(c.to_ascii_uppercase())
    } else {
        Err(build_error(format!(
            "\"{color}\" is not a hex colour like \"FF0000\""
        )))
    }
}

fn build_error(message: impl Into<String>) -> Error {
    Error::new(ErrorCode::Build, message)
}

/// The parts and relationships of one story (the body or a header...).
#[derive(Default)]
struct Rels {
    entries: Vec<(String, String, String, bool)>, // id, type, target, external
}

impl Rels {
    fn add(&mut self, kind: &str, target: &str, external: bool) -> String {
        if let Some((id, ..)) = self
            .entries
            .iter()
            .find(|(_, k, t, e)| k == kind && t == target && *e == external)
        {
            return id.clone();
        }
        let id = format!("rId{}", self.entries.len() + 1);
        self.entries
            .push((id.clone(), kind.to_owned(), target.to_owned(), external));
        id
    }

    fn xml(&self) -> String {
        let mut out = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
        );
        for (id, kind, target, external) in &self.entries {
            let _ = write!(
                out,
                r#"<Relationship Id="{id}" Type="{REL_NS}/{kind}" Target="{}"{}/>"#,
                esc(target),
                if *external {
                    r#" TargetMode="External""#
                } else {
                    ""
                }
            );
        }
        out.push_str("</Relationships>");
        out
    }
}

/// The package being written.
struct Writer<'a> {
    spec: &'a DocumentSpec,
    media: &'a [Vec<u8>],
    /// Part name → bytes, for media.
    media_parts: Vec<(String, Vec<u8>)>,
    /// Extensions used by media parts.
    media_extensions: Vec<&'static str>,
    /// numbering reference (+ instance) → numId.
    num_ids: BTreeMap<(String, u32), u32>,
    /// reference → abstractNumId.
    abstract_ids: BTreeMap<String, u32>,
    next_drawing: u32,
    next_bookmark: u32,
    next_revision: u32,
    /// Headings seen, for a table of contents: (level, text, bookmark).
    headings: Vec<(u8, String, String)>,
    /// Style ids the spec's styles define, plus the built-ins.
    style_ids: Vec<(String, String)>, // (id, name)
}

/// The built-in styles: (id, name, type, xml body). Word's own ids and names,
/// so a style named in python-docx or docx-js resolves here exactly as it would
/// in Word's default template. Values are this file's own, chosen to match
/// Word 2013+ defaults (Calibri 11 pt, 8 pt after, 1.08 lines), which the
/// bundled metric-compatible Carlito lays out identically.
const BUILTIN_STYLES: &[(&str, &str, &str, &str)] = &[
    ("Normal", "Normal", "paragraph", r#"<w:qFormat/>"#),
    (
        "Title",
        "Title",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/><w:contextualSpacing/></w:pPr><w:rPr><w:rFonts w:ascii="Calibri Light" w:hAnsi="Calibri Light"/><w:kern w:val="28"/><w:sz w:val="56"/><w:szCs w:val="56"/></w:rPr>"#,
    ),
    (
        "Subtitle",
        "Subtitle",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:numPr><w:ilvl w:val="1"/></w:numPr></w:pPr><w:rPr><w:color w:val="5A5A5A"/><w:spacing w:val="15"/><w:sz w:val="22"/></w:rPr>"#,
    ),
    (
        "Heading1",
        "heading 1",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="240" w:after="0"/><w:outlineLvl w:val="0"/></w:pPr><w:rPr><w:rFonts w:ascii="Calibri Light" w:hAnsi="Calibri Light"/><w:color w:val="2F5496"/><w:sz w:val="32"/><w:szCs w:val="32"/></w:rPr>"#,
    ),
    (
        "Heading2",
        "heading 2",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="40" w:after="0"/><w:outlineLvl w:val="1"/></w:pPr><w:rPr><w:rFonts w:ascii="Calibri Light" w:hAnsi="Calibri Light"/><w:color w:val="2F5496"/><w:sz w:val="26"/><w:szCs w:val="26"/></w:rPr>"#,
    ),
    (
        "Heading3",
        "heading 3",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="40" w:after="0"/><w:outlineLvl w:val="2"/></w:pPr><w:rPr><w:rFonts w:ascii="Calibri Light" w:hAnsi="Calibri Light"/><w:color w:val="1F3763"/><w:sz w:val="24"/><w:szCs w:val="24"/></w:rPr>"#,
    ),
    (
        "Heading4",
        "heading 4",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="40" w:after="0"/><w:outlineLvl w:val="3"/></w:pPr><w:rPr><w:i/><w:color w:val="2F5496"/></w:rPr>"#,
    ),
    (
        "Heading5",
        "heading 5",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="40" w:after="0"/><w:outlineLvl w:val="4"/></w:pPr><w:rPr><w:color w:val="2F5496"/></w:rPr>"#,
    ),
    (
        "Heading6",
        "heading 6",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:keepNext/><w:keepLines/><w:spacing w:before="40" w:after="0"/><w:outlineLvl w:val="5"/></w:pPr><w:rPr><w:color w:val="1F3763"/></w:rPr>"#,
    ),
    (
        "Quote",
        "Quote",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:before="200" w:after="160"/><w:ind w:left="864" w:right="864"/><w:jc w:val="center"/></w:pPr><w:rPr><w:i/><w:color w:val="404040"/></w:rPr>"#,
    ),
    (
        "ListParagraph",
        "List Paragraph",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:qFormat/><w:pPr><w:ind w:left="720"/><w:contextualSpacing/></w:pPr>"#,
    ),
    (
        "ListBullet",
        "List Bullet",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:pPr><w:numPr><w:numId w:val="1"/></w:numPr><w:contextualSpacing/></w:pPr>"#,
    ),
    (
        "ListNumber",
        "List Number",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:pPr><w:numPr><w:numId w:val="2"/></w:numPr><w:contextualSpacing/></w:pPr>"#,
    ),
    (
        "Caption",
        "caption",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:spacing w:after="200" w:line="240" w:lineRule="auto"/></w:pPr><w:rPr><w:i/><w:color w:val="44546A"/><w:sz w:val="18"/></w:rPr>"#,
    ),
    (
        "TOC1",
        "toc 1",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:spacing w:after="100"/></w:pPr>"#,
    ),
    (
        "TOC2",
        "toc 2",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:spacing w:after="100"/><w:ind w:left="220"/></w:pPr>"#,
    ),
    (
        "TOC3",
        "toc 3",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:pPr><w:spacing w:after="100"/><w:ind w:left="440"/></w:pPr>"#,
    ),
    (
        "Header",
        "header",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:pPr><w:tabs><w:tab w:val="center" w:pos="4680"/><w:tab w:val="right" w:pos="9360"/></w:tabs><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>"#,
    ),
    (
        "Footer",
        "footer",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:pPr><w:tabs><w:tab w:val="center" w:pos="4680"/><w:tab w:val="right" w:pos="9360"/></w:tabs><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>"#,
    ),
    (
        "FootnoteText",
        "footnote text",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:rPr><w:sz w:val="20"/></w:rPr>"#,
    ),
    (
        "CommentText",
        "annotation text",
        "paragraph",
        r#"<w:basedOn w:val="Normal"/><w:pPr><w:spacing w:line="240" w:lineRule="auto"/></w:pPr><w:rPr><w:sz w:val="20"/></w:rPr>"#,
    ),
    (
        "FootnoteReference",
        "footnote reference",
        "character",
        r#"<w:rPr><w:vertAlign w:val="superscript"/></w:rPr>"#,
    ),
    (
        "EndnoteReference",
        "endnote reference",
        "character",
        r#"<w:rPr><w:vertAlign w:val="superscript"/></w:rPr>"#,
    ),
    (
        "Hyperlink",
        "Hyperlink",
        "character",
        r#"<w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr>"#,
    ),
    (
        "Strong",
        "Strong",
        "character",
        r#"<w:qFormat/><w:rPr><w:b/><w:bCs/></w:rPr>"#,
    ),
    (
        "Emphasis",
        "Emphasis",
        "character",
        r#"<w:qFormat/><w:rPr><w:i/><w:iCs/></w:rPr>"#,
    ),
    (
        "TableGrid",
        "Table Grid",
        "table",
        r#"<w:basedOn w:val="TableNormal"/><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:left w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:right w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="auto"/><w:insideV w:val="single" w:sz="4" w:space="0" w:color="auto"/></w:tblBorders></w:tblPr>"#,
    ),
];

impl<'a> Writer<'a> {
    fn new(spec: &'a DocumentSpec, media: &'a [Vec<u8>]) -> Self {
        let mut style_ids: Vec<(String, String)> = BUILTIN_STYLES
            .iter()
            .map(|(id, name, ..)| ((*id).to_owned(), (*name).to_owned()))
            .collect();
        style_ids.push(("TableNormal".to_owned(), "Normal Table".to_owned()));
        if let Some(styles) = &spec.styles {
            for style in styles
                .paragraph_styles
                .iter()
                .chain(&styles.character_styles)
            {
                if !style_ids.iter().any(|(id, _)| *id == style.id) {
                    style_ids.push((
                        style.id.clone(),
                        style.name.clone().unwrap_or_else(|| style.id.clone()),
                    ));
                }
            }
        }
        Self {
            spec,
            media,
            media_parts: Vec::new(),
            media_extensions: Vec::new(),
            num_ids: BTreeMap::new(),
            abstract_ids: BTreeMap::new(),
            next_drawing: 1,
            next_bookmark: 1,
            next_revision: 1,
            headings: Vec::new(),
            style_ids,
        }
    }

    /// A style reference by id or display name, case-insensitively, the way
    /// python-docx and docx-js users write it (`"Heading 1"`, `"Heading1"`).
    fn style_id(&self, name: &str) -> Result<String> {
        let wanted = name.trim();
        let squash = |s: &str| s.replace(' ', "").to_ascii_lowercase();
        self.style_ids
            .iter()
            .find(|(id, _)| id == wanted)
            .or_else(|| {
                self.style_ids
                    .iter()
                    .find(|(id, n)| squash(id) == squash(wanted) || squash(n) == squash(wanted))
            })
            .map(|(id, _)| id.clone())
            .ok_or_else(|| build_error(format!("no style named \"{name}\"")))
    }

    // -- run and paragraph properties --

    fn rpr(format: &RunFormatSpec, style: Option<&str>) -> Result<String> {
        let mut out = String::new();
        if let Some(style) = style {
            let _ = write!(out, r#"<w:rStyle w:val="{}"/>"#, esc(style));
        }
        if let Some(font) = &format.font {
            let f = esc(font);
            let _ = write!(
                out,
                r#"<w:rFonts w:ascii="{f}" w:hAnsi="{f}" w:eastAsia="{f}" w:cs="{f}"/>"#
            );
        }
        let flag = |out: &mut String, tag: &str, value: Option<bool>| match value {
            Some(true) => {
                let _ = write!(out, "<w:{tag}/>");
            }
            Some(false) => {
                let _ = write!(out, r#"<w:{tag} w:val="0"/>"#);
            }
            None => {}
        };
        flag(&mut out, "b", format.bold);
        flag(&mut out, "i", format.italics);
        flag(&mut out, "caps", format.all_caps);
        flag(&mut out, "smallCaps", format.small_caps);
        flag(&mut out, "strike", format.strike);
        flag(&mut out, "dstrike", format.double_strike);
        if let Some(color) = &format.color {
            let _ = write!(out, r#"<w:color w:val="{}"/>"#, hex(color)?);
        }
        if let Some(spacing) = format.character_spacing {
            let _ = write!(out, r#"<w:spacing w:val="{spacing}"/>"#);
        }
        if let Some(size) = format.size {
            if size == 0 || size > 3_276 {
                return Err(build_error(format!(
                    "run size {size} is out of range (half-points, 1-3276)"
                )));
            }
            let _ = write!(out, r#"<w:sz w:val="{size}"/><w:szCs w:val="{size}"/>"#);
        }
        if let Some(highlight) = &format.highlight {
            let _ = write!(out, r#"<w:highlight w:val="{}"/>"#, esc(highlight));
        }
        match &format.underline {
            Some(UnderlineSpec::On(true)) => out.push_str(r#"<w:u w:val="single"/>"#),
            Some(UnderlineSpec::On(false)) => out.push_str(r#"<w:u w:val="none"/>"#),
            Some(UnderlineSpec::Typed { kind, color }) => {
                let _ = write!(
                    out,
                    r#"<w:u w:val="{}"{}/>"#,
                    esc(kind.as_deref().unwrap_or("single")),
                    match color {
                        Some(c) => format!(r#" w:color="{}""#, hex(c)?),
                        None => String::new(),
                    }
                );
            }
            None => {}
        }
        if let Some(shading) = &format.shading {
            out.push_str(&Self::shading(shading)?);
        }
        if format.super_script == Some(true) {
            out.push_str(r#"<w:vertAlign w:val="superscript"/>"#);
        } else if format.sub_script == Some(true) {
            out.push_str(r#"<w:vertAlign w:val="subscript"/>"#);
        }
        Ok(if out.is_empty() {
            out
        } else {
            format!("<w:rPr>{out}</w:rPr>")
        })
    }

    fn shading(shading: &ShadingSpec) -> Result<String> {
        Ok(format!(
            r#"<w:shd w:val="{}" w:color="{}" w:fill="{}"/>"#,
            esc(shading.kind.as_deref().unwrap_or("clear")),
            shading
                .color
                .as_deref()
                .map_or(Ok("auto".to_owned()), hex)?,
            shading.fill.as_deref().map_or(Ok("auto".to_owned()), hex)?,
        ))
    }

    fn alignment(value: &str) -> Result<&'static str> {
        Ok(match value.to_ascii_lowercase().as_str() {
            "left" | "start" => "left",
            "right" | "end" => "right",
            "center" => "center",
            "justified" | "both" | "justify" => "both",
            "distribute" => "distribute",
            other => return Err(build_error(format!("\"{other}\" is not an alignment"))),
        })
    }

    /// Paragraph properties in schema order. `extra_head` goes first (style,
    /// keepNext...), `num` is the numbering element.
    fn ppr(
        format: &ParagraphFormatSpec,
        style: Option<&str>,
        num: Option<(u32, u8)>,
    ) -> Result<String> {
        let mut out = String::new();
        if let Some(style) = style {
            let _ = write!(out, r#"<w:pStyle w:val="{}"/>"#, esc(style));
        }
        if format.keep_next {
            out.push_str("<w:keepNext/>");
        }
        if format.keep_lines {
            out.push_str("<w:keepLines/>");
        }
        if format.page_break_before {
            out.push_str("<w:pageBreakBefore/>");
        }
        if let Some((num_id, level)) = num {
            let _ = write!(
                out,
                r#"<w:numPr><w:ilvl w:val="{level}"/><w:numId w:val="{num_id}"/></w:numPr>"#
            );
        }
        if format.thematic_break {
            out.push_str(r#"<w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="auto"/></w:pBdr>"#);
        }
        if let Some(shading) = &format.shading {
            out.push_str(&Self::shading(shading)?);
        }
        if let Some(spacing) = &format.spacing {
            out.push_str("<w:spacing");
            if let Some(v) = spacing.before {
                let _ = write!(out, r#" w:before="{v}""#);
            }
            if let Some(v) = spacing.after {
                let _ = write!(out, r#" w:after="{v}""#);
            }
            if let Some(v) = spacing.line {
                let _ = write!(
                    out,
                    r#" w:line="{v}" w:lineRule="{}""#,
                    esc(spacing.line_rule.as_deref().unwrap_or("auto"))
                );
            }
            out.push_str("/>");
        }
        if let Some(indent) = &format.indent {
            out.push_str("<w:ind");
            if let Some(v) = indent.left {
                let _ = write!(out, r#" w:left="{v}""#);
            }
            if let Some(v) = indent.right {
                let _ = write!(out, r#" w:right="{v}""#);
            }
            if let Some(v) = indent.hanging {
                let _ = write!(out, r#" w:hanging="{v}""#);
            } else if let Some(v) = indent.first_line {
                let _ = write!(out, r#" w:firstLine="{v}""#);
            }
            out.push_str("/>");
        }
        if format.contextual_spacing {
            out.push_str("<w:contextualSpacing/>");
        }
        if let Some(alignment) = &format.alignment {
            let _ = write!(out, r#"<w:jc w:val="{}"/>"#, Self::alignment(alignment)?);
        }
        if let Some(level) = format.outline_level {
            let _ = write!(out, r#"<w:outlineLvl w:val="{level}"/>"#);
        }
        Ok(out)
    }

    // -- blocks --

    fn blocks(&mut self, blocks: &[BlockSpec], rels: &mut Rels, out: &mut String) -> Result<()> {
        for block in blocks {
            match block {
                BlockSpec::Paragraph(p) => self.paragraph(p, rels, out, None)?,
                BlockSpec::Table(t) => self.table(t, rels, out)?,
                BlockSpec::TableOfContents(toc) => self.toc(toc, out)?,
            }
        }
        Ok(())
    }

    fn numbering_for(&mut self, p: &ParagraphSpec) -> Result<Option<(u32, u8)>> {
        if let Some(bullet) = &p.bullet {
            return Ok(Some((1, bullet.level.min(8))));
        }
        let Some(numbering) = &p.numbering else {
            return Ok(None);
        };
        if !self.abstract_ids.contains_key(&numbering.reference) {
            return Err(build_error(format!(
                "no numbering config with reference \"{}\"",
                numbering.reference
            )));
        }
        let key = (numbering.reference.clone(), numbering.instance.unwrap_or(0));
        let next = 3 + u32::try_from(self.num_ids.len()).unwrap_or(u32::MAX - 3);
        let id = *self.num_ids.entry(key).or_insert(next);
        Ok(Some((id, numbering.level.min(8))))
    }

    /// A paragraph. `sect_pr` is written into the last paragraph of a
    /// non-final section.
    fn paragraph(
        &mut self,
        p: &ParagraphSpec,
        rels: &mut Rels,
        out: &mut String,
        sect_pr: Option<&str>,
    ) -> Result<()> {
        let style = match (&p.heading, &p.style) {
            (Some(heading), _) => Some(self.style_id(heading)?),
            (None, Some(style)) => Some(self.style_id(style)?),
            (None, None) => None,
        };
        let num = self.numbering_for(p)?;
        let style_ref = style.as_deref().or(if p.bullet.is_some() {
            Some("ListParagraph")
        } else {
            None
        });
        let mut ppr = Self::ppr(&p.format, style_ref, num)?;
        if let Some(sect) = sect_pr {
            ppr.push_str(sect);
        }
        out.push_str("<w:p>");
        if !ppr.is_empty() {
            let _ = write!(out, "<w:pPr>{ppr}</w:pPr>");
        }
        // A heading carries a bookmark, so a table of contents can link to it.
        let level = style.as_deref().and_then(crate::view::heading_level_of);
        let bookmark = match (&p.bookmark, level) {
            (Some(name), _) => Some(name.clone()),
            (None, Some(level)) if level >= 1 => Some(format!("_Toc{:08}", self.next_bookmark)),
            _ => None,
        };
        let bookmark_id = bookmark.as_ref().map(|_| {
            let id = self.next_bookmark;
            self.next_bookmark += 1;
            id
        });
        if let (Some(name), Some(id)) = (&bookmark, bookmark_id) {
            let _ = write!(
                out,
                r#"<w:bookmarkStart w:id="{id}" w:name="{}"/>"#,
                esc(name)
            );
        }
        if let Some(text) = &p.text {
            Self::text_runs(text, "", out);
        }
        self.inlines(&p.children, rels, out)?;
        if let Some(id) = bookmark_id {
            let _ = write!(out, r#"<w:bookmarkEnd w:id="{id}"/>"#);
        }
        out.push_str("</w:p>");
        if let (Some(level), Some(name)) = (level, &bookmark)
            && level >= 1
        {
            let mut text = p.text.clone().unwrap_or_default();
            collect_text(&p.children, &mut text);
            self.headings.push((level, text, name.clone()));
        }
        Ok(())
    }

    /// Text with `\n` as line breaks and `\t` as tabs, in runs carrying `rpr`.
    fn text_runs(text: &str, rpr: &str, out: &mut String) {
        out.push_str("<w:r>");
        out.push_str(rpr);
        let mut first = true;
        for line in text.split('\n') {
            if !first {
                out.push_str("<w:br/>");
            }
            first = false;
            let mut first_piece = true;
            for piece in line.split('\t') {
                if !first_piece {
                    out.push_str("<w:tab/>");
                }
                first_piece = false;
                if !piece.is_empty() {
                    let _ = write!(out, r#"<w:t xml:space="preserve">{}</w:t>"#, esc(piece));
                }
            }
        }
        out.push_str("</w:r>");
    }

    fn inlines(&mut self, inlines: &[InlineSpec], rels: &mut Rels, out: &mut String) -> Result<()> {
        for inline in inlines {
            match inline {
                InlineSpec::Text(run) => {
                    let style = run.style.as_deref().map(|s| self.style_id(s)).transpose()?;
                    let rpr = Self::rpr(&run.format, style.as_deref())?;
                    if let Some(breaks) = run.breaks {
                        out.push_str("<w:r>");
                        for _ in 0..breaks.min(64) {
                            out.push_str("<w:br/>");
                        }
                        out.push_str("</w:r>");
                    }
                    Self::text_runs(&run.text, &rpr, out);
                }
                InlineSpec::InsertedText(run) | InlineSpec::DeletedText(run) => {
                    let deleted = matches!(inline, InlineSpec::DeletedText(_));
                    let id = run.id.unwrap_or_else(|| {
                        let id = 10_000 + self.next_revision;
                        self.next_revision += 1;
                        id
                    });
                    let tag = if deleted { "del" } else { "ins" };
                    let _ = write!(
                        out,
                        r#"<w:{tag} w:id="{id}" w:author="{}""#,
                        esc(run.author.as_deref().unwrap_or("OpenDoc"))
                    );
                    if let Some(date) = &run.date {
                        let _ = write!(out, r#" w:date="{}""#, esc(date));
                    }
                    out.push('>');
                    let rpr = Self::rpr(&run.format, None)?;
                    let _ = write!(
                        out,
                        r#"<w:r>{rpr}<w:{t} xml:space="preserve">{}</w:{t}></w:r>"#,
                        esc(&run.text),
                        t = if deleted { "delText" } else { "t" }
                    );
                    let _ = write!(out, "</w:{tag}>");
                }
                InlineSpec::Image(image) => self.image(image, rels, out)?,
                InlineSpec::Hyperlink(link) => {
                    let id = rels.add("hyperlink", &link.link, true);
                    let _ = write!(out, r#"<w:hyperlink r:id="{id}" w:history="1">"#);
                    self.linked(&link.children, rels, out)?;
                    out.push_str("</w:hyperlink>");
                }
                InlineSpec::InternalHyperlink(link) => {
                    let _ = write!(
                        out,
                        r#"<w:hyperlink w:anchor="{}" w:history="1">"#,
                        esc(&link.anchor)
                    );
                    self.linked(&link.children, rels, out)?;
                    out.push_str("</w:hyperlink>");
                }
                InlineSpec::Bookmark(bookmark) => {
                    let id = self.next_bookmark;
                    self.next_bookmark += 1;
                    let _ = write!(
                        out,
                        r#"<w:bookmarkStart w:id="{id}" w:name="{}"/>"#,
                        esc(&bookmark.id)
                    );
                    self.inlines(&bookmark.children, rels, out)?;
                    let _ = write!(out, r#"<w:bookmarkEnd w:id="{id}"/>"#);
                }
                InlineSpec::PageBreak => out.push_str(r#"<w:r><w:br w:type="page"/></w:r>"#),
                InlineSpec::ColumnBreak => out.push_str(r#"<w:r><w:br w:type="column"/></w:r>"#),
                InlineSpec::LineBreak => out.push_str("<w:r><w:br/></w:r>"),
                InlineSpec::Tab => out.push_str("<w:r><w:tab/></w:r>"),
                InlineSpec::FootnoteReference(note) => {
                    let id = self.note_id(&note.id, &self.spec.footnotes, "footnote")?;
                    let _ = write!(
                        out,
                        r#"<w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="{id}"/></w:r>"#
                    );
                }
                InlineSpec::EndnoteReference(note) => {
                    let id = self.note_id(&note.id, &self.spec.endnotes, "endnote")?;
                    let _ = write!(
                        out,
                        r#"<w:r><w:rPr><w:rStyle w:val="EndnoteReference"/></w:rPr><w:endnoteReference w:id="{id}"/></w:r>"#
                    );
                }
                InlineSpec::CommentRangeStart(c) => {
                    self.comment_exists(c.id)?;
                    let _ = write!(out, r#"<w:commentRangeStart w:id="{}"/>"#, c.id);
                }
                InlineSpec::CommentRangeEnd(c) => {
                    self.comment_exists(c.id)?;
                    let _ = write!(out, r#"<w:commentRangeEnd w:id="{}"/>"#, c.id);
                }
                InlineSpec::CommentReference(c) => {
                    self.comment_exists(c.id)?;
                    let _ = write!(out, r#"<w:r><w:commentReference w:id="{}"/></w:r>"#, c.id);
                }
                InlineSpec::Field(field) => {
                    Self::field(&field.instruction, field.text.as_deref(), "", out)
                }
            }
        }
        Ok(())
    }

    /// Link content: plain runs take the Hyperlink character style.
    fn linked(&mut self, children: &[InlineSpec], rels: &mut Rels, out: &mut String) -> Result<()> {
        let styled: Vec<InlineSpec> = children
            .iter()
            .map(|child| match child {
                InlineSpec::Text(run) if run.style.is_none() => {
                    let mut run = run.clone();
                    run.style = Some("Hyperlink".to_owned());
                    InlineSpec::Text(run)
                }
                other => other.clone(),
            })
            .collect();
        self.inlines(&styled, rels, out)
    }

    fn field(instruction: &str, text: Option<&str>, rpr: &str, out: &mut String) {
        let _ = write!(
            out,
            r#"<w:r>{rpr}<w:fldChar w:fldCharType="begin"/></w:r><w:r>{rpr}<w:instrText xml:space="preserve"> {} </w:instrText></w:r><w:r>{rpr}<w:fldChar w:fldCharType="separate"/></w:r>"#,
            esc(instruction.trim())
        );
        let shown = text.unwrap_or(match instruction.split_whitespace().next() {
            Some(word) if word.eq_ignore_ascii_case("PAGE") => "1",
            Some(word) if word.eq_ignore_ascii_case("NUMPAGES") => "1",
            Some(word) if word.eq_ignore_ascii_case("SECTIONPAGES") => "1",
            _ => "",
        });
        if !shown.is_empty() {
            let _ = write!(
                out,
                r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#,
                esc(shown)
            );
        }
        let _ = write!(out, r#"<w:r>{rpr}<w:fldChar w:fldCharType="end"/></w:r>"#);
    }

    fn note_id(&self, id: &str, notes: &BTreeMap<String, StorySpec>, kind: &str) -> Result<u32> {
        if !notes.contains_key(id) {
            return Err(build_error(format!("no {kind} with id \"{id}\"")));
        }
        id.parse::<u32>()
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| build_error(format!("{kind} id \"{id}\" must be a number from 1")))
    }

    fn comment_exists(&self, id: u32) -> Result<()> {
        let known = self
            .spec
            .comments
            .as_ref()
            .is_some_and(|c| c.children.iter().any(|comment| comment.id == id));
        if known {
            Ok(())
        } else {
            Err(build_error(format!("no comment with id {id}")))
        }
    }

    fn image(&mut self, image: &ImageSpec, rels: &mut Rels, out: &mut String) -> Result<()> {
        let bytes: Vec<u8> = match (&image.data, image.media) {
            (Some(data), _) => base64_decode(data)
                .ok_or_else(|| build_error("an image's data is not valid base64"))?,
            (None, Some(index)) => self
                .media
                .get(index)
                .cloned()
                .ok_or_else(|| build_error(format!("no media at index {index}")))?,
            (None, None) => return Err(build_error("an image needs data or a media index")),
        };
        let sniffed = sniff_image(&bytes);
        let extension: &'static str = match image.image_type.as_deref() {
            Some("png") => "png",
            Some("jpg" | "jpeg") => "jpeg",
            Some("gif") => "gif",
            Some("bmp") => "bmp",
            Some(other) => return Err(build_error(format!("unsupported image type \"{other}\""))),
            None => sniffed
                .map(|(ext, ..)| ext)
                .ok_or_else(|| build_error("cannot tell the image type; set `type`"))?,
        };
        let (width_px, height_px) = match &image.transformation {
            Some(t) if t.width > 0.0 && t.height > 0.0 => (t.width, t.height),
            Some(_) => return Err(build_error("an image's width and height must be positive")),
            None => sniffed
                .map(|(_, w, h)| (f64::from(w), f64::from(h)))
                .ok_or_else(|| build_error("cannot read the image's size; set `transformation`"))?,
        };
        let index = self.media_parts.len() + 1;
        let part = format!("media/image{index}.{extension}");
        self.media_parts.push((format!("word/{part}"), bytes));
        if !self.media_extensions.contains(&extension) {
            self.media_extensions.push(extension);
        }
        let rid = rels.add("image", &part, false);
        let id = self.next_drawing;
        self.next_drawing += 1;
        let cx = (width_px * EMU_PER_PX).round() as i64;
        let cy = (height_px * EMU_PER_PX).round() as i64;
        let alt = image.alt_text.clone().unwrap_or_default();
        let name = alt.name.unwrap_or_else(|| format!("Picture {id}"));
        let _ = write!(
            out,
            r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{id}" name="{}"{}{}/><wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="{}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#,
            esc(&name),
            alt.description
                .map(|d| format!(r#" descr="{}""#, esc(&d)))
                .unwrap_or_default(),
            alt.title
                .map(|t| format!(r#" title="{}""#, esc(&t)))
                .unwrap_or_default(),
            esc(&name),
        );
        Ok(())
    }

    fn width(width: &WidthSpec) -> Result<(String, &'static str)> {
        let kind = width.kind.as_deref().unwrap_or("dxa");
        match (&width.size, kind) {
            (serde_json::Value::String(s), _) if s.ends_with('%') => {
                let pct: f64 = s
                    .trim_end_matches('%')
                    .trim()
                    .parse()
                    .map_err(|_| build_error(format!("\"{s}\" is not a percentage")))?;
                Ok((((pct * 50.0).round() as i64).to_string(), "pct"))
            }
            (serde_json::Value::Number(n), "pct") => Ok((
                n.as_f64().map_or(0, |v| v.round() as i64).to_string(),
                "pct",
            )),
            (serde_json::Value::Number(n), "auto") => Ok((
                n.as_f64().map_or(0, |v| v.round() as i64).to_string(),
                "auto",
            )),
            (serde_json::Value::Number(n), "dxa") => Ok((
                n.as_f64().map_or(0, |v| v.round() as i64).to_string(),
                "dxa",
            )),
            _ => Err(build_error(
                "a width is { size, type: \"dxa\" | \"pct\" | \"auto\" }",
            )),
        }
    }

    fn table(&mut self, table: &TableSpec, rels: &mut Rels, out: &mut String) -> Result<()> {
        if table.rows.is_empty() {
            return Err(build_error("a table needs at least one row"));
        }
        // The grid: explicit widths, or the widest row's span count, each
        // column an equal share of a 6.5-inch text width.
        let columns = table
            .rows
            .iter()
            .map(|row| {
                row.children
                    .iter()
                    .map(|cell| cell.column_span.unwrap_or(1).max(1) as usize)
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(1)
            .max(table.column_widths.len())
            .max(1);
        let widths: Vec<i32> = if table.column_widths.is_empty() {
            let each = 9_360 / i32::try_from(columns).unwrap_or(1).max(1);
            vec![each; columns]
        } else {
            let mut widths = table.column_widths.clone();
            widths.resize(columns, *widths.last().unwrap_or(&1_440));
            widths
        };
        let style = self.style_id(table.style.as_deref().unwrap_or("TableGrid"))?;
        out.push_str("<w:tbl><w:tblPr>");
        let _ = write!(out, r#"<w:tblStyle w:val="{}"/>"#, esc(&style));
        match &table.width {
            Some(width) => {
                let (w, kind) = Self::width(width)?;
                let _ = write!(out, r#"<w:tblW w:w="{w}" w:type="{kind}"/>"#);
            }
            None => out.push_str(r#"<w:tblW w:w="0" w:type="auto"/>"#),
        }
        if let Some(alignment) = &table.alignment {
            let _ = write!(out, r#"<w:jc w:val="{}"/>"#, Self::alignment(alignment)?);
        }
        if let Some(layout) = &table.layout {
            let _ = write!(
                out,
                r#"<w:tblLayout w:type="{}"/>"#,
                if layout.eq_ignore_ascii_case("fixed") {
                    "fixed"
                } else {
                    "autofit"
                }
            );
        }
        out.push_str(r#"<w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" w:firstColumn="1" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/></w:tblPr><w:tblGrid>"#);
        for width in &widths {
            let _ = write!(out, r#"<w:gridCol w:w="{width}"/>"#);
        }
        out.push_str("</w:tblGrid>");
        // Vertical merges pending from rows above: grid column → (span, rows left).
        let mut pending: BTreeMap<usize, (u32, u32)> = BTreeMap::new();
        for row in &table.rows {
            out.push_str("<w:tr>");
            let mut trpr = String::new();
            if row.cant_split {
                trpr.push_str("<w:cantSplit/>");
            }
            if let Some(height) = &row.height {
                let _ = write!(
                    trpr,
                    r#"<w:trHeight w:val="{}" w:hRule="{}"/>"#,
                    height.value,
                    esc(height.rule.as_deref().unwrap_or("atLeast"))
                );
            }
            if row.table_header {
                trpr.push_str("<w:tblHeader/>");
            }
            if !trpr.is_empty() {
                let _ = write!(out, "<w:trPr>{trpr}</w:trPr>");
            }
            let mut column = 0_usize;
            let mut cells = row.children.iter();
            loop {
                if let Some((span, left)) = pending.get(&column).copied() {
                    // A continuation of a merge from above.
                    let width: i32 = widths.iter().skip(column).take(span as usize).sum();
                    let _ = write!(
                        out,
                        r#"<w:tc><w:tcPr><w:tcW w:w="{width}" w:type="dxa"/>{}<w:vMerge/></w:tcPr><w:p/></w:tc>"#,
                        if span > 1 {
                            format!(r#"<w:gridSpan w:val="{span}"/>"#)
                        } else {
                            String::new()
                        }
                    );
                    if left <= 1 {
                        pending.remove(&column);
                    } else {
                        pending.insert(column, (span, left - 1));
                    }
                    column += span as usize;
                    continue;
                }
                let Some(cell) = cells.next() else { break };
                let span = cell.column_span.unwrap_or(1).max(1);
                out.push_str("<w:tc><w:tcPr>");
                match &cell.width {
                    Some(width) => {
                        let (w, kind) = Self::width(width)?;
                        let _ = write!(out, r#"<w:tcW w:w="{w}" w:type="{kind}"/>"#);
                    }
                    None => {
                        let width: i32 = widths.iter().skip(column).take(span as usize).sum();
                        let _ = write!(out, r#"<w:tcW w:w="{width}" w:type="dxa"/>"#);
                    }
                }
                if span > 1 {
                    let _ = write!(out, r#"<w:gridSpan w:val="{span}"/>"#);
                }
                if let Some(rows) = cell.row_span.filter(|r| *r > 1) {
                    out.push_str(r#"<w:vMerge w:val="restart"/>"#);
                    pending.insert(column, (span, rows - 1));
                }
                if let Some(shading) = &cell.shading {
                    out.push_str(&Self::shading(shading)?);
                }
                if let Some(v) = &cell.vertical_align {
                    let v = match v.to_ascii_lowercase().as_str() {
                        "top" => "top",
                        "center" => "center",
                        "bottom" => "bottom",
                        other => {
                            return Err(build_error(format!(
                                "\"{other}\" is not a vertical alignment"
                            )));
                        }
                    };
                    let _ = write!(out, r#"<w:vAlign w:val="{v}"/>"#);
                }
                out.push_str("</w:tcPr>");
                if cell.children.is_empty() {
                    out.push_str("<w:p/>");
                } else {
                    self.blocks(&cell.children, rels, out)?;
                    // A cell must end with a paragraph.
                    if matches!(cell.children.last(), Some(BlockSpec::Table(_))) {
                        out.push_str("<w:p/>");
                    }
                }
                out.push_str("</w:tc>");
                column += span as usize;
            }
            out.push_str("</w:tr>");
        }
        out.push_str("</w:tbl>");
        Ok(())
    }

    /// A table of contents: Word's `TOC` field with a cached result written
    /// from the spec's headings (titles, linked; without page numbers, which
    /// Word fills when it updates the field; `features.updateFields` asks it to).
    fn toc(&mut self, toc: &TocSpec, out: &mut String) -> Result<()> {
        let range = toc.heading_style_range.as_deref().unwrap_or("1-3");
        let (low, high) = range
            .split_once('-')
            .and_then(|(a, b)| Some((a.trim().parse::<u8>().ok()?, b.trim().parse::<u8>().ok()?)))
            .filter(|(a, b)| 1 <= *a && a <= b && *b <= 9)
            .ok_or_else(|| {
                build_error(format!("\"{range}\" is not a heading range like \"1-3\""))
            })?;
        if let Some(title) = &toc.title {
            let _ = write!(
                out,
                r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/><w:outlineLvl w:val="9"/></w:pPr><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                esc(title)
            );
        }
        let link = toc.hyperlink.unwrap_or(true);
        let instruction = format!(
            "TOC \\o \"{low}-{high}\"{} \\z \\u",
            if link { " \\h" } else { "" }
        );
        // Headings are known only after the whole body is written, so the
        // entries are a placeholder replaced in `finish_toc`.
        let _ = write!(out, "\u{0}TOC:{instruction}\u{0}");
        let _ = (low, high);
        Ok(())
    }

    fn finish_toc(&self, body: &str) -> String {
        let mut out = String::with_capacity(body.len());
        let mut rest = body;
        while let Some(start) = rest.find("\u{0}TOC:") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 5..];
            let Some(end) = after.find('\u{0}') else {
                break;
            };
            let instruction = &after[..end];
            rest = &after[end + 1..];
            let (low, high) = instruction
                .split('"')
                .nth(1)
                .and_then(|r| r.split_once('-'))
                .and_then(|(a, b)| Some((a.parse::<u8>().ok()?, b.parse::<u8>().ok()?)))
                .unwrap_or((1, 3));
            let link = instruction.contains("\\h");
            let entries: Vec<_> = self
                .headings
                .iter()
                .filter(|(level, ..)| (low..=high).contains(level))
                .collect();
            let begin = format!(
                r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>"#,
                esc(instruction)
            );
            let end_field = r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
            if entries.is_empty() {
                let _ = write!(
                    out,
                    r#"<w:p><w:pPr><w:pStyle w:val="TOC1"/></w:pPr>{begin}<w:r><w:t>No table of contents entries found.</w:t></w:r>{end_field}</w:p>"#
                );
                continue;
            }
            for (index, (level, text, bookmark)) in entries.iter().enumerate() {
                let style = format!("TOC{}", (*level).min(3));
                let _ = write!(out, r#"<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr>"#);
                if index == 0 {
                    out.push_str(&begin);
                }
                if link {
                    let _ = write!(
                        out,
                        r#"<w:hyperlink w:anchor="{}" w:history="1"><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:hyperlink>"#,
                        esc(bookmark),
                        esc(text)
                    );
                } else {
                    let _ = write!(
                        out,
                        r#"<w:r><w:t xml:space="preserve">{}</w:t></w:r>"#,
                        esc(text)
                    );
                }
                out.push_str("</w:p>");
            }
            // Word's own shape: the field ends in a paragraph after the last
            // entry, so the field spans paragraphs and its linked entries are
            // a field RANGE. The engine refuses a hyperlink inside a
            // single-paragraph field's result (a field's inlines are leaves),
            // and that refusal fails the whole open (`docs/176` §12.3).
            let _ = write!(out, "<w:p>{end_field}</w:p>");
        }
        out.push_str(rest);
        out
    }

    // -- sections --

    fn sect_pr(
        &mut self,
        section: &SectionSpec,
        rels: &mut Rels,
        stories: &mut Vec<(String, String, Rels)>,
    ) -> Result<String> {
        let mut out = String::from("<w:sectPr>");
        for (group, tag, kind_name) in [
            (&section.headers, "header", "header"),
            (&section.footers, "footer", "footer"),
        ] {
            let Some(group) = group else { continue };
            for (kind, story) in [
                ("default", &group.default),
                ("first", &group.first),
                ("even", &group.even),
            ] {
                let Some(story) = story else { continue };
                let index = stories.iter().filter(|(n, ..)| n.starts_with(tag)).count() + 1;
                let name = format!("{tag}{index}.xml");
                let mut story_rels = Rels::default();
                let mut xml = String::new();
                self.blocks(&story.children, &mut story_rels, &mut xml)?;
                if story.children.is_empty() {
                    xml.push_str("<w:p/>");
                }
                let root = if tag == "header" { "hdr" } else { "ftr" };
                let part = format!(
                    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{root} {W_NS}>{xml}</w:{root}>"#
                );
                let rid = rels.add(kind_name, &name, false);
                let _ = write!(out, r#"<w:{tag}Reference w:type="{kind}" r:id="{rid}"/>"#);
                stories.push((name, part, story_rels));
            }
        }
        let props = section.properties.clone().unwrap_or_default();
        if let Some(kind) = &props.kind {
            let kind = match kind.as_str() {
                "nextPage" | "continuous" | "evenPage" | "oddPage" | "nextColumn" => kind.as_str(),
                other => return Err(build_error(format!("\"{other}\" is not a section type"))),
            };
            let _ = write!(out, r#"<w:type w:val="{kind}"/>"#);
        }
        let page = props.page.clone().unwrap_or_default();
        let size = page.size.clone().unwrap_or_default();
        let landscape = size
            .orientation
            .as_deref()
            .is_some_and(|o| o.eq_ignore_ascii_case("landscape"));
        // docx-js's default page is A4 portrait.
        let (mut width, mut height) = (size.width.unwrap_or(11_906), size.height.unwrap_or(16_838));
        if landscape && width < height {
            std::mem::swap(&mut width, &mut height);
        }
        if !(1_440..=31_680).contains(&width) || !(1_440..=31_680).contains(&height) {
            return Err(build_error(
                "a page side must be 1 to 22 inches (1440-31680 twips)",
            ));
        }
        let _ = write!(
            out,
            r#"<w:pgSz w:w="{width}" w:h="{height}"{}/>"#,
            if landscape {
                r#" w:orient="landscape""#
            } else {
                ""
            }
        );
        let margin = page.margin.clone().unwrap_or_default();
        let _ = write!(
            out,
            r#"<w:pgMar w:top="{}" w:right="{}" w:bottom="{}" w:left="{}" w:header="{}" w:footer="{}" w:gutter="{}"/>"#,
            margin.top.unwrap_or(1_440),
            margin.right.unwrap_or(1_440),
            margin.bottom.unwrap_or(1_440),
            margin.left.unwrap_or(1_440),
            margin.header.unwrap_or(708),
            margin.footer.unwrap_or(708),
            margin.gutter.unwrap_or(0),
        );
        if let Some(numbers) = &page.page_numbers {
            out.push_str("<w:pgNumType");
            if let Some(format) = &numbers.format_type {
                let _ = write!(out, r#" w:fmt="{}""#, esc(format));
            }
            if let Some(start) = numbers.start {
                let _ = write!(out, r#" w:start="{start}""#);
            }
            out.push_str("/>");
        }
        match &props.column {
            Some(column) => {
                let _ = write!(
                    out,
                    r#"<w:cols w:space="{}" w:num="{}"{}/>"#,
                    column.space.unwrap_or(708),
                    column.count.unwrap_or(1).max(1),
                    if column.separate { r#" w:sep="1""# } else { "" }
                );
            }
            None => out.push_str(r#"<w:cols w:space="708"/>"#),
        }
        if props.title_page {
            out.push_str("<w:titlePg/>");
        }
        out.push_str("</w:sectPr>");
        Ok(out)
    }

    // -- package parts --

    fn styles_xml(&self) -> Result<String> {
        let mut out = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {W_NS}><w:docDefaults><w:rPrDefault>"#
        );
        let defaults = self
            .spec
            .styles
            .as_ref()
            .and_then(|s| s.default.as_ref())
            .and_then(|d| d.document.clone())
            .unwrap_or_default();
        let base = RunFormatSpec {
            font: Some("Calibri".to_owned()),
            size: Some(22),
            ..RunFormatSpec::default()
        };
        let run = merge_run(&base, defaults.run.as_ref());
        let rpr = Self::rpr(&run, None)?;
        out.push_str(if rpr.is_empty() { "<w:rPr/>" } else { &rpr });
        out.push_str("</w:rPrDefault><w:pPrDefault><w:pPr>");
        let paragraph = defaults.paragraph.unwrap_or_else(|| ParagraphFormatSpec {
            spacing: Some(SpacingSpec {
                before: None,
                after: Some(160),
                line: Some(259),
                line_rule: Some("auto".to_owned()),
            }),
            ..ParagraphFormatSpec::default()
        });
        out.push_str(&Self::ppr(&paragraph, None, None)?);
        out.push_str("</w:pPr></w:pPrDefault></w:docDefaults>");
        let overrides = |id: &str| -> Option<&StyleSpec> {
            self.spec.styles.as_ref().and_then(|s| {
                s.paragraph_styles
                    .iter()
                    .chain(&s.character_styles)
                    .find(|style| style.id == id)
            })
        };
        out.push_str(r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:uiPriority w:val="99"/><w:semiHidden/><w:unhideWhenUsed/><w:tblPr><w:tblInd w:w="0" w:type="dxa"/><w:tblCellMar><w:top w:w="0" w:type="dxa"/><w:left w:w="108" w:type="dxa"/><w:bottom w:w="0" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>"#);
        for (id, name, kind, body) in BUILTIN_STYLES {
            if let Some(custom) = overrides(id) {
                out.push_str(&self.custom_style(custom, kind)?);
                continue;
            }
            let default = if *id == "Normal" {
                r#" w:default="1""#
            } else {
                ""
            };
            let _ = write!(
                out,
                r#"<w:style w:type="{kind}"{default} w:styleId="{id}"><w:name w:val="{name}"/>{body}</w:style>"#
            );
        }
        if let Some(styles) = &self.spec.styles {
            for (list, kind) in [
                (&styles.paragraph_styles, "paragraph"),
                (&styles.character_styles, "character"),
            ] {
                for style in list {
                    if BUILTIN_STYLES.iter().any(|(id, ..)| *id == style.id) {
                        continue;
                    }
                    out.push_str(&self.custom_style(style, kind)?);
                }
            }
        }
        out.push_str("</w:styles>");
        Ok(out)
    }

    fn custom_style(&self, style: &StyleSpec, kind: &str) -> Result<String> {
        let mut out = format!(
            r#"<w:style w:type="{kind}"{} w:customStyle="1" w:styleId="{}"><w:name w:val="{}"/>"#,
            if style.id == "Normal" {
                r#" w:default="1""#
            } else {
                ""
            },
            esc(&style.id),
            esc(style.name.as_deref().unwrap_or(&style.id))
        );
        if let Some(based) = &style.based_on {
            let _ = write!(
                out,
                r#"<w:basedOn w:val="{}"/>"#,
                esc(&self.style_id(based)?)
            );
        }
        if let Some(next) = &style.next {
            let _ = write!(out, r#"<w:next w:val="{}"/>"#, esc(&self.style_id(next)?));
        }
        if style.quick_format {
            out.push_str("<w:qFormat/>");
        }
        if let Some(paragraph) = &style.paragraph {
            let ppr = Self::ppr(paragraph, None, None)?;
            if !ppr.is_empty() {
                let _ = write!(out, "<w:pPr>{ppr}</w:pPr>");
            }
        }
        if let Some(run) = &style.run {
            out.push_str(&Self::rpr(run, None)?);
        }
        out.push_str("</w:style>");
        Ok(out)
    }

    fn numbering_xml(&self) -> Result<String> {
        let mut out = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering {W_NS}>"#
        );
        // abstract 0: bullets; abstract 1: decimal.
        let bullets = ["\u{2022}", "o", "\u{25AA}"];
        out.push_str(
            r#"<w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="hybridMultilevel"/>"#,
        );
        for level in 0..9_u32 {
            let symbol = bullets[(level % 3) as usize];
            let _ = write!(
                out,
                r#"<w:lvl w:ilvl="{level}"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="{symbol}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{}" w:hanging="360"/></w:pPr>{}</w:lvl>"#,
                720 * (level + 1),
                if symbol == "o" {
                    r#"<w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New"/></w:rPr>"#
                } else {
                    ""
                }
            );
        }
        out.push_str(r#"</w:abstractNum><w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="hybridMultilevel"/>"#);
        let formats = ["decimal", "lowerLetter", "lowerRoman"];
        for level in 0..9_u32 {
            let _ = write!(
                out,
                r#"<w:lvl w:ilvl="{level}"><w:start w:val="1"/><w:numFmt w:val="{}"/><w:lvlText w:val="%{}."/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{}" w:hanging="360"/></w:pPr></w:lvl>"#,
                formats[(level % 3) as usize],
                level + 1,
                720 * (level + 1)
            );
        }
        out.push_str("</w:abstractNum>");
        if let Some(numbering) = &self.spec.numbering {
            for config in &numbering.config {
                let id = self.abstract_ids[&config.reference];
                let _ = write!(
                    out,
                    r#"<w:abstractNum w:abstractNumId="{id}"><w:multiLevelType w:val="hybridMultilevel"/>"#
                );
                let mut levels = config.levels.clone();
                levels.sort_by_key(|l| l.level);
                for level in &levels {
                    if level.level > 8 {
                        return Err(build_error("numbering levels are 0 to 8"));
                    }
                    let format = level.format.as_deref().unwrap_or("decimal");
                    let text = level
                        .text
                        .clone()
                        .unwrap_or_else(|| format!("%{}.", level.level + 1));
                    let jc = match level.alignment.as_deref().unwrap_or("left") {
                        "start" | "left" => "left",
                        "end" | "right" => "right",
                        "center" => "center",
                        other => {
                            return Err(build_error(format!("\"{other}\" is not an alignment")));
                        }
                    };
                    let style = level.style.clone().unwrap_or_default();
                    let paragraph = style.paragraph.unwrap_or_else(|| ParagraphFormatSpec {
                        indent: Some(IndentSpec {
                            left: Some(720 * (i32::from(level.level) + 1)),
                            hanging: Some(360),
                            ..IndentSpec::default()
                        }),
                        ..ParagraphFormatSpec::default()
                    });
                    let ppr = Self::ppr(&paragraph, None, None)?;
                    let rpr = match &style.run {
                        Some(run) => Self::rpr(run, None)?,
                        None => String::new(),
                    };
                    let _ = write!(
                        out,
                        r#"<w:lvl w:ilvl="{}"><w:start w:val="{}"/><w:numFmt w:val="{}"/><w:lvlText w:val="{}"/><w:lvlJc w:val="{jc}"/>{}{rpr}</w:lvl>"#,
                        level.level,
                        level.start.unwrap_or(1),
                        esc(format),
                        esc(&text),
                        if ppr.is_empty() {
                            String::new()
                        } else {
                            format!("<w:pPr>{ppr}</w:pPr>")
                        }
                    );
                }
                out.push_str("</w:abstractNum>");
            }
        }
        out.push_str(r#"<w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num>"#);
        for ((reference, instance), num_id) in &self.num_ids {
            let abstract_id = self.abstract_ids[reference];
            let _ = write!(
                out,
                r#"<w:num w:numId="{num_id}"><w:abstractNumId w:val="{abstract_id}"/>{}</w:num>"#,
                if *instance > 0 {
                    r#"<w:lvlOverride w:ilvl="0"><w:startOverride w:val="1"/></w:lvlOverride>"#
                } else {
                    ""
                }
            );
        }
        out.push_str("</w:numbering>");
        Ok(out)
    }

    fn notes_xml(
        &mut self,
        kind: &str,
        notes: &BTreeMap<String, StorySpec>,
        stories: &mut Vec<(String, String, Rels)>,
    ) -> Result<Option<(String, Rels)>> {
        if notes.is_empty() {
            return Ok(None);
        }
        let mut rels = Rels::default();
        let mut out = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{kind}s {W_NS}><w:{kind} w:type="separator" w:id="-1"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:separator/></w:r></w:p></w:{kind}><w:{kind} w:type="continuationSeparator" w:id="0"><w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:continuationSeparator/></w:r></w:p></w:{kind}>"#
        );
        let style_ref = if kind == "footnote" {
            "FootnoteReference"
        } else {
            "EndnoteReference"
        };
        let mut ordered: Vec<(u32, &StorySpec)> = Vec::new();
        for (id, note) in notes {
            let number = self.note_id(id, notes, kind)?;
            ordered.push((number, note));
        }
        ordered.sort_by_key(|(n, _)| *n);
        for (id, note) in ordered {
            let _ = write!(out, r#"<w:{kind} w:id="{id}">"#);
            let mut body = String::new();
            self.blocks(&note.children, &mut rels, &mut body)?;
            if body.is_empty() {
                body.push_str("<w:p/>");
            }
            // The note's number mark opens its first paragraph, as Word writes it.
            let mark = format!(
                r#"<w:r><w:rPr><w:rStyle w:val="{style_ref}"/></w:rPr><w:{kind}Ref/></w:r><w:r><w:t xml:space="preserve"> </w:t></w:r>"#
            );
            let body = insert_after_ppr(&body, &mark);
            out.push_str(&body);
            let _ = write!(out, "</w:{kind}>");
        }
        let _ = write!(out, "</w:{kind}s>");
        let _ = stories;
        Ok(Some((out, rels)))
    }

    fn comments_xml(&mut self) -> Result<Option<(String, Rels)>> {
        let Some(comments) = &self.spec.comments else {
            return Ok(None);
        };
        if comments.children.is_empty() {
            return Ok(None);
        }
        let mut rels = Rels::default();
        let mut out = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:comments {W_NS}>"#
        );
        for comment in &comments.children {
            let _ = write!(
                out,
                r#"<w:comment w:id="{}" w:author="{}""#,
                comment.id,
                esc(comment.author.as_deref().unwrap_or("OpenDoc"))
            );
            if let Some(initials) = &comment.initials {
                let _ = write!(out, r#" w:initials="{}""#, esc(initials));
            }
            if let Some(date) = &comment.date {
                let _ = write!(out, r#" w:date="{}""#, esc(date));
            }
            out.push('>');
            let mut body = String::new();
            self.blocks(&comment.children, &mut rels, &mut body)?;
            if body.is_empty() {
                body.push_str("<w:p/>");
            }
            out.push_str(&body);
            out.push_str("</w:comment>");
        }
        out.push_str("</w:comments>");
        Ok(Some((out, rels)))
    }

    fn core_xml(&self) -> String {
        let mut out = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#,
        );
        for (tag, value) in [
            ("dc:title", &self.spec.title),
            ("dc:subject", &self.spec.subject),
            ("dc:creator", &self.spec.creator),
            ("cp:keywords", &self.spec.keywords),
            ("dc:description", &self.spec.description),
            ("cp:lastModifiedBy", &self.spec.last_modified_by),
        ] {
            if let Some(value) = value {
                let _ = write!(out, "<{tag}>{}</{tag}>", esc(value));
            }
        }
        out.push_str("</cp:coreProperties>");
        out
    }
}

/// Writes `mark` right after the first paragraph's `<w:pPr>…</w:pPr>`, or
/// after its `<w:p>` when it has none.
fn insert_after_ppr(body: &str, mark: &str) -> String {
    if let Some(start) = body.find("<w:p>") {
        let open_end = start + "<w:p>".len();
        let insert_at = if body[open_end..].starts_with("<w:pPr>") {
            body[open_end..]
                .find("</w:pPr>")
                .map_or(open_end, |end| open_end + end + "</w:pPr>".len())
        } else {
            open_end
        };
        format!("{}{mark}{}", &body[..insert_at], &body[insert_at..])
    } else if let Some(start) = body.find("<w:p/>") {
        format!(
            "{}<w:p>{mark}</w:p>{}",
            &body[..start],
            &body[start + "<w:p/>".len()..]
        )
    } else {
        body.to_owned()
    }
}

fn merge_run(base: &RunFormatSpec, over: Option<&RunFormatSpec>) -> RunFormatSpec {
    let Some(over) = over else {
        return base.clone();
    };
    let mut merged = over.clone();
    if merged.font.is_none() {
        merged.font.clone_from(&base.font);
    }
    if merged.size.is_none() {
        merged.size = base.size;
    }
    merged
}

fn collect_text(inlines: &[InlineSpec], out: &mut String) {
    for inline in inlines {
        match inline {
            InlineSpec::Text(run) => out.push_str(&run.text),
            InlineSpec::InsertedText(run) => out.push_str(&run.text),
            InlineSpec::Hyperlink(link) => collect_text(&link.children, out),
            InlineSpec::InternalHyperlink(link) => collect_text(&link.children, out),
            InlineSpec::Bookmark(b) => collect_text(&b.children, out),
            InlineSpec::Tab => out.push('\t'),
            _ => {}
        }
    }
}

/// Builds the WordprocessingML package for `spec`. `media` holds the bytes
/// `ImageSpec::media` indexes.
///
/// # Errors
///
/// [`ErrorCode::Build`] naming the first invalid value (an unknown style,
/// numbering reference, comment or note id; a bad colour; an unreadable image).
pub fn build_docx(spec: &DocumentSpec, media: &[Vec<u8>]) -> Result<Vec<u8>> {
    let mut writer = Writer::new(spec, media);
    if let Some(numbering) = &spec.numbering {
        for (index, config) in numbering.config.iter().enumerate() {
            if writer
                .abstract_ids
                .insert(
                    config.reference.clone(),
                    2 + u32::try_from(index).unwrap_or(0),
                )
                .is_some()
            {
                return Err(build_error(format!(
                    "numbering reference \"{}\" is defined twice",
                    config.reference
                )));
            }
        }
    }
    let mut rels = Rels::default();
    rels.add("styles", "styles.xml", false);
    rels.add("settings", "settings.xml", false);
    rels.add("numbering", "numbering.xml", false);
    let mut stories: Vec<(String, String, Rels)> = Vec::new();
    let mut body = String::new();
    let empty = [SectionSpec::default()];
    let sections: &[SectionSpec] = if spec.sections.is_empty() {
        &empty
    } else {
        &spec.sections
    };
    let mut final_sect = String::new();
    for (index, section) in sections.iter().enumerate() {
        let sect = writer.sect_pr(section, &mut rels, &mut stories)?;
        let last = index + 1 == sections.len();
        let children = &section.children;
        let mut written = String::new();
        if last {
            writer.blocks(children, &mut rels, &mut written)?;
            final_sect = sect;
        } else {
            // A non-final section's properties ride on its last paragraph; one
            // is added when the section ends in a table or is empty.
            let (head, tail) = match children.last() {
                Some(BlockSpec::Paragraph(p)) => (&children[..children.len() - 1], Some(p)),
                _ => (&children[..], None),
            };
            writer.blocks(head, &mut rels, &mut written)?;
            let carrier = tail.cloned().unwrap_or_default();
            writer.paragraph(&carrier, &mut rels, &mut written, Some(&sect))?;
        }
        body.push_str(&written);
    }
    let body = writer.finish_toc(&body);
    let notes = [
        ("footnote", spec.footnotes.clone()),
        ("endnote", spec.endnotes.clone()),
    ];
    let mut note_parts = Vec::new();
    for (kind, map) in &notes {
        if let Some((xml, note_rels)) = writer.notes_xml(kind, map, &mut stories)? {
            rels.add(&format!("{kind}s"), &format!("{kind}s.xml"), false);
            note_parts.push((format!("{kind}s.xml"), xml, note_rels));
        }
    }
    let comments = writer.comments_xml()?;
    if comments.is_some() {
        rels.add("comments", "comments.xml", false);
    }
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W_NS}><w:body>{body}{final_sect}</w:body></w:document>"#
    );
    let features = spec.features.clone().unwrap_or_default();
    let mut settings =
        format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings {W_NS}>"#);
    if features.track_revisions {
        settings.push_str("<w:trackRevisions/>");
    }
    settings.push_str(r#"<w:defaultTabStop w:val="720"/>"#);
    if features.update_fields {
        settings.push_str(r#"<w:updateFields w:val="true"/>"#);
    }
    settings.push_str(r#"<w:characterSpacingControl w:val="doNotCompress"/><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#);

    let mut content_types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
    );
    for extension in &writer.media_extensions {
        let _ = write!(
            content_types,
            r#"<Default Extension="{extension}" ContentType="image/{extension}"/>"#
        );
    }
    let wml = "application/vnd.openxmlformats-officedocument.wordprocessingml";
    for (part, kind) in [
        ("/word/document.xml", "document.main"),
        ("/word/styles.xml", "styles"),
        ("/word/settings.xml", "settings"),
        ("/word/numbering.xml", "numbering"),
    ] {
        let _ = write!(
            content_types,
            r#"<Override PartName="{part}" ContentType="{wml}.{kind}+xml"/>"#
        );
    }
    for (name, ..) in &stories {
        let kind = if name.starts_with("header") {
            "header"
        } else {
            "footer"
        };
        let _ = write!(
            content_types,
            r#"<Override PartName="/word/{name}" ContentType="{wml}.{kind}+xml"/>"#
        );
    }
    for (name, ..) in &note_parts {
        let kind = name.trim_end_matches(".xml");
        let _ = write!(
            content_types,
            r#"<Override PartName="/word/{name}" ContentType="{wml}.{kind}+xml"/>"#
        );
    }
    if comments.is_some() {
        let _ = write!(
            content_types,
            r#"<Override PartName="/word/comments.xml" ContentType="{wml}.comments+xml"/>"#
        );
    }
    content_types.push_str(r#"<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>"#);

    let package_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#;
    let app = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>OpenDoc</Application></Properties>"#;

    let mut parts: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".to_owned(), content_types.into_bytes()),
        ("_rels/.rels".to_owned(), package_rels.as_bytes().to_vec()),
        (
            "docProps/core.xml".to_owned(),
            writer.core_xml().into_bytes(),
        ),
        ("docProps/app.xml".to_owned(), app.as_bytes().to_vec()),
        ("word/document.xml".to_owned(), document.into_bytes()),
        (
            "word/styles.xml".to_owned(),
            writer.styles_xml()?.into_bytes(),
        ),
        ("word/settings.xml".to_owned(), settings.into_bytes()),
        (
            "word/numbering.xml".to_owned(),
            writer.numbering_xml()?.into_bytes(),
        ),
        (
            "word/_rels/document.xml.rels".to_owned(),
            rels.xml().into_bytes(),
        ),
    ];
    for (name, xml, story_rels) in stories.into_iter().chain(note_parts) {
        if !story_rels.entries.is_empty() {
            parts.push((
                format!("word/_rels/{name}.rels"),
                story_rels.xml().into_bytes(),
            ));
        }
        parts.push((format!("word/{name}"), xml.into_bytes()));
    }
    if let Some((xml, comment_rels)) = comments {
        if !comment_rels.entries.is_empty() {
            parts.push((
                "word/_rels/comments.xml.rels".to_owned(),
                comment_rels.xml().into_bytes(),
            ));
        }
        parts.push(("word/comments.xml".to_owned(), xml.into_bytes()));
    }
    parts.append(&mut writer.media_parts);
    zip_parts(parts)
}

fn zip_parts(parts: Vec<(String, Vec<u8>)>) -> Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    // A fixed timestamp: the same spec writes the same bytes.
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(DateTime::default());
    for (name, bytes) in parts {
        zip.start_file(name, options)
            .map_err(|e| build_error(format!("zip: {e}")))?;
        zip.write_all(&bytes)
            .map_err(|e| build_error(format!("zip: {e}")))?;
    }
    Ok(zip
        .finish()
        .map_err(|e| build_error(format!("zip: {e}")))?
        .into_inner())
}

/// The type and pixel size of a PNG, JPEG, GIF or BMP, from its header.
fn sniff_image(bytes: &[u8]) -> Option<(&'static str, u32, u32)> {
    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return Some(("png", w, h));
    }
    if bytes.len() >= 10 && (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) {
        let w = u32::from(u16::from_le_bytes([bytes[6], bytes[7]]));
        let h = u32::from(u16::from_le_bytes([bytes[8], bytes[9]]));
        return Some(("gif", w, h));
    }
    if bytes.len() >= 26 && bytes.starts_with(b"BM") {
        let w = i32::from_le_bytes(bytes[18..22].try_into().ok()?).unsigned_abs();
        let h = i32::from_le_bytes(bytes[22..26].try_into().ok()?).unsigned_abs();
        return Some(("bmp", w, h));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        // Walk the segments to a start-of-frame marker.
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                return None;
            }
            let marker = bytes[i + 1];
            let length = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let h = u32::from(u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]));
                let w = u32::from(u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]));
                return Some(("jpeg", w, h));
            }
            i += 2 + length;
        }
    }
    None
}

/// Standard base64 (with or without padding, whitespace ignored).
pub(crate) fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let text = text
        .split_once("base64,")
        .map_or(text, |(_, rest)| rest)
        .trim();
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0_u32;
    let mut bits = 0_u32;
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Standard base64 with padding.
pub(crate) fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for sample in [&b""[..], b"a", b"ab", b"abc", b"\x00\xff\x10hello"] {
            assert_eq!(base64_decode(&base64_encode(sample)).unwrap(), sample);
        }
    }

    #[test]
    fn a_png_header_gives_its_size() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&7_u32.to_be_bytes());
        png.extend_from_slice(&3_u32.to_be_bytes());
        assert_eq!(sniff_image(&png), Some(("png", 7, 3)));
    }

    #[test]
    fn style_names_resolve_as_word_users_write_them() {
        let spec = DocumentSpec::default();
        let writer = Writer::new(&spec, &[]);
        assert_eq!(writer.style_id("Heading 1").unwrap(), "Heading1");
        assert_eq!(writer.style_id("heading1").unwrap(), "Heading1");
        assert_eq!(writer.style_id("List Bullet").unwrap(), "ListBullet");
        assert!(writer.style_id("Nope").is_err());
    }
}
