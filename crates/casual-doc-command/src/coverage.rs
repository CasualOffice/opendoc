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
const BLOCKS: &str = "Whole-block insertion and deletion need a block anchor kind (docs/181 §4.2); rewrite_block edits one block in place.";
const OBJECTS: &str = "Grouping, moving an inline object, inserting into a group and text-box body properties are arrangement gestures not offered to agents yet; pictures and their formatting are (docs/181 §12, lane families-b).";
const DOCUMENT: &str = "Document-wide settings are not offered to agents; a person changes them.";
const SECTIONS: &str = "Mirrored margins are a document-wide setting a person chooses; section page setup is offered (set_page_setup).";
const DEFINITIONS: &str = "Style definitions are written only as part of a command that needs them; no agent command defines a style yet.";
const REFERENCES: &str = "Removing a field, and the paragraph-spanning fields a table of contents is, are not offered yet: the contents lowering reads the paginated layout and is still inside the editor (docs/181 §12, lane families-b).";
const NOTES: &str =
    "Removing a note or a header/footer body is not offered yet; inserting them is.";
const PROTECTION: &str =
    "Protection and tracking settings are a person's decision, never an agent's.";
const INLINE: &str = "Replacing a paragraph's whole inline list is a structural rewrite no agent command needs; rewrite_block diffs instead (docs/181 §4.3).";

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
    SetInlines => NotExposed(INLINE),
    SetParagraphProperties => Agent,
    InsertRow => Agent,
    DeleteRow => Agent,
    InsertColumn => Agent,
    DeleteColumn => Agent,
    DeleteTable => Agent,
    InsertTable => Agent,
    InsertBlocks => NotExposed(BLOCKS),
    DeleteBlocks => NotExposed(BLOCKS),
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
    SetStyleDefinition => NotExposed(DEFINITIONS),
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
    InsertFieldRange => NotExposed(REFERENCES),
    RemoveFieldRange => NotExposed(REFERENCES),
    InsertNote => Agent,
    RemoveNote => NotExposed(NOTES),
    CreateHeaderFooterBody => Agent,
    RemoveHeaderFooterBody => NotExposed(NOTES),
    SetSectionRunningRef => Agent,
    SetSectionTitlePage => Agent,
    SetSectionWatermark => Agent,
    SetSectionLineNumbering => Agent,
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
pub const NOT_EXPOSED_CEILING: usize = 21;

/// The reach of the variant named `name`, if it is one. O(variants).
#[must_use]
pub fn reach(name: &str) -> Option<Reach> {
    OPERATIONS
        .iter()
        .find(|(variant, _)| *variant == name)
        .map(|(_, reach)| *reach)
}
