//! The redline view: a finished comparison painted into a **read-only copy of
//! its newer side**, so a reader sees what changed where it changed (ADR-065).
//!
//! # What a host calls
//!
//! ```js
//! const job = beginVersionDiff(olderBytes, newerBytes);   // drive to "complete"
//! const view = open(newerBytes);                           // a throwaway copy
//! const summary = JSON.parse(view.showComparison(job, author, date));
//! view.setShowChanges(true);                               // and put it on the canvas
//! ```
//!
//! # Why it is a document and not an overlay
//!
//! ADR-061 rejected a second mechanism that paints the same marks the review
//! layer already paints, and that rejection stands: everything here becomes an
//! ordinary [`Revision`] in an ordinary document, so the author colour, the
//! underline and strike, the change cards, next/previous and `listRevisions` are
//! review's own, unchanged. What is new is the one thing review could not say
//! and the reason ADR-064 retreated to a text diff in a side panel: **a whole
//! paragraph that is gone, and where a moved paragraph came from.** Those are
//! put back, struck through, at the position the older document had them
//! (`DiffChange::place`), through `Operation::InsertBlocks` — the structured
//! paste primitive — rather than by editing the model behind the transaction
//! layer's back.
//!
//! # Why it is safe to do this to a document
//!
//! It is never done to the reader's document. The handle it is called on must
//! hold exactly the comparison's newer side — checked by content digest, not
//! assumed — and the host opens that handle from the newer side's bytes for the
//! purpose and frees it when the view closes. The live session is not read,
//! exported or written, which is ADR-064's invariant kept.
//!
//! # What a restored paragraph keeps
//!
//! Its text, its tabs and breaks, its direct paragraph and character
//! formatting, and its paragraph and character **styles, matched by name**:
//! style ids are minted per import (`StyleId` wraps a `NodeId`), so the older
//! side's id names nothing — or the wrong style — in the newer document. A style
//! the newer document no longer defines falls back to the default, and so does
//! list numbering, whose instance ids have the same per-import problem and no
//! name to match on. Pictures, fields' codes, notes and other objects inside a
//! removed paragraph are not reconstructed — their resources belong to the
//! older package — and that is reported (`removedObject`), never silent.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use casual_doc_diff::identity::content_digest_hex;
use casual_doc_diff::projection::{block_at_path, insertion_point_at_path};
use casual_doc_diff::record::{DiffChange, DiffFamily, DiffKind, Story};
use casual_doc_edit::Operation;
use casual_doc_edit::Pos;
use casual_doc_edit::refused;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document, InlineNode, Paragraph, ParagraphProperties, Revision, RevisionKind,
    RunProperties, StyleId, StyleKind,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::diff::{WasmVersionDiff, classify_change};
use crate::{
    HistoryKind, NoteAnchorLengths, WasmDocument, to_js, validate_authored_revision_author,
};

/// What [`WasmDocument::show_comparison`] reports back, as JSON.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComparisonView {
    /// The document's revision after the marks landed.
    revision: u32,
    /// Whole paragraphs put back, struck, at the place they stood.
    restored: u32,
    /// For every change with a newer-side anchor, the paragraph it lands on in
    /// THIS document, keyed by change id — resolved before any paragraph was put
    /// back, because putting one back shifts every later path. A host navigates
    /// a formatting change (which has no mark of its own) through this.
    anchors: BTreeMap<String, String>,
    /// For every restored paragraph, its new id here, keyed by change id.
    placed: BTreeMap<String, String>,
    /// Every paragraph named in `anchors` or `placed`, in document order, so a
    /// host can step through the changes in the order a reader meets them
    /// without walking the document itself.
    order: Vec<String>,
    /// The loss keys of what could not be painted, as `applyDiffAsRevisions`
    /// reports them.
    unmarked: Vec<&'static str>,
}

/// One removed paragraph to put back.
struct Restore {
    container: Option<NodeId>,
    index: u32,
    /// Document order among restores at the same point.
    order: usize,
    change: String,
    block: BlockNode,
}

