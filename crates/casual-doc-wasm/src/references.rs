//! Captions and cross-references, host side (`docs/105` OO-005).
//!
//! `casual-doc-edit`'s `references` module writes the OOXML field markup and reads
//! no document. This module is the other half: it reads the document — **once per
//! command** — and turns the host's request into one undoable action.
//!
//! # The performance rule this module exists to obey
//!
//! The feature that most wants to enumerate a whole document is the one that
//! lists every caption in it. The defect this repository already paid for
//! (`docs/116`) was `documentOutline` calling `paragraph_properties` per node —
//! a by-id lookup that is itself a whole-document walk, so 1.3M paragraphs cost
//! 1.3M walks. Every enumerating function here therefore states its complexity,
//! and the shape is always the same: **one walk, carrying what it needs**, never a
//! lookup inside a loop over nodes. `casual_doc_edit::document_scans` counts the
//! walks, and `references_read_the_document_a_bounded_number_of_times` in the test
//! module asserts that count does not grow with the document — which a timing
//! threshold cannot do.
//!
//! Typing stays O(1) because nothing here is on the keystroke path: a caption's
//! number is resolved when the caption is inserted, cached in the model as the
//! field's result, and repainted from the model thereafter. A document full of
//! captions costs a keystroke exactly what a document full of text costs.

use casual_doc_edit::references::{
    BUILT_IN_CAPTION_LABELS, CAPTION_STYLE_NAME, CaptionChapter, CaptionNumberFormat,
    CaptionPosition, CaptionSeparator, CaptionSpec, REFERENCE_BOOKMARK_PREFIX, ReferenceTo,
    above_below, caption_paragraph, caption_style, reference_field,
};
use casual_doc_edit::{Operation, Pos, Surface, surface_of};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Bookmark, BookmarkId, Document, FieldKind, InlineNode, Paragraph, StyleId,
};
use wasm_bindgen::prelude::*;

use crate::{EditResult, HistoryKind, WasmDocument, node_id, node_plain_text, to_js};

/// One caption the document already holds.
///
/// `number` is the caption's 1-based position within its own label's sequence,
/// resolved the way a `SEQ` field resolves it: per sequence name, restarting at
/// the heading level named by a `\s` switch when there is one.
#[derive(Clone, Debug)]
struct CaptionRecord {
    /// The caption paragraph.
    node: NodeId,
    /// Document order among all paragraphs (for above/below and for renumbering).
    order: usize,
    /// The `SEQ` sequence name, which is the caption's label.
    label: String,
    /// The sequence number the document would resolve to if its fields were
    /// updated now — what a renumbering pass computes.
    number: u32,
    /// The number the caption's `SEQ` field currently **shows**: its cached result,
    /// which is what a reader sees and what a picker must therefore list.
    ///
    /// Kept separate from `number` deliberately. Reporting the recomputed number to
    /// the host would make the caption list unable to disagree with the model —
    /// which meant a build that cached the wrong number in the document still
    /// listed the right one, and every numbering guard passed. That was found by
    /// mutating the numbering and watching the guards stay green.
    cached_number: String,
    /// The number format the existing `SEQ` switch asks for.
    format: CaptionNumberFormat,
    /// The caption paragraph's plain text (tabs flattened to spaces).
    text: String,
    /// The paragraph's own inline tree, kept so a renumber can rewrite just the
    /// `SEQ` field's cached run without a second lookup by id.
    inlines: Vec<InlineNode>,
}

/// One cross-reference target: a heading, a bookmark, a note, or a caption.
#[derive(Clone, Debug)]
struct TargetRecord {
    node: NodeId,
    order: usize,
    text: String,
}

/// What a caller wants out of one walk, so the walk allocates nothing per
/// paragraph it was not asked about.
///
/// This struct is the difference between a scan that costs O(captions) memory and
/// one that costs O(document) memory. An earlier shape of this module kept a
/// `(NodeId, order)` row and a nine-word chapter snapshot for **every** paragraph
/// so that two questions could be answered afterwards — about 50 MB of tables on
/// the owner's 1.3M-paragraph file, to look up two nodes. The questions are known
/// before the walk starts, so they are answered during it.
#[derive(Clone, Copy, Default)]
struct ScanRequest<'a> {
    /// The handful of nodes whose document order, chapter ordinal and leading
    /// `_Ref` bookmark the caller needs — a command's target and its caret.
    interest: &'a [NodeId],
    /// Whether each caption keeps its inline tree. Only renumbering needs it, and
    /// only renumbering pays for it.
    keep_inlines: bool,
}

/// Everything one walk of the document yields for this feature.
#[derive(Default)]
struct ReferenceScan {
    /// Every caption, in document order, numbered.
    captions: Vec<CaptionRecord>,
    /// Every heading, in document order, as `(level, node, text)`.
    headings: Vec<(u8, NodeId, String)>,
    /// One row per node in the request's `interest`, recorded as the walk passed
    /// it: its document order, the running chapter ordinal per heading level, and
    /// the `_Ref` bookmark opening at its offset 0 if there is one.
    interest: Vec<InterestRecord>,
    /// How many paragraphs the walk saw, which is where a node the walk never
    /// found would have ranked.
    paragraphs: usize,
}

/// What the walk recorded about one node the caller asked about.
struct InterestRecord {
    node: NodeId,
    order: usize,
    chapters: [u32; 9],
    /// The paragraph's plain-text byte length — the end offset of a bookmark that
    /// covers the whole paragraph.
    ///
    /// This is deliberately NOT the length of the trimmed display text a picker
    /// shows. Using the trimmed length as a bookmark's end offset is a defect this
    /// module shipped for exactly one commit: it never errors (a trimmed length is
    /// never longer), so every test passed, and the bookmark silently covered less
    /// of the paragraph than the reference claimed — which another word processor
    /// then displays as truncated text.
    length: u32,
    /// The byte offset just past the first `SEQ` field's cached result, when the
    /// paragraph is a caption. This is the boundary Word's *Only label and number*
    /// and *Only caption text* reference kinds bookmark on either side of.
    after_sequence: Option<u32>,
    /// Every `_Ref`-prefixed bookmark in the paragraph, as `(id, start, end)`, so a
    /// second reference of the same kind to the same target reuses the bookmark
    /// whose extent actually matches rather than making another.
    reference_bookmarks: Vec<(BookmarkId, u32, u32)>,
}

