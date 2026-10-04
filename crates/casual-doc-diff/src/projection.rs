//! The semantic projection: one document reduced to a hash forest that
//! alignment can compare without touching the document again.
//!
//! # The pattern
//!
//! This is a **Merkle (hash) tree** over the document's ordered container
//! forest. Each projected block carries hashes of its own content and of its
//! whole subtree; a parent's subtree hash folds its children's in order. Two
//! subtrees with the same hash are the same subtree, so an unchanged table of
//! 40,000 cells costs **one comparison** rather than a walk — the same
//! short-circuit `git diff` gets from identical tree objects.
//!
//! # Why the projection is separate from the document
//!
//! Alignment compares keys millions of times and must not chase pointers
//! through a 300-byte property struct to do it. The projection is a flat
//! `Vec` of 96-byte records in breadth-first order, which also makes the
//! alignment work queue and the projection walk the same queue.
//!
//! # Breadth-first, not depth-first
//!
//! A block's children are **contiguous** in [`Projection::blocks`], which costs
//! nothing to store (`first_child`, `child_count`) and needs no per-block `Vec`.
//! That is only true in breadth-first order; depth-first interleaves
//! grandchildren. Document order is recovered from the parent chain when
//! records are emitted, which is O(depth) on a bounded number of records rather
//! than O(n) storage on every one.
//!
//! # Complexity
//!
//! Projecting is **O(b + t + p)** time and **O(b)** memory, where `b` is blocks,
//! `t` is text bytes and `p` is *distinct* property values — not nodes. Property
//! and object hashes are memoized by flyweight identity ([`ValueHashes`]), which
//! is what keeps a 1.3-million-paragraph plain-text document from serializing
//! 1.3 million identical `ParagraphProperties` (`docs/111` §4: that document has
//! exactly one distinct value).

use std::collections::HashMap;
use std::collections::VecDeque;

use casual_doc_layout::flow::append_node_plain_text;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Document, HeaderFooterKind, InlineNode, ReviewProjection, Revision, TableCell,
    TableRow,
};
use serde::Serialize;

use crate::hash::{ContentHasher, hash_str};
use crate::record::{PathSegment, SideSummary, Story};

/// The review projection a version diff reads.
///
/// `FinalWithMarkup` is the runtime projection (`docs/133` decision 1), so the
/// diff compares **what the editor shows**. A pending tracked change that is
/// accepted between two versions therefore leaves the projected text identical;
/// that is not a missed change, it is a [`crate::record::DiffFamily::Review`]
/// change, which is why every block also carries a review hash.
const DIFF_REVIEW_PROJECTION: ReviewProjection = ReviewProjection::FinalWithMarkup;

/// What kind of node a projected block is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKey {
    /// A paragraph.
    Paragraph,
    /// A table.
    Table,
    /// A table row.
    Row,
    /// A table cell.
    Cell,
    /// A block-level content control.
    Sdt,
    /// An aggregated external content chunk.
    AltChunk,
}

impl BlockKey {
    /// The hash tag. Distinct per kind so a paragraph and a cell holding the
    /// same text never share an alignment key.
    const fn tag(self) -> u8 {
        match self {
            Self::Paragraph => 1,
            Self::Table => 2,
            Self::Row => 3,
            Self::Cell => 4,
            Self::Sdt => 5,
            Self::AltChunk => 6,
        }
    }

    /// The path segment a child of this kind contributes.
    const fn segment(self, index: u32) -> PathSegment {
        match self {
            Self::Row => PathSegment::Row { index },
            Self::Cell => PathSegment::Cell { index },
            _ => PathSegment::Block { index },
        }
    }
}

/// Sentinel parent for a story's root blocks.
pub const NO_PARENT: u32 = u32::MAX;