#[wasm_bindgen]
impl WasmDocument {
    /// Paints a FINISHED comparison into this document as a read-only redline
    /// (**ADR-065**) and returns a JSON summary (see `ComparisonView`).
    ///
    /// This handle must be a throwaway copy holding exactly `comparison`'s newer
    /// side — opened by the host from the same bytes for this purpose. Every
    /// change becomes a tracked change authored to `author`/`date`: an insertion
    /// or a move's destination is marked where it is, removed text inside a
    /// paragraph is put back struck at its offset, and a whole paragraph that is
    /// gone — deleted, or moved away from here — is put back struck at the
    /// position it had. Formatting, table-structure, section and definition
    /// differences have no mark and are reported as `unmarked`, with their
    /// paragraphs in `anchors` so a host can still take the reader there.
    ///
    /// Unlike `applyDiffAsRevisions` it does **not** refuse a document that
    /// already carries tracked changes: nothing here is ever decided, so a
    /// version's own suggestions and the comparison's marks cannot decide each
    /// other, and a reader of a version with suggestions in it is exactly who
    /// should see both.
    ///
    /// Complexity: **O(document)** once, for the digest that proves this is the
    /// comparison's newer side, plus O(changes + the paragraphs they touch).
    /// Called once per view, never per interaction.
    ///
    /// # Errors
    ///
    /// Throws when the comparison has not finished, when this document is not
    /// its newer side, or when the author name is missing or too long.
    #[wasm_bindgen(js_name = showComparison)]
    pub fn show_comparison(
        &mut self,
        comparison: &WasmVersionDiff,
        author: &str,
        date: Option<String>,
    ) -> Result<String, JsValue> {
        self.show_comparison_inner(comparison, author, date)
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// [`WasmDocument::show_comparison`] without the `JsValue`.
    pub(crate) fn show_comparison_inner(
        &mut self,
        comparison: &WasmVersionDiff,
        author: &str,
        date: Option<String>,
    ) -> Result<String, String> {
        let Some((left, right, diff)) = comparison.finished() else {
            return Err(refused!(
                "compare.view-unfinished",
                "The comparison has not finished, so there is nothing to show yet."
            )
            .to_owned());
        };
        validate_authored_revision_author(Some(author))?;
        if content_digest_hex(&self.document) != content_digest_hex(right) {
            return Err(refused!(
                "compare.view-mismatch",
                "This document is not the newer side of the comparison, so its changes \
                 cannot be shown on it."
            )
            .to_owned());
        }

        let notes = NoteAnchorLengths::of(&self.document);
        let mut loss: BTreeSet<&'static str> = BTreeSet::new();
        if !diff.complete {
            loss.insert("incompleteComparison");
        }
        let mut view = ComparisonView::default();
        let mut edits = Vec::new();
        let mut restores: Vec<Restore> = Vec::new();
        let styles = StyleNames::of(left, &self.document);
        for (order, change) in diff.changes.iter().enumerate() {
            if let Some(anchor) = change.right.as_ref()
                && anchor.story == Story::Body
                && let Some(BlockNode::Paragraph(paragraph)) =
                    block_at_path(&self.document, &anchor.story, &anchor.path)
            {
                view.anchors
                    .insert(change.id.clone(), paragraph.id.to_string());
            }
            let kind = match (change.family, change.kind) {
                (_, DiffKind::MoveFrom) => RevisionKind::MoveFrom,
                (DiffFamily::Block, DiffKind::Deletion) => RevisionKind::Deletion,
                _ => {
                    if let Some(edit) =
                        classify_change(&self.document, &notes, change, Some(left), &mut loss)
                    {
                        edits.push(edit);
                    }
                    continue;
                }
            };
            match self.plan_restore(left, &styles, change, kind, author, &date, &mut loss)? {
                Some((container, index, block)) => restores.push(Restore {
                    container,
                    index,
                    order,
                    change: change.id.clone(),
                    block,
                }),
                None => continue,
            }
        }

        let caret = edits.first().map_or_else(
            || Pos::new(self.document.id(), 0),
            |edit| Pos::new(edit.node, edit.start),
        );
        let mut ops: Vec<Operation> = Vec::new();
        if let Some(operation) =
            self.comparison_review_operation(&notes, edits, author, &date, &mut loss)?
        {
            ops.push(operation);
        }
        // Highest position first within each list, so putting one paragraph
        // back never moves the position another was planned against. Restores
        // at the same position go in as ONE insertion, in document order.
        restores.sort_by(|a, b| {
            (a.container, std::cmp::Reverse(a.index), a.order).cmp(&(
                b.container,
                std::cmp::Reverse(b.index),
                b.order,
            ))
        });
        let mut pending: Option<(Option<NodeId>, u32, Vec<BlockNode>)> = None;
        for restore in restores {
            if let BlockNode::Paragraph(paragraph) = &restore.block {
                view.placed.insert(restore.change, paragraph.id.to_string());
            }
            view.restored = view.restored.saturating_add(1);
            match pending.as_mut() {
                Some((container, index, blocks))
                    if *container == restore.container && *index == restore.index =>
                {
                    blocks.push(restore.block);
                }
                _ => {
                    if let Some((container, index, blocks)) = pending.take() {
                        ops.push(Operation::InsertBlocks {
                            container,
                            index,
                            blocks,
                        });
                    }
                    pending = Some((restore.container, restore.index, vec![restore.block]));
                }
            }
        }
        if let Some((container, index, blocks)) = pending {
            ops.push(Operation::InsertBlocks {
                container,
                index,
                blocks,
            });
        }

        if !ops.is_empty() {
            self.apply_action_caret_as(ops, caret, HistoryKind::Review)?;
        }
        view.revision = self.revision;
        view.unmarked = loss.into_iter().collect();
        let named: BTreeSet<&str> = view
            .anchors
            .values()
            .chain(view.placed.values())
            .map(String::as_str)
            .collect();
        let mut order = Vec::new();
        paragraphs_in_order(self.document.body(), &named, &mut order);
        view.order = order;
        serde_json::to_string(&view).map_err(|error| format!("serialize view: {error}"))
    }

    /// Plans one removed paragraph's return: where it goes, and the struck
    /// copy that goes there. `Ok(None)` with a loss key when it cannot be put
    /// back honestly.
    #[allow(clippy::too_many_arguments)]
    fn plan_restore(
        &mut self,
        left: &Document,
        styles: &StyleNames,
        change: &DiffChange,
        kind: RevisionKind,
        author: &str,
        date: &Option<String>,
        loss: &mut BTreeSet<&'static str>,
    ) -> Result<Option<(Option<NodeId>, u32, BlockNode)>, String> {
        let (Some(source), Some(place)) = (change.left.as_ref(), change.place.as_ref()) else {
            loss.insert(if kind == RevisionKind::MoveFrom {
                "trackedMove"
            } else {
                "blockDeletion"
            });
            return Ok(None);
        };
        // Body only, for the reason `classify_change` gives: a header's or a
        // note's position is paired semantically, and this lane has not
        // measured that a restored paragraph lands in the right one.
        if place.story != Story::Body || source.story != Story::Body {
            loss.insert("otherStory");
            return Ok(None);
        }
        let Some(BlockNode::Paragraph(paragraph)) =
            block_at_path(left, &source.story, &source.path)
        else {
            // A removed table, row or content control has no single-paragraph
            // form; it stays in the change list.
            loss.insert("blockDeletion");
            return Ok(None);
        };
        let Some(point) = insertion_point_at_path(&self.document, &place.story, &place.path) else {
            loss.insert("unresolvedAnchor");
            return Ok(None);
        };
        let mut objects = false;
        let inlines = self.restored_inlines(&paragraph.inlines, styles, &mut objects)?;
        if objects {
            loss.insert("removedObject");
        }
        if inlines.is_empty() {
            // An empty paragraph has nothing to strike; it is still in the
            // change list.
            return Ok(None);
        }
        let revision = Revision {
            id: self.fresh_node_id()?,
            kind,
            author: Some(author.to_owned()),
            date: date.clone(),
            revision_id: Some(self.revision_ids.allocate()?),
            editor_group: None,
            inlines,
        };
        let mut properties: ParagraphProperties = (*paragraph.properties).clone();
        properties.style_ref = properties.style_ref.and_then(|id| styles.map(id));
        properties.numbering = None;
        properties.prop_change = None;
        properties.mark_revision = None;
        let block = BlockNode::Paragraph(Paragraph {
            id: self.fresh_node_id()?,
            properties: properties.into(),
            inlines: vec![InlineNode::Revision(Box::new(revision))],
        });
        Ok(Some((point.container, point.index, block)))
    }

    /// The text-bearing inlines of a removed paragraph, with fresh ids and
    /// character styles matched by name. Wrappers (hyperlinks, content
    /// controls, an earlier revision) are flattened to their text; anything
    /// that is not text sets `objects`.
    fn restored_inlines(
        &mut self,
        inlines: &[InlineNode],
        styles: &StyleNames,
        objects: &mut bool,
    ) -> Result<Vec<InlineNode>, String> {
        let mut out = Vec::new();
        for inline in inlines {
            match inline {
                InlineNode::Run(run) => {
                    let mut run = run.clone();
                    run.id = self.fresh_node_id()?;
                    let mut properties: RunProperties = (*run.properties).clone();
                    properties.style_ref = properties.style_ref.and_then(|id| styles.map(id));
                    properties.prop_change = None;
                    run.properties = properties.into();
                    out.push(InlineNode::Run(run));
                }
                InlineNode::Tab(tab) => {
                    let mut tab = tab.clone();
                    tab.id = self.fresh_node_id()?;
                    out.push(InlineNode::Tab(tab));
                }
                InlineNode::Break(item) => {
                    let mut item = item.clone();
                    item.id = self.fresh_node_id()?;
                    out.push(InlineNode::Break(item));
                }
                InlineNode::Hyperlink(link) => {
                    out.extend(self.restored_inlines(&link.inlines, styles, objects)?);
                }
                InlineNode::Sdt(sdt) => {
                    out.extend(self.restored_inlines(&sdt.inlines, styles, objects)?);
                }
                InlineNode::Revision(revision) => {
                    out.extend(self.restored_inlines(&revision.inlines, styles, objects)?);
                }
                // Zero-width markers carry nothing a reader sees.
                InlineNode::BookmarkStart(_)
                | InlineNode::BookmarkEnd(_)
                | InlineNode::CommentRangeStart(_)
                | InlineNode::CommentRangeEnd(_)
                | InlineNode::FieldRangeStart(_)
                | InlineNode::FieldRangeEnd(_)
                | InlineNode::MoveRangeStart(_)
                | InlineNode::MoveRangeEnd(_) => {}
                _ => *objects = true,
            }
        }
        Ok(out)
    }

    fn fresh_node_id(&mut self) -> Result<NodeId, String> {
        self.edit_ids
            .next_id()
            .map_err(|_| "id space exhausted".to_owned())
    }
}

/// The ids of the paragraphs in `named`, in document order, descending into
/// tables and block content controls. **O(blocks)**, once per view.
fn paragraphs_in_order(blocks: &[BlockNode], named: &BTreeSet<&str>, out: &mut Vec<String>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                let id = paragraph.id.to_string();
                if named.contains(id.as_str()) {
                    out.push(id);
                }
            }
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        paragraphs_in_order(&cell.blocks, named, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => paragraphs_in_order(&sdt.blocks, named, out),
            BlockNode::AltChunk(_) => {}
        }
    }
}

