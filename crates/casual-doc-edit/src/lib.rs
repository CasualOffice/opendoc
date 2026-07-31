//! The closed editing op set on `v1::Document` (doc 59).
//!
//! Editing mutates the **same model that is rendered** (`v1::Document`), not the
//! minimal v0 model the Phase-0 transaction layer edits. This crate is the choke
//! point (doc 45 I1): [`apply`] takes an [`Operation`] (the closed set, I2),
//! mutates the document in place, and returns the **inverse** operation so undo/
//! redo are just re-applying inverses.
//!
//! Positions are the **layout anchor space** shared with hit-testing (doc 58 §3):
//! `(NodeId paragraph, u32 byte_offset)`, a node-relative UTF-8 byte offset into
//! the paragraph's shaped plain text (`node_plain_text`). No grapheme/affinity
//! model, no byte↔grapheme bridge — hit-testing, selection, and editing all speak
//! byte offsets.
//!
//! The operation set is intentionally additive and bounded: text/paragraph/run,
//! table structure/properties, and exact-range hyperlink edits are supported.
//! Partial edits inside nested wrappers and broader object editing remain
//! explicit follow-ups (doc 59 staging).

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Color, Comment, CommentId, CoreProperties, DefinitionMap, Document, DrawingAnchor,
    Extent, FontName, FontRef, GridColumn, HighlightColor, Hyperlink, HyperlinkTarget, InlineNode,
    PageMargins, PageOrientation, PageSize, Paragraph, ParagraphProperties, ReviewProjection,
    RgbColor, Run, RunProperties, SectionColumns, SectionId, Table, TableCell, TableCellProperties,
    TableProperties, TableRow, VerticalAlignment,
};

/// A run-property change to apply over a range: each `Some(_)` field sets that
/// property, `None` leaves it untouched. Character formatting (`w:b`/`w:i`/`w:u`/
/// `w:strike`/`w:color`/`w:highlight`/`w:sz`/`w:vertAlign`/`w:rFonts`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FormatDelta {
    /// Set bold on/off.
    pub bold: Option<bool>,
    /// Set italic on/off.
    pub italic: Option<bool>,
    /// Set underline on/off.
    pub underline: Option<bool>,
    /// Set strike-through on/off.
    pub strike: Option<bool>,
    /// Set the text color (`w:color`) to an explicit RGB.
    pub color: Option<RgbColor>,
    /// Set the highlight (`w:highlight`) to a named color.
    pub highlight: Option<HighlightColor>,
    /// Set the font size in half-points (`w:sz`).
    pub size_half_points: Option<u32>,
    /// Set the baseline alignment (`w:vertAlign`): super/sub/baseline.
    pub vertical_alignment: Option<VerticalAlignment>,
    /// Set the font family (`w:rFonts`, ascii + hAnsi slots).
    pub font: Option<String>,
}

impl FormatDelta {
    fn apply_to(&self, props: &mut RunProperties) {
        if let Some(b) = self.bold {
            props.bold = Some(b);
        }
        if let Some(i) = self.italic {
            props.italic = Some(i);
        }
        if let Some(u) = self.underline {
            props.underline = Some(u);
        }
        if let Some(s) = self.strike {
            props.strike = Some(s);
        }
        if let Some(c) = self.color {
            props.color = Some(Color::Rgb(c));
        }
        if let Some(h) = self.highlight {
            props.highlight = Some(h);
        }
        if let Some(sz) = self.size_half_points {
            props.size_half_points = Some(sz);
        }
        if let Some(v) = self.vertical_alignment {
            props.vertical_alignment = Some(v);
        }
        if let Some(family) = &self.font {
            let font = FontRef::Named(FontName {
                name: family.clone(),
            });
            props.font_ref = Some(font.clone());
            props.font_ref_h_ansi = Some(font);
        }
    }
}

/// A caret position: a paragraph node and a node-relative UTF-8 byte offset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pos {
    /// The paragraph node.
    pub node: NodeId,
    /// UTF-8 byte offset into the paragraph's shaped plain text.
    pub offset: u32,
}

impl Pos {
    /// A position at `offset` within `node`.
    #[must_use]
    pub const fn new(node: NodeId, offset: u32) -> Self {
        Self { node, offset }
    }
}

/// A half-open range `[start, end)` within one paragraph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    /// Inclusive start.
    pub start: Pos,
    /// Exclusive end.
    pub end: Pos,
}

/// One paragraph-local replacement carried by an atomic review command.
///
/// Review authoring/decisions can rewrite wrapper and marker structure while
/// leaving every other paragraph untouched. Carrying complete inlines for only
/// the affected paragraph gives Undo an exact, bounded inverse.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReviewParagraphState {
    /// Paragraph whose inline tree is replaced.
    pub node: NodeId,
    /// Complete replacement inline tree.
    pub inlines: Vec<InlineNode>,
}

/// The closed editing op set (I2). Slice 1 carries the two text ops; structural
/// and object ops are additive variants (doc 59).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    /// Insert `text` at a caret position.
    InsertText {
        /// Where to insert.
        at: Pos,
        /// The text inserted.
        text: String,
    },
    /// Delete a non-empty range within one paragraph.
    DeleteText {
        /// The range removed.
        range: Range,
    },
    /// Split a paragraph at `at`, moving the trailing content into a new
    /// paragraph with id `new_id` inserted immediately after (Enter).
    SplitParagraph {
        /// The split boundary in the original paragraph.
        at: Pos,
        /// The id of the new trailing paragraph.
        new_id: NodeId,
    },
    /// Join `second` (which must immediately follow `first` in the same
    /// container) into the end of `first`, removing `second` (Backspace at a
    /// paragraph start). `first` keeps its own paragraph properties.
    JoinParagraphs {
        /// The paragraph that receives the content.
        first: NodeId,
        /// The paragraph appended and removed.
        second: NodeId,
    },
    /// Apply a run-property change over a range within one paragraph (bold,
    /// italic, …). Runs straddling the range are split so the change lands
    /// exactly on the selection.
    FormatText {
        /// The range to format (same node for `start`/`end`).
        range: Range,
        /// The property change.
        delta: FormatDelta,
    },
    /// Remove direct character formatting over a range, restoring the effective
    /// document/style defaults. The paragraph style itself is preserved.
    ClearFormatting {
        /// The range to clear (same node for `start`/`end`).
        range: Range,
    },
    /// Creates, updates, or removes a hyperlink over an exact same-paragraph text
    /// range. `Some(target)` creates a wrapper (or updates the wrapper already
    /// occupying exactly `range`); `None` removes that exact wrapper while
    /// preserving its inline children. The inverse restores the paragraph's
    /// original inline tree verbatim.
    SetHyperlink {
        /// The linked text range.
        range: Range,
        /// Fresh identity used when a new hyperlink wrapper is created.
        id: NodeId,
        /// New target, or `None` to remove the exact hyperlink wrapper.
        target: Option<HyperlinkTarget>,
        /// Optional screen tip. Must be non-empty and at most 255 bytes.
        tooltip: Option<String>,
    },
    /// Replace a paragraph's entire inline content. This is the inverse vehicle
    /// for structural edits (formatting run-splits) whose forward effect is not a
    /// simple reverse op — undo restores the paragraph's inlines verbatim.
    SetInlines {
        /// The paragraph whose content is replaced.
        node: NodeId,
        /// The inlines to install.
        inlines: Vec<InlineNode>,
    },
    /// Replace a paragraph's properties (alignment, spacing, indentation,
    /// shading, style, …). Its own inverse (carrying the previous properties), so
    /// undo is exact. Boxed to keep the enum small.
    SetParagraphProperties {
        /// The paragraph whose properties are replaced.
        node: NodeId,
        /// The properties to install.
        properties: Box<ParagraphProperties>,
    },
    /// Insert `row` into table `table` at the 0-based `index` (≤ the current row
    /// count). Inverse: [`Operation::DeleteRow`] of the same position.
    InsertRow {
        /// The table to insert into.
        table: NodeId,
        /// The 0-based row position.
        index: u32,
        /// The row to insert (its ids must be fresh).
        row: Box<TableRow>,
    },
    /// Remove the row at 0-based `index` from table `table`. Refuses to remove the
    /// last row (a table's rows are non-empty). Inverse: [`Operation::InsertRow`]
    /// carrying the removed row, so undo restores it verbatim.
    DeleteRow {
        /// The table to remove from.
        table: NodeId,
        /// The 0-based row position.
        index: u32,
    },
    /// Insert a column into a **regular** table (no `gridSpan`/`vMerge`; grid width
    /// matches every row's cell count) at 0-based grid `index`: a grid column of
    /// `width` and one `cells` entry per row (in row order, fresh ids). Inverse:
    /// [`Operation::DeleteColumn`].
    InsertColumn {
        /// The table to insert into.
        table: NodeId,
        /// The 0-based column position.
        index: u32,
        /// The new grid column's width (twips), if any.
        width: Option<i32>,
        /// One new cell per row, in row order.
        cells: Vec<TableCell>,
    },
    /// Remove the column at 0-based grid `index` from a **regular** table. Refuses a
    /// table's only column. Inverse: [`Operation::InsertColumn`] carrying the removed
    /// grid width + cells, so undo restores the column verbatim.
    DeleteColumn {
        /// The table to remove from.
        table: NodeId,
        /// The 0-based column position.
        index: u32,
    },
    /// Remove the whole `table` from its container (the body, or a cell / content
    /// control it nests in). Refuses to empty a table cell (a cell's blocks are
    /// non-empty). Inverse: [`Operation::InsertTable`] carrying the removed table +
    /// its position, so undo restores it verbatim.
    DeleteTable {
        /// The table to remove.
        table: NodeId,
    },
    /// Insert `table` at 0-based `index` in `container` (`None` = the document body,
    /// `Some(id)` = the cell or content control whose blocks hold it). Inverse:
    /// [`Operation::DeleteTable`].
    InsertTable {
        /// The container: `None` for the body, else the owning cell / SDT node.
        container: Option<NodeId>,
        /// The 0-based block position within the container.
        index: u32,
        /// The table to insert (its ids must be those the inverse recorded).
        table: Box<Table>,
    },
    /// Insert a sequence of `blocks` into `container` (`None` = the document body,
    /// `Some(id)` = the owning cell / SDT node) starting at 0-based `index` (≤ the
    /// current block count). The general block-sequence primitive behind structured
    /// paste — a fragment of copied paragraphs and tables reconstructed with fresh
    /// ids. Inverse: [`Operation::DeleteBlocks`] of the same span.
    InsertBlocks {
        /// The container: `None` for the body, else the owning cell / SDT node.
        container: Option<NodeId>,
        /// The 0-based block position within the container.
        index: u32,
        /// The blocks to insert, in order (their ids must be those the inverse
        /// recorded — the caller assigns fresh ids before inserting).
        blocks: Vec<BlockNode>,
    },
    /// Remove `count` blocks from `container` (`None` = the document body) starting
    /// at 0-based `index`. The inverse vehicle for [`Operation::InsertBlocks`];
    /// undo restores the removed blocks verbatim.
    DeleteBlocks {
        /// The container: `None` for the body, else the owning cell / SDT node.
        container: Option<NodeId>,
        /// The 0-based block position of the first removed block.
        index: u32,
        /// How many consecutive blocks to remove.
        count: u32,
    },
    /// Resize an inline drawing or text box: replace its authored extent
    /// (`wp:extent`, EMU) — the geometry op behind a handle drag-resize (docs/85
    /// §5.3). Self-inverse carrying the previous extent (the retained-value
    /// pattern, like [`Operation::SetParagraphProperties`]). `None` restores the
    /// "size resolved from content" state a missing extent means.
    SetExtent {
        /// The drawing / text-box node to resize (inline or floating).
        object: NodeId,
        /// The new authored extent (`None` = defer to content-derived sizing).
        extent: Option<Extent>,
    },
    /// Move / re-wrap / re-order a **floating** object: replace the whole
    /// [`DrawingAnchor`] of an `AnchoredDrawing` or floating `TextBox` (docs/85
    /// §5.3). One op covers position, wrap mode, wrap distances, and z-order.
    /// Self-inverse carrying the previous anchor (retained-value pattern).
    SetAnchor {
        /// The floating object node to reposition/re-wrap.
        object: NodeId,
        /// The new anchor (position + wrap + z-order).
        anchor: Box<DrawingAnchor>,
    },
    /// Replace a table cell's properties (shading, borders, vertical alignment,
    /// margins, span/merge, …). Its own inverse (carrying the previous properties).
    /// Boxed to keep the enum small.
    SetTableCellProperties {
        /// The cell whose properties are replaced.
        cell: NodeId,
        /// The properties to install.
        properties: Box<TableCellProperties>,
    },
    /// Replace a table's properties (borders, shading, alignment, indent, …). Its own
    /// inverse. Boxed to keep the enum small.
    SetTableProperties {
        /// The table whose properties are replaced.
        table: NodeId,
        /// The properties to install.
        properties: Box<TableProperties>,
    },
    /// Replace a table's full structure with `replacement`. This is reserved for
    /// structural transforms such as merge/split cells where exact undo needs the
    /// previous row/cell topology.
    ReplaceTable {
        /// The table to replace.
        table: NodeId,
        /// The replacement table. Its id must match `table`.
        replacement: Box<Table>,
    },
    /// Replace the document's core properties (`docProps/core.xml` — title,
    /// author, subject, …). Document-global, not node-scoped. Its own inverse
    /// (carrying the previous properties); rejected (doc left unchanged) if a
    /// field would exceed the model's bounded length.
    SetCoreProperties {
        /// The properties to install.
        properties: Box<CoreProperties>,
    },
    /// Atomically replace only review-touched paragraph inlines and, when
    /// necessary, the comments map. Its inverse carries the exact prior scoped
    /// values rather than a whole-document body snapshot.
    UpdateReviewState {
        /// Paragraph-local inline replacements. Node ids must be unique.
        paragraphs: Vec<ReviewParagraphState>,
        /// Replacement comments map, or `None` when revision-only editing leaves
        /// comment definitions untouched.
        comments: Option<DefinitionMap<CommentId, Comment>>,
    },
    /// Replace one section's page size, margins, orientation, and column layout
    /// (the "Page Setup" fields) — headers/footers, borders, and the section's
    /// other properties are untouched. Its own inverse; rejected
    /// (doc left unchanged) if a value falls outside the model's domain
    /// (e.g. a page dimension over ~22in).
    SetSectionGeometry {
        /// The section to update.
        section: SectionId,
        /// The page size to install.
        page_size: PageSize,
        /// The page margins to install.
        page_margins: PageMargins,
        /// The orientation to install (`None` clears it — the model then
        /// infers portrait/landscape from the page size on export).
        orientation: Option<PageOrientation>,
        /// The column layout to install.
        columns: SectionColumns,
    },
}

