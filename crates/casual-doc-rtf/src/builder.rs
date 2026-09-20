//! Block, paragraph, run, and table accumulation.
//!
//! This module owns the model invariants an importer is easiest to get wrong:
//! a run is never empty, two adjacent runs never carry equal properties, and a
//! table never has an empty row or an empty cell. Keeping them here means the
//! control-word dispatcher can push text without reasoning about any of it.

use casual_doc_model::v1::BlockNode;
use casual_doc_model::v1::GridColumn;
use casual_doc_model::v1::InlineNode;
use casual_doc_model::v1::Paragraph;
use casual_doc_model::v1::ParagraphProperties;
use casual_doc_model::v1::Run;
use casual_doc_model::v1::RunProperties;
use casual_doc_model::v1::Table;
use casual_doc_model::v1::TableCell;
use casual_doc_model::v1::TableCellProperties;
use casual_doc_model::v1::TableProperties;
use casual_doc_model::v1::TableRow;
use casual_doc_model::v1::TableRowProperties;
use casual_doc_model::v1::TableWidth;
use casual_doc_model::{IdGenerator, NodeId};

use crate::limits::enforce;
use crate::{RtfError, RtfLimits};

/// One `\cellx`-declared cell boundary and the properties written before it.
#[derive(Clone, Debug, Default)]
pub(crate) struct CellDefinition {
    /// Right boundary in twips, measured from the row's left edge.
    pub(crate) right_boundary_twips: i32,
    /// Cell properties written before this `\cellx`.
    pub(crate) properties: TableCellProperties,
    /// `\clmrg` — this cell continues the previous cell's horizontal merge.
    pub(crate) merge_continue: bool,
}

/// Accumulates body content, with a table sub-state for `\cell`/`\row`.
#[derive(Debug, Default)]
pub(crate) struct BodyBuilder {
    blocks: Vec<BlockNode>,
    inlines: Vec<InlineNode>,
    pending_text: String,
    pending_properties: RunProperties,
    cell_blocks: Vec<BlockNode>,
    row_cells: Vec<TableCell>,
    row_widths: Vec<i32>,
    rows: Vec<TableRow>,
    table_grid: Vec<GridColumn>,
    paragraph_count: usize,
    inline_count: usize,
    cell_count: usize,
    row_count: usize,
}

impl BodyBuilder {
    /// Appends decoded text under the given run properties.
    ///
    /// Text is buffered rather than emitted per call: a `\'hh` escape produces
    /// one character, and emitting one `Run` per character would both explode
    /// the node count and violate the adjacent-equal-runs invariant on every
    /// second character.
    pub(crate) fn push_text(
        &mut self,
        text: &str,
        properties: &RunProperties,
        ids: &mut IdGenerator,
        limits: RtfLimits,
    ) -> Result<(), RtfError> {
        if text.is_empty() {
            return Ok(());
        }
        if !self.pending_text.is_empty() && &self.pending_properties != properties {
            self.flush_run(ids, limits)?;
        }
        if self.pending_text.is_empty() {
            self.pending_properties = properties.clone();
        }
        self.pending_text.push_str(text);
        Ok(())
    }

    /// Appends a non-run inline, flushing any buffered text first.
    pub(crate) fn push_inline(
        &mut self,
        inline: InlineNode,
        ids: &mut IdGenerator,
        limits: RtfLimits,
    ) -> Result<(), RtfError> {
        self.flush_run(ids, limits)?;
        self.charge_inline(limits)?;
        self.inlines.push(inline);
        Ok(())
    }

    /// Ends the current paragraph.
    pub(crate) fn end_paragraph(
        &mut self,
        properties: ParagraphProperties,
        ids: &mut IdGenerator,
        limits: RtfLimits,
    ) -> Result<NodeId, RtfError> {
        self.flush_run(ids, limits)?;
        self.paragraph_count = self.paragraph_count.saturating_add(1);
        enforce(
            "rtf_paragraphs",
            self.paragraph_count,
            limits.max_paragraphs,
        )?;
        let id = next_id(ids)?;
        let paragraph = BlockNode::Paragraph(Paragraph {
            id,
            properties,
            inlines: std::mem::take(&mut self.inlines),
        });
        if self.in_cell() {
            self.cell_blocks.push(paragraph);
        } else {
            self.close_open_table(ids, limits)?;
            self.blocks.push(paragraph);
        }
        Ok(id)
    }

