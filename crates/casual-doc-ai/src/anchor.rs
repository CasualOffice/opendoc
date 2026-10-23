// SPDX-License-Identifier: Apache-2.0

//! Anchors: a `NodeId` plus a quote (the W3C Web Annotation
//! `TextQuoteSelector`: `exact`, `prefix`, `suffix`), resolved inside that node
//! only. Resolution is deterministic and **refuses rather than guesses**
//! (`docs/178` §4.2): no match is `anchor-not-found` with the node's text, two
//! matches with nothing to choose between them is `anchor-ambiguous` with the
//! candidates, and the document is never searched for a first coincidence.

use casual_doc_edit::find_paragraph_any;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{Document, Paragraph};
use casual_doc_review::body::NoteAnchorLengths;
use serde::Deserialize;
use serde_json::json;

use crate::error::{AiError, AiResult};
use crate::read::node_id;
use crate::text::{clip, offset_spaces_agree, paragraph_text};

/// Which side of the target an insertion goes.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    /// Before the quote (or at the paragraph's start).
    Before,
    /// After the quote (or at the paragraph's end). The default.
    #[default]
    After,
}

/// A byte range a program passes back exactly as `read`/`search` returned it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PosRange {
    /// Start, UTF-8 bytes into the node's text.
    pub start: u32,
    /// End, exclusive.
    pub end: u32,
}

/// A pointer at text an agent can produce and the server can check.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    /// A paragraph id returned by `get_outline`, `read_blocks` or `search`.
    pub node: String,
    /// The quoted text, matched inside `node` only.
    #[serde(default)]
    pub exact: Option<String>,
    /// Text immediately before `exact`, to disambiguate.
    #[serde(default)]
    pub prefix: Option<String>,
    /// Text immediately after `exact`, to disambiguate.
    #[serde(default)]
    pub suffix: Option<String>,
    /// 1-based occurrence of `exact` among the matches that remain.
    #[serde(default)]
    pub occurrence: Option<u32>,
    /// Offsets for programs only; never computed by a model.
    #[serde(default)]
    pub pos: Option<PosRange>,
    /// For an insertion point: before or after the target.
    #[serde(default)]
    pub side: Option<Side>,
}

/// A resolved anchor: a paragraph and a byte range of its text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resolved {
    /// The paragraph.
    pub node: NodeId,
    /// Start byte.
    pub start: u32,
    /// End byte, exclusive.
    pub end: u32,
}

impl Resolved {
    /// The insertion point `side` names.
    #[must_use]
    pub fn point(self, side: Side) -> u32 {
        match side {
            Side::Before => self.start,
            Side::After => self.end,
        }
    }
}

/// The paragraph `node` names, wherever it lives. **O(document)** (one walk).
pub(crate) fn paragraph<'a>(document: &'a Document, node: &str) -> AiResult<&'a Paragraph> {
    let id = node_id(node)?;
    find_paragraph_any(document, id).ok_or_else(|| {
        AiError::new(
            "anchor-not-found",
            "No paragraph has that id at this revision; read the document again.",
        )
        .with_details(json!({ "node": node }))
    })
}