/// Why an edit could not be applied. No partial mutation ever occurs: an op
/// validates before it mutates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditError {
    /// The target paragraph node does not exist.
    NodeNotFound,
    /// The offset is past the paragraph's text length.
    OffsetOutOfRange,
    /// The offset does not fall on a UTF-8 character boundary.
    NotCharBoundary,
    /// An empty insert, or an empty/inverted delete range.
    EmptyEdit,
    /// The range spans more than one paragraph (slice 1 is single-paragraph).
    CrossParagraph,
    /// The position is not inside an editable top-level run (e.g. inside a
    /// hyperlink/field wrapper or a tab). Slice-1 limitation.
    Unsupported,
    /// The node-id space is exhausted.
    IdExhausted,
    /// A field exceeds the model's bounded length (e.g. a metadata property).
    ValueTooLarge,
}

/// Applies `op` to `doc`, returning the inverse operation (for undo). `ids` mints
/// new run identities when an edit must create a run (e.g. typing into an empty
/// paragraph). On `Err`, `doc` is unchanged.
pub fn apply(
    doc: &mut Document,
    ids: &mut dyn RunIds,
    op: &Operation,
) -> Result<Operation, EditError> {
    match op {
        Operation::InsertText { at, text } => {
            if text.is_empty() {
                return Err(EditError::EmptyEdit);
            }
            let para =
                find_paragraph_mut(doc.body_mut(), at.node).ok_or(EditError::NodeNotFound)?;
            if at.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            insert_text(&mut para.inlines, at.offset, text, ids)?;
            let end = Pos::new(at.node, at.offset + text.len() as u32);
            Ok(Operation::DeleteText {
                range: Range { start: *at, end },
            })
        }
        Operation::DeleteText { range } => {
            if range.start.node != range.end.node {
                return Err(EditError::CrossParagraph);
            }
            if range.end.offset <= range.start.offset {
                return Err(EditError::EmptyEdit);
            }
            let para = find_paragraph_mut(doc.body_mut(), range.start.node)
                .ok_or(EditError::NodeNotFound)?;
            if range.end.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            // Fast path: the whole range lies inside one run — remove the substring
            // and invert with `InsertText` (no clone; the common single-char
            // backspace stays cheap).
            if let Some(removed) =
                delete_text(&mut para.inlines, range.start.offset, range.end.offset)?
            {
                return Ok(Operation::InsertText {
                    at: range.start,
                    text: removed,
                });
            }
            // General path: the range spans several runs (a formatted paragraph, the
            // tail/head of a cross-paragraph selection). Snapshot for an exact
            // inverse, split runs at both ends, then drop every inline the range
            // fully covers. The inverse restores the inlines verbatim, so undo brings
            // back each deleted run's own formatting — an `InsertText` (plain text)
            // inverse could not.
            let old = para.inlines.clone();
            ensure_run_boundary(&mut para.inlines, range.end.offset, ids)?;
            ensure_run_boundary(&mut para.inlines, range.start.offset, ids)?;
            remove_covered_range(&mut para.inlines, range.start.offset, range.end.offset)?;
            // The removal can leave two equal-property runs adjacent (or a boundary
            // split earlier did); the model forbids that, so merge them back.
            coalesce_adjacent_runs(&mut para.inlines);
            Ok(Operation::SetInlines {
                node: range.start.node,
                inlines: old,
            })
        }
        Operation::SplitParagraph { at, new_id } => {
            if !split_paragraph(doc.body_mut(), at.node, at.offset, *new_id, ids)? {
                return Err(EditError::NodeNotFound);
            }
            Ok(Operation::JoinParagraphs {
                first: at.node,
                second: *new_id,
            })
        }
        Operation::JoinParagraphs { first, second } => {
            match join_paragraphs(doc.body_mut(), *first, *second)? {
                Some(split_at) => Ok(Operation::SplitParagraph {
                    at: Pos::new(*first, split_at),
                    new_id: *second,
                }),
                None => Err(EditError::NodeNotFound),
            }
        }
        Operation::FormatText { range, delta } => {
            if range.start.node != range.end.node {
                return Err(EditError::CrossParagraph);
            }
            if range.end.offset <= range.start.offset {
                return Err(EditError::EmptyEdit);
            }
            let node = range.start.node;
            let para = find_paragraph_mut(doc.body_mut(), node).ok_or(EditError::NodeNotFound)?;
            if range.end.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            // Snapshot for an exact undo, then align run boundaries to the range
            // (end first, so the start offset stays valid) and format the covered
            // runs.
            let old = para.inlines.clone();
            ensure_run_boundary(&mut para.inlines, range.end.offset, ids)?;
            ensure_run_boundary(&mut para.inlines, range.start.offset, ids)?;
            let covered: Vec<usize> = run_segments(&para.inlines)
                .into_iter()
                .filter(|s| s.start >= range.start.offset && s.end <= range.end.offset)
                .map(|s| s.idx)
                .collect();
            for idx in covered {
                if let InlineNode::Run(run) = &mut para.inlines[idx] {
                    delta.apply_to(&mut run.properties);
                }
            }
            // Formatting a sub-range to match a neighbour (or the boundary split
            // above) can leave adjacent equal-property runs, which the model forbids;
            // merge them so the document stays re-validatable and export-clean.
            coalesce_adjacent_runs(&mut para.inlines);
            Ok(Operation::SetInlines { node, inlines: old })
        }
        Operation::ClearFormatting { range } => {
            if range.start.node != range.end.node {
                return Err(EditError::CrossParagraph);
            }
            if range.end.offset <= range.start.offset {
                return Err(EditError::EmptyEdit);
            }
            let node = range.start.node;
            let para = find_paragraph_mut(doc.body_mut(), node).ok_or(EditError::NodeNotFound)?;
            if range.end.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            let old = para.inlines.clone();
            ensure_run_boundary(&mut para.inlines, range.end.offset, ids)?;
            ensure_run_boundary(&mut para.inlines, range.start.offset, ids)?;
            let covered =
                covered_top_level_indices(&para.inlines, range.start.offset, range.end.offset)?;
            if covered.is_empty()
                || covered
                    .iter()
                    .any(|index| !matches!(para.inlines[*index], InlineNode::Run(_)))
            {
                return Err(EditError::Unsupported);
            }
            for index in covered {
                if let InlineNode::Run(run) = &mut para.inlines[index] {
                    run.properties = RunProperties::default();
                }
            }
            coalesce_adjacent_runs(&mut para.inlines);
            Ok(Operation::SetInlines { node, inlines: old })
        }
        Operation::SetHyperlink {
            range,
            id,
            target,
            tooltip,
        } => {
            if range.start.node != range.end.node {
                return Err(EditError::CrossParagraph);
            }
            if range.end.offset <= range.start.offset {
                return Err(EditError::EmptyEdit);
            }
            if !valid_hyperlink_values(target.as_ref(), tooltip.as_deref()) {
                return Err(EditError::Unsupported);
            }
            let node = range.start.node;
            let para = find_paragraph_mut(doc.body_mut(), node).ok_or(EditError::NodeNotFound)?;
            if range.end.offset > paragraph_text_len(para) {
                return Err(EditError::OffsetOutOfRange);
            }
            let old = para.inlines.clone();

            // Updating/removing an existing link is exact-range only. That keeps
            // edits deterministic and avoids silently splitting an imported
            // hyperlink wrapper.
            if let Some(index) =
                exact_hyperlink_index(&para.inlines, range.start.offset, range.end.offset)
            {
                if let Some(target) = target {
                    let InlineNode::Hyperlink(link) = &mut para.inlines[index] else {
                        unreachable!("exact_hyperlink_index only returns hyperlinks");
                    };
                    link.target = target.clone();
                    link.tooltip = tooltip.clone();
                } else {
                    let InlineNode::Hyperlink(link) = para.inlines.remove(index) else {
                        unreachable!("exact_hyperlink_index only returns hyperlinks");
                    };
                    para.inlines.splice(index..index, link.inlines);
                    coalesce_adjacent_runs(&mut para.inlines);
                }
                return Ok(Operation::SetInlines { node, inlines: old });
            }

            let Some(target) = target else {
                return Err(EditError::Unsupported);
            };
            // Creating a link currently accepts top-level text runs. Align both
            // boundaries first so the wrapper covers exactly the requested bytes.
            // Selections that cut through any existing wrapper remain unsupported.
            let mut next_inlines = para.inlines.clone();
            ensure_run_boundary(&mut next_inlines, range.end.offset, ids)?;
            ensure_run_boundary(&mut next_inlines, range.start.offset, ids)?;
            let covered =
                covered_top_level_indices(&next_inlines, range.start.offset, range.end.offset)?;
            if covered.is_empty()
                || covered
                    .iter()
                    .any(|index| !matches!(next_inlines[*index], InlineNode::Run(_)))
            {
                return Err(EditError::Unsupported);
            }
            let first = covered[0];
            let last = *covered.last().expect("covered is non-empty");
            let mut children: Vec<InlineNode> = next_inlines.drain(first..=last).collect();
            // Give newly-authored links a recognizable default without clobbering
            // explicit author formatting. Imported/updated links retain their
            // existing run styling verbatim.
            for child in &mut children {
                if let InlineNode::Run(run) = child {
                    run.properties.underline.get_or_insert(true);
                    run.properties.color.get_or_insert(Color::Rgb(RgbColor {
                        r: 0x05,
                        g: 0x63,
                        b: 0xc1,
                    }));
                }
            }
            next_inlines.insert(
                first,
                InlineNode::Hyperlink(Hyperlink {
                    id: *id,
                    target: target.clone(),
                    tooltip: tooltip.clone(),
                    inlines: children,
                }),
            );
            para.inlines = next_inlines;
            Ok(Operation::SetInlines { node, inlines: old })
        }
        Operation::SetInlines { node, inlines } => {
            let para = find_paragraph_mut(doc.body_mut(), *node).ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(&mut para.inlines, inlines.clone());
            Ok(Operation::SetInlines {
                node: *node,
                inlines: previous,
            })
        }
        Operation::SetParagraphProperties { node, properties } => {
            let para = find_paragraph_mut(doc.body_mut(), *node).ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(&mut para.properties, (**properties).clone());
            Ok(Operation::SetParagraphProperties {
                node: *node,
                properties: Box::new(previous),
            })
        }
        Operation::InsertRow { table, index, row } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            let idx = *index as usize;
            if idx > t.rows.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            t.rows.insert(idx, (**row).clone());
            Ok(Operation::DeleteRow {
                table: *table,
                index: *index,
            })
        }
        Operation::DeleteRow { table, index } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            let idx = *index as usize;
            if idx >= t.rows.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            // A table's rows are non-empty; removing the last row would make it
            // invalid. Deleting a whole table is a separate op.
            if t.rows.len() == 1 {
                return Err(EditError::Unsupported);
            }
            let removed = t.rows.remove(idx);
            Ok(Operation::InsertRow {
                table: *table,
                index: *index,
                row: Box::new(removed),
            })
        }
        Operation::InsertColumn {
            table,
            index,
            width,
            cells,
        } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            ensure_regular_table(t)?;
            let idx = *index as usize;
            if idx > t.grid.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            if cells.len() != t.rows.len() {
                return Err(EditError::Unsupported);
            }
            t.grid.insert(
                idx,
                GridColumn {
                    width_twips: *width,
                },
            );
            for (row, cell) in t.rows.iter_mut().zip(cells.iter()) {
                row.cells.insert(idx, cell.clone());
            }
            Ok(Operation::DeleteColumn {
                table: *table,
                index: *index,
            })
        }
        Operation::DeleteColumn { table, index } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            ensure_regular_table(t)?;
            let idx = *index as usize;
            if idx >= t.grid.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            // A row's cells are non-empty; removing the only column is invalid.
            if t.grid.len() == 1 {
                return Err(EditError::Unsupported);
            }
            let width = t.grid.remove(idx).width_twips;
            let cells: Vec<TableCell> =
                t.rows.iter_mut().map(|row| row.cells.remove(idx)).collect();
            Ok(Operation::InsertColumn {
                table: *table,
                index: *index,
                width,
                cells,
            })
        }
        Operation::DeleteTable { table } => {
            let (container, index, removed) = remove_table(doc.body_mut(), None, *table)?;
            Ok(Operation::InsertTable {
                container,
                index,
                table: Box::new(removed),
            })
        }
        Operation::InsertTable {
            container,
            index,
            table,
        } => {
            let blocks = match container {
                None => doc.body_mut(),
                Some(id) => {
                    find_container_blocks_mut(doc.body_mut(), *id).ok_or(EditError::NodeNotFound)?
                }
            };
            let idx = *index as usize;
            if idx > blocks.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            blocks.insert(idx, BlockNode::Table((**table).clone()));
            Ok(Operation::DeleteTable { table: table.id })
        }
        Operation::InsertBlocks {
            container,
            index,
            blocks: to_insert,
        } => {
            if to_insert.is_empty() {
                return Err(EditError::EmptyEdit);
            }
            let blocks = match container {
                None => doc.body_mut(),
                Some(id) => {
                    find_container_blocks_mut(doc.body_mut(), *id).ok_or(EditError::NodeNotFound)?
                }
            };
            let idx = *index as usize;
            if idx > blocks.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            for (offset, block) in to_insert.iter().enumerate() {
                blocks.insert(idx + offset, block.clone());
            }
            Ok(Operation::DeleteBlocks {
                container: *container,
                index: *index,
                count: to_insert.len() as u32,
            })
        }
        Operation::DeleteBlocks {
            container,
            index,
            count,
        } => {
            let blocks = match container {
                None => doc.body_mut(),
                Some(id) => {
                    find_container_blocks_mut(doc.body_mut(), *id).ok_or(EditError::NodeNotFound)?
                }
            };
            let idx = *index as usize;
            let count = *count as usize;
            if count == 0 {
                return Err(EditError::EmptyEdit);
            }
            if idx + count > blocks.len() {
                return Err(EditError::OffsetOutOfRange);
            }
            // A cell / SDT container's block list must stay non-empty.
            if container.is_some() && count >= blocks.len() {
                return Err(EditError::Unsupported);
            }
            let removed: Vec<BlockNode> = blocks
                .splice(idx..idx + count, std::iter::empty())
                .collect();
            Ok(Operation::InsertBlocks {
                container: *container,
                index: *index,
                blocks: removed,
            })
        }
        Operation::SetExtent { object, extent } => {
            let previous = set_object_extent(doc.body_mut(), *object, *extent)
                .ok_or(EditError::NodeNotFound)?;
            Ok(Operation::SetExtent {
                object: *object,
                extent: previous,
            })
        }
        Operation::SetAnchor { object, anchor } => {
            let previous = set_object_anchor(doc.body_mut(), *object, **anchor)
                .ok_or(EditError::NodeNotFound)?;
            Ok(Operation::SetAnchor {
                object: *object,
                anchor: Box::new(previous),
            })
        }
        Operation::SetTableCellProperties { cell, properties } => {
            let c = find_cell_mut(doc.body_mut(), *cell).ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(&mut c.properties, (**properties).clone());
            Ok(Operation::SetTableCellProperties {
                cell: *cell,
                properties: Box::new(previous),
            })
        }
        Operation::SetTableProperties { table, properties } => {
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(&mut t.properties, (**properties).clone());
            Ok(Operation::SetTableProperties {
                table: *table,
                properties: Box::new(previous),
            })
        }
        Operation::ReplaceTable { table, replacement } => {
            if replacement.id != *table {
                return Err(EditError::Unsupported);
            }
            let t = find_table_mut(doc.body_mut(), *table).ok_or(EditError::NodeNotFound)?;
            let previous = std::mem::replace(t, (**replacement).clone());
            Ok(Operation::ReplaceTable {
                table: *table,
                replacement: Box::new(previous),
            })
        }
        Operation::SetCoreProperties { properties } => {
            let slot = doc.properties_mut();
            let previous = std::mem::replace(&mut slot.core, (**properties).clone());
            if let Err(_err) = doc.validate() {
                // Roll back: no partial mutation ever survives an error.
                doc.properties_mut().core = previous;
                return Err(EditError::ValueTooLarge);
            }
            Ok(Operation::SetCoreProperties {
                properties: Box::new(previous),
            })
        }
        Operation::UpdateReviewState {
            paragraphs,
            comments,
        } => {
            if paragraphs.is_empty() && comments.is_none() {
                return Err(EditError::EmptyEdit);
            }
            for (index, paragraph) in paragraphs.iter().enumerate() {
                if paragraphs[..index]
                    .iter()
                    .any(|previous| previous.node == paragraph.node)
                    || find_paragraph(doc.body(), paragraph.node).is_none()
                {
                    return Err(EditError::NodeNotFound);
                }
            }

            let mut previous_paragraphs = Vec::with_capacity(paragraphs.len());
            for replacement in paragraphs {
                let paragraph = find_paragraph_mut(doc.body_mut(), replacement.node)
                    .ok_or(EditError::NodeNotFound)?;
                previous_paragraphs.push(ReviewParagraphState {
                    node: replacement.node,
                    inlines: std::mem::replace(&mut paragraph.inlines, replacement.inlines.clone()),
                });
            }
            let previous_comments = comments.as_ref().map(|replacement| {
                std::mem::replace(&mut doc.definitions_mut().comments, replacement.clone())
            });
            if doc.validate().is_err() {
                for previous in &previous_paragraphs {
                    let paragraph = find_paragraph_mut(doc.body_mut(), previous.node)
                        .expect("review paragraph was prevalidated");
                    paragraph.inlines = previous.inlines.clone();
                }
                if let Some(previous) = previous_comments {
                    doc.definitions_mut().comments = previous;
                }
                return Err(EditError::ValueTooLarge);
            }
            Ok(Operation::UpdateReviewState {
                paragraphs: previous_paragraphs,
                comments: previous_comments,
            })
        }
        Operation::SetSectionGeometry {
            section,
            page_size,
            page_margins,
            orientation,
            columns,
        } => {
            let s = doc
                .definitions_mut()
                .sections
                .iter_mut()
                .find(|s| s.id == *section)
                .ok_or(EditError::NodeNotFound)?;
            let previous = (
                s.page_size,
                s.page_margins,
                s.orientation,
                s.columns.clone(),
            );
            s.page_size = *page_size;
            s.page_margins = *page_margins;
            s.orientation = *orientation;
            s.columns = columns.clone();
            if doc.validate().is_err() {
                let s = doc
                    .definitions_mut()
                    .sections
                    .iter_mut()
                    .find(|s| s.id == *section)
                    .expect("the section we just found still exists");
                (s.page_size, s.page_margins, s.orientation, s.columns) = previous;
                return Err(EditError::ValueTooLarge);
            }
            Ok(Operation::SetSectionGeometry {
                section: *section,
                page_size: previous.0,
                page_margins: previous.1,
                orientation: previous.2,
                columns: previous.3,
            })
        }
    }
}

