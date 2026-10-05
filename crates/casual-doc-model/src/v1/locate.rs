// SPDX-License-Identifier: Apache-2.0

//! Resolving a paragraph by id without walking the document (`docs/109` HF-184).
//!
//! # The defect
//!
//! Every keystroke resolved its paragraph by **two** linear walks of the document:
//! one to find which block surface owns the id (`casual_doc_edit::surface_of`) and
//! one to find the paragraph inside it (`find_paragraph_mut`). Measured on the
//! commit before this one: 35 textual uses of `blocks_owning_mut`, 33 of
//! `find_paragraph_mut`, and **21 places that composed the two into one
//! lookup** — the keystroke shape. The second walk instruments its own cost —
//! `note_blocks(blocks.len())` — so the engine was counting the walk it was doing.
//! On a 1.3-million-paragraph document that is 2.6 million block visits per
//! character typed, and `docs/107` §4 is explicit that per-interaction work is O(1)
//! in document size.
//!
//! # The known pattern, named before the code
//!
//! This is **memoization with validation on use** — a lookup hint, the shape of
//! Linux's `mmap_cache` for VMA lookup and of a B-tree cursor cache. Not an
//! "index": an index has to be *invalidated*, and the row that asked for this said
//! "invalidated at the `apply_group` choke point", which would have cost a full
//! rebuild on the next keystroke after every structural edit. A hint needs no
//! invalidation at all, because it is **re-verified every time it is used**: the
//! route is walked, and the paragraph it lands on must still carry the id that was
//! asked for. A stale hint cannot produce a wrong answer, only a slow one — the
//! walk it replaces runs instead, and the hint is replaced with the truth.
//!
//! That property is why this is safe at a choke point whose blast radius is
//! everything. There is no ordering rule to get wrong, nothing to remember to
//! invalidate, and no operation that has to know this exists. An undo, a remote
//! operation, a rebase and a hand-written `body_mut()` push are all equally
//! covered, because none of them can make a validated route resolve to the wrong
//! paragraph.
//!
//! # Why a route of indices, and what it costs
//!
//! A hint cannot hold a borrow of the paragraph — that would make [`Document`]
//! self-referential — so it holds the **route**: which surface, then the child
//! index at each level down to the paragraph. Resolution is driven by the node
//! kinds it meets rather than by tags in the route, so one flat `[u32]` addresses a
//! body paragraph (one step), a paragraph in a table cell (block, row, cell,
//! block), a paragraph in a text box (block, inline, block) and a paragraph in a
//! text box in a nested shape group, with the inline axis taken from the one
//! declared container set (`super::inline_descent`).
//!
//! The cache is **bounded and direct-mapped**: sixteen entries keyed by
//! `(document id, paragraph id)`, the slot chosen by their low bits, a new entry
//! evicting whatever shared its slot. So it costs about a kilobyte **per thread**,
//! fixed, rather than growing with the document — which matters, because the
//! document this was written for holds 1.3 million paragraphs and a map keyed by
//! every one of them would add more memory than the paragraphs themselves cost
//! after `docs/111`'s interning work. A typing burst touches one or two paragraphs,
//! so a handful of slots is all the hit rate needs. It lives in thread-local
//! storage rather than on the document, for the reasons on `ROUTES`.
//!
//! # Complexity, stated
//!
//! * `hint` / `record` — **O(1)**.
//! * `resolve` / `resolve_mut` — **O(route depth)**, bounded by `MAX_ROUTE_STEPS`,
//!   so O(1) in document size.
//! * `locate` — **O(document)**, once, on a miss. It charges
//!   [`route_block_visits`] so a guard can tell a warm lookup from a cold one
//!   instead of timing them.
//!
//! # What it does NOT resolve
//!
//! Paragraphs in the body, headers, footers, footnotes and endnotes, plus
//! everything nested inside those (tables, block content controls, text boxes,
//! shape groups, inline containers). **Not comments**: `casual_doc_edit`'s own
//! `surface_block_lists` does not list a comment's blocks either, so resolving one
//! here would answer a question the rest of the editing path answers `None` to, and
//! a hint that is more capable than the walk it replaces is a behaviour change
//! wearing a performance change's clothes.

use std::cell::Cell;

use super::{
    BlockNode, Document, GroupChild, HeaderFooterId, InlineDescent, InlineDescentMut, InlineNode,
    NoteId, Paragraph, Table, inline_descent, inline_descent_mut,
};
use crate::NodeId;

/// Entries in the route cache. A power of two, so the slot is the id's low bits.
///
/// Sixteen because a typing burst lives in one paragraph and the widest edit that
/// resolves several at once — a cross-paragraph delete, a join — touches two. The
/// point of the bound is that it is a bound: the alternative is a map keyed by
/// every paragraph in the document, which on the owner's file is 1.3 million
/// entries of pure overhead for a cache whose working set is two.
const SLOTS: usize = 16;

/// The deepest route this cache will record.
///
/// Body paragraph: 1. Table cell: 4 (block, row, cell, block). Text box: 3 (block,
/// inline, block). A paragraph deeper than this is resolved by the full walk every
/// time and is never cached — stated rather than silently mis-cached, and the
/// resolution is correct either way.
const MAX_ROUTE_STEPS: usize = 8;

