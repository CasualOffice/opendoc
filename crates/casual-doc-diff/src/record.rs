//! The `VersionDiff` sidecar: the typed change records, findings and counts a
//! host renders (`docs/140` §11.1, §11.3).
//!
//! # The vocabulary is the review vocabulary
//!
//! Tracked changes already exist in this engine, and the facade already reports
//! them to the interface as `insertion` / `deletion` / `move_from` / `move_to` /
//! `formatting` with an anchor of `{node, start, end}`. A version diff is a
//! different *thing* — two snapshots, no authorship — but a reader does not have
//! two vocabularies for "this sentence was added", so [`DiffKind`] is that same
//! set of strings and [`DiffAnchor`] is that same anchor shape. The one addition
//! is `property`, for a change to a definition or a page setup that review
//! markup has no inline form for.
//!
//! # No English
//!
//! Every record carries a family, a kind, and typed field names. It carries no
//! sentence. That is the same split `version_history.mjs` and `version_policy.mjs`
//! already make (ADR-040 §6: the store returns status codes, the policy module
//! owns the words, and nineteen locale catalogues own the translations). A label
//! minted here would be an untranslatable string on a user-facing surface.
//!
//! All types here are plain data. Serialization is **O(records)**.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::hash::ContentHasher;

/// The sidecar schema version. A host that does not recognise it must refuse the
/// diff rather than render a partial one.
pub const DIFF_SCHEMA: u32 = 1;

/// Which document story a change is in.
///
/// Headers, footers and notes are ordinary block content in the same id space as
/// the body (`docs/140` §11.2 "story"), so they are diffed by the same pipeline;
/// the story only says *where*.
///
/// **Every variant is named semantically, not by id.** Every definition id in
/// this model wraps a `NodeId` minted by the importer's counter, so a header's
/// id means "the nth id this parse handed out" and is meaningless across two
/// parsed checkpoints. What is stable is a header's *position* — which section,
/// which page type — and a comment's durable id, which the package carries.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Story {
    /// The main document body.
    Body,
    /// The header of one section and page type.
    Header {
        /// Zero-based section ordinal.
        section: u32,
        /// `default`, `first` or `even`.
        page: String,
    },
    /// The footer of one section and page type.
    Footer {
        /// Zero-based section ordinal.
        section: u32,
        /// `default`, `first` or `even`.
        page: String,
    },
    /// One footnote's own body, by ordinal.
    Footnote {
        /// Zero-based ordinal among footnotes.
        index: u32,
    },
    /// One endnote's own body, by ordinal.
    Endnote {
        /// Zero-based ordinal among endnotes.
        index: u32,
    },
    /// One comment's own body, by the durable id the package carries.
    Comment {
        /// `w16cid:durableId`, else `w14:paraId`, else `#<ordinal>`.
        id: String,
    },
    /// Not inside a story: a definition, a section setup, or document metadata.
    Definitions,
}

/// One step of the container path from a story's root to a block.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PathSegment {
    /// The nth block of the story or container.
    Block {
        /// Zero-based index among siblings.
        index: u32,
    },
    /// The nth row of the enclosing table.
    Row {
        /// Zero-based index.
        index: u32,
    },
    /// The nth cell of the enclosing row.
    Cell {
        /// Zero-based index.
        index: u32,
    },
}

/// Where a change is, on one side of the comparison.
///
/// `start`/`end` are UTF-8 byte offsets into the block's **projected plain
/// text** — the `FinalWithMarkup` projection [`crate::projection::block_text`]
/// produces, which is what the editor shows. They are `0..0` for a change that
/// is not inside text, and for a whole block, whose extent is the block.
///
/// That space agrees byte for byte with the review/caret offset space for runs
/// and symbols, which is most paragraphs, **but it is not the same space**: a
/// tab contributes one byte to projected text and none to the offset space,
/// while a note reference, an equation and a label-bearing embedded object
/// contribute to the offset space and nothing to projected text. A host that
/// hands these numbers to a caret or a review anchor must therefore translate
/// them through the block's own inlines rather than assume they transfer —
/// `casual-doc-wasm`'s `plain_offset_to_anchor_offset` is that translation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffAnchor {
    /// The story this side of the change lives in.
    pub story: Story,
    /// The container path to the block, for a host that wants to say "row 3 of
    /// table 2" without walking the document.
    pub path: Vec<PathSegment>,
    /// The block's stable node identity **on that side**, and only there.
    ///
    /// Both sides of a comparison are parsed by this crate's caller, and ids are
    /// minted per import (see the crate docs), so this id addresses the parsed
    /// state the comparison ran on and **nothing else**. It is not a match key
    /// between the two sides, and it is not an id in a live editing session's
    /// document even when that session's bytes were one of the two sides: that
    /// re-export was re-parsed, and the ids restarted. The coordinate that
    /// survives into a live document is [`DiffAnchor::path`].
    pub node: Option<String>,
    /// Start byte offset in the block's projected text.
    pub start: u32,
    /// End byte offset in the block's projected text.
    pub end: u32,
}