/// One projected block.
#[derive(Clone, Copy, Debug)]
pub struct ProjectedBlock {
    /// The node's stable identity **within this parsed state**.
    pub node: NodeId,
    /// What it is.
    pub kind: BlockKey,
    /// Index into [`Projection::stories`].
    pub story: u32,
    /// Index of the containing block, or [`NO_PARENT`].
    pub parent: u32,
    /// Position among its siblings.
    pub sibling_index: u32,
    /// First child's index in [`Projection::blocks`].
    pub first_child: u32,
    /// How many children.
    pub child_count: u32,
    /// Hash of the projected plain text.
    pub text_hash: u128,
    /// Hash of paragraph/row/cell/table properties and of the ordered character
    /// formatting of the block's runs.
    pub format_hash: u128,
    /// Hash of the non-text inline objects: drawings, fields, math, symbols,
    /// embedded objects, bookmarks and the rest.
    pub object_hash: u128,
    /// Hash of the pending tracked-change structure.
    pub review_hash: u128,
    /// Hash of everything above. Two blocks with equal `own_key` differ only in
    /// their children.
    pub own_key: u128,
    /// `own_key` folded with every descendant's `subtree_hash`, in order.
    pub subtree_hash: u128,
}

/// One story and the range of [`Projection::blocks`] holding its root blocks.
#[derive(Clone, Debug)]
pub struct StorySlot {
    /// How a host names it.
    pub story: Story,
    /// The key this story is paired on between the two sides. See
    /// `crate::stories`.
    pub key: StoryKey,
    /// First root block index.
    pub first_root: u32,
    /// How many root blocks.
    pub root_count: u32,
    /// The story's own subtree hash: its roots folded in order.
    pub content_hash: u128,
}

/// The key two stories are paired on across two independently parsed documents.
///
/// **None of these is a `NodeId`.** Every definition id in this model wraps a
/// `NodeId` minted by the importer's counter, so `HeaderFooterId(7)` means "the
/// seventh id this parse handed out" and nothing else. What is stable across two
/// files is a header's *semantic position* — which section, which page type — and
/// a comment's durable id, which Word writes into the package.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StoryKey {
    /// The body.
    Body,
    /// A header: section ordinal plus page type.
    Header(u32, u8),
    /// A footer: section ordinal plus page type.
    Footer(u32, u8),
    /// A footnote, by ordinal until content pairing overrides it.
    Footnote(u32),
    /// An endnote, by ordinal.
    Endnote(u32),
    /// A comment, by durable id when the package carries one.
    Comment(String),
}

/// The projection of one document.
#[derive(Clone, Debug, Default)]
pub struct Projection {
    /// Every block, breadth-first within each story.
    pub blocks: Vec<ProjectedBlock>,
    /// Every story.
    pub stories: Vec<StorySlot>,
    /// What a host shows without walking anything.
    pub summary: SideSummary,
}

impl Projection {
    /// The container path from the story root down to `index`. **O(depth)**.
    #[must_use]
    pub fn path(&self, index: u32) -> Vec<PathSegment> {
        let mut segments = Vec::new();
        let mut cursor = index;
        while let Some(block) = self.blocks.get(cursor as usize) {
            segments.push(block.kind.segment(block.sibling_index));
            if block.parent == NO_PARENT {
                break;
            }
            cursor = block.parent;
        }
        segments.reverse();
        segments
    }

    /// The document-order sort key of `index`: the story ordinal followed by the
    /// sibling index at each depth. **O(depth)**.
    #[must_use]
    pub fn order_key(&self, index: u32) -> (u32, Vec<u32>) {
        let mut indices = Vec::new();
        let mut cursor = index;
        let mut story = 0;
        while let Some(block) = self.blocks.get(cursor as usize) {
            indices.push(block.sibling_index);
            story = block.story;
            if block.parent == NO_PARENT {
                break;
            }
            cursor = block.parent;
        }
        indices.reverse();
        (story, indices)
    }

    /// The children of `index` as a slice of indices.
    #[must_use]
    pub fn children(&self, index: u32) -> std::ops::Range<u32> {
        self.blocks.get(index as usize).map_or(0..0, |block| {
            block.first_child..block.first_child.saturating_add(block.child_count)
        })
    }
}

