// SPDX-License-Identifier: Apache-2.0

//! Table tools beyond row and column structure, as lowerings the editor and
//! agents share (`docs/181` §4.2 step 1): the six table style options
//! (`w:tblLook`), converting text to a table and a table to text, splitting a
//! table, inserting and deleting cells with a shift, and cell text direction
//! (`docs/153` `table.banded-rows-columns`, `table.text-to-table`,
//! `table.table-to-text`, `table.split-table`, `table.insert-cells`,
//! `table.delete-cells`, `table.cell-text-direction`; `109` RM-26, RM-27,
//! RM-37).
//!
//! Every function is pure: it reads a table (or a run of paragraphs) and
//! returns operations. None of them introduces an operation: each is an
//! arrangement of `SetTableProperties`, `SetTableCellProperties`,
//! `ReplaceTable`, `InsertTable`, `DeleteTable`, `InsertBlocks` and
//! `DeleteBlocks`, the vocabulary a concurrent edit already transforms against.
//!
//! # The rules, and whose they are
//!
//! | Gesture | Rule |
//! | --- | --- |
//! | Style options | One `w:tblLook` bit per toggle. "Banded rows/columns" are stored inverted (`w:noHBand`/`w:noVBand`), as the file format stores them. |
//! | Text to table | Word's and ONLYOFFICE's dialog (`TextToTableDialog.js:392-406`): separate at paragraphs, tabs, commas, semicolons or a typed character; a column count no smaller than the widest row; fixed widths, autofit to contents, or autofit to window. Each source paragraph's formatting goes with its text, and the runs keep theirs. |
//! | Table to text | `TableToTextDialog.js:164-203`: one paragraph per row with the cells joined by the separator, or every cell's paragraphs in order for "paragraph marks"; nested tables are converted too only when asked, and stay tables otherwise. |
//! | Split table | Word's Split Table and ONLYOFFICE's table break (`sdkjs/word/Editor/Shortcuts.js:188`, Ctrl+Shift+Enter): the caret's row starts a second table after an empty paragraph. In the first row Word inserts the paragraph above the table instead, and so does this. |
//! | Insert cells | Word's dialog. Shift right: the new cells push the rest of each row right, past the table's right edge, which the file format writes as a wider grid and `w:gridAfter` on the rows that did not grow. Shift down: the cells below move down one row per inserted row, and rows are added at the bottom to hold them. |
//! | Delete cells | Word's dialog. Shift left: the row closes up and ends short (`w:gridAfter`); a row left with no cells is deleted. Shift up: the cells below move up and the bottom of each column is left empty, as Word leaves it. |
//! | Text direction | `w:textDirection` on every cell of the range: horizontal (cleared), rotate down (`tbRl`), rotate up (`btLr`) — the three ONLYOFFICE offers on the cell menu. |
//!
//! Inserting and deleting cells with a shift refuses a table with merged
//! cells, by name: moving content through a merge would tear it, and Word's own
//! answer there ("this operation is not available for merged cells") is the
//! same refusal.
//!
//! # Complexity
//!
//! Every function is O(cells of the table) or O(inlines of the paragraphs it
//! converts). None resolves a node id per cell, and none touches the rest of
//! the document.

use casual_doc_edit::table_structure::{MintId, normalize_vertical_merges, sync_cell_widths};
use casual_doc_edit::{Operation, refused};
use casual_doc_model::NodeId;
use casual_doc_model::v1::{
    BlockNode, GridColumn, InlineNode, Paragraph, ParagraphProperties, Run, RunProperties, Tab,
    Table, TableCell, TableCellProperties, TableGridMap, TableLayout, TableLook, TableProperties,
    TableRow, TableRowProperties, TableWidth, TextDirection,
};

use crate::table::{TableEdit, first_paragraph_of_cell, grid_borders};

fn mint(ids: MintId<'_>) -> Result<NodeId, String> {
    ids().ok_or_else(|| "id space exhausted".to_owned())
}

fn empty_paragraph(ids: MintId<'_>) -> Result<Paragraph, String> {
    Ok(Paragraph {
        id: mint(ids)?,
        properties: ParagraphProperties::default().into(),
        inlines: Vec::new(),
    })
}

// ---- Style options (`w:tblLook`) ---------------------------------------------

/// One of the six table style options Word's Table Design tab and
/// ONLYOFFICE's table settings (`DE.Views.TableSettings.textBanded` and its
/// five siblings) offer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookOption {
    /// Header row: the style's first-row formatting (`w:firstRow`).
    HeaderRow,
    /// Total row: the style's last-row formatting (`w:lastRow`).
    TotalRow,
    /// Banded rows (`w:noHBand` off).
    BandedRows,
    /// First column (`w:firstColumn`).
    FirstColumn,
    /// Last column (`w:lastColumn`).
    LastColumn,
    /// Banded columns (`w:noVBand` off).
    BandedColumns,
}

impl LookOption {
    /// All six, in the order Word's Table Style Options group lists them.
    pub const ALL: [Self; 6] = [
        Self::HeaderRow,
        Self::FirstColumn,
        Self::TotalRow,
        Self::LastColumn,
        Self::BandedRows,
        Self::BandedColumns,
    ];

    /// The option's stable name, as the editor's command ids spell it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::HeaderRow => "headerRow",
            Self::TotalRow => "totalRow",
            Self::BandedRows => "bandedRows",
            Self::FirstColumn => "firstColumn",
            Self::LastColumn => "lastColumn",
            Self::BandedColumns => "bandedColumns",
        }
    }

    /// The option named `name`, if any. O(1).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|option| option.name() == name)
    }

    /// Whether the option is on in `look`. O(1).
    #[must_use]
    pub const fn is_on(self, look: &TableLook) -> bool {
        match self {
            Self::HeaderRow => look.first_row,
            Self::TotalRow => look.last_row,
            Self::BandedRows => !look.no_h_band,
            Self::FirstColumn => look.first_column,
            Self::LastColumn => look.last_column,
            Self::BandedColumns => !look.no_v_band,
        }
    }

    /// Turns the option on or off in `look`. O(1).
    pub const fn set(self, look: &mut TableLook, on: bool) {
        match self {
            Self::HeaderRow => look.first_row = on,
            Self::TotalRow => look.last_row = on,
            Self::BandedRows => look.no_h_band = !on,
            Self::FirstColumn => look.first_column = on,
            Self::LastColumn => look.last_column = on,
            Self::BandedColumns => look.no_v_band = !on,
        }
    }
}