impl ReferenceScan {
    fn caption_labels(&self) -> Vec<String> {
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

    /// The row the walk recorded for `node`. O(interest), which a command keeps to
    /// the two nodes it asked about — never O(document), which is the whole reason
    /// `ScanRequest::interest` exists.
    fn found(&self, node: NodeId) -> Option<&InterestRecord> {
        self.interest.iter().find(|record| record.node == node)
    }

    /// O(interest). The node's document order, or the end of the document when the walk never
    /// saw it (so a caption asked for against an unknown node sorts last rather
    /// than first, which would silently renumber everything).
    fn order(&self, node: NodeId) -> usize {
        self.found(node)
            .map_or(self.paragraphs, |record| record.order)
    }

    /// O(interest). The 1-based ordinal of the enclosing chapter at `level` — what Word's
    /// `STYLEREF N \s` shows. At least 1: a caption before the first heading
    /// belongs to chapter 1, which is what Word displays there too.
    fn chapter_ordinal(&self, node: NodeId, level: u8) -> u32 {
        let index = usize::from(level.clamp(1, 9)) - 1;
        self.found(node)
            .map_or(0, |record| record.chapters[index])
            .max(1)
    }

    /// The `_Ref` bookmark on `node` covering exactly `[start, end)`, if there is
    /// one. O(interest x bookmarks on that paragraph) — both small by construction.
    fn reference_bookmark(&self, node: NodeId, start: u32, end: u32) -> Option<BookmarkId> {
        self.found(node).and_then(|record| {
            record
                .reference_bookmarks
                .iter()
                .find(|(_, from, to)| *from == start && *to == end)
                .map(|(bookmark, _, _)| *bookmark)
        })
    }
}

/// The `\s` restart level a `SEQ` instruction asks for, if any.
/// O(instruction length); O(1) in document size.
fn sequence_restart_level(instruction: &str) -> Option<u8> {
    let tokens: Vec<&str> = instruction.split_whitespace().collect();
    let index = tokens.iter().position(|token| *token == "\\s")?;
    tokens.get(index + 1)?.parse::<u8>().ok()
}

/// The number format a `SEQ` instruction's `\*` picture switch asks for, or
/// arabic when it has none. O(instruction length); O(1) in document size.
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

/// The cached result text of the first `SEQ` field in an inline tree — the number
/// a reader currently sees. O(inlines in the paragraph's own subtree).
///
/// Descent is [`crate::contained_inlines`]'s, so it reaches a `SEQ` nested in any
/// depth of hyperlink, field, tracked revision or inline content control, and
/// cannot fall out of step with [`sequence_of`]: a caption this function cannot
/// read the cached number of, but `sequence_of` can identify, is a caption the
/// picker lists with an empty number.
fn sequence_cached_number(inlines: &[InlineNode]) -> String {
    fn find(inlines: &[InlineNode]) -> Option<String> {
        for inline in inlines {
            if let InlineNode::Field(field) = inline
                && matches!(field.kind, FieldKind::Seq { .. })
            {
                return Some(node_plain_text(&field.inlines));
            }
            if let Some(nested) = crate::contained_inlines(inline)
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
/// A caption is identified by having one — which is exactly how Word identifies
/// one for a table of figures, and is why it works on a caption a different
/// producer wrote. O(inlines in the paragraph's own subtree).
///
/// A caption inside a hyperlink, a tracked insertion, an inline content control or
/// another field's result is still a caption, and descent is
/// [`crate::contained_inlines`]'s so the set of wrappers searched is the complete
/// one rather than the three that were written out here.
///
/// It deliberately does NOT follow the `TextBox`/`Group` axis, and that is the
/// difference between this and the other walks in this file. A caption inside a
/// text box is a paragraph OF ITS OWN inside that box; it is identified when the
/// paragraph walk reaches it. Descending here would instead label the paragraph
/// that merely *anchors* the box as a caption, so one caption would be counted
/// twice and the numbering would run 1, 1, 2. The reason a caption in a box was
/// missing from the Table of Figures was the paragraph walk — `visit_text_box_paragraphs`
/// skipping `Sdt` and `Revision` — not this function.
fn sequence_of(inlines: &[InlineNode]) -> Option<(String, String)> {
    for inline in inlines {
        if let InlineNode::Field(field) = inline
            && let FieldKind::Seq { name } = &field.kind
        {
            return Some((name.clone(), field.instruction.clone()));
        }
        if let Some(nested) = crate::contained_inlines(inline)
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
/// Offsets are derived from `node_plain_text` rather than by re-implementing its
/// rules: the authoritative definition of "the paragraph's plain text" is that one
/// function, and an offset that disagrees with it by one byte is an edit applied in
/// the wrong place. Each inline contributes `node_plain_text` of itself, which for
/// the four inline containers is exactly the sum of its children's contributions
/// (`append_node_plain_text` recurses through all four), so descending and
/// accumulating gives the same total as measuring the container whole.
///
/// **The walk descends a paragraph's inline containers.** It used to look at
/// top-level inlines only, so a `_Ref…` bookmark pair or a `SEQ` field nested in a
/// hyperlink, an inline content control or a tracked insertion was invisible: the
/// bookmark could not be reused, `after_sequence` was `None`, and an inserted
/// cross-reference to that caption resolved to nothing. `Field` is descended for
/// the bookmark markers inside a field result but its own `SEQ` detection stays at
/// the outermost `SEQ`, which is the caption's.
///
/// It does NOT follow the `TextBox`/`Group` axis: a box's paragraphs have their own
/// offset spaces, and an offset from one of them addressed against this paragraph
/// would place a bookmark somewhere else entirely.
///
/// A legacy form CHECKBOX field is the one container whose plain text is not its
/// children's (layout synthesises a single glyph), so it is not descended — a
/// bookmark inside one has no representable offset here.
///
/// **O(inlines in the paragraph's subtree)**, one pass; it runs for the two
/// paragraphs a command names, never per paragraph of the document. The previous
/// shape re-ran `node_plain_text` over every prefix, which was O(inlines²).
fn paragraph_offsets(
    paragraph: &Paragraph,
    bookmarks: &casual_doc_model::v1::DefinitionMap<BookmarkId, Bookmark>,
) -> (u32, Option<u32>, Vec<(BookmarkId, u32, u32)>) {
    struct Walk<'a> {
        bookmarks: &'a casual_doc_model::v1::DefinitionMap<BookmarkId, Bookmark>,
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
                    // The boundary is past the whole field, label and number, so
                    // the caption's own text starts here.
                    self.after_sequence = Some(self.offset.saturating_add(own_len));
                    self.offset = self.offset.saturating_add(own_len);
                    continue;
                }
                match crate::contained_inlines(inline) {
                    // A form checkbox's painted glyph is not its children's text,
                    // so its interior has no offsets in this space.
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
    // A bookmark whose end marker is in a later paragraph covers this paragraph to
    // its end; reporting it as zero-length would make us reuse it for a reference
    // that then shows nothing.
    for (bookmark, start) in std::mem::take(&mut walk.open) {
        walk.extents.push((bookmark, start, length));
    }
    (length, walk.after_sequence, walk.extents)
}

/// Rewrites the cached result of the first `SEQ` field in `inlines` to `number`,
/// returning the new inline tree — the vehicle for renumbering an existing
/// caption without touching anything else about it. `None` when there is no
/// `SEQ` field or its result already reads `number`, so a renumber that changes
/// nothing produces no operation and no history entry.
/// **O(inlines in the paragraph's subtree)**.
///
/// It rewrote a TOP-LEVEL `SEQ` only, which was consistent with [`sequence_of`]
/// while that function was also top-level and became a half-fix the moment it was
/// not: a caption whose `SEQ` sits inside a hyperlink or a tracked insertion would
/// be LISTED and numbered by the scan and then silently skipped by every renumber,
/// so Update Caption Numbers would report nothing stale while the document read
/// 1, 1, 2. The two must agree about where a caption's `SEQ` can be, so the
/// descent is the same one — [`crate::contained_inlines`].
fn renumbered(
    inlines: &[InlineNode],
    number: &str,
    next_id: &mut impl FnMut() -> Option<NodeId>,
) -> Option<Vec<InlineNode>> {
    /// Rewrites in place, reporting whether it found a `SEQ` and whether the
    /// rewrite changed anything. `None` = no `SEQ` here; `Some(false)` = found one
    /// already reading `number`.
    fn rewrite(
        inlines: &mut [InlineNode],
        number: &str,
        next_id: &mut impl FnMut() -> Option<NodeId>,
    ) -> Option<bool> {
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
                    None => (
                        next_id()?,
                        casual_doc_model::v1::RunProperties::default().into(),
                    ),
                };
                field.inlines = vec![InlineNode::Run(casual_doc_model::v1::Run {
                    id,
                    properties,
                    text: number.to_owned(),
                })];
                return Some(true);
            }
            // The same container set the caption was IDENTIFIED through, so a
            // caption this engine lists is a caption this engine can renumber.
            if let Some(nested) = crate::contained_inlines_mut(inline)
                && let Some(changed) = rewrite(nested, number, next_id)
            {
                return Some(changed);
            }
        }
        None
    }

    let mut out = inlines.to_vec();
    match rewrite(&mut out, number, next_id) {
        Some(true) => Some(out),
        Some(false) | None => None,
    }
}

/// Why a caption cannot attach to `target`, or `None` when it can.
///
/// Word refuses a caption in a header, footer or note, and so does ONLYOFFICE
/// (`apiBuilder.js:12272`: "the current paragraph must be in the document (not in
/// the footer/header)"). Saying so is better than putting it somewhere the reader
/// will not find it.
///
/// This is a function rather than an inline check because a `JsValue` cannot be
/// constructed on a native target, so a refusal asserted only through the binding
/// cannot be tested at all — the assertion panics inside `wasm-bindgen` before it
/// reaches the guard. **O(document), one scan** (`surface_of`).
fn caption_target_refusal(document: &Document, target: NodeId) -> Option<&'static str> {
    match surface_of(document, target) {
        Some(Surface::Body) => None,
        Some(_) => Some(
            "a caption can only be inserted in the document body, not in a header, footer or note",
        ),
        None => Some("the caption's target is not in the document"),
    }
}

/// Where a block sits: the `InsertBlocks` container (`None` = the document body,
/// else the owning cell or content control) and the block's 0-based index in it.
///
/// **O(document), ONE walk** — the body only, because that is the surface
/// `InsertBlocks` addresses and the surface Word itself restricts a caption to
/// (ONLYOFFICE says the same in `apiBuilder.js:12272`: "the current paragraph must
/// be in the document (not in the footer/header)").
///
/// `node` may name the block itself (a paragraph, a table, a content control) or
/// anything inside one of its paragraphs (a selected picture or text box), so a
/// host that has a picture selected does not have to work out which paragraph
/// anchors it.
///
/// "Inside one of its paragraphs" now includes a text box's or a group's own block
/// content, because [`inlines_hold`] follows that axis. What it does NOT do is
/// return the text box as the CONTAINER: the caption for a picture in a box lands
/// in the body beside the block that anchors the box. That is not a judgement
/// about where a caption belongs — it is what the engine can address.
/// `Operation::InsertBlocks` resolves `container` through
/// `find_container_blocks_mut`, which handles table cells and block-level SDTs and
/// has no text-box case, so returning the box's id would build a path that always
/// fails `NodeNotFound`, which is a dead control with extra steps. Inserting inside
/// a box needs that container support first; reported as a row.
fn locate_block(document: &Document, node: NodeId) -> Option<(Option<NodeId>, usize)> {
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
        // cell lands in that cell rather than being refused.
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
                _ => {}
            }
        }
        None
    }
    search(document.body(), None, node)
}

/// Whether an inline tree holds the node `id`, at any depth — including inside a
/// text box's or a group's own block content. **O(nodes in the paragraph's
/// subtree)**, one pass, no by-id lookup.
///
/// It searched the four inline-in-inline containers only, so a picture selected
/// inside a text box or a DrawingML group was not "held" by any paragraph and
/// `locate_block` refused: Insert Caption on the most ordinary floated figure in
/// Word — a picture in a text box with its caption beneath — answered "a caption
/// attaches to something in the document body".
///
/// Both descent axes are followed here because the question is containment, not
/// offsets. The caption still goes in the BODY, beside the block that anchors the
/// box, not inside the box: `Operation::InsertBlocks` resolves a container id
/// through `find_container_blocks_mut`, which knows table cells and block-level
/// SDTs and does not know a text box, so naming the box as the container would
/// build a path that always fails `NodeNotFound`. Placing a caption inside a text
/// box needs that engine-side container support and is reported as a row rather
/// than half-built here.
fn inlines_hold(inlines: &[InlineNode], id: NodeId) -> bool {
    if inlines.iter().any(|inline| inline.id() == id) {
        return true;
    }
    for inline in inlines {
        if let Some(nested) = crate::contained_inlines(inline)
            && inlines_hold(nested, id)
        {
            return true;
        }
    }
    let mut found = false;
    crate::inline_block_stories(inlines, &mut |blocks| {
        found = found || blocks_hold(blocks, id);
    });
    found
}

/// Whether a block list holds the node `id`, at any depth — the block half of
/// [`inlines_hold`]. **O(nodes in the subtree)**.
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

#[wasm_bindgen]
impl WasmDocument {
    /// Every caption label the document uses, unioned with Word's built-ins
    /// (`Figure`, `Table`, `Equation`), sorted — the Insert Caption dialog's label
    /// list. A label is a `SEQ` sequence name, so a document another producer
    /// wrote teaches us its labels without any configuration.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = captionLabels)]
    #[must_use]
    pub fn caption_labels(&self) -> Vec<String> {
        self.scan_references(ScanRequest::default())
            .caption_labels()
    }

    /// Every caption in the document as `"{node}\t{label}\t{number}\t{text}"`
    /// rows in document order — what the cross-reference picker lists and what a
    /// table of figures will enumerate.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = captionEntries)]
    #[must_use]
    pub fn caption_entries(&self) -> Vec<String> {
        self.scan_references(ScanRequest::default())
            .captions
            .iter()
            .map(|caption| {
                format!(
                    "{}\t{}\t{}\t{}",
                    caption.node,
                    caption.label,
                    // The number the document SHOWS, not the one a field update
                    // would compute — a picker lists what the reader sees. The two
                    // agree for every caption this engine wrote, which is what
                    // `a_caption_after_an_existing_one_continues_its_sequence`
                    // asserts.
                    caption.cached_number,
                    caption.text
                )
            })
            .collect()
    }

    /// The cross-reference targets of one reference type, as `"{node}\t{text}"`
    /// rows in document order.
    ///
    /// `kind` is `"heading"`, `"bookmark"`, `"footnote"`, `"endnote"`, or a
    /// caption label (`"Figure"`, `"Table"`, or any label the document uses) —
    /// Word's *Reference type* list. An unknown kind yields no rows rather than an
    /// error, so a host offering a label the document has since stopped using gets
    /// an empty list and says so, instead of throwing.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = referenceTargets)]
    #[must_use]
    pub fn reference_targets(&self, kind: &str) -> Vec<String> {
        let scan = self.scan_references(ScanRequest::default());
        let rows: Vec<TargetRecord> = match kind {
            "heading" => scan
                .headings
                .iter()
                .enumerate()
                .map(|(order, (_, node, text))| TargetRecord {
                    node: *node,
                    order,
                    text: text.clone(),
                })
                .collect(),
            "bookmark" => self.bookmark_targets(),
            "footnote" | "endnote" => self.note_targets(kind == "footnote"),
            label => scan
                .captions
                .iter()
                .filter(|caption| caption.label == label)
                .map(|caption| TargetRecord {
                    node: caption.node,
                    order: caption.order,
                    text: caption.text.clone(),
                })
                .collect(),
        };
        let mut rows = rows;
        rows.sort_by_key(|row| row.order);
        rows.iter()
            .map(|row| format!("{}\t{}", row.node, row.text))
            .collect()
    }