/// The older side's paragraph and character styles, matched BY NAME to the
/// newer side's, because a `StyleId` is minted per import and names nothing
/// across two parses. Built once per view: O(styles on both sides).
struct StyleNames {
    to_newer: BTreeMap<StyleId, StyleId>,
}

impl StyleNames {
    fn of(older: &Document, newer: &Document) -> Self {
        let mut by_name: Vec<(StyleKind, &str, StyleId)> = Vec::new();
        for (id, style) in newer.definitions().styles.iter() {
            if let Some(name) = style.name.as_deref() {
                by_name.push((style.kind, name, *id));
            }
        }
        let mut to_newer = BTreeMap::new();
        for (id, style) in older.definitions().styles.iter() {
            let Some(name) = style.name.as_deref() else {
                continue;
            };
            if let Some((_, _, newer_id)) = by_name
                .iter()
                .find(|(kind, candidate, _)| *kind == style.kind && *candidate == name)
            {
                to_newer.insert(*id, *newer_id);
            }
        }
        Self { to_newer }
    }

    /// The newer side's style for an older-side id, when the newer document
    /// defines one of the same kind and name.
    fn map(&self, older: StyleId) -> Option<StyleId> {
        self.to_newer.get(&older).copied()
    }
}

#[cfg(test)]
mod tests {
    use casual_doc_layout::flow::append_node_plain_text;
    use casual_doc_model::v1::{BlockNode, ReviewProjection};