    /// Ends a table cell (`\cell`).
    pub(crate) fn end_cell(
        &mut self,
        definition: &CellDefinition,
        previous_boundary_twips: i32,
        ids: &mut IdGenerator,
        limits: RtfLimits,
    ) -> Result<(), RtfError> {
        self.cell_count = self.cell_count.saturating_add(1);
        enforce("rtf_table_cells", self.cell_count, limits.max_table_cells)?;
        // A cell whose content is still buffered ends its paragraph here; the
        // model refuses an empty cell, so an genuinely empty one still gets one
        // empty paragraph.
        self.flush_run(ids, limits)?;
        if !self.inlines.is_empty() || self.cell_blocks.is_empty() {
            self.paragraph_count = self.paragraph_count.saturating_add(1);
            enforce(
                "rtf_paragraphs",
                self.paragraph_count,
                limits.max_paragraphs,
            )?;
            self.cell_blocks.push(BlockNode::Paragraph(Paragraph {
                id: next_id(ids)?,
                properties: ParagraphProperties::default(),
                inlines: std::mem::take(&mut self.inlines),
            }));
        }
        let width = definition
            .right_boundary_twips
            .saturating_sub(previous_boundary_twips)
            .clamp(0, 31_680);
        let mut properties = definition.properties.clone();
        if properties.width.is_none() && width > 0 {
            properties.width = Some(TableWidth::dxa(width));
        }
        if definition.merge_continue {
            // A `\clmrg` cell is not its own cell in the normalized model: its
            // grid column is folded into the cell that started the merge, the
            // way WordprocessingML's `w:gridSpan` expresses it. Its content is
            // appended so no text is lost by the fold.
            if let Some(previous) = self.row_cells.last_mut() {
                let span = previous.properties.grid_span.unwrap_or(1).saturating_add(1);
                previous.properties.grid_span = Some(span.min(16_384));
                if let Some(TableWidth { value, .. }) = previous.properties.width.as_mut() {
                    *value = value.saturating_add(width).min(31_680);
                }
                previous.blocks.append(&mut self.cell_blocks);
                if let Some(last) = self.row_widths.last_mut() {
                    *last = last.saturating_add(width).min(31_680);
                }
                self.cell_blocks.clear();
                return Ok(());
            }
            // A `\clmrg` with nothing to merge into is malformed; treat it as an
            // ordinary cell rather than dropping its content.
        }
        self.row_cells.push(TableCell {
            id: next_id(ids)?,
            properties,
            blocks: std::mem::take(&mut self.cell_blocks),
        });
        self.row_widths.push(width);
        Ok(())
    }

    /// Ends a table row (`\row`). A row with no cells is dropped by the caller.
    pub(crate) fn end_row(
        &mut self,
        properties: TableRowProperties,
        ids: &mut IdGenerator,
        limits: RtfLimits,
    ) -> Result<bool, RtfError> {
        if self.row_cells.is_empty() {
            self.cell_blocks.clear();
            self.row_widths.clear();
            return Ok(false);
        }
        self.row_count = self.row_count.saturating_add(1);
        enforce("rtf_table_rows", self.row_count, limits.max_table_rows)?;
        if self.table_grid.is_empty() {
            self.table_grid = self
                .row_widths
                .iter()
                .map(|width| GridColumn {
                    width_twips: Some((*width).clamp(0, 31_680)),
                })
                .collect();
        }
        self.row_widths.clear();
        self.rows.push(TableRow {
            id: next_id(ids)?,
            properties,
            cells: std::mem::take(&mut self.row_cells),
        });
        Ok(true)
    }

    /// Whether content is currently being routed into a table cell.
    pub(crate) fn in_cell(&self) -> bool {
        !self.cell_blocks.is_empty() || !self.row_cells.is_empty()
    }

