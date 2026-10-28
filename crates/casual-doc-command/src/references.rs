// SPDX-License-Identifier: Apache-2.0

//! Table of contents, captions and cross-references: the lowerings the editor
//! and agents share (`docs/181` §4.2 step 4, §12 lane references).
//!
//! These are the gestures that read the document's **layout**: a contents
//! entry prints the page its heading is on, a chapter number counts headings,
//! and a page cross-reference prints a page. The lowering itself stays pure and
//! layout-free: what it needs from layout arrives as [`LayoutFacts`], which the
//! editor fills from its own pagination and style cascade
//! (`casual-doc-wasm`'s `toc.rs` and `references.rs`) and a headless session
//! fills from a layout pass of its own state (`casual-doc-ai`). The same input
//! gives the same operations, which `casual-doc-wasm`'s parity tests
//! (`docs/181` G7) hold.
//!
//! # The performance rule this module obeys
//!
//! The feature that most wants to enumerate a whole document is the one that
//! lists every caption in it. The defect this repository already paid for
//! (`docs/116`) was a by-id lookup inside a walk over nodes. Every enumerating
//! function here states its complexity, and the shape is always the same:
//! **one walk, carrying what it needs**. A table of contents and its updates are
//! O(document) — a statement about the whole document — and none of this is on
//! a keystroke path.
//!
//! # Where the table of contents deliberately differs from Word
//!
//! Word writes `\h` and wraps every entry in a hyperlink to a `_TocNNNN`
//! bookmark it plants in each heading. **We do not**, and the instruction we
//! write says so: it has no `\h`. Planting a bookmark per heading means one
//! `CreateBookmark` per heading, and each of those re-validates the whole
//! document — O(headings x document). Navigation is not lost: the editor follows
//! an entry by its heading text, and a generated table is marked with the same
//! `TOC 1`..`TOC 9` styles that path keys on.

use std::collections::BTreeMap;

use casual_doc_edit::references::{
    BUILT_IN_CAPTION_LABELS, CAPTION_STYLE_NAME, CaptionNumberFormat, CaptionPosition, CaptionSpec,
    REFERENCE_BOOKMARK_PREFIX, ReferenceTo, above_below, caption_paragraph, caption_style,
    reference_field,
};
use casual_doc_edit::table_structure::MintId;
use casual_doc_edit::{
    EditError, Operation, Pos, Surface, body_field_range_blocks, surface_of,
    visit_paragraphs_all_surfaces,
};
// Its own line (rustfmt is set to Preserve; a shared `use` list is where
// parallel lanes collide).
use casual_doc_edit::refused;
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Bookmark, BookmarkId, DefinitionMap, Document, FieldKind, FieldRange, FieldRangeEnd,
    FieldRangeId, FieldRangeStart, FieldUpdateState, Indentation, InlineNode, Paragraph,
    ParagraphProperties, Run, RunProperties, Style, StyleId, StyleKind, Tab, TabAlignment,
    TabLeader, TabStop, contained_inlines, contained_inlines_mut, inline_block_stories,
    node_plain_text, visit_paragraphs,
};

/// What these lowerings need from layout, supplied by the caller.
///
/// The editor answers from its paginated layout and its style cascade; a
/// headless session answers from a layout pass of its own state. Nothing here
/// reads layout itself, so the operations are a function of the document and
/// these answers.
pub trait LayoutFacts {
    /// The 1-based outline level of `paragraph` (1 = top), resolved through the
    /// style cascade, or `None` when it is not a heading. Called once per
    /// paragraph of a walk, so it must be O(style chain), never a document scan.
    fn heading_level(&self, paragraph: &Paragraph) -> Option<u8>;

    /// The page label printed on the page `node` is first laid out on — the
    /// label a `PAGE` field would print there — or `None` when the layout placed
    /// no fragment for it. O(1) per call.
    fn page_label(&self, node: NodeId) -> Option<String>;

    /// Whether [`page_label`](Self::page_label) can answer for every node of the
    /// document. A table of contents needs the whole document paginated; a
    /// layout that holds one window of it says so here, and the contents
    /// lowering refuses with that reason rather than invent page numbers.
    ///
    /// # Errors
    /// The reason the whole pagination is not available.
    fn whole_pagination(&self) -> Result<(), String> {
        Ok(())
    }
}

/// Operations plus the caret the editor places after them.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// The operations, one step.
    pub operations: Vec<Operation>,
    /// Where the caret goes.
    pub caret: Pos,
}

fn next(ids: MintId<'_>) -> Result<NodeId, String> {
    ids().ok_or_else(|| "id space exhausted".to_owned())
}

/// The plain text of a paragraph as a picker or an entry shows it: trimmed,
/// tabs flattened to spaces. O(inlines).
fn display_text(paragraph: &Paragraph) -> String {
    node_plain_text(&paragraph.inlines)
        .trim()
        .replace('\t', " ")
}

/// The paragraph style named exactly `name`, if the document defines one.
/// **O(styles).**
#[must_use]
pub fn paragraph_style_named(document: &Document, name: &str) -> Option<StyleId> {
    if name.is_empty() {
        return None;
    }
    document
        .definitions()
        .styles
        .iter()
        .find(|(_, style)| {
            style.kind == StyleKind::Paragraph && style.name.as_deref() == Some(name)
        })
        .map(|(id, _)| *id)
}

// ---- Table of contents ------------------------------------------------------

/// The deepest heading level a table of contents can show, as Word's dialog
/// offers.
pub const MAX_TOC_LEVEL: u8 = 9;

/// The default when the caller names no level — Word's own default.
pub const DEFAULT_TOC_LEVEL: u8 = 3;

/// The indent one contents level adds, in twips (0.15 in), matching the
/// built-in `TOC n` styles Word ships.
const TOC_LEVEL_INDENT_TWIPS: i32 = 220;

/// The levels a request for `requested` shows: `0` is Word's default of 3,
/// anything else is clamped to 1..=9. O(1).
#[must_use]
pub fn toc_level(requested: u32) -> u8 {
    if requested == 0 {
        DEFAULT_TOC_LEVEL
    } else {
        (requested.min(u32::from(MAX_TOC_LEVEL)) as u8).max(1)
    }
}

/// The name of the paragraph style for contents level `level` (1-based).
/// `TOC 1`..`TOC 9` is Word's marking, and the one the editor's contents
/// navigation recognises. O(1).
fn toc_style_name(level: u8) -> String {
    format!("TOC {level}")
}

/// One heading the contents names.
struct Heading {
    /// 1-based outline level (1 = top).
    level: u8,
    /// The heading's own text, tabs flattened.
    text: String,
    /// The page label printed on the page the heading is laid out on. Empty
    /// when the layout placed no fragment for it (a heading in a text box); it
    /// is not invented, and the entry then shows no page.
    page: String,
}