/// The names of the options on in `look`, in [`LookOption::ALL`] order.
/// O(1).
#[must_use]
pub fn look_options_on(look: &TableLook) -> Vec<&'static str> {
    LookOption::ALL
        .into_iter()
        .filter(|option| option.is_on(look))
        .map(LookOption::name)
        .collect()
}

/// Turns one style option on or off: one `SetTableProperties` carrying the
/// table's own properties with that bit changed, so nothing else is
/// materialised. O(1) plus the properties clone.
#[must_use]
pub fn set_look_option(table: &Table, option: LookOption, on: bool) -> Vec<Operation> {
    let mut properties: TableProperties = table.properties.clone();
    option.set(&mut properties.look, on);
    vec![Operation::SetTableProperties {
        table: table.id,
        properties: Box::new(properties),
    }]
}

// ---- Nested tables -----------------------------------------------------------

/// Word's default left and right cell margin, in twips, when neither the cell
/// nor the table sets one (the layout's own default).
const DEFAULT_CELL_MARGIN_LR: i32 = 108;

/// The width a table nested in cell `cell` of row `row` should span: the grid
/// columns the cell covers, less its left and right margins. `None` when the
/// grid does not state those widths. O(cells of the table).
#[must_use]
pub fn cell_content_width(table: &Table, row: usize, cell: usize) -> Option<i32> {
    let grid = TableGridMap::build(table);
    let slot = grid.cell(row, cell)?;
    let total: i32 = table
        .grid
        .get(slot.first_column..slot.end_column())?
        .iter()
        .map(|column| column.width_twips)
        .sum::<Option<i32>>()?;
    let margins = &table.rows.get(row)?.cells.get(cell)?.properties.margins;
    let start = margins
        .start_twips
        .or(table.properties.cell_margins.start_twips)
        .unwrap_or(DEFAULT_CELL_MARGIN_LR);
    let end = margins
        .end_twips
        .or(table.properties.cell_margins.end_twips)
        .unwrap_or(DEFAULT_CELL_MARGIN_LR);
    Some((total - start - end).max(1))
}

// ---- Text to table -----------------------------------------------------------

/// Where text is divided into cells, and what joins cells back into text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Separator {
    /// Each paragraph is a cell (text to table); each cell's paragraphs stay
    /// paragraphs (table to text).
    Paragraph,
    /// A tab.
    Tab,
    /// A comma.
    Comma,
    /// A semicolon.
    Semicolon,
    /// Any other single character the user typed.
    Other(char),
}

impl Separator {
    /// The separator named `name` (`paragraph`, `tab`, `comma`, `semicolon`,
    /// `other`), with `other` taking the first character of `character`.
    ///
    /// # Errors
    /// An unknown name, or `other` with no character — ONLYOFFICE's own
    /// "You must type a character for the custom separator".
    pub fn parse(name: &str, character: &str) -> Result<Self, String> {
        match name {
            "paragraph" => Ok(Self::Paragraph),
            "tab" => Ok(Self::Tab),
            "comma" => Ok(Self::Comma),
            "semicolon" => Ok(Self::Semicolon),
            "other" => character
                .chars()
                .next()
                .filter(|c| !c.is_control())
                .map(Self::Other)
                .ok_or_else(|| {
                    refused!(
                        "table.separator-empty",
                        "Type a character to separate the text at."
                    )
                    .to_owned()
                }),
            _ => Err("separator must be paragraph, tab, comma, semicolon or other".to_owned()),
        }
    }

    /// The character a text separator splits at, or `None` for paragraph marks
    /// and tabs (a tab is an inline node of its own, not a character).
    const fn character(self) -> Option<char> {
        match self {
            Self::Paragraph | Self::Tab => None,
            Self::Comma => Some(','),
            Self::Semicolon => Some(';'),
            Self::Other(c) => Some(c),
        }
    }
}

/// How a converted table's columns are sized — ONLYOFFICE's and Word's three
/// autofit choices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Autofit {
    /// Equal fixed columns spanning the content width (`w:tblLayout fixed`).
    Fixed,
    /// Columns sized by their contents (no widths stated, autofit layout).
    Contents,
    /// The table spans the window: 100% wide, equal percentage columns.
    Window,
}

impl Autofit {
    /// The choice named `name` (`fixed`, `contents`, `window`); anything else
    /// is `fixed`, Word's default.
    #[must_use]
    pub fn parse(name: &str) -> Self {
        match name {
            "contents" => Self::Contents,
            "window" => Self::Window,
            _ => Self::Fixed,
        }
    }
}

/// The most columns a converted table may have: Word's own limit.
pub const MAX_COLUMNS: usize = 63;

/// Appends `inlines` to `out`, joining a run onto an equal-formatted run it
/// lands beside — the model refuses two such runs side by side, and a reader
/// sees one run there anyway. O(inlines).
fn push_inlines(out: &mut Vec<InlineNode>, inlines: impl IntoIterator<Item = InlineNode>) {
    for inline in inlines {
        if let InlineNode::Run(run) = &inline
            && let Some(InlineNode::Run(last)) = out.last_mut()
            && last.properties == run.properties
        {
            last.text.push_str(&run.text);
            continue;
        }
        out.push(inline);
    }
}

/// One paragraph's inlines cut into cell pieces at `separator`.
///
/// Only top-level runs and tabs are cut: a separator inside a hyperlink, a
/// field or a content control belongs to that object and stays in its piece,
/// so a link with a comma in its text is not broken in two. The first piece's
/// first run keeps its id; every later piece's runs are minted. O(inlines).
fn split_inlines(
    inlines: &[InlineNode],
    separator: Separator,
    ids: MintId<'_>,
) -> Result<Vec<Vec<InlineNode>>, String> {
    let mut pieces: Vec<Vec<InlineNode>> = vec![Vec::new()];
    for inline in inlines {
        match (inline, separator) {
            (InlineNode::Tab(_), Separator::Tab) => pieces.push(Vec::new()),
            (InlineNode::Run(run), _) if separator.character().is_some() => {
                let Some(cut) = separator.character() else {
                    continue;
                };
                let mut parts = run.text.split(cut).peekable();
                let mut first = true;
                while let Some(part) = parts.next() {
                    if !part.is_empty() {
                        let id = if first { run.id } else { mint(ids)? };
                        let piece = pieces.last_mut().ok_or("no piece")?;
                        push_inlines(
                            piece,
                            [InlineNode::Run(Run {
                                id,
                                properties: run.properties.clone(),
                                text: part.to_owned(),
                            })],
                        );
                    }
                    first = false;
                    if parts.peek().is_some() {
                        pieces.push(Vec::new());
                    }
                }
            }
            // Every other inline (a tab under a character separator, a link, a
            // field, a picture, a marker) belongs to the piece it sits in: no
            // separator is looked for inside it, on purpose (see above).
            (other, _) => {
                let piece = pieces.last_mut().ok_or("no piece")?;
                push_inlines(piece, [other.clone()]);
            }
        }
    }
    Ok(pieces)
}

