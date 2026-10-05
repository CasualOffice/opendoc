// SPDX-License-Identifier: Apache-2.0

//! **Content identity**: one document reduced to a digest that is equal exactly
//! when a comparison of two states would report nothing.
//!
//! # The question this answers, and the one it replaces
//!
//! Version history needs to know whether a save has anything new in it. It used
//! to ask that of the **source-format bytes** — the SHA-256 that is also the
//! checkpoint's storage key — and that is the wrong identity, because the same
//! document serializes differently depending on which export mode produced it:
//!
//! - the import row holds the original file verbatim (`ExportMode::ExactIfUnchanged`
//!   returns `source.original_bytes`), and
//! - `source_unchanged` is `revision == 0`, a **monotonic watermark**, so after
//!   any edit at all — including one that is immediately undone — the exact mode
//!   is permanently unavailable and every later checkpoint is a
//!   `preserve_when_safe` re-export with a different byte layout.
//!
//! Two checkpoints of one unchanged document therefore had two different hashes,
//! and the duplicate suppression that was written to stop "a version with no
//! changes in it" could not see that they were the same document. This module is
//! the identity that can.
//!
//! # Why it is this crate's projection and not a second notion of sameness
//!
//! "Same content" is already defined here: it is what [`crate::job::DiffJob`]
//! finds nothing to say about. Minting a second definition would give the product
//! two answers to one question that drift apart, so the digest is folded from the
//! **same two halves the comparison is made of**:
//!
//! 1. every story's [`crate::projection::StorySlot::content_hash`] — the root of the Merkle
//!    projection the aligner compares, which is deliberately free of
//!    parse-minted [`casual_doc_model::NodeId`]s and so is equal for two
//!    documents that say the same thing however they were serialized; and
//! 2. the definition values [`crate::compare::compare_definitions`] compares,
//!    keyed the way it keys them — styles by `kind/name`, media by package part
//!    name, bookmarks by name — because those are the identities the *source
//!    format* writes rather than ones a parse minted.
//!
//! So `content_digest(a) == content_digest(b)` implies a comparison of `a` and
//! `b` reports no change, for every family that comparison characterises.
//! [`DIGESTED_DEFINITION_FIELDS`] is asserted equal to
//! [`crate::compare::DEFINITION_FIELDS`] by
//! `the_digest_covers_every_compared_definition_field`, which closes the loop
//! with the structural guard that already refuses a new `Definitions` field until
//! `compare.rs` says what happens to it: a field added to the model cannot become
//! invisible to the digest without failing a test.
//!
//! # What is deliberately NOT in it
//!
//! The revision watermark, the engine version, the actor, the export mode, the
//! byte length, [`Document::id`] and the `createdAt` of anything. None of them is
//! content: a document that says the same thing after an edit and its undo has
//! moved all of them and changed nothing a reader can see. Layout is not in it
//! either, for the reason the diff gives: where a line breaks is not a document
//! difference.
//!
//! # Complexity, and when it is paid
//!
//! [`content_digest`] is **O(blocks + text bytes + definitions)** time and
//! **O(blocks)** memory — one projection pass plus one pass over the definition
//! tables, which do not scale with body length. It is **O(document), so it is
//! computed at SAVE and never per edit**: the editing path's per-interaction work
//! stays O(1) in document size (`docs/107` §4), and nothing here is reachable
//! from a keystroke. The caller that needs it — a version capture — is already
//! paying for a whole export and a SHA-256 of the result at that moment.

use std::collections::BTreeSet;

use casual_doc_model::v1::Document;

use crate::compare::{media_index, style_index};
use crate::hash::ContentHasher;
use crate::projection::{Projector, StoryKey, ValueHashes};

/// The digest's scheme. Mixed in first, so a change to what the digest covers
/// cannot make an old digest and a new one compare equal.
///
/// A stored digest from an older scheme is not upgraded and must not be
/// reinterpreted: a caller compares digests only when both carry the same prefix,
/// which [`content_digest_hex`] writes into the string.
pub const CONTENT_IDENTITY_SCHEMA: u32 = 1;