/// Every heading at or above `max_level`, in document order, with its page
/// label.
///
/// **O(document)**: one walk of the body carrying each paragraph (never a by-id
/// lookup per heading, `docs/116`), with O(1) page lookups.
fn toc_headings(
    document: &Document,
    facts: &impl LayoutFacts,
    max_level: u8,
) -> Result<Vec<Heading>, String> {
    facts.whole_pagination()?;
    let mut headings = Vec::new();
    visit_paragraphs(document.body(), &mut |paragraph| {
        let Some(level) = facts.heading_level(paragraph) else {
            return;
        };
        if level > max_level {
            return;
        }
        let text = node_plain_text(&paragraph.inlines);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        headings.push(Heading {
            level,
            text: trimmed.replace('\t', " "),
            page: facts.page_label(paragraph.id).unwrap_or_default(),
        });
    });
    Ok(headings)
}

/// The style id for contents level `level`, plus the operation that creates
/// the style when the document does not already define it.
///
/// **O(styles)** per level; at most nine levels exist.
fn toc_style(
    document: &Document,
    level: u8,
    ids: MintId<'_>,
) -> Result<(StyleId, Option<Operation>), String> {
    let name = toc_style_name(level);
    if let Some(id) = paragraph_style_named(document, &name) {
        return Ok((id, None));
    }
    let id = StyleId::new(next(ids)?);
    let style = Style {
        kind: StyleKind::Paragraph,
        is_default: false,
        name: Some(name),
        aliases: None,
        based_on: None,
        next: None,
        link: None,
        hidden: false,
        ui_priority: Some(39),
        semi_hidden: false,
        unhide_when_used: true,
        q_format: false,
        locked: false,
        paragraph: Some(ParagraphProperties {
            indentation: Some(Indentation {
                start_twips: Some(i32::from(level - 1) * TOC_LEVEL_INDENT_TWIPS),
                ..Indentation::default()
            }),
            ..ParagraphProperties::default()
        }),
        run: None,
        table: None,
        table_row: None,
        table_cell: None,
        conditional: Vec::new(),
        custom_style: false,
    };
    Ok((
        id,
        Some(Operation::SetStyleDefinition {
            id,
            style: Some(Box::new(style)),
        }),
    ))
}

/// The right-aligned dot-leader tab stop an entry's page number sits on, at
/// `position_twips` (the caller's content width) — Word's shape for an entry.
/// O(1).
fn toc_tab_stop(position_twips: i32) -> TabStop {
    TabStop {
        position_twips: position_twips.max(0),
        alignment: TabAlignment::End,
        leader: Some(TabLeader::Dot),
    }
}

/// The entry paragraphs for `headings`, carrying the field range's two markers
/// on the first and last block, and the `SetStyleDefinition` operations for any
/// `TOC n` style the document does not yet define.
///
/// **O(headings)** plus the bounded style pass.
fn toc_blocks(
    document: &Document,
    headings: &[Heading],
    field: FieldRangeId,
    tab_position_twips: i32,
    ids: MintId<'_>,
) -> Result<(Vec<Operation>, Vec<BlockNode>), String> {
    let mut style_ops = Vec::new();
    let mut styles: BTreeMap<u8, StyleId> = BTreeMap::new();
    for heading in headings {
        if let std::collections::btree_map::Entry::Vacant(slot) = styles.entry(heading.level) {
            let (id, op) = toc_style(document, heading.level, ids)?;
            slot.insert(id);
            style_ops.extend(op);
        }
    }
    let tab_stop = toc_tab_stop(tab_position_twips);

    let mut blocks = Vec::with_capacity(headings.len());
    for (index, heading) in headings.iter().enumerate() {
        let mut inlines = Vec::with_capacity(5);
        if index == 0 {
            inlines.push(InlineNode::FieldRangeStart(FieldRangeStart {
                id: next(ids)?,
                field,
            }));
        }
        inlines.push(InlineNode::Run(Run {
            id: next(ids)?,
            properties: RunProperties::default().into(),
            text: heading.text.clone(),
        }));
        inlines.push(InlineNode::Tab(Tab { id: next(ids)? }));
        // A heading the layout placed nowhere it can number (one in a text
        // box) has no page: the entry keeps its tab and shows none, because an
        // empty run is not a valid model node and would refuse the whole table.
        if !heading.page.is_empty() {
            inlines.push(InlineNode::Run(Run {
                id: next(ids)?,
                properties: RunProperties::default().into(),
                text: heading.page.clone(),
            }));
        }
        if index + 1 == headings.len() {
            inlines.push(InlineNode::FieldRangeEnd(FieldRangeEnd {
                id: next(ids)?,
                field,
            }));
        }
        blocks.push(BlockNode::Paragraph(Paragraph {
            id: next(ids)?,
            properties: ParagraphProperties {
                style_ref: styles.get(&heading.level).copied(),
                tabs: vec![tab_stop],
                ..ParagraphProperties::default()
            }
            .into(),
            inlines,
        }));
    }
    Ok((style_ops, blocks))
}

/// The deepest level a `TOC` instruction asks for: the upper bound of its
/// `\o "1-3"` switch, clamped to 1..=9; Word's 3 when the switch is absent or
/// unreadable. **O(instruction length).**
#[must_use]
pub fn toc_levels_of(instruction: &str) -> u8 {
    let Some(rest) = instruction.split("\\o").nth(1) else {
        return DEFAULT_TOC_LEVEL;
    };
    let Some(quoted) = rest.split('"').nth(1) else {
        return DEFAULT_TOC_LEVEL;
    };
    quoted
        .split('-')
        .nth(1)
        .and_then(|value| value.trim().parse::<u8>().ok())
        .map_or(DEFAULT_TOC_LEVEL, |level| level.clamp(1, MAX_TOC_LEVEL))
}

/// The instruction a generated table of contents carries. `\z` (hide in web
/// layout) and `\u` (use outline levels) are Word's own defaults; `\h` is
/// deliberately absent (see the module note). O(1).
#[must_use]
pub fn toc_instruction(max_level: u8) -> String {
    format!(" TOC \\o \"1-{max_level}\" \\z \\u ")
}

/// Inserts a table of contents before the top-level body block holding
/// `before`, listing every heading down to `max_level` (see [`toc_level`]).
///
/// Each entry is a `TOC n`-styled paragraph holding the heading's own text, a
/// right-aligned dot-leader tab at `tab_position_twips` (the content width),
/// and the page label that page prints. The whole run of entries is one
/// paragraph-spanning complex field (`docs/128`), so it is a live field an
/// update finds again. The caret goes to the start of the first entry.
///
/// **O(document)** — one walk for the headings. Never on a keystroke path.
///
/// # Errors
/// `before` is not in the body; the whole document is not paginated
/// ([`LayoutFacts::whole_pagination`]); no heading at `max_level` or above; ids
/// ran out.
pub fn insert_table_of_contents(
    document: &Document,
    facts: &impl LayoutFacts,
    before: NodeId,
    max_level: u8,
    tab_position_twips: i32,
    ids: MintId<'_>,
) -> Result<Placed, String> {
    let index = document
        .body()
        .iter()
        .position(|block| crate::table::holds_paragraph(block, before))
        .ok_or_else(|| {
            refused!(
                "toc.body-only",
                "A table of contents goes in the document body, not in a header, footer or note."
            )
            .to_owned()
        })?;
    let headings = toc_headings(document, facts, max_level)?;
    if headings.is_empty() {
        return Err(format!(
            "the document has no headings at level {max_level} or above to list"
        ));
    }
    let field = FieldRangeId::new(next(ids)?);
    let (mut operations, blocks) = toc_blocks(document, &headings, field, tab_position_twips, ids)?;
    let caret = blocks
        .first()
        .and_then(|block| match block {
            BlockNode::Paragraph(paragraph) => Some(Pos::new(paragraph.id, 0)),
            _ => None,
        })
        .ok_or_else(|| {
            refused!(
                "toc.empty",
                "This table of contents has no entries to go to."
            )
            .to_owned()
        })?;
    let instruction = toc_instruction(max_level);
    operations.push(Operation::InsertFieldRange {
        field,
        definition: Box::new(FieldRange {
            kind: FieldKind::parse(&instruction),
            instruction,
            update: FieldUpdateState::default(),
        }),
        index: index as u32,
        blocks,
    });
    Ok(Placed { operations, caret })
}