/// Hashes of property and object values, memoized by flyweight identity.
///
/// # Why an address is a legitimate key
///
/// `Paragraph::properties` is a `Shared<ParagraphProperties>` — an `Arc` into the
/// model's interning table, where every paragraph with default formatting shares
/// **one** entry (`v1::intern`). The address of that entry is therefore a perfect
/// hash of the value for as long as the entry is alive, and it is alive for the
/// whole diff because the diff borrows the document. So the expensive part
/// (serializing a 304-byte struct to name its fields) happens once per *distinct*
/// value instead of once per node.
///
/// This is the flyweight being used, not re-invented: the memo exists because the
/// model already normalized these values.
#[derive(Debug, Default)]
pub struct ValueHashes {
    by_address: HashMap<usize, u128>,
    /// How many values were actually serialized.
    pub serialized: u64,
}

impl ValueHashes {
    /// The hash of a value reached through a flyweight handle, computed once per
    /// distinct entry. **O(1)** amortized after the first sighting.
    pub fn hash_shared<T: Serialize>(&mut self, value: &T) -> u128 {
        let address = std::ptr::from_ref(value) as usize;
        if let Some(found) = self.by_address.get(&address) {
            return *found;
        }
        let hash = self.hash_value(value);
        self.by_address.insert(address, hash);
        hash
    }

    /// The hash of a value with no stable identity (an inline object), computed
    /// every time. **O(size of the value)**.
    pub fn hash_value<T: Serialize>(&mut self, value: &T) -> u128 {
        self.serialized += 1;
        match serde_json::to_string(value) {
            Ok(json) => hash_str(&json),
            // A property struct is plain data and always serializes. If that
            // ever stops being true the diff must not silently call two
            // different values equal, so an unserializable value gets a hash
            // that cannot collide with a serialized one.
            Err(_) => hash_str("\u{0}unserializable"),
        }
    }
}

/// A sibling list still to be projected.
#[derive(Clone, Copy, Debug)]
struct Pending {
    story: u32,
    /// Index of the block whose children these are, or [`NO_PARENT`] for a
    /// story's roots.
    parent: u32,
}

/// The resumable projector.
///
/// Holds no borrow of the document, so a host may park it between animation
/// frames or move the whole job into a Worker. `step` takes the document again.
#[derive(Debug, Default)]
pub struct Projector {
    queue: VecDeque<Pending>,
    out: Projection,
    values: ValueHashes,
    started: bool,
    folded: bool,
    /// Blocks visited, for diagnostics and the complexity guard.
    pub visited: u64,
}

impl Projector {
    /// A projector that has not started.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Projects at most `budget` blocks. Returns whether the projection is
    /// complete. **O(budget)** per call, O(blocks) in total.
    pub fn step(&mut self, document: &Document, budget: usize) -> bool {
        if !self.started {
            self.enumerate_stories(document);
            self.started = true;
        }
        let mut spent = 0usize;
        while spent < budget {
            let Some(pending) = self.queue.pop_front() else {
                break;
            };
            spent += self.project_list(document, pending);
        }
        if !self.queue.is_empty() {
            return false;
        }
        if !self.folded {
            self.fold_subtrees();
            self.folded = true;
        }
        true
    }

    /// The finished projection. Call only after [`Projector::step`] returned
    /// `true`.
    #[must_use]
    pub fn finish(self) -> (Projection, ValueHashes, u64) {
        (self.out, self.values, self.visited)
    }

