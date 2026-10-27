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
    /// `webapp/tests/command_registry_parity.test.mjs` (`docs/181` G3). An id
    /// ending in `.` names a whole value family (`style.` is every
    /// `style.<name>`): the command takes that value as an argument.
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
        ui_commands: &["edit.pasteText", "insert.symbol", "insert.emoji"],
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
        ui_commands: &["edit.replace"],
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
        ui_commands: &["edit.cut"],
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
            "style.",
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
            "format.family",
            "format.family.",
            "format.size",
            "format.size.",
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
        ui_commands: &["review.comment"],
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
        ui_commands: &["review.comment.resolve"],
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
        ui_commands: &[
            "review.acceptAtCaret",
            "review.rejectAtCaret",
            "review.acceptNext",
            "review.rejectNext",
            "review.acceptAll",
            "review.rejectAll",
        ],
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

// ---- Editor commands deliberately not agent-exposed (`docs/181` G3) --------

const UI_HISTORY: &str = "Undo and redo walk the person's own history; an agent undoes its own change with the `undo_change` tool.";
const UI_CLIPBOARD: &str = "Rich paste reads the person's clipboard; an agent inserts text with insert_text and formats it with set_text_format.";
const UI_DIRECTION: &str =
    "Paragraph direction arrives with the paragraph family (docs/181 §4.2 step 2).";
const UI_CHARACTER: &str = "Character formatting beyond set_text_format's fields arrives with the formatting unification (docs/181 §4.2 step 2).";
const UI_PAINTER: &str = "Format painter copies the caret's formatting, a gesture with no document-addressed equivalent; an agent sets formatting with set_text_format.";
const UI_CASE: &str = "Change case keeps each run's formatting, which replace_text would flatten; it arrives with rewrite_block (docs/181 §4.3).";
const UI_PARAGRAPH: &str = "Alignment, indents, spacing, tab stops and drop caps arrive with the paragraph family (docs/181 §4.2 step 2).";
const UI_LISTS: &str = "Numbering on/off, level, format and restart arrive with the lists family (docs/181 §4.2 step 3).";
const UI_STYLE_DEFINITIONS: &str = "Redefining a style changes every paragraph that uses it; agents apply styles and do not define them yet (docs/181 §4.2 step 2).";
const UI_REFERENCES: &str = "Hyperlinks, bookmarks, fields, captions, cross-references and the table of contents arrive with the links and fields family (docs/181 §4.2 step 4).";
const UI_OBJECTS: &str = "Pictures, shapes, text boxes and their arrangement arrive with the images and objects family (docs/181 §4.2 step 5).";
const UI_CHARTS: &str =
    "Charts come last, behind the chart model work they need (docs/181 §4.2 step 7, docs/155).";
const UI_TABLES: &str = "Table structure beyond inserting and deleting rows (columns, merge and split, move, sort, sizing, cell and table formatting, table styles) arrives with the rest of the tables family (docs/181 §4.2 step 1).";
const UI_NOTES: &str =
    "Headers, footers, footnotes and endnotes need the surfaces of docs/181 §2.2 first (step 6).";
const UI_SECTIONS: &str = "Page setup, columns, section breaks and line numbering need the surfaces of docs/181 §2.2 first (step 6).";
const UI_DOCUMENT: &str =
    "Document-wide settings are not offered to agents; a person changes them.";
const UI_BREAKS: &str = "Line, page and column breaks are inline structure no registry command emits yet (docs/181 §4.2).";
const UI_MODE: &str = "Switches the editor's own editing mode, not the document; an agent's mode is its profile plus the plan's `mode`.";
const UI_PROTECTION: &str =
    "Protection and tracking settings are a person's decision, never an agent's.";
const UI_COMMENT_DELETE: &str =
    "No command deletes a comment yet; an agent resolves a thread with resolve_comment.";

