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
            | Self::Comment { text, .. } => text.len(),
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
            | Self::ResolveComment { .. }
            | Self::Review { .. } => 0,
        }
    }
}