    use crate::WasmDocument;
    use crate::diff::{WasmVersionDiff, begin_version_diff};

    const DATE: &str = "2026-10-09T10:00:00Z";

    /// Plain text is a registered format, so a fixture needs no ZIP.
    fn text(lines: &[&str]) -> Vec<u8> {
        lines.join("\n").into_bytes()
    }

    /// A comparison of `older` against `newer`, driven to completion.
    fn finished(older: &[u8], newer: &[u8]) -> WasmVersionDiff {
        let mut job = begin_version_diff(older.to_vec(), newer.to_vec());
        for _ in 0..64 {
            if job.step_inner(4_000).expect("both sides open") == "complete" {
                return job;
            }
        }
        panic!("the comparison did not finish");
    }

    /// The redline of `older` → `newer`, as the host builds it: a throwaway copy
    /// opened from the newer bytes, with the comparison painted into it.
    fn redline(older: &[u8], newer: &[u8]) -> (WasmDocument, serde_json::Value) {
        let job = finished(older, newer);
        let mut view = crate::open_document(newer).expect("the newer side opens");
        let summary = view
            .show_comparison_inner(&job, "Ann", Some(DATE.to_owned()))
            .expect("the comparison paints");
        (
            view,
            serde_json::from_str(&summary).expect("a JSON summary"),
        )
    }