/// Which block surface a route starts from.
///
/// The same five `casual_doc_edit::Surface` names, kept private here: this is a
/// cache key, and making it public would be a second public spelling of a type that
/// crate already exports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RouteSurface {
    Body,
    Header(HeaderFooterId),
    Footer(HeaderFooterId),
    Footnote(NoteId),
    Endnote(NoteId),
}

/// Where a paragraph was found: its surface, and the child index at each level down
/// to it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ParagraphRoute {
    surface: RouteSurface,
    steps: [u32; MAX_ROUTE_STEPS],
    len: u8,
}

impl ParagraphRoute {
    /// The recorded steps.
    fn steps(&self) -> &[u32] {
        &self.steps[..usize::from(self.len)]
    }
}

/// One cache entry: which document, which paragraph, and the route to it.
type Entry = (NodeId, NodeId, ParagraphRoute);

std::thread_local! {
    /// The route cache — **thread-local, not a field on `Document`**.
    ///
    /// # Why it is not on the document
    ///
    /// It was, first, and a field was worse in two ways that come from the same
    /// place: a cache is not part of a document's VALUE, so the field had to be
    /// excused from `serde`, from `PartialEq` and from `Debug`; and — the part that
    /// decided it — a field can only be written through `&mut Document`, so the
    /// immutable lookup could read a hint and never install one. A read-then-read
    /// pair therefore paid the walk twice, which is exactly the shape of the
    /// review-typing path (the review body is read, then the operation it becomes
    /// reads the same paragraph again). That showed up as a `casual-doc-wasm` guard
    /// measuring three whole-document passes on a path whose own prose claimed two.
    ///
    /// Thread-local storage is writable from `&self`, so **both faces record**, and
    /// `Document` keeps `Send + Sync` — which an embedder needs, because
    /// `Arc<RwLock<Document>>` is `Sync` only if `Document` is. A `Cell` or
    /// `RefCell` field would have bought the same recording and silently taken that
    /// away.
    ///
    /// # Why one cache shared between documents is safe
    ///
    /// Entries are keyed by `(document id, paragraph id)`, so two open documents do
    /// not read each other's routes — and even if they did the answer would still be
    /// right, because a route is **re-verified on every use** and a paragraph that
    /// does not carry the id asked for is a miss, never a wrong answer. Two clones
    /// of one document share a document id and share routes, which is correct (they
    /// are the same tree) and stays correct after either is edited, for the same
    /// reason.
    ///
    /// The only thing sharing costs is capacity: an entry for a dropped document
    /// sits in its slot until something evicts it, bounded by `SLOTS`, which is the
    /// whole point of a fixed-size cache.
    ///
    /// # One `Cell` per slot, not one over the array
    ///
    /// `Cell::get` copies the value it holds, so a single `Cell` over the whole
    /// array would copy EVERY slot on every lookup — the entries are wide (two ids
    /// and a route each) and a keystroke makes several lookups, so that is kilobytes
    /// of memcpy on the one path whose whole claim is that it is O(1) *with a small
    /// constant*. No figure is quoted here because none is derived; per-slot cells
    /// copy one entry and the question does not arise.
    static ROUTES: [Cell<Option<Entry>>; SLOTS] =
        const { [const { Cell::new(None) }; SLOTS] };

    /// Per-thread, so a parallel test run never reads another test's walks.
    static ROUTE_BLOCK_VISITS: Cell<u64> = const { Cell::new(0) };
}

/// The slot a `(document, paragraph)` pair maps to. **O(1)**.
#[allow(clippy::cast_possible_truncation)] // the modulo is the point
fn slot(document: NodeId, id: NodeId) -> usize {
    ((document.as_u128() ^ id.as_u128()) as usize) % SLOTS
}

/// The route recorded for `id` in `document`, if that slot still holds it.
/// **O(1)**.
///
/// A hit is a *hint*, not an answer: the caller resolves it and checks the
/// paragraph it lands on.
fn hint(document: NodeId, id: NodeId) -> Option<ParagraphRoute> {
    match ROUTES.with(|slots| slots[slot(document, id)].get()) {
        Some((owner, cached, route)) if owner == document && cached == id => Some(route),
        _ => None,
    }
}

/// Records a route, evicting whatever shared its slot. **O(1)**.
fn record(document: NodeId, id: NodeId, route: ParagraphRoute) {
    ROUTES.with(|slots| slots[slot(document, id)].set(Some((document, id, route))));
}

/// How many blocks the route locator has examined on this thread since
/// [`reset_route_block_visits`].
///
/// The **complexity meter** for this module, and the companion to
/// `casual_doc_edit::block_visits`: between them they account for every block a
/// position lookup touched. A guard builds documents of `n` and `2n` and asserts
/// that a *warm* keystroke's total does not grow while a *cold* one roughly
/// doubles — which is what proves both that the hint works and that the
/// measurement can see growth at all. A millisecond threshold could do neither.
#[must_use]
pub fn route_block_visits() -> u64 {
    ROUTE_BLOCK_VISITS.with(Cell::get)
}

/// Zeroes the [`route_block_visits`] counter for this thread.
pub fn reset_route_block_visits() {
    ROUTE_BLOCK_VISITS.with(|visits| visits.set(0));
}

/// Charges `n` block visits to this thread's counter.
fn note_blocks(n: usize) {
    ROUTE_BLOCK_VISITS.with(|visits| visits.set(visits.get().saturating_add(n as u64)));
}

