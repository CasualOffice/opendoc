// SPDX-License-Identifier: Apache-2.0

//! `casual-doc-diff` — the structural difference between two document states
//! (`docs/140` §11, H3; `docs/139` §9, VH-008/VH-009).
//!
//! # What this is for
//!
//! Version history stores **fidelity-complete source-format checkpoints** — real
//! DOCX/ODT/RTF/TXT bytes, not normalized JSON, because `docs/112` measured that
//! JSON omits binary resources and the retained source envelope. So comparing two
//! versions means comparing two *parsed documents*, and the cost of a diff is the
//! cost of opening two documents plus the cost of this crate. This crate is the
//! second half; the facade owns the parsing.
//!
//! # The algorithm, named before it was written
//!
//! | Step | Established algorithm |
//! | --- | --- |
//! | Reduce each document to a comparable sequence | **Merkle hash tree** over the ordered container forest ([`projection`]) |
//! | Skip identical subtrees | hash-tree short-circuit, as `git diff` does with identical tree objects |
//! | Pair up sibling lists | **common prefix/suffix trim**, then **patience anchoring** (unique common keys + LIS), then **Myers' O(ND) diff** ([`align`]) |
//! | Recognise an edited block rather than a delete plus an add | a second pass of the same aligner on a weaker key: accepted outright where the correspondence is one-to-one, and on a similarity threshold where it is ragged ([`job`]) |
//! | Recognise a moved block | **hash join** on subtree hash, accepted only when unique on both sides |
//! | Diff text inside a paragraph | the same aligner over **word tokens built from grapheme clusters** ([`inline`]) |
//! | Say which property changed | **reflection over the model type's own serde field names** ([`compare`]) |
//!
//! **General tree edit distance (Zhang–Shasha and descendants) is deliberately
//! rejected**: O(n²) at best plus depth factors, and it produces a
//! relabel/reparent script that has no counterpart in what a reader of a document
//! wants to know. The hierarchy here is ordered and typed, so aligning sibling
//! lists and recursing into matched containers is both exact and near-linear.
//!
//! # How far stable `NodeId`s get you: not as far as it looks
//!
//! `NodeId` is stable and anchored (doc 45 I3), and `docs/140` §11.2 step 2 says
//! to "match stable `NodeId` identities when the states share the same retained
//! lineage". **A pair of checkpoints does not share one.** Ids are minted by
//! `IdGenerator::new(config.id_namespace)`, a counter that starts at one for every
//! import, so across two independently parsed files `NodeId` equality means "the
//! same ordinal position in the parse walk" and nothing more. Matching on it would
//! misalign every block after the first insertion — worse than not matching at
//! all, because it would do so confidently. `crates/casual-doc-diff/src/tests.rs`
//! proves this with two real imports.
//!
//! What ids *are* used for is the thing they are good for: **anchors**. Every
//! change record carries the `NodeId` of the block on each side, in the same
//! `{node, start, end}` shape the review surface already uses, so a host can
//! scroll a live document or a preview session to a change without re-deriving
//! anything.
//!
//! The identities that genuinely survive two independent parses are the ones the
//! *source format* writes, and they are used wherever they exist: a comment's
//! `w16cid:durableId`/`w14:paraId`, a style's `w:name`, a bookmark's name, a media
//! part's package path, and a header's semantic position (section ordinal plus
//! page type). A body paragraph's `w14:paraId` is **not** among them — this engine
//! reports that attribute as a located loss on save (FID-R-03), so it is not
//! available to match on, and that is recorded in `docs/140` rather than assumed
//! away.
//!
//! # Diff families
//!
//! Detected and typed: whole blocks inserted, deleted and moved; text changed
//! inside a block, at word granularity with grapheme clusters never split; paragraph and character formatting
//! with the text identical; paragraph style and list membership; table rows,
//! cells and properties; sections and page setup; styles, numbering, bookmarks,
//! fonts, themes and settings; media parts added, removed and (given digests)
//! replaced; comments; pending tracked-change markup; and document metadata.
//!
//! Deliberately **not** detected, each with an explicit finding so a reader is
//! never told less than the truth:
//!
//! | Not detected | What a reader sees |
//! | --- | --- |
//! | A move that also changed content | a deletion and an insertion, separately |
//! | A move whose content is not unique on both sides | a deletion and an insertion, plus an `ambiguous_match` finding |
//! | What changed *inside* a drawing, embedded object, OMML subtree or chart | an `object` change located at the paragraph, plus a `not_compared` finding for `inlineObject` |
//! | Which numbering definition changed | a `definition` change naming `numbering`, plus `not_compared` — numbering is keyed by parse-minted ids, so identity cannot be established |
//! | What changed inside the theme's retained format scheme | a `definition` change naming `formatSchemeXml`, plus `not_compared` |
//! | Media bytes replaced under the same part name, when the host supplies no digests | a `missing_resource` finding for `mediaBytes` |
//! | An exact alignment of a region with no unique common key and more than [`align::MAX_ALIGN_CELLS`] cells | that region wholesale, plus `ambiguous_match` |
//! | An exact text diff of a paragraph longer than [`inline::MAX_INLINE_TOKENS`] tokens | the whole text replaced, plus `truncated` |
//! | Changes past [`job::MAX_CHANGES`] | the first `MAX_CHANGES`, plus `truncated` |
//! | Layout differences — page count, where a line breaks | nothing: layout is not a document difference |
//! | Per-change authorship | nothing; a snapshot pair carries no authorship (`docs/140` §11.5) |
//!
//! [`record::VersionDiff::complete`] is false whenever any finding is present, so
//! "this is the complete document diff" cannot be claimed while a family was
//! skipped (`docs/139` §9.2, VH-009).
//!
//! # Where it runs
//!
//! Nowhere in particular, on purpose. [`job::DiffJob`] is a budgeted coroutine
//! that holds no borrow of either document: `step(sides, budget)` does a bounded
//! amount of work and returns, `cancel()` stops it, and progress is real. A host
//! on a UI thread drives it from idle time; a host with a Worker moves the whole
//! job there without changing a line of this crate. See `docs/140` §11 for why
//! the browser lane cannot put it in a Worker yet.
//!
//! # No new engine operation
//!
//! A diff is a read (ADR-030 I2). Nothing here mutates a document, and the closed
//! operation set is untouched.

pub mod align;
pub mod compare;
pub mod hash;
pub mod inline;
pub mod job;
pub mod projection;
pub mod record;

#[cfg(test)]
mod tests;

pub use compare::MediaDigests;
pub use job::{DiffJob, DiffSides, Progress};
pub use record::{
    Confidence, DIFF_SCHEMA, DiffAnchor, DiffChange, DiffFamily, DiffFinding, DiffKind,
    FindingCode, PathSegment, Story, VersionDiff,
};