    /// Every body paragraph's text as one projection sees it.
    fn paragraphs(document: &WasmDocument, projection: ReviewProjection) -> Vec<String> {
        document
            .document
            .body()
            .iter()
            .filter_map(|block| match block {
                BlockNode::Paragraph(paragraph) => {
                    let mut text = String::new();
                    append_node_plain_text(&paragraph.inlines, projection, &mut text);
                    Some(text)
                }
                _ => None,
            })
            .collect()
    }

    fn revisions_of(document: &WasmDocument) -> Vec<serde_json::Value> {
        serde_json::from_str(&document.list_revisions()).expect("a typed revision list")
    }

    /// **The defect ADR-065 exists for.** A whole paragraph that is gone was
    /// reported by Compare as "found, but not marked", because a tracked change
    /// edits a paragraph's inlines and cannot add one. The redline puts it
    /// back, struck, at the position it had — between `one` and `three`.
    #[test]
    fn a_removed_paragraph_is_put_back_struck_where_it_stood() {
        let (view, summary) = redline(&text(&["one", "two", "three"]), &text(&["one", "three"]));
        assert_eq!(
            paragraphs(&view, ReviewProjection::Original),
            vec!["one", "two", "three"],
            "the removed paragraph is back, in place: the older version's reading"
        );
        assert_eq!(
            paragraphs(&view, ReviewProjection::Final),
            vec!["one", "", "three"],
            "and it is a deletion: the newer version's own text is unchanged"
        );
        let revisions = revisions_of(&view);
        assert_eq!(revisions.len(), 1, "one change: {revisions:?}");
        assert_eq!(revisions[0]["kind"], "deletion");
        assert_eq!(revisions[0]["text"], "two");
        assert_eq!(revisions[0]["author"], "Ann");
        assert_eq!(revisions[0]["date"], DATE);
        assert_eq!(summary["restored"], 1);
        assert_eq!(
            summary["order"].as_array().map(Vec::len),
            Some(1),
            "the restored paragraph is the one place to step to: {summary}"
        );
        assert_eq!(summary["unmarked"], serde_json::json!([]));
    }