/// How many columns `paragraphs` divide into at `separator`: the widest row,
/// or 1 for paragraph marks. What the dialog pre-fills, as Word's does.
/// O(inlines of the paragraphs).
#[must_use]
pub fn text_to_table_columns(paragraphs: &[&Paragraph], separator: Separator) -> usize {
    if separator == Separator::Paragraph {
        return 1;
    }
    let cut = separator.character();
    paragraphs
        .iter()
        .map(|paragraph| {
            1 + paragraph
                .inlines
                .iter()
                .map(|inline| match (inline, cut) {
                    (InlineNode::Tab(_), None) => 1,
                    (InlineNode::Run(run), Some(c)) => run.text.matches(c).count(),
                    // Separators inside other inlines are not looked for (see
                    // `split_inlines`).
                    _ => 0,
                })
                .sum::<usize>()
        })
        .max()
        .unwrap_or(1)
        .clamp(1, MAX_COLUMNS)
}

/// A cell holding `paragraph`, `width` wide.
fn cell_of(
    paragraph: Paragraph,
    width: Option<TableWidth>,
    ids: MintId<'_>,
) -> Result<TableCell, String> {
    Ok(TableCell {
        id: mint(ids)?,
        properties: TableCellProperties {
            width,
            ..TableCellProperties::default()
        },
        blocks: vec![BlockNode::Paragraph(paragraph)],
    })
}

/// The Convert Text to Table dialog's choices.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextToTable {
    /// Where the text is cut into cells.
    pub separator: Separator,
    /// The column count; `0` for the count the text needs.
    pub columns: u32,
    /// How the columns are sized.
    pub autofit: Autofit,
    /// The width a fixed table spans, in twips (the content width).
    pub content_width: i32,
}

/// Converts the paragraphs `paragraphs` — consecutive blocks `index..` of
/// `container` (`None` = the body) — into one table, with `columns` columns
/// (`options.columns`; `0` = the count [`text_to_table_columns`] finds, and
/// never fewer than that for a text separator), sized by `options.autofit`
/// across `options.content_width` twips.
///
/// Each paragraph's own properties go to the cells cut from it, the first
/// cell keeps the paragraph's id, and every run keeps its formatting. Rows are
/// padded with empty cells to the column count. The thin grid a new table
/// gets is drawn so the result is visible. One `DeleteBlocks` and one
/// `InsertTable`; the caret goes to the first cell. O(inlines of the
/// paragraphs).
///
/// # Errors
/// No paragraphs; a column count over [`MAX_COLUMNS`]; ids ran out.
pub fn text_to_table(
    container: Option<NodeId>,
    index: usize,
    paragraphs: &[&Paragraph],
    options: TextToTable,
    ids: MintId<'_>,
) -> Result<TableEdit, String> {
    let TextToTable {
        separator,
        columns,
        autofit,
        content_width,
    } = options;
    if paragraphs.is_empty() {
        return Err(refused!(
            "table.convert-nothing",
            "Select the paragraphs to convert to a table."
        )
        .to_owned());
    }
    let found = text_to_table_columns(paragraphs, separator);
    let columns = if columns == 0 {
        found
    } else {
        (columns as usize).max(if separator == Separator::Paragraph {
            1
        } else {
            found
        })
    };
    if columns > MAX_COLUMNS {
        return Err(refused!(
            "table.convert-too-wide",
            "A table can have at most 63 columns."
        )
        .to_owned());
    }
    let count = i32::try_from(columns).unwrap_or(1);
    let fixed = content_width.max(count) / count;
    let cell_width = match autofit {
        Autofit::Fixed => Some(TableWidth::dxa(fixed)),
        Autofit::Contents => None,
        Autofit::Window => Some(TableWidth::pct(5000 / count)),
    };

    // The cells, row-major.
    let mut grid_cells: Vec<Vec<TableCell>> = Vec::new();
    if separator == Separator::Paragraph {
        let mut row = Vec::with_capacity(columns);
        for paragraph in paragraphs {
            row.push(cell_of((*paragraph).clone(), cell_width, ids)?);
            if row.len() == columns {
                grid_cells.push(std::mem::take(&mut row));
            }
        }
        if !row.is_empty() {
            grid_cells.push(row);
        }
    } else {
        for paragraph in paragraphs {
            let pieces = split_inlines(&paragraph.inlines, separator, ids)?;
            let mut row = Vec::with_capacity(columns);
            for (i, inlines) in pieces.into_iter().enumerate() {
                let id = if i == 0 { paragraph.id } else { mint(ids)? };
                row.push(cell_of(
                    Paragraph {
                        id,
                        properties: paragraph.properties.clone(),
                        inlines,
                    },
                    cell_width,
                    ids,
                )?);
            }
            grid_cells.push(row);
        }
    }
    let mut rows = Vec::with_capacity(grid_cells.len());
    for mut cells in grid_cells {
        while cells.len() < columns {
            cells.push(cell_of(empty_paragraph(ids)?, cell_width, ids)?);
        }
        rows.push(TableRow {
            id: mint(ids)?,
            properties: TableRowProperties::default(),
            cells,
        });
    }
    let mut properties = TableProperties::default();
    grid_borders(&mut properties);
    match autofit {
        Autofit::Fixed => properties.layout = Some(TableLayout::Fixed),
        Autofit::Contents => properties.layout = Some(TableLayout::Autofit),
        Autofit::Window => properties.width = Some(TableWidth::pct(5000)),
    }
    let grid = (0..columns)
        .map(|_| GridColumn {
            width_twips: Some(fixed),
        })
        .collect();
    let table = Table {
        id: mint(ids)?,
        grid,
        grid_change: None,
        properties,
        rows,
    };
    let caret = table
        .rows
        .first()
        .and_then(|row| row.cells.first())
        .and_then(first_paragraph_of_cell);
    let index32 = u32::try_from(index).map_err(|_| "the container is too long".to_owned())?;
    Ok(TableEdit {
        operations: vec![
            Operation::DeleteBlocks {
                container,
                index: index32,
                count: u32::try_from(paragraphs.len())
                    .map_err(|_| "too many paragraphs".to_owned())?,
            },
            Operation::InsertTable {
                container,
                index: index32,
                table: Box::new(table),
            },
        ],
        caret,
    })
}

