# Table row and column distribution

## Decision

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

## Follow-up

Selection-scoped distribution, merged-cell distribution, and layout-aware
auto-height measurement remain out of scope until the runtime has a
deterministic layout measurement API.