/// Removes the table `table_id` from `blocks` or any nested cell/SDT, returning its
/// container (`None` = this level / the body, else the owning cell/SDT id), its
/// 0-based index there, and the removed table (for the inverse). Refuses to empty a
/// nested container (a cell's/SDT's blocks are non-empty). `container` is the id of
/// the container `blocks` belongs to as the recursion descends.
fn remove_table(
    blocks: &mut Vec<BlockNode>,
    container: Option<NodeId>,
    table_id: NodeId,
) -> Result<(Option<NodeId>, u32, Table), EditError> {
    if let Some(i) = blocks
        .iter()
        .position(|b| matches!(b, BlockNode::Table(t) if t.id == table_id))
    {
        if container.is_some() && blocks.len() == 1 {
            return Err(EditError::Unsupported);
        }
        let BlockNode::Table(t) = blocks.remove(i) else {
            unreachable!("position matched a table");
        };
        return Ok((container, i as u32, t));
    }
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        match remove_table(&mut cell.blocks, Some(cell.id), table_id) {
                            Ok(found) => return Ok(found),
                            Err(EditError::NodeNotFound) => {}
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                let sid = sdt.id;
                match remove_table(&mut sdt.blocks, Some(sid), table_id) {
                    Ok(found) => return Ok(found),
                    Err(EditError::NodeNotFound) => {}
                    Err(e) => return Err(e),
                }
            }
            _ => {}
        }
    }
    Err(EditError::NodeNotFound)
}

