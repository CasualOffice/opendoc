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
//! **This is slice 1: text `InsertText` + `DeleteText` on top-level runs.**
//! `SplitParagraph`/`JoinParagraphs`, nested-wrapper edits, and object/table ops
//! are additive follow-ups (doc 59 staging).

use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, Color, Document, FontName, FontRef, GridColumn, HighlightColor, InlineNode,
    Paragraph, ParagraphProperties, RgbColor, Run, RunProperties, Table, TableCell, TableRow,
    VerticalAlignment,
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
    let Some(para) = find_paragraph(document.body(), range.start.node) else {
        return FormatState::default();
    };
    let covered: Vec<&RunProperties> = run_segments(&para.inlines)
        .into_iter()
        .filter(|s| s.end > range.start.offset && s.start < range.end.offset && s.start < s.end)
        .filter_map(|s| match &para.inlines[s.idx] {
            InlineNode::Run(run) => Some(&run.properties),
            _ => None,
        })
        .collect();
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
    let props = caret_run_props(document, node, offset).unwrap_or_default();
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
    let Some(props) = caret_run_props(document, node, offset) else {
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
fn caret_run_props(document: &Document, node: NodeId, offset: u32) -> Option<RunProperties> {
    let para = find_paragraph(document.body(), node)?;
    let segs = run_segments(&para.inlines);
    let seg = segs
        .iter()
        .find(|s| offset > s.start && offset <= s.end)
        .or_else(|| segs.iter().find(|s| offset >= s.start && offset < s.end))
        .or_else(|| segs.first())?;
    match &para.inlines[seg.idx] {
        InlineNode::Run(run) => Some(run.properties.clone()),
        _ => None,
    }
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
    let Some(para) = find_paragraph(document.body(), range.start.node) else {
        return RunStyleState::default();
    };
    let covered: Vec<&RunProperties> = run_segments(&para.inlines)
        .into_iter()
        .filter(|s| s.end > range.start.offset && s.start < range.end.offset && s.start < s.end)
        .filter_map(|s| match &para.inlines[s.idx] {
            InlineNode::Run(run) => Some(&run.properties),
            _ => None,
        })
        .collect();
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
fn find_paragraph(blocks: &[BlockNode], id: NodeId) -> Option<&Paragraph> {
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
        InlineNode::Revision(revision) => nested_len(&revision.inlines),
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
    // Offset sits in a non-run gap: append to the nearest preceding run…
    if let Some(seg) = segs.iter().rev().find(|s| s.end <= offset)
        && let InlineNode::Run(run) = &mut inlines[seg.idx]
    {
        run.text.push_str(text);
        return Ok(());
    }
    // …else prepend to the nearest following run…
    if let Some(seg) = segs.iter().find(|s| s.start >= offset)
        && let InlineNode::Run(run) = &mut inlines[seg.idx]
    {
        run.text.insert_str(0, text);
        return Ok(());
    }
    // …else the paragraph has no runs: create one at the front.
    let id = ids.next().ok_or(EditError::IdExhausted)?;
    inlines.insert(
        0,
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
    use casual_doc_model::v1::{Definitions, ParagraphProperties};

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
}