impl Document {
    /// The paragraph `id` names, wherever it lives.
    ///
    /// **O(route depth) — O(1) in document size — on a hint hit; O(document) on a
    /// miss, which then records the route.** Both faces record, because the cache is
    /// thread-local rather than a field on this type; see `ROUTES`.
    ///
    /// Resolution covers the body, headers, footers and notes, and everything
    /// nested in them; see this module's header for what it deliberately does not
    /// cover.
    #[must_use]
    pub fn paragraph(&self, id: NodeId) -> Option<&Paragraph> {
        if let Some(route) = hint(self.id(), id)
            && let Some(found) = resolve(self, route.surface, route.steps(), id)
        {
            return Some(found);
        }
        let (surface, steps) = locate(self, id)?;
        if let Some(route) = route_from(surface, &steps) {
            record(self.id(), id, route);
        }
        resolve(self, surface, &steps, id)
    }

    /// The paragraph `id` names, wherever it lives, for in-place mutation.
    ///
    /// **O(route depth) — O(1) in document size — on a hint hit; O(document) on a
    /// miss, which then records the route so the next keystroke in the same
    /// paragraph is a hit.** This is the lookup a keystroke takes (`docs/109`
    /// HF-184): it replaces one whole-surface search for the owning block list plus
    /// a second one for the paragraph inside it.
    pub fn paragraph_mut(&mut self, id: NodeId) -> Option<&mut Paragraph> {
        // Validated immutably first, then re-walked mutably: a route is a handful of
        // indices, so walking it twice is cheaper than the borrow gymnastics that
        // would avoid it — and it keeps the "answer only if the id still matches"
        // check in one place.
        let document = self.id();
        let hinted = hint(document, id)
            .filter(|route| resolve(self, route.surface, route.steps(), id).is_some());
        let (surface, steps) = match hinted {
            Some(route) => (route.surface, route.steps().to_vec()),
            None => {
                let (surface, steps) = locate(self, id)?;
                // Recorded only when the route FITS. A paragraph nested deeper than
                // the cache can encode is resolved by the walk every time, which is
                // what it did before this module existed — and it is still resolved,
                // which is the part that would have been a regression if the cache
                // had been allowed to decide what exists.
                if let Some(route) = route_from(surface, &steps) {
                    record(document, id, route);
                }
                (surface, steps)
            }
        };
        resolve_mut(self, surface, &steps, id)
    }

    /// The block list for one route surface.
    fn route_blocks(&self, surface: RouteSurface) -> Option<&[BlockNode]> {
        match surface {
            RouteSurface::Body => Some(self.body()),
            RouteSurface::Header(key) => self
                .definitions()
                .headers
                .get(&key)
                .map(|header| header.blocks.as_slice()),
            RouteSurface::Footer(key) => self
                .definitions()
                .footers
                .get(&key)
                .map(|footer| footer.blocks.as_slice()),
            RouteSurface::Footnote(key) => self
                .definitions()
                .footnotes
                .get(&key)
                .map(|note| note.blocks.as_slice()),
            RouteSurface::Endnote(key) => self
                .definitions()
                .endnotes
                .get(&key)
                .map(|note| note.blocks.as_slice()),
        }
    }

    /// [`Document::route_blocks`] for mutation.
    fn route_blocks_mut(&mut self, surface: RouteSurface) -> Option<&mut [BlockNode]> {
        match surface {
            RouteSurface::Body => Some(self.body_mut().as_mut_slice()),
            RouteSurface::Header(key) => self
                .definitions_mut()
                .headers
                .get_mut(&key)
                .map(|header| header.blocks.as_mut_slice()),
            RouteSurface::Footer(key) => self
                .definitions_mut()
                .footers
                .get_mut(&key)
                .map(|footer| footer.blocks.as_mut_slice()),
            RouteSurface::Footnote(key) => self
                .definitions_mut()
                .footnotes
                .get_mut(&key)
                .map(|note| note.blocks.as_mut_slice()),
            RouteSurface::Endnote(key) => self
                .definitions_mut()
                .endnotes
                .get_mut(&key)
                .map(|note| note.blocks.as_mut_slice()),
        }
    }
}

/// The paragraph a route leads to, if it still leads to `id`. **O(route depth)**.
///
/// The id check is the whole safety argument: a route recorded before a structural
/// edit may now point at a different paragraph, or at a table, or past the end of a
/// list, and every one of those answers `None` rather than something wrong.
fn resolve<'a>(
    document: &'a Document,
    surface: RouteSurface,
    steps: &[u32],
    id: NodeId,
) -> Option<&'a Paragraph> {
    let blocks = document.route_blocks(surface)?;
    let found = resolve_in_blocks(blocks, steps)?;
    (found.id == id).then_some(found)
}

/// [`resolve`] for a walk that MUTATES what it finds. **O(route depth)**.
fn resolve_mut<'a>(
    document: &'a mut Document,
    surface: RouteSurface,
    steps: &[u32],
    id: NodeId,
) -> Option<&'a mut Paragraph> {
    let blocks = document.route_blocks_mut(surface)?;
    let found = resolve_in_blocks_mut(blocks, steps)?;
    (found.id == id).then_some(found)
}

