// SPDX-License-Identifier: Apache-2.0

//! A slide table (`a:tbl`), its grid, its cells, and the deck's
//! `tableStyles.xml`.
//!
//! # The one thing this type exists to get right: merges
//!
//! PowerPoint writes a merged region **twice over**. The top-left cell — the
//! origin — carries `a:tc@gridSpan` and `a:tc@rowSpan`, the counts of grid
//! columns and rows it covers. Every cell the origin covers is *still written*,
//! as a placeholder carrying `a:tc@hMerge="1"` or `a:tc@vMerge="1"`, because a
//! row must hold exactly one `a:tc` per `a:gridCol` for the row to be
//! well-formed. So the file states one fact — "this 2x3 region is one cell" — in
//! two encodings, on different cells, and a reader that treats `@rowSpan` and
//! `@vMerge` as the same thing paints the origin's text once per covered row.
//!
//! The established representation for this is the **anchor/covered** pair that
//! HTML's `colspan`/`rowspan` and OpenDocument's `table:covered-table-cell` are
//! two halves of: one cell owns the region, the rest are covered and own
//! nothing. So the model carries ONE enum per axis, [`CellMerge`], whose
//! variants are exactly those three states. The two encodings cannot be
//! conflated because they are not two fields: `@gridSpan` reads as
//! [`CellMerge::Origin`] and `@hMerge` as [`CellMerge::Continuation`], and one
//! value cannot be both.
//!
//! ## Why not `CellMergeAnnotation`
//!
//! `casual_doc_model::v1::CellMergeAnnotation` was the obvious candidate and it
//! does not fit. It is `ST_AnnotationVMerge` — the `cont`/`rest` pair that
//! `w:tcPr/w:cellMerge` writes to record a **tracked change** to a cell's
//! vertical-merge role — and it lives on `v1::CellMergeRevision` beside an
//! author, a date and a revision id. Its two values are "the annotation after"
//! and "the annotation before" a revision, not "origin" and "covered".
//!
//! What the DOCX model uses for the merge itself is
//! `v1::TableCellProperties::grid_span: Option<u32>` plus
//! `vertical_merge: Option<VerticalMerge>` (`Restart`/`Continue`) — the same
//! anchor/covered idea, split across two differently-shaped fields because
//! WordprocessingML states a horizontal span and *no* horizontal continuation
//! marker. PresentationML states a marker on both axes, so one symmetrical enum
//! used twice is the smaller model, and [`CellMerge`] is that enum. The DOCX
//! cell-properties type is not reused directly because it is a `w:tcPr`
//! projection: it carries `w:cnfStyle`, `w:tcW` with its unit enum, `w:shd`'s
//! pattern and `w:textDirection`, none of which an `a:tcPr` has — and none of
//! `a:tcPr`'s own four EMU margins, `@anchor` or `a:lnL`..`a:lnB` as full
//! `a:CT_LineProperties` is expressible in it.
//!
//! # Units
//!
//! `a:gridCol@w`, `a:tr@h` and the four `a:tcPr` margins are **EMU**, as is
//! `a:lnL@w`. Nothing here is in twips, points or half-points; the conversion
//! happens once, in `casual-pres-layout`.

use std::collections::BTreeSet;

use casual_doc_model::NodeId;
use casual_doc_model::v1::{Fill, ShapeStroke};
use serde::{Deserialize, Serialize};

use crate::{PresentationError, TableAxis, TextAnchor, TextBody};

/// The widest grid a table may declare.
///
/// `16384` rather than a presentation-specific number, so the whole engine has
/// one answer: it is the ceiling `w:gridSpan` already carries in
/// `casual_doc_model::v1::TableCellProperties`, and a cell cannot span more
/// columns than the grid has. PowerPoint's own UI stops at 75 columns, so this is
/// a structural bound rather than a usability one.
pub const MAX_TABLE_GRID_COLUMNS: usize = 16_384;

/// The most rows a table may declare.
///
/// Matches [`MAX_TABLE_GRID_COLUMNS`] because the product is what costs memory,
/// and a 16384-row table is already far past anything a slide can show.
pub const MAX_TABLE_ROWS: usize = 16_384;

/// `a:tcPr@marL`/`@marR`'s schema default, in EMU (0.1 inch).
pub const DEFAULT_CELL_MARGIN_HORIZONTAL_EMU: i64 = 91_440;

