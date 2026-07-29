# 70 — Table Editing UX Foundation

Status: accepted first increment
Date: 2026-07-30
Scope: contextual table ribbon, table/cell formatting popover, and table properties inspector

## Problem

The engine already exposes undoable commands for table selection, structural
row/column edits, merge/split, borders, shading, alignment, sizing, header rows,
margins, and spacing. The web host placed all of them in one 325 px-wide popover
that measured roughly 1,111 px tall in a 1,000 px viewport. Its lower controls
were clipped, core commands were hidden behind one generic table icon, and the
contextual Table ribbon mostly displayed a right-click hint.

This is an information-architecture and transaction problem, not a reason to add
parallel DOM-owned table state.

## Accepted interaction

The contextual Table ribbon remains disabled outside a table. Inside a table it
contains only working commands, split into five groups:

1. **Select** — current row, column, or table.
2. **Rows & columns** — insert above/below/left/right and delete row/column/table.
3. **Merge** — merge the active table selection or split the active merged cell.
4. **Cell format** — opens a short anchored popover for shading, vertical
   alignment, cell borders, and table borders.
5. **Properties** — opens a right-side inspector for table, row, column, and
   default-cell measurements.

The detailed inspector follows the shared panel contract from doc 63: titled,
non-modal, scrollable body, fixed action row, explicit Reset/Apply, Escape/close
dismissal, and trigger-focus restoration. On desktop it shares the work area
with the canvas so the selected table remains visible. At narrow widths it
becomes a viewport-bounded right drawer.

## Mutation and undo contract

The inspector is a draft form. It reads one `TableInfo` snapshot on open and does
not mutate during input. Apply sends one bounded JSON payload to the WASM bridge.
The bridge clones the innermost table, updates the table properties plus the
active row/column fields, and commits one `ReplaceTable` operation. Therefore:

- one Apply is one undo action;
- invalid payloads leave the document unchanged;
- Reset is mutation-free and restores the current model snapshot;
- the model remains the source of truth;
- regular-grid-only column sizing remains explicitly disabled/refused for
  merged or spanned tables.

The retained compact formatting popover continues to use the existing
single-command transactions because every click/change is already one visible
user action. After a successful Apply, the inspector remains open and reflects
the committed model values for iterative adjustment. While the draft is clean,
moving the caret to another cell refreshes the inspector context; a dirty draft
remains pinned to its original table cell until it is applied, reset, or closed.

## Responsive and accessibility rules

- The ribbon stays one row and horizontally scrolls at narrow widths.
- Every icon command has an accessible name and tooltip.
- Destructive commands use the existing danger treatment without hiding them.
- The inspector is 320 px wide on desktop; its body scrolls while actions remain
  reachable.
- At widths below 900 px it overlays as a bounded right drawer rather than
  collapsing the document canvas to an unusable width.
- Fields use native labels and numeric constraints.
- The inspector does not lock the page or trap focus. Escape closes it when focus
  is within the inspector, and focus returns to Properties after closing.

## Explicit deferrals

This increment does not claim arbitrary rectangular drag selection,
distribute rows/columns, multi-count split-cell UI, styles gallery, sort,
formulas, captions, or alt-text authoring. Those remain separate table slices.

## Verification

- Rust regression: applying all inspector fields changes the intended innermost
  table and one undo restores the full prior table.
- Browser regression: the Table ribbon exposes every structural/select/merge
  command; the formatting popover fits in the viewport; the properties inspector
  reflects context, resets without mutation, stays non-modal and reachable at
  narrow widths, restores focus on close, and applies through one editor command.
- `cargo fmt --check`, focused WASM tests, web build/unit tests, Playwright, and
  `git diff --check`.