/// Follows a route through one block list.
///
/// The route carries no tags: what each index means is decided by the node it lands
/// on, which is why one flat `[u32]` addresses a body paragraph, a cell paragraph
/// and a paragraph in a text box in a group. A step that does not fit the node it
/// meets — a table with no row/cell pair left, an index past the end, a paragraph
/// with steps still to spend and no container at that inline — answers `None`,
/// which sends the caller to the full walk.
fn resolve_in_blocks<'a>(blocks: &'a [BlockNode], steps: &[u32]) -> Option<&'a Paragraph> {
    let (&first, rest) = steps.split_first()?;
    match blocks.get(first as usize)? {
        BlockNode::Paragraph(paragraph) if rest.is_empty() => Some(paragraph),
        BlockNode::Paragraph(paragraph) => resolve_in_inlines(&paragraph.inlines, rest),
        BlockNode::Table(table) => {
            let (&row, rest) = rest.split_first()?;
            let (&cell, rest) = rest.split_first()?;
            let cell = table.rows.get(row as usize)?.cells.get(cell as usize)?;
            resolve_in_blocks(&cell.blocks, rest)
        }
        BlockNode::Sdt(sdt) => resolve_in_blocks(&sdt.blocks, rest),
        BlockNode::AltChunk(_) => None,
    }
}

/// [`resolve_in_blocks`] for a walk that MUTATES what it finds.
fn resolve_in_blocks_mut<'a>(
    blocks: &'a mut [BlockNode],
    steps: &[u32],
) -> Option<&'a mut Paragraph> {
    let (&first, rest) = steps.split_first()?;
    match blocks.get_mut(first as usize)? {
        BlockNode::Paragraph(paragraph) => {
            if rest.is_empty() {
                Some(paragraph)
            } else {
                resolve_in_inlines_mut(&mut paragraph.inlines, rest)
            }
        }
        BlockNode::Table(table) => {
            let (&row, rest) = rest.split_first()?;
            let (&cell, rest) = rest.split_first()?;
            let cell = table
                .rows
                .get_mut(row as usize)?
                .cells
                .get_mut(cell as usize)?;
            resolve_in_blocks_mut(&mut cell.blocks, rest)
        }
        BlockNode::Sdt(sdt) => resolve_in_blocks_mut(&mut sdt.blocks, rest),
        BlockNode::AltChunk(_) => None,
    }
}

/// Follows a route through one paragraph's inline list, taking the descent axis from
/// the one declared container set.
fn resolve_in_inlines<'a>(inlines: &'a [InlineNode], steps: &[u32]) -> Option<&'a Paragraph> {
    let (&first, rest) = steps.split_first()?;
    match inline_descent(inlines.get(first as usize)?) {
        InlineDescent::Inlines(nested) => resolve_in_inlines(nested, rest),
        InlineDescent::Blocks(blocks) => resolve_in_blocks(blocks, rest),
        InlineDescent::Group(children) => resolve_in_group(children, rest),
        InlineDescent::Leaf => None,
    }
}

/// [`resolve_in_inlines`] for a walk that MUTATES what it finds.
fn resolve_in_inlines_mut<'a>(
    inlines: &'a mut [InlineNode],
    steps: &[u32],
) -> Option<&'a mut Paragraph> {
    let (&first, rest) = steps.split_first()?;
    match inline_descent_mut(inlines.get_mut(first as usize)?) {
        InlineDescentMut::Inlines(nested) => resolve_in_inlines_mut(nested, rest),
        InlineDescentMut::Blocks(blocks) => resolve_in_blocks_mut(blocks, rest),
        InlineDescentMut::Group(children) => resolve_in_group_mut(children, rest),
        InlineDescentMut::Leaf => None,
    }
}

/// Follows a route through a shape group's children.
fn resolve_in_group<'a>(children: &'a [GroupChild], steps: &[u32]) -> Option<&'a Paragraph> {
    let (&first, rest) = steps.split_first()?;
    match children.get(first as usize)? {
        GroupChild::TextBox(text_box) => resolve_in_blocks(&text_box.blocks, rest),
        GroupChild::Group(nested) => resolve_in_group(&nested.children, rest),
        GroupChild::Picture(_) | GroupChild::Shape(_) => None,
    }
}

/// [`resolve_in_group`] for a walk that MUTATES what it finds.
fn resolve_in_group_mut<'a>(
    children: &'a mut [GroupChild],
    steps: &[u32],
) -> Option<&'a mut Paragraph> {
    let (&first, rest) = steps.split_first()?;
    match children.get_mut(first as usize)? {
        GroupChild::TextBox(text_box) => resolve_in_blocks_mut(&mut text_box.blocks, rest),
        GroupChild::Group(nested) => resolve_in_group_mut(&mut nested.children, rest),
        GroupChild::Picture(_) | GroupChild::Shape(_) => None,
    }
}