/// The construct family a change belongs to (`docs/139` §9.2).
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffFamily {
    /// A whole block appeared, disappeared or moved.
    Block,
    /// Text inside a block that exists on both sides.
    Text,
    /// Character or paragraph formatting, with the text identical.
    Formatting,
    /// Paragraph style or list/numbering membership.
    Style,
    /// Table rows, cells, grid or merge topology.
    Table,
    /// A drawing, image, embedded object, text box or math object.
    Object,
    /// Section and page setup.
    Section,
    /// A style, numbering or other definition redefined.
    Definition,
    /// Media or another package resource added, removed or replaced.
    Resource,
    /// A comment added, removed or edited.
    Comment,
    /// Pending tracked-change markup differs while the projected text does not.
    Review,
    /// Document metadata (core properties).
    Metadata,
}

/// What kind of change it is. These strings are the ones review already uses.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    /// Content present on the right and not on the left.
    Insertion,
    /// Content present on the left and not on the right.
    Deletion,
    /// The place a moved block came from.
    MoveFrom,
    /// The place a moved block went to.
    MoveTo,
    /// The same content, formatted differently.
    Formatting,
    /// A typed property changed.
    Property,
}

/// How sure the record is.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Derived from an exact comparison. Insertions, deletions, text and
    /// property changes are all exact.
    Exact,
    /// Derived from a heuristic: a move, which is only reported when the
    /// content is unique on both sides (`docs/140` §11.2 step 6).
    Heuristic,
}

/// One change.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffChange {
    /// A stable id: the hex of a hash of the change's own content, so the same
    /// two versions always produce the same ids and a host may use one as a key
    /// across a re-run. Not a position, because positions shift.
    pub id: String,
    /// The construct family.
    pub family: DiffFamily,
    /// The kind, in review's vocabulary.
    pub kind: DiffKind,
    /// Where it is on the left (older) side, when it exists there.
    pub left: Option<DiffAnchor>,
    /// Where it is on the right (newer) side, when it exists there.
    pub right: Option<DiffAnchor>,
    /// A bounded excerpt of the left text, for a list row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_text: Option<String>,
    /// A bounded excerpt of the right text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_text: Option<String>,
    /// The typed field paths that differ, for a formatting or property change —
    /// `["alignment"]`, `["spacing.beforeTwips"]`, `["pageSize.orientation"]`.
    /// Reflected from the model type's own serde field names, so a property
    /// added to the model is named here without this crate being edited.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// The other half of a move pair, by change id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paired_with: Option<String>,
    /// Exact, or heuristic.
    pub confidence: Confidence,
}

/// Why part of the comparison is not in the change list.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCode {
    /// The construct differs and this engine has no typed comparison for it, so
    /// the reader is told *that* it changed and told that the detail is missing.
    NotCompared,
    /// Two candidate matches were equally good, so the region is reported as a
    /// deletion plus an insertion rather than as an edit or a move.
    AmbiguousMatch,
    /// A referenced resource is absent on one side, so it could not be compared
    /// by content.
    MissingResource,
    /// A bound was reached and the comparison stopped being exhaustive.
    Truncated,
}

/// One aggregated finding.
///
/// Aggregated on purpose: a document with 40,000 drawings must produce one
/// finding saying so, not 40,000 rows. `count` is the number of occurrences
/// folded into this row.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFinding {
    /// Why.
    pub code: FindingCode,
    /// Which construct, as a stable machine name (`drawing`, `math`,
    /// `formatScheme`, `inlineTextBudget`).
    pub construct: String,
    /// How many occurrences this row stands for.
    pub count: u32,
}

/// Per-side totals a host shows without walking anything.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SideSummary {
    /// Projected blocks (paragraphs, tables, rows, cells, content controls).
    pub blocks: u32,
    /// Stories projected (body plus every header, footer, note and comment).
    pub stories: u32,
}

/// Work actually done, for a host's diagnostics and for the complexity guard.
///
/// `comparisons` is the alignment-key comparison count. It is the quantity the
/// doubling guard measures, because a wall clock cannot tell a slow constant
/// from a quadratic (SKILL §8).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffDiagnostics {
    /// Blocks visited while projecting, both sides.
    pub blocks_visited: u64,
    /// Alignment-key comparisons, both sides.
    pub comparisons: u64,
    /// Property/object value serializations performed. Memoized by flyweight
    /// identity, so this is the count of *distinct* property values, not of
    /// nodes.
    pub value_hashes: u64,
    /// Steps the host drove the job through.
    pub steps: u32,
}