/// The `Definitions` fields [`content_digest`] folds in, in the order it folds
/// them.
///
/// Equal to [`crate::compare::DEFINITION_FIELDS`] by construction and by test. The story-bearing
/// fields (`crate::compare::STORY_FIELDS`) are absent on purpose: their content
/// reaches the digest through the projection, which is where the comparison reads
/// them too.
pub const DIGESTED_DEFINITION_FIELDS: &[&str] = &[
    "styles",
    "charts",
    "abstractNumbering",
    "numbering",
    "sections",
    "media",
    "bookmarks",
    "fieldRanges",
    "documentDefaults",
    "latentStyles",
    "fontTable",
    "fontScheme",
    "colorScheme",
    "formatSchemeXml",
    "formatScheme",
    "themes",
    "shapeStyles",
    "shapeFillDetail",
    "settings",
    "people",
];

/// The document's content identity, as the 128-bit fold described above.
///
/// **O(blocks + text bytes + definitions)**, computed at save. See the module
/// docs for why it is not computed per edit.
#[must_use]
pub fn content_digest(document: &Document) -> u128 {
    let mut hasher = ContentHasher::new();
    hasher.write_u64(u64::from(CONTENT_IDENTITY_SCHEMA));
    hash_stories(document, &mut hasher);
    hash_definitions(document, &mut hasher);
    hasher.finish()
}

/// [`content_digest`] as the string a host stores and compares.
///
/// Prefixed with the scheme so a stored value says which definition of sameness
/// produced it, and so it can never be confused with a checkpoint's
/// `sha256-…`/`fnv1a-…` byte hash — two strings that mean different things must
/// not be shaped the same.
#[must_use]
pub fn content_digest_hex(document: &Document) -> String {
    format!(
        "cid{CONTENT_IDENTITY_SCHEMA}-{:032x}",
        content_digest(document)
    )
}

/// Folds every story's projection root in.
///
/// The key is folded alongside the hash so that moving a header's content to a
/// different section is a difference, rather than two equal multisets of hashes.
fn hash_stories(document: &Document, hasher: &mut ContentHasher) {
    let mut projector = Projector::new();
    // One slice, because this caller is not on a frame budget: the budgeted
    // driver exists for the comparison panel, and a save is already blocking on
    // an export. `usize::MAX` drains the queue in the first call.
    while !projector.step(document, usize::MAX) {}
    let (projection, _values, _visited) = projector.finish();
    hasher.write_str("stories");
    hasher.write_u64(projection.stories.len() as u64);
    for slot in &projection.stories {
        hash_story_key(&slot.key, hasher);
        hasher.write_u128(slot.content_hash);
    }
}

/// Mixes a story key in structurally — a tag plus its ordinals — rather than
/// through `Debug`, so the digest does not depend on a formatting impl.
fn hash_story_key(key: &StoryKey, hasher: &mut ContentHasher) {
    match key {
        StoryKey::Body => hasher.write_tag(0),
        StoryKey::Header(section, page) => {
            hasher.write_tag(1);
            hasher.write_u64(u64::from(*section));
            hasher.write_tag(*page);
        }
        StoryKey::Footer(section, page) => {
            hasher.write_tag(2);
            hasher.write_u64(u64::from(*section));
            hasher.write_tag(*page);
        }
        StoryKey::Footnote(ordinal) => {
            hasher.write_tag(3);
            hasher.write_u64(u64::from(*ordinal));
        }
        StoryKey::Endnote(ordinal) => {
            hasher.write_tag(4);
            hasher.write_u64(u64::from(*ordinal));
        }
        StoryKey::Comment(durable) => {
            hasher.write_tag(5);
            hasher.write_str(durable);
        }
    }
}

/// Names a field in the hash as well as hashing it, so a value moving from one
/// field to another is a difference rather than a wash. **O(1)**.
fn field(name: &str, hash: u128, hasher: &mut ContentHasher) {
    hasher.write_str(name);
    hasher.write_u128(hash);
}