/// Where the paragraph `id` names is, by walking the document. **O(document)**.
///
/// Returns the steps it found at whatever depth they are, so resolution never
/// depends on the cache being able to hold them — `route_from` decides separately
/// whether a route is cacheable. Returning a route here instead was a defect found
/// by reading this back: a paragraph nested deeper than `MAX_ROUTE_STEPS` would have
/// been reported as not existing, which is a lookup cache deciding what the document
/// contains.
///
/// Surfaces in the same order `casual_doc_edit`'s own searches try them — body
/// first, because it is the overwhelmingly common case — so a document that somehow
/// held one id twice would resolve to the same paragraph either way. Validation
/// forbids that, which is why this is a note and not a tie-break rule.
fn locate(document: &Document, id: NodeId) -> Option<(RouteSurface, Vec<u32>)> {
    let mut steps = Vec::with_capacity(MAX_ROUTE_STEPS);
    let mut surfaces: Vec<RouteSurface> = vec![RouteSurface::Body];
    let definitions = document.definitions();
    surfaces.extend(
        definitions
            .headers
            .iter()
            .map(|(key, _)| RouteSurface::Header(*key)),
    );
    surfaces.extend(
        definitions
            .footers
            .iter()
            .map(|(key, _)| RouteSurface::Footer(*key)),
    );
    surfaces.extend(
        definitions
            .footnotes
            .iter()
            .map(|(key, _)| RouteSurface::Footnote(*key)),
    );
    surfaces.extend(
        definitions
            .endnotes
            .iter()
            .map(|(key, _)| RouteSurface::Endnote(*key)),
    );
    for surface in surfaces {
        let Some(blocks) = document.route_blocks(surface) else {
            continue;
        };
        steps.clear();
        if locate_in_blocks(blocks, id, &mut steps) {
            return Some((surface, steps));
        }
    }
    None
}

/// A cacheable route, or `None` when the paragraph sits deeper than the cache can
/// address.
///
/// `None` means "not cacheable", never "not found": the caller resolves from the
/// steps either way and simply records nothing, so such a paragraph pays the full
/// walk every time, which is what it did before this module existed. The
/// alternative — truncating the route — would record a hint that resolves to the
/// wrong node, and the id check would reject it on every use: a cache that is
/// always wrong is worse than no cache.
fn route_from(surface: RouteSurface, steps: &[u32]) -> Option<ParagraphRoute> {
    if steps.is_empty() || steps.len() > MAX_ROUTE_STEPS {
        return None;
    }
    let mut recorded = [0u32; MAX_ROUTE_STEPS];
    recorded[..steps.len()].copy_from_slice(steps);
    Some(ParagraphRoute {
        surface,
        steps: recorded,
        len: u8::try_from(steps.len()).ok()?,
    })
}

/// Pushes the route to `id` onto `steps`, or leaves `steps` as it found it.
///
/// **O(blocks in this subtree)**, and charged to [`route_block_visits`] so a guard
/// can tell a cold lookup from a warm one.
fn locate_in_blocks(blocks: &[BlockNode], id: NodeId, steps: &mut Vec<u32>) -> bool {
    note_blocks(blocks.len());
    for (index, block) in blocks.iter().enumerate() {
        let index = match u32::try_from(index) {
            Ok(index) => index,
            // A block list longer than `u32::MAX` cannot be addressed by a route, so
            // the rest of it is left to the full walk rather than mis-addressed.
            Err(_) => return false,
        };
        steps.push(index);
        let found = match block {
            BlockNode::Paragraph(paragraph) => {
                paragraph.id == id || locate_in_inlines(&paragraph.inlines, id, steps)
            }
            BlockNode::Table(table) => locate_in_table(table, id, steps),
            BlockNode::Sdt(sdt) => locate_in_blocks(&sdt.blocks, id, steps),
            BlockNode::AltChunk(_) => false,
        };
        if found {
            return true;
        }
        steps.pop();
    }
    false
}

/// The table half of [`locate_in_blocks`].
fn locate_in_table(table: &Table, id: NodeId, steps: &mut Vec<u32>) -> bool {
    for (row_index, row) in table.rows.iter().enumerate() {
        let Ok(row_index) = u32::try_from(row_index) else {
            return false;
        };
        for (cell_index, cell) in row.cells.iter().enumerate() {
            let Ok(cell_index) = u32::try_from(cell_index) else {
                return false;
            };
            steps.push(row_index);
            steps.push(cell_index);
            if locate_in_blocks(&cell.blocks, id, steps) {
                return true;
            }
            steps.pop();
            steps.pop();
        }
    }
    false
}

/// The inline half of [`locate_in_blocks`]. Descent is the one declared container
/// set, so a paragraph inside a text box inside a content control or a tracked
/// insertion gets a route like any other (HF-194/195's shape).
fn locate_in_inlines(inlines: &[InlineNode], id: NodeId, steps: &mut Vec<u32>) -> bool {
    for (index, inline) in inlines.iter().enumerate() {
        let Ok(index) = u32::try_from(index) else {
            return false;
        };
        steps.push(index);
        let found = match inline_descent(inline) {
            InlineDescent::Inlines(nested) => locate_in_inlines(nested, id, steps),
            InlineDescent::Blocks(blocks) => locate_in_blocks(blocks, id, steps),
            InlineDescent::Group(children) => locate_in_group(children, id, steps),
            InlineDescent::Leaf => false,
        };
        if found {
            return true;
        }
        steps.pop();
    }
    false
}