// ---- Table to text -----------------------------------------------------------

/// The blocks table `table` becomes as text: for a text separator, one
/// paragraph per row, the cells' first paragraphs joined by the separator
/// (a cell's further paragraphs stay paragraphs of their own, as Word keeps
/// them); for paragraph marks, every cell's blocks in reading order. A nested
/// table is converted the same way when `nested`, and kept as a table
/// otherwise. A `w:vMerge` continuation contributes nothing — its content is
/// the merged cell's, already emitted above it.
///
/// Paragraph and inline ids are kept, so a comment or bookmark anchored in a
/// cell stays anchored in the text. O(inlines of the table).
///
/// # Errors
/// When ids run out.
pub fn table_to_text_blocks(
    table: &Table,
    separator: Separator,
    nested: bool,
    ids: MintId<'_>,
) -> Result<Vec<BlockNode>, String> {
    let grid = TableGridMap::build(table);
    let mut out = Vec::new();
    for (r, row) in table.rows.iter().enumerate() {
        // The paragraph the next cell's first paragraph joins, for a text
        // separator; `None` at the start of a row and after a non-paragraph.
        let mut current: Option<Paragraph> = None;
        let mut first_cell = true;
        for (c, cell) in row.cells.iter().enumerate() {
            if grid.cell(r, c).is_some_and(|slot| slot.continuation) {
                continue;
            }
            for (b, block) in cell.blocks.iter().enumerate() {
                match block {
                    BlockNode::Paragraph(paragraph) => {
                        let joins = separator != Separator::Paragraph && b == 0 && !first_cell;
                        if joins {
                            let mut target = match current.take() {
                                Some(target) => target,
                                None => empty_paragraph(ids)?,
                            };
                            let joint = match separator {
                                Separator::Tab => InlineNode::Tab(Tab { id: mint(ids)? }),
                                _ => InlineNode::Run(Run {
                                    id: mint(ids)?,
                                    properties: RunProperties::default().into(),
                                    text: separator.character().unwrap_or(',').to_string(),
                                }),
                            };
                            push_inlines(&mut target.inlines, [joint]);
                            push_inlines(&mut target.inlines, paragraph.inlines.iter().cloned());
                            current = Some(target);
                        } else {
                            out.extend(current.take().map(BlockNode::Paragraph));
                            current = Some(paragraph.clone());
                        }
                    }
                    BlockNode::Table(inner) if nested => {
                        out.extend(current.take().map(BlockNode::Paragraph));
                        out.extend(table_to_text_blocks(inner, separator, nested, ids)?);
                    }
                    other => {
                        out.extend(current.take().map(BlockNode::Paragraph));
                        out.push(other.clone());
                    }
                }
            }
            first_cell = false;
        }
        out.extend(current.take().map(BlockNode::Paragraph));
    }
    if out.is_empty() {
        out.push(BlockNode::Paragraph(empty_paragraph(ids)?));
    }
    Ok(out)
}

/// Converts table `table`, block `index` of `container`, to text (see
/// [`table_to_text_blocks`]): the text is inserted before the table and the
/// table deleted, in that order, so a cell that held only this table is never
/// left empty in between. The caret goes to the first paragraph produced.
/// O(inlines of the table).
///
/// # Errors
/// When ids run out or the index overflows.
pub fn table_to_text(
    container: Option<NodeId>,
    index: usize,
    table: &Table,
    separator: Separator,
    nested: bool,
    ids: MintId<'_>,
) -> Result<TableEdit, String> {
    let blocks = table_to_text_blocks(table, separator, nested, ids)?;
    let caret = blocks.iter().find_map(|block| match block {
        BlockNode::Paragraph(paragraph) => Some(paragraph.id),
        _ => None,
    });
    Ok(TableEdit {
        operations: vec![
            Operation::InsertBlocks {
                container,
                index: u32::try_from(index).map_err(|_| "the container is too long".to_owned())?,
                blocks,
            },
            Operation::DeleteTable { table: table.id },
        ],
        caret,
    })
}

// ---- Split table -------------------------------------------------------------

/// Splits table `table` (block `index` of `container`) so row `row` starts a
/// second table, with an empty paragraph between them; in the first row, an
/// empty paragraph is inserted above the table instead (Word's and
/// ONLYOFFICE's behaviour). Rows keep their ids, so the caret stays where it
/// is; the second table takes the first one's grid and properties, and a
/// vertical merge the split runs through is closed on both sides.
///
/// Returns the operations and the paragraph between the tables (where the
/// caret goes for the first-row case). O(cells of the table).
///
/// # Errors
/// `row` is not a row of the table; ids ran out.
pub fn split_table(
    container: Option<NodeId>,
    index: usize,
    table: &Table,
    row: usize,
    ids: MintId<'_>,
) -> Result<(Vec<Operation>, NodeId), String> {
    if row >= table.rows.len() {
        return Err("That table has no such row.".to_owned());
    }
    let paragraph = empty_paragraph(ids)?;
    let between = paragraph.id;
    let at = |i: usize| u32::try_from(i).map_err(|_| "the container is too long".to_owned());
    if row == 0 {
        return Ok((
            vec![Operation::InsertBlocks {
                container,
                index: at(index)?,
                blocks: vec![BlockNode::Paragraph(paragraph)],
            }],
            between,
        ));
    }
    let mut first = table.clone();
    let rest = first.rows.split_off(row);
    normalize_vertical_merges(&mut first);
    let mut second = Table {
        id: mint(ids)?,
        grid: table.grid.clone(),
        grid_change: None,
        properties: table.properties.clone(),
        rows: rest,
    };
    normalize_vertical_merges(&mut second);
    Ok((
        vec![
            Operation::ReplaceTable {
                table: table.id,
                replacement: Box::new(first),
            },
            Operation::InsertBlocks {
                container,
                index: at(index + 1)?,
                blocks: vec![BlockNode::Paragraph(paragraph), BlockNode::Table(Box::new(second))],
            },
        ],
        between,
    ))
}

// ---- Insert and delete cells with a shift --------------------------------------

/// Which way an Insert or Delete Cells shift goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellShift {
    /// Insert: push the row's later cells right.
    Right,
    /// Insert: push the cells below down.
    Down,
    /// Delete: close the row up to the left.
    Left,
    /// Delete: move the cells below up.
    Up,
}