    /// Every caption whose `SEQ` field **shows** a number that disagrees with the
    /// number the document would resolve now, as `"{node}\t{shown}\t{correct}"`.
    ///
    /// # Dirty, not silently stale — and why this exists at all
    ///
    /// Word caches a field's result and leaves it stale until the reader presses
    /// F9; nothing in the document says it is stale, and nothing in the UI says so
    /// either. We diverge deliberately: inserting a caption renumbers what follows
    /// in the same undoable action, so this engine never *creates* staleness. But
    /// two things still produce it and neither is ours to prevent:
    ///
    /// 1. a caption paragraph deleted with Backspace, which is ordinary text
    ///    editing and must not trigger a document-wide renumber (that would make a
    ///    keystroke O(document) — see
    ///    `a_keystroke_costs_a_bounded_number_of_document_scans`);
    /// 2. a document another producer wrote, which arrives with whatever its last
    ///    reader cached.
    ///
    /// So the answer is not "always correct" and not "silently wrong": it is
    /// **reportable**. This is the read a host uses to say so, and
    /// [`updateCaptionNumbers`](Self::update_caption_numbers) is how a reader fixes
    /// it. An empty result means every caption shows the right number.
    ///
    /// **O(document), ONE walk.**
    #[wasm_bindgen(js_name = staleCaptionNumbers)]
    #[must_use]
    pub fn stale_caption_numbers(&self) -> Vec<String> {
        self.scan_references(ScanRequest::default())
            .captions
            .iter()
            .filter_map(|caption| {
                let correct = caption.format.render(caption.number);
                (caption.cached_number != correct)
                    .then(|| format!("{}\t{}\t{}", caption.node, caption.cached_number, correct))
            })
            .collect()
    }

    /// Rewrites every caption's `SEQ` result to the number the document resolves
    /// now — Word's F9 over the caption sequences, as ONE undoable action.
    ///
    /// Refuses with a reason when nothing is stale, rather than pushing an empty
    /// history entry a user would then have to undo twice.
    ///
    /// **O(document)** — one walk, then one operation per caption that actually
    /// changed. A caption already showing the right number produces no operation,
    /// so updating a correct document costs no history.
    #[wasm_bindgen(js_name = updateCaptionNumbers)]
    pub fn update_caption_numbers(&mut self) -> Result<EditResult, JsValue> {
        let (ops, first) = self.caption_renumber_operations();
        let Some(first) = first else {
            return Err(to_js("the document has no captions to update".to_owned()));
        };
        if ops.is_empty() {
            return Err(to_js(
                "every caption already shows the right number".to_owned(),
            ));
        }
        self.apply_action_caret_as(ops, Pos::new(first, 0), HistoryKind::FieldChange)
            .map_err(to_js)
    }

    /// Inserts a caption paragraph above or below the block that holds
    /// `target_node`, and renumbers the captions of the same label that follow it
    /// — Word's Insert Caption, as one undoable action.
    ///
    /// The paragraph is `Caption`-styled; the style is created first when the
    /// document has not got one, which is what Word does and is the difference
    /// between a caption and a line of body text that happens to hold a number.
    ///
    /// `position` is `"above"`/`"below"`, `number_format` and `separator` take the
    /// wire names from `casual_doc_edit::references`, and `chapter_level` is `0`
    /// for "do not include a chapter number" or a 1-based heading level.
    ///
    /// **O(document)**: two walks — one to number and enumerate, one to locate the
    /// target block — plus O(captions after the insertion point) renumbering
    /// operations. Never a lookup by id inside a loop.
    #[wasm_bindgen(js_name = insertCaption)]
    #[allow(clippy::too_many_arguments)]
    pub fn insert_caption(
        &mut self,
        target_node: &str,
        label: &str,
        text: String,
        position: &str,
        exclude_label: bool,
        number_format: &str,
        chapter_level: u8,
        separator: &str,
    ) -> Result<EditResult, JsValue> {
        let target = node_id(target_node)?;
        let label = label.trim();
        if label.is_empty() {
            return Err(to_js("a caption needs a label".into()));
        }
        let position = match position {
            "above" => CaptionPosition::Above,
            "below" => CaptionPosition::Below,
            other => return Err(to_js(format!("unknown caption position {other:?}"))),
        };
        let number_format = CaptionNumberFormat::parse(number_format)
            .ok_or_else(|| to_js(format!("unknown caption number format {number_format:?}")))?;
        let chapter = if chapter_level == 0 {
            None
        } else {
            if !(1..=9).contains(&chapter_level) {
                return Err(to_js("a chapter heading level is 1 to 9".into()));
            }
            Some(CaptionChapter {
                heading_level: chapter_level,
                separator: CaptionSeparator::parse(separator)
                    .ok_or_else(|| to_js(format!("unknown caption separator {separator:?}")))?,
            })
        };

        // Walk once for the numbering and the chapter ordinal, and once for the
        // insertion point. Two constant walks per command, not one per caption.
        if let Some(refusal) = caption_target_refusal(&self.document, target) {
            return Err(to_js(refusal.to_owned()));
        }
        let scan = self.scan_references(ScanRequest {
            interest: &[target],
            keep_inlines: true,
        });
        let (container, index) = locate_block(&self.document, target)
            .ok_or_else(|| to_js("a caption attaches to something in the document body".into()))?;

        let spec = CaptionSpec {
            label: label.to_owned(),
            text,
            position,
            exclude_label,
            number_format,
            chapter,
        };

        // The new caption's number: one more than the captions of this label that
        // precede the insertion point. Read off the already-computed scan, so the
        // count costs nothing extra.
        let target_order = scan.order(target);
        let insert_after = position == CaptionPosition::Below;
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
        let chapter_number = chapter
            .map(|chapter| {
                scan.chapter_ordinal(target, chapter.heading_level)
                    .to_string()
            })
            .unwrap_or_default();

        let mut ops = Vec::new();
        // Word creates the `Caption` style on first use; a document that has not
        // got it would otherwise render the caption as body text — "built is not
        // reachable" in miniature.
        let style = match self.style_id_by_name(CAPTION_STYLE_NAME) {
            Some(existing) => Some(existing),
            None => {
                let id = StyleId::new(self.fresh_id()?);
                let default = self
                    .document
                    .definitions()
                    .styles
                    .iter()
                    .find(|(_, style)| style.is_default)
                    .map(|(id, _)| *id);
                ops.push(Operation::SetStyleDefinition {
                    id,
                    style: Some(Box::new(caption_style(default))),
                });
                Some(id)
            }
        };

        let paragraph_id = self.fresh_id()?;
        let paragraph = {
            let mut allocate = || {
                self.edit_ids
                    .next_id()
                    .map_err(|_| casual_doc_edit::EditError::IdExhausted)
            };
            caption_paragraph(
                paragraph_id,
                style,
                &spec,
                &number_format.render(number),
                &chapter_number,
                &mut allocate,
            )
            .map_err(|error| to_js(format!("{error:?}")))?
        };
        let caret = Pos::new(paragraph_id, 0);
        let insert_index = if insert_after { index + 1 } else { index };
        ops.push(Operation::InsertBlocks {
            container,
            index: insert_index as u32,
            blocks: vec![BlockNode::Paragraph(paragraph)],
        });

        // Renumber the captions of this label that now come later. Word does this
        // too; the alternative is a document whose figures read 1, 1, 2.
        let mut running = number;
        for caption in &scan.captions {
            if caption.label != label || precedes(caption) {
                continue;
            }
            running += 1;
            let rendered = caption.format.render(running);
            let mut allocate = || self.edit_ids.next_id().ok();
            if let Some(inlines) = renumbered(&caption.inlines, &rendered, &mut allocate) {
                ops.push(Operation::SetInlines {
                    node: caption.node,
                    inlines,
                });
            }
        }

        self.apply_action_caret_as(ops, caret, HistoryKind::FieldChange)
            .map_err(to_js)
    }

    /// Inserts a cross-reference field at the caret, pointing at `target_node` —
    /// Word's Cross-reference, as one undoable action.
    ///
    /// The target is bookmarked first when it is not already: Word names its own
    /// automatic bookmarks `_Ref` plus a decimal, and we reuse that prefix and
    /// reuse an existing `_Ref` bookmark on the same target, so a document with
    /// ten references to one figure carries one bookmark rather than ten.
    ///
    /// `reference_to` takes the wire names from `casual_doc_edit::references`
    /// (`"entireCaption"`, `"pageNumber"`, `"aboveBelow"`, …); `hyperlink` writes
    /// the `\h` switch, which is Word's *Insert as hyperlink*.
    ///
    /// `include_above_below` is Word's *Include above/below* checkbox, and it is
    /// **two fields, one action**: the reference, then a space, then a second
    /// ` REF … \p ` naming the same bookmark — which is exactly what Word writes
    /// for it. It is ignored for a reference that already IS the above/below word.
    /// Taking it as an argument rather than leaving the checkbox disabled is
    /// deliberate: a control that cannot run is a dead control (`SKILL.md` §10),
    /// and the alternative was shipping one.
    ///
    /// **O(document)** — one walk for the target's text and document order, plus,
    /// for `"pageNumber"` only, one layout query for the page the target is on.
    /// That query is the single hard coupling to layout in this feature, and it is
    /// why a page reference is the one result that can go stale after a reflow,
    /// exactly as it does in Word.
    #[wasm_bindgen(js_name = insertCrossReference)]
    pub fn insert_cross_reference(
        &mut self,
        caret_node: &str,
        caret_offset: u32,
        target_node: &str,
        reference_to: &str,
        hyperlink: bool,
        include_above_below: bool,
    ) -> Result<EditResult, JsValue> {
        let caret = Pos::new(node_id(caret_node)?, caret_offset);
        let target = node_id(target_node)?;
        let to = ReferenceTo::parse(reference_to)
            .ok_or_else(|| to_js(format!("unknown reference kind {reference_to:?}")))?;

        let scan = self.scan_references(ScanRequest {
            interest: &[target, caret.node],
            keep_inlines: false,
        });
        // The target's OWN plain text, untrimmed, because these offsets address the
        // model. The trimmed text a picker displays is a different string and using
        // it here silently shortens every bookmark.
        let offsets = scan
            .found(target)
            .ok_or_else(|| to_js("the cross-reference target is not in the document".into()))?;
        let length = offsets.length;
        let after_sequence = offsets.after_sequence;
        let text = self
            .paragraph_text_of(target)
            .ok_or_else(|| to_js("the cross-reference target is not in the document".into()))?;

        // Word's three caption reference kinds differ ONLY in what the bookmark
        // covers, not in the field instruction — "Entire caption", "Only label and
        // number" and "Only caption text" are all a plain `REF`. The boundary is the
        // end of the `SEQ` field: before it is the label and number, after it is the
        // author's text. Offering all three while bookmarking the whole paragraph
        // for each would put three rows in the dialog that do the same thing, which
        // is worse than offering one.
        let (start, end) = match (to, after_sequence) {
            (ReferenceTo::LabelAndNumber, Some(boundary)) => (0, boundary),
            (ReferenceTo::CaptionTextOnly, Some(boundary)) => (boundary, length),
            // A target with no `SEQ` field — a heading or a bookmark — has no such
            // boundary, so those two kinds fall back to the whole paragraph rather
            // than to an empty range. The host does not offer them for a
            // non-caption target; this is the engine refusing to produce an empty
            // reference if it ever does.
            _ => (0, length),
        };
        let result_text_from_range = || {
            text.get(start as usize..end as usize)
                .unwrap_or(&text)
                .trim()
                .replace('\t', " ")
        };

        let mut ops = Vec::new();
        // Reuse an existing `_Ref` bookmark on the target ONLY when its extent is
        // the one this reference kind needs, so ten "entire caption" references share
        // one bookmark while a "label and number" reference gets its own.
        let (bookmark_name, bookmark_id) = match scan.reference_bookmark(target, start, end) {
            Some(existing) => (
                self.document
                    .definitions()
                    .bookmarks
                    .get(&existing)
                    .map(|bookmark| bookmark.name.clone())
                    .unwrap_or_default(),
                Some(existing),
            ),
            None => (self.fresh_reference_bookmark_name(), None),
        };
        if bookmark_id.is_none() {
            let bookmark = BookmarkId::new(self.fresh_id()?);
            let start_id = self.fresh_id()?;
            let end_id = self.fresh_id()?;
            ops.push(Operation::CreateBookmark {
                bookmark,
                name: bookmark_name.clone(),
                start: Pos::new(target, start),
                start_id,
                end: Pos::new(target, end),
                end_id,
            });
        }

        let result_text = match to {
            ReferenceTo::PageNumber => self.page_of(target).map(|page| page.to_string()),
            ReferenceTo::AboveBelow => {
                Some(above_below(scan.order(target), scan.order(caret.node)).to_owned())
            }
            _ => Some(result_text_from_range()),
        }
        .unwrap_or_default();

        let field_id = self.fresh_id()?;
        let result_id = self.fresh_id()?;
        let field = reference_field(
            field_id,
            result_id,
            to,
            &bookmark_name,
            hyperlink,
            &result_text,
        )
        .map_err(|error| to_js(format!("{error:?}")))?;
        ops.push(Operation::InsertField {
            at: caret,
            field: Box::new(field),
        });

        // *Include above/below*: a space and a second ` REF … \p ` after the
        // reference, which is Word's own markup for that checkbox. Two fields in
        // ONE undoable action, so a reader who regrets it presses undo once.
        //
        // Each op inserts at an offset in the paragraph AS THE PREVIOUS OPS LEFT
        // IT, so the space goes after the reference's own cached text and the `\p`
        // field after the space. Pushing all three at `caret` reverses them —
        // measured, before this arithmetic: " belowFigure 1: Wiring diagram".
        if include_above_below && to != ReferenceTo::AboveBelow {
            let word = above_below(scan.order(target), scan.order(caret.node));
            let space_field = self.fresh_id()?;
            let space_result = self.fresh_id()?;
            // A field, not a bare run: a bare run would merge with the surrounding
            // text and a later edit could not tell the two references apart.
            let above = reference_field(
                space_field,
                space_result,
                ReferenceTo::AboveBelow,
                &bookmark_name,
                hyperlink,
                word,
            )
            .map_err(|error| to_js(format!("{error:?}")))?;
            let after_reference = caret.offset.saturating_add(result_text.len() as u32);
            ops.push(Operation::InsertText {
                at: Pos::new(caret.node, after_reference),
                text: " ".to_owned(),
            });
            ops.push(Operation::InsertField {
                at: Pos::new(caret.node, after_reference.saturating_add(1)),
                field: Box::new(above),
            });
        }

        self.apply_action_caret_as(ops, caret, HistoryKind::FieldChange)
            .map_err(to_js)
    }
}