/// The shape-group half of [`locate_in_blocks`].
fn locate_in_group(children: &[GroupChild], id: NodeId, steps: &mut Vec<u32>) -> bool {
    for (index, child) in children.iter().enumerate() {
        let Ok(index) = u32::try_from(index) else {
            return false;
        };
        steps.push(index);
        let found = match child {
            GroupChild::TextBox(text_box) => locate_in_blocks(&text_box.blocks, id, steps),
            GroupChild::Group(nested) => locate_in_group(&nested.children, id, steps),
            GroupChild::Picture(_) | GroupChild::Shape(_) => false,
        };
        if found {
            return true;
        }
        steps.pop();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::{
        BlockSdt, Definitions, Extent, GroupTextBox, GroupTransform, HeaderFooter, PointEmu, Run,
        RunProperties, SdtProperties, ShapeGeometry, Table, TableCell, TableCellProperties,
        TableProperties, TableRow, TableRowProperties, TextBox, TextBoxBodyProperties,
        WordprocessingGroup,
    };
    use super::{
        BlockNode, Document, GroupChild, HeaderFooterId, InlineNode, Paragraph,
        reset_route_block_visits, route_block_visits,
    };
    use crate::{IdGenerator, NodeId};

    /// Where the fixture put each paragraph, so a test can name a nesting position
    /// rather than an index.
    struct Nested {
        document: Document,
        /// A paragraph directly in the body.
        body: NodeId,
        /// A paragraph in a table cell.
        cell: NodeId,
        /// A paragraph in a block-level content control.
        control: NodeId,
        /// A paragraph in an inline text box.
        text_box: NodeId,
        /// A paragraph in a text box inside a shape group.
        grouped: NodeId,
        /// A paragraph in a header definition.
        header: NodeId,
    }

    /// An id namespace no other fixture in this binary uses.
    ///
    /// The route cache is **thread-local** and `cargo test` reuses threads across
    /// tests, so two fixtures that minted the same document id could read each
    /// other's recorded routes. That can never produce a wrong answer — a route is
    /// re-verified on use — but it could make a lookup a test believes is COLD be
    /// warm, and the cold measurement is what several of these guards rest on. A
    /// unique namespace per fixture removes the question rather than reasoning about
    /// it. libtest spawns a thread per test today — checked with `--test-threads=1`
    /// and with `--nocapture` as well, and the cold measurements hold — so this is
    /// removing a dependency rather than fixing an observed failure, and a runner
    /// that ran tests in-process would not quietly turn a cold lookup warm.
    fn fresh_namespace() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0x1000);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    fn paragraph(id: NodeId, run: NodeId, text: &str) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id,
            properties: super::super::ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Run(Run {
                id: run,
                text: text.to_owned(),
                properties: RunProperties::default().into(),
            })],
        })
    }

    /// A document with a paragraph in every position a route can address.
    ///
    /// Built in code, never from a file: the owner's `.docx` corpus is private, and a
    /// fixture that states its own nesting is also the only readable way to assert
    /// which route a paragraph should get.
    fn nested() -> Nested {
        let mut ids = IdGenerator::new(fresh_namespace());
        let mut next = || ids.next_id().expect("id");
        let document_id = next();
        let extent = Extent {
            width_emu: 914_400,
            height_emu: 914_400,
        };
        let origin = PointEmu { x_emu: 0, y_emu: 0 };

        let body = next();
        let cell_paragraph = next();
        let control_paragraph = next();
        let box_paragraph = next();
        let grouped_paragraph = next();
        let header_paragraph = next();

        let blocks = vec![
            // [0]
            paragraph(body, next(), "body"),
            // [1, row, cell, 0]
            BlockNode::Table(Box::new(Table {
                id: next(),
                grid: Vec::new(),
                grid_change: None,
                properties: TableProperties::default(),
                rows: vec![TableRow {
                    id: next(),
                    properties: TableRowProperties::default(),
                    cells: vec![TableCell {
                        id: next(),
                        properties: TableCellProperties::default(),
                        blocks: vec![paragraph(cell_paragraph, next(), "cell")],
                    }],
                }],
            })),
            // [2, 0]
            BlockNode::Sdt(Box::new(BlockSdt {
                id: next(),
                properties: SdtProperties::default(),
                blocks: vec![paragraph(control_paragraph, next(), "control")],
            })),
            // [3, 1, 0] — the run at inline 0 is deliberate, so the inline step is
            // not zero and a route that ignored it would resolve to the wrong thing.
            BlockNode::Paragraph(Paragraph {
                id: next(),
                properties: super::super::ParagraphProperties::default().into(),
                inlines: vec![
                    InlineNode::Run(Run {
                        id: next(),
                        text: "holder".to_owned(),
                        properties: RunProperties::default().into(),
                    }),
                    InlineNode::TextBox(Box::new(TextBox {
                        hyperlink: None,
                        id: next(),
                        anchor: None,
                        relative_height: None,
                        extent: Some(extent),
                        fill: None,
                        border: None,
                        body_properties: TextBoxBodyProperties::default(),
                        blocks: vec![paragraph(box_paragraph, next(), "boxed")],
                    })),
                ],
            }),
            // [4, 0, 0, 0]
            BlockNode::Paragraph(Paragraph {
                id: next(),
                properties: super::super::ParagraphProperties::default().into(),
                inlines: vec![InlineNode::Group(Box::new(WordprocessingGroup {
                    id: next(),
                    anchor: None,
                    relative_height: None,
                    extent,
                    transform: GroupTransform {
                        offset: origin,
                        extent,
                        child_offset: origin,
                        child_extent: extent,
                        flip_h: false,
                        flip_v: false,
                        rotation: None,
                    },
                    hyperlink: None,
                    children: vec![GroupChild::TextBox(GroupTextBox {
                        hyperlink: None,
                        id: next(),
                        offset: origin,
                        extent,
                        geometry: ShapeGeometry::Rectangle,
                        preset: None,
                        adjustments: Vec::new(),
                        blocks: vec![paragraph(grouped_paragraph, next(), "grouped")],
                        fill: None,
                        border: None,
                        body_properties: TextBoxBodyProperties::default(),
                        flip_h: false,
                        flip_v: false,
                        rotation: None,
                    })],
                }))],
            }),
        ];

        let mut definitions = Definitions::default();
        assert!(
            definitions
                .headers
                .insert(
                    HeaderFooterId::new(next()),
                    HeaderFooter {
                        blocks: vec![paragraph(header_paragraph, next(), "header")],
                    },
                )
                .is_none(),
            "the fixture's header key must be fresh"
        );

        Nested {
            document: Document::new(document_id, blocks, definitions).expect("a valid document"),
            body,
            cell: cell_paragraph,
            control: control_paragraph,
            text_box: box_paragraph,
            grouped: grouped_paragraph,
            header: header_paragraph,
        }
    }

    /// Every nesting position a route can address resolves, read and write.
    ///
    /// The point is not that a lookup is fast; it is that the fast lookup answers the
    /// same question as the walk it replaces. A hint that reached fewer places than
    /// the walk would be a behaviour change wearing a performance change's clothes.
    #[test]
    fn a_paragraph_resolves_wherever_it_is_nested() {
        let mut fixture = nested();
        for (position, id) in [
            ("the body", fixture.body),
            ("a table cell", fixture.cell),
            ("a content control", fixture.control),
            ("an inline text box", fixture.text_box),
            ("a text box in a shape group", fixture.grouped),
            ("a header", fixture.header),
        ] {
            assert_eq!(
                fixture.document.paragraph(id).map(|found| found.id),
                Some(id),
                "a paragraph in {position} must resolve"
            );
            assert_eq!(
                fixture.document.paragraph_mut(id).map(|found| found.id),
                Some(id),
                "a paragraph in {position} must resolve for mutation"
            );
        }
        let absent = NodeId::new(0xdead_beef).expect("a non-zero id");
        assert!(fixture.document.paragraph(absent).is_none());
        assert!(fixture.document.paragraph_mut(absent).is_none());
    }

    /// A second lookup of the same paragraph walks no blocks at all.
    ///
    /// This is the HF-184 guarantee stated as work rather than as milliseconds: the
    /// first lookup pays for the walk, and every lookup after it in the same
    /// paragraph — which is what a typing burst is — examines nothing.
    #[test]
    fn a_repeated_lookup_walks_no_blocks() {
        let mut fixture = nested();
        for (position, id) in [
            ("the body", fixture.body),
            ("a table cell", fixture.cell),
            ("an inline text box", fixture.text_box),
            ("a header", fixture.header),
        ] {
            reset_route_block_visits();
            fixture.document.paragraph_mut(id).expect("resolves cold");
            let cold = route_block_visits();
            assert!(
                cold > 0,
                "the cold lookup for {position} must do the walk this is replacing, or \
                 the measurement is not measuring anything"
            );
            reset_route_block_visits();
            fixture.document.paragraph_mut(id).expect("resolves warm");
            assert_eq!(
                route_block_visits(),
                0,
                "the second lookup for {position} must examine no blocks at all"
            );
        }
    }

    /// The work a warm lookup does is the same at `n` and `2n` blocks, and the
    /// measurement can see growth — so the first half is a result and not a tautology.
    ///
    /// `SKILL` §8: guard complexity by DOUBLING, never by a clock. A millisecond
    /// threshold cannot tell a slow constant from a quadratic and is flaky under load.
    #[test]
    fn lookup_cost_does_not_scale_with_the_document() {
        fn work(blocks: usize) -> (u64, u64) {
            let mut ids = IdGenerator::new(fresh_namespace());
            let document_id = ids.next_id().expect("id");
            let mut body = Vec::new();
            let mut last = None;
            for _ in 0..blocks {
                let id = ids.next_id().expect("id");
                last = Some(id);
                body.push(paragraph(id, ids.next_id().expect("id"), "abcdefgh"));
            }
            // The LAST paragraph, so a linear scan pays its full price rather than
            // returning on the first block.
            let target = last.expect("at least one paragraph");
            let mut document =
                Document::new(document_id, body, Definitions::default()).expect("document");
            reset_route_block_visits();
            document.paragraph_mut(target).expect("resolves cold");
            let cold = route_block_visits();
            reset_route_block_visits();
            document.paragraph_mut(target).expect("resolves warm");
            (cold, route_block_visits())
        }

        let (small_cold, small_warm) = work(256);
        let (large_cold, large_warm) = work(512);
        assert_eq!(
            (small_warm, large_warm),
            (0, 0),
            "a warm lookup must examine no blocks at either size; it read {small_warm} \
             at n and {large_warm} at 2n"
        );
        // And the same measurement, proving it can detect scaling: the COLD lookup is
        // the O(document) walk, and it must roughly double.
        assert!(
            large_cold >= small_cold * 2 - 4,
            "the measurement cannot see growth: a cold lookup went {small_cold} -> \
             {large_cold}, so the warm result above proves nothing"
        );
    }

    /// A paragraph nested deeper than the cache can address still resolves.
    ///
    /// Found by reading this module back rather than by a failure: `locate` used to
    /// hand its caller a *cacheable route* and `route_from` answers `None` beyond
    /// `MAX_ROUTE_STEPS`, so a paragraph nested deeper than the cache can encode was
    /// reported as **not existing** — a lookup cache deciding what the document
    /// contains. Resolution and cacheability are now separate answers, and this is
    /// the guard for the difference.
    ///
    /// The fixture nests tables inside cells until the route is longer than the
    /// cache allows, so the depth is derived from `MAX_ROUTE_STEPS` rather than
    /// guessed at: raising the constant cannot quietly stop this test from testing
    /// anything.
    #[test]
    fn a_paragraph_too_deep_to_cache_still_resolves() {
        let mut ids = IdGenerator::new(fresh_namespace());
        let mut next = || ids.next_id().expect("id");
        let document_id = next();
        let deepest = next();
        // Each table level costs three steps (block, row, cell) plus one for the
        // block inside the cell, so this is comfortably past the bound.
        let levels = super::MAX_ROUTE_STEPS;
        let mut blocks = vec![paragraph(deepest, next(), "the deepest paragraph")];
        for _ in 0..levels {
            blocks = vec![BlockNode::Table(Box::new(Table {
                id: next(),
                grid: Vec::new(),
                grid_change: None,
                properties: TableProperties::default(),
                rows: vec![TableRow {
                    id: next(),
                    properties: TableRowProperties::default(),
                    cells: vec![TableCell {
                        id: next(),
                        properties: TableCellProperties::default(),
                        blocks,
                    }],
                }],
            }))];
        }
        let mut document =
            Document::new(document_id, blocks, Definitions::default()).expect("document");

        assert_eq!(
            document.paragraph(deepest).map(|found| found.id),
            Some(deepest),
            "a paragraph the cache cannot address is still in the document"
        );
        assert_eq!(
            document.paragraph_mut(deepest).map(|found| found.id),
            Some(deepest),
            "and is still editable"
        );
        // And it is honestly uncached: every lookup pays the walk, which is what it
        // did before this module existed.
        reset_route_block_visits();
        document.paragraph_mut(deepest).expect("resolves");
        assert!(
            route_block_visits() > 0,
            "an uncacheable paragraph must keep paying the walk rather than appear to \
             be cached"
        );
    }

    /// A hint that a structural edit has invalidated answers correctly anyway.
    ///
    /// This is the property the whole design rests on, so it is checked rather than
    /// argued: the route is re-verified on every use, so inserting blocks ahead of a
    /// cached paragraph — which shifts its index and makes every recorded step wrong
    /// — cannot make the lookup answer the wrong paragraph. There is no invalidation
    /// to forget, which is why no operation has to know this cache exists.
    #[test]
    fn a_stale_route_answers_correctly_rather_than_wrongly() {
        let mut fixture = nested();
        let target = fixture.body;
        fixture
            .document
            .paragraph_mut(target)
            .expect("resolves, recording the route");

        let mut ids = IdGenerator::new(fresh_namespace());
        let intruder = ids.next_id().expect("id");
        let intruder_run = ids.next_id().expect("id");
        // Straight into the body, behind the cache's back: exactly what a structural
        // operation does, and what an "index invalidated at the choke point" would
        // have to be told about.
        fixture
            .document
            .body_mut()
            .insert(0, paragraph(intruder, intruder_run, "pushed in front"));

        assert_eq!(
            fixture.document.paragraph(target).map(|found| found.id),
            Some(target),
            "the recorded route now points at the intruder, so it must be rejected and \
             re-walked rather than returned"
        );
        assert_eq!(
            fixture.document.paragraph_mut(target).map(|found| found.id),
            Some(target),
            "the same, for the mutable face"
        );
        assert_eq!(
            fixture
                .document
                .paragraph_mut(intruder)
                .map(|found| found.id),
            Some(intruder),
            "and the paragraph that took the cached slot resolves in its own right"
        );
    }

    /// The cache is not part of a document's value.
    ///
    /// Every `assert_eq!` over a `Document` in this workspace depends on this, and a
    /// round trip through JSON must not be able to see it either.
    #[test]
    fn the_route_cache_is_not_part_of_a_documents_value() {
        let mut warm = nested();
        let cold = warm.document.clone();
        warm.document
            .paragraph_mut(warm.body)
            .expect("records a route");
        assert_eq!(
            warm.document, cold,
            "looking a paragraph up must not change what the document IS"
        );
        assert_eq!(
            warm.document.to_json().expect("serializes"),
            cold.to_json().expect("serializes"),
            "and it must not reach the snapshot"
        );
    }

    /// `Document` can still cross a thread.
    ///
    /// A `Cell` or `RefCell` in the route cache would have silently cost `Sync`, and
    /// nothing else in this workspace would have noticed until a host tried to move a
    /// document onto a worker. Stated as a compile-time assertion because that is the
    /// only kind that cannot drift.
    #[test]
    fn a_document_is_still_send_and_sync() {
        const fn require<T: Send + Sync>() {}
        require::<Document>();
    }
}