/// Word's two ways to update a table of contents.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TocUpdate {
    /// Re-read the pagination and keep the entries.
    PageNumbers,
    /// Rebuild the entries from the heading outline.
    Entire,
}

/// The field range a contents command acts on: `field`, or — when `None` — the
/// document's single `TOC` field range. Returns its id and instruction.
///
/// **O(field ranges).**
///
/// # Errors
/// `field` names no field range; with `None`, the document has no table of
/// contents or more than one.
pub fn toc_field(
    document: &Document,
    field: Option<FieldRangeId>,
) -> Result<(FieldRangeId, String), String> {
    let ranges = &document.definitions().field_ranges;
    if let Some(field) = field {
        let range = ranges
            .get(&field)
            .ok_or_else(|| "no such field range".to_owned())?;
        return Ok((field, range.instruction.clone()));
    }
    let mut found = ranges
        .iter()
        .filter(|(_, range)| range.kind == FieldKind::Toc)
        .map(|(id, range)| (*id, range.instruction.clone()));
    let first = found.next().ok_or_else(|| {
        refused!(
            "toc.none",
            "This document has no table of contents to update."
        )
        .to_owned()
    })?;
    if found.next().is_some() {
        return Err(
            "this document holds more than one table of contents; name the one to update"
                .to_owned(),
        );
    }
    Ok(first)
}

/// Updates a table of contents: [`TocUpdate::PageNumbers`] rewrites only the
/// page column from the current pagination; [`TocUpdate::Entire`] rebuilds the
/// entries from the heading outline. One `DeleteBlocks` + `InsertBlocks` pair
/// (plus any `TOC n` style the rebuild needs).
///
/// **O(document)** — one walk for the headings.
///
/// # Errors
/// As [`toc_field`]; the field's markers are not at body top level; the whole
/// document is not paginated; for `Entire`, no headings; for `PageNumbers`, the
/// entry count no longer matches the heading count (the outline changed, which
/// needs a whole rebuild rather than numbers on the wrong rows).
pub fn update_table_of_contents(
    document: &Document,
    facts: &impl LayoutFacts,
    field: Option<FieldRangeId>,
    mode: TocUpdate,
    tab_position_twips: i32,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    let (field, instruction) = toc_field(document, field)?;
    let (first, last) = body_field_range_blocks(document.body(), field)
        .map_err(|_| "the contents field's markers are not at body top level".to_owned())?;
    let count = (last - first + 1) as u32;
    let max_level = toc_levels_of(&instruction);
    let headings = toc_headings(document, facts, max_level)?;

    let (mut operations, blocks) = match mode {
        TocUpdate::Entire => {
            if headings.is_empty() {
                return Err(refused!(
                    "toc.no-headings",
                    "This document has no headings to list. Apply a heading style to the text \
                     you want listed."
                )
                .to_owned());
            }
            toc_blocks(document, &headings, field, tab_position_twips, ids)?
        }
        TocUpdate::PageNumbers => (
            Vec::new(),
            toc_renumbered_blocks(document, first, last, &headings, ids)?,
        ),
    };
    operations.push(Operation::DeleteBlocks {
        container: None,
        index: first as u32,
        count,
    });
    operations.push(Operation::InsertBlocks {
        container: None,
        index: first as u32,
        blocks,
    });
    Ok(operations)
}

/// The contents blocks `first..=last` with only their page numbers rewritten
/// from `headings`: each entry keeps its own runs, formatting and field
/// markers, and only the text after its tab is replaced. Entries pair with
/// headings POSITIONALLY, which is what "update page numbers only" means.
///
/// **O(entries + inlines in them).**
fn toc_renumbered_blocks(
    document: &Document,
    first: usize,
    last: usize,
    headings: &[Heading],
    ids: MintId<'_>,
) -> Result<Vec<BlockNode>, String> {
    let blocks: Vec<BlockNode> = document.body()[first..=last].to_vec();
    let entries = blocks
        .iter()
        .filter(|block| matches!(block, BlockNode::Paragraph(p) if has_tab(&p.inlines)))
        .count();
    if entries != headings.len() {
        return Err(format!(
            "this table lists {entries} entries but the document now has {} headings; \
             update the entire table instead of only its page numbers",
            headings.len()
        ));
    }

    let mut out = Vec::with_capacity(blocks.len());
    let mut next_heading = 0_usize;
    for block in blocks {
        let BlockNode::Paragraph(mut paragraph) = block else {
            out.push(block);
            continue;
        };
        let Some(tab) = paragraph
            .inlines
            .iter()
            .position(|inline| matches!(inline, InlineNode::Tab(_)))
        else {
            out.push(BlockNode::Paragraph(paragraph));
            continue;
        };
        let end_marker = match paragraph.inlines.last() {
            Some(InlineNode::FieldRangeEnd(marker)) => Some(InlineNode::FieldRangeEnd(*marker)),
            _ => None,
        };
        let mut inlines: Vec<InlineNode> = paragraph.inlines[..=tab].to_vec();
        let page = &headings[next_heading].page;
        if !page.is_empty() {
            inlines.push(InlineNode::Run(Run {
                id: next(ids)?,
                properties: RunProperties::default().into(),
                text: page.clone(),
            }));
        }
        inlines.extend(end_marker);
        paragraph.inlines = inlines;
        next_heading += 1;
        out.push(BlockNode::Paragraph(paragraph));
    }
    Ok(out)
}

/// Whether an inline list holds a tab — what makes a contents paragraph an
/// ENTRY rather than a "Contents" title the field happens to enclose.
/// **O(inlines).**
fn has_tab(inlines: &[InlineNode]) -> bool {
    inlines
        .iter()
        .any(|inline| matches!(inline, InlineNode::Tab(_)))
}

// ---- Captions and cross-references -----------------------------------------