impl WasmDocument {
    /// The operations an update would apply, and the first caption's node (for the
    /// caret). Empty operations means nothing is stale.
    ///
    /// Factored out of [`update_caption_numbers`](Self::update_caption_numbers) so
    /// that its two refusals are testable: a `JsValue` cannot be constructed on a
    /// native target, so a refusal asserted only through the binding panics inside
    /// `wasm-bindgen` before it reaches the code under test — which is how a
    /// refusal ships unguarded.
    ///
    /// **O(document), ONE walk.**
    fn caption_renumber_operations(&mut self) -> (Vec<Operation>, Option<NodeId>) {
        let scan = self.scan_references(ScanRequest {
            interest: &[],
            keep_inlines: true,
        });
        let mut ops = Vec::new();
        for caption in &scan.captions {
            let correct = caption.format.render(caption.number);
            let mut allocate = || self.edit_ids.next_id().ok();
            if let Some(inlines) = renumbered(&caption.inlines, &correct, &mut allocate) {
                ops.push(Operation::SetInlines {
                    node: caption.node,
                    inlines,
                });
            }
        }
        (ops, scan.captions.first().map(|caption| caption.node))
    }

    /// ONE walk of every block surface, carrying everything captions and
    /// cross-references need.
    ///
    /// **O(document) time, and exactly one whole-document scan** (the one
    /// `surface_block_lists` performs). Memory is O(captions + headings + the
    /// request's `interest`) — nothing per paragraph. Every field of the result is
    /// filled from this one traversal rather than from a per-node lookup, which is
    /// the difference between this and the `documentOutline` defect of `docs/116`.
    fn scan_references(&self, request: ScanRequest<'_>) -> ReferenceScan {
        let cascade = casual_doc_layout::cascade::StyleCascade::new(self.document.definitions());
        let bookmarks = &self.document.definitions().bookmarks;
        let mut scan = ReferenceScan::default();
        // The running per-label sequence counter, and the `\s` restart level each
        // label asks for, so a heading of that level zeroes it — which is the
        // whole meaning of `SEQ … \s N`.
        let mut counters: Vec<(String, u32, Option<u8>)> = Vec::new();
        let mut chapters = [0u32; 9];
        let mut order = 0usize;
        crate::visit_paragraphs_all_surfaces(&self.document, &mut |paragraph| {
            if let Some(level) = self.heading_level_of(paragraph.properties.get(), &cascade) {
                let index = usize::from(level.clamp(1, 9)) - 1;
                chapters[index] = chapters[index].saturating_add(1);
                // A heading resets every deeper level's counter, as a multilevel
                // list does.
                for deeper in chapters.iter_mut().skip(index + 1) {
                    *deeper = 0;
                }
                // And it restarts every sequence that names this level, which is
                // the whole meaning of `SEQ … \s N`.
                for (_, count, restart) in counters.iter_mut() {
                    if *restart == Some(level) {
                        *count = 0;
                    }
                }
                let trimmed = node_plain_text(&paragraph.inlines)
                    .trim()
                    .replace('\t', " ");
                if !trimmed.is_empty() {
                    scan.headings.push((level, paragraph.id, trimmed));
                }
            }
            // Only the nodes the caller named get a row; a document-wide
            // enumeration names none and so allocates none.
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
                let entry = match counters.iter_mut().find(|(name, _, _)| *name == label) {
                    Some(entry) => entry,
                    None => {
                        counters.push((label.clone(), 0, restart));
                        counters.last_mut().expect("just pushed")
                    }
                };
                entry.1 = entry.1.saturating_add(1);
                scan.captions.push(CaptionRecord {
                    node: paragraph.id,
                    order,
                    label,
                    number: entry.1,
                    cached_number: sequence_cached_number(&paragraph.inlines),
                    format: sequence_format(&instruction),
                    text: node_plain_text(&paragraph.inlines)
                        .trim()
                        .replace('\t', " "),
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
    /// paragraph its start marker sits in.
    ///
    /// **O(document), ONE walk.** The document order comes from the walk's own
    /// counter rather than from a lookup per bookmark: asking a table for each
    /// bookmark's paragraph would be O(bookmarks x paragraphs), and
    /// `document_scans` would NOT have caught that, because it is a linear search
    /// over a table rather than a second scan of the document. Not every quadratic
    /// in this area is a scan.
    ///
    /// The per-paragraph part descends the paragraph's inline containers, because a
    /// bookmark start marker inside a hyperlink, an inline content control or a
    /// tracked insertion is an authored bookmark like any other, and leaving it out
    /// meant the cross-reference picker offered no row for it — the same top-level
    /// blind spot as `paragraph_offsets`, one function along. O(inlines in the
    /// paragraph's subtree) per paragraph, so the whole walk stays O(document).
    fn bookmark_targets(&self) -> Vec<TargetRecord> {
        fn collect(
            inlines: &[InlineNode],
            bookmarks: &casual_doc_model::v1::DefinitionMap<BookmarkId, Bookmark>,
            node: NodeId,
            order: usize,
            rows: &mut Vec<TargetRecord>,
        ) {
            for inline in inlines {
                if let InlineNode::BookmarkStart(marker) = inline
                    && let Some(bookmark) = bookmarks.get(&marker.bookmark)
                    // Word hides its own `_Ref`/`_Toc` bookkeeping bookmarks from
                    // this list; so do we, or the picker fills with machine names.
                    && !bookmark.name.starts_with('_')
                {
                    rows.push(TargetRecord {
                        node,
                        order,
                        text: bookmark.name.clone(),
                    });
                }
                if let Some(nested) = crate::contained_inlines(inline) {
                    collect(nested, bookmarks, node, order, rows);
                }
            }
        }

        let mut rows = Vec::new();
        let bookmarks = &self.document.definitions().bookmarks;
        let mut order = 0usize;
        crate::visit_paragraphs_all_surfaces(&self.document, &mut |paragraph| {
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

    /// Footnotes or endnotes as cross-reference targets, numbered in definition
    /// order with their first paragraph's text. **O(notes)** — the note
    /// definitions are their own map, so this needs no document walk.
    fn note_targets(&self, footnotes: bool) -> Vec<TargetRecord> {
        let definitions = self.document.definitions();
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
                let text = node_plain_text(&first.inlines).trim().replace('\t', " ");
                Some(TargetRecord {
                    node: first.id,
                    order,
                    text,
                })
            })
            .collect()
    }

    /// The plain text of the paragraph `node`, or `None`.
    ///
    /// **O(document), ONE walk.** Called once per cross-reference insertion, for
    /// the targets the scan did not already name (a bookmark or a note).
    fn paragraph_text_of(&self, node: NodeId) -> Option<String> {
        let mut found = None;
        crate::visit_paragraphs_all_surfaces(&self.document, &mut |paragraph| {
            if paragraph.id == node && found.is_none() {
                found = Some(
                    node_plain_text(&paragraph.inlines)
                        .trim()
                        .replace('\t', " "),
                );
            }
        });
        found
    }

    /// The 1-based page the model position at the start of `node` is painted on,
    /// or `None` when the layout cannot say (a windowed document, or a node the
    /// painted layout does not hold).
    ///
    /// **O(lines in the document)** — one geometry query, once per inserted page
    /// reference. It is deliberately not called per caption: doing that inside a
    /// loop is the shape of the `docs/116` defect, and a table of contents will
    /// need a single index instead.
    fn page_of(&self, node: NodeId) -> Option<u32> {
        let snapshot = self.painted_snapshot();
        snapshot
            .caret_rect(self.view_pos(casual_doc_layout::model::ModelPos::new(node, 0)))
            .map(|(page, _)| page)
    }

    /// A fresh `_Ref`-prefixed bookmark name no bookmark in the document uses.
    ///
    /// Word's own automatic names are `_Ref` plus a decimal, and ours are too, so a
    /// reader cannot tell which produced the file. One pass over the bookmark
    /// definitions, taking one past the highest number already used rather than
    /// probing candidates — probing is O(bookmarks) per candidate. **O(bookmarks).**
    fn fresh_reference_bookmark_name(&self) -> String {
        let highest = self
            .document
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

    /// One fresh node id, or a readable refusal.
    fn fresh_id(&mut self) -> Result<NodeId, JsValue> {
        self.edit_ids
            .next_id()
            .map_err(|_| to_js("id space exhausted".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_document;

    /// The one genuinely Microsoft-Word-produced file in the repository, so a
    /// round-trip assertion is against a real producer's package rather than our
    /// own writer's idea of one.
    const SAMPLE_DOCX: &[u8] = include_bytes!("../../../sample.docx");

    /// Arabic, below, label shown, no chapter number — the defaults Word's dialog
    /// opens with.
    fn insert_figure(document: &mut WasmDocument, target: &str, text: &str) -> Result<(), String> {
        document
            .insert_caption(
                target,
                "Figure",
                text.to_owned(),
                "below",
                false,
                "arabic",
                0,
                "hyphen",
            )
            .map(|_| ())
            .map_err(|error| format!("{error:?}"))
    }

    /// `captionEntries` parsed into `(label, number, text)`, dropping the node id.
    fn captions(document: &WasmDocument) -> Vec<(String, String, String)> {
        document
            .caption_entries()
            .iter()
            .map(|row| {
                let mut parts = row.split('\t');
                let _node = parts.next().unwrap_or_default();
                (
                    parts.next().unwrap_or_default().to_owned(),
                    parts.next().unwrap_or_default().to_owned(),
                    parts.collect::<Vec<_>>().join(" "),
                )
            })
            .collect()
    }

    /// A plain-text document of `lines` paragraphs — the cheapest way to get a
    /// document of a known size, and the same shape the class guard uses.
    fn plain_text_of(lines: usize) -> Vec<u8> {
        "A paragraph of ordinary body text.\r\n"
            .repeat(lines)
            .into_bytes()
    }

    /// Every top-level body paragraph id, in document order.
    fn body_paragraph_ids(document: &WasmDocument) -> Vec<String> {
        document
            .document
            .body()
            .iter()
            .filter_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
                _ => None,
            })
            .collect()
    }

    fn first_body_paragraph(document: &WasmDocument) -> String {
        document
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id.to_string()),
                _ => None,
            })
            .expect("the fixture has a body paragraph")
    }

    /// The exported package's `word/document.xml`, as text — the only way to
    /// assert what another word processor will actually read.
    fn exported_document_xml(document: &WasmDocument) -> String {
        let bytes = document.export_docx().expect("export docx");
        let mut package = crate::DocxPackage::open(&bytes, crate::viewer_limits())
            .expect("the export opens as a package");
        String::from_utf8(
            package
                .read_part("word/document.xml")
                .expect("read document.xml"),
        )
        .expect("document.xml is utf-8")
    }

    #[test]
    fn a_caption_is_numbered_from_one_and_reads_back_with_its_label() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert");

        assert_eq!(
            captions(&document),
            vec![(
                "Figure".to_owned(),
                "1".to_owned(),
                "Figure 1: Wiring diagram".to_owned()
            )],
            "the first caption of a label is number 1 and reads back whole"
        );
        assert_eq!(
            document.undo_label(),
            "Field change",
            "a caption is one undoable action a user can name"
        );
    }

    #[test]
    fn a_caption_inserted_before_an_existing_one_renumbers_the_one_after_it() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body: Vec<String> = body_paragraph_ids(&document);
        // Caption the LATER paragraph first, so the second insertion lands before
        // it and must renumber it. A document that reads "Figure 1, Figure 1" is
        // the defect this guards.
        insert_figure(&mut document, &body[3], ": the later one").expect("insert later");
        insert_figure(&mut document, &body[0], ": the earlier one").expect("insert earlier");

        let numbers: Vec<(String, String)> = captions(&document)
            .into_iter()
            .map(|(_, number, text)| (number, text))
            .collect();
        assert_eq!(
            numbers,
            vec![
                ("1".to_owned(), "Figure 1: the earlier one".to_owned()),
                ("2".to_owned(), "Figure 2: the later one".to_owned()),
            ],
            "inserting a caption before another renumbers it, as Word does"
        );
    }

    #[test]
    fn undoing_a_caption_removes_the_paragraph_and_puts_the_numbers_back() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body: Vec<String> = body_paragraph_ids(&document);
        insert_figure(&mut document, &body[3], ": the later one").expect("insert later");
        let before = captions(&document);
        insert_figure(&mut document, &body[0], ": the earlier one").expect("insert earlier");
        document.undo().expect("undo");

        assert_eq!(
            captions(&document),
            before,
            "one undo removes the caption AND reverses the renumbering it caused"
        );
    }

    #[test]
    fn a_caption_survives_a_word_round_trip_as_the_field_word_writes() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert");

        let xml = exported_document_xml(&document);
        assert!(
            xml.contains(" SEQ Figure \\* ARABIC "),
            "the export must carry the SEQ instruction verbatim, not a rendered number"
        );
        // `w:pStyle w:val` carries the exported style ID token, not the style's
        // human name, so the assertion that matters to another reader is that
        // `styles.xml` defines a style NAMED Caption and that the caption
        // paragraph resolves to it after a round trip.
        let reopened = open_document(&document.export_docx().expect("export")).expect("reopen");
        let caption_node = reopened.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();
        assert_eq!(
            reopened.paragraph_style_at(&caption_node),
            "Caption",
            "the caption paragraph must resolve to the Caption style after a round trip, \
             or another reader renders it as body text"
        );
        assert_eq!(
            captions(&reopened),
            captions(&document),
            "a caption read back out of our own DOCX is the same caption"
        );
    }

    #[test]
    fn include_chapter_number_writes_a_styleref_and_restarts_the_sequence() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        document
            .insert_caption(
                &target,
                "Figure",
                ": with a chapter".to_owned(),
                "below",
                false,
                "arabic",
                1,
                "hyphen",
            )
            .expect("insert");

        let xml = exported_document_xml(&document);
        assert!(
            xml.contains(" STYLEREF 1 \\s "),
            "the chapter number is a STYLEREF field, which is how Word writes it"
        );
        assert!(
            xml.contains(" SEQ Figure \\* ARABIC \\s 1 "),
            "the SEQ must carry the restart switch, or the number never restarts"
        );
    }