    /// Emits any open table, then returns the finished body.
    ///
    /// The model refuses an empty body, so a document whose RTF produced no
    /// block at all (an empty `{\rtf1}`) gets one empty paragraph rather than
    /// failing: an empty document is a legitimate thing to open.
    pub(crate) fn finish(
        mut self,
        trailing: ParagraphProperties,
        ids: &mut IdGenerator,
        limits: RtfLimits,
    ) -> Result<Vec<BlockNode>, RtfError> {
        self.flush_run(ids, limits)?;
        if !self.inlines.is_empty() {
            self.end_paragraph(trailing, ids, limits)?;
        }
        if self.in_cell() {
            // A stream that ends mid-row still has content in the open cell.
            // Close it into a row so the text reaches the document.
            self.row_cells.push(TableCell {
                id: next_id(ids)?,
                properties: TableCellProperties::default(),
                blocks: if self.cell_blocks.is_empty() {
                    vec![BlockNode::Paragraph(Paragraph {
                        id: next_id(ids)?,
                        properties: ParagraphProperties::default(),
                        inlines: Vec::new(),
                    })]
                } else {
                    std::mem::take(&mut self.cell_blocks)
                },
            });
            self.end_row(TableRowProperties::default(), ids, limits)?;
        }
        self.close_open_table(ids, limits)?;
        if self.blocks.is_empty() {
            self.blocks.push(BlockNode::Paragraph(Paragraph {
                id: next_id(ids)?,
                properties: ParagraphProperties::default(),
                inlines: Vec::new(),
            }));
        }
        Ok(self.blocks)
    }

    /// Replaces the properties of the most recently emitted body paragraph.
    ///
    /// `\sect` needs to stamp the section break onto the paragraph it just
    /// closed, which is the only backward edit the builder allows.
    pub(crate) fn amend_last_paragraph(
        &mut self,
        amend: impl FnOnce(&mut ParagraphProperties),
    ) -> bool {
        let target = if self.in_cell() {
            self.cell_blocks.last_mut()
        } else {
            self.blocks.last_mut()
        };
        match target {
            Some(BlockNode::Paragraph(paragraph)) => {
                amend(&mut paragraph.properties);
                true
            }
            _ => false,
        }
    }

    fn close_open_table(
        &mut self,
        ids: &mut IdGenerator,
        limits: RtfLimits,
    ) -> Result<(), RtfError> {
        if self.rows.is_empty() {
            return Ok(());
        }
        let _ = limits;
        self.blocks.push(BlockNode::Table(Table {
            id: next_id(ids)?,
            grid: std::mem::take(&mut self.table_grid),
            grid_change: None,
            properties: TableProperties::default(),
            rows: std::mem::take(&mut self.rows),
        }));
        Ok(())
    }

    fn flush_run(&mut self, ids: &mut IdGenerator, limits: RtfLimits) -> Result<(), RtfError> {
        if self.pending_text.is_empty() {
            return Ok(());
        }
        let text = std::mem::take(&mut self.pending_text);
        // Merge into the previous run when the properties match. Without this
        // the document is *invalid*, not merely untidy: the model rejects two
        // adjacent runs carrying equal properties, and the sequence
        // "text, format-on, format-off, text" produces exactly that pair.
        if let Some(InlineNode::Run(previous)) = self.inlines.last_mut()
            && previous.properties == self.pending_properties
        {
            previous.text.push_str(&text);
            return Ok(());
        }
        self.charge_inline(limits)?;
        self.inlines.push(InlineNode::Run(Run {
            id: next_id(ids)?,
            properties: self.pending_properties.clone(),
            text,
        }));
        Ok(())
    }

    fn charge_inline(&mut self, limits: RtfLimits) -> Result<(), RtfError> {
        self.inline_count = self.inline_count.saturating_add(1);
        enforce(
            "rtf_inline_nodes",
            self.inline_count,
            limits.max_inline_nodes,
        )
    }
}

pub(crate) fn next_id(ids: &mut IdGenerator) -> Result<NodeId, RtfError> {
    ids.next_id().map_err(|error| RtfError::Model {
        reason: error.to_string(),
    })
}