/// One caption the document already holds.
///
/// `number` is the caption's 1-based position within its own label's sequence,
/// resolved the way a `SEQ` field resolves it: per sequence name, restarting
/// at the heading level named by a `\s` switch when there is one.
#[derive(Clone, Debug)]
pub struct CaptionRecord {
    /// The caption paragraph.
    pub node: NodeId,
    /// Document order among all paragraphs.
    pub order: usize,
    /// The `SEQ` sequence name, which is the caption's label.
    pub label: String,
    /// The number the document would resolve to if its fields were updated
    /// now.
    pub number: u32,
    /// The number the caption's `SEQ` field currently **shows**: its cached
    /// result, which is what a reader sees and what a picker lists. Kept apart
    /// from `number` deliberately: reporting the recomputed number would make
    /// the caption list unable to disagree with the model, so a build that
    /// cached the wrong number would still list the right one.
    pub cached_number: String,
    /// The number format the existing `SEQ` switch asks for.
    pub format: CaptionNumberFormat,
    /// The caption paragraph's plain text (trimmed, tabs flattened).
    pub text: String,
    /// The paragraph's own inline tree, kept (only when asked) so a renumber
    /// can rewrite the `SEQ` field's cached run without a second lookup.
    pub inlines: Vec<InlineNode>,
}

/// One cross-reference target: a heading, a bookmark, a note, or a caption.
#[derive(Clone, Debug)]
pub struct TargetRecord {
    /// The paragraph a reference to it is bookmarked in.
    pub node: NodeId,
    /// Document order.
    pub order: usize,
    /// What a picker shows.
    pub text: String,
}

/// What a caller wants out of one walk, so the walk allocates nothing per
/// paragraph it was not asked about: O(captions) memory, not O(document).
#[derive(Clone, Copy, Debug, Default)]
pub struct ScanRequest<'a> {
    /// The handful of nodes whose document order, chapter ordinal and leading
    /// `_Ref` bookmarks the caller needs — a command's target and its caret.
    pub interest: &'a [NodeId],
    /// Whether each caption keeps its inline tree. Only renumbering needs it.
    pub keep_inlines: bool,
}

/// Everything one walk of the document yields for captions and
/// cross-references.
#[derive(Debug, Default)]
pub struct ReferenceScan {
    /// Every caption, in document order, numbered.
    pub captions: Vec<CaptionRecord>,
    /// Every heading, in document order, as `(level, node, text)`.
    pub headings: Vec<(u8, NodeId, String)>,
    interest: Vec<InterestRecord>,
    paragraphs: usize,
}

/// What the walk recorded about one node the caller asked about.
#[derive(Debug)]
pub struct InterestRecord {
    /// The node asked about.
    pub node: NodeId,
    /// Its document order among all paragraphs.
    pub order: usize,
    /// The running chapter ordinal per heading level as the walk passed it.
    pub chapters: [u32; 9],
    /// The paragraph's plain-text byte length — the end offset of a bookmark
    /// covering the whole paragraph. Deliberately NOT the trimmed display
    /// text's length: a bookmark ending there silently covers less than the
    /// reference claims.
    pub length: u32,
    /// The byte offset just past the first `SEQ` field's cached result: the
    /// boundary *Only label and number* and *Only caption text* bookmark on
    /// either side of.
    pub after_sequence: Option<u32>,
    /// Every `_Ref`-prefixed bookmark in the paragraph, as `(id, start, end)`.
    pub reference_bookmarks: Vec<(BookmarkId, u32, u32)>,
}

impl ReferenceScan {
    /// Every caption label the document uses, unioned with Word's built-ins,
    /// sorted. O(captions).
    #[must_use]
    pub fn caption_labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = BUILT_IN_CAPTION_LABELS
            .iter()
            .map(|label| (*label).to_owned())
            .collect();
        for caption in &self.captions {
            if !labels.iter().any(|known| known == &caption.label) {
                labels.push(caption.label.clone());
            }
        }
        labels.sort();
        labels.dedup();
        labels
    }

    /// The row the walk recorded for `node`. O(interest).
    #[must_use]
    pub fn found(&self, node: NodeId) -> Option<&InterestRecord> {
        self.interest.iter().find(|record| record.node == node)
    }

    /// `node`'s document order, or the end of the document when the walk never
    /// saw it (so it sorts last rather than first). O(interest).
    fn order(&self, node: NodeId) -> usize {
        self.found(node)
            .map_or(self.paragraphs, |record| record.order)
    }

    /// The 1-based ordinal of the enclosing chapter at `level` — what Word's
    /// `STYLEREF N \s` shows; at least 1. O(interest).
    fn chapter_ordinal(&self, node: NodeId, level: u8) -> u32 {
        let index = usize::from(level.clamp(1, 9)) - 1;
        self.found(node)
            .map_or(0, |record| record.chapters[index])
            .max(1)
    }

    /// The `_Ref` bookmark on `node` covering exactly `[start, end)`.
    /// O(interest x bookmarks on that paragraph).
    #[must_use]
    pub fn reference_bookmark(&self, node: NodeId, start: u32, end: u32) -> Option<BookmarkId> {
        self.found(node).and_then(|record| {
            record
                .reference_bookmarks
                .iter()
                .find(|(_, from, to)| *from == start && *to == end)
                .map(|(bookmark, _, _)| *bookmark)
        })
    }
}

/// The `\s` restart level a `SEQ` instruction asks for. O(instruction).
fn sequence_restart_level(instruction: &str) -> Option<u8> {
    let tokens: Vec<&str> = instruction.split_whitespace().collect();
    let index = tokens.iter().position(|token| *token == "\\s")?;
    tokens.get(index + 1)?.parse::<u8>().ok()
}

/// The number format a `SEQ` instruction's `\*` switch asks for, or arabic.
/// O(instruction).
fn sequence_format(instruction: &str) -> CaptionNumberFormat {
    let tokens: Vec<&str> = instruction.split_whitespace().collect();
    let Some(index) = tokens.iter().position(|token| *token == "\\*") else {
        return CaptionNumberFormat::Arabic;
    };
    match tokens.get(index + 1).copied().unwrap_or("") {
        "ROMAN" => CaptionNumberFormat::UpperRoman,
        "roman" => CaptionNumberFormat::LowerRoman,
        "ALPHABETIC" => CaptionNumberFormat::UpperLetter,
        "alphabetic" => CaptionNumberFormat::LowerLetter,
        _ => CaptionNumberFormat::Arabic,
    }
}

/// The cached result text of the first `SEQ` field in an inline tree — the
/// number a reader currently sees. Descent is `contained_inlines`', the same as
/// [`sequence_of`]'s. O(inlines in the paragraph's subtree).
fn sequence_cached_number(inlines: &[InlineNode]) -> String {
    fn find(inlines: &[InlineNode]) -> Option<String> {
        for inline in inlines {
            if let InlineNode::Field(field) = inline
                && matches!(field.kind, FieldKind::Seq { .. })
            {
                return Some(node_plain_text(&field.inlines));
            }
            if let Some(nested) = contained_inlines(inline)
                && let Some(found) = find(nested)
            {
                return Some(found);
            }
        }
        None
    }
    find(inlines).unwrap_or_default()
}

/// The first `SEQ` field in an inline tree, as `(sequence name, instruction)`.
/// A caption is identified by having one, which is how Word identifies one, so
/// a caption another producer wrote is still a caption.
///
/// It descends the inline containers (hyperlink, field, revision, content
/// control) and deliberately NOT a text box or group: a caption in a text box
/// is a paragraph of its own, found when the paragraph walk reaches it;
/// descending here would label the anchoring paragraph a caption too and
/// number one caption twice. O(inlines in the paragraph's subtree).
fn sequence_of(inlines: &[InlineNode]) -> Option<(String, String)> {
    for inline in inlines {
        if let InlineNode::Field(field) = inline
            && let FieldKind::Seq { name } = &field.kind
        {
            return Some((name.clone(), field.instruction.clone()));
        }
        if let Some(nested) = contained_inlines(inline)
            && let Some(found) = sequence_of(nested)
        {
            return Some(found);
        }
    }
    None
}

