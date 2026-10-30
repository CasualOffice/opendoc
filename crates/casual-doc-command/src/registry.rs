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
    /// Hyperlinks and bookmarks.
    Links,
    /// Fields (page numbers, dates, document properties).
    Fields,
    /// Pictures, shapes and text boxes as objects.
    Objects,
    /// Footnotes and endnotes.
    Notes,
    /// Breaks, sections, page setup, line numbering and the watermark.
    Sections,
    /// Headers, footers and their page variants.
    HeadersFooters,
    /// Charts.
    Charts,
    /// Tables of contents, captions and cross-references.
    References,
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
            Self::Links => "links",
            Self::Fields => "fields",
            Self::Objects => "objects",
            Self::Notes => "notes",
            Self::Sections => "sections",
            Self::HeadersFooters => "headers_footers",
            Self::Charts => "charts",
            Self::References => "references",
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

const INLINE_NOT_TRACKABLE: &str = "Links, bookmarks, fields, pictures, notes and breaks cannot \
     be suggested yet: the review lowering tracks text, paragraph marks and properties, not the \
     insertion or removal of inline structure. Use mode `direct` under a profile that grants \
     `edit`.";

const OBJECT_NOT_TRACKABLE: &str = "Object size, wrap, crop, alt text, fill, outline and deletion \
     cannot be suggested: no tracked-change record for them is modelled. Use mode `direct` under \
     a profile that grants `edit`.";

const REFERENCES_NOT_TRACKABLE: &str = "Tables of contents, captions and cross-references \
     cannot be suggested yet: they insert fields and whole paragraphs and rewrite field results, \
     which the review lowering does not track. Use mode `direct` under a profile that grants \
     `edit`.";

const SECTION_NOT_TRACKABLE: &str = "Page setup, section breaks, headers, footers and the \
     watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not \
     modelled. Use mode `direct` under a profile that grants `edit`.";

/// The paragraph text every command's `example` quotes, as paragraph
/// `EXAMPLE_IDS[0]`. The suggest round-trip guard (`docs/181` G5) builds a
/// document from these and runs every trackable example against it, so an
/// example is a working edit, not an illustration.
pub const EXAMPLE_PARAGRAPH: &str = "Notice: the notice period is thirty (30) days without notice.";

/// The placeholder ids examples use: a paragraph holding
/// [`EXAMPLE_PARAGRAPH`], a numbered list item after it, a 2x2 table, and a
/// picture in the list item. The example fixture also holds a bookmark named
/// [`EXAMPLE_BOOKMARK`] and a hyperlink over the list item's word `First`.
pub const EXAMPLE_IDS: [&str; 4] = ["1:2", "1:3", "1:9", "1:20"];