/// `a:tcPr@marT`/`@marB`'s schema default, in EMU (0.05 inch).
pub const DEFAULT_CELL_MARGIN_VERTICAL_EMU: i64 = 45_720;

/// One cell's role in a merged region, on one axis.
///
/// See the module header: this is the single representation of a fact
/// PresentationML writes in two places, and the reason there is no second one.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CellMerge {
    /// The cell is not merged on this axis: it covers exactly one grid column or
    /// one row, and nothing covers it.
    #[default]
    None,
    /// The cell OWNS a region `span` units long on this axis, starting at itself
    /// (`a:tc@gridSpan` / `a:tc@rowSpan`). `span` is at least two — a span of one
    /// is [`CellMerge::None`], which is what PowerPoint writes.
    Origin(u32),
    /// The cell is COVERED by an origin earlier on this axis (`a:tc@hMerge` /
    /// `a:tc@vMerge`). It owns no box and paints no content; it exists so the row
    /// holds one `a:tc` per `a:gridCol`, and it is retained rather than dropped
    /// because a round trip must write it back.
    Continuation,
}

impl CellMerge {
    /// The number of grid units this cell occupies on its axis: the span for an
    /// origin, one for an unmerged cell, and zero for a continuation — which owns
    /// nothing.
    #[must_use]
    pub const fn units(self) -> u32 {
        match self {
            Self::None => 1,
            Self::Origin(span) => span,
            Self::Continuation => 0,
        }
    }

    /// Whether this cell is covered by another and therefore paints nothing.
    #[must_use]
    pub const fn is_continuation(self) -> bool {
        matches!(self, Self::Continuation)
    }
}

/// One `a:gridCol`: a grid column's authored width in EMU.
///
/// A struct rather than a bare `i64` so the unit is in the field's name and a
/// future `a:extLst` has somewhere to land.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableGridColumn {
    /// `a:gridCol@w`, in EMU.
    pub width_emu: i64,
}

/// `a:tblPr`: the banding flags, the reading direction and the style reference.
///
/// The six flags are the **conditional-formatting selectors** a table style's
/// `a:firstRow`/`a:lastRow`/`a:firstCol`/`a:lastCol`/`a:band1H`/`a:band1V` parts
/// are applied through. They are not formatting themselves, which is why they are
/// plain booleans here and why dropping one silently removes a header row's
/// emphasis without removing the header row.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableProperties {
    /// `@firstRow`: the first row takes the style's header formatting.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub first_row: bool,
    /// `@lastRow`: the last row takes the style's total-row formatting.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub last_row: bool,
    /// `@firstCol`: the first column takes the style's leading-column formatting.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub first_column: bool,
    /// `@lastCol`: the last column takes the style's trailing-column formatting.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub last_column: bool,
    /// `@bandRow`: alternate row banding.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub banded_rows: bool,
    /// `@bandCol`: alternate column banding.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub banded_columns: bool,
    /// `@rtl`: the grid runs right to left, so grid column zero is the rightmost.
    #[serde(default, skip_serializing_if = "core::ops::Not::not")]
    pub rtl: bool,
    /// `a:tblPr/a:tableStyleId`: the style's GUID, **braces included**.
    ///
    /// `{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}` is one token and the braces are
    /// part of it: `tableStyles.xml` writes `a:tblStyle@styleId` the same way, so
    /// stripping them breaks the join and the table silently loses its style.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style_id: Option<String>,
}

impl TableProperties {
    /// Whether no property is set (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// `a:tcPr`: a cell's margins, its vertical anchor, its fill and its four border
/// lines.
///
/// The borders are `casual_doc_model::v1::ShapeStroke` and the fill is
/// `v1::Fill`, unchanged — `a:lnL` **is** an `a:CT_LineProperties`, the same
/// element `a:ln` on a `p:spPr` is, and a cell's `a:solidFill` is the same element
/// a shape's is. Declaring a cell-only border type would fork the one stroke
/// vocabulary the engine paints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCellProperties {
    /// `@marL`, in EMU.
    pub margin_left_emu: i64,
    /// `@marR`, in EMU.
    pub margin_right_emu: i64,
    /// `@marT`, in EMU.
    pub margin_top_emu: i64,
    /// `@marB`, in EMU.
    pub margin_bottom_emu: i64,
    /// `@anchor`: where the cell's content sits in a taller cell box.
    pub anchor: TextAnchor,
    /// The cell's background (`a:tcPr/a:solidFill`), when it states one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Fill>,
    /// `a:lnL`, the leading edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_left: Option<ShapeStroke>,
    /// `a:lnR`, the trailing edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_right: Option<ShapeStroke>,
    /// `a:lnT`, the top edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_top: Option<ShapeStroke>,
    /// `a:lnB`, the bottom edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_bottom: Option<ShapeStroke>,
}