/// The plain-text byte offsets a cross-reference needs from one paragraph: its
/// total length, the offset just past its first `SEQ` field, and every
/// `_Ref`-prefixed bookmark's extent.
///
/// Offsets derive from `node_plain_text`, the one definition of a paragraph's
/// text. The walk descends the inline containers (a `_Ref` bookmark or a `SEQ`
/// inside a hyperlink is still found) and not a text box or group (their
/// paragraphs have their own offset spaces). A legacy form checkbox is not
/// descended: its painted glyph is not its children's text.
///
/// **O(inlines in the paragraph's subtree)**, one pass; it runs for the
/// paragraphs a command names, never per paragraph of the document.
fn paragraph_offsets(
    paragraph: &Paragraph,
    bookmarks: &DefinitionMap<BookmarkId, Bookmark>,
) -> (u32, Option<u32>, Vec<(BookmarkId, u32, u32)>) {
    struct Walk<'a> {
        bookmarks: &'a DefinitionMap<BookmarkId, Bookmark>,
        offset: u32,
        after_sequence: Option<u32>,
        open: Vec<(BookmarkId, u32)>,
        extents: Vec<(BookmarkId, u32, u32)>,
    }

    impl Walk<'_> {
        fn inlines(&mut self, inlines: &[InlineNode]) {
            for inline in inlines {
                let own_len = node_plain_text(core::slice::from_ref(inline)).len() as u32;
                match inline {
                    InlineNode::BookmarkStart(marker) => {
                        if self
                            .bookmarks
                            .get(&marker.bookmark)
                            .is_some_and(|b| b.name.starts_with(REFERENCE_BOOKMARK_PREFIX))
                        {
                            self.open.push((marker.bookmark, self.offset));
                        }
                    }
                    InlineNode::BookmarkEnd(marker) => {
                        if let Some(position) = self
                            .open
                            .iter()
                            .position(|(bookmark, _)| *bookmark == marker.bookmark)
                        {
                            let (bookmark, start) = self.open.remove(position);
                            self.extents.push((bookmark, start, self.offset));
                        }
                    }
                    _ => {}
                }
                let sequence = matches!(inline,
                    InlineNode::Field(field) if matches!(field.kind, FieldKind::Seq { .. }));
                if sequence && self.after_sequence.is_none() {
                    // Past the whole field, label and number: the caption's own
                    // text starts here.
                    self.after_sequence = Some(self.offset.saturating_add(own_len));
                    self.offset = self.offset.saturating_add(own_len);
                    continue;
                }
                match contained_inlines(inline) {
                    Some(_)
                        if matches!(inline, InlineNode::Field(field)
                            if field.form.is_some()) =>
                    {
                        self.offset = self.offset.saturating_add(own_len);
                    }
                    Some(nested) => self.inlines(nested),
                    None => self.offset = self.offset.saturating_add(own_len),
                }
            }
        }
    }

    let mut walk = Walk {
        bookmarks,
        offset: 0,
        after_sequence: None,
        open: Vec::new(),
        extents: Vec::new(),
    };
    walk.inlines(&paragraph.inlines);
    let length = walk.offset;
    // A bookmark whose end marker is in a later paragraph covers this one to
    // its end; reporting it as zero-length would reuse it for a reference that
    // then shows nothing.
    for (bookmark, start) in std::mem::take(&mut walk.open) {
        walk.extents.push((bookmark, start, length));
    }
    (length, walk.after_sequence, walk.extents)
}

/// `inlines` with the cached result of its first `SEQ` field rewritten to
/// `number`; `None` when there is no `SEQ` or it already reads `number`, so a
/// renumber that changes nothing produces no operation. The descent is
/// `sequence_of`'s, so a caption this module lists is one it can renumber.
/// **O(inlines in the paragraph's subtree)**.
#[must_use]
pub fn renumbered(
    inlines: &[InlineNode],
    number: &str,
    ids: MintId<'_>,
) -> Option<Vec<InlineNode>> {
    fn rewrite(inlines: &mut [InlineNode], number: &str, ids: MintId<'_>) -> Option<bool> {
        for inline in inlines {
            if let InlineNode::Field(field) = inline
                && matches!(field.kind, FieldKind::Seq { .. })
            {
                if node_plain_text(&field.inlines) == number {
                    return Some(false);
                }
                let first = match field.inlines.first() {
                    Some(InlineNode::Run(run)) => Some((run.id, run.properties.clone())),
                    _ => None,
                };
                let (id, properties) = match first {
                    Some(pair) => pair,
                    None => (ids()?, RunProperties::default().into()),
                };
                field.inlines = vec![InlineNode::Run(Run {
                    id,
                    properties,
                    text: number.to_owned(),
                })];
                return Some(true);
            }
            if let Some(nested) = contained_inlines_mut(inline)
                && let Some(changed) = rewrite(nested, number, ids)
            {
                return Some(changed);
            }
        }
        None
    }

    let mut out = inlines.to_vec();
    match rewrite(&mut out, number, ids) {
        Some(true) => Some(out),
        Some(false) | None => None,
    }
}

/// Why a caption cannot attach to `target`, or `None` when it can. Word and
/// ONLYOFFICE both refuse a caption in a header, footer or note.
/// **O(document), one scan** (`surface_of`).
#[must_use]
pub fn caption_target_refusal(document: &Document, target: NodeId) -> Option<&'static str> {
    match surface_of(document, target) {
        Some(Surface::Body) => None,
        Some(_) => Some(
            "a caption can only be inserted in the document body, not in a header, footer or note",
        ),
        None => Some("the caption's target is not in the document"),
    }
}

/// Where a block sits: the `InsertBlocks` container (`None` = the body, else the
/// owning cell or content control) and its 0-based index there.
///
/// `node` may name the block itself or anything inside one of its paragraphs
/// (a selected picture, or a picture in a text box: the caption then lands in
/// the body beside the block that anchors the box, because `InsertBlocks` has
/// no text-box container). **O(document), ONE walk** of the body.
#[must_use]
pub fn locate_block(document: &Document, node: NodeId) -> Option<(Option<NodeId>, usize)> {
    fn search(
        blocks: &[BlockNode],
        container: Option<NodeId>,
        node: NodeId,
    ) -> Option<(Option<NodeId>, usize)> {
        for (index, block) in blocks.iter().enumerate() {
            let named = match block {
                BlockNode::Paragraph(paragraph) => {
                    paragraph.id == node || inlines_hold(&paragraph.inlines, node)
                }
                BlockNode::Table(table) => table.id == node,
                BlockNode::Sdt(sdt) => sdt.id == node,
                BlockNode::AltChunk(chunk) => chunk.id == node,
            };
            if named {
                return Some((container, index));
            }
        }
        // Not at this level: descend, so a caption on a picture inside a table
        // cell lands in that cell.
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            if let Some(found) = search(&cell.blocks, Some(cell.id), node) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = search(&sdt.blocks, Some(sdt.id), node) {
                        return Some(found);
                    }
                }
                BlockNode::Paragraph(_) | BlockNode::AltChunk(_) => {}
            }
        }
        None
    }
    search(document.body(), None, node)
}

