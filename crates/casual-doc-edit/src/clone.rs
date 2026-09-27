//! A **total** structural deep copy of block content, with every `NodeId`
//! re-minted (`docs/129` §2).
//!
//! # Why this exists as its own module
//!
//! There used to be exactly one "clone a block tree with fresh ids" in the tree:
//! the structured clipboard's reconstructor in `casual-doc-wasm`, whose
//! `sanitize_inlines` was a `match` over four [`InlineNode`] variants with a
//! `_ => {}` arm. `InlineNode` has **29** variants, so **25** of them — every
//! picture, every field, text boxes, groups, embedded objects, math, symbols,
//! inline content controls, tracked changes, and every reference marker — were
//! discarded on the way through. Nothing refused and nothing reported, which is
//! the one failure mode `AGENTS.md` names as a hard rule rather than a
//! preference.
//!
//! The copy therefore does not belong in a sanitizer and must not be built by
//! calling one. It lives here, beside the operations that consume it, and it is
//! **total**: the inline match has **no catch-all**, so adding a 30th
//! `InlineNode` variant is a compile error in this file rather than a new silent
//! drop. A catch-all plus a comment is exactly how the previous helper came to
//! drop 25 kinds without anybody deciding to.
//!
//! # The three outcomes, and why each variant gets the one it does
//!
//! A same-document copy cannot simply duplicate every reference: some references
//! are *unique by construction*, and a second copy of one is either an invalid
//! document or an incoherent one.
//!
//! **Carried** — everything self-contained, and everything whose reference is to
//! a *shared, immutable definition* that a second referrer does not disturb:
//! runs, tabs, breaks, hyperlinks, drawings and anchored drawings (a
//! `MediaId` into `definitions.media`), embedded objects (a preserved package
//! part), fields, text boxes, groups, inline content controls, tracked-change
//! ranges, note auto-number marks, math, symbols, horizontal rules, the two
//! hyphens, and positional tabs. These are the kinds that carry **ink**.
//!
//! **Degraded, and counted in a [`CloneReport`]** — the five families whose
//! reference is unique *per range*, where a duplicate is a defect rather than a
//! copy. The marker is dropped; it carries no ink, so the surrounding text and
//! objects still arrive:
//!
//! | Family | Why a duplicate is wrong |
//! | --- | --- |
//! | bookmark markers | A bookmark name is unique per document; two ranges under one `BookmarkId` make `REF`/`PAGEREF` ambiguous. Word drops bookmarks on a same-document paste for this reason. |
//! | comment reference / range markers | One comment has one anchored range; a second anchor exports two `w:commentRangeStart` with one id. |
//! | note references | One footnote body cannot be referenced twice. Word *duplicates the note*, which needs a new note definition — not something a structural clone can mint. |
//! | field-range markers | A duplicate is rejected outright by `Document::validate` (`DuplicateFieldRangeMarker`), so carrying one produces a document that cannot be saved. The cached result *between* the markers is ordinary inline content and is carried, so the text survives and only its field-ness is lost. |
//! | tracked-move range markers | A move pairs one source with one destination by `move_id`; a second destination is incoherent review state. |
//!
//! Carrying any of those five faithfully means **minting a new definition**, which
//! is a definitions mutation and a different operation. Until one exists, the
//! caller is told what was degraded and reports it — which is the difference
//! between a documented bound and silent loss.
//!
//! **Refused** — nothing. Every [`BlockNode`] and every [`InlineNode`] variant
//! either arrives or is counted.
//!
//! # Complexity
//!
//! `O(size of the copied fragment)` — one pass, no lookups against the
//! destination document, and therefore independent of document size. That is what
//! lets a paste stay an `O(1)`-in-document-size interaction (`docs/107` §4).

use crate::RunIds;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, GroupChild, InlineNode, Paragraph, ParagraphProperties, Table, TableCell, TableRow,
};

/// What a [`clone_blocks_with_fresh_ids`] call could not duplicate, by family.
///
/// Counts of *markers dropped*, not of characters: every one of these carries no
/// ink, so the text and objects around it arrived. Empty means the copy was
/// faithful.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CloneReport {
    /// `BookmarkStart` / `BookmarkEnd` markers dropped.
    pub bookmarks: u32,
    /// `CommentReference` / `CommentRangeStart` / `CommentRangeEnd` dropped.
    pub comments: u32,
    /// `NoteReference` (footnote or endnote) markers dropped.
    pub notes: u32,
    /// `FieldRangeStart` / `FieldRangeEnd` markers dropped.
    pub field_ranges: u32,
    /// `MoveRangeStart` / `MoveRangeEnd` markers dropped.
    pub tracked_moves: u32,
}