/// Folds in everything outside story content, field for field, in
/// [`DIGESTED_DEFINITION_FIELDS`] order.
///
/// **O(definitions)** — styles, numbering, sections, media, bookmarks and the
/// theme parts, none of which scale with body length. Each field is named in the
/// hash as well as hashed, so a value moving from one field to another is a
/// difference.
fn hash_definitions(document: &Document, hasher: &mut ContentHasher) {
    let definitions = document.definitions();
    let mut values = ValueHashes::default();

    // Styles, media and bookmarks are keyed the way the comparison keys them:
    // by what the source format writes, not by a `NodeId` the parse minted.
    let styles = style_index(document);
    hasher.write_str("styles");
    hasher.write_u64(styles.len() as u64);
    for (key, style) in &styles {
        hasher.write_str(key);
        hasher.write_u128(values.hash_value(style));
    }

    field("charts", values.hash_value(&definitions.charts), hasher);
    field(
        "abstractNumbering",
        values.hash_value(&definitions.abstract_numbering),
        hasher,
    );
    field(
        "numbering",
        values.hash_value(&definitions.numbering),
        hasher,
    );
    field("sections", values.hash_value(&definitions.sections), hasher);

    let media = media_index(document);
    hasher.write_str("media");
    hasher.write_u64(media.len() as u64);
    for (part, reference) in &media {
        hasher.write_str(part);
        hasher.write_u128(values.hash_value(reference));
    }

    let bookmarks: BTreeSet<&str> = definitions
        .bookmarks
        .iter()
        .map(|(_, bookmark)| bookmark.name.as_str())
        .collect();
    hasher.write_str("bookmarks");
    hasher.write_u64(bookmarks.len() as u64);
    for name in &bookmarks {
        hasher.write_str(name);
    }

    field(
        "fieldRanges",
        values.hash_value(&definitions.field_ranges),
        hasher,
    );
    field(
        "documentDefaults",
        values.hash_value(&definitions.document_defaults),
        hasher,
    );
    field(
        "latentStyles",
        values.hash_value(&definitions.latent_styles),
        hasher,
    );
    field(
        "fontTable",
        values.hash_value(&definitions.font_table),
        hasher,
    );
    field(
        "fontScheme",
        values.hash_value(&definitions.font_scheme),
        hasher,
    );
    field(
        "colorScheme",
        values.hash_value(&definitions.color_scheme),
        hasher,
    );
    field(
        "formatSchemeXml",
        values.hash_value(&definitions.format_scheme_xml),
        hasher,
    );
    // Folded although the comparison skips it, and for the opposite reason: there,
    // reporting a projection of `formatSchemeXml` as well as the XML itself would
    // name one authored change twice; here a redundant hash costs one small
    // serialization and keeps this list exactly equal to `DEFINITION_FIELDS`, so
    // the coverage guard is an equality rather than an equality-with-exceptions.
    field(
        "formatScheme",
        values.hash_value(&definitions.format_scheme),
        hasher,
    );
    // The per-master theme table (`feat/slides`). It folds BEFORE `shapeStyles`
    // because this list is asserted equal to `DEFINITION_FIELDS` in order, and a
    // digest that folded the same fields in a different sequence would be a
    // different number for the same document.
    field("themes", values.hash_value(&definitions.themes), hasher);
    field(
        "shapeStyles",
        values.hash_value(&definitions.shape_styles),
        hasher,
    );
    field(
        "shapeFillDetail",
        values.hash_value(&definitions.shape_fill_detail),
        hasher,
    );
    field("settings", values.hash_value(&definitions.settings), hasher);
    field("people", values.hash_value(&definitions.people), hasher);

    // Document metadata and the page background, which sit on `Document` rather
    // than in `Definitions` and which the comparison compares in the same pass.
    field(
        "properties",
        values.hash_value(&document.properties()),
        hasher,
    );
    field(
        "background",
        values.hash_value(&document.background()),
        hasher,
    );
}

/// The digest's own guards live beside it so the fold and its proof move
/// together.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare::DEFINITION_FIELDS;

    /// A field added to `Definitions` must not become invisible to the digest.
    ///
    /// This is the half of the loop this module owns; the other half is
    /// `every_definitions_field_is_either_compared_or_a_story` in
    /// `crate::tests`, which reads the model's own source and refuses a field
    /// that is in neither `DEFINITION_FIELDS` nor `STORY_FIELDS`. Together they
    /// say: every field of `Definitions` is either projected as a story or folded
    /// into the content digest.
    #[test]
    fn the_digest_covers_every_compared_definition_field() {
        assert_eq!(
            DIGESTED_DEFINITION_FIELDS, DEFINITION_FIELDS,
            "the content digest folds exactly the definition fields the comparison compares — \
             add the new field to `hash_definitions` and to `DIGESTED_DEFINITION_FIELDS`, in the \
             same order"
        );
    }
}