impl Default for TableCellProperties {
    /// DrawingML's own `a:tcPr` defaults, not zeros.
    ///
    /// `@marL`/`@marR` default to 91440 EMU and `@marT`/`@marB` to 45720 — a tenth
    /// and a twentieth of an inch — and a cell that states no margin is inset by
    /// them. Defaulting to zero would make every unmargined table's text touch its
    /// own borders, which looks like a layout bug rather than a missing attribute.
    fn default() -> Self {
        Self {
            margin_left_emu: DEFAULT_CELL_MARGIN_HORIZONTAL_EMU,
            margin_right_emu: DEFAULT_CELL_MARGIN_HORIZONTAL_EMU,
            margin_top_emu: DEFAULT_CELL_MARGIN_VERTICAL_EMU,
            margin_bottom_emu: DEFAULT_CELL_MARGIN_VERTICAL_EMU,
            anchor: TextAnchor::Top,
            fill: None,
            border_left: None,
            border_right: None,
            border_top: None,
            border_bottom: None,
        }
    }
}

/// One `a:tc`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableCell {
    /// Stable identity, minted by the reader: an `a:tc` carries no id of its own.
    pub id: NodeId,
    /// The cell's role in a horizontal merge (`@gridSpan` / `@hMerge`).
    #[serde(default, skip_serializing_if = "is_unmerged")]
    pub horizontal: CellMerge,
    /// The cell's role in a vertical merge (`@rowSpan` / `@vMerge`).
    #[serde(default, skip_serializing_if = "is_unmerged")]
    pub vertical: CellMerge,
    /// `a:tcPr`.
    #[serde(default, skip_serializing_if = "is_default_cell_properties")]
    pub properties: TableCellProperties,
    /// `a:tc/a:txBody`, read by the SAME `a:txBody` reader a shape's text goes
    /// through — there is one DrawingML text body in this engine, not two.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextBody>,
}

fn is_unmerged(merge: &CellMerge) -> bool {
    *merge == CellMerge::None
}

fn is_default_cell_properties(properties: &TableCellProperties) -> bool {
    *properties == TableCellProperties::default()
}

impl TableCell {
    /// An unmerged cell with DrawingML's default `a:tcPr` and no text.
    #[must_use]
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            horizontal: CellMerge::None,
            vertical: CellMerge::None,
            properties: TableCellProperties::default(),
            text: None,
        }
    }

    /// Attaches the cell's `a:txBody`.
    #[must_use]
    pub fn with_text(mut self, text: TextBody) -> Self {
        self.text = Some(text);
        self
    }

    /// Whether this cell is covered on either axis and so paints nothing.
    #[must_use]
    pub const fn is_covered(&self) -> bool {
        self.horizontal.is_continuation() || self.vertical.is_continuation()
    }
}

/// One `a:tr`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableRow {
    /// Stable identity, minted by the reader: an `a:tr` carries no id of its own.
    pub id: NodeId,
    /// `a:tr@h`, in EMU. It is a **minimum**, not a fixed height: PowerPoint grows
    /// a row whose content does not fit, exactly as `w:trHeight`'s `atLeast` rule
    /// does, and there is no `exact` spelling for an `a:tr`.
    pub height_emu: i64,
    /// One cell per grid column, in grid order, continuations included.
    pub cells: Vec<TableCell>,
}

/// A table on a slide (`a:tbl`), as reached through a `p:graphicFrame`.
///
/// The frame's own box is NOT here: it is the `GroupChild` on the
/// [`SlideNode`](crate::SlideNode) this table hangs off, so a frame is positioned
/// and sized by the same transform every other shape on the slide is, and the
/// shared placement walk needs no table arm.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlideTable {
    /// Stable identity for the `a:tbl` itself.
    pub id: NodeId,
    /// `a:tblPr`.
    #[serde(default, skip_serializing_if = "TableProperties::is_empty")]
    pub properties: TableProperties,
    /// `a:tblGrid`'s columns, in grid order.
    pub grid: Vec<TableGridColumn>,
    /// The rows, in top-to-bottom order.
    pub rows: Vec<TableRow>,
}