/// Whether an inline tree holds `id` at any depth, a text box's or group's own
/// block content included. **O(nodes in the paragraph's subtree).**
fn inlines_hold(inlines: &[InlineNode], id: NodeId) -> bool {
    if inlines.iter().any(|inline| inline.id() == id) {
        return true;
    }
    for inline in inlines {
        if let Some(nested) = contained_inlines(inline)
            && inlines_hold(nested, id)
        {
            return true;
        }
    }
    let mut found = false;
    inline_block_stories(inlines, &mut |blocks| {
        found = found || blocks_hold(blocks, id);
    });
    found
}

/// The block half of [`inlines_hold`]. **O(nodes in the subtree).**
fn blocks_hold(blocks: &[BlockNode], id: NodeId) -> bool {
    blocks.iter().any(|block| match block {
        BlockNode::Paragraph(paragraph) => {
            paragraph.id == id || inlines_hold(&paragraph.inlines, id)
        }
        BlockNode::Table(table) => {
            table.id == id
                || table.rows.iter().any(|row| {
                    row.cells
                        .iter()
                        .any(|cell| cell.id == id || blocks_hold(&cell.blocks, id))
                })
        }
        BlockNode::Sdt(sdt) => sdt.id == id || blocks_hold(&sdt.blocks, id),
        BlockNode::AltChunk(chunk) => chunk.id == id,
    })
}

/// ONE walk of every block surface, carrying everything captions and
/// cross-references need.
///
/// **O(document) time and exactly one whole-document scan**
/// (`visit_paragraphs_all_surfaces`); memory O(captions + headings + the
/// request's interest), nothing per paragraph.
#[must_use]
pub fn scan(
    document: &Document,
    facts: &impl LayoutFacts,
    request: ScanRequest<'_>,
) -> ReferenceScan {
    let bookmarks = &document.definitions().bookmarks;
    let mut scan = ReferenceScan::default();
    // The running per-label counter, and the `\s` restart level each label asks
    // for, so a heading of that level zeroes it.
    let mut counters: Vec<(String, u32, Option<u8>)> = Vec::new();
    let mut chapters = [0u32; 9];
    let mut order = 0usize;
    visit_paragraphs_all_surfaces(document, &mut |paragraph| {
        if let Some(level) = facts.heading_level(paragraph) {
            let index = usize::from(level.clamp(1, 9)) - 1;
            chapters[index] = chapters[index].saturating_add(1);
            // A heading resets every deeper level's counter, as a multilevel
            // list does.
            for deeper in chapters.iter_mut().skip(index + 1) {
                *deeper = 0;
            }
            // And restarts every sequence that names this level (`SEQ … \s N`).
            for (_, count, restart) in &mut counters {
                if *restart == Some(level) {
                    *count = 0;
                }
            }
            let trimmed = display_text(paragraph);
            if !trimmed.is_empty() {
                scan.headings.push((level, paragraph.id, trimmed));
            }
        }
        if request.interest.contains(&paragraph.id) {
            let (length, after_sequence, reference_bookmarks) =
                paragraph_offsets(paragraph, bookmarks);
            scan.interest.push(InterestRecord {
                node: paragraph.id,
                order,
                chapters,
                length,
                after_sequence,
                reference_bookmarks,
            });
        }
        if let Some((label, instruction)) = sequence_of(&paragraph.inlines) {
            let restart = sequence_restart_level(&instruction);
            let entry = if let Some(index) = counters.iter().position(|(name, _, _)| *name == label)
            {
                &mut counters[index]
            } else {
                counters.push((label.clone(), 0, restart));
                counters.last_mut().expect("just pushed")
            };
            entry.1 = entry.1.saturating_add(1);
            scan.captions.push(CaptionRecord {
                node: paragraph.id,
                order,
                label,
                number: entry.1,
                cached_number: sequence_cached_number(&paragraph.inlines),
                format: sequence_format(&instruction),
                text: display_text(paragraph),
                inlines: if request.keep_inlines {
                    paragraph.inlines.clone()
                } else {
                    Vec::new()
                },
            });
        }
        order += 1;
    });
    scan.paragraphs = order;
    scan
}

/// Every authored bookmark as a cross-reference target, named by the
/// paragraph its start marker sits in; Word's own `_Ref`/`_Toc` bookkeeping
/// bookmarks are hidden, as Word hides them.
///
/// **O(document), ONE walk**; document order comes from the walk's counter,
/// never a per-bookmark lookup.
#[must_use]
pub fn bookmark_targets(document: &Document) -> Vec<TargetRecord> {
    fn collect(
        inlines: &[InlineNode],
        bookmarks: &DefinitionMap<BookmarkId, Bookmark>,
        node: NodeId,
        order: usize,
        rows: &mut Vec<TargetRecord>,
    ) {
        for inline in inlines {
            if let InlineNode::BookmarkStart(marker) = inline
                && let Some(bookmark) = bookmarks.get(&marker.bookmark)
                && !bookmark.name.starts_with('_')
            {
                rows.push(TargetRecord {
                    node,
                    order,
                    text: bookmark.name.clone(),
                });
            }
            if let Some(nested) = contained_inlines(inline) {
                collect(nested, bookmarks, node, order, rows);
            }
        }
    }

    let mut rows = Vec::new();
    let bookmarks = &document.definitions().bookmarks;
    let mut order = 0usize;
    visit_paragraphs_all_surfaces(document, &mut |paragraph| {
        collect(
            &paragraph.inlines,
            bookmarks,
            paragraph.id,
            order,
            &mut rows,
        );
        order += 1;
    });
    rows
}

/// Footnotes or endnotes as cross-reference targets, in definition order, with
/// their first paragraph's text. **O(notes)**, no document walk.
#[must_use]
pub fn note_targets(document: &Document, footnotes: bool) -> Vec<TargetRecord> {
    let definitions = document.definitions();
    let notes = if footnotes {
        &definitions.footnotes
    } else {
        &definitions.endnotes
    };
    notes
        .iter()
        .enumerate()
        .filter_map(|(order, (_, note))| {
            let first = note.blocks.iter().find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph),
                _ => None,
            })?;
            Some(TargetRecord {
                node: first.id,
                order,
                text: display_text(first),
            })
        })
        .collect()
}

/// The display text of paragraph `node` on any surface. **O(document), ONE walk.**
fn paragraph_text_of(document: &Document, node: NodeId) -> Option<String> {
    let mut found = None;
    visit_paragraphs_all_surfaces(document, &mut |paragraph| {
        if paragraph.id == node && found.is_none() {
            found = Some(display_text(paragraph));
        }
    });
    found
}