/// The whole sidecar.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionDiff {
    /// [`DIFF_SCHEMA`].
    pub schema: u32,
    /// The older side.
    pub left: SideSummary,
    /// The newer side.
    pub right: SideSummary,
    /// Changes in document order of the right side, then of the left.
    pub changes: Vec<DiffChange>,
    /// Everything the comparison could not characterise, aggregated.
    pub findings: Vec<DiffFinding>,
    /// Count per [`DiffKind`], keyed by the same strings the records carry.
    pub kind_counts: Vec<(DiffKind, u32)>,
    /// Count per [`DiffFamily`].
    pub family_counts: Vec<(DiffFamily, u32)>,
    /// Whether every construct that differs is characterised in `changes`.
    /// **False whenever `findings` is non-empty** — the two cannot disagree,
    /// which is what stops a diff from being labelled complete while a family
    /// was skipped (`docs/139` VH-009).
    pub complete: bool,
    /// What the run cost.
    pub diagnostics: DiffDiagnostics,
}

impl VersionDiff {
    /// Serializes the sidecar as JSON. **O(records)**.
    ///
    /// # Errors
    ///
    /// Only if `serde_json` cannot serialize plain data, which it always can;
    /// the result is propagated rather than unwrapped so the facade has one
    /// error path.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// The parameters of one change, so the constructor is not an eight-argument
/// function.
#[derive(Clone, Debug, Default)]
pub struct ChangeSpec {
    /// Family.
    pub family: Option<DiffFamily>,
    /// Kind.
    pub kind: Option<DiffKind>,
    /// Left anchor.
    pub left: Option<DiffAnchor>,
    /// Right anchor.
    pub right: Option<DiffAnchor>,
    /// Left excerpt.
    pub left_text: Option<String>,
    /// Right excerpt.
    pub right_text: Option<String>,
    /// Typed field paths.
    pub fields: Vec<String>,
    /// Exact unless a heuristic produced it.
    pub heuristic: bool,
}

impl ChangeSpec {
    /// Finishes a change, deriving its stable id from its own content.
    ///
    /// The id must be the same for the same two versions on every run, so it is
    /// a hash of the record rather than its position in the list: positions
    /// shift when a filter is applied, and a host that remembers "the change I
    /// was reading" needs a key that does not.
    ///
    /// **O(the record)**.
    #[must_use]
    pub fn build(self) -> DiffChange {
        let family = self.family.unwrap_or(DiffFamily::Block);
        let kind = self.kind.unwrap_or(DiffKind::Property);
        let mut hasher = ContentHasher::new();
        hasher.write_str(&format!("{family:?}/{kind:?}"));
        for anchor in [self.left.as_ref(), self.right.as_ref()] {
            match anchor {
                Some(anchor) => {
                    hasher.write_str(&format!("{:?}", anchor.story));
                    hasher.write_str(&format!("{:?}", anchor.path));
                    hasher.write_str(anchor.node.as_deref().unwrap_or(""));
                    hasher.write_u64(u64::from(anchor.start));
                    hasher.write_u64(u64::from(anchor.end));
                }
                None => hasher.write_tag(0),
            }
        }
        for field in &self.fields {
            hasher.write_str(field);
        }
        DiffChange {
            id: format!("{:032x}", hasher.finish()),
            family,
            kind,
            left: self.left,
            right: self.right,
            left_text: self.left_text,
            right_text: self.right_text,
            fields: self.fields,
            paired_with: None,
            confidence: if self.heuristic {
                Confidence::Heuristic
            } else {
                Confidence::Exact
            },
        }
    }
}

/// Findings, aggregated by `(code, construct)` as they are discovered.
///
/// Aggregation is the point: a diff of a document with 40,000 drawings must
/// report **one** row saying drawings are not characterised, with a count. A list
/// of 40,000 identical findings is the same defect as no finding at all, because
/// nobody reads it.
#[derive(Clone, Debug, Default)]
pub struct FindingSet {
    counts: BTreeMap<(FindingCode, String), u32>,
}

impl FindingSet {
    /// Records one occurrence. **O(log rows)**.
    pub fn note(&mut self, code: FindingCode, construct: &str) {
        *self.counts.entry((code, construct.to_owned())).or_insert(0) += 1;
    }

    /// Whether anything was reported.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// The findings, in a deterministic order.
    #[must_use]
    pub fn into_vec(self) -> Vec<DiffFinding> {
        self.counts
            .into_iter()
            .map(|((code, construct), count)| DiffFinding {
                code,
                construct,
                count,
            })
            .collect()
    }
}

/// A short excerpt of `text`, ending on a character boundary.
///
/// **O(limit)**, not O(text): a change record must not carry a whole paragraph
/// of a pathological document, and a host that wants the full text has the
/// anchor to fetch it with.
#[must_use]
pub fn excerpt(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = text[..end].to_owned();
    out.push('\u{2026}');
    out
}