impl CellShift {
    /// The shift named `name` (`right`, `down`, `left`, `up`).
    ///
    /// # Errors
    /// Any other name.
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "right" => Ok(Self::Right),
            "down" => Ok(Self::Down),
            "left" => Ok(Self::Left),
            "up" => Ok(Self::Up),
            _ => Err("shift must be right, down, left or up".to_owned()),
        }
    }
}

/// A rectangle of physical cells: rows `rows.0..=rows.1`, cell indices
/// `cells.0..=cells.1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellRange {
    /// First and last row (inclusive).
    pub rows: (usize, usize),
    /// First and last cell index within those rows (inclusive).
    pub cells: (usize, usize),
}

impl CellRange {
    /// The rectangle between two cells, either corner first.
    #[must_use]
    pub fn between(a: (usize, usize), b: (usize, usize)) -> Self {
        Self {
            rows: (a.0.min(b.0), a.0.max(b.0)),
            cells: (a.1.min(b.1), a.1.max(b.1)),
        }
    }

    const fn height(self) -> usize {
        self.rows.1 - self.rows.0 + 1
    }

    const fn width(self) -> usize {
        self.cells.1 - self.cells.0 + 1
    }
}

/// Refuses a table a cell shift would tear: any horizontal or vertical merge,
/// or a row that starts after `w:gridBefore` (so its cell indices are not its
/// columns). O(cells).
fn shiftable(table: &Table) -> Result<(), String> {
    let merged = table.rows.iter().any(|row| {
        row.properties.grid_before.unwrap_or(0) > 0
            || row.cells.iter().any(|cell| {
                cell.properties.grid_span.unwrap_or(1) > 1
                    || cell.properties.vertical_merge.is_some()
            })
    });
    if merged {
        return Err(refused!(
            "table.shift-merged",
            "Cells cannot be shifted in a table with merged cells. Unmerge them first."
        )
        .to_owned());
    }
    Ok(())
}

/// A cell made beside `model`: its formatting, none of its content or width.
fn cell_like(model: Option<&TableCell>, ids: MintId<'_>) -> Result<TableCell, String> {
    let properties = model.map_or_else(TableCellProperties::default, |cell| TableCellProperties {
        cell_revision: None,
        cell_merge: None,
        prop_change: None,
        ..cell.properties.clone()
    });
    Ok(TableCell {
        id: mint(ids)?,
        properties,
        blocks: vec![BlockNode::Paragraph(empty_paragraph(ids)?)],
    })
}

/// Inserts `range`'s size of empty cells at `range` with a shift, as Word's
/// Insert Cells dialog does (see the module table). One `ReplaceTable`; the
/// caret goes to the first new cell. O(cells of the table).
///
/// # Errors
/// A merged table; a shift that is not right or down; a range off the table;
/// ids ran out.
pub fn insert_cells(
    table: &Table,
    range: CellRange,
    shift: CellShift,
    ids: MintId<'_>,
) -> Result<TableEdit, String> {
    shiftable(table)?;
    if range.rows.1 >= table.rows.len() {
        return Err("That table has no such row.".to_owned());
    }
    let mut next = table.clone();
    let caret;
    match shift {
        CellShift::Right => {
            let width = range.width();
            // How far past the grid the longest grown row now reaches.
            let mut reach = 0_usize;
            for row in &next.rows[range.rows.0..=range.rows.1] {
                if range.cells.0 > row.cells.len() {
                    return Err("That row has no such cell.".to_owned());
                }
                let after = row.properties.grid_after.unwrap_or(0) as usize;
                reach = reach.max(width.saturating_sub(after));
            }
            let mut first = None;
            for (r, row) in next.rows.iter_mut().enumerate() {
                let grows = (range.rows.0..=range.rows.1).contains(&r);
                let after = row.properties.grid_after.unwrap_or(0) as usize;
                let new_after = if grows {
                    after + reach - width
                } else {
                    after + reach
                };
                row.properties.grid_after = u32::try_from(new_after).ok().filter(|n| *n > 0);
                if grows {
                    let model = row
                        .cells
                        .get(range.cells.0)
                        .or_else(|| row.cells.last())
                        .cloned();
                    for k in 0..width {
                        let cell = cell_like(model.as_ref(), ids)?;
                        if first.is_none() {
                            first = first_paragraph_of_cell(&cell);
                        }
                        row.cells.insert(range.cells.0 + k, cell);
                    }
                }
            }
            if reach > 0 {
                let width_twips = table
                    .grid
                    .get(range.cells.0)
                    .or_else(|| table.grid.last())
                    .and_then(|column| column.width_twips);
                for _ in 0..reach {
                    next.grid.push(GridColumn { width_twips });
                }
            }
            caret = first;
        }
        CellShift::Down => {
            let height = range.height();
            if next.rows[range.rows.0..]
                .iter()
                .any(|row| row.cells.len() <= range.cells.1)
            {
                return Err(refused!(
                    "table.shift-short-row",
                    "A row below is too short to move these cells down."
                )
                .to_owned());
            }
            // New rows at the bottom, shaped like the last row and empty.
            let last = next.rows.last().cloned().ok_or("empty table")?;
            for _ in 0..height {
                let mut cells = Vec::with_capacity(last.cells.len());
                for model in &last.cells {
                    cells.push(cell_like(Some(model), ids)?);
                }
                next.rows.push(TableRow {
                    id: mint(ids)?,
                    properties: TableRowProperties {
                        header: false,
                        ..last.properties.clone()
                    },
                    cells,
                });
            }
            // Move each column's cells down by `height`, bottom first, and
            // put fresh cells where the moved ones were.
            let total = next.rows.len();
            for c in range.cells.0..=range.cells.1 {
                for r in (range.rows.0 + height..total).rev() {
                    let moved = next.rows[r - height].cells[c].clone();
                    next.rows[r].cells[c] = moved;
                }
                for r in range.rows.0..range.rows.0 + height {
                    let model = next.rows[r].cells[c].clone();
                    next.rows[r].cells[c] = cell_like(Some(&model), ids)?;
                }
            }
            caret = first_paragraph_of_cell(&next.rows[range.rows.0].cells[range.cells.0]);
        }
        CellShift::Left | CellShift::Up => {
            return Err("insert shifts cells right or down".to_owned());
        }
    }
    sync_cell_widths(&mut next);
    Ok(TableEdit {
        operations: vec![Operation::ReplaceTable {
            table: table.id,
            replacement: Box::new(next),
        }],
        caret,
    })
}

