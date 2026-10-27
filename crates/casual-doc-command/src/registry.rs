// SPDX-License-Identifier: Apache-2.0

//! Every agent-facing command, declared once (`docs/181` §2.1).
//!
//! Adding a command is adding one entry to [`COMMANDS`]. From it, the edit
//! vocabulary `casual-doc-ai` accepts, the `op` enumeration `opendoc-mcp`
//! publishes and the coverage of [`crate::coverage`] all follow. The operations
//! an entry declares in `emits` are enforced at the AI layer's choke point: a
//! lowering that emits anything else is refused there, so the declaration
//! cannot quietly drift from the code.

/// The toolset a command belongs to (`docs/181` §5.2).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Family {
    /// Inserting, replacing, deleting and formatting text.
    Text,
    /// Paragraph structure and paragraph properties.
    Paragraph,
    /// Applying styles.
    Styles,
    /// Table content and structure.
    Tables,
    /// Bulleted and numbered lists.
    Lists,
    /// Comments, tracked changes and their decisions.
    Review,
}

impl Family {
    /// Its stable, snake_case name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Paragraph => "paragraph",
            Self::Styles => "styles",
            Self::Tables => "tables",
            Self::Lists => "lists",
            Self::Review => "review",
        }
    }
}

/// Whether a command can be a tracked suggestion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Trackable {
    /// Lowered through `casual_doc_review::lower` in suggest mode.
    Yes,
    /// Refused in suggest mode with this reason, which is published with the
    /// command so an agent knows before it tries.
    No(&'static str),
    /// Not an edit of document content (a comment, a review decision), so the
    /// suggest/direct distinction does not apply.
    NotApplicable,
}

/// A suggestion is a review body, and the review lowering
/// (`casual_doc_review::lower`) builds one from these four operations. Every
/// trackable command and every review command may therefore emit them.
pub const TRACKED_LOWERING: [&str; 4] = [
    "UpdateReviewState",
    "SplitParagraph",
    "JoinParagraphs",
    "SetParagraphProperties",
];

/// One command.
#[derive(Clone, Copy, Debug)]
pub struct Command {
    /// The stable `op` name. Never reused; renaming one is a breaking change.
    pub id: &'static str,
    /// Its toolset.
    pub family: Family,
    /// One literal sentence for a model.
    pub summary: &'static str,
    /// Every `Operation` variant its lowering may emit, in either mode.
    pub emits: &'static [&'static str],
    /// Suggest-mode behaviour.
    pub trackable: Trackable,
    /// A worked example: a complete, valid edit (anchors use placeholder ids).
    pub example: &'static str,
    /// The editor command ids (`webapp/src/host_contract.mjs`) this command is
    /// the agent-side twin of; held to the contract in both directions by
    /// `webapp/tests/command_registry_parity.test.mjs` (`docs/181` G3).
    pub ui_commands: &'static [&'static str],
}

const ROWS_NOT_TRACKABLE: &str = "Table rows cannot be suggested yet: tracked table structure is \
     not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.";

const TABLE_NOT_TRACKABLE: &str = "Table structure and cell formatting cannot be suggested yet: \
     tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile \
     that grants `edit`.";

const CLEAR_NOT_TRACKABLE: &str = "Clearing formatting cannot be suggested yet: a suggested format \
     change is recorded as the properties it sets, and clearing sets none. Use set_text_format \
     with explicit values, or mode `direct`.";

const NUMBERING_NOT_TRACKABLE: &str = "Restarting or continuing numbering cannot be suggested: it \
     moves items to another numbering instance, which Word has no tracked-change record for. \
     Use mode `direct` under a profile that grants `edit`.";

/// The paragraph text every command's `example` quotes, as paragraph
/// `EXAMPLE_IDS[0]`. The suggest round-trip guard (`docs/181` G5) builds a
/// document from these and runs every trackable example against it, so an
/// example is a working edit, not an illustration.
pub const EXAMPLE_PARAGRAPH: &str = "Notice: the notice period is thirty (30) days without notice.";