    /// Lists the stories and queues each one's root blocks.
    ///
    /// **O(sections + headers + footers + notes + comments)** — it reads
    /// definition maps, not content.
    fn enumerate_stories(&mut self, document: &Document) {
        self.push_story(Story::Body, StoryKey::Body);
        let definitions = document.definitions();
        for (section_index, section) in definitions.sections.iter().enumerate() {
            let section_index = u32::try_from(section_index).unwrap_or(u32::MAX);
            for reference in &section.headers {
                if definitions.headers.contains_key(&reference.reference) {
                    self.push_story(
                        Story::Header {
                            section: section_index,
                            page: kind_name(reference.kind).to_owned(),
                        },
                        StoryKey::Header(section_index, kind_tag(reference.kind)),
                    );
                }
            }
            for reference in &section.footers {
                if definitions.footers.contains_key(&reference.reference) {
                    self.push_story(
                        Story::Footer {
                            section: section_index,
                            page: kind_name(reference.kind).to_owned(),
                        },
                        StoryKey::Footer(section_index, kind_tag(reference.kind)),
                    );
                }
            }
        }
        for (index, _) in definitions.footnotes.iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            self.push_story(Story::Footnote { index }, StoryKey::Footnote(index));
        }
        for (index, _) in definitions.endnotes.iter().enumerate() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            self.push_story(Story::Endnote { index }, StoryKey::Endnote(index));
        }
        for (index, (_, comment)) in definitions.comments.iter().enumerate() {
            // A comment is the one construct with a genuinely durable
            // cross-file identity: Word writes `w16cid:durableId` and
            // `w14:paraId` into the package and this model retains both. So a
            // comment edited between two versions is recognised as the same
            // comment, not as one deleted and another added.
            let key = comment
                .durable_id
                .as_ref()
                .or(comment.para_id.as_ref())
                .cloned()
                .unwrap_or_else(|| format!("#{index}"));
            self.push_story(Story::Comment { id: key.clone() }, StoryKey::Comment(key));
        }
    }

    fn push_story(&mut self, story: Story, key: StoryKey) {
        let index = u32::try_from(self.out.stories.len()).unwrap_or(u32::MAX);
        self.out.stories.push(StorySlot {
            story,
            key,
            first_root: 0,
            root_count: 0,
            content_hash: 0,
        });
        self.queue.push_back(Pending {
            story: index,
            parent: NO_PARENT,
        });
    }

    /// Projects one sibling list, appending its blocks contiguously and queueing
    /// each one's own children. Returns how many blocks it appended.
    fn project_list(&mut self, document: &Document, pending: Pending) -> usize {
        let first = u32::try_from(self.out.blocks.len()).unwrap_or(u32::MAX);
        let count = match self.locate(document, pending) {
            Some(List::Blocks(blocks)) => {
                let blocks: Vec<&BlockNode> = blocks.iter().collect();
                for (index, block) in blocks.iter().enumerate() {
                    let index = u32::try_from(index).unwrap_or(u32::MAX);
                    self.append_block(pending, index, block);
                }
                blocks.len()
            }
            Some(List::Rows(rows)) => {
                for (index, row) in rows.iter().enumerate() {
                    let index = u32::try_from(index).unwrap_or(u32::MAX);
                    self.append_row(pending, index, row);
                }
                rows.len()
            }
            Some(List::Cells(cells)) => {
                for (index, cell) in cells.iter().enumerate() {
                    let index = u32::try_from(index).unwrap_or(u32::MAX);
                    self.append_cell(pending, index, cell);
                }
                cells.len()
            }
            None => 0,
        };
        let count32 = u32::try_from(count).unwrap_or(u32::MAX);
        if pending.parent == NO_PARENT {
            if let Some(slot) = self.out.stories.get_mut(pending.story as usize) {
                slot.first_root = first;
                slot.root_count = count32;
            }
        } else if let Some(parent) = self.out.blocks.get_mut(pending.parent as usize) {
            parent.first_child = first;
            parent.child_count = count32;
        }
        for offset in 0..count32 {
            let index = first.saturating_add(offset);
            let has_children = self.out.blocks.get(index as usize).is_some_and(|block| {
                !matches!(block.kind, BlockKey::Paragraph | BlockKey::AltChunk)
            });
            if has_children {
                self.queue.push_back(Pending {
                    story: pending.story,
                    parent: index,
                });
            }
        }
        count
    }

    /// Resolves the model list a queue entry names. **O(depth)**.
    fn locate<'a>(&self, document: &'a Document, pending: Pending) -> Option<List<'a>> {
        if pending.parent == NO_PARENT {
            let slot = self.out.stories.get(pending.story as usize)?;
            return story_blocks(document, &slot.story).map(List::Blocks);
        }
        let path = self.model_path(pending.parent);
        let mut list = {
            let slot = self
                .out
                .stories
                .get(self.out.blocks.get(pending.parent as usize)?.story as usize)?;
            List::Blocks(story_blocks(document, &slot.story)?)
        };
        for step in path {
            list = descend(list, step)?;
        }
        Some(list)
    }

    /// The sibling indices from the story root down to and including `index`.
    fn model_path(&self, index: u32) -> Vec<u32> {
        let mut path = Vec::new();
        let mut cursor = index;
        while let Some(block) = self.out.blocks.get(cursor as usize) {
            path.push(block.sibling_index);
            if block.parent == NO_PARENT {
                break;
            }
            cursor = block.parent;
        }
        path.reverse();
        path
    }

    fn append_block(&mut self, pending: Pending, sibling_index: u32, block: &BlockNode) {
        self.visited += 1;
        let (node, kind, text_hash, format_hash, object_hash, review_hash) = match block {
            BlockNode::Paragraph(paragraph) => {
                let mut text = String::new();
                append_node_plain_text(&paragraph.inlines, DIFF_REVIEW_PROJECTION, &mut text);
                let properties = self.values.hash_shared(&*paragraph.properties);
                let (format, object, review) = self.hash_inlines(&paragraph.inlines);
                let mut formatting = ContentHasher::new();
                formatting.write_u128(properties);
                formatting.write_u128(format);
                (
                    paragraph.id,
                    BlockKey::Paragraph,
                    hash_str(&text),
                    formatting.finish(),
                    object,
                    review,
                )
            }
            BlockNode::Table(table) => {
                let mut formatting = ContentHasher::new();
                formatting.write_u128(self.values.hash_value(&table.properties));
                formatting.write_u128(self.values.hash_value(&table.grid));
                formatting.write_u128(self.values.hash_value(&table.grid_change));
                (
                    table.id,
                    BlockKey::Table,
                    hash_str(""),
                    formatting.finish(),
                    hash_str(""),
                    hash_str(""),
                )
            }
            BlockNode::Sdt(sdt) => (
                sdt.id,
                BlockKey::Sdt,
                hash_str(""),
                self.values.hash_value(&sdt.properties),
                hash_str(""),
                hash_str(""),
            ),
            BlockNode::AltChunk(chunk) => (
                chunk.id,
                BlockKey::AltChunk,
                hash_str(""),
                self.values.hash_value(&chunk.properties),
                self.values.hash_value(&chunk.part),
                hash_str(""),
            ),
        };
        self.push_block(
            pending,
            sibling_index,
            node,
            kind,
            text_hash,
            format_hash,
            object_hash,
            review_hash,
        );
    }

    fn append_row(&mut self, pending: Pending, sibling_index: u32, row: &TableRow) {
        self.visited += 1;
        let format = self.values.hash_value(&row.properties);
        self.push_block(
            pending,
            sibling_index,
            row.id,
            BlockKey::Row,
            hash_str(""),
            format,
            hash_str(""),
            hash_str(""),
        );
    }

    fn append_cell(&mut self, pending: Pending, sibling_index: u32, cell: &TableCell) {
        self.visited += 1;
        let format = self.values.hash_value(&cell.properties);
        self.push_block(
            pending,
            sibling_index,
            cell.id,
            BlockKey::Cell,
            hash_str(""),
            format,
            hash_str(""),
            hash_str(""),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_block(
        &mut self,
        pending: Pending,
        sibling_index: u32,
        node: NodeId,
        kind: BlockKey,
        text_hash: u128,
        format_hash: u128,
        object_hash: u128,
        review_hash: u128,
    ) {
        let mut own = ContentHasher::new();
        own.write_tag(kind.tag());
        own.write_u128(text_hash);
        own.write_u128(format_hash);
        own.write_u128(object_hash);
        own.write_u128(review_hash);
        self.out.blocks.push(ProjectedBlock {
            node,
            kind,
            story: pending.story,
            parent: pending.parent,
            sibling_index,
            first_child: 0,
            child_count: 0,
            text_hash,
            format_hash,
            object_hash,
            review_hash,
            own_key: own.finish(),
            subtree_hash: 0,
        });
    }

    /// Hashes a paragraph's inline stream into three independent answers:
    /// character formatting, non-text objects, and tracked-change structure.
    ///
    /// Three rather than one because a reader asks three different questions,
    /// and a single hash could only say "something in this paragraph changed".
    /// **O(inlines)**, memoized per distinct `RunProperties` value.
    fn hash_inlines(&mut self, inlines: &[InlineNode]) -> (u128, u128, u128) {
        let mut format = ContentHasher::new();
        let mut object = ContentHasher::new();
        let mut review = ContentHasher::new();
        self.walk_inlines(inlines, &mut format, &mut object, &mut review);
        (format.finish(), object.finish(), review.finish())
    }

    fn walk_inlines(
        &mut self,
        inlines: &[InlineNode],
        format: &mut ContentHasher,
        object: &mut ContentHasher,
        review: &mut ContentHasher,
    ) {
        for inline in inlines {
            match inline {
                InlineNode::Run(run) => {
                    format.write_u128(self.values.hash_shared(&*run.properties));
                    // The run's own byte length, so moving a boundary between
                    // two differently-formatted runs is a formatting change
                    // even though the paragraph's text is unchanged.
                    format.write_u64(run.text.len() as u64);
                }
                InlineNode::Hyperlink(hyperlink) => {
                    object.write_tag(7);
                    object.write_u128(self.values.hash_value(&hyperlink.target));
                    self.walk_inlines(&hyperlink.inlines, format, object, review);
                }
                InlineNode::Revision(revision) => {
                    self.hash_revision(revision, review);
                    self.walk_inlines(&revision.inlines, format, object, review);
                }
                InlineNode::Sdt(sdt) => {
                    object.write_tag(8);
                    object.write_u128(self.values.hash_value(&sdt.properties));
                    self.walk_inlines(&sdt.inlines, format, object, review);
                }
                InlineNode::Field(field) => {
                    object.write_tag(9);
                    object.write_u128(self.values.hash_value(&field.instruction));
                    self.walk_inlines(&field.inlines, format, object, review);
                }
                InlineNode::Tab(_) | InlineNode::Break(_) => {}
                // Everything else contributes no projected text, so it is an
                // OBJECT: its serialized value is its identity. Reflecting the
                // value rather than listing the kinds is deliberate — a new
                // inline kind added to the model is compared from the day it
                // exists instead of being silently invisible here.
                other => {
                    object.write_tag(10);
                    object.write_u128(self.values.hash_value(other));
                }
            }
        }
    }

    /// The tracked-change structure of one revision wrapper: kind, author and
    /// date, but **not** its node id, which is minted by the parse.
    fn hash_revision(&mut self, revision: &Revision, review: &mut ContentHasher) {
        review.write_tag(match revision.kind {
            casual_doc_model::v1::RevisionKind::Insertion => 1,
            casual_doc_model::v1::RevisionKind::Deletion => 2,
            casual_doc_model::v1::RevisionKind::MoveFrom => 3,
            casual_doc_model::v1::RevisionKind::MoveTo => 4,
        });
        review.write_str(revision.author.as_deref().unwrap_or(""));
        review.write_str(revision.date.as_deref().unwrap_or(""));
    }

    /// Folds each block's descendants into its `subtree_hash`, and each story's
    /// roots into its `content_hash`.
    ///
    /// One reverse pass is enough: breadth-first order guarantees every child's
    /// index is greater than its parent's, so a child is already final when its
    /// parent is reached. **O(blocks)**.
    fn fold_subtrees(&mut self) {
        for index in (0..self.out.blocks.len()).rev() {
            let block = self.out.blocks[index];
            let mut hasher = ContentHasher::new();
            hasher.write_u128(block.own_key);
            hasher.write_u64(u64::from(block.child_count));
            for child in block.first_child..block.first_child + block.child_count {
                if let Some(child) = self.out.blocks.get(child as usize) {
                    hasher.write_u128(child.subtree_hash);
                }
            }
            self.out.blocks[index].subtree_hash = hasher.finish();
        }
        for slot in &mut self.out.stories {
            let mut hasher = ContentHasher::new();
            hasher.write_u64(u64::from(slot.root_count));
            for root in slot.first_root..slot.first_root + slot.root_count {
                if let Some(block) = self.out.blocks.get(root as usize) {
                    hasher.write_u128(block.subtree_hash);
                }
            }
            slot.content_hash = hasher.finish();
        }
        self.out.summary = SideSummary {
            blocks: u32::try_from(self.out.blocks.len()).unwrap_or(u32::MAX),
            stories: u32::try_from(self.out.stories.len()).unwrap_or(u32::MAX),
        };
    }
}

/// A model sibling list.
#[derive(Clone, Copy, Debug)]
pub enum List<'a> {
    /// Block content.
    Blocks(&'a [BlockNode]),
    /// A table's rows.
    Rows(&'a [TableRow]),
    /// A row's cells.
    Cells(&'a [TableCell]),
}

/// One step down a located list. **O(1)**.
fn descend<'a>(list: List<'a>, index: u32) -> Option<List<'a>> {
    let index = index as usize;
    match list {
        List::Blocks(blocks) => match blocks.get(index)? {
            BlockNode::Table(table) => Some(List::Rows(&table.rows)),
            BlockNode::Sdt(sdt) => Some(List::Blocks(&sdt.blocks)),
            BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => None,
        },
        List::Rows(rows) => Some(List::Cells(&rows.get(index)?.cells)),
        List::Cells(cells) => Some(List::Blocks(&cells.get(index)?.blocks)),
    }
}

/// The root block list of one story. **O(1)** for the body, O(log n) for a
/// definition-map lookup.
#[must_use]
pub fn story_blocks<'a>(document: &'a Document, story: &Story) -> Option<&'a [BlockNode]> {
    let definitions = document.definitions();
    match story {
        Story::Body => Some(document.body()),
        Story::Header { section, page } => {
            let boundary = definitions.sections.get(*section as usize)?;
            let reference = boundary
                .headers
                .iter()
                .find(|reference| kind_name(reference.kind) == page)?;
            Some(&definitions.headers.get(&reference.reference)?.blocks)
        }
        Story::Footer { section, page } => {
            let boundary = definitions.sections.get(*section as usize)?;
            let reference = boundary
                .footers
                .iter()
                .find(|reference| kind_name(reference.kind) == page)?;
            Some(&definitions.footers.get(&reference.reference)?.blocks)
        }
        Story::Footnote { index } => definitions
            .footnotes
            .iter()
            .nth(*index as usize)
            .map(|(_, note)| note.blocks.as_slice()),
        Story::Endnote { index } => definitions
            .endnotes
            .iter()
            .nth(*index as usize)
            .map(|(_, note)| note.blocks.as_slice()),
        Story::Comment { id } => definitions
            .comments
            .iter()
            .find(|(_, comment)| {
                comment.durable_id.as_deref() == Some(id.as_str())
                    || comment.para_id.as_deref() == Some(id.as_str())
            })
            .map(|(_, comment)| comment.blocks.as_slice()),
        Story::Definitions => None,
    }
}