/// Sets the authored extent of the drawing / text box `object` (inline
/// `Drawing`/`TextBox` or a floating `AnchoredDrawing`), searched recursively
/// across body paragraphs (through hyperlink/revision wrappers), table cells,
/// block SDTs, and inline text-box bodies. Returns the **previous** extent (as an
/// `Option`, so a floating drawing's always-present extent and an inline
/// object's optional extent share one inverse shape); `None` if `object` is not
/// a resizable object. [`Operation::SetExtent`]'s target.
fn set_object_extent(
    blocks: &mut [BlockNode],
    object: NodeId,
    extent: Option<Extent>,
) -> Option<Option<Extent>> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(prev) =
                    set_object_extent_in_inlines(&mut paragraph.inlines, object, extent)
                {
                    return Some(prev);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(prev) = set_object_extent(&mut cell.blocks, object, extent) {
                            return Some(prev);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(prev) = set_object_extent(&mut sdt.blocks, object, extent) {
                    return Some(prev);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

fn set_object_extent_in_inlines(
    inlines: &mut [InlineNode],
    object: NodeId,
    extent: Option<Extent>,
) -> Option<Option<Extent>> {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::Drawing(drawing) if drawing.id == object => {
                return Some(core::mem::replace(&mut drawing.extent, extent));
            }
            // A floating anchored picture always carries an extent; a `None`
            // request leaves it unchanged (resize always supplies a size).
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                let previous = Some(drawing.extent);
                if let Some(new) = extent {
                    drawing.extent = new;
                }
                return Some(previous);
            }
            InlineNode::TextBox(text_box) => {
                if text_box.id == object {
                    return Some(core::mem::replace(&mut text_box.extent, extent));
                }
                if let Some(prev) = set_object_extent(&mut text_box.blocks, object, extent) {
                    return Some(prev);
                }
            }
            InlineNode::Hyperlink(hyperlink) => {
                if let Some(prev) =
                    set_object_extent_in_inlines(&mut hyperlink.inlines, object, extent)
                {
                    return Some(prev);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(prev) =
                    set_object_extent_in_inlines(&mut revision.inlines, object, extent)
                {
                    return Some(prev);
                }
            }
            _ => {}
        }
    }
    None
}

/// Replaces the [`DrawingAnchor`] of the floating object `object` (an
/// `AnchoredDrawing`, a floating `TextBox`, or an anchored `Group`), searched the
/// same way as [`set_object_extent`]. Returns the **previous** anchor; `None` if
/// `object` is not a currently floating object (an inline object has no anchor to
/// set, so a move/wrap is rejected rather than silently converting it to a float).
/// [`Operation::SetAnchor`]'s target.
fn set_object_anchor(
    blocks: &mut [BlockNode],
    object: NodeId,
    anchor: DrawingAnchor,
) -> Option<DrawingAnchor> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Paragraph(paragraph) => {
                if let Some(prev) =
                    set_object_anchor_in_inlines(&mut paragraph.inlines, object, anchor)
                {
                    return Some(prev);
                }
            }
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(prev) = set_object_anchor(&mut cell.blocks, object, anchor) {
                            return Some(prev);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(prev) = set_object_anchor(&mut sdt.blocks, object, anchor) {
                    return Some(prev);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

fn set_object_anchor_in_inlines(
    inlines: &mut [InlineNode],
    object: NodeId,
    anchor: DrawingAnchor,
) -> Option<DrawingAnchor> {
    for inline in inlines.iter_mut() {
        match inline {
            InlineNode::AnchoredDrawing(drawing) if drawing.id == object => {
                return Some(core::mem::replace(&mut drawing.anchor, anchor));
            }
            InlineNode::TextBox(text_box) => {
                if text_box.id == object {
                    // Only an already-floating text box can be re-anchored.
                    return text_box.anchor.replace(anchor);
                }
                if let Some(prev) = set_object_anchor(&mut text_box.blocks, object, anchor) {
                    return Some(prev);
                }
            }
            InlineNode::Group(group) if group.id == object => {
                return group.anchor.replace(anchor);
            }
            InlineNode::Hyperlink(hyperlink) => {
                if let Some(prev) =
                    set_object_anchor_in_inlines(&mut hyperlink.inlines, object, anchor)
                {
                    return Some(prev);
                }
            }
            InlineNode::Revision(revision) => {
                if let Some(prev) =
                    set_object_anchor_in_inlines(&mut revision.inlines, object, anchor)
                {
                    return Some(prev);
                }
            }
            _ => {}
        }
    }
    None
}

/// The mutable block list of the cell or SDT with id `id`, searched recursively —
/// the re-insertion target for [`Operation::InsertTable`]. `None` if not found.
fn find_container_blocks_mut(blocks: &mut [BlockNode], id: NodeId) -> Option<&mut Vec<BlockNode>> {
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        if cell.id == id {
                            return Some(&mut cell.blocks);
                        }
                        if let Some(found) = find_container_blocks_mut(&mut cell.blocks, id) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if sdt.id == id {
                    return Some(&mut sdt.blocks);
                }
                if let Some(found) = find_container_blocks_mut(&mut sdt.blocks, id) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// A table is *regular* for column edits when it has a non-empty grid, every row
/// has exactly one cell per grid column, and no cell is horizontally or vertically
/// merged. Column insert/delete on a merged table would desync the grid from the
/// cells, so those are refused (a later slice handles spans).
fn ensure_regular_table(table: &Table) -> Result<(), EditError> {
    let cols = table.grid.len();
    if cols == 0 {
        return Err(EditError::Unsupported);
    }
    for row in &table.rows {
        if row.cells.len() != cols {
            return Err(EditError::Unsupported);
        }
        for cell in &row.cells {
            if cell.properties.grid_span.is_some_and(|s| s > 1)
                || cell.properties.vertical_merge.is_some()
            {
                return Err(EditError::Unsupported);
            }
        }
    }
    Ok(())
}

/// The table with id `table`, searching the body recursively (a table can nest in
/// a cell or content control).
fn find_table_mut(blocks: &mut [BlockNode], table: NodeId) -> Option<&mut Table> {
    // First pass: a top-level table match at this level (a returned borrow in a
    // loop that also recurses trips the borrow checker, so keep the passes apart).
    if blocks
        .iter()
        .any(|b| matches!(b, BlockNode::Table(t) if t.id == table))
    {
        return blocks.iter_mut().find_map(|b| match b {
            BlockNode::Table(t) if t.id == table => Some(t),
            _ => None,
        });
    }
    // Second pass: recurse into nested tables / content controls.
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        if let Some(found) = find_table_mut(&mut cell.blocks, table) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = find_table_mut(&mut sdt.blocks, table) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// The cell with id `cell` (mutable), searching the body recursively (including
/// nested tables). Two passes so a returned borrow never collides with the
/// recursion, matching [`find_table_mut`].
fn find_cell_mut(blocks: &mut [BlockNode], cell: NodeId) -> Option<&mut TableCell> {
    // First pass: a direct cell match at any table in these blocks.
    let direct = blocks.iter().any(|b| match b {
        BlockNode::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| c.id == cell)),
        _ => false,
    });
    if direct {
        return blocks.iter_mut().find_map(|b| match b {
            BlockNode::Table(t) => t
                .rows
                .iter_mut()
                .find_map(|r| r.cells.iter_mut().find(|c| c.id == cell)),
            _ => None,
        });
    }
    // Second pass: recurse into nested cells / content controls.
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(t) => {
                for row in &mut t.rows {
                    for c in &mut row.cells {
                        if let Some(found) = find_cell_mut(&mut c.blocks, cell) {
                            return Some(found);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(found) = find_cell_mut(&mut sdt.blocks, cell) {
                    return Some(found);
                }
            }
            _ => {}
        }
    }
    None
}

/// The id of the table cell that (recursively) contains paragraph `node`, and the id
/// of the innermost table it belongs to — what a host passes to
/// [`Operation::SetTableCellProperties`] / [`Operation::SetTableProperties`]. `None`
/// if the node is not inside a table cell.
#[must_use]
pub fn locate_cell(document: &Document, node: NodeId) -> Option<(NodeId, NodeId)> {
    fn walk(blocks: &[BlockNode], node: NodeId) -> Option<(NodeId, NodeId)> {
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for cell in &row.cells {
                            if block_contains(&cell.blocks, node) {
                                return Some((table.id, cell.id));
                            }
                            if let Some(found) = walk(&cell.blocks, node) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, node) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(document.body(), node)
}

/// A clone of the cell `cell`'s current properties (read-only), searching the body
/// recursively — what a host reads before a modify-and-`SetTableCellProperties`
/// round-trip. `None` if no such cell exists.
#[must_use]
pub fn cell_properties(document: &Document, cell: NodeId) -> Option<TableCellProperties> {
    fn walk(blocks: &[BlockNode], cell: NodeId) -> Option<TableCellProperties> {
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for c in &row.cells {
                            if c.id == cell {
                                return Some(c.properties.clone());
                            }
                            if let Some(found) = walk(&c.blocks, cell) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, cell) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(document.body(), cell)
}

/// The table with id `table` (read-only), searching the body recursively — a host
/// query for reading a table's current rows (e.g. to place the caret after a
/// row edit). `None` if no such table exists.
#[must_use]
pub fn find_table(document: &Document, table: NodeId) -> Option<&Table> {
    fn walk(blocks: &[BlockNode], table: NodeId) -> Option<&Table> {
        for block in blocks {
            match block {
                BlockNode::Table(t) if t.id == table => return Some(t),
                BlockNode::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            if let Some(found) = walk(&cell.blocks, table) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, table) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(document.body(), table)
}

/// Locates the table row that (recursively) contains paragraph `node`: the table's
/// id, the 0-based row index, and a clone of that row (a host builds a matching
/// empty row from it to insert). `None` if the node is not inside a table cell.
#[must_use]
pub fn locate_table_row(document: &Document, node: NodeId) -> Option<(NodeId, u32, TableRow)> {
    fn walk(blocks: &[BlockNode], node: NodeId) -> Option<(NodeId, u32, TableRow)> {
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for (i, row) in table.rows.iter().enumerate() {
                        for cell in &row.cells {
                            if block_contains(&cell.blocks, node) {
                                return Some((table.id, i as u32, row.clone()));
                            }
                            // Recurse into nested tables within the cell.
                            if let Some(found) = walk(&cell.blocks, node) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, node) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(document.body(), node)
}

/// Whether `node` is a paragraph directly in `blocks` (not descending into nested
/// tables — the caller handles that level).
fn block_contains(blocks: &[BlockNode], node: NodeId) -> bool {
    blocks.iter().any(|b| match b {
        BlockNode::Paragraph(p) => p.id == node,
        _ => false,
    })
}

/// Locates the table cell that (recursively) contains paragraph `node`: the table's
/// id and the 0-based cell index within its row. For a regular table (the only kind
/// column edits accept) the cell index equals the grid column index. `None` if the
/// node is not inside a table cell.
#[must_use]
pub fn locate_table_cell(document: &Document, node: NodeId) -> Option<(NodeId, u32)> {
    fn walk(blocks: &[BlockNode], node: NodeId) -> Option<(NodeId, u32)> {
        for block in blocks {
            match block {
                BlockNode::Table(table) => {
                    for row in &table.rows {
                        for (ci, cell) in row.cells.iter().enumerate() {
                            if block_contains(&cell.blocks, node) {
                                return Some((table.id, ci as u32));
                            }
                            if let Some(found) = walk(&cell.blocks, node) {
                                return Some(found);
                            }
                        }
                    }
                }
                BlockNode::Sdt(sdt) => {
                    if let Some(found) = walk(&sdt.blocks, node) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(document.body(), node)
}

/// The properties of paragraph `node` (a clone), for a host to read the current
/// alignment/spacing/… before computing a change. `None` if not a paragraph.
#[must_use]
pub fn paragraph_properties(document: &Document, node: NodeId) -> Option<ParagraphProperties> {
    find_paragraph(document.body(), node).map(|p| p.properties.clone())
}

/// Ensures a run boundary exists at byte `offset` by splitting the run that
/// straddles it (the tail becomes a new run with the same properties). A no-op
/// when the offset already falls on a run boundary or outside every run.
fn ensure_run_boundary(
    inlines: &mut Vec<InlineNode>,
    offset: u32,
    ids: &mut dyn RunIds,
) -> Result<(), EditError> {
    let target = run_segments(inlines)
        .into_iter()
        .find(|s| offset > s.start && offset < s.end)
        .map(|s| (s.idx, (offset - s.start) as usize));
    let Some((idx, local)) = target else {
        return Ok(());
    };
    let (head, tail, properties) = match &inlines[idx] {
        InlineNode::Run(run) => {
            if !run.text.is_char_boundary(local) {
                return Err(EditError::NotCharBoundary);
            }
            (
                run.text[..local].to_string(),
                run.text[local..].to_string(),
                run.properties.clone(),
            )
        }
        _ => return Ok(()),
    };
    let tail_id = ids.next().ok_or(EditError::IdExhausted)?;
    if let InlineNode::Run(run) = &mut inlines[idx] {
        run.text = head;
    }
    inlines.insert(
        idx + 1,
        InlineNode::Run(Run {
            id: tail_id,
            properties,
            text: tail,
        }),
    );
    Ok(())
}

/// Whether each toggle is uniformly on across a formatted range — drives a
/// toolbar's active state and the toggle direction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FormatState {
    /// Every covered run is bold.
    pub bold: bool,
    /// Every covered run is italic.
    pub italic: bool,
    /// Every covered run is underlined.
    pub underline: bool,
    /// Every covered run is struck through.
    pub strike: bool,
}

/// The [`FormatState`] of the runs a `range` covers within one paragraph — `true`
/// for a toggle only when **every** covered run sets it (an empty range or no
/// covered runs yields all-false).
#[must_use]
pub fn format_state(document: &Document, range: Range) -> FormatState {
    if range.start.node != range.end.node || range.end.offset <= range.start.offset {
        return FormatState::default();
    }
    let covered = run_properties_in_range(document, range);
    if covered.is_empty() {
        return FormatState::default();
    }
    let all = |f: fn(&RunProperties) -> Option<bool>| covered.iter().all(|p| f(p) == Some(true));
    FormatState {
        bold: all(|p| p.bold),
        italic: all(|p| p.italic),
        underline: all(|p| p.underline),
        strike: all(|p| p.strike),
    }
}

/// The run formatting a caret at `(node, offset)` inherits — what new typing there
/// would carry. Word's rule: the run to the **left** of the caret, or (at a
/// paragraph start) the run to the right, or defaults for an empty paragraph. This
/// drives the toolbar's active state at a collapsed caret and the "type bold"
/// toggle direction, where [`format_state`] (which needs a non-empty range) returns
/// all-false.
#[must_use]
pub fn caret_format(document: &Document, node: NodeId, offset: u32) -> FormatState {
    let props = caret_run_properties(document, node, offset)
        .cloned()
        .unwrap_or_default();
    let on = |v: Option<bool>| v == Some(true);
    FormatState {
        bold: on(props.bold),
        italic: on(props.italic),
        underline: on(props.underline),
        strike: on(props.strike),
    }
}

/// The size / color / font / super-sub a caret at `(node, offset)` inherits — the
/// caret counterpart to [`run_style_state`], so a collapsed caret reflects (and can
/// arm) the same run styling a selection does. Defaults when the paragraph is empty.
#[must_use]
pub fn caret_run_style(document: &Document, node: NodeId, offset: u32) -> RunStyleState {
    let Some(props) = caret_run_properties(document, node, offset) else {
        return RunStyleState::default();
    };
    RunStyleState {
        size_half_points: props.size_half_points,
        color_rgb: match props.color {
            Some(Color::Rgb(c)) => Some(c),
            _ => None,
        },
        font: match &props.font_ref {
            Some(FontRef::Named(name)) => Some(name.name.clone()),
            _ => None,
        },
        superscript: props.vertical_alignment == Some(VerticalAlignment::Superscript),
        subscript: props.vertical_alignment == Some(VerticalAlignment::Subscript),
    }
}

/// The run properties a caret at `(node, offset)` inherits — the run to its left
/// (Word's rule), else the run to its right at a paragraph start, else the first
/// run; `None` for an empty paragraph. The shared basis of the caret-format and
/// caret-style queries (what new typing there would carry).
#[must_use]
pub fn caret_run_properties(
    document: &Document,
    node: NodeId,
    offset: u32,
) -> Option<&RunProperties> {
    let para = find_paragraph(document.body(), node)?;
    // Flatten across final-with-markup wrappers so a caret resting inside a
    // pending tracked revision reflects that run's formatting (docs/81
    // REVIEW-GAP-030), not the paragraph default.
    let segs = flatten_run_segments(&para.inlines);
    segs.iter()
        .find(|s| offset > s.start && offset <= s.end)
        .or_else(|| segs.iter().find(|s| offset >= s.start && offset < s.end))
        .or_else(|| segs.first())
        .map(|s| s.properties)
}

/// The uniform run styling across a range — each field is `Some`/`true` only when
/// **every** covered run shares that value, so a toolbar can show the current
/// size/color/font (or blank for a mixed selection).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunStyleState {
    /// Common font size in half-points, if uniform.
    pub size_half_points: Option<u32>,
    /// Common RGB text color, if uniform (theme colors count as mixed).
    pub color_rgb: Option<RgbColor>,
    /// Common font family, if uniform.
    pub font: Option<String>,
    /// Every covered run is superscript.
    pub superscript: bool,
    /// Every covered run is subscript.
    pub subscript: bool,
}

/// The [`RunStyleState`] of the runs a `range` covers within one paragraph.
#[must_use]
pub fn run_style_state(document: &Document, range: Range) -> RunStyleState {
    if range.start.node != range.end.node || range.end.offset <= range.start.offset {
        return RunStyleState::default();
    }
    let covered = run_properties_in_range(document, range);
    if covered.is_empty() {
        return RunStyleState::default();
    }
    RunStyleState {
        size_half_points: uniform(&covered, |p| p.size_half_points),
        color_rgb: uniform(&covered, |p| match p.color {
            Some(Color::Rgb(c)) => Some(c),
            _ => None,
        }),
        font: uniform(&covered, |p| match &p.font_ref {
            Some(FontRef::Named(name)) => Some(name.name.clone()),
            _ => None,
        }),
        superscript: covered
            .iter()
            .all(|p| p.vertical_alignment == Some(VerticalAlignment::Superscript)),
        subscript: covered
            .iter()
            .all(|p| p.vertical_alignment == Some(VerticalAlignment::Subscript)),
    }
}

/// The direct run properties covered by a non-empty range within one paragraph.
///
/// This is intentionally a direct-property query: hosts that need the effective
/// value visible to a user must pass each result through the document style
/// cascade. Returning references keeps the editing crate independent of layout
/// while avoiding another run-segmentation implementation at bridge layers.
#[must_use]
pub fn run_properties_in_range(document: &Document, range: Range) -> Vec<&RunProperties> {
    if range.start.node != range.end.node || range.end.offset <= range.start.offset {
        return Vec::new();
    }
    let Some(para) = find_paragraph(document.body(), range.start.node) else {
        return Vec::new();
    };
    // Descend into final-with-markup-contributing wrappers so a selection that
    // touches a run inside a pending tracked revision (or hyperlink/SDT)
    // reflects that run's real formatting, matching the copy/layout projections
    // (docs/81 REVIEW-GAP-030). The editing/split paths keep using
    // `run_segments` (top-level runs only) because revision-aware splitting is
    // separate work (REVIEW-GAP-007).
    flatten_run_segments(&para.inlines)
        .into_iter()
        .filter(|s| s.end > range.start.offset && s.start < range.end.offset && s.start < s.end)
        .map(|s| s.properties)
        .collect()
}

/// The common value of `f` across all covered runs, or `None` if any run differs
/// or leaves it unset.
fn uniform<T: PartialEq>(
    covered: &[&RunProperties],
    f: impl Fn(&RunProperties) -> Option<T>,
) -> Option<T> {
    let first = f(covered[0])?;
    covered
        .iter()
        .skip(1)
        .all(|p| f(p).as_ref() == Some(&first))
        .then_some(first)
}

/// Finds the paragraph with `id` (immutable), recursing into tables and content
/// controls.
#[must_use]
pub fn find_paragraph(blocks: &[BlockNode], id: NodeId) -> Option<&Paragraph> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(p) if p.id == id => return Some(p),
            BlockNode::Paragraph(_) => {}
            BlockNode::Table(table) => {
                for row in &table.rows {
                    for cell in &row.cells {
                        if let Some(p) = find_paragraph(&cell.blocks, id) {
                            return Some(p);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(p) = find_paragraph(&sdt.blocks, id) {
                    return Some(p);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

/// A source of fresh run identities. Backed by
/// [`IdGenerator`](casual_doc_model::IdGenerator) in practice; a trait so the
/// edit crate does not dictate the id-allocation policy.
pub trait RunIds {
    /// Returns a fresh, unique node id, or `None` if the space is exhausted.
    fn next(&mut self) -> Option<NodeId>;
}

impl RunIds for casual_doc_model::IdGenerator {
    fn next(&mut self) -> Option<NodeId> {
        self.next_id().ok()
    }
}

/// The byte range of one top-level [`InlineNode::Run`] in a paragraph's text.
struct RunSeg {
    /// Index into the paragraph's `inlines`.
    idx: usize,
    /// Byte offset of the run's first byte in the concatenated text.
    start: u32,
    /// Byte offset one past the run's last byte.
    end: u32,
}

/// The text bytes a single inline contributes — identical to
/// `node_plain_text`'s accounting, so offsets align with hit-testing.
fn inline_text_len(inline: &InlineNode) -> u32 {
    match inline {
        InlineNode::Run(run) => run.text.len() as u32,
        InlineNode::Tab(_) => 1,
        InlineNode::Symbol(symbol) => {
            char::from_u32(symbol.char).map_or(0, |c| c.len_utf8() as u32)
        }
        InlineNode::Hyperlink(hyperlink) => nested_len(&hyperlink.inlines),
        InlineNode::Revision(revision)
            if revision
                .kind
                .contributes_to(ReviewProjection::FinalWithMarkup) =>
        {
            nested_len(&revision.inlines)
        }
        InlineNode::Revision(_) => 0,
        InlineNode::Sdt(sdt) => nested_len(&sdt.inlines),
        _ => 0,
    }
}

fn nested_len(inlines: &[InlineNode]) -> u32 {
    inlines.iter().map(inline_text_len).sum()
}

/// The paragraph's total shaped-text byte length.
fn paragraph_text_len(para: &Paragraph) -> u32 {
    para.inlines.iter().map(inline_text_len).sum()
}

/// One run's projected byte range and its direct properties, flattened across
/// the final-with-markup-contributing wrappers so a run inside a pending tracked
/// revision (or hyperlink/SDT) is visible to read-only reflection queries
/// (docs/81 REVIEW-GAP-030). Distinct from [`RunSeg`], which the editing/split
/// paths use for top-level runs only.
struct FlatRun<'a> {
    /// Byte offset of the run's first byte in the projected paragraph text.
    start: u32,
    /// Byte offset one past the run's last byte.
    end: u32,
    /// The run's direct properties.
    properties: &'a RunProperties,
}

/// Every run in projected order — top-level and nested inside
/// final-with-markup-contributing `Revision`/`Hyperlink`/`Sdt` wrappers — with
/// cumulative byte offsets aligned to [`inline_text_len`], mirroring the copy
/// path's `walk_inlines_rich`. Used by the reflection/caret-property queries so
/// formatting inside a pending suggestion is not silently dropped.
fn flatten_run_segments(inlines: &[InlineNode]) -> Vec<FlatRun<'_>> {
    let mut out = Vec::new();
    let mut cum = 0u32;
    push_run_segments(inlines, &mut cum, &mut out);
    out
}

fn push_run_segments<'a>(inlines: &'a [InlineNode], cum: &mut u32, out: &mut Vec<FlatRun<'a>>) {
    for inline in inlines {
        match inline {
            InlineNode::Run(run) => {
                let start = *cum;
                let end = start.saturating_add(run.text.len() as u32);
                *cum = end;
                out.push(FlatRun {
                    start,
                    end,
                    properties: &run.properties,
                });
            }
            InlineNode::Hyperlink(link) => push_run_segments(&link.inlines, cum, out),
            InlineNode::Revision(revision)
                if revision
                    .kind
                    .contributes_to(ReviewProjection::FinalWithMarkup) =>
            {
                push_run_segments(&revision.inlines, cum, out);
            }
            InlineNode::Revision(_) => {}
            InlineNode::Sdt(sdt) => push_run_segments(&sdt.inlines, cum, out),
            other => {
                *cum = cum.saturating_add(inline_text_len(other));
            }
        }
    }
}

/// The byte ranges of the paragraph's top-level runs (the editable segments).
fn run_segments(inlines: &[InlineNode]) -> Vec<RunSeg> {
    let mut segs = Vec::new();
    let mut cum = 0u32;
    for (idx, inline) in inlines.iter().enumerate() {
        let len = inline_text_len(inline);
        if matches!(inline, InlineNode::Run(_)) {
            segs.push(RunSeg {
                idx,
                start: cum,
                end: cum + len,
            });
        }
        cum += len;
    }
    segs
}

fn valid_hyperlink_values(target: Option<&HyperlinkTarget>, tooltip: Option<&str>) -> bool {
    let target_valid = target.is_none_or(|target| match target {
        HyperlinkTarget::External(external) => {
            !external.url.is_empty() && external.url.len() <= 2048
        }
        HyperlinkTarget::Internal(internal) => {
            !internal.anchor.is_empty() && internal.anchor.len() <= 255
        }
    });
    let tooltip_valid = tooltip.is_none_or(|value| !value.is_empty() && value.len() <= 255);
    target_valid && tooltip_valid
}

/// Returns the top-level hyperlink whose cumulative text range exactly matches
/// `[start, end)`.
fn exact_hyperlink_index(inlines: &[InlineNode], start: u32, end: u32) -> Option<usize> {
    let mut offset = 0u32;
    for (index, inline) in inlines.iter().enumerate() {
        let next = offset.saturating_add(inline_text_len(inline));
        if matches!(inline, InlineNode::Hyperlink(_)) && offset == start && next == end {
            return Some(index);
        }
        offset = next;
    }
    None
}

/// Returns the contiguous top-level inline indices exactly covered by
/// `[start, end)`. A partial overlap with a non-run wrapper is rejected.
fn covered_top_level_indices(
    inlines: &[InlineNode],
    start: u32,
    end: u32,
) -> Result<Vec<usize>, EditError> {
    let mut covered = Vec::new();
    let mut offset = 0u32;
    for (index, inline) in inlines.iter().enumerate() {
        let len = inline_text_len(inline);
        let next = offset.saturating_add(len);
        if len > 0 && offset < end && next > start {
            if offset < start || next > end {
                return Err(EditError::Unsupported);
            }
            covered.push(index);
        }
        offset = next;
    }
    Ok(covered)
}

/// Inserts `text` at `offset` into a paragraph's inlines, splicing into the run
/// the offset lands in (or the nearest run, or a new run for an empty paragraph).
fn insert_text(
    inlines: &mut Vec<InlineNode>,
    offset: u32,
    text: &str,
    ids: &mut dyn RunIds,
) -> Result<(), EditError> {
    let segs = run_segments(inlines);

    // The run whose range contains the offset (interior or either boundary).
    if let Some(seg) = segs.iter().find(|s| offset >= s.start && offset <= s.end) {
        let local = (offset - seg.start) as usize;
        if let InlineNode::Run(run) = &mut inlines[seg.idx] {
            if !run.text.is_char_boundary(local) {
                return Err(EditError::NotCharBoundary);
            }
            run.text.insert_str(local, text);
            return Ok(());
        }
    }
    // Offset sits exactly at a non-run boundary (e.g. right after/before a
    // hyperlink or tab): extend the run truly touching that edge, not merely
    // the nearest one — `<=`/`>=` here would let the insert jump *across* an
    // intervening non-run inline (a trailing hyperlink, say) into a run that
    // only looks "nearest" by position, silently absorbing new text into the
    // wrong (and wrongly-formatted) run.
    if let Some(seg) = segs.iter().find(|s| s.end == offset)
        && let InlineNode::Run(run) = &mut inlines[seg.idx]
    {
        run.text.push_str(text);
        return Ok(());
    }
    if let Some(seg) = segs.iter().find(|s| s.start == offset)
        && let InlineNode::Run(run) = &mut inlines[seg.idx]
    {
        run.text.insert_str(0, text);
        return Ok(());
    }
    // …else no run touches `offset` at all (it sits at the edge of a non-run
    // inline with no adjacent run — e.g. a paragraph ending in a hyperlink, or
    // an empty paragraph): insert a fresh run at the matching top-level
    // position, not always at the front.
    let mut cum = 0u32;
    let mut insert_at = inlines.len();
    for (idx, inline) in inlines.iter().enumerate() {
        if cum == offset {
            insert_at = idx;
            break;
        }
        cum += inline_text_len(inline);
    }
    let id = ids.next().ok_or(EditError::IdExhausted)?;
    inlines.insert(
        insert_at,
        InlineNode::Run(Run {
            id,
            properties: RunProperties::default(),
            text: text.to_string(),
        }),
    );
    Ok(())
}

/// Deletes `[start, end)` when it lies within a single top-level run, returning
/// `Some(removed_text)`. Returns `None` when the range spans more than one run (or
/// a non-run inline), so the caller falls to the general multi-run path; a
/// mid-character offset is still a hard `NotCharBoundary` error.
fn delete_text(
    inlines: &mut [InlineNode],
    start: u32,
    end: u32,
) -> Result<Option<String>, EditError> {
    let segs = run_segments(inlines);
    let Some(seg) = segs.iter().find(|s| start >= s.start && end <= s.end) else {
        return Ok(None);
    };
    // Deleting the whole run would remove it and could leave its neighbours adjacent
    // (and possibly equal-propertied, which the model forbids). Defer that to the
    // general path — its `SetInlines` inverse stays exact through the coalescing.
    if start == seg.start && end == seg.end {
        return Ok(None);
    }
    let (from, to) = ((start - seg.start) as usize, (end - seg.start) as usize);
    let idx = seg.idx;

    let InlineNode::Run(run) = &mut inlines[idx] else {
        return Ok(None);
    };
    if !run.text.is_char_boundary(from) || !run.text.is_char_boundary(to) {
        return Err(EditError::NotCharBoundary);
    }
    let removed = run.text[from..to].to_string();
    run.text.replace_range(from..to, "");
    // The run keeps text (full-run deletion bailed above), so no neighbours merge
    // and the plain-text `InsertText` inverse is exact.
    Ok(Some(removed))
}

/// Merges adjacent top-level runs with equal properties into one. The model forbids
/// adjacent equal-property runs, and a delete that drops the content separating two
/// such runs (or empties a run between them) would otherwise leave them adjacent —
/// so the delete path coalesces before returning. Text and total length are
/// unchanged, so byte offsets are preserved; the merged run keeps the first's id.
fn coalesce_adjacent_runs(inlines: &mut Vec<InlineNode>) {
    let mut i = 0;
    while i + 1 < inlines.len() {
        let mergeable = matches!(
            (&inlines[i], &inlines[i + 1]),
            (InlineNode::Run(a), InlineNode::Run(b)) if a.properties == b.properties
        );
        if mergeable {
            let InlineNode::Run(next) = inlines.remove(i + 1) else {
                unreachable!("matched a run above");
            };
            if let InlineNode::Run(cur) = &mut inlines[i] {
                cur.text.push_str(&next.text);
            }
        } else {
            i += 1;
        }
    }
}

/// Removes every inline that lies fully inside `[start, end)`, by cumulative text
/// length. The caller runs [`ensure_run_boundary`] at both ends first, so every
/// **run** is then either wholly inside or wholly outside the range. A non-run
/// wrapper (hyperlink, content control, tab, symbol) cannot be split here, so a
/// range that only *partially* covers one is refused (`Unsupported`) rather than
/// silently mis-deleting — editing inside a nested wrapper is a later slice.
fn remove_covered_range(
    inlines: &mut Vec<InlineNode>,
    start: u32,
    end: u32,
) -> Result<(), EditError> {
    // First pass: reject a partial cut into an inline we cannot split.
    let mut cum = 0u32;
    for inline in inlines.iter() {
        let len = inline_text_len(inline);
        let (s, e) = (cum, cum + len);
        cum = e;
        if len > 0 && s < end && e > start && !(s >= start && e <= end) {
            return Err(EditError::Unsupported);
        }
    }
    // Second pass: drop the fully-covered inlines.
    let mut cum = 0u32;
    inlines.retain(|inline| {
        let len = inline_text_len(inline);
        let (s, e) = (cum, cum + len);
        cum = e;
        !(len > 0 && s >= start && e <= end)
    });
    Ok(())
}

/// Splits the paragraph `id` at byte `offset` into two, inserting the trailing
/// half as a new paragraph `new_id` immediately after. Recurses into tables and
/// content controls to find the paragraph's container. Returns whether it was
/// found and split.
fn split_paragraph(
    blocks: &mut Vec<BlockNode>,
    id: NodeId,
    offset: u32,
    new_id: NodeId,
    ids: &mut dyn RunIds,
) -> Result<bool, EditError> {
    if let Some(index) = blocks
        .iter()
        .position(|b| matches!(b, BlockNode::Paragraph(p) if p.id == id))
    {
        let BlockNode::Paragraph(para) = &mut blocks[index] else {
            unreachable!("index selected a paragraph");
        };
        if offset > paragraph_text_len(para) {
            return Err(EditError::OffsetOutOfRange);
        }
        let inlines = std::mem::take(&mut para.inlines);
        let (left, right) = split_inlines(inlines, offset, ids)?;
        let properties = para.properties.clone();
        para.inlines = left;
        blocks.insert(
            index + 1,
            BlockNode::Paragraph(Paragraph {
                id: new_id,
                properties,
                inlines: right,
            }),
        );
        return Ok(true);
    }
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if split_paragraph(&mut cell.blocks, id, offset, new_id, ids)? {
                            return Ok(true);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if split_paragraph(&mut sdt.blocks, id, offset, new_id, ids)? {
                    return Ok(true);
                }
            }
            _ => {}
        }
    }
    Ok(false)
}

/// Joins `second` (which must immediately follow `first`) into `first`, removing
/// `second`. Returns the byte offset at which `first` ended before the join (the
/// inverse split point), or `None` if `first` was not found. `Err(Unsupported)`
/// if `second` is not the adjacent next paragraph.
fn join_paragraphs(
    blocks: &mut Vec<BlockNode>,
    first: NodeId,
    second: NodeId,
) -> Result<Option<u32>, EditError> {
    if let Some(i) = blocks
        .iter()
        .position(|b| matches!(b, BlockNode::Paragraph(p) if p.id == first))
    {
        if !matches!(blocks.get(i + 1), Some(BlockNode::Paragraph(p)) if p.id == second) {
            return Err(EditError::Unsupported);
        }
        let BlockNode::Paragraph(second_para) = blocks.remove(i + 1) else {
            unreachable!("checked it is a paragraph");
        };
        let BlockNode::Paragraph(first_para) = &mut blocks[i] else {
            unreachable!("position found a paragraph");
        };
        let split_at = paragraph_text_len(first_para);
        first_para.inlines.extend(second_para.inlines);
        return Ok(Some(split_at));
    }
    for block in blocks.iter_mut() {
        match block {
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(at) = join_paragraphs(&mut cell.blocks, first, second)? {
                            return Ok(Some(at));
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(at) = join_paragraphs(&mut sdt.blocks, first, second)? {
                    return Ok(Some(at));
                }
            }
            _ => {}
        }
    }
    Ok(None)
}

/// Splits a paragraph's inline content at byte `offset` into (left, right). A run
/// straddling the offset is split (the right half gets a fresh id). A non-run
/// inline straddling the offset (a wrapper) is a slice-2 limit → `Unsupported`;
/// at a boundary it goes wholly to one side.
fn split_inlines(
    inlines: Vec<InlineNode>,
    offset: u32,
    ids: &mut dyn RunIds,
) -> Result<(Vec<InlineNode>, Vec<InlineNode>), EditError> {
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut cum = 0u32;
    for inline in inlines {
        let len = inline_text_len(&inline);
        if cum >= offset {
            right.push(inline);
        } else if cum + len <= offset {
            left.push(inline);
        } else if let InlineNode::Run(run) = inline {
            let local = (offset - cum) as usize;
            if !run.text.is_char_boundary(local) {
                return Err(EditError::NotCharBoundary);
            }
            let (head, tail) = run.text.split_at(local);
            left.push(InlineNode::Run(Run {
                id: run.id,
                properties: run.properties.clone(),
                text: head.to_string(),
            }));
            let tail_id = ids.next().ok_or(EditError::IdExhausted)?;
            right.push(InlineNode::Run(Run {
                id: tail_id,
                properties: run.properties,
                text: tail.to_string(),
            }));
        } else {
            return Err(EditError::Unsupported);
        }
        cum += len;
    }
    Ok((left, right))
}

/// Finds the paragraph with `id`, recursing into table cells and block content
/// controls (document order), for in-place mutation.
fn find_paragraph_mut(blocks: &mut [BlockNode], id: NodeId) -> Option<&mut Paragraph> {
    for block in blocks {
        match block {
            BlockNode::Paragraph(p) if p.id == id => return Some(p),
            BlockNode::Paragraph(_) => {}
            BlockNode::Table(table) => {
                for row in &mut table.rows {
                    for cell in &mut row.cells {
                        if let Some(p) = find_paragraph_mut(&mut cell.blocks, id) {
                            return Some(p);
                        }
                    }
                }
            }
            BlockNode::Sdt(sdt) => {
                if let Some(p) = find_paragraph_mut(&mut sdt.blocks, id) {
                    return Some(p);
                }
            }
            BlockNode::AltChunk(_) => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::IdGenerator;
    use casual_doc_model::v1::{
        Definitions, DocGrid, LineNumbering, NoteProperties, PageBorders, PageNumbering,
        PaperSource, ParagraphProperties, Revision, RevisionKind, SectionBoundary, SectionColumns,
    };

    fn n(counter: u64) -> NodeId {
        NodeId::from_parts(7, counter).unwrap()
    }

    fn run(id: u64, text: &str) -> InlineNode {
        InlineNode::Run(Run {
            id: n(id),
            properties: RunProperties::default(),
            text: text.to_string(),
        })
    }

    fn revision(id: u64, run_id: u64, kind: RevisionKind, text: &str) -> InlineNode {
        InlineNode::Revision(Revision {
            id: n(id),
            kind,
            author: Some("Reviewer".to_owned()),
            date: None,
            revision_id: Some(id.to_string()),
            editor_group: None,
            inlines: vec![run(run_id, text)],
        })
    }

    fn para(id: u64, inlines: Vec<InlineNode>) -> BlockNode {
        BlockNode::Paragraph(Paragraph {
            id: n(id),
            properties: ParagraphProperties::default(),
            inlines,
        })
    }

    fn doc(paragraphs: Vec<BlockNode>) -> Document {
        Document::new(n(1000), paragraphs, Definitions::default()).expect("valid document")
    }

    fn external(url: &str) -> HyperlinkTarget {
        HyperlinkTarget::External(casual_doc_model::v1::ExternalTarget {
            url: url.to_owned(),
        })
    }

    /// The concatenated text of paragraph `id` (top-level runs), for assertions.
    fn text_of(document: &Document, id: NodeId) -> String {
        fn walk(blocks: &[BlockNode], id: NodeId) -> Option<String> {
            for block in blocks {
                match block {
                    BlockNode::Paragraph(p) if p.id == id => {
                        return Some(
                            p.inlines
                                .iter()
                                .filter_map(|i| match i {
                                    InlineNode::Run(r) => Some(r.text.clone()),
                                    _ => None,
                                })
                                .collect(),
                        );
                    }
                    BlockNode::Table(t) => {
                        for row in &t.rows {
                            for cell in &row.cells {
                                if let Some(s) = walk(&cell.blocks, id) {
                                    return Some(s);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        walk(document.body(), id).unwrap_or_default()
    }

    #[test]
    fn set_extent_resizes_an_inline_drawing_and_inverse_restores_it() {
        use casual_doc_model::v1::{Drawing, Extent, MediaId, MediaReference};
        let media = MediaId::new(NodeId::from_parts(7, 900).unwrap());
        let drawing_id = n(50);
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rId9".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let mut d = Document::new(
            n(1000),
            vec![BlockNode::Paragraph(Paragraph {
                id: n(2),
                properties: ParagraphProperties::default(),
                inlines: vec![
                    run(3, "before"),
                    InlineNode::Drawing(Drawing {
                        id: drawing_id,
                        media,
                        extent: Some(Extent {
                            width_emu: 914_400,
                            height_emu: 457_200,
                        }),
                        descr: None,
                        crop: None,
                    }),
                ],
            })],
            definitions,
        )
        .expect("valid document with a registered media part");
        let mut ids = IdGenerator::new(9);

        let new_extent = Some(Extent {
            width_emu: 1_828_800,
            height_emu: 914_400,
        });
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetExtent {
                object: drawing_id,
                extent: new_extent,
            },
        )
        .expect("resize the drawing");
        // The inverse carries the previous extent.
        assert_eq!(
            inverse,
            Operation::SetExtent {
                object: drawing_id,
                extent: Some(Extent {
                    width_emu: 914_400,
                    height_emu: 457_200,
                }),
            }
        );
        // Applying the inverse restores the original extent (undo).
        apply(&mut d, &mut ids, &inverse).expect("undo the resize");
        let BlockNode::Paragraph(p) = &d.body()[0] else {
            unreachable!()
        };
        let InlineNode::Drawing(drawing) = &p.inlines[1] else {
            panic!("the drawing is intact");
        };
        assert_eq!(drawing.extent.unwrap().width_emu, 914_400);

        // An unknown object is rejected.
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::SetExtent {
                    object: n(999),
                    extent: new_extent,
                },
            ),
            Err(EditError::NodeNotFound)
        ));
    }

    #[test]
    fn set_anchor_moves_and_rewraps_a_floating_drawing_with_exact_inverse() {
        use casual_doc_model::v1::{
            AnchorHorizontal, AnchorVertical, AnchoredDrawing, DrawingAnchor, Extent,
            HorizontalAnchor, HorizontalPosition, MediaId, MediaReference, VerticalAnchor,
            VerticalPosition, WrapDistances, WrapMode,
        };
        let media = MediaId::new(NodeId::from_parts(7, 901).unwrap());
        let float_id = n(60);
        let original = DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Column,
                position: HorizontalPosition::Offset(100_000),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Paragraph,
                position: VerticalPosition::Offset(50_000),
            },
            wrap: WrapMode::Square,
            wrap_distances: WrapDistances::default(),
            behind_doc: false,
        };
        let mut definitions = Definitions::default();
        definitions.media.insert(
            media,
            MediaReference {
                relationship_id: "rId9".to_owned(),
                media_type: "image/png".to_owned(),
                part_name: "word/media/image1.png".to_owned(),
            },
        );
        let mut d = Document::new(
            n(1000),
            vec![BlockNode::Paragraph(Paragraph {
                id: n(2),
                properties: ParagraphProperties::default(),
                inlines: vec![
                    run(3, "anchor"),
                    InlineNode::AnchoredDrawing(AnchoredDrawing {
                        id: float_id,
                        media,
                        extent: Extent {
                            width_emu: 914_400,
                            height_emu: 457_200,
                        },
                        anchor: original,
                        descr: None,
                        relative_height: None,
                        crop: None,
                    }),
                ],
            })],
            definitions,
        )
        .expect("valid document with a floating drawing");
        let mut ids = IdGenerator::new(9);

        // Move to an absolute page position + change wrap to behind-text.
        let moved = DrawingAnchor {
            horizontal: AnchorHorizontal {
                relative_from: HorizontalAnchor::Page,
                position: HorizontalPosition::Offset(2_000_000),
            },
            vertical: AnchorVertical {
                relative_from: VerticalAnchor::Page,
                position: VerticalPosition::Offset(3_000_000),
            },
            wrap: WrapMode::None,
            wrap_distances: WrapDistances::default(),
            behind_doc: true,
        };
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetAnchor {
                object: float_id,
                anchor: Box::new(moved),
            },
        )
        .expect("move + re-wrap the float");
        // The inverse carries the original anchor.
        assert_eq!(
            inverse,
            Operation::SetAnchor {
                object: float_id,
                anchor: Box::new(original),
            }
        );
        // Applying the inverse restores it exactly (undo).
        apply(&mut d, &mut ids, &inverse).expect("undo the move");
        let BlockNode::Paragraph(p) = &d.body()[0] else {
            unreachable!()
        };
        let InlineNode::AnchoredDrawing(drawing) = &p.inlines[1] else {
            panic!("the float is intact");
        };
        assert_eq!(drawing.anchor, original);

        // An inline (non-floating) object has no anchor to set — rejected.
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::SetAnchor {
                    object: n(3), // a plain run, not floating
                    anchor: Box::new(moved),
                },
            ),
            Err(EditError::NodeNotFound)
        ));
    }

    #[test]
    fn insert_blocks_splices_a_sequence_and_inverse_removes_it() {
        // Body: [P2, P3]. Insert two blocks between them at index 1.
        let mut d = doc(vec![
            para(2, vec![run(3, "one")]),
            para(4, vec![run(5, "two")]),
        ]);
        let mut ids = IdGenerator::new(9);

        let inserted = vec![para(6, vec![run(7, "A")]), para(8, vec![run(9, "B")])];
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertBlocks {
                container: None,
                index: 1,
                blocks: inserted.clone(),
            },
        )
        .expect("insert blocks");
        assert_eq!(d.body().len(), 4, "two blocks spliced in");
        assert_eq!(text_of(&d, n(6)), "A");
        assert_eq!(text_of(&d, n(8)), "B");
        assert_eq!(
            inverse,
            Operation::DeleteBlocks {
                container: None,
                index: 1,
                count: 2,
            }
        );

        // Applying the inverse removes exactly the two inserted blocks (undo),
        // and its own inverse restores them verbatim (redo).
        let redo = apply(&mut d, &mut ids, &inverse).expect("delete blocks");
        assert_eq!(d.body().len(), 2, "the two inserted blocks are gone");
        assert_eq!(text_of(&d, n(2)), "one");
        assert_eq!(text_of(&d, n(4)), "two");
        assert_eq!(
            redo,
            Operation::InsertBlocks {
                container: None,
                index: 1,
                blocks: inserted,
            }
        );
        apply(&mut d, &mut ids, &redo).expect("redo");
        assert_eq!(d.body().len(), 4);
        assert_eq!(text_of(&d, n(6)), "A");
    }

    #[test]
    fn insert_blocks_rejects_an_out_of_range_index_and_empty_edit() {
        let mut d = doc(vec![para(2, vec![run(3, "x")])]);
        let mut ids = IdGenerator::new(9);
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertBlocks {
                    container: None,
                    index: 9,
                    blocks: vec![para(4, vec![run(5, "y")])],
                },
            ),
            Err(EditError::OffsetOutOfRange)
        ));
        assert!(matches!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertBlocks {
                    container: None,
                    index: 0,
                    blocks: Vec::new(),
                },
            ),
            Err(EditError::EmptyEdit)
        ));
    }

    #[test]
    fn insert_splices_and_inverse_removes() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Helloworld")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(p, 5),
                text: " brave ".to_string(),
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "Hello brave world");
        assert_eq!(
            inverse,
            Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 5),
                    end: Pos::new(p, 12), // 5 + len(" brave ")
                },
            }
        );

        // Applying the inverse restores the original text (undo).
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(text_of(&d, p), "Helloworld");
    }

    #[test]
    fn edits_after_hidden_deletion_use_final_projected_offsets() {
        let p = n(2);
        let mut d = doc(vec![para(
            2,
            vec![
                revision(3, 4, RevisionKind::Deletion, "removed"),
                run(5, "B"),
            ],
        )]);
        let mut ids = IdGenerator::new(9);
        let paragraph = find_paragraph(d.body(), p).expect("paragraph");
        assert_eq!(paragraph_text_len(paragraph), 1);

        apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(p, 0),
                text: "A".to_owned(),
            },
        )
        .expect("insert at projected start");
        assert_eq!(text_of(&d, p), "AB");
        let paragraph = find_paragraph(d.body(), p).expect("paragraph");
        assert_eq!(paragraph_text_len(paragraph), 2);
    }

    #[test]
    fn set_hyperlink_create_update_remove_and_inverse_are_exact() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello world")])]);
        let mut ids = IdGenerator::new(9);
        let range = Range {
            start: Pos::new(p, 6),
            end: Pos::new(p, 11),
        };

        let original = d.clone();
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetHyperlink {
                range,
                id: n(4),
                target: Some(external("https://example.com/one")),
                tooltip: Some("Example".to_owned()),
            },
        )
        .unwrap();
        d.validate().unwrap();
        let para = find_paragraph(d.body(), p).unwrap();
        let InlineNode::Hyperlink(link) = &para.inlines[1] else {
            panic!("selected text was not wrapped");
        };
        assert_eq!(link.id, n(4));
        assert_eq!(link.target, external("https://example.com/one"));
        assert_eq!(nested_len(&link.inlines), 5);
        let created = d.clone();

        let update_inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetHyperlink {
                range,
                id: n(99),
                target: Some(external("https://example.com/two")),
                tooltip: None,
            },
        )
        .unwrap();
        let para = find_paragraph(d.body(), p).unwrap();
        let InlineNode::Hyperlink(link) = &para.inlines[1] else {
            panic!("existing link disappeared");
        };
        assert_eq!(link.id, n(4), "updates preserve the imported link identity");
        assert_eq!(link.target, external("https://example.com/two"));

        apply(&mut d, &mut ids, &update_inverse).unwrap();
        assert_eq!(d, created, "inverse restores the post-create inline tree");

        let remove_inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetHyperlink {
                range,
                id: n(4),
                target: None,
                tooltip: None,
            },
        )
        .unwrap();
        d.validate().unwrap();
        assert!(
            find_paragraph(d.body(), p)
                .unwrap()
                .inlines
                .iter()
                .all(|inline| !matches!(inline, InlineNode::Hyperlink(_)))
        );
        apply(&mut d, &mut ids, &remove_inverse).unwrap();
        assert!(matches!(
            find_paragraph(d.body(), p).unwrap().inlines[1],
            InlineNode::Hyperlink(_)
        ));

        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(d, original);
    }

    #[test]
    fn set_hyperlink_rejects_partial_existing_wrapper_without_mutation() {
        let p = n(2);
        let linked = InlineNode::Hyperlink(Hyperlink {
            id: n(4),
            target: external("https://example.com"),
            tooltip: None,
            inlines: vec![run(5, "linked")],
        });
        let mut d = doc(vec![para(2, vec![run(3, "A "), linked])]);
        let before = d.clone();
        let mut ids = IdGenerator::new(9);
        let result = apply(
            &mut d,
            &mut ids,
            &Operation::SetHyperlink {
                range: Range {
                    start: Pos::new(p, 3),
                    end: Pos::new(p, 8),
                },
                id: n(6),
                target: Some(external("https://other.example")),
                tooltip: None,
            },
        );
        assert_eq!(result, Err(EditError::Unsupported));
        assert_eq!(d, before);
    }

    #[test]
    fn delete_within_run_and_inverse_reinserts() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "Hello world")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 5),
                    end: Pos::new(p, 11),
                },
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "Hello");
        assert_eq!(
            inverse,
            Operation::InsertText {
                at: Pos::new(p, 5),
                text: " world".to_string(),
            }
        );
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(text_of(&d, p), "Hello world");
    }

    /// The runs (text, bold flag) of paragraph `id`, for formatting assertions.
    fn runs_of(document: &Document, id: NodeId) -> Vec<(String, Option<bool>)> {
        document
            .body()
            .iter()
            .find_map(|b| match b {
                BlockNode::Paragraph(p) if p.id == id => Some(
                    p.inlines
                        .iter()
                        .filter_map(|i| match i {
                            InlineNode::Run(r) => Some((r.text.clone(), r.properties.bold)),
                            _ => None,
                        })
                        .collect(),
                ),
                _ => None,
            })
            .unwrap_or_default()
    }

    #[test]
    fn format_splits_runs_and_undo_restores() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "HelloWorld")])]);
        let mut ids = IdGenerator::new(9);

        // Bold the first 5 bytes: the run splits, only "Hello" becomes bold.
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::FormatText {
                range: Range {
                    start: Pos::new(p, 0),
                    end: Pos::new(p, 5),
                },
                delta: FormatDelta {
                    bold: Some(true),
                    ..FormatDelta::default()
                },
            },
        )
        .unwrap();
        assert_eq!(
            runs_of(&d, p),
            vec![
                ("Hello".to_string(), Some(true)),
                ("World".to_string(), None),
            ]
        );

        // The inverse restores the original single, unformatted run.
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(runs_of(&d, p), vec![("HelloWorld".to_string(), None)]);
    }

    #[test]
    fn clear_formatting_restores_direct_defaults_and_undoes() {
        let p = n(2);
        let mut styled = run(3, "Styled text");
        if let InlineNode::Run(run) = &mut styled {
            run.properties.bold = Some(true);
            run.properties.italic = Some(true);
            run.properties.size_half_points = Some(28);
        }
        let original = styled.clone();
        let mut d = doc(vec![para(2, vec![styled])]);
        let mut ids = IdGenerator::new(9);
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::ClearFormatting {
                range: Range {
                    start: Pos::new(p, 0),
                    end: Pos::new(p, 11),
                },
            },
        )
        .unwrap();
        let BlockNode::Paragraph(paragraph) = &d.body()[0] else {
            panic!("expected paragraph");
        };
        let InlineNode::Run(cleared) = &paragraph.inlines[0] else {
            panic!("expected cleared run");
        };
        assert_eq!(cleared.properties, RunProperties::default());
        apply(&mut d, &mut ids, &inverse).unwrap();
        let BlockNode::Paragraph(paragraph) = &d.body()[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(paragraph.inlines[0], original);
    }

    #[test]
    fn split_and_join_are_inverses() {
        let p = n(2);
        let new = n(50);
        let mut d = doc(vec![para(2, vec![run(3, "HelloWorld")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 5),
                new_id: new,
            },
        )
        .unwrap();
        assert_eq!(d.body().len(), 2, "one paragraph became two");
        assert_eq!(text_of(&d, p), "Hello");
        assert_eq!(text_of(&d, new), "World");
        assert_eq!(
            inverse,
            Operation::JoinParagraphs {
                first: p,
                second: new
            }
        );

        // The inverse join restores a single paragraph with the joined text.
        apply(&mut d, &mut ids, &inverse).unwrap();
        assert_eq!(d.body().len(), 1);
        assert_eq!(text_of(&d, p), "HelloWorld");
    }

    #[test]
    fn split_at_start_leaves_an_empty_leading_paragraph() {
        let p = n(2);
        let new = n(50);
        let mut d = doc(vec![para(2, vec![run(3, "abc")])]);
        let mut ids = IdGenerator::new(9);
        apply(
            &mut d,
            &mut ids,
            &Operation::SplitParagraph {
                at: Pos::new(p, 0),
                new_id: new,
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "");
        assert_eq!(text_of(&d, new), "abc");
    }

    #[test]
    fn join_requires_the_second_to_be_adjacent() {
        let mut d = doc(vec![
            para(2, vec![run(3, "a")]),
            para(4, vec![run(5, "b")]),
            para(6, vec![run(7, "c")]),
        ]);
        let mut ids = IdGenerator::new(9);
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::JoinParagraphs {
                    first: n(2),
                    second: n(6), // not adjacent to 2
                }
            ),
            Err(EditError::Unsupported)
        );
        assert_eq!(d.body().len(), 3, "no mutation on error");
    }

    #[test]
    fn typing_into_an_empty_paragraph_creates_a_run() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![])]);
        let mut ids = IdGenerator::new(9);
        apply(
            &mut d,
            &mut ids,
            &Operation::InsertText {
                at: Pos::new(p, 0),
                text: "hi".to_string(),
            },
        )
        .unwrap();
        assert_eq!(text_of(&d, p), "hi");
    }

    #[test]
    fn out_of_range_and_missing_node_are_errors_and_do_not_mutate() {
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "abc")])]);
        let mut ids = IdGenerator::new(9);

        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertText {
                    at: Pos::new(p, 99),
                    text: "x".into()
                }
            ),
            Err(EditError::OffsetOutOfRange)
        );
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::InsertText {
                    at: Pos::new(n(404), 0),
                    text: "x".into()
                }
            ),
            Err(EditError::NodeNotFound)
        );
        // A cross-paragraph delete range is rejected.
        assert_eq!(
            apply(
                &mut d,
                &mut ids,
                &Operation::DeleteText {
                    range: Range {
                        start: Pos::new(p, 0),
                        end: Pos::new(n(3), 1)
                    }
                }
            ),
            Err(EditError::CrossParagraph)
        );
        assert_eq!(text_of(&d, p), "abc", "no mutation on error");
    }

    #[test]
    fn delete_across_runs_removes_range_and_undo_restores_formatting() {
        // A formatted paragraph: "Hello" (bold) + "World" (normal). Deleting a range
        // that spans both runs must work (this is what a multi-paragraph selection's
        // tail/head reduces to) and undo must bring back the bold run's formatting —
        // the reason the inverse is `SetInlines`, not a plain-text `InsertText`.
        let bold = RunProperties {
            bold: Some(true),
            ..RunProperties::default()
        };
        let p = n(2);
        let mut d = doc(vec![BlockNode::Paragraph(Paragraph {
            id: p,
            properties: ParagraphProperties::default(),
            inlines: vec![
                InlineNode::Run(Run {
                    id: n(3),
                    properties: bold,
                    text: "Hello".into(),
                }),
                run(4, "World"),
            ],
        })]);
        let mut ids = IdGenerator::new(9);

        // [3, 7) = "lo" (from bold "Hello") + "Wo" (from "World") → "Helrld".
        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 3),
                    end: Pos::new(p, 7),
                },
            },
        )
        .expect("multi-run delete succeeds");
        assert_eq!(text_of(&d, p), "Helrld");

        apply(&mut d, &mut ids, &inverse).expect("undo restores");
        assert_eq!(text_of(&d, p), "HelloWorld");
        let BlockNode::Paragraph(para) = &d.body()[0] else {
            panic!("paragraph");
        };
        let InlineNode::Run(first) = &para.inlines[0] else {
            panic!("run");
        };
        assert_eq!(
            first.properties.bold,
            Some(true),
            "undo restored the run's bold, not just its text"
        );
    }

    #[test]
    fn delete_whole_paragraph_text_leaves_it_empty() {
        // Deleting a paragraph's entire content (what a cross-paragraph selection does
        // to each whole middle paragraph before joining) empties its inlines cleanly.
        // (Single run — the model forbids adjacent equal-property runs, so "alphabeta"
        // is one run, not two.)
        let p = n(2);
        let mut d = doc(vec![para(2, vec![run(3, "alphabeta")])]);
        let mut ids = IdGenerator::new(9);
        apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 0),
                    end: Pos::new(p, 9),
                },
            },
        )
        .expect("full delete");
        assert_eq!(text_of(&d, p), "");
    }

    #[test]
    fn format_to_match_neighbour_coalesces_to_stay_valid() {
        // "abc"(bold) + "def"(normal): bolding [3,6) makes the second run match the
        // first — which the model forbids as two adjacent equal-property runs. The
        // format must coalesce them into one bold run, and the document stay valid.
        let bold = RunProperties {
            bold: Some(true),
            ..RunProperties::default()
        };
        let p = n(2);
        let mut d = doc(vec![BlockNode::Paragraph(Paragraph {
            id: p,
            properties: ParagraphProperties::default(),
            inlines: vec![
                InlineNode::Run(Run {
                    id: n(3),
                    properties: bold,
                    text: "abc".into(),
                }),
                run(4, "def"),
            ],
        })]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::FormatText {
                range: Range {
                    start: Pos::new(p, 3),
                    end: Pos::new(p, 6),
                },
                delta: FormatDelta {
                    bold: Some(true),
                    ..FormatDelta::default()
                },
            },
        )
        .expect("bold the second run");
        let BlockNode::Paragraph(para) = &d.body()[0] else {
            panic!("paragraph");
        };
        assert_eq!(para.inlines.len(), 1, "the two bold runs must coalesce");
        assert_eq!(text_of(&d, p), "abcdef");
        Document::new(
            n(1001),
            d.body().to_vec(),
            casual_doc_model::v1::Definitions::default(),
        )
        .expect("stays valid after formatting");

        // Undo restores the original two-run structure exactly.
        apply(&mut d, &mut ids, &inverse).expect("undo");
        let BlockNode::Paragraph(para) = &d.body()[0] else {
            panic!("paragraph");
        };
        assert_eq!(para.inlines.len(), 2, "undo restores the split");
    }

    #[test]
    fn delete_between_equal_runs_coalesces_to_stay_valid() {
        // "a"(normal) "BOLD"(bold) "c"(normal): deleting the whole bold middle would
        // leave the two normal runs adjacent — which the model forbids. The delete
        // must coalesce them into one run so the document stays re-validatable.
        let bold = RunProperties {
            bold: Some(true),
            ..RunProperties::default()
        };
        let p = n(2);
        let mut d = doc(vec![BlockNode::Paragraph(Paragraph {
            id: p,
            properties: ParagraphProperties::default(),
            inlines: vec![
                run(3, "a"),
                InlineNode::Run(Run {
                    id: n(4),
                    properties: bold,
                    text: "BOLD".into(),
                }),
                run(5, "c"),
            ],
        })]);
        let mut ids = IdGenerator::new(9);

        // [1, 5) = the whole bold run.
        apply(
            &mut d,
            &mut ids,
            &Operation::DeleteText {
                range: Range {
                    start: Pos::new(p, 1),
                    end: Pos::new(p, 5),
                },
            },
        )
        .expect("delete the middle run");
        assert_eq!(text_of(&d, p), "ac");

        let BlockNode::Paragraph(para) = &d.body()[0] else {
            panic!("paragraph");
        };
        assert_eq!(
            para.inlines.len(),
            1,
            "the two equal-property runs must coalesce into one"
        );
        // The whole document must still validate (the invariant we just protected).
        Document::new(
            n(1001),
            d.body().to_vec(),
            casual_doc_model::v1::Definitions::default(),
        )
        .expect("document stays valid after the delete");
    }

    #[test]
    fn set_core_properties_applies_and_inverse_restores() {
        let mut d = doc(vec![para(2, vec![run(3, "text")])]);
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetCoreProperties {
                properties: Box::new(CoreProperties {
                    title: Some("Quarterly Report".to_string()),
                    creator: Some("Ada Lovelace".to_string()),
                    ..CoreProperties::default()
                }),
            },
        )
        .expect("core properties install");
        assert_eq!(
            d.properties().unwrap().core.title.as_deref(),
            Some("Quarterly Report")
        );

        // Inverse restores the empty starting state.
        apply(&mut d, &mut ids, &inverse).expect("undo restores previous properties");
        assert!(d.properties().is_none_or(|p| p.core.is_empty()));
    }

    #[test]
    fn set_core_properties_rejects_an_oversized_field_and_leaves_doc_unchanged() {
        let mut d = doc(vec![para(2, vec![run(3, "text")])]);
        let mut ids = IdGenerator::new(9);
        let huge = "x".repeat(5_000); // over MAX_META_TEXT (4096)

        let err = apply(
            &mut d,
            &mut ids,
            &Operation::SetCoreProperties {
                properties: Box::new(CoreProperties {
                    title: Some(huge),
                    ..CoreProperties::default()
                }),
            },
        )
        .unwrap_err();
        assert_eq!(err, EditError::ValueTooLarge);
        // No partial mutation survives an error.
        assert!(d.properties().is_none_or(|p| p.core.is_empty()));
    }

    #[test]
    fn scoped_review_state_applies_inverts_and_rolls_back_atomically() {
        let paragraph = n(2);
        let mut d = doc(vec![
            para(2, vec![run(3, "before")]),
            para(4, vec![run(5, "untouched")]),
        ]);
        let original = d.clone();
        let mut ids = IdGenerator::new(9);

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::UpdateReviewState {
                paragraphs: vec![ReviewParagraphState {
                    node: paragraph,
                    inlines: vec![run(6, "after")],
                }],
                comments: None,
            },
        )
        .expect("scoped review update");
        assert_eq!(text_of(&d, paragraph), "after");
        assert_eq!(text_of(&d, n(4)), "untouched");
        assert!(matches!(
            &inverse,
            Operation::UpdateReviewState {
                paragraphs,
                comments: None,
            } if paragraphs.len() == 1 && paragraphs[0].node == paragraph
        ));

        apply(&mut d, &mut ids, &inverse).expect("exact review undo");
        assert_eq!(d, original);

        let invalid = Operation::UpdateReviewState {
            paragraphs: vec![ReviewParagraphState {
                node: paragraph,
                // Adjacent runs with equal properties violate the normalized
                // model invariant and must roll the entire operation back.
                inlines: vec![run(10, "a"), run(11, "b")],
            }],
            comments: None,
        };
        assert_eq!(
            apply(&mut d, &mut ids, &invalid),
            Err(EditError::ValueTooLarge)
        );
        assert_eq!(d, original, "failed review update leaves no partial state");
    }

    fn section(id: u64) -> SectionBoundary {
        SectionBoundary {
            id: SectionId::new(n(id)),
            page_size: PageSize {
                width_twips: 12_240,
                height_twips: 15_840,
            },
            page_margins: PageMargins {
                top_twips: 1_440,
                bottom_twips: 1_440,
                start_twips: 1_440,
                end_twips: 1_440,
                header_twips: None,
                footer_twips: None,
            },
            columns: SectionColumns {
                count: 1,
                space_twips: None,
                separator: None,
                equal_width: None,
                columns: Vec::new(),
            },
            headers: Vec::new(),
            footers: Vec::new(),
            section_type: None,
            title_page: None,
            vertical_alignment: None,
            page_numbering: PageNumbering::default(),
            doc_grid: DocGrid::default(),
            orientation: None,
            paper_source: PaperSource::default(),
            page_borders: PageBorders::default(),
            line_numbering: LineNumbering::default(),
            footnote_props: NoteProperties::default(),
            endnote_props: NoteProperties::default(),
            text_direction: None,
            bidi: false,
        }
    }

    fn doc_with_section(paragraphs: Vec<BlockNode>, section_id: u64) -> Document {
        let mut definitions = Definitions::default();
        definitions.sections.push(section(section_id));
        Document::new(n(1000), paragraphs, definitions).expect("valid document")
    }

    #[test]
    fn set_section_geometry_applies_and_inverse_restores() {
        let mut d = doc_with_section(vec![para(2, vec![run(3, "text")])], 500);
        let mut ids = IdGenerator::new(9);
        let sid = SectionId::new(n(500));
        let previous_columns = d.definitions().sections[0].columns.clone();
        let columns = SectionColumns {
            count: 2,
            space_twips: Some(360),
            separator: Some(true),
            equal_width: Some(true),
            columns: Vec::new(),
        };

        let inverse = apply(
            &mut d,
            &mut ids,
            &Operation::SetSectionGeometry {
                section: sid,
                page_size: PageSize {
                    width_twips: 15_840,
                    height_twips: 12_240,
                },
                page_margins: PageMargins {
                    top_twips: 720,
                    bottom_twips: 720,
                    start_twips: 720,
                    end_twips: 720,
                    header_twips: None,
                    footer_twips: None,
                },
                orientation: Some(PageOrientation::Landscape),
                columns,
            },
        )
        .expect("section geometry install");

        let installed = &d.definitions().sections[0];
        assert_eq!(installed.page_size.width_twips, 15_840);
        assert_eq!(installed.page_margins.top_twips, 720);
        assert_eq!(installed.orientation, Some(PageOrientation::Landscape));
        assert_eq!(installed.columns.count, 2);
        assert_eq!(installed.columns.space_twips, Some(360));
        assert_eq!(installed.columns.separator, Some(true));

        apply(&mut d, &mut ids, &inverse).expect("undo restores previous geometry");
        let restored = &d.definitions().sections[0];
        assert_eq!(restored.page_size.width_twips, 12_240);
        assert_eq!(restored.page_margins.top_twips, 1_440);
        assert_eq!(restored.orientation, None);
        assert_eq!(restored.columns, previous_columns);
    }

    #[test]
    fn set_section_geometry_rejects_an_oversized_page_and_leaves_doc_unchanged() {
        let mut d = doc_with_section(vec![para(2, vec![run(3, "text")])], 500);
        let mut ids = IdGenerator::new(9);
        let sid = SectionId::new(n(500));
        let columns = d.definitions().sections[0].columns.clone();

        let original_margins = d.definitions().sections[0].page_margins;
        let err = apply(
            &mut d,
            &mut ids,
            &Operation::SetSectionGeometry {
                section: sid,
                page_size: PageSize {
                    width_twips: 999_999, // over the ~22in (31_680 twip) domain bound
                    height_twips: 15_840,
                },
                page_margins: original_margins,
                orientation: None,
                columns,
            },
        )
        .unwrap_err();
        assert_eq!(err, EditError::ValueTooLarge);
        assert_eq!(d.definitions().sections[0].page_size.width_twips, 12_240);
    }

    #[test]
    fn set_section_geometry_rejects_an_unknown_section() {
        let mut d = doc_with_section(vec![para(2, vec![run(3, "text")])], 500);
        let mut ids = IdGenerator::new(9);
        let original = d.definitions().sections[0].clone();

        let err = apply(
            &mut d,
            &mut ids,
            &Operation::SetSectionGeometry {
                section: SectionId::new(n(999)),
                page_size: original.page_size,
                page_margins: original.page_margins,
                orientation: None,
                columns: original.columns.clone(),
            },
        )
        .unwrap_err();
        assert_eq!(err, EditError::NodeNotFound);
    }

    /// REVIEW-GAP-030: the toolbar-reflection queries must descend into a
    /// pending tracked revision so selecting (or resting a caret in) suggested
    /// text reflects its real run formatting, not the paragraph default. Before
    /// the fix `run_properties_in_range`/`caret_run_properties` matched only a
    /// top-level `InlineNode::Run` and silently skipped the wrapped run.
    #[test]
    fn reflection_sees_formatting_inside_a_pending_revision() {
        // "Hi " (0..3) + a pending bold insertion "bold" (3..7) + " tail" (7..12).
        let bold_run = InlineNode::Run(Run {
            id: n(20),
            properties: RunProperties {
                bold: Some(true),
                ..RunProperties::default()
            },
            text: "bold".to_string(),
        });
        let insertion = InlineNode::Revision(Revision {
            id: n(21),
            kind: RevisionKind::Insertion,
            author: Some("Reviewer".to_owned()),
            date: None,
            revision_id: Some("21".to_owned()),
            editor_group: None,
            inlines: vec![bold_run],
        });
        let p = n(10);
        let document = doc(vec![para(
            10,
            vec![run(19, "Hi "), insertion, run(22, " tail")],
        )]);

        // A selection wholly inside the pending insertion.
        let inside = Range {
            start: Pos::new(p, 3),
            end: Pos::new(p, 7),
        };
        let covered = run_properties_in_range(&document, inside);
        assert_eq!(covered.len(), 1, "the wrapped run is now covered");
        assert_eq!(covered[0].bold, Some(true));
        assert!(
            format_state(&document, inside).bold,
            "the toolbar reflects bold for a selection inside a suggestion"
        );

        // A caret resting inside the pending insertion reflects it too.
        assert!(
            caret_format(&document, p, 5).bold,
            "the toolbar reflects bold at a caret inside a suggestion"
        );

        // A selection spanning plain + pending + plain text sees all three runs,
        // and is correctly reported as mixed (not uniformly bold).
        let whole = Range {
            start: Pos::new(p, 0),
            end: Pos::new(p, 12),
        };
        let across = run_properties_in_range(&document, whole);
        assert_eq!(across.len(), 3, "top-level and wrapped runs both covered");
        assert_eq!(
            across
                .iter()
                .filter(|props| props.bold == Some(true))
                .count(),
            1
        );
        assert!(
            !format_state(&document, whole).bold,
            "a mixed selection is not reported as uniformly bold"
        );

        // A rejected/zero-width deletion still contributes nothing to the
        // projected offsets, so plain-text reflection is unchanged.
        let plain = Range {
            start: Pos::new(p, 7),
            end: Pos::new(p, 12),
        };
        assert!(!format_state(&document, plain).bold);
    }
}