impl SlideTable {
    /// Visits this table's node ids: its own, each row's and each cell's, plus
    /// every id inside a cell's text.
    ///
    /// Carried through the same uniqueness check every other slide id goes
    /// through, because a cell id colliding with a shape id is the same defect as
    /// two shapes colliding.
    ///
    /// # Complexity
    ///
    /// O(cells + text nodes). One visit each, no lookups.
    pub fn visit_node_ids(&self, visit: &mut dyn FnMut(NodeId)) {
        visit(self.id);
        for row in &self.rows {
            visit(row.id);
            for cell in &row.cells {
                visit(cell.id);
                if let Some(text) = cell.text.as_ref() {
                    text.visit_node_ids(visit);
                }
            }
        }
    }

    /// The sum of the grid's column widths, in EMU — the table's authored width,
    /// which an `a:tbl` states nowhere else.
    ///
    /// # Complexity
    ///
    /// O(columns).
    #[must_use]
    pub fn width_emu(&self) -> i64 {
        self.grid
            .iter()
            .map(|column| column.width_emu)
            .fold(0_i64, i64::saturating_add)
    }

    /// Checks this table's invariants: a usable grid, one cell per grid column in
    /// every row, non-negative measures, and merges whose origins and
    /// continuations agree on both axes.
    ///
    /// The merge checks are the load-bearing ones. A `@gridSpan="3"` with only one
    /// following `@hMerge` is a file whose row is a column short, and a `@vMerge`
    /// in a column no `@rowSpan` reaches is a cell that paints nothing with
    /// nothing above it to paint instead — both render as a visibly broken table,
    /// and both are detectable here with integer arithmetic.
    ///
    /// # Errors
    ///
    /// See [`PresentationError`]'s table variants.
    ///
    /// # Complexity
    ///
    /// O(cells): one row-major pass and one column-major pass. Not for a
    /// keystroke.
    pub fn validate(&self) -> Result<(), PresentationError> {
        if self.grid.is_empty() {
            return Err(PresentationError::EmptyTableGrid(self.id));
        }
        if self.grid.len() > MAX_TABLE_GRID_COLUMNS {
            return Err(PresentationError::TableTooLarge {
                table: self.id,
                axis: TableAxis::Column,
                count: self.grid.len(),
            });
        }
        if self.rows.len() > MAX_TABLE_ROWS {
            return Err(PresentationError::TableTooLarge {
                table: self.id,
                axis: TableAxis::Row,
                count: self.rows.len(),
            });
        }
        for column in &self.grid {
            if column.width_emu < 0 {
                return Err(PresentationError::TableMeasureOutOfDomain(column.width_emu));
            }
        }
        for row in &self.rows {
            if row.height_emu < 0 {
                return Err(PresentationError::TableMeasureOutOfDomain(row.height_emu));
            }
            if row.cells.len() != self.grid.len() {
                return Err(PresentationError::TableRowWidthMismatch {
                    table: self.id,
                    row: row.id,
                    cells: row.cells.len(),
                    columns: self.grid.len(),
                });
            }
            validate_merge_run(
                row.cells.iter().map(|cell| (cell.id, cell.horizontal)),
                TableAxis::Column,
                row.id,
            )?;
            for cell in &row.cells {
                if let Some(text) = cell.text.as_ref() {
                    text.validate()?;
                }
            }
        }
        // Column-major, because a `@rowSpan` on row 0 is answered by `@vMerge` on
        // rows 1..n in the SAME column index — which a row-major walk cannot see.
        for column in 0..self.grid.len() {
            validate_merge_run(
                self.rows
                    .iter()
                    .filter_map(|row| row.cells.get(column))
                    .map(|cell| (cell.id, cell.vertical)),
                TableAxis::Row,
                self.id,
            )?;
        }
        Ok(())
    }
}