/// The model block a [`crate::record::DiffAnchor`]'s `story` + `path` names, in
/// a document the caller still holds. **O(depth)**.
///
/// This is the only live-document coordinate a change record carries (see the
/// crate docs: `node` addresses the parse the comparison ran on), so applying a
/// comparison to an open document goes through here.
///
/// It lives beside the walk that *produced* the path rather than in the caller
/// for one reason: **the projection's block sequence is not the model's block
/// list.** A table contributes rows and cells, which are not `BlockNode`s; a
/// block-level content control contributes itself *and* is descended into; a
/// paragraph and an alt-chunk are leaves. A hand-rolled walk over
/// `Document::body()` would be subtly wrong at exactly those three points, and
/// nothing would catch it drifting. This reuses the producer's own `descend`
/// walk and [`story_blocks`], so the producer and the resolver cannot disagree.
///
/// Returns `None` when the path is empty, names a row or a cell rather than a
/// block, leaves the document's shape (a stale path against an edited
/// document), or names a story this document does not have.
#[must_use]
pub fn block_at_path<'a>(
    document: &'a Document,
    story: &Story,
    path: &[PathSegment],
) -> Option<&'a BlockNode> {
    let (last, container) = path.split_last()?;
    // A row and a cell are projected blocks and are not `BlockNode`s, so a path
    // that ends at one names no block. Saying so is the point: the caller
    // reports it rather than silently resolving the enclosing table.
    let PathSegment::Block { index } = last else {
        return None;
    };
    let mut list = List::Blocks(story_blocks(document, story)?);
    for segment in container {
        list = descend(list, segment_index(*segment))?;
    }
    match list {
        List::Blocks(blocks) => blocks.get(*index as usize),
        List::Rows(_) | List::Cells(_) => None,
    }
}