    #[test]
    fn a_caption_is_refused_outside_the_body_with_a_reason_rather_than_silently_misplaced() {
        const HEADER_FOOTER_DOCX: &[u8] =
            include_bytes!("../../../fixtures/corpus/real-producer-header-footer.docx");
        let document = open_document(HEADER_FOOTER_DOCX).expect("open corpus docx");
        let header = document
            .document
            .definitions()
            .headers
            .iter()
            .flat_map(|(_, header)| header.blocks.iter())
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                _ => None,
            })
            .expect("the fixture carries a header paragraph, or this guard proves nothing");
        let refusal = caption_target_refusal(&document.document, header)
            .expect("a caption in a header must be refused");
        assert!(
            refusal.contains("body"),
            "the refusal must say WHY, so the control can be disabled with a reason \
             rather than doing nothing: {refusal}"
        );

        // And the body is not refused, or the guard would pass by refusing
        // everything.
        let body = document
            .document
            .body()
            .iter()
            .find_map(|block| match block {
                BlockNode::Paragraph(paragraph) => Some(paragraph.id),
                _ => None,
            })
            .expect("the fixture has a body paragraph");
        assert_eq!(
            caption_target_refusal(&document.document, body),
            None,
            "a body paragraph must NOT be refused"
        );
    }

    #[test]
    fn a_cross_reference_points_at_one_bookmark_however_many_references_use_it() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();

        let before = document.document.definitions().bookmarks.iter().count();
        for _ in 0..3 {
            document
                .insert_cross_reference(&target, 0, &caption, "entireCaption", true, false)
                .expect("insert cross-reference");
        }
        let after = document.document.definitions().bookmarks.iter().count();
        assert_eq!(
            after - before,
            1,
            "three references to one caption must share one bookmark, not make three"
        );
    }

    #[test]
    fn every_reference_kind_caches_the_result_a_reader_will_see() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();

        // The whole caption's text.
        document
            .insert_cross_reference(&target, 0, &caption, "entireCaption", true, false)
            .expect("entire caption");
        let xml = exported_document_xml(&document);
        assert!(
            xml.contains(" REF _Ref"),
            "a text cross-reference is a REF field: {}",
            &xml[..0]
        );

        // The page, which comes from layout and is the one hard coupling.
        document
            .insert_cross_reference(&target, 0, &caption, "pageNumber", true, false)
            .expect("page number");
        let xml = exported_document_xml(&document);
        assert!(
            xml.contains(" PAGEREF _Ref"),
            "a page cross-reference is a PAGEREF field"
        );

        // Above or below, which comes from document order.
        document
            .insert_cross_reference(&target, 0, &caption, "aboveBelow", false, false)
            .expect("above/below");
        let xml = exported_document_xml(&document);
        assert!(xml.contains("\\p "), "above/below is the REF \\p switch");
        assert!(
            xml.contains(">below<") || xml.contains(">above<"),
            "the cached result must be the word a reader sees, not an empty field"
        );
    }

    #[test]
    fn a_cross_reference_survives_a_round_trip_with_its_bookmark_intact() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();
        document
            .insert_cross_reference(&target, 0, &caption, "entireCaption", true, false)
            .expect("insert cross-reference");

        let reopened = open_document(&document.export_docx().expect("export")).expect("reopen");
        let names: Vec<String> = reopened
            .document
            .definitions()
            .bookmarks
            .iter()
            .map(|(_, bookmark)| bookmark.name.clone())
            .collect();
        assert!(
            names.iter().any(|name| name.starts_with("_Ref")),
            "the target's bookmark must survive the round trip, or the REF dangles: {names:?}"
        );
        // And the field is still a REF field pointing at that name, not flattened
        // into its cached text.
        let referenced: Vec<String> = {
            let mut found = Vec::new();
            crate::visit_paragraphs_all_surfaces(&reopened.document, &mut |paragraph| {
                for inline in &paragraph.inlines {
                    if let InlineNode::Field(field) = inline
                        && let FieldKind::Ref { bookmark } = &field.kind
                    {
                        found.push(bookmark.clone());
                    }
                }
            });
            found
        };
        assert!(
            referenced.iter().any(|name| name.starts_with("_Ref")),
            "the reopened document must still hold a REF field: {referenced:?}"
        );
    }

    #[test]
    fn reference_targets_answer_per_reference_type_and_refuse_nothing_silently() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");

        assert!(
            !document.reference_targets("heading").is_empty(),
            "the fixture has headings, so the heading type must offer targets"
        );
        assert_eq!(
            document.reference_targets("Figure").len(),
            1,
            "the Figure type offers the one caption that exists"
        );
        assert!(
            document.reference_targets("Table").is_empty(),
            "a label with no captions offers none — an empty list, not an error"
        );
        assert!(
            document.reference_targets("nonsense").is_empty(),
            "an unknown type yields an empty list the host can explain, not a throw"
        );
        assert!(
            document.caption_labels().contains(&"Figure".to_owned()),
            "the label list carries the built-ins"
        );
    }

    /// A label the document invented, not one of ours, must appear in the label
    /// list and number independently — the property that makes this work on a file
    /// someone else wrote.
    #[test]
    fn a_label_the_document_invented_gets_its_own_counter() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body: Vec<String> = body_paragraph_ids(&document);
        insert_figure(&mut document, &body[0], ": a figure").expect("figure");
        document
            .insert_caption(
                &body[3],
                "Listing",
                ": a listing".to_owned(),
                "below",
                false,
                "arabic",
                0,
                "hyphen",
            )
            .expect("listing");

        assert!(
            document.caption_labels().contains(&"Listing".to_owned()),
            "a label the document uses joins the list"
        );
        let numbers: Vec<(String, String)> = captions(&document)
            .into_iter()
            .map(|(label, number, _)| (label, number))
            .collect();
        assert_eq!(
            numbers,
            vec![
                ("Figure".to_owned(), "1".to_owned()),
                ("Listing".to_owned(), "1".to_owned()),
            ],
            "two labels are two sequences; neither continues the other"
        );
    }

    /// A caption inserted AFTER an existing one of the same label continues its
    /// sequence.
    ///
    /// Separate from the renumbering guard on purpose: that one inserts backwards,
    /// so a build that numbered every caption `1` and then renumbered what follows
    /// still produced 1, 2 and passed. Inserting forwards is the case where the new
    /// caption's own number has to count what is already there.
    #[test]
    fn a_caption_after_an_existing_one_continues_its_sequence() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body: Vec<String> = body_paragraph_ids(&document);
        insert_figure(&mut document, &body[0], ": the first").expect("first");
        insert_figure(&mut document, &body[3], ": the second").expect("second");
        insert_figure(&mut document, &body[5], ": the third").expect("third");

        let numbers: Vec<String> = captions(&document)
            .into_iter()
            .map(|(_, number, _)| number)
            .collect();
        assert_eq!(
            numbers,
            vec!["1".to_owned(), "2".to_owned(), "3".to_owned()],
            "captions inserted in document order must number 1, 2, 3"
        );
    }

    /// The caption paragraph carries the `Caption` style, and the style is created
    /// when the document has not got one.
    ///
    /// Its own guard rather than a line in the round-trip test, because the
    /// round-trip test passed with the styling removed: the caption's *text* is the
    /// same either way, and a caption that renders as body text is exactly the
    /// "built is not reachable" failure `docs/99` §9.4 records.
    #[test]
    fn a_caption_is_styled_as_a_caption_and_the_style_is_created_if_missing() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert");

        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();
        assert_eq!(
            document.paragraph_style_at(&caption),
            CAPTION_STYLE_NAME,
            "the caption paragraph must resolve to the Caption style, or it renders \
             as body text"
        );
        assert!(
            document
                .list_styles()
                .contains(&CAPTION_STYLE_NAME.to_owned()),
            "the Caption style must exist in the document, as Word creates it"
        );
    }

    /// Word's three caption reference kinds show three DIFFERENT things.
    ///
    /// They are all a plain `REF` field — what differs is the extent of the bookmark
    /// it names, either side of the `SEQ` field. A build that bookmarked the whole
    /// caption for each would put three rows in the dialog that do the same thing,
    /// and every instruction-level assertion would still pass, because the
    /// instructions ARE identical. So this guard reads the cached result, which is
    /// the only thing that differs.
    #[test]
    fn the_three_caption_reference_kinds_show_three_different_things() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();

        let shown = |document: &mut WasmDocument, kind: &str| {
            document
                .insert_cross_reference(&target, 0, &caption, kind, false, false)
                .expect("insert cross-reference");
            // The field just inserted is the first `REF`/`PAGEREF` in the caret's
            // paragraph; read its cached result.
            let mut text = String::new();
            crate::visit_paragraphs_all_surfaces(&document.document, &mut |paragraph| {
                if paragraph.id.to_string() != target || !text.is_empty() {
                    return;
                }
                for inline in &paragraph.inlines {
                    if let InlineNode::Field(field) = inline
                        && matches!(
                            field.kind,
                            FieldKind::Ref { .. } | FieldKind::PageRef { .. }
                        )
                    {
                        text = node_plain_text(&field.inlines);
                        return;
                    }
                }
            });
            document.undo().expect("undo the reference");
            text
        };

        let entire = shown(&mut document, "entireCaption");
        let label_and_number = shown(&mut document, "labelAndNumber");
        let caption_text = shown(&mut document, "captionText");

        assert_eq!(entire, "Figure 1: Wiring diagram");
        assert_eq!(
            label_and_number, "Figure 1",
            "\"only label and number\" must stop at the end of the SEQ field"
        );
        assert_eq!(
            caption_text, ": Wiring diagram",
            "\"only caption text\" must start at the end of the SEQ field"
        );
        assert!(
            entire != label_and_number && entire != caption_text,
            "three reference kinds that show the same string are three dialog rows \
             that do the same thing"
        );
    }

    /// A whole-caption reference bookmarks the WHOLE caption, not the trimmed
    /// display string a picker shows.
    ///
    /// Using the trimmed length as the bookmark's end offset never errors — a
    /// trimmed length is never longer — so it passes every test that only checks the
    /// instruction, and another word processor then shows truncated text. The
    /// fixture's caption ends in a space so trimmed and untrimmed differ.
    #[test]
    fn a_reference_bookmark_covers_the_whole_target_not_its_trimmed_display_text() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);
        insert_figure(&mut document, &target, ": Wiring diagram   ").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();
        let caption_id = caption.parse::<NodeId>().expect("a node id");
        document
            .insert_cross_reference(&target, 0, &caption, "entireCaption", false, false)
            .expect("insert cross-reference");

        // The bookmark's end marker must sit at the caption paragraph's real end.
        let mut length = 0u32;
        let mut end_offset = None;
        crate::visit_paragraphs_all_surfaces(&document.document, &mut |paragraph| {
            if paragraph.id != caption_id {
                return;
            }
            length = node_plain_text(&paragraph.inlines).len() as u32;
            for (index, inline) in paragraph.inlines.iter().enumerate() {
                if matches!(inline, InlineNode::BookmarkEnd(_)) {
                    end_offset = Some(node_plain_text(&paragraph.inlines[..index]).len() as u32);
                }
            }
        });
        assert!(
            length > 0,
            "the caption must have text, or this proves nothing"
        );
        assert_eq!(
            end_offset,
            Some(length),
            "the bookmark must end at the caption's real end ({length} bytes), not at \
             the end of its trimmed display text"
        );
    }

    /// *Include above/below* inserts the word as a SECOND field, in one action.
    ///
    /// Word writes two fields for that checkbox — the reference, a space, and a
    /// ` REF … \p ` — and the alternative to taking the argument was shipping the
    /// checkbox disabled, which is a dead control.
    #[test]
    fn include_above_below_adds_a_second_reference_field_in_the_same_action() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body = body_paragraph_ids(&document);
        // Caption a LATER paragraph and reference it from an earlier one, so the
        // word is "below" — a guard that could only ever read "above" would pass
        // on a build that hard-coded it.
        insert_figure(&mut document, &body[4], ": Wiring diagram").expect("insert caption");
        let caption = document.caption_entries()[0]
            .split('\t')
            .next()
            .expect("the caption's node id")
            .to_owned();

        document
            .insert_cross_reference(&body[0], 0, &caption, "entireCaption", true, true)
            .expect("insert with above/below");

        let mut instructions = Vec::new();
        let mut text = String::new();
        let anchor = body[0].parse::<NodeId>().expect("a node id");
        crate::visit_paragraphs_all_surfaces(&document.document, &mut |paragraph| {
            if paragraph.id != anchor {
                return;
            }
            text = node_plain_text(&paragraph.inlines);
            for inline in &paragraph.inlines {
                if let InlineNode::Field(field) = inline {
                    instructions.push(field.instruction.clone());
                }
            }
        });
        assert_eq!(
            instructions.len(),
            2,
            "two fields: the reference and the above/below word — got {instructions:?}"
        );
        assert!(
            instructions.iter().any(|i| i.contains("\\p")),
            "the second field must be the REF \\p Word writes: {instructions:?}"
        );
        assert!(
            text.starts_with("Figure 1: Wiring diagram below"),
            "the paragraph must read the reference, a space, then the word: {text:?}"
        );
        // ONE undo, not three. This is the assertion that the three ops are one
        // action; the undo LABEL cannot say so, because the caption insertion before
        // it is also a "Field change" and comparing labels would pass either way.
        document.undo().expect("undo");
        let mut after = String::new();
        crate::visit_paragraphs_all_surfaces(&document.document, &mut |paragraph| {
            if paragraph.id == anchor {
                after = node_plain_text(&paragraph.inlines);
            }
        });
        assert!(
            !after.contains("below") && !after.contains("Figure 1"),
            "ONE undo must remove both fields and the space: {after:?}"
        );
    }

    /// Inserting a field leaves the caret AFTER it, not before it.
    ///
    /// The engine rested it at the insertion point and a comment there asserted "a
    /// field is zero-width in the edit anchor space" — which is false: a field's
    /// cached result is part of the paragraph's plain text, because
    /// `node_plain_text` recurses into `Field::inlines`. The caret therefore landed
    /// immediately BEFORE the field a reader had just inserted.
    ///
    /// It was invisible for as long as a field's own glyphs carried no caret stops:
    /// the caret fell back to another position in the line and landed somewhere
    /// plausible, and the e2e assertion "the insert leaves the caret immediately
    /// after the field" passed while the code did the opposite. That is the
    /// green-for-the-wrong-reason failure `SKILL.md` §4 is about, and it surfaced
    /// only when a cached-result field started flowing as ordinary text (OO-005) and
    /// the layout began agreeing with the model.
    ///
    /// This guard asserts the MODEL OFFSET, not pixels: the geometry follows from
    /// it, and a pixel assertion is what let the defect hide in the first place.
    #[test]
    fn inserting_a_field_leaves_the_caret_after_it() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let target = first_body_paragraph(&document);

        // A cached-result field: the engine has no clock, so the host supplies the
        // already-formatted string — and its length is what the caret must clear.
        let result = document
            .insert_field(&target, 0, "author", Some("Ada Lovelace".to_owned()))
            .expect("insert an author field");
        assert_eq!(
            result.node(),
            target,
            "the caret stays in the same paragraph"
        );
        assert_eq!(
            result.offset(),
            "Ada Lovelace".len() as u32,
            "the caret must rest past the field's cached text, so the next character \
             typed lands after the field and not before it"
        );

        // And the same for a caption's SEQ field, which is the shape OO-005 inserts:
        // the caret lands at the start of the new caption paragraph, so this asserts
        // the sibling guarantee rather than repeating the one above.
        let caption = document
            .insert_caption(
                &target,
                "Figure",
                ": Wiring diagram".to_owned(),
                "below",
                false,
                "arabic",
                0,
                "hyphen",
            )
            .expect("insert a caption");
        assert_eq!(
            caption.offset(),
            0,
            "a caption's caret rests at the start of the caption paragraph it made"
        );
    }

    /// A caption's number can go stale, the engine says which, and a reader can fix
    /// it — the "dirty, not silently stale" contract.
    ///
    /// Staleness is created here the way a user creates it: by deleting a caption
    /// paragraph's text, which is ordinary editing and deliberately does NOT
    /// renumber (renumbering on a keystroke would make typing O(document)).
    #[test]
    fn a_stale_caption_number_is_reported_and_can_be_put_right() {
        let mut document = open_document(SAMPLE_DOCX).expect("open sample docx");
        let body = body_paragraph_ids(&document);
        insert_figure(&mut document, &body[0], ": the first").expect("first");
        insert_figure(&mut document, &body[3], ": the second").expect("second");
        assert!(
            document.stale_caption_numbers().is_empty(),
            "nothing this engine inserted is stale on arrival"
        );

        // Delete the FIRST caption's paragraph outright. The second still shows 2,
        // but the document now resolves it to 1. Applied straight through the edit
        // crate rather than through a binding, because that is what the two real
        // sources of staleness look like to this module: a change it did not make.
        let first = document.caption_entries()[0]
            .split('\t')
            .next()
            .and_then(|id| id.parse::<NodeId>().ok())
            .expect("the caption's node id");
        let index = document
            .document
            .body()
            .iter()
            .position(|block| matches!(block, BlockNode::Paragraph(p) if p.id == first))
            .expect("the caption is a top-level body paragraph");
        casual_doc_edit::apply(
            &mut document.document,
            &mut document.edit_ids,
            &Operation::DeleteBlocks {
                container: None,
                index: index as u32,
                count: 1,
            },
        )
        .expect("delete the caption paragraph");

        let stale = document.stale_caption_numbers();
        assert_eq!(
            stale.len(),
            1,
            "the surviving caption must be reported as showing the wrong number: {stale:?}"
        );
        assert!(
            stale[0].ends_with("\t2\t1"),
            "the report must name what is shown and what is right: {}",
            stale[0]
        );

        document
            .update_caption_numbers()
            .expect("updating puts it right");
        assert!(
            document.stale_caption_numbers().is_empty(),
            "after an update nothing is stale"
        );
        assert_eq!(
            document
                .caption_entries()
                .iter()
                .map(|row| row.split('\t').nth(2).unwrap_or_default().to_owned())
                .collect::<Vec<_>>(),
            vec!["1".to_owned()],
            "and the caption now SHOWS the right number"
        );
        // A second update has nothing to do, so it builds no operations and the
        // binding refuses rather than pushing an empty undo step.
        assert!(
            document.caption_renumber_operations().0.is_empty(),
            "updating a correct document must produce no operations, so the command \
             can refuse with a reason instead of making an empty history entry"
        );
    }

    /// **Typing stays O(1) in document size.**
    ///
    /// A caption's number is resolved once, at insertion, and cached in the model
    /// as the field's result. So a keystroke must cost a bounded number of
    /// whole-document scans, that number must not change because the document holds
    /// captions, and it must not grow with the document.
    ///
    /// All three assertions are needed, and the third is why there is a budget
    /// rather than only a comparison: a renumbering pass bolted onto the keystroke
    /// path costs ONE extra whole-document scan, which is a real O(document)
    /// keystroke but is the same number at any document size and in a document with
    /// or without captions. A comparison alone passes it; the budget does not.
    /// `docs/107` §4 makes per-keystroke work an owner constraint, so this is a
    /// budget, not a measurement — raising it needs the same justification any
    /// other budget change needs.
    #[test]
    fn a_keystroke_costs_a_bounded_number_of_document_scans() {
        /// ONE. The editing path resolves the caret's surface once, and that is
        /// the whole per-keystroke document cost. This is a budget, not a
        /// measurement: `docs/107` §4 makes per-keystroke work O(1) in document
        /// size an owner constraint, so a second whole-document walk on the
        /// keystroke path is a defect to fix, not a number to raise. A renumbering
        /// pass bolted on here costs exactly one more, which is why the budget is
        /// tight enough to notice it.
        const BUDGET: usize = 1;

        let scans_for_a_keystroke = |lines: usize, with_caption: bool| {
            let mut document = open_document(&plain_text_of(lines)).expect("open");
            let target = body_paragraph_ids(&document)[0].clone();
            if with_caption {
                insert_figure(&mut document, &target, ": Wiring diagram").expect("insert");
            }
            casual_doc_edit::reset_document_scans();
            document
                .insert_text(&target, 0, "x".to_owned())
                .expect("type one character");
            casual_doc_edit::document_scans()
        };

        let plain = scans_for_a_keystroke(200, false);
        let captioned = scans_for_a_keystroke(200, true);
        let captioned_twice_as_long = scans_for_a_keystroke(400, true);
        assert!(
            captioned <= BUDGET,
            "a keystroke in a document with a caption scanned the whole document \
             {captioned} times, against a budget of {BUDGET}; per-keystroke work is \
             O(1) in document size (`docs/107` §4)"
        );
        assert_eq!(
            plain, captioned,
            "a document with a caption must not make a keystroke cost more than the \
             same document without one"
        );
        assert_eq!(
            captioned, captioned_twice_as_long,
            "a keystroke cost {captioned} scans at 200 paragraphs and \
             {captioned_twice_as_long} at 400 — the cost grows with the document"
        );
    }

    /// **Enumerating the captions is one walk, not one walk per caption.**
    ///
    /// The class guard in this crate's `tests` module
    /// (`document_wide_reads_do_not_scan_the_document_once_per_node`) covers the
    /// same rule for every document-wide read, but its fixture is plain text and
    /// therefore holds **no captions** — so a scan added inside the per-caption loop
    /// would never execute there, and the guard would pass while the defect shipped.
    /// That is the "green for the wrong reason" failure `SKILL.md` §4 warns about,
    /// found by mutating this exact code and watching the class guard stay green.
    /// This guard uses a caption-bearing document, and asserts both shapes: a budget
    /// (which catches per-caption cost) and a doubling (which catches per-paragraph
    /// cost).
    #[test]
    fn enumerating_captions_costs_one_walk_however_many_captions_there_are() {
        /// Enough captions that one scan each is unmistakable against the budget.
        const CAPTIONS: usize = 20;
        /// A handful of whole-document scans, independent of the caption count.
        const BUDGET: usize = 6;

        let scans = |lines: usize| {
            let mut document = open_document(&plain_text_of(lines)).expect("open");
            let ids = body_paragraph_ids(&document);
            for index in 0..CAPTIONS {
                insert_figure(&mut document, &ids[index * 2], ": a figure").expect("insert");
            }
            assert_eq!(
                document.caption_entries().len(),
                CAPTIONS,
                "the fixture must actually hold captions, or this guard proves nothing"
            );
            casual_doc_edit::reset_document_scans();
            let _ = document.caption_entries();
            let _ = document.caption_labels();
            let _ = document.reference_targets("Figure");
            casual_doc_edit::document_scans()
        };

        let one = scans(120);
        let two = scans(240);
        assert!(
            one <= BUDGET,
            "reading {CAPTIONS} captions scanned the whole document {one} times, \
             against a budget of {BUDGET}; a document-wide read resolves ids once, \
             not once per caption (`docs/116`)"
        );
        assert_eq!(
            one, two,
            "reading the captions scanned the document {one} times at 120 paragraphs \
             and {two} times at 240 — the cost grows with the document"
        );
    }

    // ---- A paragraph's inline containers are descended, on every reference path -
    // Four walks in this file stopped at a paragraph's top-level inlines or at
    // three of the four inline containers. The guards below assert the guarantee a
    // user would notice — a caption is listed, a bookmark is offered, a cross
    // reference resolves to text, Insert Caption is not refused — rather than the
    // shape of what a walk returned.

    /// Ids for a fixture, from one space so nothing collides with the sample.
    fn fixture_ids(space: u64) -> impl FnMut() -> NodeId {
        let mut next = 1u64;
        move || {
            next += 1;
            NodeId::from_parts(space, next).unwrap()
        }
    }

    /// A hyperlink target for a fixture whose point is the WRAPPER, not where it
    /// points.
    fn fixture_link_target() -> casual_doc_model::v1::HyperlinkTarget {
        casual_doc_model::v1::HyperlinkTarget::Internal(casual_doc_model::v1::InternalTarget {
            anchor: "Anywhere".to_owned(),
        })
    }

    fn fixture_run(id: NodeId, text: &str) -> InlineNode {
        InlineNode::Run(casual_doc_model::v1::Run {
            id,
            properties: casual_doc_model::v1::RunProperties::default().into(),
            text: text.to_owned(),
        })
    }

    fn fixture_paragraph(id: NodeId, inlines: Vec<InlineNode>) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id,
            properties: casual_doc_model::v1::ParagraphProperties::default().into(),
            inlines,
        })
    }

    /// A `SEQ Figure` field whose cached result reads `number`.
    fn fixture_seq(id: NodeId, result: NodeId, number: &str) -> InlineNode {
        InlineNode::Field(Box::new(casual_doc_model::v1::Field {
            id,
            instruction: "SEQ Figure \\* ARABIC".to_owned(),
            kind: FieldKind::Seq {
                name: "Figure".to_owned(),
            },
            inlines: vec![fixture_run(result, number)],
            form: None,
            update: casual_doc_model::v1::FieldUpdateState::default(),
        }))
    }

    fn fixture_text_box(id: NodeId, blocks: Vec<BlockNode>) -> InlineNode {
        InlineNode::TextBox(Box::new(casual_doc_model::v1::TextBox {
            hyperlink: None,
            id,
            anchor: None,
            relative_height: None,
            extent: Some(casual_doc_model::v1::Extent {
                width_emu: 2_743_200,
                height_emu: 1_828_800,
            }),
            fill: None,
            border: None,
            body_properties: casual_doc_model::v1::TextBoxBodyProperties::default(),
            blocks,
        }))
    }

    fn fixture_sdt(id: NodeId, inlines: Vec<InlineNode>) -> InlineNode {
        InlineNode::Sdt(Box::new(casual_doc_model::v1::InlineSdt {
            id,
            properties: casual_doc_model::v1::SdtProperties::default(),
            inlines,
        }))
    }

    /// A caption paragraph inside a text box that itself sits inside an inline
    /// content control — the box a figure caption is authored in, in a document
    /// whose fields a producer wrapped in a control.
    ///
    /// This is the case the whole caption feature could not see: the paragraph walk
    /// (`visit_text_box_paragraphs`) descended `TextBox`, `Hyperlink`, `Field` and
    /// `Group` and missed `Sdt` and `Revision`, so the caption paragraph inside the
    /// box was never visited and the caption was absent from `captionEntries`, from
    /// the cross-reference picker, and from renumbering.
    #[test]
    fn a_caption_in_a_box_inside_a_content_control_is_listed_and_numbered() {
        let mut id = fixture_ids(71);
        let plain_caption = id();
        let boxed_caption = id();
        let document = Document::new(
            NodeId::from_parts(71, 1).unwrap(),
            vec![
                // A plain body caption first, so the numbering has something to
                // count from and the guard can tell "not found" from "found but
                // mis-numbered".
                fixture_paragraph(
                    plain_caption,
                    vec![
                        fixture_run(id(), "Figure "),
                        fixture_seq(id(), id(), "1"),
                        fixture_run(id(), ": in the body"),
                    ],
                ),
                fixture_paragraph(
                    id(),
                    vec![fixture_sdt(
                        id(),
                        vec![fixture_text_box(
                            id(),
                            vec![fixture_paragraph(
                                boxed_caption,
                                vec![
                                    fixture_run(id(), "Figure "),
                                    fixture_seq(id(), id(), "1"),
                                    fixture_run(id(), ": in a box in a control"),
                                ],
                            )],
                        )],
                    )],
                ),
            ],
            casual_doc_model::v1::Definitions::default(),
        )
        .expect("a valid boxed-caption document");

        let d = crate::tests::wasm_document(document);
        let rows = captions(&d);
        assert!(
            rows.iter().any(|(_, _, text)| text.contains("in the body")),
            "the fixture's plain caption must be listed, or this guard cannot tell \
             a missing descent from a broken scan: {rows:?}"
        );
        assert!(
            rows.iter()
                .any(|(_, _, text)| text.contains("in a box in a control")),
            "a caption in a text box inside an inline content control is missing \
             from the caption list, so it is missing from the cross-reference \
             picker and from a Table of Figures: {rows:?}"
        );
        assert_eq!(
            rows.iter()
                .find(|(_, _, text)| text.contains("in a box in a control"))
                .map(|(_, number, _)| number.as_str()),
            Some("1"),
            "the boxed caption's CACHED number is what the document shows, and it \
             shows 1; renumbering is what changes it, and renumbering could not see \
             it either: {rows:?}"
        );
        // And renumbering can now reach it: two captions of one label cannot both
        // be 1, so an update must produce work.
        let mut d = d;
        let (ops, first) = d.caption_renumber_operations();
        assert!(
            first.is_some() && !ops.is_empty(),
            "two captions of the same label both cached as 1 must be stale; a \
             renumber that produces nothing means one of them is invisible"
        );
    }

    /// A cross-reference to a caption whose `SEQ` field sits inside a hyperlink
    /// resolves to the caption's text, not to nothing.
    ///
    /// `paragraph_offsets` looked at top-level inlines only, so for such a caption
    /// `after_sequence` was `None` and every reference kind fell back to the whole
    /// paragraph — Word's "Only label and number" and "Only caption text" silently
    /// became "Entire caption", which is three rows in a dialog that do the same
    /// thing.
    #[test]
    fn a_seq_inside_a_hyperlink_still_splits_label_from_caption_text() {
        let mut id = fixture_ids(72);
        let caption = id();
        let caret = id();
        let document = Document::new(
            NodeId::from_parts(72, 1).unwrap(),
            vec![
                fixture_paragraph(
                    caption,
                    vec![InlineNode::Hyperlink(Box::new(
                        casual_doc_model::v1::Hyperlink {
                            id: id(),
                            target: fixture_link_target(),
                            tooltip: None,
                            inlines: vec![
                                fixture_run(id(), "Figure "),
                                fixture_seq(id(), id(), "1"),
                            ],
                        },
                    ))],
                ),
                fixture_paragraph(caret, vec![fixture_run(id(), "see ")]),
            ],
            casual_doc_model::v1::Definitions::default(),
        )
        .expect("a valid hyperlinked-caption document");

        let mut d = crate::tests::wasm_document(document);
        // Append the caption's own text after the field, inside the link, so the
        // label/number boundary has text on both sides of it.
        {
            let BlockNode::Paragraph(paragraph) = &mut d.document.body_mut()[0] else {
                panic!("the caption paragraph");
            };
            let InlineNode::Hyperlink(link) = &mut paragraph.inlines[0] else {
                panic!("the hyperlink");
            };
            link.inlines
                .push(fixture_run(NodeId::from_parts(72, 90).unwrap(), ": Wiring"));
        }

        let scan = d.scan_references(ScanRequest {
            interest: &[caption],
            keep_inlines: false,
        });
        let found = scan.found(caption).expect("the caption is in the scan");
        assert_eq!(
            found.length,
            "Figure 1: Wiring".len() as u32,
            "the paragraph's length must count text nested in the hyperlink, or the \
             bookmark a reference creates stops short of what it names"
        );
        assert_eq!(
            found.after_sequence,
            Some("Figure 1".len() as u32),
            "the boundary between the caption's label-and-number and its own text \
             is the end of the `SEQ` field, and a `SEQ` inside a hyperlink is still \
             the caption's `SEQ`"
        );

        // And a caption this engine LISTS is one it can RENUMBER. `renumbered`
        // rewrote a top-level `SEQ` only, which agreed with `sequence_of` while
        // that was also top-level and became a half-fix the moment it was not: the
        // caption would be listed, counted, and then silently skipped by every
        // renumber, so Update Caption Numbers reported nothing stale while the
        // document read 1, 1, 2.
        let BlockNode::Paragraph(caption_paragraph) = &d.document.body()[0] else {
            panic!("the caption paragraph");
        };
        let mut allocate = || Some(NodeId::from_parts(72, 95).unwrap());
        let rewritten = renumbered(&caption_paragraph.inlines, "4", &mut allocate)
            .expect("a caption whose SEQ sits inside a hyperlink can be renumbered");
        assert_eq!(
            node_plain_text(&rewritten),
            "Figure 4: Wiring",
            "the renumber must reach the nested `SEQ`'s cached result"
        );
    }

    /// A `_Ref` bookmark nested inside a hyperlink is found and reused, so ten
    /// references to one figure carry one bookmark rather than ten.
    ///
    /// The extent scan was top-level only, so a bookmark Word itself had written
    /// inside the link of a hyperlinked cross-reference was invisible and every new
    /// reference minted another `_Ref…` over the same range.
    #[test]
    fn a_reference_bookmark_nested_in_a_hyperlink_is_reused() {
        use casual_doc_model::v1::{Bookmark, BookmarkEnd, BookmarkStart};

        let mut id = fixture_ids(73);
        let bookmark = casual_doc_model::v1::BookmarkId::new(NodeId::from_parts(73, 800).unwrap());
        let mut definitions = casual_doc_model::v1::Definitions::default();
        definitions.bookmarks.insert(
            bookmark,
            Bookmark {
                name: format!("{REFERENCE_BOOKMARK_PREFIX}123456789"),
            },
        );
        let caption = id();
        let document = Document::new(
            NodeId::from_parts(73, 1).unwrap(),
            vec![fixture_paragraph(
                caption,
                vec![InlineNode::Hyperlink(Box::new(
                    casual_doc_model::v1::Hyperlink {
                        id: id(),
                        target: fixture_link_target(),
                        tooltip: None,
                        inlines: vec![
                            InlineNode::BookmarkStart(BookmarkStart { id: id(), bookmark }),
                            fixture_run(id(), "Figure 1"),
                            InlineNode::BookmarkEnd(BookmarkEnd { id: id(), bookmark }),
                        ],
                    },
                ))],
            )],
            definitions,
        )
        .expect("a valid nested-bookmark document");

        let d = crate::tests::wasm_document(document);
        let scan = d.scan_references(ScanRequest {
            interest: &[caption],
            keep_inlines: false,
        });
        assert_eq!(
            scan.reference_bookmark(caption, 0, "Figure 1".len() as u32),
            Some(bookmark),
            "a `_Ref` bookmark inside a hyperlink covers the same characters as one \
             beside it, and must be reused for a reference over that range rather \
             than duplicated"
        );
    }

    /// A picture selected inside a text box resolves to the body block that
    /// anchors the box, so a caption has somewhere to go.
    ///
    /// `inlines_hold` searched the four inline-in-inline containers only, so a node
    /// inside a text box's own block content was not "held" by any paragraph and
    /// `locate_block` returned `None` — the insertion point for Word's most
    /// ordinary floated figure, a picture in a box, could not be computed at all.
    ///
    /// **Insert Caption on such a picture is still refused, one layer above this,
    /// and the guard says so rather than pretending otherwise.**
    /// `caption_target_refusal` asks `casual_doc_edit::surface_of`, whose
    /// `blocks_contain` matches paragraph, table, row and cell ids and does not
    /// descend a paragraph's inline text boxes, so it answers "not in the document"
    /// and the command refuses before `locate_block` is consulted. That function is
    /// in another lane's crate, so it is reported as a row instead of edited here —
    /// and the assertion below pins the refusal that is CURRENTLY correct behaviour
    /// for this engine, so the day `surface_of` learns text boxes this guard fails
    /// and is updated deliberately rather than silently passing on a changed
    /// meaning.
    #[test]
    fn a_picture_inside_a_text_box_resolves_to_the_block_that_anchors_it() {
        use casual_doc_model::v1::{Definitions, Drawing, MediaId, MediaReference};

        let mut id = fixture_ids(74);
        let media = MediaId::new(NodeId::from_parts(74, 800).unwrap());
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rId74".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let picture = id();
        let document = Document::new(
            NodeId::from_parts(74, 1).unwrap(),
            vec![fixture_paragraph(
                id(),
                vec![fixture_text_box(
                    id(),
                    vec![fixture_paragraph(
                        id(),
                        vec![InlineNode::Drawing(Box::new(Drawing {
                            hyperlink: None,
                            opacity: None,
                            id: picture,
                            media,
                            extent: Some(casual_doc_model::v1::Extent {
                                width_emu: 914_400,
                                height_emu: 914_400,
                            }),
                            crop: None,
                            descr: None,
                            border: None,
                            flip_h: false,
                            flip_v: false,
                            rotation: None,
                        }))],
                    )],
                )],
            )],
            definitions,
        )
        .expect("a valid boxed-picture document");

        let d = crate::tests::wasm_document(document);
        assert_eq!(
            locate_block(&d.document, picture),
            Some((None, 0)),
            "a picture selected inside a text box must resolve to the body block \
             that anchors the box — index 0 of the body, with no `InsertBlocks` \
             container, because the engine cannot address a text box as a container"
        );
        assert_eq!(
            caption_target_refusal(&d.document, picture),
            Some("the caption's target is not in the document"),
            "Insert Caption is still refused for a boxed picture, by \
             `casual_doc_edit::surface_of`, which does not descend a paragraph's \
             inline text boxes. Reported as a row; when it does, update this guard \
             deliberately"
        );
    }

    /// An authored bookmark inside a hyperlink is offered as a cross-reference
    /// target.
    ///
    /// `bookmark_targets` looked at a paragraph's top-level inlines, so a bookmark
    /// an author had placed over linked text simply had no row in the picker — a
    /// capability present in the model and unreachable from the product.
    #[test]
    fn a_bookmark_inside_a_hyperlink_is_offered_as_a_target() {
        use casual_doc_model::v1::{Bookmark, BookmarkEnd, BookmarkStart};

        let mut id = fixture_ids(75);
        let nested = casual_doc_model::v1::BookmarkId::new(NodeId::from_parts(75, 800).unwrap());
        let top_level = casual_doc_model::v1::BookmarkId::new(NodeId::from_parts(75, 801).unwrap());
        let mut definitions = casual_doc_model::v1::Definitions::default();
        for (bookmark, name) in [(nested, "InsideTheLink"), (top_level, "BesideTheLink")] {
            definitions.bookmarks.insert(
                bookmark,
                Bookmark {
                    name: name.to_owned(),
                },
            );
        }
        let document = Document::new(
            NodeId::from_parts(75, 1).unwrap(),
            vec![fixture_paragraph(
                id(),
                vec![
                    InlineNode::BookmarkStart(BookmarkStart {
                        id: id(),
                        bookmark: top_level,
                    }),
                    fixture_run(id(), "plain"),
                    InlineNode::BookmarkEnd(BookmarkEnd {
                        id: id(),
                        bookmark: top_level,
                    }),
                    InlineNode::Hyperlink(Box::new(casual_doc_model::v1::Hyperlink {
                        id: id(),
                        target: fixture_link_target(),
                        tooltip: None,
                        inlines: vec![
                            InlineNode::BookmarkStart(BookmarkStart {
                                id: id(),
                                bookmark: nested,
                            }),
                            fixture_run(id(), "linked"),
                            InlineNode::BookmarkEnd(BookmarkEnd {
                                id: id(),
                                bookmark: nested,
                            }),
                        ],
                    })),
                ],
            )],
            definitions,
        )
        .expect("a valid bookmark document");

        let d = crate::tests::wasm_document(document);
        let rows = d.reference_targets("bookmark");
        assert!(
            rows.iter().any(|row| row.contains("BesideTheLink")),
            "the fixture's top-level bookmark must be offered, or this guard cannot \
             tell a missing descent from a broken picker: {rows:?}"
        );
        assert!(
            rows.iter().any(|row| row.contains("InsideTheLink")),
            "a bookmark an author placed over linked text is not offered as a \
             cross-reference target, so it is in the document and unreachable from \
             the product: {rows:?}"
        );
    }
}