/// Deletes the cells of `range` with a shift, as Word's Delete Cells dialog
/// does (see the module table). One `ReplaceTable`; the caret goes to the cell
/// now at the range's top-left, else the row's last cell. O(cells).
///
/// # Errors
/// A merged table; a shift that is not left or up; a range off the table; a
/// delete that would leave no rows at all (delete the table instead).
pub fn delete_cells(
    table: &Table,
    range: CellRange,
    shift: CellShift,
    ids: MintId<'_>,
) -> Result<TableEdit, String> {
    shiftable(table)?;
    if range.rows.1 >= table.rows.len() {
        return Err("That table has no such row.".to_owned());
    }
    let mut next = table.clone();
    match shift {
        CellShift::Left => {
            for row in &mut next.rows[range.rows.0..=range.rows.1] {
                if range.cells.0 >= row.cells.len() {
                    continue;
                }
                let end = (range.cells.1 + 1).min(row.cells.len());
                let removed = end - range.cells.0;
                row.cells.drain(range.cells.0..end);
                let after = row.properties.grid_after.unwrap_or(0) as usize + removed;
                row.properties.grid_after = u32::try_from(after).ok().filter(|n| *n > 0);
            }
            next.rows.retain(|row| !row.cells.is_empty());
            if next.rows.is_empty() {
                return Err(refused!(
                    "table.shift-empties-table",
                    "That would delete every cell. Delete the table instead."
                )
                .to_owned());
            }
        }
        CellShift::Up => {
            let height = range.height();
            let total = next.rows.len();
            if next.rows[range.rows.0..]
                .iter()
                .any(|row| row.cells.len() <= range.cells.1)
            {
                return Err(refused!(
                    "table.shift-short-row",
                    "A row below is too short to move these cells up."
                )
                .to_owned());
            }
            for c in range.cells.0..=range.cells.1 {
                for r in range.rows.0..total {
                    if r + height < total {
                        let moved = next.rows[r + height].cells[c].clone();
                        next.rows[r].cells[c] = moved;
                    } else {
                        let model = next.rows[r].cells[c].clone();
                        next.rows[r].cells[c] = cell_like(Some(&model), ids)?;
                    }
                }
            }
        }
        CellShift::Right | CellShift::Down => {
            return Err("delete shifts cells left or up".to_owned());
        }
    }
    sync_cell_widths(&mut next);
    let row = range.rows.0.min(next.rows.len() - 1);
    let caret = next.rows[row]
        .cells
        .get(range.cells.0)
        .or_else(|| next.rows[row].cells.last())
        .and_then(first_paragraph_of_cell);
    Ok(TableEdit {
        operations: vec![Operation::ReplaceTable {
            table: table.id,
            replacement: Box::new(next),
        }],
        caret,
    })
}

// ---- Cell text direction -----------------------------------------------------

/// A text direction by the name the editor and agents use: `horizontal`
/// (cleared, Word's default), `rotateDown` / `rotate_down` (`tbRl`, read top to
/// bottom), `rotateUp` / `rotate_up` (`btLr`, read bottom to top).
///
/// # Errors
/// Any other name.
pub fn text_direction_named(name: &str) -> Result<Option<TextDirection>, String> {
    match name {
        "horizontal" => Ok(None),
        "rotateDown" | "rotate_down" => Ok(Some(TextDirection::TbRl)),
        "rotateUp" | "rotate_up" => Ok(Some(TextDirection::BtLr)),
        _ => Err("text direction must be horizontal, rotateDown or rotateUp".to_owned()),
    }
}