impl CloneReport {
    /// The copy was faithful: nothing was degraded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }

    /// Every dropped marker, across the families.
    #[must_use]
    pub fn total(&self) -> u32 {
        self.bookmarks
            .saturating_add(self.comments)
            .saturating_add(self.notes)
            .saturating_add(self.field_ranges)
            .saturating_add(self.tracked_moves)
    }

    /// The stable keys of the families that lost something, in a fixed order so a
    /// host message reads the same way twice. A host maps each key to its own
    /// localized noun phrase; the engine does not hold user-facing prose.
    #[must_use]
    pub fn kinds(&self) -> Vec<&'static str> {
        [
            (self.bookmarks, "bookmark"),
            (self.comments, "comment"),
            (self.notes, "note"),
            (self.field_ranges, "fieldRange"),
            (self.tracked_moves, "trackedMove"),
        ]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(_, key)| key)
        .collect()
    }
}

/// Deep-copies `blocks`, re-minting every `NodeId` from `ids` and recording in
/// `report` the reference markers that could not be duplicated (see the module
/// documentation for the per-variant decision and its reason).
///
/// `None` only when the id space is exhausted; the partial work is discarded by
/// the caller dropping the result.
///
/// `O(size of blocks)`.
pub fn clone_blocks_with_fresh_ids(
    blocks: &[BlockNode],
    ids: &mut dyn RunIds,
    report: &mut CloneReport,
) -> Option<Vec<BlockNode>> {
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        out.push(clone_block_with_fresh_ids(block, ids, report)?);
    }
    Some(out)
}

/// One block, as [`clone_blocks_with_fresh_ids`] does the list.
///
/// Total over `BlockNode`'s four variants with no catch-all, for the same reason
/// the inline match has none: the previous clipboard helper *rejected* the `Sdt`
/// block outright, so copying a table with a content control in a cell failed the
/// whole paste.
pub fn clone_block_with_fresh_ids(
    block: &BlockNode,
    ids: &mut dyn RunIds,
    report: &mut CloneReport,
) -> Option<BlockNode> {
    match block {
        BlockNode::Paragraph(paragraph) => {
            let mut cloned = paragraph.clone();
            cloned.id = ids.next()?;
            cloned.inlines = clone_inlines(&paragraph.inlines, ids, report)?;
            Some(BlockNode::Paragraph(cloned))
        }
        BlockNode::Table(table) => {
            Some(BlockNode::Table(Box::new(clone_table(table, ids, report)?)))
        }
        BlockNode::Sdt(sdt) => {
            let mut cloned = sdt.clone();
            cloned.id = ids.next()?;
            cloned.blocks = clone_blocks_with_fresh_ids(&sdt.blocks, ids, report)?;
            Some(BlockNode::Sdt(cloned))
        }
        // An alt-chunk names a preserved package part and holds no node children
        // of its own, so a fresh id is the whole copy.
        BlockNode::AltChunk(chunk) => {
            let mut cloned = chunk.clone();
            cloned.id = ids.next()?;
            Some(BlockNode::AltChunk(cloned))
        }
    }
}

/// A table with fresh ids on the table, every row, every cell, and every block
/// inside every cell. A cell's block list must stay non-empty, so a cell that
/// somehow arrives empty is given one empty paragraph rather than being written
/// out invalid.
fn clone_table(table: &Table, ids: &mut dyn RunIds, report: &mut CloneReport) -> Option<Table> {
    let mut rows = Vec::with_capacity(table.rows.len());
    for row in &table.rows {
        let mut cells = Vec::with_capacity(row.cells.len());
        for cell in &row.cells {
            let mut blocks = clone_blocks_with_fresh_ids(&cell.blocks, ids, report)?;
            if blocks.is_empty() {
                blocks.push(BlockNode::Paragraph(Paragraph {
                    id: ids.next()?,
                    properties: ParagraphProperties::default().into(),
                    inlines: Vec::new(),
                }));
            }
            cells.push(TableCell {
                id: ids.next()?,
                properties: cell.properties.clone(),
                blocks,
            });
        }
        rows.push(TableRow {
            id: ids.next()?,
            properties: row.properties.clone(),
            cells,
        });
    }
    let mut cloned = table.clone();
    cloned.id = ids.next()?;
    cloned.rows = rows;
    Some(cloned)
}

