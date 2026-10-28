// SPDX-License-Identifier: Apache-2.0

//! Text projections and the declared token estimate.

use casual_doc_layout::flow::node_plain_text;
use casual_doc_model::v1::{InlineNode, Paragraph, ReviewProjection, RunProperties};
use casual_doc_review::body::{NoteAnchorLengths, inlines_anchor_len};
use serde::Serialize;

/// The name of the one token heuristic every bound and estimate uses
/// (`docs/178` §4.5). A token count depends on the model's vocabulary, which
/// the server does not know, so the estimate is labelled rather than presented
/// as a count.
pub const ESTIMATOR: &str = "bytes/4";

/// Estimated tokens for `text`: UTF-8 bytes ÷ 4, rounded up, at least one.
/// O(1).
#[must_use]
pub fn estimate_tokens(text: &str) -> u32 {
    u32::try_from(text.len().div_ceil(4))
        .unwrap_or(u32::MAX)
        .max(1)
}

/// The paragraph's text in the space the editor's offsets address (final with
/// markup: pending insertions and deletions both present). O(inlines).
#[must_use]
pub fn paragraph_text(paragraph: &Paragraph) -> String {
    node_plain_text(&paragraph.inlines)
}

/// Whether the two offset spaces the engine uses for one paragraph agree.
///
/// Direct edits address `node_plain_text`; review splices address the
/// `inline_anchor_len` space. They differ only on equations, note references,
/// preview-less embedded objects and the two special hyphens, each of which is
/// zero bytes in one and positive in the other, so equal totals mean equal
/// positions. Where they disagree an anchor resolved against the text could
/// land in a different place than the splice, so the edit is refused instead
/// (`unsupported-content`). O(inlines).
#[must_use]
pub fn offset_spaces_agree(notes: &NoteAnchorLengths, paragraph: &Paragraph) -> bool {
    paragraph_text(paragraph).len() as u64
        == u64::from(inlines_anchor_len(notes, &paragraph.inlines))
}

/// One run of a paragraph as an agent reads it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunView {
    /// The run's text.
    pub text: String,
    /// Direct formatting and review state: `bold`, `italic`, `underline`,
    /// `strike`, `inserted`, `deleted`, `link`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<&'static str>,
}

fn marks_of(properties: &RunProperties, extra: &[&'static str]) -> Vec<&'static str> {
    let mut marks = Vec::new();
    if properties.bold == Some(true) {
        marks.push("bold");
    }
    if properties.italic == Some(true) {
        marks.push("italic");
    }
    if properties.underline == Some(true) {
        marks.push("underline");
    }
    if properties.strike == Some(true) {
        marks.push("strike");
    }
    marks.extend_from_slice(extra);
    marks
}

/// The paragraph's runs with their marks, in reading order, bounded by
/// `max_runs` (the rest are summarised by the caller as truncated).
/// O(inlines).
#[must_use]
pub fn runs(paragraph: &Paragraph, max_runs: usize) -> Vec<RunView> {
    runs_in(paragraph, max_runs, false)
}

/// [`runs`] with pending **deletions** too, each marked `deleted`: the redline
/// a preview shows (`docs/178` §4.3, "insertions and deletions marked").
/// [`runs`] leaves them out because anchors resolve against the text without
/// them; a person deciding on a change has to see what it removes.
/// O(inlines).
#[must_use]
pub fn redline_runs(paragraph: &Paragraph, max_runs: usize) -> Vec<RunView> {
    runs_in(paragraph, max_runs, true)
}

fn runs_in(paragraph: &Paragraph, max_runs: usize, deletions: bool) -> Vec<RunView> {
    fn walk(
        inlines: &[InlineNode],
        extra: &mut Vec<&'static str>,
        out: &mut Vec<RunView>,
        max: usize,
        deletions: bool,
    ) {
        for inline in inlines {
            if out.len() >= max {
                return;
            }
            // container-set: Hyperlink, Revision and Sdt carry this paragraph's
            // runs; Field results, TextBox and Group stories are other text and
            // are read through their own anchors.
            match inline {
                InlineNode::Run(run) if !run.text.is_empty() => out.push(RunView {
                    text: run.text.clone(),
                    marks: marks_of(&run.properties, extra),
                }),
                InlineNode::Hyperlink(link) => {
                    extra.push("link");
                    walk(&link.inlines, extra, out, max, deletions);
                    extra.pop();
                }
                InlineNode::Revision(revision)
                    if revision
                        .kind
                        .contributes_to(ReviewProjection::FinalWithMarkup)
                        || (deletions
                            && revision.kind.contributes_to(ReviewProjection::Original)) =>
                {
                    let mark = match revision.kind {
                        casual_doc_model::v1::RevisionKind::Deletion
                        | casual_doc_model::v1::RevisionKind::MoveFrom => "deleted",
                        _ => "inserted",
                    };
                    extra.push(mark);
                    walk(&revision.inlines, extra, out, max, deletions);
                    extra.pop();
                }
                InlineNode::Sdt(sdt) => walk(&sdt.inlines, extra, out, max, deletions),
                // `docs/181` §2.2 (G2): every inline kind is classified, with no
                // wildcard, so a new kind does not compile until someone decides
                // whether runs show it. Not run text, by design: an empty run,
                // a revision the markup view hides, layout characters, markers
                // and anchors, and stories read through their own anchors
                // (fields, drawings, objects, text boxes, notes, math; surfaces
                // arrive with R2).
                InlineNode::Run(_)
                | InlineNode::Revision(_)
                | InlineNode::Tab(_)
                | InlineNode::Break(_)
                | InlineNode::Symbol(_)
                | InlineNode::HorizontalRule(_)
                | InlineNode::NoBreakHyphen(_)
                | InlineNode::SoftHyphen(_)
                | InlineNode::PositionalTab(_)
                | InlineNode::Drawing(_)
                | InlineNode::AnchoredDrawing(_)
                | InlineNode::EmbeddedObject(_)
                | InlineNode::Field(_)
                | InlineNode::TextBox(_)
                | InlineNode::Group(_)
                | InlineNode::Math(_)
                | InlineNode::NoteReference(_)
                | InlineNode::NoteNumberMark(_)
                | InlineNode::CommentReference(_)
                | InlineNode::CommentRangeStart(_)
                | InlineNode::CommentRangeEnd(_)
                | InlineNode::BookmarkStart(_)
                | InlineNode::BookmarkEnd(_)
                | InlineNode::FieldRangeStart(_)
                | InlineNode::FieldRangeEnd(_)
                | InlineNode::MoveRangeStart(_)
                | InlineNode::MoveRangeEnd(_) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(
        &paragraph.inlines,
        &mut Vec::new(),
        &mut out,
        max_runs,
        deletions,
    );
    out
}

/// `text` cut to at most `max_bytes` on a character boundary. O(max_bytes).
#[must_use]
pub fn clip(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
