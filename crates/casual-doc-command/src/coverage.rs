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

const TABLES: &str = "Table structure beyond rows moves into the registry with the tables family (docs/181 §4.2 step 1).";
const BLOCKS: &str = "Whole-block insertion and deletion need a block anchor kind; with rewrite_block (docs/181 §4.3).";
const OBJECTS: &str = "Pictures, shapes and text boxes arrive with the images and objects family (docs/181 §4.2 step 5).";
const DOCUMENT: &str = "Document-wide settings are not offered to agents; a person changes them.";
const SECTIONS: &str = "Sections and page setup need the surfaces of docs/181 §2.2 first (step 6).";
const DEFINITIONS: &str = "Style, numbering, media and chart definitions are written only as part of a command that needs them.";
const REFERENCES: &str =
    "Bookmarks, fields and the table of contents arrive with the links and fields family (step 4).";
const NOTES: &str = "Notes and headers/footers need the surfaces of docs/181 §2.2 first (step 6).";
const PROTECTION: &str =
    "Protection and tracking settings are a person's decision, never an agent's.";
const INLINE: &str = "Inline structure an agent cannot express yet; links arrive with step 4, clear formatting with step 2.";

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
    ClearFormatting => NotExposed(INLINE),
    SetHyperlink => NotExposed(INLINE),
    SetInlines => NotExposed(INLINE),
    SetParagraphProperties => Agent,
    InsertRow => Agent,
    DeleteRow => Agent,
    InsertColumn => NotExposed(TABLES),
    DeleteColumn => NotExposed(TABLES),
    DeleteTable => NotExposed(TABLES),
    InsertTable => NotExposed(TABLES),
    InsertBlocks => NotExposed(BLOCKS),
    DeleteBlocks => NotExposed(BLOCKS),
    SetExtent => NotExposed(OBJECTS),
    SetGroupGeometry => NotExposed(OBJECTS),
    SetAnchor => NotExposed(OBJECTS),
    SetImageCrop => NotExposed(OBJECTS),
    SetObjectDescr => NotExposed(OBJECTS),
    DeleteObject => NotExposed(OBJECTS),
    InsertObjectNode => NotExposed(OBJECTS),
    InsertInlineObject => NotExposed(OBJECTS),
    RemoveInlineObject => NotExposed(OBJECTS),
    SetTableCellProperties => NotExposed(TABLES),
    SetTableProperties => NotExposed(TABLES),
    ReplaceTable => Agent,
    SetCoreProperties => NotExposed(DOCUMENT),
    UpdateReviewState => Agent,
    SetSectionGeometry => NotExposed(SECTIONS),
    SpliceSectionBoundary => NotExposed(SECTIONS),
    SetStyleDefinition => NotExposed(DEFINITIONS),
    SetAbstractNumbering => NotExposed(DEFINITIONS),
    SetNumberingInstance => NotExposed(DEFINITIONS),
    SetMediaReference => NotExposed(DEFINITIONS),
    SetChartDefinition => NotExposed(DEFINITIONS),
    SetDocumentProtection => NotExposed(PROTECTION),
    CreateBookmark => NotExposed(REFERENCES),
    DeleteBookmark => NotExposed(REFERENCES),
    RenameBookmark => NotExposed(REFERENCES),
    InsertField => NotExposed(REFERENCES),
    RemoveField => NotExposed(REFERENCES),
    InsertFieldRange => NotExposed(REFERENCES),
    RemoveFieldRange => NotExposed(REFERENCES),
    InsertNote => NotExposed(NOTES),
    RemoveNote => NotExposed(NOTES),
    CreateHeaderFooterBody => NotExposed(NOTES),
    RemoveHeaderFooterBody => NotExposed(NOTES),
    SetSectionRunningRef => NotExposed(SECTIONS),
    SetSectionTitlePage => NotExposed(SECTIONS),
    SetSectionWatermark => NotExposed(SECTIONS),
    SetSectionLineNumbering => NotExposed(SECTIONS),
    SetSectionPageNumbering => NotExposed(SECTIONS),
    SetSectionVerticalAlignment => NotExposed(SECTIONS),
    SetEvenAndOddHeaders => NotExposed(SECTIONS),
    SetShapeFill => NotExposed(OBJECTS),
    SetShapeStroke => NotExposed(OBJECTS),
    SetTextBoxBody => NotExposed(OBJECTS),
    SetObjectLocks => NotExposed(OBJECTS),
    SetDocumentBackground => NotExposed(DOCUMENT),
    SetMirroredMargins => NotExposed(SECTIONS),
    SetDefaultTabStop => NotExposed(DOCUMENT),
    RestoreDefaultTabStop => NotExposed(DOCUMENT),
    SetTrackRevisions => NotExposed(PROTECTION),
    SetSectionFormProtection => NotExposed(PROTECTION),
}

/// The most operations that may be not exposed. Lower it when a family moves
/// into the registry; raising it needs an ADR line (`docs/181` §3).
pub const NOT_EXPOSED_CEILING: usize = 57;

/// The reach of the variant named `name`, if it is one. O(variants).
#[must_use]
pub fn reach(name: &str) -> Option<Reach> {
    OPERATIONS
        .iter()
        .find(|(variant, _)| *variant == name)
        .map(|(_, reach)| *reach)
}