/// Rebuilds an inline sequence with fresh ids.
///
/// **This match must stay exhaustive with no catch-all.** It is the guard that
/// makes a 30th `InlineNode` variant a compile error here instead of a silent
/// drop in a paste, and `casual-doc-edit`'s own source guard fails the build if a
/// `_ =>` arm appears in this file.
///
/// A wrapper (hyperlink, tracked-change range, inline content control) whose
/// content rebuilds to nothing is dropped rather than written out empty: the
/// model rejects an empty revision and an empty inline SDT, and a hyperlink with
/// no content has nothing to click. That only happens when a wrapper held
/// *nothing but* degraded markers, which the report has already counted.
fn clone_inlines(
    inlines: &[InlineNode],
    ids: &mut dyn RunIds,
    report: &mut CloneReport,
) -> Option<Vec<InlineNode>> {
    let mut out = Vec::with_capacity(inlines.len());
    for inline in inlines {
        match inline {
            // --- Self-contained leaves: a fresh id is the whole copy. ---------
            InlineNode::Run(run) => {
                let mut cloned = run.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::Run(cloned));
            }
            InlineNode::Tab(tab) => {
                let mut cloned = tab.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::Tab(cloned));
            }
            InlineNode::Break(node) => {
                let mut cloned = node.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::Break(cloned));
            }
            // A picture. Its `MediaId` names a shared, immutable entry in
            // `definitions.media`: a second referrer disturbs nothing, and this is
            // the single most-copied object in a document.
            InlineNode::Drawing(drawing) => {
                let mut cloned = drawing.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::Drawing(cloned));
            }
            InlineNode::AnchoredDrawing(drawing) => {
                let mut cloned = drawing.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::AnchoredDrawing(cloned));
            }
            // A chart / SmartArt / OLE object, naming preserved package parts —
            // shared and immutable, exactly like media.
            InlineNode::EmbeddedObject(object) => {
                let mut cloned = object.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::EmbeddedObject(cloned));
            }
            // The note's own auto-number mark is an inert leaf carrying only run
            // formatting; it resolves against no definition, so it copies.
            InlineNode::NoteNumberMark(mark) => {
                let mut cloned = mark.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::NoteNumberMark(cloned));
            }
            InlineNode::Math(math) => {
                let mut cloned = math.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::Math(cloned));
            }
            InlineNode::Symbol(symbol) => {
                let mut cloned = symbol.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::Symbol(cloned));
            }
            InlineNode::HorizontalRule(rule) => {
                let mut cloned = *rule;
                cloned.id = ids.next()?;
                out.push(InlineNode::HorizontalRule(cloned));
            }
            InlineNode::NoBreakHyphen(hyphen) => {
                let mut cloned = hyphen.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::NoBreakHyphen(cloned));
            }
            InlineNode::SoftHyphen(hyphen) => {
                let mut cloned = hyphen.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::SoftHyphen(cloned));
            }
            InlineNode::PositionalTab(tab) => {
                let mut cloned = tab.clone();
                cloned.id = ids.next()?;
                out.push(InlineNode::PositionalTab(cloned));
            }

            // --- Wrappers: fresh id, children rebuilt. ------------------------
            InlineNode::Hyperlink(link) => {
                let inner = clone_inlines(&link.inlines, ids, report)?;
                if inner.is_empty() {
                    continue;
                }
                let mut cloned = link.clone();
                cloned.id = ids.next()?;
                cloned.inlines = inner;
                out.push(InlineNode::Hyperlink(cloned));
            }
            // A field's instruction and its cached result. A `REF`/`PAGEREF` whose
            // bookmark was not part of the copy still resolves against the
            // ORIGINAL bookmark in the same document, so nothing dangles — and
            // Word likewise copies the field rather than flattening it.
            InlineNode::Field(field) => {
                let mut cloned = field.clone();
                cloned.id = ids.next()?;
                cloned.inlines = clone_inlines(&field.inlines, ids, report)?;
                out.push(InlineNode::Field(cloned));
            }
            // A tracked insertion/deletion. `revision_id` is the producer's own
            // `w:id` annotation and is carried VERBATIM: it is authored data, the
            // schema does not require it to be unique, and clearing it would lose
            // something to avoid a collision that costs nothing.
            InlineNode::Revision(revision) => {
                let inner = clone_inlines(&revision.inlines, ids, report)?;
                if inner.is_empty() {
                    continue;
                }
                let mut cloned = revision.clone();
                cloned.id = ids.next()?;
                cloned.inlines = inner;
                out.push(InlineNode::Revision(cloned));
            }
            InlineNode::Sdt(sdt) => {
                let inner = clone_inlines(&sdt.inlines, ids, report)?;
                if inner.is_empty() {
                    continue;
                }
                let mut cloned = sdt.clone();
                cloned.id = ids.next()?;
                cloned.inlines = inner;
                out.push(InlineNode::Sdt(cloned));
            }
            // A text box's blocks are a container of their own and go through the
            // block clone, so a paragraph inside a copied text box is as fresh as
            // one in the body.
            InlineNode::TextBox(text_box) => {
                let blocks = clone_blocks_with_fresh_ids(&text_box.blocks, ids, report)?;
                let mut cloned = text_box.clone();
                cloned.id = ids.next()?;
                cloned.blocks = blocks;
                out.push(InlineNode::TextBox(cloned));
            }
            InlineNode::Group(group) => {
                let mut cloned = group.clone();
                cloned.id = ids.next()?;
                cloned.children = clone_group_children(&group.children, ids, report)?;
                out.push(InlineNode::Group(cloned));
            }

            // --- Degraded: a unique reference a second copy would break. ------
            // See the table in the module documentation for why each of these
            // cannot be duplicated, and what the caller is expected to do with the
            // report.
            InlineNode::BookmarkStart(_) | InlineNode::BookmarkEnd(_) => {
                report.bookmarks = report.bookmarks.saturating_add(1);
            }
            InlineNode::CommentReference(_)
            | InlineNode::CommentRangeStart(_)
            | InlineNode::CommentRangeEnd(_) => {
                report.comments = report.comments.saturating_add(1);
            }
            InlineNode::NoteReference(_) => {
                report.notes = report.notes.saturating_add(1);
            }
            InlineNode::FieldRangeStart(_) | InlineNode::FieldRangeEnd(_) => {
                report.field_ranges = report.field_ranges.saturating_add(1);
            }
            InlineNode::MoveRangeStart(_) | InlineNode::MoveRangeEnd(_) => {
                report.tracked_moves = report.tracked_moves.saturating_add(1);
            }
        }
    }
    Some(out)
}

