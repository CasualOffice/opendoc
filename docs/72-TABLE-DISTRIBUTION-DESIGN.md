# Table editing operation designs

These are bounded implementation notes. [The later table audit](141-GOOGLE-DOCS-TABLE-EXPERIENCE-GAP-AND-DESIGN.md) records where their original descriptions drifted from the implementation.

## Row and column distribution

### Decision

The table ribbon exposes `Distribute columns` and `Distribute rows` as one
undoable operation each. Distribution is available only for regular tables:
every row must have the same number of cells and no grid spans or vertical
merges. This keeps the operation deterministic and avoids silently rewriting
merged-cell geometry.

Column distribution preserves the table's current preferred grid width and
divides it evenly, assigning any remainder to the first columns. Every grid
column and corresponding cell width is updated together so DOCX export and the
web layout agree.

Row distribution requires every row to have a positive explicit height and a
non-auto rule. It preserves the current total height, divides it evenly, and
preserves the common `exact` or `atLeast` rule. Auto-height rows are refused
instead of being assigned an arbitrary height that could clip content.

Both commands use `ReplaceTable` and `HistoryKind::TableResize`, so undo and
redo restore the complete table snapshot as a single history entry.

### Follow-up

Selection-scoped distribution, merged-cell distribution, and layout-aware
auto-height measurement remain out of scope until the runtime has a
deterministic layout measurement API.

The next table increment adds a count-aware split dialog. The first safe
geometry slice supports horizontal merged cells and preserves all unaffected
rows; vertical and multi-row subdivision remains explicitly rejected until
row insertion/removal can be made container-safe.

The table style slice reuses authored `w:style` table definitions rather than
inventing CSS-only presets. The gallery reflects the active style name and
commits style changes through `SetTableProperties`, preserving DOCX round-trip
fidelity and making each selection one undoable action.


## Table sorting (former doc 73)

Table sorting is a deterministic, undoable row reorder over the active table.
It sorts by the first cell's plain text using Unicode case-folded lexical order;
`ascending` and `descending` are the only modes. A repeated-header row remains
at the top and is not moved. Sorting is refused for merged/spanned tables or
cells whose first block is not a paragraph, avoiding ambiguous structural
reordering. The complete table is replaced in one `ReplaceTable` transaction,
so content, formatting, and row properties move together and undo restores the
original order exactly.


## Table caption and accessibility description (former doc 74)

Table caption and description are exposed in the existing live Table
properties inspector. Values are trimmed, bounded to the model's 255-byte
limit, and empty values clear the authored metadata. They use the existing
`ReplaceTable` property transaction, so each completed field interaction is
undoable and DOCX export preserves `w:tblCaption` and `w:tblDescription`.


## Table formula calculation (former doc 75)

The first formula slice is intentionally bounded to deterministic calculated
values. It accepts `=SUM(ABOVE)`, `=SUM(LEFT)`, `=AVERAGE(...)`, `=MIN(...)`,
and `=MAX(...)` over numeric cells in the active regular table. The result is
written as a normal cell run through one undoable table replacement. Formula
field-code preservation and automatic dependency recalculation remain deferred
until the model has a first-class OOXML field representation; the UI labels this
as “Calculate formula” rather than claiming live spreadsheet semantics.