    /// A moved paragraph is shown at BOTH ends: struck where it was, marked
    /// where it went — the "changes in position" a reader asked to see.
    #[test]
    fn a_moved_paragraph_is_shown_where_it_left_and_where_it_arrived() {
        let (view, _) = redline(
            &text(&["first", "second", "third"]),
            &text(&["second", "third", "first"]),
        );
        assert_eq!(
            paragraphs(&view, ReviewProjection::Original),
            vec!["first", "second", "third", ""],
            "the older version's reading: `first` where it was"
        );
        assert_eq!(
            paragraphs(&view, ReviewProjection::Final),
            vec!["", "second", "third", "first"],
            "the newer version's reading: `first` where it went"
        );
        let kinds: Vec<_> = revisions_of(&view)
            .iter()
            .map(|revision| revision["kind"].as_str().unwrap_or_default().to_owned())
            .collect();
        assert_eq!(
            kinds,
            vec!["move_from", "move_to"],
            "both halves of the move"
        );
    }

    /// Removed text inside a paragraph is shown IN FULL. `applyDiffAsRevisions`
    /// can only use the record's 160-byte excerpt and reports a longer removal
    /// as `truncatedText`; the redline holds the older document and reads it.
    #[test]
    fn removed_text_longer_than_an_excerpt_is_shown_in_full() {
        let removed = "x".repeat(300);
        let older = format!("keep this {removed} and this");
        let (view, summary) = redline(&text(&[&older]), &text(&["keep this  and this"]));
        let revisions = revisions_of(&view);
        assert!(
            revisions
                .iter()
                .any(|revision| revision["kind"] == "deletion"
                    && revision["text"]
                        .as_str()
                        .is_some_and(|text| text.contains(&removed))),
            "the whole removal is shown: {revisions:?}"
        );
        assert!(
            !summary["unmarked"]
                .as_array()
                .expect("a list")
                .iter()
                .any(|key| key == "truncatedText"),
            "and nothing is reported as cut short: {summary}"
        );
    }

    /// The view paints only onto the comparison's own newer side. A handle
    /// holding anything else would have every path resolve to the wrong
    /// paragraph, so it is refused rather than marked wrongly.
    #[test]
    fn a_document_that_is_not_the_newer_side_is_refused() {
        let older = text(&["one", "two"]);
        let job = finished(&older, &text(&["one"]));
        let mut wrong = crate::open_document(&older).expect("opens");
        let error = wrong
            .show_comparison_inner(&job, "Ann", None)
            .expect_err("refused");
        assert!(error.contains("compare.view-mismatch"), "{error}");
        assert!(revisions_of(&wrong).is_empty(), "and nothing was written");
    }

    /// The marks reach the PAGE, not just the model: the markup render of the
    /// redline carries the author's colour (`105` CQ-003 — typed, anchored and
    /// listed is not the same as seen).
    #[test]
    fn the_redline_is_painted_in_the_authors_colour() {
        let (mut view, _) = redline(&text(&["one", "two", "three"]), &text(&["one", "three"]));
        view.set_show_changes_inner(true)
            .expect("the markup view builds");
        let page = view.render_page_inner(0, 96.0).expect("the page renders");
        assert!(
            page.rgba.chunks_exact(4).any(|pixel| pixel[3] > 0
                && (pixel[0].abs_diff(pixel[1]) > 24
                    || pixel[1].abs_diff(pixel[2]) > 24
                    || pixel[0].abs_diff(pixel[2]) > 24)),
            "coloured ink on the page"
        );
    }
}