/// A fresh `_Ref`-prefixed bookmark name no bookmark uses: one past the highest
/// number already used, as Word names its own. **O(bookmarks).**
fn fresh_reference_bookmark_name(document: &Document) -> String {
    let highest = document
        .definitions()
        .bookmarks
        .iter()
        .filter_map(|(_, bookmark)| {
            bookmark
                .name
                .strip_prefix(REFERENCE_BOOKMARK_PREFIX)
                .and_then(|digits| digits.parse::<u64>().ok())
        })
        .max()
        .unwrap_or(100_000_000);
    format!("{REFERENCE_BOOKMARK_PREFIX}{}", highest + 1)
}

/// Inserts a caption paragraph above or below the block that holds `target`
/// and renumbers the captions of the same label that follow — Word's Insert
/// Caption. The paragraph is `Caption`-styled; the style is created first when
/// the document has none, as Word does. The caret goes to the caption's start.
///
/// **O(document)**: two walks (number and enumerate, locate the block), plus
/// one `SetInlines` per later caption of that label whose number changes.
///
/// # Errors
/// An empty label; a chapter heading level outside 1..=9; a target outside the
/// body or not in the document; ids ran out.
pub fn insert_caption(
    document: &Document,
    facts: &impl LayoutFacts,
    target: NodeId,
    spec: CaptionSpec,
    ids: MintId<'_>,
) -> Result<Placed, String> {
    if spec.label.trim().is_empty() {
        return Err(refused!(
            "caption.label-required",
            "Choose a caption label, such as Figure or Table."
        )
        .to_owned());
    }
    if let Some(chapter) = spec.chapter
        && !(1..=9).contains(&chapter.heading_level)
    {
        return Err(refused!(
            "caption.chapter-level-out-of-range",
            "A chapter heading level has to be between 1 and 9."
        )
        .to_owned());
    }
    let spec = CaptionSpec {
        label: spec.label.trim().to_owned(),
        ..spec
    };
    let label = spec.label.as_str();
    if let Some(refusal) = caption_target_refusal(document, target) {
        return Err(refusal.to_owned());
    }
    let scan = scan(
        document,
        facts,
        ScanRequest {
            interest: &[target],
            keep_inlines: true,
        },
    );
    let (container, index) = locate_block(document, target).ok_or_else(|| {
        refused!(
            "caption.body-only",
            "A caption attaches to something in the document body, not in a header, footer or \
             note."
        )
        .to_owned()
    })?;

    // One more than the captions of this label before the insertion point,
    // read off the scan.
    let target_order = scan.order(target);
    let insert_after = spec.position == CaptionPosition::Below;
    let precedes = |caption: &CaptionRecord| {
        if insert_after {
            caption.order <= target_order
        } else {
            caption.order < target_order
        }
    };
    let number = 1 + scan
        .captions
        .iter()
        .filter(|caption| caption.label == label && precedes(caption))
        .count() as u32;
    let chapter_number = spec
        .chapter
        .map(|chapter| {
            scan.chapter_ordinal(target, chapter.heading_level)
                .to_string()
        })
        .unwrap_or_default();

    let mut operations = Vec::new();
    let style = match paragraph_style_named(document, CAPTION_STYLE_NAME) {
        Some(existing) => Some(existing),
        None => {
            let id = StyleId::new(next(ids)?);
            let default = document
                .definitions()
                .styles
                .iter()
                .find(|(_, style)| style.is_default)
                .map(|(id, _)| *id);
            operations.push(Operation::SetStyleDefinition {
                id,
                style: Some(Box::new(caption_style(default))),
            });
            Some(id)
        }
    };

    let paragraph_id = next(ids)?;
    let paragraph = caption_paragraph(
        paragraph_id,
        style,
        &spec,
        &spec.number_format.render(number),
        &chapter_number,
        &mut || ids().ok_or(EditError::IdExhausted),
    )
    .map_err(|error| format!("{error:?}"))?;
    let insert_index = if insert_after { index + 1 } else { index };
    operations.push(Operation::InsertBlocks {
        container,
        index: insert_index as u32,
        blocks: vec![BlockNode::Paragraph(paragraph)],
    });

    // Renumber the captions of this label that now come later; the
    // alternative is a document whose figures read 1, 1, 2.
    let mut running = number;
    for caption in &scan.captions {
        if caption.label != label || precedes(caption) {
            continue;
        }
        running += 1;
        let rendered = caption.format.render(running);
        if let Some(inlines) = renumbered(&caption.inlines, &rendered, ids) {
            operations.push(Operation::SetInlines {
                node: caption.node,
                inlines,
            });
        }
    }
    Ok(Placed {
        operations,
        caret: Pos::new(paragraph_id, 0),
    })
}

/// Rewrites every caption's `SEQ` result to the number the document resolves
/// now — Word's F9 over the caption sequences. A caption already showing the
/// right number produces no operation. The caret goes to the first caption.
///
/// **O(document)** — one walk.
///
/// # Errors
/// The document has no captions, or every caption already shows the right
/// number (rather than an empty step a person would have to undo).
pub fn update_caption_numbers(
    document: &Document,
    facts: &impl LayoutFacts,
    ids: MintId<'_>,
) -> Result<Placed, String> {
    let scan = scan(
        document,
        facts,
        ScanRequest {
            interest: &[],
            keep_inlines: true,
        },
    );
    let mut operations = Vec::new();
    for caption in &scan.captions {
        let correct = caption.format.render(caption.number);
        if let Some(inlines) = renumbered(&caption.inlines, &correct, ids) {
            operations.push(Operation::SetInlines {
                node: caption.node,
                inlines,
            });
        }
    }
    let Some(first) = scan.captions.first().map(|caption| caption.node) else {
        return Err(
            refused!("caption.none", "This document has no captions to update.").to_owned(),
        );
    };
    if operations.is_empty() {
        return Err("every caption already shows the right number".to_owned());
    }
    Ok(Placed {
        operations,
        caret: Pos::new(first, 0),
    })
}

