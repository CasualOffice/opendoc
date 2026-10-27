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
        ui_commands: &[],
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
        ui_commands: &[],
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
        ui_commands: &[],
    },
    Command {
        id: "table_delete_row",
        family: Family::Tables,
        summary: "Delete one row; a merged cell anchored in it keeps its content on the next row.",
        emits: &["DeleteRow", "ReplaceTable"],
        trackable: Trackable::No(ROWS_NOT_TRACKABLE),
        example: r#"{"op":"table_delete_row","table":"1:9","index":1}"#,
        ui_commands: &[],
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
        example: r#"{"op":"comment","target":{"node":"1:2","exact":"termination"},"text":"Aligned with the MSA, clause 9."}"#,
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