/// The bookmark the example fixture holds, which the bookmark examples name.
pub const EXAMPLE_BOOKMARK: &str = "Notice";

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
        ui_commands: &[
            "edit.pasteText",
            "insert.symbol",
            "insert.emoji",
            "insert.nonBreakingSpace",
        ],
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
        // `insert.drawTable` is Draw Table's pencil: its outline inserts a table,
        // which is this command; an agent splits cells with the row and column
        // inserts below, which the pencil's lines are the pointer form of.
        ui_commands: &["insert.table", "insert.tableDialog", "insert.drawTable"],
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
        summary: "Set the shading, vertical alignment and/or text direction of one cell or a rectangle of cells.",
        emits: &["SetTableCellProperties"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r##"{"op":"table_set_cell_format","table":"1:9","from":{"row":0,"column":0},"to":{"row":0,"column":1},"shading":"#FFF2CC","vertical_alignment":"center","text_direction":"rotate_up"}"##,
        ui_commands: &[
            "table.textDirection.horizontal",
            "table.textDirection.rotateDown",
            "table.textDirection.rotateUp",
        ],
    },
    Command {
        id: "table_set_style_options",
        family: Family::Tables,
        summary: "Turn table style options on or off: header row, first column, total row, last column, banded rows, banded columns.",
        emits: &["SetTableProperties"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_set_style_options","table":"1:9","header_row":true,"banded_rows":false}"#,
        ui_commands: &[
            "table.look.headerRow",
            "table.look.firstColumn",
            "table.look.totalRow",
            "table.look.lastColumn",
            "table.look.bandedRows",
            "table.look.bandedColumns",
        ],
    },
    Command {
        id: "table_split",
        family: Family::Tables,
        summary: "Split a body table so a row starts a second table, with an empty paragraph between; at row 0 the paragraph goes above the table.",
        emits: &["ReplaceTable", "InsertBlocks"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_split","table":"1:9","row":1}"#,
        ui_commands: &["table.splitTable"],
    },
    Command {
        id: "table_insert_cells",
        family: Family::Tables,
        summary: "Insert empty cells at a cell or rectangle, shifting the cells there right or down (refused in a table with merged cells).",
        emits: &["ReplaceTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_insert_cells","table":"1:9","from":{"row":0,"column":1},"shift":"right"}"#,
        ui_commands: &["table.insertCells"],
    },
    Command {
        id: "table_delete_cells",
        family: Family::Tables,
        summary: "Delete a cell or rectangle of cells, closing the gap left or up (refused in a table with merged cells).",
        emits: &["ReplaceTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_delete_cells","table":"1:9","from":{"row":1,"column":0},"to":{"row":1,"column":1},"shift":"up"}"#,
        ui_commands: &["table.deleteCells"],
    },
    Command {
        id: "table_to_text",
        family: Family::Tables,
        summary: "Convert a body table to text: one paragraph per row with its cells joined by a separator, or every cell's paragraphs in order.",
        emits: &["InsertBlocks", "DeleteTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"table_to_text","table":"1:9","separator":"tab"}"#,
        ui_commands: &["table.convertToText"],
    },
    Command {
        id: "text_to_table",
        family: Family::Tables,
        summary: "Convert consecutive paragraphs into a table, cutting each paragraph into cells at a separator (tab, comma, semicolon, a character) or one cell per paragraph.",
        emits: &["DeleteBlocks", "InsertTable"],
        trackable: Trackable::No(TABLE_NOT_TRACKABLE),
        example: r#"{"op":"text_to_table","from":{"node":"1:2"},"separator":"comma"}"#,
        ui_commands: &["insert.textToTable"],
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
        id: "set_hyperlink",
        family: Family::Links,
        summary: "Link the quoted text to an http(s), mailto, ftp or tel URL, or to a bookmark as `#name`.",
        emits: &["SetHyperlink"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"set_hyperlink","target":{"node":"1:2","exact":"notice period"},"url":"https://example.com/terms"}"#,
        ui_commands: &["insert.link"],
    },
    Command {
        id: "remove_hyperlink",
        family: Family::Links,
        summary: "Remove the hyperlink whose text is exactly the quote; the text and its formatting stay.",
        emits: &["SetHyperlink"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"remove_hyperlink","target":{"node":"1:3","exact":"First"}}"#,
        ui_commands: &[],
    },
    Command {
        id: "create_bookmark",
        family: Family::Links,
        summary: "Bookmark the quoted text (a letter, then letters, digits or _, at most 40).",
        emits: &["CreateBookmark"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"create_bookmark","target":{"node":"1:2","exact":"thirty (30) days"},"name":"NoticePeriod"}"#,
        ui_commands: &["insert.bookmark"],
    },
    Command {
        id: "rename_bookmark",
        family: Family::Links,
        summary: "Rename a bookmark.",
        emits: &["RenameBookmark"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"rename_bookmark","name":"Notice","new_name":"NoticeClause"}"#,
        ui_commands: &[],
    },
    Command {
        id: "delete_bookmark",
        family: Family::Links,
        summary: "Delete a bookmark; the text it spans stays.",
        emits: &["DeleteBookmark"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"delete_bookmark","name":"Notice"}"#,
        ui_commands: &[],
    },
    Command {
        id: "insert_field",
        family: Family::Fields,
        summary: "Insert a page-number, page-count, date, time, file-name or author field at an anchor.",
        emits: &["InsertField"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"insert_field","at":{"node":"1:2","side":"after"},"kind":"page"}"#,
        ui_commands: &["insert.field", "insert.field."],
    },
    Command {
        id: "insert_image",
        family: Family::Objects,
        summary: "Insert a picture (base64 PNG, JPEG, GIF, BMP, TIFF or WebP, at most 8 MiB) at an anchor.",
        emits: &[
            "SetMediaReference",
            "InsertInlineObject",
            "SetObjectLocks",
            "SetObjectDescr",
        ],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"insert_image","at":{"node":"1:2","side":"after"},"data":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==","content_type":"image/png","width_pt":36,"alt_text":"Company logo"}"#,
        // Picture from URL and Paste as picture end in this same insert: the
        // host downloads or paints the bytes, the agent passes them (agents never
        // fetch; `picture_url.mjs`, `paste_picture.mjs`).
        ui_commands: &["insert.image", "insert.imageFromUrl", "edit.pasteImage"],
    },
    Command {
        id: "set_alt_text",
        family: Family::Objects,
        summary: "Set or clear an object's alt text.",
        emits: &["SetObjectDescr"],
        trackable: Trackable::No(OBJECT_NOT_TRACKABLE),
        example: r#"{"op":"set_alt_text","object":"1:20","text":"Signature of the lessor"}"#,
        ui_commands: &["object.altText"],
    },
    Command {
        id: "resize_object",
        family: Family::Objects,
        summary: "Resize a picture, shape or text box, in points.",
        emits: &["SetExtent"],
        trackable: Trackable::No(OBJECT_NOT_TRACKABLE),
        example: r#"{"op":"resize_object","object":"1:20","width_pt":72,"height_pt":72}"#,
        ui_commands: &[],
    },
    Command {
        id: "set_wrap",
        family: Family::Objects,
        summary: "Set how text wraps around a floating object.",
        emits: &["SetAnchor"],
        trackable: Trackable::No(OBJECT_NOT_TRACKABLE),
        example: r#"{"op":"set_wrap","object":"1:20","wrap":"square"}"#,
        ui_commands: &["layout.arrange.wrap", "object.wrap"],
    },
    Command {
        id: "crop_image",
        family: Family::Objects,
        summary: "Crop a picture by the fraction of each edge to hide; all zero removes the crop.",
        emits: &["SetImageCrop"],
        trackable: Trackable::No(OBJECT_NOT_TRACKABLE),
        example: r#"{"op":"crop_image","object":"1:20","left":0.1,"top":0,"right":0.1,"bottom":0}"#,
        ui_commands: &["object.crop"],
    },
    Command {
        id: "delete_object",
        family: Family::Objects,
        summary: "Delete a picture, shape, text box or group.",
        emits: &["DeleteObject"],
        trackable: Trackable::No(OBJECT_NOT_TRACKABLE),
        example: r#"{"op":"delete_object","object":"1:20"}"#,
        ui_commands: &["object.delete"],
    },
    Command {
        id: "set_shape_fill",
        family: Family::Objects,
        summary: "Set a shape's fill to a colour, or remove it.",
        emits: &["SetShapeFill"],
        trackable: Trackable::No(OBJECT_NOT_TRACKABLE),
        example: r##"{"op":"set_shape_fill","shape":"1:20","color":"#FFF2CC"}"##,
        ui_commands: &["object.fill"],
    },
    Command {
        id: "set_shape_outline",
        family: Family::Objects,
        summary: "Set a shape's or a picture's outline colour and/or weight, or remove it.",
        emits: &["SetShapeStroke"],
        trackable: Trackable::No(OBJECT_NOT_TRACKABLE),
        example: r##"{"op":"set_shape_outline","shape":"1:20","color":"#1F3864","width_pt":1.5}"##,
        ui_commands: &["object.outline"],
    },
    Command {
        id: "insert_footnote",
        family: Family::Notes,
        summary: "Insert a footnote with its text; the reference mark goes at the anchor.",
        emits: &["InsertNote"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"insert_footnote","at":{"node":"1:2","exact":"days","side":"after"},"text":"Calendar days, not business days."}"#,
        ui_commands: &["insert.footnote"],
    },
    Command {
        id: "insert_endnote",
        family: Family::Notes,
        summary: "Insert an endnote with its text; the reference mark goes at the anchor.",
        emits: &["InsertNote"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"insert_endnote","at":{"node":"1:2","exact":"days","side":"after"},"text":"See the schedule of notice periods."}"#,
        ui_commands: &["insert.endnote"],
    },
    Command {
        id: "insert_break",
        family: Family::Sections,
        summary: "Insert a line, page or column break, or a blank page, at an anchor.",
        emits: &["InsertInlineObject"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"insert_break","at":{"node":"1:2","side":"after"},"kind":"page"}"#,
        ui_commands: &[
            "insert.lineBreak",
            "layout.break.page",
            "layout.break.column",
            "layout.blankPage",
        ],
    },
    Command {
        id: "insert_section_break",
        family: Family::Sections,
        summary: "Start a new section at an anchor; it inherits the page setup, headers and footers.",
        emits: &[
            "SpliceSectionBoundary",
            "SetParagraphProperties",
            "SplitParagraph",
            "SetSectionFormProtection",
        ],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"insert_section_break","at":{"node":"1:2","side":"after"},"start":"next_page"}"#,
        ui_commands: &[
            "layout.break.section.nextPage",
            "layout.break.section.continuous",
            "layout.break.section.evenPage",
            "layout.break.section.oddPage",
        ],
    },
    Command {
        id: "set_page_setup",
        family: Family::Sections,
        summary: "Change a section's (or every section's) page size, orientation, margins or columns, in twips.",
        emits: &["SetSectionGeometry"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"set_page_setup","section":0,"orientation":"landscape","margins":{"left":1080,"right":1080}}"#,
        ui_commands: &[
            "layout.pageSetup",
            "layout.margins",
            "layout.orientation",
            "layout.size",
            "layout.columns",
        ],
    },
    Command {
        id: "set_line_numbering",
        family: Family::Sections,
        summary: "Number a section's lines in the margin every N lines, or turn it off with 0.",
        emits: &["SetSectionLineNumbering"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"set_line_numbering","section":0,"count_by":1,"restart":"each_page"}"#,
        ui_commands: &["layout.lineNumbers"],
    },
    Command {
        id: "set_page_numbering",
        family: Family::Sections,
        summary: "Set a section's page-number format, and restart it at a number or continue.",
        emits: &["SetSectionPageNumbering", "SetSectionVerticalAlignment"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"set_page_numbering","section":0,"format":"lower_roman","start":1}"#,
        ui_commands: &[],
    },
    Command {
        id: "set_watermark",
        family: Family::Sections,
        summary: "Stamp a text watermark on every page, or remove it.",
        emits: &["SetSectionWatermark"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"set_watermark","text":"DRAFT"}"#,
        ui_commands: &["layout.watermark"],
    },
    Command {
        id: "insert_header",
        family: Family::HeadersFooters,
        summary: "Add an empty header to a section; read_blocks on its surface gives the paragraph to write into.",
        emits: &["CreateHeaderFooterBody", "SetSectionRunningRef"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"insert_header","section":0}"#,
        ui_commands: &["insert.header"],
    },
    Command {
        id: "insert_footer",
        family: Family::HeadersFooters,
        summary: "Add an empty footer to a section; read_blocks on its surface gives the paragraph to write into.",
        emits: &["CreateHeaderFooterBody", "SetSectionRunningRef"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"insert_footer","section":0}"#,
        ui_commands: &["insert.footer"],
    },
    Command {
        id: "set_different_first_page",
        family: Family::HeadersFooters,
        summary: "Give a section's first page its own header and footer, or not.",
        emits: &["SetSectionTitlePage"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"set_different_first_page","section":0,"enabled":true}"#,
        ui_commands: &["layout.firstPageVariant"],
    },
    Command {
        id: "set_different_odd_even",
        family: Family::HeadersFooters,
        summary: "Give even and odd pages their own headers and footers, or not (document-wide).",
        emits: &["SetEvenAndOddHeaders"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"set_different_odd_even","enabled":true}"#,
        ui_commands: &["layout.evenOddVariant"],
    },
    Command {
        id: "link_to_previous",
        family: Family::HeadersFooters,
        summary: "Link a section's header or footer to the previous section's, or give it its own copy (page number, pictures and all) to change on its own.",
        emits: &["CreateHeaderFooterBody", "SetSectionRunningRef"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"link_to_previous","section":1,"band":"header","linked":false}"#,
        ui_commands: &[
            "layout.header.linkToPrevious",
            "layout.footer.linkToPrevious",
        ],
    },
    Command {
        id: "remove_header_footer",
        family: Family::HeadersFooters,
        summary: "Remove the header or footer a section shows, from that section and the sections linked to it, never from an earlier one.",
        emits: &["CreateHeaderFooterBody", "SetSectionRunningRef"],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"remove_header_footer","section":0,"band":"footer"}"#,
        ui_commands: &["layout.header.remove", "layout.footer.remove"],
    },
    Command {
        id: "insert_page_number",
        family: Family::HeadersFooters,
        summary: "Put a page number, or \"Page X of Y\", in a section's header or footer, left, centred or right; the band is created when there is none, and what it holds is kept.",
        emits: &[
            "CreateHeaderFooterBody",
            "SetSectionRunningRef",
            "SplitParagraph",
            "SetParagraphProperties",
            "SetInlines",
        ],
        trackable: Trackable::No(SECTION_NOT_TRACKABLE),
        example: r#"{"op":"insert_page_number","section":0,"band":"footer","side":"center"}"#,
        ui_commands: &[
            "layout.pageNumber.topLeft",
            "layout.pageNumber.topCenter",
            "layout.pageNumber.topRight",
            "layout.pageNumber.bottomLeft",
            "layout.pageNumber.bottomCenter",
            "layout.pageNumber.bottomRight",
            "layout.pageNumber.pageOfTotal",
        ],
    },
    Command {
        id: "insert_chart",
        family: Family::Charts,
        summary: "Insert a chart (column, bar, line, area, scatter, pie or doughnut, plain, stacked or percent) with sample data at an anchor.",
        emits: &["InsertInlineObject", "SetChartDefinition"],
        trackable: Trackable::No(INLINE_NOT_TRACKABLE),
        example: r#"{"op":"insert_chart","at":{"node":"1:2","side":"after"},"kind":"column"}"#,
        ui_commands: &["insert.chart", "insert.chart."],
    },
    Command {
        id: "insert_table_of_contents",
        family: Family::References,
        summary: "Insert a table of contents (each heading with its page) before a body paragraph.",
        emits: &["SetStyleDefinition", "InsertFieldRange"],
        trackable: Trackable::No(REFERENCES_NOT_TRACKABLE),
        example: r#"{"op":"insert_table_of_contents","before":{"node":"1:2"},"levels":2}"#,
        ui_commands: &["reference.tableOfContents"],
    },
    Command {
        id: "update_table_of_contents",
        family: Family::References,
        summary: "Update a table of contents: its page numbers only, or the entire table from the headings.",
        // `CreateBookmark`: a hyperlinked (`\h`) table plants a `_Toc` bookmark on
        // a heading added since it was built (`references` contents note).
        emits: &[
            "SetStyleDefinition",
            "CreateBookmark",
            "DeleteBlocks",
            "InsertBlocks",
            "InsertFieldRange",
        ],
        trackable: Trackable::No(REFERENCES_NOT_TRACKABLE),
        example: r#"{"op":"update_table_of_contents","mode":"entire"}"#,
        ui_commands: &[
            "reference.updateFields",
            "reference.updateToc.pageNumbers",
            "reference.updateToc.entire",
        ],
    },
    Command {
        id: "remove_table_of_contents",
        family: Family::References,
        summary: "Remove a table of contents or table of figures: its field and every entry paragraph.",
        emits: &["RemoveFieldRange", "DeleteBlocks"],
        trackable: Trackable::No(REFERENCES_NOT_TRACKABLE),
        example: r#"{"op":"remove_table_of_contents"}"#,
        ui_commands: &["reference.removeToc"],
    },
    Command {
        id: "insert_table_of_figures",
        family: Family::References,
        summary: "Insert a table of figures (every caption of a label, with its page) before a body paragraph.",
        emits: &["SetStyleDefinition", "InsertFieldRange"],
        trackable: Trackable::No(REFERENCES_NOT_TRACKABLE),
        example: r#"{"op":"insert_table_of_figures","before":{"node":"1:2"},"label":"Figure"}"#,
        ui_commands: &["reference.tableOfFigures"],
    },
    Command {
        id: "set_contents_level",
        family: Family::References,
        summary: "List body paragraphs in the table of contents at a level (1 to 9), or keep them out of it (0), without restyling them.",
        emits: &["SetParagraphProperties"],
        trackable: Trackable::No(REFERENCES_NOT_TRACKABLE),
        example: r#"{"op":"set_contents_level","paragraphs":["1:2"],"level":2}"#,
        ui_commands: &["reference.addText."],
    },
    Command {
        id: "insert_caption",
        family: Family::References,
        summary: "Insert a numbered caption (Figure, Table, Equation or the document's own label) above or below a table, picture or paragraph.",
        emits: &["SetStyleDefinition", "InsertBlocks", "SetInlines"],
        trackable: Trackable::No(REFERENCES_NOT_TRACKABLE),
        example: r#"{"op":"insert_caption","target":"1:9","label":"Table","text":": Payment schedule"}"#,
        ui_commands: &["reference.caption"],
    },
    Command {
        id: "update_caption_numbers",
        family: Family::References,
        summary: "Renumber every caption whose shown number is stale.",
        emits: &["SetInlines"],
        trackable: Trackable::No(REFERENCES_NOT_TRACKABLE),
        example: r#"{"op":"update_caption_numbers"}"#,
        ui_commands: &["reference.updateCaptionNumbers"],
    },
    Command {
        id: "insert_cross_reference",
        family: Family::References,
        summary: "Insert a cross-reference to a heading, caption, bookmarked paragraph or note: its text, label and number, page, or above/below.",
        emits: &["CreateBookmark", "InsertField", "InsertText"],
        trackable: Trackable::No(REFERENCES_NOT_TRACKABLE),
        example: r#"{"op":"insert_cross_reference","at":{"node":"1:2","side":"after"},"target":"1:3","show":"page_number"}"#,
        ui_commands: &["reference.crossReference"],
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
const UI_OBJECTS: &str = "Inserting shapes and text boxes, and arranging objects (position, order, grouping, rotation, flips, inline or floating), are not offered to agents yet; pictures, alt text, size, wrap, crop, fill, outline and deletion are.";
const UI_OBJECT_MENU: &str = "The object menus are one contract family; its members that change a picture or shape (object.altText, object.crop, object.delete, object.fill, object.outline, object.wrap) are named by the objects commands, the rest are arrangement.";
const UI_NOTE_SETTINGS: &str = "Footnote and endnote options are a person's layout choice in the Footnote and Endnote dialog; no agent command offers them yet (docs/181 §3).";
const UI_NOTE_CONVERT: &str = "Converting footnotes to endnotes is a person's editorial decision; an agent inserts the kind of note it means with insert_note.";
const UI_PAGE_BORDERS: &str = "Page borders are a person's design choice in the Page Borders dialog; no agent command offers them yet (docs/181 §3).";
const UI_HEADER_FOOTER_SETTINGS: &str = "The Header & Footer options dialog also writes the header and footer distances, which agents are not offered; its toggles and page numbering are (set_different_first_page, set_different_odd_even, set_page_numbering).";
const UI_TABLES: &str = "Table structure beyond inserting and deleting rows (columns, merge and split, move, sort, sizing, cell and table formatting, table styles) arrives with the rest of the tables family (docs/181 §4.2 step 1).";

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
    ("format.language", UI_CHARACTER),
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
    ("insert.shape", UI_OBJECTS),
    ("insert.textbox", UI_OBJECTS),
    ("layout.arrange.position", UI_OBJECTS),
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
    ("object.", UI_OBJECT_MENU),
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
    ("layout.headerFooterSettings", UI_HEADER_FOOTER_SETTINGS),
    ("layout.pageBorders", UI_PAGE_BORDERS),
    ("reference.noteSettings", UI_NOTE_SETTINGS),
    ("reference.convertNotes", UI_NOTE_CONVERT),
    ("review.mode.editing", UI_MODE),
    ("review.mode.suggesting", UI_MODE),
    ("review.restrictEditing", UI_PROTECTION),
    ("review.comment.delete", UI_COMMENT_DELETE),
];

/// The length of [`UI_NOT_EXPOSED`]. It may only fall: lower it when a command
/// names an id; raising it needs an ADR line (`docs/181` §3).
///
/// 84 -> 85 with `layout.pageBorders` (`109` HF-193), recorded in `docs/181` §3.
/// 85 -> 96 with the eleven table-tool ids (`109` RM-26, RM-27), 96 -> 99 with
/// `reference.removeToc`, `reference.tableOfFigures` and `reference.addText.`
/// (`109` RM-25), 99 -> 101 with `reference.noteSettings` and
/// `reference.convertNotes` (`109` RM-24), and 101 -> 102 with `format.language`
/// (`109` RM-42), each recorded in `docs/181` §3. 102 -> 88 when the eleven
/// table tools and the three contents ids gained agent commands
/// (`table_set_style_options`, `table_split`, `table_insert_cells`,
/// `table_delete_cells`, `table_to_text`, `text_to_table`,
/// `remove_table_of_contents`, `insert_table_of_figures`,
/// `set_contents_level`), recorded in `docs/181` §3.
pub const UI_NOT_EXPOSED_CEILING: usize = 88;
