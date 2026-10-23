// SPDX-License-Identifier: Apache-2.0

//! The save-time projection that adds the active author's `people.xml` entry (`docs/175` §6.2).
//!
//! # Why this is not a document edit, and lives outside `lib.rs`
//!
//! `every_document_mutation_is_a_transaction` forbids the facade a mutable borrow of the
//! definitions, because a definition installed into the LIVE document outside the transaction
//! envelope is one undo cannot remove and no other replica learns of. This function never sees
//! the live document: it takes a copy, adds one `w15:person`, and hands the copy to the writer,
//! which drops it after the save. The live document, its log and every replica are untouched,
//! and saving twice adds the entry twice to two copies, never once to the document.
//!
//! It is a projection for the same reason `w:author` on a comment is not: a person's account id
//! is the host's to attach to a file that leaves the editor, and only when the host opted in.
//!
//! O(document) for the copy, at save time only.

use casual_doc_model::v1::{Document, Person};

/// A copy of `document` whose `people.xml` table also names `person`.
#[must_use]
pub(crate) fn with_person(document: &Document, person: Person) -> Document {
    let mut copy = document.clone();
    copy.definitions_mut().people.push(person);
    copy
}
