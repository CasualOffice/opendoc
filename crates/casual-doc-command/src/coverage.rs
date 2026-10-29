// SPDX-License-Identifier: Apache-2.0

//! Which operations agents can reach (`docs/181` §3, guard G1).
//!
//! Every [`Operation`] variant is listed once below, either as reached by a
//! command in [`crate::COMMANDS`] or as not exposed, with the sentence a user
//! would read. The list expands into an exhaustive `match` with no wildcard
//! arm, so **a new operation does not compile until it is classified here**.
//! The tests then hold the classification to the registry in both directions:
//! a variant marked reached must be emitted by some command, and a command may
//! emit only variants marked reached.
//!
//! The not-exposed list is a ratchet (`NOT_EXPOSED_CEILING`): it may shrink as
//! families move into the registry (`docs/181` §4.2), never grow without an
//! ADR line.

use casual_doc_edit::Operation;

/// How agents stand with one operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reach {
    /// Emitted by at least one registry command.
    Agent,
    /// No command emits it yet, for this reason.
    NotExposed(&'static str),
}

const TABLES: &str = "Whole-table properties (width, borders, style, layout) are not offered to agents yet; cell formatting and structure are.";
const OBJECTS: &str = "Grouping, moving an inline object, inserting into a group and text-box body properties are arrangement gestures not offered to agents yet; pictures and their formatting are (docs/181 §12, lane families-b).";
const DOCUMENT: &str = "Document-wide settings are not offered to agents; a person changes them.";
const SECTIONS: &str = "Mirrored margins are a document-wide setting a person chooses; section page setup is offered (set_page_setup).";
const REFERENCES: &str = "Removing a field or a paragraph-spanning field (unlinking it to plain text) has no editor gesture to share yet; inserting and updating tables of contents, captions and cross-references is offered (docs/181 §12, lane references).";
const NOTES: &str =
    "Removing a note or a header/footer body is not offered yet; inserting them is.";
const PAGE_BORDERS: &str = "Page borders are a person's design choice in the Page Borders dialog; no agent command offers them yet.";
const NOTE_SETTINGS: &str = "Footnote and endnote options are a person's layout choice in the Footnote and Endnote dialog; no agent command offers them yet.";
const PROTECTION: &str =
    "Protection and tracking settings are a person's decision, never an agent's.";

macro_rules! classify {
    ($($variant:ident => $reach:expr),* $(,)?) => {
        /// Every operation variant, in declaration order, with its reach.
        pub const OPERATIONS: &[(&str, Reach)] = &[$((stringify!($variant), $reach)),*];

        /// The variant name of `operation`. Exhaustive by construction: a new
        /// variant is a compile error here until it is classified. O(1).
        #[must_use]
        pub fn operation_name(operation: &Operation) -> &'static str {
            match operation {
                $(Operation::$variant { .. } => stringify!($variant),)*
            }
        }
    };
}

use Reach::{Agent, NotExposed};

classify! {
    InsertText => Agent,
    DeleteText => Agent,
    SplitParagraph => Agent,
    JoinParagraphs => Agent,
    FormatText => Agent,
    ClearFormatting => Agent,
    SetHyperlink => Agent,
    // Emitted only by the caption commands, to rewrite a caption's cached
    // `SEQ` number in place; rewrite_block diffs text instead (docs/181 §4.3).
    SetInlines => Agent,
    SetParagraphProperties => Agent,
    InsertRow => Agent,
    DeleteRow => Agent,
    InsertColumn => Agent,
    DeleteColumn => Agent,
    DeleteTable => Agent,
    InsertTable => Agent,
    // Emitted by insert_caption (one caption paragraph) and
    // update_table_of_contents (the contents field's own entries); no command
    // inserts or deletes arbitrary blocks.
    InsertBlocks => Agent,
    DeleteBlocks => Agent,
    SetExtent => Agent,
    SetGroupGeometry => NotExposed(OBJECTS),
    SetAnchor => Agent,
    SetImageCrop => Agent,
    SetObjectDescr => Agent,
    DeleteObject => Agent,
    InsertObjectNode => NotExposed(OBJECTS),
    InsertInlineObject => Agent,
    RemoveInlineObject => NotExposed(OBJECTS),
    SetTableCellProperties => Agent,
    SetTableProperties => NotExposed(TABLES),
    ReplaceTable => Agent,
    SetCoreProperties => NotExposed(DOCUMENT),
    UpdateReviewState => Agent,
    SetSectionGeometry => Agent,
    SpliceSectionBoundary => Agent,
    // Emitted only to create the built-in `TOC n` or `Caption` style a table
    // of contents or a caption needs when the document lacks it (Word does the
    // same); no command redefines an existing style.
    SetStyleDefinition => Agent,
    SetAbstractNumbering => Agent,
    SetNumberingInstance => Agent,
    SetMediaReference => Agent,
    SetChartDefinition => Agent,
    SetDocumentProtection => NotExposed(PROTECTION),
    CreateBookmark => Agent,
    DeleteBookmark => Agent,
    RenameBookmark => Agent,
    InsertField => Agent,
    RemoveField => NotExposed(REFERENCES),
    InsertFieldRange => Agent,
    RemoveFieldRange => NotExposed(REFERENCES),
    InsertNote => Agent,
    RemoveNote => NotExposed(NOTES),
    CreateHeaderFooterBody => Agent,
    RemoveHeaderFooterBody => NotExposed(NOTES),
    SetSectionRunningRef => Agent,
    SetSectionTitlePage => Agent,
    SetSectionWatermark => Agent,
    SetSectionLineNumbering => Agent,
    SetSectionPageBorders => NotExposed(PAGE_BORDERS),
    SetNoteProperties => NotExposed(NOTE_SETTINGS),
    SetSectionPageNumbering => Agent,
    SetSectionVerticalAlignment => Agent,
    SetEvenAndOddHeaders => Agent,
    SetShapeFill => Agent,
    SetShapeStroke => Agent,
    SetTextBoxBody => NotExposed(OBJECTS),
    SetObjectLocks => Agent,
    SetDocumentBackground => NotExposed(DOCUMENT),
    SetMirroredMargins => NotExposed(SECTIONS),
    SetDefaultTabStop => NotExposed(DOCUMENT),
    RestoreDefaultTabStop => NotExposed(DOCUMENT),
    SetTrackRevisions => NotExposed(PROTECTION),
    // Emitted only by insert_section_break, which copies the split section's
    // existing `w:formProt` to the new one (Word copies the whole `w:sectPr`);
    // no command sets or lifts protection.
    SetSectionFormProtection => Agent,
}

/// The most operations that may be not exposed. Lower it when a family moves
/// into the registry; raising it needs an ADR line (`docs/181` §3).
///
/// 16 -> 17 with `SetSectionPageBorders` (`109` HF-193, #912): page borders are a
/// person's design choice in the Page Borders dialog, recorded in `docs/181` §3.
/// 17 -> 18 with `SetNoteProperties` (`docs/153` `page.note.settings`, `109` RM-24):
/// note options are a person's layout choice in the Footnote and Endnote dialog,
/// recorded in `docs/181` §3.
pub const NOT_EXPOSED_CEILING: usize = 18;

/// The reach of the variant named `name`, if it is one. O(variants).
#[must_use]
pub fn reach(name: &str) -> Option<Reach> {
    OPERATIONS
        .iter()
        .find(|(variant, _)| *variant == name)
        .map(|(_, reach)| *reach)
}