/// Inserts a cross-reference field at `caret` pointing at paragraph `target` —
/// Word's Cross-reference.
///
/// The target is bookmarked first unless a `_Ref` bookmark of exactly the
/// extent this kind needs is already on it, so ten references to one figure
/// share one bookmark. The three caption kinds differ only in what the
/// bookmark covers (the boundary is the end of the `SEQ` field). A page
/// reference's cached result is [`LayoutFacts::page_label`] of the target.
/// `include_above_below` is Word's *Include above/below*: a space and a second
/// `REF … \p` field in the same step.
///
/// **O(document)** — two walks (the scan, and the target's text).
///
/// # Errors
/// The target is not a paragraph in the document; ids ran out.
#[allow(clippy::too_many_arguments)]
pub fn insert_cross_reference(
    document: &Document,
    facts: &impl LayoutFacts,
    caret: Pos,
    target: NodeId,
    to: ReferenceTo,
    hyperlink: bool,
    include_above_below: bool,
    ids: MintId<'_>,
) -> Result<Vec<Operation>, String> {
    let missing = || {
        refused!(
            "crossref.target-missing",
            "The thing this cross-reference points at is no longer in the document."
        )
        .to_owned()
    };
    let scan = scan(
        document,
        facts,
        ScanRequest {
            interest: &[target, caret.node],
            keep_inlines: false,
        },
    );
    // The target's OWN plain text, untrimmed, because these offsets address the
    // model.
    let offsets = scan.found(target).ok_or_else(missing)?;
    let length = offsets.length;
    let after_sequence = offsets.after_sequence;
    let text = paragraph_text_of(document, target).ok_or_else(missing)?;

    let (start, end) = match (to, after_sequence) {
        (ReferenceTo::LabelAndNumber, Some(boundary)) => (0, boundary),
        (ReferenceTo::CaptionTextOnly, Some(boundary)) => (boundary, length),
        // A target with no `SEQ` (a heading, a bookmark) has no boundary: the
        // whole paragraph rather than an empty range.
        _ => (0, length),
    };
    let result_text_from_range = || {
        text.get(start as usize..end as usize)
            .unwrap_or(&text)
            .trim()
            .replace('\t', " ")
    };

    let mut operations = Vec::new();
    let (bookmark_name, bookmark_id) = match scan.reference_bookmark(target, start, end) {
        Some(existing) => (
            document
                .definitions()
                .bookmarks
                .get(&existing)
                .map(|bookmark| bookmark.name.clone())
                .unwrap_or_default(),
            Some(existing),
        ),
        None => (fresh_reference_bookmark_name(document), None),
    };
    if bookmark_id.is_none() {
        let bookmark = BookmarkId::new(next(ids)?);
        let start_id = next(ids)?;
        let end_id = next(ids)?;
        operations.push(Operation::CreateBookmark {
            bookmark,
            name: bookmark_name.clone(),
            start: Pos::new(target, start),
            start_id,
            end: Pos::new(target, end),
            end_id,
        });
    }

    let result_text = match to {
        ReferenceTo::PageNumber => facts.page_label(target),
        ReferenceTo::AboveBelow => {
            Some(above_below(scan.order(target), scan.order(caret.node)).to_owned())
        }
        _ => Some(result_text_from_range()),
    }
    .unwrap_or_default();

    let field_id = next(ids)?;
    let result_id = next(ids)?;
    let field = reference_field(
        field_id,
        result_id,
        to,
        &bookmark_name,
        hyperlink,
        &result_text,
    )
    .map_err(|error| format!("{error:?}"))?;
    operations.push(Operation::InsertField {
        at: caret,
        field: Box::new(field),
    });

    // *Include above/below*: a space and a second `REF … \p` after the
    // reference. Each op inserts at an offset as the previous ops left the
    // paragraph, so the space goes after the reference's cached text and the
    // `\p` field after the space.
    if include_above_below && to != ReferenceTo::AboveBelow {
        let word = above_below(scan.order(target), scan.order(caret.node));
        let space_field = next(ids)?;
        let space_result = next(ids)?;
        // A field, not a bare run, so a later edit can tell the two apart.
        let above = reference_field(
            space_field,
            space_result,
            ReferenceTo::AboveBelow,
            &bookmark_name,
            hyperlink,
            word,
        )
        .map_err(|error| format!("{error:?}"))?;
        let after_reference = caret.offset.saturating_add(result_text.len() as u32);
        operations.push(Operation::InsertText {
            at: Pos::new(caret.node, after_reference),
            text: " ".to_owned(),
        });
        operations.push(Operation::InsertField {
            at: Pos::new(caret.node, after_reference.saturating_add(1)),
            field: Box::new(above),
        });
    }
    Ok(operations)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use casual_doc_model::IdGenerator;
    use casual_doc_model::v1::Definitions;

    /// Facts a test states outright: every paragraph whose text starts with
    /// `#` is a level-1 heading, and the pages are whatever the test says.
    struct Stated {
        pages: HashMap<NodeId, String>,
        whole: bool,
    }

    impl LayoutFacts for Stated {
        fn heading_level(&self, paragraph: &Paragraph) -> Option<u8> {
            node_plain_text(&paragraph.inlines)
                .starts_with('#')
                .then_some(1)
        }

        fn page_label(&self, node: NodeId) -> Option<String> {
            self.pages.get(&node).cloned()
        }

        fn whole_pagination(&self) -> Result<(), String> {
            if self.whole {
                Ok(())
            } else {
                Err("one window only".to_owned())
            }
        }
    }

    fn id(n: u64) -> NodeId {
        NodeId::from_parts(9, n).expect("non-zero")
    }

    fn paragraph(n: u64, text: &str) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: id(n),
            properties: ParagraphProperties::default().into(),
            inlines: vec![InlineNode::Run(Run {
                id: id(n + 100),
                properties: RunProperties::default().into(),
                text: text.to_owned(),
            })],
        })
    }

    fn document() -> Document {
        Document::new(
            id(1),
            vec![
                paragraph(2, "Intro"),
                paragraph(3, "#One"),
                paragraph(4, "#Two"),
            ],
            Definitions::default(),
        )
        .expect("a valid document")
    }

    fn entry_texts(operations: &[Operation]) -> Vec<String> {
        operations
            .iter()
            .filter_map(|op| match op {
                Operation::InsertFieldRange { blocks, .. } => Some(blocks),
                _ => None,
            })
            .flatten()
            .map(|block| match block {
                BlockNode::Paragraph(paragraph) => node_plain_text(&paragraph.inlines),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn toc_levels_are_read_out_of_the_instruction() {
        assert_eq!(toc_levels_of(r#" TOC \o "1-5" \h "#), 5);
        assert_eq!(toc_levels_of(r#" TOC \o "1-99" \h "#), MAX_TOC_LEVEL);
        assert_eq!(toc_levels_of(" TOC \\h "), DEFAULT_TOC_LEVEL);
        assert_eq!(toc_levels_of(""), DEFAULT_TOC_LEVEL);
        assert_eq!(toc_level(0), DEFAULT_TOC_LEVEL);
        assert_eq!(toc_level(40), MAX_TOC_LEVEL);
    }

    /// The seam this module exists for: the operations are a function of the
    /// document and the facts the caller lends, so the editor and an agent
    /// that lend the same facts get the same table. The page column is the
    /// facts' labels, verbatim — including a label no page index could
    /// produce — and nothing is read from anywhere else.
    #[test]
    fn a_table_of_contents_prints_the_page_labels_the_caller_supplies() {
        let document = document();
        let facts = Stated {
            pages: HashMap::from([(id(3), "iv".to_owned()), (id(4), "12".to_owned())]),
            whole: true,
        };
        let mut ids = IdGenerator::new(77);
        let placed = insert_table_of_contents(&document, &facts, id(2), 3, 9000, &mut || {
            ids.next_id().ok()
        })
        .expect("lowers");
        assert_eq!(entry_texts(&placed.operations), ["#One\tiv", "#Two\t12"]);
        assert!(
            matches!(
                placed.operations.last(),
                Some(Operation::InsertFieldRange { index: 0, .. })
            ),
            "the table goes before the block holding `before`"
        );
    }

    #[test]
    fn a_layout_that_cannot_answer_for_the_whole_document_is_a_refusal_not_a_guess() {
        let document = document();
        let facts = Stated {
            pages: HashMap::new(),
            whole: false,
        };
        let mut ids = IdGenerator::new(77);
        let refused = insert_table_of_contents(&document, &facts, id(2), 3, 9000, &mut || {
            ids.next_id().ok()
        })
        .expect_err("refused");
        assert_eq!(refused, "one window only");
    }
}
