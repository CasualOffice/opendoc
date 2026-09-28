//! Text diff inside one block that exists on both sides.
//!
//! # One aligner, not two
//!
//! This does **not** implement a second diff algorithm. A paragraph's text is
//! projected to a sequence of extended-grapheme-cluster hashes and handed to
//! [`crate::align::align`], the same anchored Myers aligner the block level uses.
//! Two implementations of one rule diverge (SKILL §8), and the grapheme level is
//! where a second one would have been written by reflex.
//!
//! # Tokens built from graphemes, not bare graphemes
//!
//! Aligning single grapheme clusters is technically correct and useless to read:
//! `brown` against `red` shares an `r`, so a cluster-level diff reports "deleted
//! b, kept r, replaced own with ed". A reader wants one changed word, which is
//! what Word's and Google Docs' comparisons show.
//!
//! So the sequence is **tokens**: a maximal run of alphanumeric clusters, a
//! maximal run of whitespace clusters, or one cluster of anything else. Tokens
//! are built by grouping grapheme clusters, never by splitting them, so an emoji
//! with a skin-tone modifier is one token and a diff can never claim half of it
//! changed. Offsets stay UTF-8 byte offsets at cluster boundaries — the positions
//! the caret can occupy.
//!
//! # Complexity
//!
//! **O(t log t)** in the tokens of the two texts, with the per-piece ceilings of
//! the block aligner. A pair of texts whose shorter side exceeds
//! [`MAX_INLINE_TOKENS`] tokens is reported as one whole-text replacement with a
//! `Truncated` finding, because a 20,000-token paragraph is a pathological
//! document (`docs/138`) and an exact intra-paragraph diff of one is not worth a
//! stalled tab.

use unicode_segmentation::UnicodeSegmentation;

use crate::align::{Step, align};
use crate::hash::hash_str;

/// The largest shorter-side token count that gets an exact intra-block diff.
pub const MAX_INLINE_TOKENS: usize = 20_000;

/// One text change inside a block, in UTF-8 byte offsets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextEdit {
    /// Byte range removed from the left text.
    pub left: (u32, u32),
    /// Byte range added in the right text.
    pub right: (u32, u32),
}

impl TextEdit {
    /// Whether this edit removes anything.
    #[must_use]
    pub const fn removes(&self) -> bool {
        self.left.1 > self.left.0
    }

    /// Whether this edit adds anything.
    #[must_use]
    pub const fn adds(&self) -> bool {
        self.right.1 > self.right.0
    }
}

/// The result of diffing one block's text.
#[derive(Clone, Debug, Default)]
pub struct TextDiff {
    /// The edits, in left-then-right document order.
    pub edits: Vec<TextEdit>,
    /// Grapheme comparisons performed.
    pub comparisons: u64,
    /// Whether the exact diff was skipped for a bound.
    pub truncated: bool,
}

/// Diffs two block texts at token granularity.
#[must_use]
pub fn diff_text(left: &str, right: &str) -> TextDiff {
    let left_clusters = tokens(left);
    let right_clusters = tokens(right);
    if left_clusters.len().min(right_clusters.len()) > MAX_INLINE_TOKENS {
        return TextDiff {
            edits: vec![TextEdit {
                left: (0, u32::try_from(left.len()).unwrap_or(u32::MAX)),
                right: (0, u32::try_from(right.len()).unwrap_or(u32::MAX)),
            }],
            comparisons: 0,
            truncated: true,
        };
    }
    let left_keys: Vec<u128> = left_clusters.iter().map(|cluster| cluster.hash).collect();
    let right_keys: Vec<u128> = right_clusters.iter().map(|cluster| cluster.hash).collect();
    let alignment = align(&left_keys, &right_keys, 0, 0);

    // Coalesce each maximal run of Delete/Insert steps into ONE edit, so
    // "replaced a word" is one record rather than one per cluster. Two running
    // cursors carry where the last kept cluster ended on each side, which is
    // where a pure insertion or a pure deletion is anchored on the other side.
    // **O(steps)** — deliberately not a scan-back per step, which is how an
    // innocent-looking coalescer becomes quadratic.
    let mut edits: Vec<TextEdit> = Vec::new();
    let mut pending: Option<TextEdit> = None;
    let mut left_cursor = 0u32;
    let mut right_cursor = 0u32;
    for step in &alignment.steps {
        match *step {
            Step::Equal(left_index, right_index) => {
                if let Some(edit) = pending.take() {
                    edits.push(edit);
                }
                left_cursor = left_clusters[left_index as usize].end;
                right_cursor = right_clusters[right_index as usize].end;
            }
            Step::Delete(index) => {
                let cluster = left_clusters[index as usize];
                let slot = pending.get_or_insert(TextEdit {
                    left: (cluster.start, cluster.start),
                    right: (right_cursor, right_cursor),
                });
                slot.left.1 = cluster.end;
            }
            Step::Insert(index) => {
                let cluster = right_clusters[index as usize];
                let slot = pending.get_or_insert(TextEdit {
                    left: (left_cursor, left_cursor),
                    right: (cluster.start, cluster.start),
                });
                slot.right.1 = cluster.end;
            }
        }
    }
    if let Some(edit) = pending.take() {
        edits.push(edit);
    }
    TextDiff {
        edits,
        comparisons: alignment.comparisons,
        truncated: false,
    }
}

/// One token's byte range and hash.
#[derive(Clone, Copy, Debug)]
struct Cluster {
    start: u32,
    end: u32,
    hash: u128,
}

/// What a grapheme cluster groups with.
#[derive(Clone, Copy, Eq, PartialEq)]
enum Class {
    /// Alphanumeric: groups into words.
    Word,
    /// Whitespace: groups into runs of space.
    Space,
    /// Punctuation, symbols, emoji: one token each, so a changed bullet or a
    /// changed emoji is its own change and not part of a neighbouring word.
    Other,
}

/// The class of a cluster, decided by its FIRST character.
///
/// First character, not "all characters", because a cluster's later code points
/// are its combining marks and modifiers: they belong to whatever the base
/// character is, and asking about them is how a cluster ends up split.
fn class_of(cluster: &str) -> Class {
    match cluster.chars().next() {
        Some(character) if character.is_alphanumeric() => Class::Word,
        Some(character) if character.is_whitespace() => Class::Space,
        _ => Class::Other,
    }
}

/// Groups `text`'s grapheme clusters into tokens with their byte ranges.
/// **O(bytes)**.
fn tokens(text: &str) -> Vec<Cluster> {
    let mut out: Vec<Cluster> = Vec::new();
    let mut open: Option<(usize, Class)> = None;
    let flush = |out: &mut Vec<Cluster>, start: usize, end: usize| {
        out.push(Cluster {
            start: u32::try_from(start).unwrap_or(u32::MAX),
            end: u32::try_from(end).unwrap_or(u32::MAX),
            hash: hash_str(&text[start..end]),
        });
    };
    for (start, cluster) in text.grapheme_indices(true) {
        let end = start + cluster.len();
        let class = class_of(cluster);
        match open {
            Some((_, open_class)) if open_class == class && class != Class::Other => {}
            Some((open_start, _)) => {
                flush(&mut out, open_start, start);
                open = Some((start, class));
            }
            None => open = Some((start, class)),
        }
        if class == Class::Other {
            flush(&mut out, start, end);
            open = None;
        }
    }
    if let Some((open_start, _)) = open {
        flush(&mut out, open_start, text.len());
    }
    out
}