/// Checks that one line of cells — a row's cells left to right, or a column's
/// cells top to bottom — has origins and continuations that tile it exactly.
///
/// ONE function for both axes rather than one per axis. The rule is identical and
/// two copies of it is how a reader ends up validating rows and not columns:
/// `owed` is the number of continuations the last origin still expects, so a
/// continuation with `owed == 0` is unanchored, an origin or a plain cell with
/// `owed != 0` overlaps the region above or to the left of it, and a non-zero
/// `owed` at the end is an origin claiming past the edge.
///
/// `fallback` is the id a failure past the last cell is charged to.
///
/// # Complexity
///
/// O(cells in the line), with one `u32` of state.
fn validate_merge_run(
    cells: impl Iterator<Item = (NodeId, CellMerge)>,
    axis: TableAxis,
    fallback: NodeId,
) -> Result<(), PresentationError> {
    let mut owed = 0_u32;
    let mut last = fallback;
    for (id, merge) in cells {
        last = id;
        match merge {
            CellMerge::Continuation => {
                if owed == 0 {
                    return Err(PresentationError::UnanchoredCellMerge { cell: id, axis });
                }
                owed -= 1;
            }
            CellMerge::Origin(span) => {
                if owed != 0 {
                    return Err(PresentationError::OverlappingCellMerge { cell: id, axis });
                }
                if span < 2 {
                    return Err(PresentationError::CellSpanOutOfDomain { cell: id, span });
                }
                owed = span - 1;
            }
            CellMerge::None => {
                if owed != 0 {
                    return Err(PresentationError::OverlappingCellMerge { cell: id, axis });
                }
            }
        }
    }
    if owed != 0 {
        return Err(PresentationError::OverlappingCellMerge { cell: last, axis });
    }
    Ok(())
}

/// One `a:tblStyle` entry in `tableStyles.xml`.
///
/// The entry's *formatting* — `a:wholeTbl`, `a:band1H`, `a:firstRow` and the rest
/// — is deliberately not modelled: each is a `CT_TablePartStyle` of cell fills,
/// borders and text properties, and there is nothing yet that would apply one.
/// The id and the name ARE modelled, because they are the join and the
/// author-visible label, and because a table whose style GUID resolves to nothing
/// cannot otherwise be told from one whose GUID resolves to an entry this build
/// cannot paint. `casual-pres-import` reports the unmodelled parts per entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableStyle {
    /// `a:tblStyle@styleId`: the GUID, braces included.
    pub id: String,
    /// `a:tblStyle@styleName`, the author-visible label ("Medium Style 2 - Accent 1").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The deck's `tableStyles.xml` part: the default style id and the entries.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TableStyles {
    /// `a:tblStyleLst@def`: the GUID a table that states no `a:tableStyleId`
    /// takes.
    ///
    /// A separate fact from the entry list, and a real part states a `@def` whose
    /// entry it does not carry — PowerPoint's `tableStyles.xml` for a deck with no
    /// inserted table is `<a:tblStyleLst def="{GUID}"/>` and nothing else. So this
    /// is deliberately NOT validated to resolve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_style_id: Option<String>,
    /// The entries, in part order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub styles: Vec<TableStyle>,
}

impl TableStyles {
    /// Whether the part carried nothing (serializes to nothing).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.default_style_id.is_none() && self.styles.is_empty()
    }

    /// The entry with this GUID, matched EXACTLY — braces and case included.
    ///
    /// `{5C22544A-…}` is one token. Trimming the braces, or comparing
    /// case-insensitively, would join a table to a style the file does not name.
    ///
    /// # Complexity
    ///
    /// O(styles) — a linear scan of a list a real part keeps in the low dozens.
    /// Deliberately not indexed: building a map per lookup would cost more.
    #[must_use]
    pub fn style(&self, id: &str) -> Option<&TableStyle> {
        self.styles.iter().find(|style| style.id == id)
    }

    /// Refuses two entries with the same GUID.
    ///
    /// A duplicate is a wrong ANSWER rather than a missing one: [`Self::style`]
    /// takes the first, so a second entry with the same id silently never
    /// applies.
    ///
    /// # Errors
    ///
    /// [`PresentationError::DuplicateTableStyleId`].
    ///
    /// # Complexity
    ///
    /// O(styles log styles).
    pub fn validate(&self) -> Result<(), PresentationError> {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for style in &self.styles {
            if !seen.insert(style.id.as_str()) {
                return Err(PresentationError::DuplicateTableStyleId(style.id.clone()));
            }
        }
        Ok(())
    }
}