/// The sibling index a path segment carries, whichever kind it is. The kind
/// names what the *segment's own block* is; `descend` already knows what a
/// step into each list yields, so the walk needs only the number.
const fn segment_index(segment: PathSegment) -> u32 {
    match segment {
        PathSegment::Block { index } | PathSegment::Row { index } | PathSegment::Cell { index } => {
            index
        }
    }
}

/// The projected text of one block, or `None` when the block is not a paragraph.
///
/// **O(the block's inlines)**. Called only for blocks a change record names, so
/// the whole-document cost is O(changes), not O(document) — which is why the
/// projection stores hashes and not text.
#[must_use]
pub fn block_text(block: &BlockNode) -> Option<String> {
    match block {
        BlockNode::Paragraph(paragraph) => {
            let mut text = String::new();
            append_node_plain_text(&paragraph.inlines, DIFF_REVIEW_PROJECTION, &mut text);
            Some(text)
        }
        _ => None,
    }
}

/// Resolves a projected block back to its model node. **O(depth)**.
#[must_use]
pub fn resolve<'a>(
    document: &'a Document,
    projection: &Projection,
    index: u32,
) -> Option<List<'a>> {
    let block = projection.blocks.get(index as usize)?;
    let slot = projection.stories.get(block.story as usize)?;
    let mut list = List::Blocks(story_blocks(document, &slot.story)?);
    let mut path = Vec::new();
    let mut cursor = index;
    while let Some(step) = projection.blocks.get(cursor as usize) {
        path.push(step.sibling_index);
        if step.parent == NO_PARENT {
            break;
        }
        cursor = step.parent;
    }
    path.reverse();
    for step in &path[..path.len().saturating_sub(1)] {
        list = descend(list, *step)?;
    }
    Some(list)
}

/// The model block a projected paragraph/table/sdt index names. **O(depth)**.
#[must_use]
pub fn resolve_block<'a>(
    document: &'a Document,
    projection: &Projection,
    index: u32,
) -> Option<&'a BlockNode> {
    let sibling = projection.blocks.get(index as usize)?.sibling_index as usize;
    match resolve(document, projection, index)? {
        List::Blocks(blocks) => blocks.get(sibling),
        List::Rows(_) | List::Cells(_) => None,
    }
}

/// The stable name of a header/footer page type.
const fn kind_name(kind: HeaderFooterKind) -> &'static str {
    match kind {
        HeaderFooterKind::Default => "default",
        HeaderFooterKind::First => "first",
        HeaderFooterKind::Even => "even",
    }
}

/// The page type as a pairing tag.
const fn kind_tag(kind: HeaderFooterKind) -> u8 {
    match kind {
        HeaderFooterKind::Default => 0,
        HeaderFooterKind::First => 1,
        HeaderFooterKind::Even => 2,
    }
}