/// A group's children, recursively: a picture, a text box (whose blocks are their
/// own container), a shape, or a nested group. Total over `GroupChild` for the
/// same reason as the inline match — every one of these ids is recorded by
/// `Document::validate`'s uniqueness pass, so a missed one is a duplicate-id
/// document.
fn clone_group_children(
    children: &[GroupChild],
    ids: &mut dyn RunIds,
    report: &mut CloneReport,
) -> Option<Vec<GroupChild>> {
    let mut out = Vec::with_capacity(children.len());
    for child in children {
        out.push(match child {
            GroupChild::Picture(picture) => {
                let mut cloned = picture.clone();
                cloned.id = ids.next()?;
                GroupChild::Picture(cloned)
            }
            GroupChild::TextBox(text_box) => {
                let blocks = clone_blocks_with_fresh_ids(&text_box.blocks, ids, report)?;
                let mut cloned = text_box.clone();
                cloned.id = ids.next()?;
                cloned.blocks = blocks;
                GroupChild::TextBox(cloned)
            }
            GroupChild::Shape(shape) => {
                let mut cloned = shape.clone();
                cloned.id = ids.next()?;
                GroupChild::Shape(cloned)
            }
            GroupChild::Group(nested) => {
                let mut cloned = nested.clone();
                cloned.id = ids.next()?;
                cloned.children = clone_group_children(&nested.children, ids, report)?;
                GroupChild::Group(cloned)
            }
        });
    }
    Some(out)
}

/// Every `NodeId` in a block tree, in document order — the sibling of
/// `Document::validate`'s uniqueness pass, for a caller that wants to assert a
/// clone shares no identity with its source.
///
/// `O(size of blocks)`.
#[must_use]
pub fn node_ids_of_blocks(blocks: &[BlockNode]) -> Vec<NodeId> {
    let mut out = Vec::new();
    collect_block_ids(blocks, &mut out);
    out
}