/// Resolves `anchor` against `document`. With no quote and no offsets it names
/// the whole paragraph. **O(document)** to find the paragraph, then
/// O(paragraph text).
///
/// # Errors
///
/// `anchor-not-found`, `anchor-ambiguous`, `invalid-argument`, or
/// `unsupported-content` for a paragraph whose two offset spaces disagree.
pub fn resolve(document: &Document, anchor: &Anchor) -> AiResult<Resolved> {
    let paragraph = paragraph(document, &anchor.node)?;
    if !offset_spaces_agree(&NoteAnchorLengths::of(document), paragraph) {
        return Err(AiError::new(
            "unsupported-content",
            "This paragraph holds an equation, a note reference or an embedded object, whose \
             positions the agent edit path cannot yet address exactly; it is refused rather \
             than edited in the wrong place.",
        ));
    }
    let text = paragraph_text(paragraph);
    let len = text.len() as u32;
    let not_found = |why: &str| {
        AiError::new("anchor-not-found", why.to_owned()).with_details(json!({
            "node": anchor.node,
            "text": clip(&text, 2_000),
            "truncated": text.len() > 2_000,
        }))
    };
    let Some(exact) = anchor.exact.as_deref() else {
        if let Some(pos) = anchor.pos {
            return check_pos(&text, paragraph.id, pos);
        }
        return Ok(Resolved {
            node: paragraph.id,
            start: 0,
            end: len,
        });
    };
    if exact.is_empty() {
        return Err(AiError::invalid(
            "`exact` must not be empty; omit it to mean the whole paragraph.",
        ));
    }
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    let mut from = 0;
    while let Some(found) = text[from..].find(exact) {
        let start = from + found;
        candidates.push((start, start + exact.len()));
        from = start + exact.chars().next().map_or(1, char::len_utf8);
    }
    if candidates.is_empty() {
        return Err(not_found("The quoted text is not in that paragraph."));
    }
    if let Some(prefix) = anchor.prefix.as_deref() {
        candidates.retain(|(start, _)| text[..*start].ends_with(prefix));
    }
    if let Some(suffix) = anchor.suffix.as_deref() {
        candidates.retain(|(_, end)| text[*end..].starts_with(suffix));
    }
    if candidates.is_empty() {
        return Err(not_found(
            "The quoted text is in that paragraph, but not with that prefix or suffix.",
        ));
    }
    // The quote is checked even when offsets are given: a program that passes
    // `pos` back must be pointing at the text it thinks it is.
    if let Some(pos) = anchor.pos {
        let resolved = check_pos(&text, paragraph.id, pos)?;
        if candidates
            .iter()
            .any(|(start, end)| *start as u32 == resolved.start && *end as u32 == resolved.end)
        {
            return Ok(resolved);
        }
        return Err(not_found("`pos` does not cover the quoted text."));
    }
    let chosen = match anchor.occurrence {
        Some(0) => return Err(AiError::invalid("`occurrence` is 1-based.")),
        Some(n) => *candidates
            .get(n as usize - 1)
            .ok_or_else(|| not_found("There are fewer occurrences than that."))?,
        None if candidates.len() == 1 => candidates[0],
        None => {
            let listed: Vec<_> = candidates
                .iter()
                .take(20)
                .enumerate()
                .map(|(index, (start, end))| {
                    let before = floor(&text, start.saturating_sub(30));
                    let after = ceil(&text, (*end + 30).min(text.len()));
                    json!({
                        "occurrence": index + 1,
                        "prefix": &text[before..*start],
                        "suffix": &text[*end..after],
                    })
                })
                .collect();
            return Err(AiError::new(
                "anchor-ambiguous",
                "The quoted text occurs more than once in that paragraph; add a prefix, a suffix \
                 or an occurrence.",
            )
            .with_details(json!({ "node": anchor.node, "candidates": listed })));
        }
    };
    Ok(Resolved {
        node: paragraph.id,
        start: chosen.0 as u32,
        end: chosen.1 as u32,
    })
}

fn check_pos(text: &str, node: NodeId, pos: PosRange) -> AiResult<Resolved> {
    let (start, end) = (pos.start as usize, pos.end as usize);
    if start > end
        || end > text.len()
        || !text.is_char_boundary(start)
        || !text.is_char_boundary(end)
    {
        return Err(AiError::invalid(
            "`pos` is outside the paragraph or splits a character.",
        ));
    }
    Ok(Resolved {
        node,
        start: pos.start,
        end: pos.end,
    })
}

fn floor(text: &str, mut at: usize) -> usize {
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

fn ceil(text: &str, mut at: usize) -> usize {
    while at < text.len() && !text.is_char_boundary(at) {
        at += 1;
    }
    at
}