/// The editor's name for a cell's text direction (`horizontal` for none or
/// `lrTb`). O(1).
#[must_use]
pub const fn text_direction_name(direction: Option<TextDirection>) -> &'static str {
    match direction {
        Some(TextDirection::TbRl) => "rotateDown",
        Some(TextDirection::BtLr) => "rotateUp",
        Some(TextDirection::LrTb) | None => "horizontal",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use casual_doc_model::v1::VerticalMerge;

    fn ids() -> impl FnMut() -> Option<NodeId> {
        let mut next = 1_000_u64;
        move || {
            next += 1;
            NodeId::from_parts(9, next).ok()
        }
    }

    fn id(n: u64) -> NodeId {
        NodeId::from_parts(1, n).expect("non-zero")
    }

    fn run(n: u64, text: &str) -> InlineNode {
        InlineNode::Run(Run {
            id: id(n),
            properties: RunProperties::default().into(),
            text: text.to_owned(),
        })
    }

    fn paragraph(n: u64, inlines: Vec<InlineNode>) -> Paragraph {
        Paragraph {
            id: id(n),
            properties: ParagraphProperties::default().into(),
            inlines,
        }
    }

    fn text_of(paragraph: &Paragraph) -> String {
        paragraph
            .inlines
            .iter()
            .map(|inline| match inline {
                InlineNode::Run(run) => run.text.clone(),
                InlineNode::Tab(_) => "\t".to_owned(),
                _ => String::new(),
            })
            .collect()
    }

    fn cell_text(cell: &TableCell) -> String {
        cell.blocks
            .iter()
            .map(|block| match block {
                BlockNode::Paragraph(p) => text_of(p),
                _ => "[table]".to_owned(),
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    /// A regular table whose cells read "r,c".
    fn grid(rows: usize, columns: usize) -> Table {
        let mut n = 100;
        let mut next = || {
            n += 1;
            n
        };
        Table {
            id: id(1),
            grid: (0..columns)
                .map(|_| GridColumn {
                    width_twips: Some(1000),
                })
                .collect(),
            grid_change: None,
            properties: TableProperties::default(),
            rows: (0..rows)
                .map(|r| TableRow {
                    id: id(next()),
                    properties: TableRowProperties::default(),
                    cells: (0..columns)
                        .map(|c| TableCell {
                            id: id(next()),
                            properties: TableCellProperties::default(),
                            blocks: vec![BlockNode::Paragraph(paragraph(
                                next(),
                                vec![run(next(), &format!("{r}{c}"))],
                            ))],
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    fn options(separator: Separator, columns: u32, autofit: Autofit) -> TextToTable {
        TextToTable {
            separator,
            columns,
            autofit,
            content_width: 9000,
        }
    }

    fn replacement(edit: &TableEdit) -> &Table {
        match &edit.operations[0] {
            Operation::ReplaceTable { replacement, .. } => replacement,
            other => panic!("expected ReplaceTable, got {other:?}"),
        }
    }

    fn texts(table: &Table) -> Vec<Vec<String>> {
        table
            .rows
            .iter()
            .map(|row| row.cells.iter().map(cell_text).collect())
            .collect()
    }

    #[test]
    fn each_look_option_flips_exactly_its_own_bit() {
        let table = grid(2, 2);
        for option in LookOption::ALL {
            for on in [true, false] {
                let ops = set_look_option(&table, option, on);
                let Operation::SetTableProperties { properties, .. } = &ops[0] else {
                    panic!("expected SetTableProperties");
                };
                assert_eq!(option.is_on(&properties.look), on, "{}", option.name());
                for other in LookOption::ALL.into_iter().filter(|o| *o != option) {
                    assert_eq!(
                        other.is_on(&properties.look),
                        other.is_on(&table.properties.look),
                        "{} moved {}",
                        option.name(),
                        other.name()
                    );
                }
            }
        }
        // Banding is stored inverted, as the file format stores it.
        let ops = set_look_option(&table, LookOption::BandedRows, false);
        let Operation::SetTableProperties { properties, .. } = &ops[0] else {
            panic!();
        };
        assert!(properties.look.no_h_band);
    }

    #[test]
    fn comma_text_becomes_a_table_that_keeps_ids_and_pads_short_rows() {
        let first = paragraph(10, vec![run(11, "Name,Qty,Price")]);
        let second = paragraph(20, vec![run(21, "Pears,4")]);
        let mut mint = ids();
        let edit = text_to_table(
            None,
            3,
            &[&first, &second],
            options(Separator::Comma, 0, Autofit::Fixed),
            &mut mint,
        )
        .expect("converts");
        let Operation::DeleteBlocks { index, count, .. } = &edit.operations[0] else {
            panic!("expected DeleteBlocks first");
        };
        assert_eq!((*index, *count), (3, 2));
        let Operation::InsertTable { table, index, .. } = &edit.operations[1] else {
            panic!("expected InsertTable");
        };
        assert_eq!(*index, 3);
        assert_eq!(
            texts(table),
            vec![vec!["Name", "Qty", "Price"], vec!["Pears", "4", ""]]
        );
        // The first cell of each row is the source paragraph, by id.
        let BlockNode::Paragraph(p) = &table.rows[1].cells[0].blocks[0] else {
            panic!();
        };
        assert_eq!(p.id, id(20));
        assert_eq!(table.grid.len(), 3);
        assert_eq!(table.properties.layout, Some(TableLayout::Fixed));
    }

    #[test]
    fn tab_text_splits_at_tab_nodes_and_paragraph_marks_fill_rows() {
        let row = paragraph(
            10,
            vec![
                run(11, "a"),
                InlineNode::Tab(Tab { id: id(12) }),
                run(13, "b"),
            ],
        );
        let mut mint = ids();
        let edit = text_to_table(
            None,
            0,
            &[&row],
            options(Separator::Tab, 0, Autofit::Window),
            &mut mint,
        )
        .expect("converts");
        let Operation::InsertTable { table, .. } = &edit.operations[1] else {
            panic!();
        };
        assert_eq!(texts(table), vec![vec!["a", "b"]]);
        assert_eq!(table.properties.width, Some(TableWidth::pct(5000)));

        let paragraphs: Vec<Paragraph> = (0..5)
            .map(|i| paragraph(30 + i * 2, vec![run(31 + i * 2, &i.to_string())]))
            .collect();
        let refs: Vec<&Paragraph> = paragraphs.iter().collect();
        let edit = text_to_table(
            None,
            0,
            &refs,
            options(Separator::Paragraph, 2, Autofit::Contents),
            &mut mint,
        )
        .expect("converts");
        let Operation::InsertTable { table, .. } = &edit.operations[1] else {
            panic!();
        };
        assert_eq!(
            texts(table),
            vec![vec!["0", "1"], vec!["2", "3"], vec!["4", ""]]
        );
    }

    #[test]
    fn a_column_count_below_the_widest_row_is_raised_to_it() {
        let row = paragraph(10, vec![run(11, "a;b;c")]);
        assert_eq!(text_to_table_columns(&[&row], Separator::Semicolon), 3);
        let mut mint = ids();
        let edit = text_to_table(
            None,
            0,
            &[&row],
            options(Separator::Semicolon, 1, Autofit::Fixed),
            &mut mint,
        )
        .expect("converts");
        let Operation::InsertTable { table, .. } = &edit.operations[1] else {
            panic!();
        };
        assert_eq!(
            table.grid.len(),
            3,
            "no text is dropped by a too-small count"
        );
    }

    #[test]
    fn table_to_text_joins_cells_with_the_separator_and_is_inserted_before_the_delete() {
        let table = grid(2, 3);
        let mut mint = ids();
        let edit =
            table_to_text(None, 4, &table, Separator::Tab, false, &mut mint).expect("converts");
        let Operation::InsertBlocks { blocks, index, .. } = &edit.operations[0] else {
            panic!("the text goes in first");
        };
        assert_eq!(*index, 4);
        assert!(matches!(edit.operations[1], Operation::DeleteTable { .. }));
        let lines: Vec<String> = blocks
            .iter()
            .map(|b| match b {
                BlockNode::Paragraph(p) => text_of(p),
                _ => String::new(),
            })
            .collect();
        assert_eq!(lines, vec!["00\t01\t02", "10\t11\t12"]);

        let edit = table_to_text(None, 0, &table, Separator::Paragraph, false, &mut mint)
            .expect("converts");
        let Operation::InsertBlocks { blocks, .. } = &edit.operations[0] else {
            panic!();
        };
        assert_eq!(
            blocks.len(),
            6,
            "one paragraph per cell for paragraph marks"
        );

        let edit = table_to_text(None, 0, &table, Separator::Other('|'), false, &mut mint)
            .expect("converts");
        let Operation::InsertBlocks { blocks, .. } = &edit.operations[0] else {
            panic!();
        };
        let BlockNode::Paragraph(p) = &blocks[0] else {
            panic!();
        };
        // Equal-formatted runs joined into one: the model refuses them side by side.
        assert_eq!(p.inlines.len(), 1);
        assert_eq!(text_of(p), "00|01|02");
    }

    #[test]
    fn a_nested_table_is_kept_unless_asked_to_convert_it() {
        let mut outer = grid(1, 2);
        let mut inner = grid(1, 2);
        inner.id = id(900);
        for (i, row) in inner.rows.iter_mut().enumerate() {
            row.id = id(901 + i as u64);
        }
        // Fresh ids for the inner table's cells and paragraphs.
        let mut n = 950;
        for row in &mut inner.rows {
            for cell in &mut row.cells {
                n += 1;
                cell.id = id(n);
                for block in &mut cell.blocks {
                    if let BlockNode::Paragraph(p) = block {
                        n += 1;
                        p.id = id(n);
                        for inline in &mut p.inlines {
                            if let InlineNode::Run(r) = inline {
                                n += 1;
                                r.id = id(n);
                            }
                        }
                    }
                }
            }
        }
        outer.rows[0].cells[1].blocks.push(BlockNode::Table(Box::new(inner)));
        let mut mint = ids();
        let kept =
            table_to_text_blocks(&outer, Separator::Tab, false, &mut mint).expect("converts");
        assert!(kept.iter().any(|b| matches!(b, BlockNode::Table(_))));
        let flat = table_to_text_blocks(&outer, Separator::Tab, true, &mut mint).expect("converts");
        assert!(flat.iter().all(|b| matches!(b, BlockNode::Paragraph(_))));
    }

    #[test]
    fn splitting_moves_the_caret_row_and_below_into_a_second_table() {
        let table = grid(4, 2);
        let mut mint = ids();
        let (ops, between) = split_table(None, 2, &table, 2, &mut mint).expect("splits");
        let Operation::ReplaceTable { replacement, .. } = &ops[0] else {
            panic!();
        };
        assert_eq!(replacement.rows.len(), 2);
        let Operation::InsertBlocks { index, blocks, .. } = &ops[1] else {
            panic!();
        };
        assert_eq!(*index, 3);
        assert!(matches!(&blocks[0], BlockNode::Paragraph(p) if p.id == between));
        let BlockNode::Table(second) = &blocks[1] else {
            panic!();
        };
        assert_ne!(second.id, table.id);
        assert_eq!(second.rows[0].id, table.rows[2].id, "rows keep their ids");
        assert_eq!(texts(second), vec![vec!["20", "21"], vec!["30", "31"]]);

        let (ops, _) = split_table(None, 2, &table, 0, &mut mint).expect("splits");
        assert_eq!(
            ops.len(),
            1,
            "the first row inserts a paragraph above instead"
        );
        assert!(matches!(&ops[0], Operation::InsertBlocks { index: 2, .. }));
    }

    #[test]
    fn a_split_through_a_vertical_merge_closes_it_on_both_sides() {
        let mut table = grid(3, 1);
        table.rows[0].cells[0].properties.vertical_merge = Some(VerticalMerge::Restart);
        table.rows[1].cells[0].properties.vertical_merge = Some(VerticalMerge::Continue);
        let mut mint = ids();
        let (ops, _) = split_table(None, 0, &table, 1, &mut mint).expect("splits");
        let Operation::ReplaceTable { replacement, .. } = &ops[0] else {
            panic!();
        };
        assert_eq!(replacement.rows[0].cells[0].properties.vertical_merge, None);
        let Operation::InsertBlocks { blocks, .. } = &ops[1] else {
            panic!();
        };
        let BlockNode::Table(second) = &blocks[1] else {
            panic!();
        };
        assert_eq!(second.rows[0].cells[0].properties.vertical_merge, None);
    }

    #[test]
    fn inserting_cells_right_widens_the_grid_and_pads_the_other_rows() {
        let table = grid(2, 2);
        let mut mint = ids();
        let range = CellRange::between((0, 0), (0, 0));
        let edit = insert_cells(&table, range, CellShift::Right, &mut mint).expect("inserts");
        let next = replacement(&edit);
        assert_eq!(texts(next), vec![vec!["", "00", "01"], vec!["10", "11"]]);
        assert_eq!(next.grid.len(), 3);
        assert_eq!(next.rows[0].properties.grid_after, None);
        assert_eq!(next.rows[1].properties.grid_after, Some(1));
        assert!(edit.caret.is_some());
    }

    #[test]
    fn inserting_cells_down_moves_the_column_and_adds_rows() {
        let table = grid(2, 2);
        let mut mint = ids();
        let range = CellRange::between((0, 1), (0, 1));
        let edit = insert_cells(&table, range, CellShift::Down, &mut mint).expect("inserts");
        let next = replacement(&edit);
        assert_eq!(
            texts(next),
            vec![vec!["00", ""], vec!["10", "01"], vec!["", "11"]]
        );
    }

    #[test]
    fn deleting_cells_left_closes_the_row_and_records_grid_after() {
        let table = grid(2, 3);
        let mut mint = ids();
        let range = CellRange::between((1, 0), (1, 1));
        let edit = delete_cells(&table, range, CellShift::Left, &mut mint).expect("deletes");
        let next = replacement(&edit);
        assert_eq!(texts(next), vec![vec!["00", "01", "02"], vec!["12"]]);
        assert_eq!(next.rows[1].properties.grid_after, Some(2));
        assert_eq!(next.grid.len(), 3, "the grid is unchanged");
    }

    #[test]
    fn deleting_cells_up_moves_the_column_up_and_empties_the_bottom() {
        let table = grid(3, 2);
        let mut mint = ids();
        let range = CellRange::between((0, 0), (0, 0));
        let edit = delete_cells(&table, range, CellShift::Up, &mut mint).expect("deletes");
        let next = replacement(&edit);
        assert_eq!(
            texts(next),
            vec![vec!["10", "01"], vec!["20", "11"], vec!["", "21"]]
        );
    }

    #[test]
    fn a_shift_refuses_a_merged_table_by_name() {
        let mut table = grid(2, 2);
        table.rows[0].cells[0].properties.grid_span = Some(2);
        table.rows[0].cells.pop();
        let mut mint = ids();
        let range = CellRange::between((0, 0), (0, 0));
        let refused = insert_cells(&table, range, CellShift::Right, &mut mint).unwrap_err();
        assert!(refused.contains("merged"), "{refused}");
        let refused = delete_cells(&table, range, CellShift::Up, &mut mint).unwrap_err();
        assert!(refused.contains("merged"), "{refused}");
    }

    #[test]
    fn text_direction_names_round_trip() {
        for name in ["horizontal", "rotateDown", "rotateUp"] {
            assert_eq!(
                text_direction_name(text_direction_named(name).unwrap()),
                name
            );
        }
        assert!(text_direction_named("diagonal").is_err());
    }

    #[test]
    fn a_nested_table_spans_its_cell_less_the_margins() {
        let table = grid(1, 2);
        assert_eq!(
            cell_content_width(&table, 0, 1),
            Some(1000 - 2 * DEFAULT_CELL_MARGIN_LR)
        );
    }
}