/// Editor command ids (`webapp/src/host_contract.mjs` `COMMAND_CONTRACT` and
/// `COMMAND_FAMILIES`) that can change the document but that no registry
/// command is the twin of, each with the sentence a user would read. An id
/// ending in `.` is a whole value family.
///
/// `webapp/tests/command_registry_parity.test.mjs` holds this list, together
/// with every command's `ui_commands`, to cover every such contract id exactly
/// once (`docs/181` G3). It is a ratchet ([`UI_NOT_EXPOSED_CEILING`]): an entry
/// leaves when a command names the id, and an addition needs an ADR line.
pub const UI_NOT_EXPOSED: &[(&str, &str)] = &[
    ("edit.undo", UI_HISTORY),
    ("edit.redo", UI_HISTORY),
    ("edit.paste", UI_CLIPBOARD),
    ("format.direction.rtl", UI_DIRECTION),
    ("format.direction.ltr", UI_DIRECTION),
    ("format.subscript", UI_CHARACTER),
    ("format.superscript", UI_CHARACTER),
    ("format.grow", UI_CHARACTER),
    ("format.shrink", UI_CHARACTER),
    ("format.underline.", UI_CHARACTER),
    ("format.character.allCaps", UI_CHARACTER),
    ("format.character.smallCaps", UI_CHARACTER),
    ("format.character.doubleStrike", UI_CHARACTER),
    ("format.character.outline", UI_CHARACTER),
    ("format.character.shadow", UI_CHARACTER),
    ("format.character.emboss", UI_CHARACTER),
    ("format.character.imprint", UI_CHARACTER),
    ("format.character.dialog", UI_CHARACTER),
    ("format.character.advanced", UI_CHARACTER),
    ("format.character.border", UI_CHARACTER),
    ("format.character.emphasis", UI_CHARACTER),
    ("format.character.spacing", UI_CHARACTER),
    ("format.character.scale", UI_CHARACTER),
    ("format.character.position", UI_CHARACTER),
    ("format.character.kerning", UI_CHARACTER),
    ("format.character.ligatures", UI_CHARACTER),
    ("format.painter", UI_PAINTER),
    ("format.case.upper", UI_CASE),
    ("format.case.lower", UI_CASE),
    ("format.case.title", UI_CASE),
    ("format.case.sentence", UI_CASE),
    ("format.case.toggle", UI_CASE),
    ("paragraph.spacing.", UI_PARAGRAPH),
    ("paragraph.space.", UI_PARAGRAPH),
    ("layout.paragraph", UI_PARAGRAPH),
    ("layout.indent", UI_PARAGRAPH),
    ("layout.spacing", UI_PARAGRAPH),
    ("layout.tabStops", UI_PARAGRAPH),
    ("insert.dropCap", UI_PARAGRAPH),
    ("paragraph.list.settings", UI_LISTS),
    ("paragraph.list.checklist", UI_LISTS),
    ("paragraph.listFormat.", UI_LISTS),
    ("style.manage", UI_STYLE_DEFINITIONS),
    ("style.updateFromSelection", UI_STYLE_DEFINITIONS),
    ("style.createFromSelection", UI_STYLE_DEFINITIONS),
    ("insert.link", UI_REFERENCES),
    ("insert.bookmark", UI_REFERENCES),
    ("insert.field", UI_REFERENCES),
    ("insert.field.", UI_REFERENCES),
    ("reference.caption", UI_REFERENCES),
    ("reference.crossReference", UI_REFERENCES),
    ("reference.tableOfContents", UI_REFERENCES),
    ("reference.updateToc.pageNumbers", UI_REFERENCES),
    ("reference.updateToc.entire", UI_REFERENCES),
    ("reference.updateFields", UI_REFERENCES),
    ("reference.updateCaptionNumbers", UI_REFERENCES),
    ("insert.image", UI_OBJECTS),
    ("insert.shape", UI_OBJECTS),
    ("insert.textbox", UI_OBJECTS),
    ("layout.arrange.position", UI_OBJECTS),
    ("layout.arrange.wrap", UI_OBJECTS),
    ("layout.arrange.bringForward", UI_OBJECTS),
    ("layout.arrange.sendBackward", UI_OBJECTS),
    ("layout.arrange.bringToFront", UI_OBJECTS),
    ("layout.arrange.sendToBack", UI_OBJECTS),
    ("layout.arrange.group", UI_OBJECTS),
    ("layout.arrange.ungroup", UI_OBJECTS),
    ("layout.arrange.rotateRight", UI_OBJECTS),
    ("layout.arrange.rotateLeft", UI_OBJECTS),
    ("layout.arrange.flipHorizontal", UI_OBJECTS),
    ("layout.arrange.flipVertical", UI_OBJECTS),
    ("layout.arrange.inLine", UI_OBJECTS),
    ("layout.arrange.addText", UI_OBJECTS),
    ("object.", UI_OBJECTS),
    ("insert.chart", UI_CHARTS),
    ("table.", UI_TABLES),
    ("table.move.rowUp", UI_TABLES),
    ("table.move.rowDown", UI_TABLES),
    ("table.move.columnLeft", UI_TABLES),
    ("table.move.columnRight", UI_TABLES),
    ("table.split", UI_TABLES),
    ("table.distribute.rows", UI_TABLES),
    ("table.distribute.columns", UI_TABLES),
    ("table.sort.ascending", UI_TABLES),
    ("table.sort.descending", UI_TABLES),
    ("table.column.grow", UI_TABLES),
    ("table.column.shrink", UI_TABLES),
    ("table.row.grow", UI_TABLES),
    ("table.row.shrink", UI_TABLES),
    ("table.cellFormat", UI_TABLES),
    ("table.borderStyle", UI_TABLES),
    ("table.properties", UI_TABLES),
    ("table.style.none", UI_TABLES),
    ("insert.header", UI_NOTES),
    ("insert.footer", UI_NOTES),
    ("insert.footnote", UI_NOTES),
    ("insert.endnote", UI_NOTES),
    ("layout.firstPageVariant", UI_NOTES),
    ("layout.evenOddVariant", UI_NOTES),
    ("layout.headerFooterSettings", UI_NOTES),
    ("layout.pageSetup", UI_SECTIONS),
    ("layout.margins", UI_SECTIONS),
    ("layout.orientation", UI_SECTIONS),
    ("layout.size", UI_SECTIONS),
    ("layout.columns", UI_SECTIONS),
    ("layout.break.section.nextPage", UI_SECTIONS),
    ("layout.break.section.continuous", UI_SECTIONS),
    ("layout.break.section.evenPage", UI_SECTIONS),
    ("layout.break.section.oddPage", UI_SECTIONS),
    ("layout.lineNumbers", UI_SECTIONS),
    ("layout.watermark", UI_DOCUMENT),
    ("insert.lineBreak", UI_BREAKS),
    ("layout.break.page", UI_BREAKS),
    ("layout.break.column", UI_BREAKS),
    ("layout.blankPage", UI_BREAKS),
    ("review.mode.editing", UI_MODE),
    ("review.mode.suggesting", UI_MODE),
    ("review.restrictEditing", UI_PROTECTION),
    ("review.comment.delete", UI_COMMENT_DELETE),
];

/// The length of [`UI_NOT_EXPOSED`]. It may only fall: lower it when a command
/// names an id; raising it needs an ADR line (`docs/181` §3).
pub const UI_NOT_EXPOSED_CEILING: usize = 119;