fn collect_block_ids(blocks: &[BlockNode], out: &mut Vec<NodeId>) {
    for block in blocks {
        match block {
            BlockNode::Paragraph(paragraph) => {
                out.push(paragraph.id);
                collect_inline_ids(&paragraph.inlines, out);
            }
            BlockNode::Table(table) => {
                out.push(table.id);
                for row in &table.rows {
                    out.push(row.id);
                    for cell in &row.cells {
                        out.push(cell.id);
                        collect_block_ids(&cell.blocks, out);
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                out.push(sdt.id);
                collect_block_ids(&sdt.blocks, out);
            }
            BlockNode::AltChunk(chunk) => out.push(chunk.id),
        }
    }
}

fn collect_inline_ids(inlines: &[InlineNode], out: &mut Vec<NodeId>) {
    for inline in inlines {
        out.push(inline.id());
        match inline {
            InlineNode::Hyperlink(link) => collect_inline_ids(&link.inlines, out),
            InlineNode::Field(field) => collect_inline_ids(&field.inlines, out),
            InlineNode::Revision(revision) => collect_inline_ids(&revision.inlines, out),
            InlineNode::Sdt(sdt) => collect_inline_ids(&sdt.inlines, out),
            InlineNode::TextBox(text_box) => collect_block_ids(&text_box.blocks, out),
            InlineNode::Group(group) => collect_group_ids(&group.children, out),
            // container-set: this walk names all six containers — `Hyperlink`,
            // `Field`, `Revision`, `Sdt`, `TextBox`, `Group` — above, so the
            // catch-all covers leaves only. It is kept as a catch-all rather than
            // 23 named arms because this is a read-only traversal, not a rebuild: a
            // new variant reaching it is still VISITED (its own id is pushed above)
            // and loses nothing, whereas in `clone_inlines` a catch-all is the
            // defect itself.
            _ => {}
        }
    }
}

fn collect_group_ids(children: &[GroupChild], out: &mut Vec<NodeId>) {
    for child in children {
        match child {
            GroupChild::Picture(picture) => out.push(picture.id),
            GroupChild::TextBox(text_box) => {
                out.push(text_box.id);
                collect_block_ids(&text_box.blocks, out);
            }
            GroupChild::Shape(shape) => out.push(shape.id),
            GroupChild::Group(nested) => {
                out.push(nested.id);
                collect_group_ids(&nested.children, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rule this module exists to enforce, enforced on the module itself.
    ///
    /// The exhaustive `match` in `clone_inlines` is already a compile error when a
    /// 30th `InlineNode` variant appears — but the cheapest way to "fix" that
    /// compile error is to add a `_ => {}`, which is precisely the defect
    /// (`docs/129` §2). So the rebuild functions are read from source and a
    /// catch-all in one of them fails the build.
    ///
    /// `node_ids_of_blocks`'s read-only traversal is allowed one, and is named
    /// here rather than matched loosely so the exemption cannot spread.
    #[test]
    fn the_rebuild_functions_carry_no_catch_all_arm() {
        let source = include_str!("clone.rs").replace("\r\n", "\n");
        let tests_at = source
            .find("\n#[cfg(test)]\nmod tests {")
            .expect("the test module marker");
        let production = &source[..tests_at];

        // The one read-only traversal that may hold a catch-all, excised by name.
        let exempt_from = production
            .find("fn collect_inline_ids(")
            .expect("the read-only inline traversal");
        let exempt_to = production[exempt_from..]
            .find("\nfn collect_group_ids(")
            .map(|offset| exempt_from + offset)
            .expect("the traversal's end");
        let mut rebuild = String::with_capacity(production.len());
        rebuild.push_str(&production[..exempt_from]);
        rebuild.push_str(&production[exempt_to..]);
        // Comment lines are dropped before the scan, or this file's own prose
        // about `_ => {}` would fail the guard it is explaining. Dropping them
        // also means the rule cannot be commented out to satisfy it.
        let code: String = rebuild
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        for arm in ["_ =>", "_=>", ".. =>"] {
            assert!(
                !code.contains(arm),
                "`{arm}` in a rebuild function is how 25 inline kinds came to be \
                 dropped silently; name every variant instead"
            );
        }
    }

    #[test]
    fn the_report_names_only_the_families_that_lost_something() {
        let mut report = CloneReport::default();
        assert!(report.is_empty());
        assert!(report.kinds().is_empty());
        report.comments = 2;
        report.bookmarks = 1;
        assert!(!report.is_empty());
        assert_eq!(report.total(), 3);
        assert_eq!(report.kinds(), vec!["bookmark", "comment"]);
    }
}