/// The placeholder ids examples use: a paragraph holding
/// [`EXAMPLE_PARAGRAPH`], a numbered list item after it, and a 2x2 table.
pub const EXAMPLE_IDS: [&str; 3] = ["1:2", "1:3", "1:9"];

/// Every command, in the fixed order agents see them.
pub const COMMANDS: &[Command] = &[
    Command {
        id: "insert_text",
        family: Family::Text,
        summary: "Insert text at an anchor.",
        emits: &[
            "InsertText",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
            "SetParagraphProperties",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"insert_text","at":{"node":"1:2","exact":"days","side":"after"},"text":" (calendar)"}"#,
        ui_commands: &[],
    },
    Command {
        id: "replace_text",
        family: Family::Text,
        summary: "Replace the quoted text with new text.",
        emits: &[
            "DeleteText",
            "InsertText",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
            "SetParagraphProperties",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"replace_text","target":{"node":"1:2","exact":"thirty (30) days"},"text":"sixty (60) days"}"#,
        ui_commands: &[],
    },
    Command {
        id: "delete_text",
        family: Family::Text,
        summary: "Delete the quoted text.",
        emits: &[
            "DeleteText",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
            "SetParagraphProperties",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"delete_text","target":{"node":"1:2","exact":" without notice"}}"#,
        ui_commands: &[],
    },
    Command {
        id: "insert_paragraphs",
        family: Family::Paragraph,
        summary: "Insert new paragraphs after an anchor's paragraph.",
        emits: &[
            "SplitParagraph",
            "SetParagraphProperties",
            "InsertText",
            "UpdateReviewState",
            "JoinParagraphs",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"insert_paragraphs","after":{"node":"1:2"},"paragraphs":[{"text":"Termination","style":"Heading 2"}]}"#,
        ui_commands: &[],
    },
    Command {
        id: "apply_paragraph_style",
        family: Family::Styles,
        summary: "Apply a paragraph style, by name or id.",
        emits: &[
            "SetParagraphProperties",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"apply_paragraph_style","target":{"node":"1:2"},"style":"Heading 1"}"#,
        ui_commands: &[
            "paragraph.heading.1",
            "paragraph.heading.2",
            "paragraph.heading.3",
            "paragraph.normal",
        ],
    },
    Command {
        id: "set_text_format",
        family: Family::Text,
        summary: "Set bold, italic, underline, strike, size, colour, highlight or font on the quoted text.",
        emits: &[
            "FormatText",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
            "SetParagraphProperties",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"set_text_format","target":{"node":"1:2","exact":"Notice"},"format":{"bold":true}}"#,
        ui_commands: &[
            "format.bold",
            "format.italic",
            "format.underline",
            "format.strike",
            "format.color",
            "format.highlight",
            "format.size",
            "format.family",
        ],
    },
    Command {
        id: "clear_formatting",
        family: Family::Text,
        summary: "Remove direct character formatting from the quoted text; the paragraph style stays.",
        emits: &["ClearFormatting"],
        trackable: Trackable::No(CLEAR_NOT_TRACKABLE),
        example: r#"{"op":"clear_formatting","target":{"node":"1:2","exact":"notice period"}}"#,
        ui_commands: &["format.clear"],
    },
    Command {
        id: "rewrite_block",
        family: Family::Paragraph,
        summary: "Rewrite one paragraph from read_blocks Markdown; only the words that differ change.",
        emits: &[
            "DeleteText",
            "InsertText",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
            "SetParagraphProperties",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"rewrite_block","target":{"node":"1:2"},"markdown":"Notice: the notice period is sixty (60) days without notice."}"#,
        ui_commands: &[],
    },
    Command {
        id: "set_alignment",
        family: Family::Paragraph,
        summary: "Align a paragraph: start, center, end or justify.",
        emits: &[
            "SetParagraphProperties",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"set_alignment","target":{"node":"1:2"},"alignment":"center"}"#,
        ui_commands: &[
            "paragraph.align.start",
            "paragraph.align.center",
            "paragraph.align.end",
            "paragraph.align.justify",
        ],
    },
    Command {
        id: "set_line_spacing",
        family: Family::Paragraph,
        summary: "Set a paragraph's line spacing: a multiple of single, or an exact or minimum height in twips.",
        emits: &[
            "SetParagraphProperties",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"set_line_spacing","target":{"node":"1:2"},"rule":"multiple","value":150}"#,
        ui_commands: &[
            "paragraph.spacing.100",
            "paragraph.spacing.150",
            "paragraph.spacing.200",
        ],
    },
    Command {
        id: "set_paragraph_spacing",
        family: Family::Paragraph,
        summary: "Set the space before and/or after a paragraph, in twips.",
        emits: &[
            "SetParagraphProperties",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"set_paragraph_spacing","target":{"node":"1:2"},"before":240,"after":120}"#,
        ui_commands: &[],
    },
    Command {
        id: "set_indent",
        family: Family::Paragraph,
        summary: "Set a paragraph's start, end or first-line indent, in twips (a negative first line hangs).",
        emits: &[
            "SetParagraphProperties",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"set_indent","target":{"node":"1:2"},"start":720,"first_line":-360}"#,
        ui_commands: &["paragraph.indent.increase", "paragraph.indent.decrease"],
    },
    Command {
        id: "set_cell_text",
        family: Family::Tables,
        summary: "Set the text of one table cell.",
        emits: &[
            "InsertText",
            "DeleteText",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
            "SetParagraphProperties",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"set_cell_text","table":"1:9","row":1,"column":0,"text":"Q3"}"#,
        ui_commands: &[],
    },
    Command {
        id: "table_insert_row",
        family: Family::Tables,
        summary: "Insert an empty row at an index; a vertical merge it lands inside grows to cover it.",
        emits: &["InsertRow", "ReplaceTable"],
        trackable: Trackable::No(ROWS_NOT_TRACKABLE),
        example: r#"{"op":"table_insert_row","table":"1:9","index":1}"#,
        ui_commands: &["table.insert.rowAbove", "table.insert.rowBelow"],
    },
    Command {
        id: "table_delete_row",
        family: Family::Tables,
        summary: "Delete one row; a merged cell anchored in it keeps its content on the next row.",
        emits: &["DeleteRow", "ReplaceTable"],
        trackable: Trackable::No(ROWS_NOT_TRACKABLE),
        example: r#"{"op":"table_delete_row","table":"1:9","index":1}"#,
        ui_commands: &["table.delete.row"],
    },
    Command {
        id: "insert_table",
        family: Family::Tables,
        summary: "Insert an empty table (rows x columns, with a thin grid) after an anchor's top-level block.",
        emits: &["InsertTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"insert_table","after":{"node":"1:2"},"rows":2,"columns":3}"#,
        ui_commands: &["insert.table"],
    },
    Command {
        id: "table_insert_column",
        family: Family::Tables,
        summary: "Insert an empty column at a grid index; a merged cell it lands inside grows to cover it.",
        emits: &["InsertColumn", "ReplaceTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_insert_column","table":"1:9","index":1}"#,
        ui_commands: &["table.insert.columnLeft", "table.insert.columnRight"],
    },
    Command {
        id: "table_delete_column",
        family: Family::Tables,
        summary: "Delete one grid column; a merged cell crossing it shrinks.",
        emits: &["DeleteColumn", "ReplaceTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_delete_column","table":"1:9","index":1}"#,
        ui_commands: &["table.delete.column"],
    },
    Command {
        id: "table_merge_cells",
        family: Family::Tables,
        summary: "Merge the rectangle between two cells into one; every cell's text moves into it in reading order.",
        emits: &["ReplaceTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_merge_cells","table":"1:9","from":{"row":0,"column":0},"to":{"row":0,"column":1}}"#,
        ui_commands: &["table.merge"],
    },
    Command {
        id: "table_split_cell",
        family: Family::Tables,
        summary: "Unmerge the merged cell a cell belongs to; its text stays in the top-left cell.",
        emits: &["ReplaceTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_split_cell","table":"1:9","cell":{"row":0,"column":0}}"#,
        ui_commands: &["table.unmerge"],
    },
    Command {
        id: "table_delete",
        family: Family::Tables,
        summary: "Delete a whole table.",
        emits: &["DeleteTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_delete","table":"1:9"}"#,
        ui_commands: &["table.delete.table"],
    },
    Command {
        id: "table_set_cell_format",
        family: Family::Tables,
        summary: "Set the shading and/or vertical alignment of one cell or a rectangle of cells.",
        emits: &["SetTableCellProperties"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r##"{"op":"table_set_cell_format","table":"1:9","from":{"row":0,"column":0},"to":{"row":0,"column":1},"shading":"#FFF2CC","vertical_alignment":"center"}"##,
        ui_commands: &[],
    },
    Command {
        id: "set_list",
        family: Family::Lists,
        summary: "Make a paragraph a bulleted or numbered list item, or take it out of its list.",
        emits: &[
            "SetAbstractNumbering",
            "SetNumberingInstance",
            "SetParagraphProperties",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"set_list","target":{"node":"1:2"},"kind":"bullet"}"#,
        ui_commands: &["paragraph.list.bullet", "paragraph.list.numbered"],
    },
    Command {
        id: "change_list_level",
        family: Family::Lists,
        summary: "Move a list item down (positive) or up (negative) a number of levels.",
        emits: &[
            "SetParagraphProperties",
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
        ],
        trackable: Trackable::Yes,
        example: r#"{"op":"change_list_level","target":{"node":"1:3"},"delta":1}"#,
        ui_commands: &[],
    },
    Command {
        id: "restart_numbering",
        family: Family::Lists,
        summary: "Restart numbering at a numbered list item, at 1 or a given value.",
        emits: &["SetNumberingInstance", "SetParagraphProperties"],
        trackable: Trackable::No(NUMBERING_NOT_TRACKABLE),
        example: r#"{"op":"restart_numbering","target":{"node":"1:3"},"value":1}"#,
        ui_commands: &["paragraph.list.restart"],
    },
    Command {
        id: "continue_numbering",
        family: Family::Lists,
        summary: "Continue numbering from the nearest earlier numbered list at the item's level.",
        emits: &["SetParagraphProperties"],
        trackable: Trackable::No(NUMBERING_NOT_TRACKABLE),
        example: r#"{"op":"continue_numbering","target":{"node":"1:3"}}"#,
        ui_commands: &["paragraph.list.continue"],
    },
    Command {
        id: "comment",
        family: Family::Review,
        summary: "Comment on the quoted text, or reply to a comment.",
        emits: &[
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
            "SetParagraphProperties",
        ],
        trackable: Trackable::NotApplicable,
        example: r#"{"op":"comment","target":{"node":"1:2","exact":"notice period"},"text":"Aligned with the MSA, clause 9."}"#,
        ui_commands: &[],
    },
    Command {
        id: "resolve_comment",
        family: Family::Review,
        summary: "Resolve or reopen a comment thread.",
        emits: &[
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
            "SetParagraphProperties",
        ],
        trackable: Trackable::NotApplicable,
        example: r#"{"op":"resolve_comment","id":"1:40","resolved":true}"#,
        ui_commands: &[],
    },
    Command {
        id: "review",
        family: Family::Review,
        summary: "Accept or reject named tracked changes, or all of them.",
        emits: &[
            "UpdateReviewState",
            "SplitParagraph",
            "JoinParagraphs",
            "SetParagraphProperties",
        ],
        trackable: Trackable::NotApplicable,
        example: r#"{"op":"review","decision":"accept","revisions":["1:41"]}"#,
        ui_commands: &[],
    },
];

/// The command named `id`, if any. O(commands).
#[must_use]
pub fn find(id: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.id == id)
}

/// Every command id, in order. O(commands).
#[must_use]
pub fn ids() -> Vec<&'static str> {
    COMMANDS.iter().map(|command| command.id).collect()
}
