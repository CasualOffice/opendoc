# 141 — Google Docs table experience: gap analysis and design

**Status:** Design, complete. **Opened:** 2026-09-28. **Owner:** unassigned.
**Scope:** the *interaction* quality of table editing in OpenDoc measured against Google
Docs, plus the design for the three lanes that close most of it. Docs-only: this document
changes no code.

**Why this exists.** The owner's words: *"table experience is pathetic in our platform .. we
need complete table editing experience of google docs and try to provide that experience"*.

**This is not a feature inventory.** We already ship **19 invocable table commands** plus 4
submenu containers (§0.3), a contextual Table ribbon tab with 19 controls, a live right-side
table properties inspector, a Google-Docs-shaped grid-picker table inserter, table sorting and
table formulas — the last two of which ONLYOFFICE does not have at all. Listing commands again
would answer a question nobody asked. The complaint is about **affordances, directness and
feedback**, and that is what is measured here.

**The finding, in one sentence.**

> Our table is a **menu-operated** table: almost nothing about it responds to the pointer
> until you have first clicked *inside* it, and then only one gesture — column-width drag —
> is direct. Google Docs' table is a **pointer-operated** table: every structural edit has a
> hit zone on or beside the table itself. We have the commands; we have almost none of the
> zones.

The single measurement that shows it: `webapp/src/pointer_cursor.mjs` enumerates 32 pointer
targets on the editing surface, and the only table entries are `table-column-handle`,
`drag-table-column`, `table-cell` (deliberately plain text) and `table-row-boundary` — the
last of which is recorded with `owner: "unprobed"`, i.e. a target the engine cannot report.
Four rows out of 32, one of them a known hole.

---

## 0. Method — and why every number here is reproducible

### 0.1 Our side is cited by grep anchor, never by line number

`webapp/src/main.js` is 16,579 lines in this branch and has moved by hundreds of lines
per day. `docs/130` §0 records that its first draft carried ~30 `main.js:NNNN` citations
that were all stale within 24 hours. So every claim about our code below names **a string
to grep** — a function name, a CSS class, a `js_name`, an error message — and the file it
is in. Where a Rust helper is cited the same rule applies.

### 0.2 Claims about Google Docs are KNOWLEDGE-BASED, not source-verified

There is no Google Docs source in this environment. `/Users/sachin/Desktop/melp/reference/`
contains exactly two checkouts, `sdkjs` and `web-apps`, both ONLYOFFICE. Google Docs'
client is closed and minified; its behaviour cannot be read the way ONLYOFFICE's can, and
it cannot be run from here.

**Therefore every "Docs does X" statement in this document is the author's knowledge of the
product, not a citation, and each is tagged `[K]` in the tables.** That is a real limit and
it is stated per row rather than hidden. A wrong confident claim about a competitor is the
worst possible content in a document that will be quoted (`SKILL.md` §9).

Where a row's competitive standard can be *source-verified*, it is — from ONLYOFFICE, which
is the product we are replacing and whose table pointer code is readable. Those rows are
tagged `[S]` with a `file:line` into the pinned checkout. ONLYOFFICE is not the bar Docs is,
but a source-verified second data point is worth more than none, and in several rows below
**ONLYOFFICE is ahead of us and Docs is ahead of both**, which is the most useful shape a
row can have.

### 0.3 The command count, derived

This is the same extraction `webapp/tests/menu_taxonomy.test.mjs` performs, so the number the
guard sees and the number published here cannot drift:

```sh
cd <repo>
grep -oE '(id: |tableMutation\()"table\.[A-Za-z.]+"' webapp/src/main.js \
  | grep -oE 'table\.[A-Za-z.]+' | sort -u          # the ids
grep -oE '(id: |tableMutation\()"table\.[A-Za-z.]+"' webapp/src/main.js \
  | grep -oE 'table\.[A-Za-z.]+' | sort -u | wc -l  # => 23
```

**The widely-quoted "24 table commands" is wrong twice, and this document's own first draft
repeated one of the errors.** `table.styleNamed` is not a command — it is an **i18n message
key**, used as `t("table.styleNamed", { name: activeTableStyle })` to title the style button.
And `table.insert`, `table.delete`, `table.select` and `table.layout` are **submenu
containers** built in `tableToolCommands` with a `submenu:` and no `run`. So:

| Figure | Value |
| --- | --- |
| `table.*` ids in `main.js` | **23** |
| …minus the 4 submenu containers | **19 invocable table commands** |
| plus `insert.table` (insert namespace) | **20** |
| plus one generated `table.style.<name>` per table style the open document defines | variable — `for (const name of doc.listTableStyles?.() ?? [])` |

Every one of the 19 appears in `webapp/src/command_taxonomy.mjs` (grep `table: [`) and is
reachable from the contextual Table ribbon band, the application Table menu, the canvas
context menu and the command palette — four surfaces, comfortably past the ≥2 floor
(`SKILL.md` §10). There are **no table keyboard shortcuts**: `grep -n shortcut
webapp/src/main.js | grep -i table` returns only comments, and `webapp/src/keymap.mjs` has no
table chord.

**Caveat on "reachable from four surfaces": only the MENU surface is recorded as data.**
`command_taxonomy.mjs` exports `menuHomes()` / `menuCommandIds()`, so "which menu owns this
id" is machine-checkable in both directions. Ribbon and context reachability are not:
`sed -n '/id="panelTable"/,/id="panelView"/p' webapp/editor.html | grep -oE 'data-[a-z-]+' |
sort -u` shows **no `data-command` attribute anywhere in the Table band** — only
`data-table-action|distribute|select|sort`. That is the `docs/105` UX-005 residue, whose
re-measurement note records Home (45), **Table (19)** and View (5) as still unstamped while
Insert, Layout, References and Review are declarative. **A table SURFACE table does not exist
yet**, so no design below may assume one.

### 0.4 The engine op count, derived

```sh
awk '/^pub enum Operation \{/,/^\}/' crates/casual-doc-edit/src/lib.rs \
  | grep -oE '^    [A-Z][A-Za-z0-9]* \{' | tr -d ' {' | wc -l                        # => 53
awk '/^pub enum Operation \{/,/^\}/' crates/casual-doc-edit/src/lib.rs \
  | grep -oE '^    [A-Z][A-Za-z0-9]* \{' | tr -d ' {' | grep -cE 'Row|Column|Table'  # => 9
```

The 9: `InsertRow`, `DeleteRow`, `InsertColumn`, `DeleteColumn`, `InsertTable`,
`DeleteTable`, `SetTableCellProperties`, `SetTableProperties`, `ReplaceTable`.

**`Operation::ReplaceTable` is the load-bearing one, and it is why so few rows below grade
`engine`.** Its own doc comment calls it *"reserved for structural transforms such as
merge/split cells where exact undo needs the previous row/cell topology"*, and row height,
header-row repeat, column width, distribute, merge, split and sort are **all** already
shipped through it. So *"there is no `set_row_height` op"* is not evidence of an engine gap,
and a previous gap document graded a page break `engine` by exactly that mistaken inference.
Every `engine` grade in §2 names the missing model field, the missing layout consumer or the
missing selection type — not a missing command.

### 0.5 The facade export count, derived

```sh
grep -n 'js_name' crates/casual-doc-wasm/src/lib.rs | grep -iE 'table|cell|row|column'
```

This under-counts, because wasm-bindgen exports a `pub fn` with no `js_name` under its
snake_case name verbatim and several `TableInfo` getters have none. The authoritative count,
taken by brace-matching every `#[wasm_bindgen]` `impl` block and listing its `pub fn`s, is
**425 exported methods in the crate, 66 of them table-related** — 41 on `WasmDocument`, 20
`TableInfo` getters, 5 `CellTextRange` getters.

### 0.6 The ribbon budget, and the figure not to quote

Any row below that proposes a ribbon control says where the width comes from. The budget is
derived by `webapp/tests/e2e/ribbon-width-budget.spec.mjs` at a fixed 1280×900, and the
derivation is worth restating because the *mechanism* used to be quoted wrongly:

1. "Fits" is a **two-part** predicate — nothing exiled (`#ribbonOverflowBtn` absent or hidden)
   **and** no horizontal scroll (`scrollWidth > clientWidth + 1` on `#panelHome`, its parent,
   or `documentElement`).
2. Headroom is measured by **growing the band's last `.rgroup` with a pad `<span>` in 4px
   steps**, `await`ing a `requestAnimationFrame` each step, and returning the last width that
   still fit.

Published: **~288px** of Home headroom at 1280 (in the file's comment), Home fits down to a
**1017px** viewport, the other six bands to **702–743px**. The single assertion is a **120px
floor** — `expect(headroom).toBeGreaterThanOrEqual(120)` — deliberately not pinned to 288,
because a guard that reds on any change that spends budget gets deleted. The number is
published to the log as `RIBBON_HOME_HEADROOM_AT_1280 <n>px` so spending it shows in a diff.
A second test pins the mechanism: grow the last group by a flat 400px and assert
`hscroll === true` **and** `exiled === false`.

**Do not quote the old "~55px of slack"**: it was wrong by five-fold, it named the wrong
failure mode, and it was being used to reject additions (`SKILL.md` §11).

**No row in this document proposes a new ribbon control.** Every design below spends canvas
and overlay space, not band space — which is the point: the gap is direct manipulation, and
a direct-manipulation gap cannot be closed by a button.

### 0.7 The four grades

| Grade | Meaning |
| --- | --- |
| **UI-only** | the facade and engine already do it; only `webapp/**` changes |
| **facade+UI** | `crates/casual-doc-wasm` needs a new export or a new parameter; model and layout already suffice |
| **engine** | a model field, a layout consumer, an op, or a selection type is genuinely missing |
| **reachability-only** | we ship it and it works, but not from where the user reaches for it |

---

## 1. The interactions, audited one at a time

### 1.1 Direct resize

**Docs `[K]`.** Hover any table — no click first — and the pointer over a vertical border
becomes `col-resize`, over a horizontal border `row-resize`. Dragging a *vertical* border
**moves the border**: the column left of it grows by exactly what the column right of it
loses, and the table's total width does not change. Dragging the table's outer right border
changes the table width. Dragging a *horizontal* border changes that row's height. A guide
follows the pointer and the table re-flows on release.

**Ours.**

| Aspect | State | Anchor |
| --- | --- | --- |
| Column boundary is a hit target | **Only for the table containing the caret** | `paintTableResizeHandles` in `webapp/src/main.js` opens `if (!doc?.inTable(focus.node)) return;` and is called from `paintOverlayLayer` as `paintTableResizeHandles(selection.focus)` |
| …and only for a *regular* table | yes — a single merged cell removes every handle | `fn table_column_resize_handles` in `crates/casual-doc-wasm/src/lib.rs`: `if !table_is_regular(t) { return Vec::new(); }` |
| …and only on INTERNAL borders | yes — the table's outer left and right edges have no handle | same fn: `for col in 0..cols - 1` |
| Hit zone width | 10px (±5px about the border) | `.overlay .table-col-resize-handle { width: 10px; margin-left: -5px }` in `webapp/src/style.css` |
| Cursor | correct (`col-resize`) on hover and held through the drag | `webapp/src/pointer_cursor.mjs` rows `table-column-handle` and `drag-table-column` |
| Live preview | a **vertical guide line only**; the table does not reflow until release | `.table-col-resize-preview` created in `startTableColumnResize`, moved by `transform: translateX` in `updateTableColumnResize`, committed in `finishTableColumnResize` |
| Guide extent | the **full page height**, not the table's height | `startTableColumnResize` sets the preview height from `page.overlay.clientHeight` |
| Commit semantics | **sets one column's absolute width; no neighbour absorbs it** | `finishTableColumnResize` calls `doc.setTableColumnWidthAt(node, col, lastWidthTwips)`; the facade writes `grid[col].width_twips` and each row's `cells[col].properties.width` and touches nothing else |
| Dead-zone | changes under 8 twips (about 0.006 in) are discarded | `finishTableColumnResize`: `if (Math.abs(drag.lastWidthTwips - drag.startWidthTwips) >= 8)` |
| Minimum width | clamped to 72 twips (0.05 in) in the UI, 1 twip in the facade | `Math.max(72, …)` in `updateTableColumnResize`; `width_twips.clamp(1, 31_680)` in `set_table_column_width_at` |
| Row boundary | **no hit target of any kind** | `grep -rn 'row-resize' webapp/src crates` returns exactly one line: the `table-row-boundary` row in `pointer_cursor.mjs`, whose `owner` is `"unprobed"` |
| Keyboard equivalent | row height and column width, absolute, in the inspector only | `#tableRowHeight`, `#tableColumnWidth` in `webapp/editor.html` |
| Touch | the column handle works (`touch-action: none` is set) but is 10px wide | `.overlay .table-col-resize-handle` |
| Undo | one entry per drag, not per pointer-move — the commit is on release only | `finishTableColumnResize`; `HistoryKind::TableResize` |

**ONLYOFFICE `[S]`, and they are ahead of us here.** `CTable.prototype.IsTableBorder(X, Y,
CurPage)` (`reference/sdkjs/word/Editor/Table.js:3515`) is a **point-based** border hit test
reached from `CDocument.prototype.IsTableBorder` (`.../Editor/Document.js:8520`) on every
mouse move, with no dependency on where the caret is.
`CTable.prototype.private_CheckHitInBorder` gives the geometry: a tolerance of
`nRadius = this.DrawingDocument.GetMMPerDot(3)` — **3 px** — applied to **all four** borders
(`Border = 0` top, `1` right, `2` bottom, `3` left). So ONLYOFFICE hit-tests row boundaries
and we do not; our 10px column zone is more generous than their 3px, which is the one part
of this row where we lead.

**What the difference costs.** Three separate costs, and they compound:

1. **You must commit to the table before it becomes manipulable.** A user who wants to widen
   a column must click into a cell first — which moves the caret, breaks the typing session
   (`breakTypingSession` runs in `navToPosition`) and, if they were mid-edit elsewhere, loses
   their place. In Docs, widening a column is a gesture you perform on the way past.
2. **The drag does the wrong thing.** Because `setTableColumnWidthAt` changes one column and
   nothing else, dragging column 1's right border *widens the whole table* rather than moving
   the border. On a table already at the content width the result depends on the width solver
   rather than on the gesture, which is the definition of an unpredictable control. This is
   the row most likely to read as "pathetic" while every individual part works.
3. **Row height is menu-only.** Making a row taller requires: click into it, open the Table
   tab, open Properties, change Height rule from Auto to At least, type inches. Docs: drag.

**Grades.** Caret-gated handles: **facade+UI** — no page-scoped table enumeration exists;
every facade table entry point starts from a caret `NodeId`. Border semantics: **facade+UI**.
Row-boundary resize: **facade+UI** — the *mutation* already exists (`js_name =
setTableRowHeight`) and the layout already honours `trHeight` exactly (`fn resolve_row_height`
in `crates/casual-doc-layout/src/flow.rs`, where `Exact` pins the height and sets a clip flag
and `atLeast` takes the max of content and value); only the **geometry query** is missing.

### 1.2 Hover insert affordances

**Docs `[K]`.** Hovering the strip just outside a table shows a small plus between every pair
of rows and every pair of columns; one click inserts there. This is the affordance that makes
"add a row where I am looking" a single gesture rather than a menu trip.

**Ours: absent.** `grep -rn 'table-row-handle\|table-insert-affordance\|tableRowResizeHandles'
webapp crates` returns nothing. The only pointer-reachable insert is the right-click menu's
`table.insert` submenu, which does act on the pointed-at cell (see 1.9).

**Grade: UI-only** once 1.3's strips exist — the commands (`table.insert.rowAbove` /
`rowBelow` / `columnLeft` / `columnRight`) and the facade calls (`insertRow(node, below)`,
`insertColumn(node, after)`) are all present and already wired in
`webapp/src/table_band.mjs` (`TABLE_ROW_COLUMN_ACTIONS`). Nothing new is needed below the UI.

**ONLYOFFICE `[S]`: they do not have this either.** Their `private_CheckHitInBorder` result
object carries `RowSelection`, `ColumnSelection` and `CellSelection` but no insert zone, and
there is no plus affordance in `sdkjs/word/Drawing/`. So this row is a Docs-only lead.

### 1.3 Edge-strip selection

**Docs `[K]`.** Click the strip left of a row to select the row; click the strip above a
column to select the column; drag along either strip to select several. The pointer becomes a
solid arrow pointing into the table.

**Ours: the commands exist, the pointer path does not.** `table.select.row`,
`table.select.column` and `table.select.table` are declared in `tableToolCommands` and call
`selectTableContext(node, mode)`, which sets the module-level `tableSelection = { node, mode }`
and repaints. They are reachable from four surfaces — Table ribbon band
(`data-table-select="row"` in `webapp/editor.html`), Table menu, context menu, palette — and
from **no pointer gesture on the table**.

The *painting* half is already built and good: `paintTableSelection` places one
`table-cell-selection` rect per cell from `doc.tableSelectionRects(node, mode)`, filled with
the accent at 16% with an inset ring (`webapp/src/style.css`, `.overlay
.table-cell-selection`). So a selected row *looks* right; there is simply no way to select it
by pointing.

**ONLYOFFICE `[S]`, ahead of us again.** `private_CheckHitInBorder`
(`reference/sdkjs/word/Editor/Table.js`) sets, verbatim:

```js
if (0 === nCurCell && X <= X_cell_start)        { oResult.RowSelection = true;    oResult.Border = -1; }
else if (0 === nCurRow && Y <= Y_cell_start + nRadius) { oResult.ColumnSelection = true; oResult.Border = -1; }
else if (X_cell_start + nRadius <= X && X <= X_cell_end) {
  … if (X <= X_cell_start + nSpacingShift + oLeftMargin.W) { oResult.CellSelection = true; oResult.Border = -1; }
}
```

Three zones, not two: row-select is the whole area left of the first cell (unbounded),
column-select is above the first row within 3px, and **cell-select is the cell's own left
margin** — the band between the cell border and where its text starts. We have none of them.

**Grade: facade+UI.** `tableInfo` already reports `rows`, `columns`, `row`, `column` and
`regular`; what is missing is geometry — a page-scoped query returning the strip rectangles.
No engine change.

### 1.4 Drag to reorder a row or column

**Docs `[K]`.** Select a row via its edge strip, then drag the strip to a new position; the
row moves and a drop indicator shows where it will land. Same for columns.

**Ours: absent.** `grep -rn 'MoveRow\|move_row\|MoveColumn\|move_column' crates/` returns
nothing for tables. (`js_name = moveTableCell` is Tab-key caret navigation — `fn
move_table_cell` returns a `Caret` and applies no operation.)

**But no engine op is needed.** `js_name = sortTable` already permutes `replacement.rows`
and commits the whole thing as **one** `Operation::ReplaceTable` with node ids preserved, so
"move row i to j" is the same transform with a different permutation. For a merged table
`ReplaceTable` is in fact the only correct route, because `DeleteColumn` refuses one (1.12).
**Grade: facade+UI.**

### 1.5 Cell-range selection by drag

**This is the deepest gap in the document and the one everything else leans on.**

**Docs `[K]`.** Drag from cell A1 to cell C3 and you select the **rectangle** A1:C3 — a
nine-cell block with a visible fill. The toolbar then acts on the block: merge, shading,
borders, alignment, delete.

**Ours: there is no rectangular cell selection anywhere in the stack.**

| Layer | What exists | Anchor |
| --- | --- | --- |
| Selection crate | **one** selection type | `crates/casual-doc-selection/src/lib.rs` declares `pub struct TextSelection` and `pub enum SelectionError` and nothing else; `grep -i 'cell\|table'` over that file returns zero hits |
| Edit crate | a range **within one paragraph** | `pub struct Range { start: Pos, end: Pos }`, doc-commented as a half-open range within one paragraph |
| Facade | three **mode strings**, not an anchor/focus pair | `fn table_selection_anchors(table, row_index, col_index, mode)`: `"row"` gives `(row, row, 0, cols-1)`, `"column"` gives `(0, rows-1, col, col)`, `"table"` gives `(0, rows-1, 0, cols-1)`, and `_ => {}` |
| Webapp | the same three modes | `let tableSelection = null; // { node, mode: "row" \| "column" \| "table" }` in `webapp/src/main.js` |
| Layout | **already sufficient** | `crates/casual-doc-layout/src/hittest.rs` has `struct CellBox { left, right, top, bottom }` with `contains(x, y)` and innermost-cell routing by `CellBox::area` |

So a drag from A1 to C3 today produces an ordinary **flow-order text range**:
`updateDragSelection` in `webapp/src/main.js` sets `selection = { anchor: selection.anchor,
focus }` from `anchorAt(page, event)` and `paintSelection` highlights every line between the
two points. That includes every cell the flow passes through, not the rectangle — so
selecting a 3x3 block of a 5-column table highlights the two columns outside it as well, and
any formatting command then applies to them too.

**The one piece of good news, and it changes the cost of this row.** The merge *mathematics*
already accepts an arbitrary rectangle:

```rust
let replacement = merge_regular_table_selection(original, r0, r1, c0, c1, &mut self.edit_ids)
```

`js_name = mergeTableSelection` computes `(r0, r1, c0, c1)` from the mode string and the
caret and then calls that. **The engine can merge any rectangle; the facade exposes only
three degenerate ones.** This is `SKILL.md` §9.4 exactly — built, and unreachable.

**Grade: engine** for the selection *type* — a table cell selection alongside `TextSelection`
in `crates/casual-doc-selection`, normalising anchor-cell plus focus-cell to `(r0,r1,c0,c1)`
across `gridSpan`/`vMerge` — and **facade+UI** for everything that consumes it.

### 1.6 A contextual surface with the selection, not a modal from a menu

**This row is a correction to the brief, and we are at or ahead of Docs.**

`docs/70` designed a right-side properties inspector. **It shipped.** `webapp/editor.html`
carries `<aside id="tablePropertiesPanel" class="side-panel table-properties-panel">`, whose
own intro paragraph reads *"Adjust the current table, row, column, and default cell spacing.
Changes apply automatically."* — so it is a live inspector, not an OK/Cancel dialog. It is
kept in sync with the caret: `updateToolbar` in `webapp/src/main.js` ends with

```js
if (!tablePropertiesPanel.hidden) {
  if (!tableInfo?.found) toggleTableProperties(false);
  else reflectTableProperties(selection.focus.node);
}
```

and every field change is applied through one `js_name = applyTableProperties` JSON patch,
committed as a single `Operation::ReplaceTable` (`serde` `camelCase`,
`deny_unknown_fields`, only present fields written, so it does not materialise defaults).

Its layout obeys `docs/63`'s shared panel contract and `docs/70`'s own figures:
`.side-panel.table-properties-panel { flex-basis: 320px; width: 320px }`, and inside
`@media (max-width: 900px)` it becomes `position: fixed; right: 8px; width: min(340px,
calc(100vw - 42px))` — a bounded right drawer rather than a canvas-crushing column. There is
no Apply/Reset footer, and an e2e spec asserts `#tablePropertiesApply` and
`#tablePropertiesReset` both have count 0.

There is also a **contextual ribbon tab**, Word-style: `id="tabTable"` carries
`class="ribbon-tab ribbon-tab-contextual"` and is enabled only in a table
(`tabTable.disabled = !inTable` in `updateToolbar`). Its panel `#panelTable` holds **19
buttons** in **6 labelled groups** — Select (3), Rows and columns (11: four inserts, three
deletes, two distributes, two sorts), Merge (2), Cell format (1), Properties (1), Style (1) —
plus a seventh unlabelled group holding the live `#tableContext` hint with
`aria-live="polite"`, whose text comes from `tableContextLabel(info)` and reads e.g.
*"3×2 table · row 1, column 1 · merged/spanned"*.

Recipe for the 19, cross-checked by `TABLE_FACES` in `webapp/src/ribbon_faces.mjs` (18
`face(...)` entries plus one `chooser("#tableStyleBtn", "table.style.")`):

```sh
awk '/id="panelTable"/,/id="panelView"/' webapp/editor.html | grep -c '<button'
```

So the brief's premise for this row does not hold: our table surface is **not** a modal
reached from a menu. Four real sub-gaps remain inside it:

- **The contextual tab never auto-activates.** `tabTable.disabled = !inTable` enables it, and
  the only auto-select in the product is the context menu's `table.cellFormat` row
  (`selectRibbonTab("table"); tableBtn.click();`). Word activates Table Layout the moment the
  caret enters a table; we require a click on the tab. Docs `[K]` has no ribbon, so this is a
  Word-parity row, not a Docs one.
- **Nothing in the ribbon reacts to a selection** — one exception, `mergeCellsBtn.disabled =
  !inTable || !tableSelection`. Everything else keys off `inTable` alone.
- **It only follows the caret, never a selection**, because there is no cell-range selection
  to follow (1.5). Docs' sidebar reflects the selected block.
- **Nine facade setters are reachable from exactly one surface — this panel.**
  `setTableAlignment`, `setTableColumnWidth`, `setTableHeaderRow`, `setTableRowHeight`,
  `setTableWidth`, `setTableIndent`, `setTableFixedLayout`, `setTableCellMargins`,
  `setTableCellSpacing` have **zero callers** in `webapp/`. Recipe:
  `grep -rIl --exclude-dir=node_modules --exclude-dir=pkg "\bsetTableRowHeight\b" webapp packages`.
  They are not dead capability — the inspector routes everything through
  `applyTableProperties` — but they are dead *surfaces*, and they are the reason "set the row
  height without opening a panel" grades **UI-only**: the method is already there and idle.
  This is the command-surface-parity defect (`docs/105` UX-004) in its table instance.

### 1.7 Keyboard

| Gesture | Docs `[K]` | Ours | Anchor | Grade |
| --- | --- | --- | --- | --- |
| Tab / Shift+Tab across cells | moves and **selects the target cell's contents** | moves, caret at **offset 0**, nothing selected | the `key === "Tab"` branch in `webapp/src/main.js` calls `doc.moveTableCell(focus.node, !e.shiftKey)` then `navToPosition(c, false)`; `fn move_table_cell` returns a caret at offset 0 | UI-only |
| **Tab in the last cell** | **appends a row** and moves into its first cell | **silent no-op** | the same branch wraps the call in `try { … } catch { }` whose comment reads *"First/last-cell boundaries are expected no-ops for this navigation slice."*; the facade throws `"no adjacent table cell"`, and `fn move_table_cell`'s own doc comment says it does not create a row | UI-only |
| Shift+Tab in the first cell | leaves the table | silent no-op, same `catch` | as above | UI-only |
| Arrow up/down crossing cells | works | works, but through a recovery path | `navCaret` comment: *"The engine still dead-ends going UP out of a table… `recoverVerticalMove` takes the engine's answer whenever it really moved and only otherwise finds the neighbouring line by hit-testing"* | — |
| Arrow left/right at a cell edge | crosses into the adjacent cell | **UNVERIFIED** — `doc.moveCaret(node, offset, "left")` is the only path and its cell-boundary behaviour was not exercised here | `navCaret` | — |
| Enter inside a cell | new paragraph in the cell | **UNVERIFIED** — no table-specific branch in the Enter path | — | — |
| Ctrl/Cmd+A in a cell | selects the whole document | selects the **cell's contents first**, the document on a second press, with a status line saying so | `selectAll` in `webapp/src/main.js` calls `doc.cellTextRange(focus.node)`; status *"Cell contents selected — choose Select All again to select the document"* | **we are ahead** |
| Any structural table edit from the keyboard alone | Docs has none either | none | — | parity |

**Tab-in-last-cell is the cheapest high-value row in this whole document**: the engine call
(`insertRow(node, true)`) and the command (`table.insert.rowBelow`) both already exist, and
the change is to replace one empty `catch` with a forward branch. It is also the single most
habitual table gesture in any word processor, so its absence is felt on the first table a
user builds.

### 1.8 Merge and unmerge

| | Docs `[K]` | Ours | Anchor |
| --- | --- | --- | --- |
| Merge an arbitrary rectangle | yes, from a dragged cell range | **no** — only a whole row, a whole column, or the whole table | `mergeTableSelection(node, mode)`; see 1.5 |
| Merge reachability | right-click, Merge cells | ribbon `#mergeCellsBtn`, Table menu, context menu, palette | `table.merge` |
| Merge refusal when nothing is selected | the item is hidden | **disabled with a reason**: *"Select a row, column, or table before merging"* | `tableToolCommands`, `disabledReason` |
| Merge in a table that already has a merge | yes | **refused**: *"merge requires a regular table"* | `merge_table_selection`: `if !table_is_regular(&original)` |
| **Unmerge** | right-click, **Unmerge cells**, shown only on a merged cell | **not reachable at all** | see below |
| Split into R x C | yes, a small dialog | yes, a dialog (`#splitCellDialog`, defaults 1 row by 2 columns) | `toggleSplitCellDialog`, `applySplitCell` |

**Unmerge is built and unreachable — `SKILL.md` §9.4 again, and this one is a one-line fix.**
The facade signature is `splitMergedCell(node, requested_rows?, requested_columns?)` with
`Option<u32>` parameters, and `fn split_table_cell_counts` opens:

```rust
if requested_rows == 0 && requested_columns == 0 {
    return split_table_cell(table, row_index, col_index, ids);
}
```

`fn split_table_cell` is a proper unmerge: it walks forward from a `VerticalMerge::Restart`
cell collecting its `Continue` rows, clears `grid_span` and `vertical_merge` across the whole
span, and back-fills the vacated cells. It is exercised by a native test in
`crates/casual-doc-wasm/src/lib.rs` that calls `split_merged_cell(&merged_node, None, None)`.

**The webapp never calls it.** `applySplitCell` has the only call site and always passes two
integers: `doc.splitMergedCell(selection.focus.node, rows, columns)`. Its own validation
requires both to be at least 1, so 1x1 reaches the engine and is refused with *"choose more
than one row or column to split the cell"*. **There is no value a user can type into the
split dialog that unmerges a cell.**

Two smaller defects in the same area:

- `#splitCellBtn`'s label is *"Split current merged cell"* but it is enabled whenever the
  caret is in a table — `tableRibbonControls` is `[...tableRibbon.querySelectorAll("button")]`
  and `updateToolbar` sets `control.disabled = !inTable` for all of them. It is not a dead
  control (splitting an unmerged cell is legitimate and is what Word does) but the label is
  wrong, and there is no state in which it offers *unmerge*.
- `tableInfo` has **no per-cell merge information** — 20 getters and not one reports
  `gridSpan`, `vMerge` or "this cell is merged"; `regular` is a whole-table boolean. So even
  a correct UI cannot decide whether to show Unmerge for the cell under the pointer without a
  facade addition.

Grades: unmerge reachability **UI-only**; correct enablement of it **facade** (one `merged`
flag on `TableInfo`); rectangular merge **facade+UI** (the math is there); merge inside an
already-merged table **engine**.

### 1.9 Delete row/column from where the user is pointing

**We are at parity here and it is worth recording, because the brief expected otherwise.**
The canvas context-menu handler resolves the click point *first* and re-anchors the caret to
it before building the menu:

```js
const anchor = anchorAt(page, event);
…
if (!preserveSelection) { selection = { anchor, focus: anchor }; tableSelection = null; drawSelection(); }
showContextMenu(event.clientX, event.clientY, contextAt(anchor, linkAt(page, event)));
```

and `contextAt` puts `table: plainTableInfo(anchor.node)` on the context, which
`tableToolCommands` then uses for every entry (`doc.deleteRow(context.anchor.node)` and so
on). So right-clicking a cell and choosing Delete row deletes **that** row.
`preserveSelection` also does the right thing: a right-click *inside* an existing selection
or table selection keeps it rather than collapsing it (`selectionContainsClientPoint` /
`tableSelectionContainsClientPoint`).

The residual gap is that delete is reachable from the pointer **only via a menu**, whereas
Docs also offers it from the edge strip's own context menu. That is subsumed by 1.3.

One smaller loss: `plainTableInfo` keeps only five of `tableInfo`'s 20 fields —
`{ found, regular, rowHeightRule, table, column }` — so the context menu cannot know the row
count and cannot, for instance, disable "Delete row" on a one-row table before the engine
refuses it.

### 1.10 Pinned header row repeating across pages

**Shipped end to end, and reachable. We are at parity with Docs — recorded so it is not
re-filed as a gap.**

- Model: `TableRowProperties::header`, doc-commented *"Repeat as a header row across pages
  (`w:tblHeader`)"* — `crates/casual-doc-model/src/v1/table.rs`.
- Layout ingest: `header: row.properties.header` becomes `FlowedTableRow.header` in
  `crates/casual-doc-layout/src/flow.rs`; carried on the chunk in `block.rs`.
- Pagination really repeats them: `fn repeat_headers_if_needed` in
  `crates/casual-doc-layout/src/paginate.rs`, called from four sites, with a header-row stack;
  the multi-column path too (`crates/casual-doc-layout/src/columns.rs`, where `let is_header =
  *header` feeds `make_row_chunk`).
- Guarded: `fn a_table_repeats_its_header_row_on_each_continuation_page` in `paginate.rs`.
- Reachable: `#tableHeaderRow` in the inspector, *"Repeat this row when the table continues
  on another page"*, written through `applyTableProperties`'s `headerRow` field.

The one sub-gap: `setTableHeaderRow` and the inspector checkbox act on **the active row
only**, and `TableInfo.headerRow` reports the active row's own flag rather than "the table has
a header row". A user cannot mark rows 1 and 2 as a repeating header in one gesture, and the
context menu offers no header-row toggle at all — one surface, not two.

### 1.11 Refusal behaviour — "never a dead control"

**Three qualities of refusal in one product, and the worst two are both on table paths.**

**Quality 1, good: a pre-emptively disabled MENU row with a stated reason.** `runEdit` catches
every facade throw and routes it through `editRefusalMessage(err, …)` to
`setStatus(…, "error")`; `webapp/src/status_policy.mjs` makes any `"error"` a 6-second
viewport toast plus a live-region announcement (`needsToast`, `toastDuration("error") ===
6000`) — deliberately not a modal, against ONLYOFFICE's 31 `Common.UI.warning` dialogs.
`tableToolCommands` ships genuinely disabled-with-a-reason entries, rendered as `title` by
`webapp/src/menu_render.mjs` (`if (entry.disabledReason) button.title = entry.disabledReason;`)
and by `webapp/src/command_menu.mjs` for the palette:

- *"This structural change cannot be tracked in Suggesting mode"* — every structural table
  edit, because the engine cannot represent a grid rewrite as a tracked change;
- *"Unavailable for merged or spanned tables"* — every column-axis command;
- *"Rows need a fixed or minimum height before distribution"* — distribute rows;
- *"Select a row, column, or table before merging"* — merge.

**Hole 1: the RIBBON states no reason at all.** `updateToolbar` sets `disabled` on the Table
band's controls and never touches their `title`, which keeps its authored text:

```js
for (const control of tableRibbon.querySelectorAll("[data-table-sort]")) {
  control.disabled = !inTable || !tableInfo?.regular;
}
```

So **sorting a table with merged cells presents as a grey button whose tooltip still says
"Sort rows ascending"** — the same capability that explains itself properly in the menu and
the palette explains nothing in the band. `#tableStyleBtn` is the only Table-band control
with a dynamic title. This is the §10 "never a dead control" rule half-kept: the control is
correctly disabled, but a disabled control with no reason is indistinguishable from a broken
one. It is a **UI-only** fix and it covers 9 of the band's 19 buttons.

**Hole 2: the table formatting path prints raw engine text.** `runNodeEdit(thunk)` — the
function behind cell shading, vertical align, cell borders, table borders, the table formula
and `commitTableProperties` — does

```js
setStatus(err?.message ?? "Table change could not be applied", "error")
```

i.e. it bypasses `webapp/src/edit_errors.mjs` entirely and puts the facade's own prose on the
status line. `edit_errors.mjs` exists precisely because the engine's vocabulary must not reach
the user, and it holds the mapped sentences (`GENERIC`, `HISTORY`, `VIEWING`, and a
pass-through for engine strings already prefixed `refused: `). So a failed cell-border change
can announce *"column width requires a regular table"*. **UI-only.**

**Hole 3: the Tab boundary is silently swallowed.** The `catch {}` in the Tab branch is the
only table gesture in the product that refuses with *nothing at all* — no status, no console
line. That is the §10 violation in its purest form, and 1.7's fix removes it by making the
gesture succeed.

**Two wording drifts found while reading this.** `#mergeCellsBtn`'s own click handler says
*"Select a table row, column, or table first"* while the menu row says *"Select a row, column,
or table before merging"* — and the button handler is **unreachable**, because
`mergeCellsBtn.disabled = !inTable || !tableSelection` means it cannot be clicked in the state
its message describes. Dead code carrying a second, divergent copy of one sentence.

**Hole 4: facade refusals are free-form unlocalised English.** Every mutating table method
returns `Result<EditResult, JsValue>` where the error is `to_js(String)` — a thrown JS
`Error` with a prose message, no code and no i18n key. The strings, verbatim: `"caret is not
inside a table"`, `"table not found"`, `"column width requires a regular table"`, `"column is
outside the table"`, `"merge requires a regular table"`, `"select at least two cells to
merge"`, `"unknown table selection mode"`, `"sorting requires a regular table"`, `"sort
direction must be ascending or descending"`, `"formulas require a regular table"`, `"formula
range contains no numeric cells"`, `"row distribution requires at least two rows"`, `"row is
outside the table"`, `"no table property changes"`, `"no adjacent table cell"`. A host
embedding us cannot branch on any of them without string matching, and none can be
translated. **Grade: facade.**

**Hole 5: silent clamping.** `set_table_column_width_at` does
`width_twips.clamp(1, 31_680)` and `set_table_row_height` clamps to `0..=31_680`; an
out-of-range request **succeeds at a different value** rather than saying so. Same in
`insertTable`, where rows are clamped to 1..50 and columns to 1..20.

### 1.12 The merged-table cliff — a gap the brief did not list, and the largest single cause of "pathetic"

`fn table_is_regular(table)` in `crates/casual-doc-wasm/src/lib.rs` returns false for **any**
table in which one cell has `grid_span > 1` or any `vertical_merge`. The moment a user merges
two cells anywhere, all of the following stop working on that table:

| Capability | Where it is gated |
| --- | --- |
| Column resize handles disappear entirely | `fn table_column_resize_handles`: `if !table_is_regular(t) { return Vec::new(); }` |
| Insert column left/right | `Operation::InsertColumn`'s apply arm calls `ensure_regular_table(t)?` in `crates/casual-doc-edit/src/lib.rs` |
| Delete column | `Operation::DeleteColumn`, same `ensure_regular_table` |
| Select column | `tableToolCommands` `selectSubmenu`, `enabled: regular` |
| Distribute rows, distribute columns | `distribute_table_rows` / `distribute_table_columns` |
| Sort ascending/descending | `"sorting requires a regular table"` |
| Table formula | `"formulas require a regular table"` |
| Any further merge | `"merge requires a regular table"` |
| Column preferred width in the inspector | `#tableColumnWidthNote`: *"Available for regular, unmerged tables."* |

That is **nine capabilities lost to one merge**, and a merged header cell is the first thing
most people do to a table. Docs `[K]` has no such cliff: merging a cell does not remove
column resizing, insertion, deletion or sorting. Word does not either.

The refusal wording is honest and the controls are properly disabled-with-a-reason, so this
is not a dead-control defect — it is a **capability** defect, and it is the one place in this
document where real engine work is unavoidable: `InsertColumn`/`DeleteColumn` need
grid-span- and `vMerge`-aware implementations (or a documented `ReplaceTable` fallback), and
`table_column_resize_handles` needs to reason about grid columns rather than cell indices.
`fn cell_grid_start` already converts a cell index to a grid column across spans and is the
right seam. **Grade: engine** for the column ops; **facade+UI** for resize, sort and select,
which need only grid-column arithmetic.

### 1.13 Per-interaction cost — the O(1) rule, and where we break it

`docs/107` §4 makes per-interaction work O(1) in document size an owner constraint, and
`SKILL.md` §8 restates it. **Every table facade call in the product is O(document), and the
resize-handle query is worse.**

The mechanism, from `crates/casual-doc-edit/src/lib.rs`:

```rust
pub fn locate_table_row(document: &Document, node: NodeId) -> Option<(NodeId, u32, TableRow)> {
    // walk: for every block -> for every row -> for every cell -> block_contains + recurse
    //       and row.clone() on the hit
    surface_block_lists(document).into_iter().find_map(|blocks| walk(blocks, node))
}
```

`locate_table_row`, `locate_table_cell` and `find_table` are three independent full-document
walks, and `tableInfo`, `tableSelectionRects`, `mergeTableSelection`, `splitMergedCell`,
`calculateTableFormula` and `moveTableCell` each call **all three** — so one `tableInfo` is
three document walks plus a `TableRow::clone()`.

On top of that, `LayoutSnapshot::cell_rect` in `crates/casual-doc-layout/src/hittest.rs` is

```rust
for page in &self.layout.pages {
    for placed in &page.placed {
        if let Some(rect) = find_cell_rect(&placed.fragment, …, node) { return Some((page.number, rect)); }
    }
}
```

and `fn table_column_resize_handles` calls it **once per (row, column) pair**:

```rust
for row in &t.rows { for col in 0..cols - 1 { … layout.cell_rect(anchor) … } }
```

**Derived cost, with the recipe being "read those two functions":** painting the handles for
an R x C table costs `R * (C-1)` full scans of the resident page tree, plus a fresh
`LayoutSnapshot::new(self.painted_layout())` per call. A 20x5 table is **80** page-tree
scans. `paintActiveCell` adds one more (`doc.cellRect(focus.node)`), and `updateToolbar` adds
a `tableInfo` (three document walks). All of it runs inside `drawSelection`, which appears
**64** times in `main.js` — recipe: `grep -c 'drawSelection()' webapp/src/main.js` —
including on every caret move and every applied edit.

This is a defect by the stated rule regardless of how fast it feels on the demo document,
and it is a **blocker for 1.1**: a hover-driven table chrome fires on pointer-move, so it
would multiply this cost by the frame rate. `webapp/src/table_band.mjs`'s own doc comment
claims *"each handler reads the caret and nothing else, so activation is O(1) in document
size"* — true of the handler, false of the facade call it makes. **Grade: facade.**

### 1.14 Touch

The owner has said mobile is supported. The state of table editing on touch:

- **The column handle is the only table gesture that works by touch at all.** It sets
  `touch-action: none` and uses pointer events, so a touch drag resizes a column — but the
  target is **10px wide**, against WCAG 2.5.8 Target Size (Minimum) of 24 by 24 CSS px and
  2.5.5 (Enhanced) of 44 by 44. Those are the W3C figures, not a Docs claim.
- **Nothing in the webapp knows what a touch is.** Recipe:
  `grep -rn 'pointerType' webapp/src/` returns **zero hits**. So there is no enlarged hit
  zone, no long-press affordance and no touch-specific handle anywhere in the product, for
  tables or anything else.
- The handle is also invisible at rest — `.overlay .table-col-resize-handle::after` is
  `opacity: 0` and only `:hover` raises it — and **a touch device has no hover**, so on touch
  the affordance is both too small and never shown.
- **The compact ("Docs-shaped", mobile) toolbar carries no table commands at all.** Recipe:
  `grep -c 'table\.' webapp/src/compact_toolbar.mjs` returns 0. In compact mode the ribbon is
  hidden and the menu bar is the axis (`docs/122`), so the Table *menu* is still reachable —
  but the quick-access toolbar a phone user actually sees offers nothing for tables.
- Everything else (select, insert, delete, merge, reorder) is reachable on touch only through
  the Table menu or the long-press context menu.
- ONLYOFFICE `[S]` is **worse**: `CTable.prototype.IsTableBorder` opens
  `if (true === this.DrawingDocument.IsMobileVersion()) return null;` — they disable table
  border hit-testing on mobile outright. So every touch affordance in §4 is a lead over them,
  not a catch-up.

Each design in §4 therefore states its touch gesture explicitly. **Grade: UI-only**, plus
design.

### 1.15 Other capability built and unreachable

Found while grading the rows above; all are `SKILL.md` §9.4 instances — the model has the
field, the layout honours it, and nothing in the product writes it.

| Capability | Model | Layout consumer | Writer | Grade |
| --- | --- | --- | --- | --- |
| `w:cantSplit` — "allow row to break across pages" | `TableRowProperties::cant_split` | `can_split: !row.properties.cant_split` in `flow.rs`; honoured in `paginate.rs`, guarded by `fn a_cant_split_row_taller_than_the_remaining_space_moves_whole` | **none** — `grep -rn cant_split crates/` returns 11 `.rs` lines, all import, export, RTF, model, the one flow read and the one layout test | facade+UI |
| `w:tblLook` — Header Row / Total Row / First Column / Banded Rows toggles | `TableLook { first_row, last_row, first_column, last_column, no_h_band, no_v_band }` | `fn active_table_regions` in `crates/casual-doc-layout/src/cascade.rs` gates every `TableStyleRegion` on `look.*` and `cnf.*` | **none** — `applyTableProperties` does not patch `look`; every `.look` reference outside import/export is a read | facade+UI |
| `w:cnfStyle` per row/cell | `TableRowProperties::conditional_format`, `TableCellProperties::conditional_format` | `pub fn union_cnf` plus the cascade | **none** outside import | facade+UI |
| Band *period* (`w:tblStyleRowBandSize`) | `TableProperties::row_band_size`, `col_band_size` | **none** — `grep -rn 'row_band_size\|col_band_size' crates/` hits only import, export and the model | — | **engine** |
| `w:bidiVisual` RTL column mirroring | `TableProperties::tbl_bidi_visual` | four sites in `flow.rs` (slot x, cell geometry, border mirroring) | **none** | facade+UI |
| Per-row `w:jc` | `TableRowProperties::alignment` | `row.properties.alignment.or(table.properties.alignment)` in `flow.rs` | **none** — only table-level `setTableAlignment` | facade+UI |
| Per-side cell margins | `CellMargins` has four sides | honoured | **partial** — `setTableCellMargins(node, marginTwips)` takes **one** value for all four, and `TableInfo.cellMarginTwips` returns `-1` when the four differ, so a non-uniform table reads as "unset" | facade+UI |
| A table anywhere but the body | `Operation::InsertTable { container, … }` | — | **hardcoded** `container: None` in `js_name = insertTable` — no table can be inserted into a cell, header, footer or text box | facade |
| Distribute rows on auto-height rows | — | — | refused with *"row distribution requires explicit row heights"* and *"all rows must use the same explicit height rule"*, although the measured heights are already in `FlowedTableRow.height` | facade |

`w:cantSplit` is the cheapest row in this table and one of the cheapest in the document: one
`applyTableProperties` field, one inspector checkbox, one context-menu toggle.

### 1.16 Every cell format applies to exactly one cell — the caret's — even when a row is selected

This is the second-largest single cause of the reported feeling, and it was not on the
brief's list.

Cell shading (`#cellShade`), vertical alignment (`#cellVAlign`), the six cell-border presets
and the table-border presets all live in the `#tableMenu` cell-format popover and all commit
through one helper:

```js
function runNodeEdit(thunk) { … thunk(selection.focus.node) … }
```

`selection.focus.node` is **the caret's paragraph**. So the sequence a user would naturally
perform — *Select row*, then *shade it grey* — shades **one cell**. The same is true of
vertical alignment and of every border preset. There is no code path that iterates the cells
of a `tableSelection`.

Docs `[K]` applies shading, borders and alignment to the whole selected block; so does Word.
The consequence for us is worse than a missing feature, because the gesture appears to work:
the row lights up with the selection fill, the user clicks a shading swatch, and one cell
changes. Nothing refuses, so nothing explains.

**Grade: UI-only.** `doc.setCellShading(node, …)`, `setCellVerticalAlign`, `setCellBorder` all
take a node, and `doc.tableSelectionRects(node, mode)` already knows which cells are in the
selection — but it returns *rectangles*, not node ids. The clean fix needs one facade query
returning the selection's cell anchor node ids (the Rust helper `fn table_selection_anchors`
already computes exactly that list and is used only to derive the rects), after which the UI
loops. So: **facade+UI** if done properly, UI-only if done by re-deriving anchors in JS. Take
the facade route — `table_selection_anchors` is already the single source of truth and a
second implementation in JS is the "two mechanisms for one rule" defect.

### 1.17 The table selection is destroyed by a left-click

`tableSelection = null` appears at 14 sites in `main.js` (recipe: `grep -c 'tableSelection =
null' webapp/src/main.js`), including in `onPointerDown` — so **any left-click on the canvas,
including a click inside the accent fill of the row you just selected, clears the selection.**
`navCaret` clears it too (correctly — arrowing out of a table used to leave it painted).

The practical consequence: after *Select row*, the only surviving routes to *Merge cells* are
the ribbon `#mergeCellsBtn`, the Table menu, the palette, or a **right**-click inside the
fill — `tableSelectionContainsClientPoint` is consulted only by the `contextmenu` handler's
`preserveSelection` expression. A user who selects a row and then left-clicks anywhere to
"confirm" it loses it silently.

Docs `[K]` keeps a cell-block selection until you click *outside* it or start typing. **Grade:
UI-only** — treat a pointer-down inside `tableSelectionContainsClientPoint` the way the
context menu already does.

### 1.18 One asymmetry worth recording, because it will look like a bug

The right-click menu's commands act on the **clicked** cell, and that is right (1.9). But when
`preserveSelection` is true — a right-click inside an existing selection — the *selection* is
kept while the commands still close over the **clicked** anchor. So right-clicking cell C
while the caret is in cell A and choosing *Insert row above* inserts above **C**'s row. That
is what Docs does too `[K]`, and it is what a user pointing at C expects; it is recorded here
so that the next reader does not "fix" it. `table.merge` is the single exception in the
opposite direction: it always uses `tableSelection.node`, never the pointed-at cell.

---

## 2. The ranked gap table

**Row ids are `TBL-nn` and are claimed by this document.** They are not tracker ids; `docs/104`
`105` `109` and `14` are deliberately untouched (§6 lists what to file).

**Ordering rule.** Ranked by *value removed per unit of cost*, so the table can be worked
straight down. "Value" is how much of the reported feeling the row accounts for; "cost" is the
grade plus the surface area. Ties break towards the row that unblocks another row.

**Cost key.** XS = under a day, one call site. S = a few days, one module. M = a lane-week.
L = a lane-month or a new type in a shared crate.

| # | id | Gap | Docs evidence | Class | Cost | Unblocks |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | **TBL-01** | Tab in the last cell is a silent no-op; Docs appends a row (1.7, 1.11) | `[K]` | UI-only | XS | — |
| 2 | **TBL-02** | Unmerge is unreachable — no value in the split dialog unmerges a cell, while `split_table_cell` is built and tested (1.8) | `[K]` | UI-only | XS | — |
| 3 | **TBL-03** | Nine of the Table band's 19 buttons ship disabled with **no stated reason**, while the same commands explain themselves in the menu (1.11 Hole 1) | §10 rule | UI-only | XS | — |
| 4 | **TBL-04** | `runNodeEdit` prints raw engine prose on the status line, bypassing `edit_errors.mjs` (1.11 Hole 2) | §10 rule | UI-only | XS | — |
| 5 | **TBL-05** | A left-click destroys a row/column selection, including a click inside its own fill (1.17) | `[K]` | UI-only | XS | TBL-08 |
| 6 | **TBL-06** | `w:cantSplit` ("allow row to break across pages") is honoured by layout and written by nothing (1.15) | Word/Docs both have it `[K]` | facade+UI | XS | — |
| 7 | **TBL-07** | Tab moves the caret to offset 0 instead of selecting the destination cell's contents (1.7) | `[K]` | UI-only | XS | — |
| 8 | **TBL-08** | Cell shading, vertical align and borders apply to **one cell** even with a row selected (1.16) | `[K]` | facade+UI | S | — |
| 9 | **TBL-09** | Column/row handles exist only for the table the **caret** is in — you must click into a table before it is manipulable (1.1) | `[S]` ONLYOFFICE hit-tests by point; `[K]` Docs too | facade+UI | M | TBL-10, TBL-11, TBL-12 |
| 10 | **TBL-10** | The column drag **widens the table** instead of moving the border; the neighbour absorbs nothing (1.1) | `[K]` | facade+UI | S | — |
| 11 | **TBL-11** | No row-boundary hit zone at all; row height is menu-only (1.1) | `[S]` `[K]` | facade+UI | M | — |
| 12 | **TBL-12** | No edge strips, so `table.select.row` / `.column` have no pointer path (1.3) | `[S]` `[K]` | facade+UI | M | TBL-13, TBL-14 |
| 13 | **TBL-13** | No hover `+` insert affordance between rows or columns (1.2) | `[K]` Docs only | UI-only *after* TBL-12 | S | — |
| 14 | **TBL-14** | No drag-to-reorder for a row or column (1.4) | `[K]` | facade+UI | M | — |
| 15 | **TBL-15** | Table facade calls are O(document) ×3 and handle painting is `R×(C−1)×O(pages)` per redraw (1.13) | `docs/107` §4 | facade | M | blocks TBL-09 |
| 16 | **TBL-16** | No rectangular cell-range selection anywhere in the stack (1.5) | `[K]` | **engine** + facade + UI | L | TBL-17 |
| 17 | **TBL-17** | Merge is limited to row/column/table although `merge_regular_table_selection` already takes `(r0,r1,c0,c1)` (1.5, 1.8) | `[K]` | facade+UI | S *after* TBL-16 | — |
| 18 | **TBL-18** | Touch: the only table gesture is a 10px handle; `pointerType` has zero occurrences in `webapp/src`; the compact toolbar has no table commands (1.14) | WCAG 2.5.5/2.5.8 | UI-only | M | — |
| 19 | **TBL-19** | The merged-table cliff: one merge removes nine capabilities (1.12) | `[K]` neither Docs nor Word does this | **engine** (column ops) + facade+UI (resize/sort/select) | L | — |
| 20 | **TBL-20** | `tableInfo` reports no per-cell merge state, so no UI can correctly offer Unmerge for the pointed-at cell (1.8) | — | facade | XS | TBL-02 quality |
| 21 | **TBL-21** | `w:tblLook` — Header Row / Total Row / First Column / Banded Rows toggles — honoured by the cascade, written by nothing (1.15) | `[K]` Docs has header-row + banding | facade+UI | M | — |
| 22 | **TBL-22** | Distribute rows refuses auto-height rows although the measured heights are in `FlowedTableRow.height` (1.15) | `[K]` Docs distributes measured heights | facade | S | — |
| 23 | **TBL-23** | The contextual Table tab never auto-activates (1.6) | Word `[K]`; Docs has no ribbon | UI-only | XS | — |
| 24 | **TBL-24** | Facade refusals are free-form unlocalised English with no code; a host cannot branch on them (1.11 Hole 4) | — | facade | M | — |
| 25 | **TBL-25** | Silent clamping: out-of-range width/height/row-count requests succeed at a different value (1.11 Hole 5) | — | facade | S | — |
| 26 | **TBL-26** | `insertTable` hardcodes `container: None`, so no table can be inserted into a cell, header, footer or text box (1.15) | `[K]` Docs allows nested tables | facade | M | — |
| 27 | **TBL-27** | `setTableCellMargins` takes one value for all four sides; `TableInfo.cellMarginTwips` returns `-1` when they differ, so a non-uniform table reads as "unset" (1.15) | `[K]` | facade+UI | S | — |
| 28 | **TBL-28** | Header-row repeat acts on the active row only; no way to mark rows 1–2 in one gesture, and no context-menu toggle (1.10) | `[K]` | facade+UI | S | — |
| 29 | **TBL-29** | Per-row `w:jc` and `w:bidiVisual` are honoured by layout, written by nothing (1.15) | — | facade+UI | S | — |
| 30 | **TBL-30** | Band *period* (`w:tblStyleRowBandSize` / `ColBandSize`) has **no layout consumer**, so a Banded Rows toggle would have to re-stamp `cnfStyle` on every row after every insert (1.15) | — | **engine** | M | quality of TBL-21 |
| 31 | **TBL-31** | The insert-table grid picker maxes at 8×10 with no numeric fallback; a 12×3 table must be grown row by row (§3) | Word has a numeric dialog `[K]` | UI-only | XS | — |
| 32 | **TBL-32** | No double-click-on-border autofit (Word and Docs both size a column to its content this way) | `[K]` | facade+UI | S | needs TBL-09 |
| 33 | **TBL-33** | No table move/drag handle at the top-left (Word has one; **Docs does not**, so this is not a Docs-parity row) | `[S]` ONLYOFFICE has `TableOutlineDr` | facade+UI | M | — |
| 34 | **TBL-34** | Column resize has no width readout during the drag, although object resize does (`object-resize-readout`) | `[K]` Docs shows none either; Word does | UI-only | XS | — |
| 35 | **TBL-35** | Handles are painted and draggable in Viewing and Suggesting; the refusal arrives only on pointer-up (1.1) | §10 rule | UI-only | XS | — |
| 36 | **TBL-36** | `plainTableInfo` keeps 5 of `tableInfo`'s 20 fields, so the context menu cannot disable "Delete row" on a one-row table before the engine refuses (1.9) | §10 rule | UI-only | XS | — |
| 37 | **TBL-37** | Column resize is never actually **dragged** by any test; only the handle's position is asserted, in `painted-layout-consistency.spec.mjs` | test gap | UI-only | S | — |
| 38 | **TBL-38** | Table formula UI (`#tableFormula`, `#tableFormulaApply`) has **zero** test coverage: `grep -rn 'tableFormula' webapp/tests/` returns nothing | test gap | UI-only | S | — |

### 2.1 The shape of the table

Counted off the `Class` column above, enumerated so the figures are checkable rather than
asserted:

| Class | Count | Rows |
| --- | --- | --- |
| **UI-only** | **15** | TBL-01, 02, 03, 04, 05, 07, 13, 18, 23, 31, 34, 35, 36, 37, 38 |
| **facade+UI** | **14** | TBL-06, 08, 09, 10, 11, 12, 14, 17, 21, 27, 28, 29, 32, 33 |
| **facade alone** | **6** | TBL-15, 20, 22, 24, 25, 26 |
| **engine** (each also needing facade and UI) | **3** | TBL-16 (the cell-selection type), TBL-19 (merge-tolerant column ops), TBL-30 (band period) |

15 + 14 + 6 + 3 = 38. Five of the six cheapest rows are UI-only; the sixth (TBL-06,
`w:cantSplit`) needs one serde field.

That distribution is the headline for planning: **the table experience is not blocked on the
engine.** Two of the three engine rows are quality refinements of rows that can ship without
them, and the third (TBL-16) is one new type in a 215-line crate.

---

## 3. Where we are ahead, with the same rigour

`docs/99` §9.6 and `SKILL.md` §9.6: understating our own position is also false. Every claim
here is anchored the same way as the gaps.

| We lead on | Evidence | Against |
| --- | --- | --- |
| **Table sorting** | `js_name = sortTable`, and it keys off **the caret's own column** (`column < 0` means "use the caret's"), not always the first — guarded by the e2e test named *"table sort keys off the column containing the caret, not always the first"* | ONLYOFFICE has **no** table sorting at all (`SKILL.md` §1). Docs sorts a table `[K]`, so this is an ONLYOFFICE lead, not a Docs one |
| **Table formulas** | `js_name = calculateTableFormula` — `=SUM/AVERAGE/MIN/MAX(ABOVE\|LEFT)`, `#tableFormula` in the inspector | Docs `[K]` has **no** table formulas in a document (only in Sheets). ONLYOFFICE has 18 functions over 4 directions (`docs/105` OO-008), so we lead Docs and trail ONLYOFFICE here |
| **Cell-scoped Select All** | `selectAll` selects the cell's contents first and the document on a second press, with the status line *"Cell contents selected — choose Select All again to select the document"*; guarded by three cases in `table-cell-context.spec.mjs` including a real `copy`-event clipboard comparison | Docs `[K]` selects the whole document immediately. Word selects the cell. We match Word and explain ourselves, which neither does |
| **Blank-area cell hit routing** | `table-cell-hit-routing.spec.mjs` — three cases on page 2 of a real fixture: the blank top of an empty value cell, a label cell's blank area, a picture-only cell's blank area. Plus the engine-side `crates/casual-doc-render/tests/table_cell_hit_routing.rs` and the `hit_test` invariant comment *"a click inside a table cell resolves to a position inside THAT cell"* | this is the class of defect that makes typing land in the wrong box. It is guarded here at two layers |
| **Insert-table grid picker** | `// -- Insert table: a hover grid picker (Google-Docs style)`, `GRID_ROWS = 8`, `GRID_COLS = 10`, live `pointermove` preview with a `10 × 3` readout, a roving-tabindex keyboard path (arrows / Home / End / ⌘Home / ⌘End / Escape), real `<button>` gridcells inside a `role="dialog"` | at parity with Docs `[K]` on the gesture and **ahead on keyboard**: the grid is fully operable without a pointer. Our gap is the missing numeric fallback above 8×10 (TBL-31) |
| **Table caption and accessibility description** | `#tableCaption`, `#tableDescription` (`maxlength="255"`), round-tripped; `webapp/src/a11y_mirror.mjs` projects every table into a real `<table>/<tr>/<td>` with `caption` and `aria-description` | ONLYOFFICE has no table alt-text authoring for documents. The a11y mirror is also how our own e2e specs read table content, which is why table behaviour is testable here at all |
| **Column-resize hit zone width** | 10px (±5px), `.overlay .table-col-resize-handle` | ONLYOFFICE `[S]`: `nRadius = GetMMPerDot(3)` — **3px**. Ours is over three times more forgiving |
| **Table border fidelity** | segmented per-interval border winners (`struct ResolvedBorderSegment { offset, length, edge }` in `crates/casual-doc-layout/src/block.rs`), conditional table-style cascade (`fn active_table_regions`, `pub fn union_cnf`), cell spacing (`fn cell_box_spacing`), bidi-visual mirroring, vertical-merge geometry, intrinsic inline-box sizing | this is genuinely deep engine work — `docs/49, 50, 89, 90, 91, 92` — and it is why so many rows above grade `facade+UI` rather than `engine`. The **rendering** of tables is strong; the **handling** of them is not |
| **Touch table resize exists at all** | `touch-action: none` on the handle, pointer events throughout | ONLYOFFICE `[S]` returns `null` from `IsTableBorder` when `IsMobileVersion()`, i.e. **no** table border interaction on mobile. Ours is too small (TBL-18) but it is not absent |
| **Refusals are toasts with reasons, not modals** | `webapp/src/status_policy.mjs` `needsToast`, `toastDuration("error") === 6000`, plus a live-region announcement | ONLYOFFICE ships 31 `Common.UI.warning` modal dialogs. Our failure mode is a banner, not a stop |
| **The properties inspector's per-field undo** | the e2e spec drives all twelve inspector fields and asserts **one undo reverts only the last field** | Word's table-properties dialog is one transaction for the whole dialog; Docs' sidebar is live like ours `[K]` |

**One thing to be careful not to overclaim.** The rendering strength above does not transfer
to the editing experience, and it is the reason the owner's report can be true at the same time
as eleven table design documents being shipped and accurate: **those eleven documents are about
layout and paint, not about the pointer.** Only `docs/70` is about interaction, and its own
deferral list (§5) says so.

---

## 4. The designs

### 4.0 Rows 1–8 need no design beyond their own row

The eight cheapest rows in §2 are each one call site, and their §1 subsections already contain
everything a lane needs. Restated here as one-line specs so the queue can be worked from the
top without reading back:

| id | The change |
| --- | --- |
| **TBL-01** | In the `key === "Tab"` branch, replace the empty `catch` with: if `forward` and the caret is in the table's **last** cell, `await runEdit(() => doc.insertRow(anchorOfLastRow, true), { gate: true })` then `navToPosition(doc.moveTableCell(focus.node, true), false)`. If `!forward` and it is the first cell, keep the no-op but say so: `setStatus("The caret is already in the first cell")`. Drives the existing `insertRow`. Gate stays `{ gate: true }` so Suggesting still refuses with its existing reason. |
| **TBL-02** | Add an `Unmerge cells` row to `tableToolCommands`, `enabled` only when the pointed-at cell is merged, `run: () => runEdit(() => doc.splitMergedCell(context.anchor.node), { gate: true })` — **no** second and third argument, which is what reaches `split_table_cell`. New command id `table.unmerge`, added to `APP_MENU_SECTIONS.table` and `TABLE_MENU_LABELS` beside `table.split`, plus the existing `#splitCellBtn` group gains no button: reachability is menu + context + palette = three surfaces, no band width spent. Correct the `#splitCellBtn` title from *"Split current merged cell"* to *"Split cell into rows and columns"*. |
| **TBL-03** | In `updateToolbar`, wherever a Table-band control's `disabled` is set from `!tableInfo?.regular` or a rule check, set `control.title` to the same string the menu uses (`"Unavailable for merged or spanned tables"`, `"Rows need a fixed or minimum height before distribution"`) and restore `authoredTitle(control)` when enabled — the helper already exists and `#tableStyleBtn` already uses it. Add a unit guard asserting no Table-band control is ever `disabled` with its authored title still showing. |
| **TBL-04** | Change `runNodeEdit`'s catch from `setStatus(err?.message ?? …)` to `setStatus(editRefusalMessage(err, { editingUnavailableReason: readOnlyReason }), "error")` — the same call `runEdit` makes. Then extend `webapp/src/edit_errors.mjs` with the table sentences, and add a guard that no `setStatus` call in `main.js` passes a raw `err.message`. |
| **TBL-05** | In `onPointerDown`, before clearing, keep the table selection when `tableSelectionContainsClientPoint(event.clientX, event.clientY)` — the function already exists and the `contextmenu` handler already uses it. Also delete `#mergeCellsBtn`'s unreachable click handler and its divergent copy of the merge sentence (1.11). |
| **TBL-06** | Add `cantSplit` (or the positive `allowRowBreak`) to `applyTableProperties`'s serde struct, one checkbox in the inspector's **Current row** section — *"Allow this row to break across pages"* — and a context-menu toggle under `table.layout`. No engine work: `flow.rs` reads `row.properties.cant_split` and `paginate.rs` honours it, guarded. |
| **TBL-07** | After `navToPosition(c, false)` in the Tab branch, select the destination cell's contents with the existing `doc.cellTextRange(c.node)` — the same call `selectAll` makes — and fall back to the collapsed caret when it reports `found: false` (a cell holding a text-box story). |
| **TBL-08** | Add `tableSelectionAnchorNodes(node, mode) -> Vec<String>` to the facade, returning `fn table_selection_anchors`'s existing list rather than the rects derived from it; then have `runNodeEdit` loop it when a `tableSelection` is live. **Do not** re-derive the anchors in JS — that is a second implementation of one rule. |

Also fix, while in the area: `setStatus(\`Selected table ${mode}\`)` in `selectTableContext`
is a raw template literal, not a `t()` call, so the one status line the table selection
produces is **not localised**.

The three designs below are the top three rows that a lane cannot build without design.

---

### 4.1 Design D-1 — the table chrome layer (TBL-09, TBL-10, TBL-11; fixes TBL-15 and TBL-35 on the way)

**The named pattern, first.** This is a **hover router over a canvas surface**, and this
repository has already written one: `webapp/src/pointer_cursor.mjs`'s own header says *"A canvas
surface needs a HOVER ROUTER — ask the engine what is under the point, decide from that, write
the answer onto the canvas."* D-1 is the second half of that same router — not only the cursor
but the *chrome*. The second pattern it needs is a **per-page spatial index** built once per
layout revision and invalidated on `dirtyPages`: plain indexing plus memoisation, which is what
turns the current `R×(C−1)` document walks into one page walk.

No third pattern is being invented.

#### 4.1.1 The one facade addition

```
tableChromeOnPage(pageNumber: u32) -> String   // JSON
```

JSON, not a flat `Vec<i32>`, because the payload must carry table **NodeIds** and the flat-ints
convention cannot; `applyTableProperties(node, propertiesJson)` already establishes JSON as this
facade's structured-payload idiom, so this is not a new shape.

```json
{ "tables": [ {
    "node": "<hex NodeId>", "rows": 3, "columns": 4, "regular": true,
    "x": 1440, "y": 2160, "w": 8640, "h": 2880,
    "colEdges": [ { "i": 0, "x": 3600, "y": 2160, "h": 2880, "outer": false }, … ],
    "rowEdges": [ { "i": 0, "x": 1440, "y": 3120, "w": 8640, "outer": false }, … ],
    "rowStrip": { "x": 1152, "w": 288 },
    "colStrip": { "y": 1872, "h": 288 },
    "cells":    [ { "r": 0, "c": 0, "node": "<hex>", "x": …, "y": …, "w": …, "h": …,
                    "gridSpan": 1, "vMerge": "none" }, … ]
  } ] }
```

All coordinates page-local twips, the same convention as every existing geometry call. One call
per **page**, for **every** table painted on it, including continuation pages of a table that
spans a break (the existing `BTreeMap<(page, col), …>` coalescing in
`table_column_resize_handles` already proves that per-page-per-column shape is right).

**It must be built from the LAYOUT, not from the model.** One walk of `page.placed`'s fragment
tree yields every `BlockFragment::TableRow` on the page with its cells and rects — which is
exactly what `find_cell_rect` already recurses through, one cell at a time. Turning that
one-cell lookup inside out is the whole perf fix:

| | today | with D-1 |
| --- | --- | --- |
| calls into the layout per table | `R × (C−1)` | 1 |
| cost of each | O(placed fragments across all resident pages) | O(fragments on this page) |
| total for a 20×5 table | 80 whole-page-tree scans | one page walk |
| document walks for `tableInfo` | 3 | unchanged (out of scope; TBL-15's remainder) |

Memoise on `(pageNumber, revision)`. `EditResult` already exposes `revision` and `dirtyPages`,
so the invalidation channel exists and nothing new is needed. **State the complexity in the doc
comment** — `SKILL.md` §8 requires it: *"O(fragments on `pageNumber`), memoised per layout
revision."*

#### 4.1.2 The one other facade addition — correct border semantics

```
moveTableColumnBoundary(node, boundaryIndex: u32, deltaTwips: i32, mode: &str) -> EditResult
```

- `mode = "border"` (the default, and what an **internal** boundary drag uses): column
  `boundaryIndex` grows by `delta`, column `boundaryIndex + 1` shrinks by `delta`, **the table's
  total width does not change**. This is the Docs behaviour `[K]` and the fix for TBL-10.
- `mode = "table"` (what the **outer right** boundary uses): column `boundaryIndex` grows by
  `delta` and the table grows with it — today's `setTableColumnWidthAt` behaviour, kept because
  it is the right answer for the outer edge.
- Both write `grid[i].width_twips` **and** every row's `cells[i].properties.width`, exactly as
  `set_table_column_width_at` already does, and commit **one** `Operation::ReplaceTable` under
  `HistoryKind::TableResize` — so one drag is one undo entry, as it is today.
- The minimum column width moves **into the facade** as one constant, `MIN_COLUMN_TWIPS = 72`
  (0.05 in, today's JS floor). Right now 72 lives in `updateTableColumnResize` and the facade
  clamps to 1 — two floors for one rule.

`setTableColumnWidthAt` stays, because the inspector's absolute **Preferred width** field needs
it. Two calls, two genuinely different questions ("move this border" vs "make this column
exactly this wide"), not two ways to do one thing.

Row height needs **no** new facade method: `setTableRowHeight(node, heightTwips, rule)` already
exists and currently has **zero webapp callers** (1.6). D-1 makes it live, which retires one of
the nine orphan setters.

#### 4.1.3 Hit zones

Resolved in JS against `tableChromeOnPage`, on `pointermove`, through the existing
`webapp/src/pointer_hover.mjs` scheduler so it is already rAF-throttled and already the place
the cursor is decided.

| Zone | Geometry | Cursor | Pointer-down starts |
| --- | --- | --- | --- |
| internal column boundary | `x = colEdges[i].x ± 5px`, `y` within `[colEdges[i].y, +h]` | `col-resize` | `moveTableColumnBoundary(…, "border")` drag |
| outer right boundary | the last `colEdges` entry with `outer: true`, ±5px | `col-resize` | `moveTableColumnBoundary(…, "table")` drag |
| internal row boundary | `y = rowEdges[i].y ± 5px`, `x` within `[rowEdges[i].x, +w]` | `row-resize` | `setTableRowHeight` drag on row `i` |
| outer bottom boundary | last `rowEdges` with `outer: true` | `row-resize` | `setTableRowHeight` on the last row |

- **±5px, not ONLYOFFICE's ±3px** — keep the tolerance we already ship
  (`.table-col-resize-handle { width: 10px; margin-left: -5px }`), because it is measurably more
  forgiving and is one of the few places we lead (§3).
- **At a crossing, the ROW boundary wins.** This is a decision, not an accident: ONLYOFFICE
  tests `Border = 0` (top) and `2` (bottom) before `3`/`1` (left/right) in
  `private_CheckHitInBorder`, so a corner there resolves to the horizontal edge, and a user
  travelling along a row edge should not have the gesture change under them at every column.
  Record it in the `CURSOR_TARGETS` row's `why`.
- **Do not paint any resting chrome.** The border is already drawn by the raster; the cursor
  change *is* the affordance, which is what Docs does `[K]` and what avoids proposing a visual
  change the owner has not approved (`docs/63`). Hover reuses the existing
  `.table-col-resize-handle::after` accent wash; a `.table-row-resize-handle` gets the same
  treatment rotated. No new design token.

**`pointer_cursor.mjs` must gain the rows for this**, because that module throws on an
unrecognised overlay target by construction. Concretely: promote `table-row-boundary` from
`owner: "unprobed"` to `owner: "css"` with `selector: ".overlay .table-row-resize-handle"`, and
add `drag-table-row` mirroring the existing `drag-table-column`. `pointer_cursor.test.mjs` lists
the `unprobed` rows back, so this is the guard that will tell the lane it is done.

#### 4.1.4 States

```
idle ──hover a boundary──▶ armed (cursor + wash)
armed ──pointerdown──▶ dragging (guide follows, clamped)
dragging ──pointerup, |delta| ≥ 8 twips──▶ committed (one undo entry, announced)
dragging ──pointerup, |delta| < 8 twips──▶ abandoned (no edit, no message)
dragging ──pointercancel / blur──▶ abandoned
idle ──Viewing or Suggesting──▶ never armed (see refusals)
```

Two fixes inside the `dragging` state:

- **The guide spans the TABLE, not the page.** Today `startTableColumnResize` sets the preview
  height from `page.overlay.clientHeight`; use `colEdges[i].y` and `.h`. The row-axis guide is
  the same, on `rowEdges[i].x` and `.w`.
- **The guide clamps; it does not refuse.** When the shrinking neighbour reaches
  `MIN_COLUMN_TWIPS`, the guide stops moving and the pointer keeps going. A toast per pixel of
  over-drag would be noise, and the 8-twip dead zone already establishes "a movement that
  changes nothing produces nothing."

#### 4.1.5 Refusals — and TBL-35, which this fixes

Today the handles are painted and draggable in Viewing and Suggesting and the refusal arrives
only on pointer-up, which is a dead control that looks live. New rule: **the chrome layer is
armed only when the edit would be accepted.**

| Situation | Behaviour |
| --- | --- |
| Viewing mode | zone not armed; cursor stays `text`. A click is an ordinary caret placement — no message, because nothing was offered |
| Suggesting mode | zone not armed; **a click in the zone** sets `"This structural change cannot be tracked in Suggesting mode"` — the string that already exists in `tableToolCommands` — so the user learns why the gesture they expected is absent |
| shrinking neighbour at the floor | guide clamps, silently (above) |
| a merged/spanned table | until TBL-19, column boundaries are not armed; a click in the zone sets `"Unavailable for merged or spanned tables"`, the string that already exists. **Row** boundaries ARE armed, because `setTableRowHeight` has no regularity gate |

Every string above already exists in the product. **This design introduces no new refusal
wording**, which is deliberate: 1.11 found four variants of one sentence already.

#### 4.1.6 Keyboard equivalent — required, not optional

- `Alt+Shift+Right` / `Alt+Shift+Left` — widen / narrow the caret's column by one grid step
  (36 twips = 0.025 in), through `moveTableColumnBoundary(node, col, ±36, "border")`.
- `Alt+Shift+Down` / `Alt+Shift+Up` — grow / shrink the caret's row by 36 twips, through
  `setTableRowHeight(node, h ± 36, "atLeast")`.
- **The lane must re-check the chords before binding them.** Measured today:
  `webapp/src/keymap.mjs` declares 36 chords and only two use Alt — `alt+h` and
  `command+alt+a` — so `alt+shift+arrow` is free. `chord_portability.test.mjs` is the guard, and
  specs must derive labels from the `shortcutHint` fixture, never assert Mac glyphs
  (`docs/105` UX-009).
- Even without the chord the ≥2-surface floor is already met: the inspector's `#tableColumnWidth`
  and `#tableRowHeight` fields are keyboard-operable today, and `table.distribute.*` is on four
  surfaces.

#### 4.1.7 Touch equivalent — required

Hover does not exist on touch, and `grep -rn 'pointerType' webapp/src/` returns **zero** hits
today, so this is the first `pointerType` read in the product (TBL-18).

On a **tap inside a table** with `event.pointerType === "touch"`, paint a persistent pill on each
internal boundary of *that* table: **24 CSS px** across the boundary (WCAG 2.5.8 Target Size
Minimum), centred on the visible portion of the edge, one per boundary rather than one
continuous strip so two adjacent boundaries never merge into one target. The pills persist while
the caret is in that table and disappear with it — the same lifetime the column handles already
have. Drag a pill to resize; the same guide, the same clamp, the same commit.

ONLYOFFICE `[S]` returns `null` from `IsTableBorder` under `IsMobileVersion()`, i.e. they have
**no** table border interaction on mobile at all, so this is a lead rather than a catch-up.

#### 4.1.8 Accessibility

- **The result must reach AT.** On commit, announce through the existing status channel:
  `setStatus(t("table.columnWidthSet", { width: … }))` / `t("table.rowHeightSet", …)`, which
  `webapp/src/status_channel.mjs` already routes to a live region. Localised via `t()`, unlike
  the existing `Selected table ${mode}` (§4.0).
- **No new tab stops.** The chrome paints no focusable elements. Putting 40 handles in the tab
  order would flood it; the keyboard path is §4.1.6 plus the inspector. The existing overlay
  handles carry `pointer-events: auto` and no `tabindex`, which is already correct, and nothing
  focusable gains `aria-hidden` (axe `aria-hidden-focus`).

#### 4.1.9 How it is proven — and the mutation

`SKILL.md` §4: a guard that cannot fail is worse than none. TBL-37 records that **no test drags
a column today**; only the handle's resting position is asserted, in
`painted-layout-consistency.spec.mjs`.

1. **Border semantics.** Drag an internal boundary right by a known amount; assert column `i`'s
   width grew by it, column `i+1`'s shrank by it, and the **table width is unchanged** (read
   through `tableInfo` / the a11y mirror). *Mutation:* set `mode` back to `"table"` and the
   table-width assertion must go red.
2. **Row resize.** Drag a row boundary down; assert the row's `rowHeightTwips` grew and
   `rowHeightRule` became `atLeast`. *Mutation:* drop the `rule` argument and watch the rule
   assertion fail.
3. **Hover without the caret.** With the caret in a paragraph **before** the table, move the
   pointer onto a column boundary and assert the computed cursor is `col-resize`. *Mutation:*
   restore the `if (!doc?.inTable(focus.node)) return;` gate and it must go red. This is the
   guard for the row the owner's report is actually about.
4. **Complexity, not milliseconds** (`SKILL.md` §8). Build tables of R and 2R rows and assert
   the work `tableChromeOnPage` does roughly **doubles**, not quadruples. *Mutation:* reintroduce
   a per-cell `cell_rect` call and the doubling assertion must fail.
5. **Armed-only-when-accepted.** In Viewing, assert the boundary's computed cursor is `text` and
   a click there places a caret; in Suggesting, assert the click produces the existing
   Suggesting reason. *Mutation:* arm the zone unconditionally and both must go red.

Note for the lane: `table-editing-ux.spec.mjs` is on the known-flaky list under worker
contention (`SKILL.md` §6) — re-run any failure in isolation before calling it a regression.

---

### 4.2 Design D-2 — the table gutter: edge strips, hover insert, drag reorder (TBL-12, TBL-13, TBL-14)

**The named pattern.** A **gutter / header strip** — the spreadsheet row-and-column-header
idiom — plus **drag-and-drop with a drop indicator**. Both are established, and the sibling
`opencalc` already has the spreadsheet version; look there before inventing one.

**D-2 needs no facade work of its own.** `tableChromeOnPage`'s `rowStrip` / `colStrip` / `cells`
already carry everything, which is why D-1 is sequenced first. The only Rust change in D-2 is
computing those two rects, which belongs in D-1's payload anyway.

#### 4.2.1 Hit zones

| Zone | Geometry | Cursor | Click | Drag |
| --- | --- | --- | --- | --- |
| **row strip** | `rowStrip.x`, `rowStrip.w` (≥ 12 CSS px, or the cell's start margin, whichever is larger), split per row at `rowEdges` | `cell` | `selectTableContext(rowCellNode, "row")` | extend across rows → a multi-row selection (**needs D-3**) |
| **column strip** | `colStrip.y`, `colStrip.h`, split per column at `colEdges` | `cell` | `selectTableContext(colCellNode, "column")` | extend across columns (**needs D-3**) |
| **insert target** | a 16px circle centred **on** a boundary, inside the strip, painted on hover only | `pointer` | `insertRow(node, below)` / `insertColumn(node, after)` | — |
| **reorder grip** | the strip of an **already-selected** row or column | `move` | — | reorder that row/column |

Cursor choices, stated because `pointer_cursor.mjs` will refuse an unnamed target:

- **`cell` for the strips.** Docs uses a custom arrow bitmap `[K]`; we have no custom cursor
  images for this and the CSS keyword whose meaning is "select a table cell" is `cell`. Adding a
  data-URI cursor would be a visual change for the owner to approve, and `cell` is honest today.
- **`move` for a reorder grip**, not `grab`. `pointer_cursor.mjs` already reserves `grab` for the
  ruler-tab idiom and records that ONLYOFFICE reserves it for the hand/pan tool; `move` is what
  this product already uses for "dragging this thing to a new position"
  (`drag-object-move`, `object-movable`).

New `CURSOR_TARGETS` rows required: `table-row-strip`, `table-column-strip`,
`table-insert-target`, `drag-table-reorder`.

#### 4.2.2 Two slices, because one of them needs D-3

- **Slice 1 (buildable now):** single-row and single-column selection by click, the insert
  targets, and reorder. All three are expressible in today's `tableSelection = { node, mode }`.
- **Slice 2 (after D-3):** drag **along** a strip to select several rows or columns.
  `tableSelectionRects(node, mode)` can only describe one row or one column, so a multi-row
  selection has no representation until D-3's type exists. Saying this now stops a lane from
  discovering it halfway.

#### 4.2.3 Reorder — the mechanism, and why no new op

`js_name = sortTable` already permutes `replacement.rows` and commits **one**
`Operation::ReplaceTable` with node ids preserved. "Move row *i* to *j*" is the same transform
with a different permutation, and for a merged table `ReplaceTable` is the *only* correct route
because `Operation::DeleteColumn` calls `ensure_regular_table`. So:

```
moveTableRow(node, fromIndex: u32, toIndex: u32) -> EditResult       // HistoryKind::TableStructure
moveTableColumn(node, fromIndex: u32, toIndex: u32) -> EditResult
```

Both are facade-level permutations of a cloned table. `moveTableColumn` must permute
`table.grid` alongside every row's `cells`, and on a non-regular table it must permute **grid
columns**, resolved through `fn cell_grid_start` — the same helper D-3 needs (§4.3.1).

**Drop indicator.** A 2px line in the accent colour at the candidate boundary, reusing
`.table-col-resize-preview`'s existing styling rather than a new class — again, no new token.

**A repeating header row keeps its flag when moved.** `w:tblHeader` is a per-row property and
Word repeats only the *leading* run of header rows, so moving a header row down simply stops it
repeating. That is a **decision, not a refusal**: the flag travels with the row and nothing is
said. Recorded so it is not later "fixed" into a refusal.

#### 4.2.4 New command ids — the only ones this document proposes

Four, because nothing in the registry moves a row or a column:

```
table.move.rowUp   table.move.rowDown   table.move.columnLeft   table.move.columnRight
```

- **Surfaces:** the existing `table.layout` ("Autofit & sort") context submenu, the application
  Table menu (`APP_MENU_SECTIONS.table` + `TABLE_MENU_LABELS`), and the palette — three
  surfaces, past the ≥2 floor.
- **Ribbon width spent: none.** They go into a menu, not a band group. §0.6's budget is
  untouched, which matters because the Table band already holds 19 controls.
- `menu_taxonomy.test.mjs` asserts set equality between the parsed `table.*` ids and
  `menuCommandIds("table")` **in both directions**, so forgetting either half fails the build —
  that is this row's own guard and the lane does not need to write it.
- The §0.3 count becomes 27 ids / 23 invocable. Re-derive it; do not carry that arithmetic
  forward (`SKILL.md` §5a).

#### 4.2.5 Refusals

Every string already exists; reuse, do not add variants.

| Situation | Behaviour |
| --- | --- |
| column strip on a merged/spanned table | strip not painted (`tableSelectionRects` returns empty for `"column"` there). A click in the region sets `"Unavailable for merged or spanned tables"` |
| insert target in Suggesting | not painted; a click sets `"This structural change cannot be tracked in Suggesting mode"` |
| insert target in Viewing | not painted; a click is an ordinary caret placement, no message |
| reorder dropped on its own index | no-op, no message — the 8-twip dead-zone rule |
| reorder a column on a merged table | armed, because `moveTableColumn` goes through `ReplaceTable` and does not need regularity — this is one place D-2 is *ahead* of the rest of the product's merged-table behaviour |

#### 4.2.6 Keyboard and touch

- **Keyboard:** select row/column is already on four surfaces; insert is already on four
  surfaces; reorder is the four new commands above, so every D-2 gesture has a keyboard route
  with no chord at all. A chord is optional and, if added, must not collide with D-1's
  `alt+shift+arrow`.
- **Touch:** a **long-press** on a row strip selects the row and paints a **44 CSS px** drag pill
  (WCAG 2.5.5) at the strip's centre; drag the pill to reorder. Insert targets are **not** hover
  targets on touch — paint them at 24px for the *selected* row/column only, so they have a
  cause. Requires the same `pointerType` read as D-1.
- **A11y:** announce a reorder — `setStatus(t("table.rowMoved", { from, to }))`. Selection already
  announces, but see §4.0: that string is not localised today.

#### 4.2.7 How it is proven — and the mutation

1. Click the row strip; assert 4 `.table-cell-selection` rects appear for a 4-column table.
   *Mutation:* make the strip resolve to the wrong row and the rect y-coordinates must diverge.
2. Click an insert target between rows 1 and 2; assert the a11y mirror gains a row **at index 1**,
   not at the caret. *Mutation:* pass the caret's node instead of the target's and the index
   assertion must go red. This is the guard that distinguishes "insert where I am pointing" from
   "insert where the caret is" — the whole point of the affordance.
3. Reorder row 3 above row 2; assert the mirrored cell text order changed and the undo label is
   a single `Undo Table structure`. *Mutation:* commit two ops instead of one and the single-undo
   assertion must fail.
4. Column strip on a merged table: assert no strip is painted and a click yields the existing
   reason. *Mutation:* drop the `regular` gate and the "no strip" assertion must go red.

---

### 4.3 Design D-3 — rectangular cell-range selection (TBL-16, and TBL-17 falls out of it)

**The named pattern.** An **anchor/focus selection over a 2-D grid, normalised to a rectangle** —
the spreadsheet selection model. In-house prior art: `opencalc` (`../sheets`), which
`SKILL.md`-adjacent notes call further ahead; and `TextSelection` in
`crates/casual-doc-selection` already has the anchor/focus + `validate` + `mapped` shape to
mirror. Nothing here is novel; the only genuinely new thing is the **merged-cell expansion
rule**, and that is a correctness requirement rather than a design choice.

#### 4.3.1 The engine change — one new type in a 215-line crate

In `crates/casual-doc-selection/src/lib.rs`, beside `TextSelection`:

```rust
/// A rectangular block of table cells, as the user dragged it: `anchor` is the cell
/// the gesture started in, `focus` the cell it is currently over. The rectangle is
/// DERIVED on demand, never stored, so a grid edit cannot leave it stale.
pub struct TableCellSelection { pub table: NodeId, pub anchor: NodeId, pub focus: NodeId }
```

with one method that is the whole reason this belongs in the engine:

```rust
/// The inclusive grid rectangle `(r0, r1, c0, c1)` this selection covers, in GRID
/// columns, expanded until it contains every merged cell it touches.
fn normalised(&self, table: &Table) -> Option<(usize, usize, usize, usize)>
```

Two rules inside it:

1. **Cell index to grid column.** A row's `cells[i]` is not grid column `i` once any cell carries
   `gridSpan`. `fn cell_grid_start(row, col_index)` in `crates/casual-doc-wasm/src/lib.rs`
   already does this conversion and **must move down** into the shared crate, because
   `merge_regular_table_selection` and this new method have to agree by construction. That move
   is the one refactor D-3 requires.
2. **Expand to whole merged cells,** iterating to a fixed point: if a `gridSpan > 1` or
   `vMerge` cell straddles the rectangle's edge, grow the rectangle until it is contained. This
   is what Docs and Word both do `[K]`, and it is the rule that makes "merge the selection"
   always well-defined.

**Why the selection crate and not the facade.** `SelectionError`, `validate` and `mapped` live
there, and OT (ADR-033, `docs/107`) will have to transform a cell selection exactly as it
transforms a text one. A facade-local implementation guarantees a second one later — the "prefer
one mechanism over two" rule (`SKILL.md` §8).

#### 4.3.2 The facade additions

```
setTableCellSelection(anchorCellNode, focusCellNode) -> TableSelectionInfo
        // { found, table, r0, r1, c0, c1, cells, expanded }
tableCellSelectionRects(anchorCellNode, focusCellNode) -> Vec<i32>   // stride 5, as today
tableSelectionAnchorNodes(anchorCellNode, focusCellNode) -> Vec<String>
mergeTableCellRange(anchorCellNode, focusCellNode) -> EditResult
```

- The existing three-mode `tableSelectionRects(node, mode)` and `mergeTableSelection(node, mode)`
  are **left untouched**, so nothing regresses and the row/column/table paths keep their guards.
- `mergeTableCellRange` calls `merge_regular_table_selection(original, r0, r1, c0, c1,
  &mut self.edit_ids)` — **the existing helper, unchanged**. That is TBL-17, and it is a handful
  of lines, because the engine already accepts an arbitrary rectangle and the facade simply never
  offered one (1.5).
- `tableSelectionAnchorNodes` is the same query TBL-08 needs; ship one, not two.
- `expanded: true` tells the UI the rectangle grew, so it can say so rather than appearing to
  select more than was dragged.

#### 4.3.3 The UI

`tableSelection` becomes one shape with a fourth mode, so `paintTableSelection`, the 14
`tableSelection = null` sites and `tableSelectionContainsClientPoint` all keep working:

```js
// { mode: "row" | "column" | "table" | "cells", node, anchorCell?, focusCell? }
```

**The gesture.** In `updateDragSelection`, before its text branch: if pointer-down landed in a
table cell and the current point resolves to a **different cell of the same table**, switch the
gesture from text-drag to cell-drag — discard the text selection, set
`{ mode: "cells", anchorCell, focusCell }`, and paint from `tableCellSelectionRects`.

- **The threshold is the first crossing of a cell boundary, not a pixel distance.** A drag inside
  one cell selects text; crossing a border switches to block selection. That is the Docs rule
  `[K]` and it is what keeps both gestures reachable from one drag with no modifier.
- **Crossing out of the table clamps to the edge cell** — reuse the clamping `updateDragSelection`
  already applies at a story boundary (`pointerGesture.runningBand`, `pointerGesture.objectNode`),
  rather than writing a second clamp.
- The painted fill is the existing `.table-cell-selection` class. No new visual.

#### 4.3.4 States

```
pointerdown in a cell ─▶ text-drag
text-drag ──crossed a cell border, same table──▶ cell-drag   (text selection discarded)
cell-drag ──moved back into the anchor cell──▶ cell-drag with a 1×1 rectangle
             (NOT back to text-drag: a gesture that changes kind twice is unpredictable)
cell-drag ──pointer leaves the table──▶ cell-drag, clamped to the edge cell
cell-drag ──pointerup──▶ "cells" selection, announced
```

The one-way transition is a decision: once the gesture has become a block selection it stays
one for the rest of the drag.

#### 4.3.5 Refusals and one required rewording

| Situation | Behaviour |
| --- | --- |
| merge a rectangle in a table that already has merges | `"Unavailable for merged or spanned tables"` — existing string, the `table_is_regular` gate (TBL-19) |
| the rectangle expanded to contain a merged cell | **not** a refusal: `setStatus(t("table.selectionExpanded"))` — *"Selection expanded to include a merged cell"* |
| a 1×1 rectangle with merge invoked | merge stays disabled, **but the existing reason must change** |

**The required rewording.** `"Select a row, column, or table before merging"` enumerates exactly
the three modes that will no longer be the only ones. It becomes **"Select two or more cells
before merging"**, and `#mergeCellsBtn`'s unreachable duplicate (*"Select a table row, column, or
table first"*) is deleted with TBL-05 rather than reworded — one sentence, one place.

#### 4.3.6 Keyboard equivalent — required, and it changes existing behaviour

- With the caret in a cell, `Shift+Arrow` that **would cross a cell boundary** starts a cell
  selection anchored at the caret's cell — the keyboard mirror of the pointer rule.
- Within a cell, `Shift+Arrow` keeps extending the text selection exactly as today.
- **This changes behaviour that exists**, so it needs its own guard: `Shift+Down` on the last
  line of a cell currently leaves the cell through `recoverVerticalMove`
  (`webapp/src/caret_navigation.mjs`), and `navCaret`'s own comment records that the engine
  *"dead-ends going UP out of a table"*. A spec must prove the in-cell text path still works
  before and after.
- `Ctrl+Shift+Space` (Word's Select Table) maps to the existing `table.select.table`, which is
  already on four surfaces — so no new chord is strictly needed.

#### 4.3.7 Touch equivalent

**Honest gap in this design:** there is no touch text-selection grip in the product to build on.
`grep -rn 'selection-grip\|selHandle\|selectionHandle' webapp/src/` returns **zero** hits, so the
two-handle drag idiom a phone user expects does not exist for text either. D-3's touch path is
therefore: **long-press a cell to anchor, then drag to extend**, with a 44px pill at the
rectangle's bottom-right corner to adjust it afterwards. Building the general text-selection
grips is a larger row than this design and is **not** claimed here — it is listed in §7 as
deliberately out of scope, and D-3 must not be called done on touch until it exists.

#### 4.3.8 Accessibility

- Announce the block on pointer-up: `setStatus(t("table.cellsSelected", { rows, columns }))` —
  *"Selected 3 rows by 2 columns"*.
- **Mark the selection in the a11y mirror.** `webapp/src/a11y_mirror.mjs` already projects every
  table into a real `<table>/<tr>/<td>`; set `aria-selected="true"` on the mirrored `<td>`s in
  the rectangle. That is the channel by which assistive technology learns the block exists, and
  without it a drag-only block selection is invisible to AT — which by `SKILL.md` §10 means not
  done.

#### 4.3.9 How it is proven — and the mutation

1. Drag A1→C3 in a 5-column table; assert **9** `.table-cell-selection` rects and **zero**
   `.highlight` rects. *Mutation:* remove the cell-boundary switch and the `.highlight` count
   assertion must go red — that is the current behaviour, so the test is written against the bug
   first and must be seen failing.
2. Apply shading with the block selected; assert **9** cells shaded (through the mirror).
   *Mutation:* revert `runNodeEdit` to `selection.focus.node` and it must drop to 1.
3. Merge A1:C3; assert one cell with `gridSpan 3` and two `vMerge` continuations, and **one**
   undo entry. *Mutation:* pass the row mode instead and the span must come out wrong.
4. Drag a rectangle whose edge cuts a merged cell; assert `expanded === true`, the rectangle
   grew, and the status line said so. *Mutation:* skip the fixed-point iteration and the
   rectangle must come out too small.
5. `Shift+Right` within one cell still extends a **text** selection. *Mutation:* make the cell
   switch unconditional and this must go red — the guard that stops D-3 from breaking ordinary
   typing.

---

## 5. What the eleven existing table documents now get wrong

All eleven shipped — there is no abandoned table design among `docs/49, 50, 70, 72, 73, 74, 75,
89, 90, 91, 92`. What has gone stale is the **deferral prose**: seven of the eleven still tell a
reader that something is deferred, refused or unsupported which the code now does. That matters
because a deferral is how the next lane decides what to build, and a stale one either duplicates
finished work or hides a capability nobody knows is there.

**Nothing below is edited by this document.** Each row states the sentence and the anchor that
disproves it, and §6 files them.

| Doc | The sentence that is now false | What the code does | Anchor |
| --- | --- | --- | --- |
| **49** | *"This slice does not add table-style cascade, cell spacing, bidi/alignment, floating tables, or styled/segmented border paint."* | **all five** have since landed | `fn active_table_regions` (`cascade.rs`), `fn cell_box_spacing` (`block.rs`), `tbl_bidi_visual` in `flow.rs`, `crates/casual-doc-layout/src/table_float.rs` (24 fns), `struct ResolvedBorderSegment` (`block.rs`) |
| **49** | *"Differently styled side segments within one vertical merge continue to use the restart cell's side appearance."* | `docs/50` §6 explicitly reversed this — a vertical-merge restart copies the closing edge **and its resolved segments** from the final continuation | `struct ResolvedBorderSegment { offset, length, edge }` |
| **50** | deferral: *"non-zero cell-spacing conflict behavior"* | shipped as `docs/92` | `fn cell_box_spacing`, `CellBoxSpacing` |
| **50** | deferral: *"table-style/conditional-format border cascade"* | shipped as `docs/89` | `cascade.rs` `TableStyleLayer.table_borders` / `.cell_borders` |
| **70** | *"split into five groups"* (the Table band) | **six** labelled groups plus a context hint — `Style` was added, and `Rows & columns` absorbed distribute and sort, both of which doc 70 deferred | `sed -n '/id="panelTable"/,/id="panelView"/p' webapp/editor.html \| grep rgroup-label` |
| **70** | *"The ribbon stays one row and horizontally scrolls at narrow widths."* | it does **not** scroll — groups collapse into the shared `⋯` overflow, and `ribbon-width-budget.spec.mjs` treats any band hscroll as a **failure** | `ribbon-width-budget.spec.mjs`; `docs/64` |
| **70** | deferrals: *"distribute rows/columns, multi-count split-cell UI, styles gallery, sort, formulas, captions, alt-text authoring"* | **seven of the eight shipped.** Only *"arbitrary rectangular drag selection"* has not — which is TBL-16, the deepest row in §2 | `data-table-distribute`, `#splitCellRows`/`#splitCellColumns`, `#tableStyleMenu`, `data-table-sort`, `#tableFormula`, `#tableCaption`, `#tableDescription` |
| **72** | *"vertical and multi-row subdivision remains explicitly rejected until row insertion/removal can be made container-safe."* | multi-row split shipped: `split_table_cell_counts` accepts up to 20 rows and calls `split_cell_rows_phase` when `rows > 1`, exercised by the e2e case *"split an ordinary cell into a rows x columns grid (Word Split Cells)"*. What is **still** refused is far narrower — splitting a **vertically merged** cell | `fn split_table_cell_counts`; the refusal string `"splitting a vertically merged cell is not supported"` |
| **73** | *"It sorts by the first cell's plain text using Unicode case-folded lexical order"* | it sorts by **the caret's own column** (`column < 0` means "the caret's"), and there is a guard named for exactly that. Two smaller drifts: the comparison is `to_lowercase()`, not Unicode case-folding; and there is an undocumented refusal `"sorting requires at least two data rows"`. The doc also omits `HistoryKind::TableStructure` | `js_name = sortTable`; the e2e test *"table sort keys off the column containing the caret, not always the first"* |
| **89** | deferrals: *"table alignment/bidi layout"*, *"non-zero cell-spacing conflict behavior"*, *"floating-table placement"* | all three shipped (`docs/91`, `docs/92`, `table_float.rs` — the last for top-level body tables; nested and running-content positioning is genuinely still open, `docs/109` FID-L-07b) | as above |
| **89** | deferral: *"no-wrap/fit-text"* | **half** shipped — `w:noWrap` is consumed; `fit_text` has **zero** consumers (recipe: `grep -rn fit_text crates/casual-doc-layout crates/casual-doc-render \| wc -l` → 0), so that half is still honest | `fn cell_no_wrap_applies` in `flow.rs` |
| **91** | deferral: *"floating tables"* | shipped for top-level body tables | `crates/casual-doc-layout/src/table_float.rs` |

**Accurate as written:** `docs/92`, `docs/74`, `docs/75`. `docs/90`'s four remaining deferrals
(natural-size media probing, chart/OLE parsing, anchored floats in table width, Word's full
preferred-width negotiation) could not be disproved and are marked **UNVERIFIED** — `docs/105`
OO-014 independently records charts as undrawn, which is consistent with them still standing.

**`docs/91` is the best-maintained of the eleven** and shows the habit that would have prevented
the rest: it carries its own correction inline — *"Cell spacing subsequently landed in
`P1F-TBL-CELL-SPACING` (doc 92)"*. A deferral list that names the doc that later closed each item
does not rot.

### 5.1 Three tracker rows about tables are stale in the other direction

Reported here rather than edited, per the brief. **`docs/109` is the live queue** per its own
banner; `docs/104` is archived and holds no open table row.

| Row | What it says | Why it is false now |
| --- | --- | --- |
| **UX-008** (`105`, `109` row 102) | *"`insert.table` is two different products behind one id"* — ribbon a grid picker, menu/palette a silent 3×3 | Fixed in code: `id: "insert.table"` has `run: () => insertTableBtn.click()`, so every surface opens the **same** grid picker. Its `104` twin HF-148 is already marked Fixed (#528); `105`/`109` were never updated |
| **UX-012** (`105`, `109` row 110), first half | *"No Table menu on the menu bar"* | `APP_MENU_SECTIONS.table` exists with 19 ids and `menu_taxonomy.test.mjs` asserts set equality with the real command set **in both directions** |
| **UX-012**, second half | *"the palette hides table commands on complex tables"* | It does not hide them — it shows them **disabled with the reason** `"Unavailable for merged or spanned tables"` (the `columnsReason` ladder in `tableMutation`) |
| **UX-015** (`105`, `109` row 111) | lists the table style gallery as single-surface | `table.style.none` is in the Table menu, and it and the generated `table.style.<name>` rows are in the palette |
| **FID-L-13** | `105` says Open; `109` row 116 says Partly fixed with the `noWrap` evidence | cross-tracker drift; `109` is the authority. `fitText` and cell `textDirection`-as-rotation are genuinely still open (`text_direction` is read **only** as a no-wrap exemption and rotates nothing) |

Three `docs/109` rows are genuinely open and directly relevant to this document:

- **HF-165** — arrow keys skip a whole table whose cells lie outside the current column, because
  `move_vertical` prefers candidates whose cell x-range contains the affinity *globally*. Related
  to 1.7's arrow-key row and to `navCaret`'s `recoverVerticalMove` patch.
- **HF-204** — a page break inside a table cell is refused where Word splits the row.
- **FID-L-23** — the line breaker splits a line that fits its measure exactly, so **a cell can
  wrap one line short of its own intrinsic width**. That is a table-visible defect that no amount
  of interaction work will fix.

---

## 6. Rows to file — the owner applies these centrally

Per the brief, this document does not edit `docs/104`, `105`, `109` or `14`. The rows to file:

**New rows (interaction):** TBL-01 … TBL-38 from §2. If they are filed as a single themed row
rather than 38, the three that must survive individually are **TBL-01** (Tab appends a row),
**TBL-02** (unmerge unreachable) and **TBL-16** (cell-range selection), because the first two are
XS and the third gates six others.

**Close or amend as already fixed:** UX-008, UX-012 (both halves), UX-015's table-gallery clause;
and reconcile FID-L-13 between `105` and `109`.

**Doc corrections (§5):** `docs/49` compatibility boundary and its final paragraph; `docs/50`
deferrals 3 and 4; `docs/70`'s group count, its ribbon-scroll rule, and seven of its eight
deferrals; `docs/72`'s follow-up paragraph; `docs/73`'s opening sentence plus the two drifts and
the missing refusal; `docs/89`'s three shipped deferrals and the half-shipped fourth; `docs/91`'s
floating-table deferral. **`docs/70` is the one that matters most**, because it is the only
interaction document of the eleven and its deferral list is what a lane would read before
starting any of §4.

**Engine rows worth their own entry:** `Operation::SetTableRowProperties` (every row-level change
today clones and replaces the whole table through `ReplaceTable` — an O(table) undo payload for a
one-flag change, which will matter for OT granularity, ADR-033); merge-tolerant
`InsertColumn`/`DeleteColumn` (TBL-19); a layout consumer for
`row_band_size`/`col_band_size` (TBL-30).

**Test rows:** TBL-37 (nothing drags a column) and TBL-38 (the table formula UI has zero
coverage — `grep -rn 'tableFormula' webapp/tests/` returns nothing, so the whole of `docs/75`'s
UI is unexercised in the webapp suite).

---

## 7. What this document deliberately leaves out

- **Any code change.** Every code domain was occupied when this was written: `webapp/**` by SDK
  Phase 3, `webapp/src/drafts.mjs` by version history, `crates/casual-doc-wasm` by a container-set
  lane, and `casual-doc-edit|model|layout|import|export|odf` by a footer-field lane. §4 is
  buildable as written; none of it was built here.
- **Table *rendering* fidelity.** `docs/49, 50, 89, 90, 91, 92` own it and are strong (§3). The
  open rendering rows — FID-L-07b (nested/running-content floating tables), FID-L-21 (a ~240 twip
  bottom-edge divergence), FID-L-23 (a cell wrapping one line short) — are real and are not
  restated here beyond §5.1, because they are not what the owner's report is about.
- **Table of contents / table of figures** (`docs/109` OO-001). "Table" in a different sense.
- **Table formulas beyond the current four functions** (`docs/105` OO-008, blocked on cell
  references and ranges). An engine capability row, not an interaction row.
- **Touch text-selection grips.** D-3's touch path needs them and they do not exist for text
  either (`grep -rn 'selection-grip\|selHandle\|selectionHandle' webapp/src/` → zero hits). That
  is a general selection row, larger than this document, and D-3 explicitly must not be called
  done on touch until it lands (§4.3.7).
- **A localisation sweep.** Two unlocalised table strings were found in passing
  (`Selected table ${mode}`, and `runNodeEdit`'s raw engine message) and are filed as TBL-04 and
  a note in §4.0, but the product's wider `t()` coverage was not audited.
- **A `data-command` stamping pass on the Table band.** §0.3 found the band carries no
  `data-command`, so ribbon reachability is not machine-checkable for tables. That is `docs/105`
  UX-005's residue and belongs to that row, not to this one — but no design in §4 may assume a
  SURFACE table exists.
- **ONLYOFFICE's table UI as a target.** Their border hit-testing is ahead of ours (1.1, 1.3) and
  is cited where it informs a design, but the brief's bar is Google Docs and in most rows Docs is
  ahead of both.
- **Any visual or token change.** `docs/63` says propose, do not restyle. Every design in §4
  reuses an existing class (`.table-col-resize-handle`, `.table-col-resize-preview`,
  `.table-cell-selection`) or an existing CSS cursor keyword. The two places a design would
  benefit from a new visual — a custom strip-arrow cursor bitmap, and any resting chrome on a
  hovered table — are named as such and left for the owner.
- **A number for how much faster D-1 makes things.** §1.13 and §4.1.1 publish the *complexity*
  and the call-count arithmetic, which are derivable from the two functions cited. A millisecond
  figure would have needed a code change to instrument, and `SKILL.md` §8 asks for a doubling
  guard rather than a timing threshold anyway.

---

## 8. Every published number, and how to re-derive it

| Number | Recipe |
| --- | --- |
| 23 `table.*` ids / 19 invocable | `grep -oE '(id: \|tableMutation\()"table\.[A-Za-z.]+"' webapp/src/main.js \| grep -oE 'table\.[A-Za-z.]+' \| sort -u \| wc -l`, minus the 4 `submenu:` containers |
| 53 v1 ops / 9 table ops | `awk '/^pub enum Operation \{/,/^\}/' crates/casual-doc-edit/src/lib.rs \| grep -oE '^    [A-Z][A-Za-z0-9]* \{' \| tr -d ' {' \| wc -l` and `… \| grep -cE 'Row\|Column\|Table'` |
| 425 facade exports / 66 table-related | brace-match every `#[wasm_bindgen]` `impl` block in `crates/casual-doc-wasm/src/lib.rs` and list its `pub fn`s with any `js_name` override. A plain `grep js_name` under-counts: wasm-bindgen exports a `pub fn` without `js_name` under its snake_case name, and several `TableInfo` getters have none |
| 19 Table-band buttons | `awk '/id="panelTable"/,/id="panelView"/' webapp/editor.html \| grep -c '<button'`; cross-check `TABLE_FACES` in `webapp/src/ribbon_faces.mjs` (18 `face` + 1 `chooser`) |
| 6 labelled Table-band groups | `sed -n '/id="panelTable"/,/id="panelView"/p' webapp/editor.html \| grep -c rgroup-label` |
| 32 pointer targets, 4 of them table | `CURSOR_TARGETS` in `webapp/src/pointer_cursor.mjs`; `pointer_cursor.test.mjs` lists the `unprobed` rows back |
| 64 `drawSelection()` call sites | `grep -c 'drawSelection()' webapp/src/main.js` |
| 14 `tableSelection = null` sites | `grep -c 'tableSelection = null' webapp/src/main.js` |
| 0 `pointerType` reads in the webapp | `grep -rn 'pointerType' webapp/src/ \| wc -l` |
| 0 table commands in the compact toolbar | `grep -c 'table\.' webapp/src/compact_toolbar.mjs` |
| 0 `fit_text` layout consumers | `grep -rn fit_text crates/casual-doc-layout crates/casual-doc-render \| wc -l` |
| `cant_split` has no authoring writer | `grep -rn cant_split crates/ \| grep '\.rs:'` → 11 lines: import ×3, export, RTF, model ×2, one `flow.rs` read, one `paginate.rs` test |
| `row_band_size` has no layout consumer | `grep -rn 'row_band_size\|col_band_size' crates/ --include='*.rs'` → import, export, model only |
| 9 orphan facade setters | `grep -rIl --exclude-dir=node_modules --exclude-dir=pkg "\bsetTableRowHeight\b" webapp packages` (and the same for the other eight) → no hits |
| Handle geometry: 10px / ±5px | `.overlay .table-col-resize-handle` in `webapp/src/style.css` |
| Commit dead zone: 8 twips; UI floor: 72 twips | `finishTableColumnResize` and `updateTableColumnResize` in `webapp/src/main.js` |
| Facade clamps: 1..31 680 twips; 1..50 rows, 1..20 columns | `set_table_column_width_at`, `set_table_row_height`, `insert_table` in `crates/casual-doc-wasm/src/lib.rs` |
| Handle painting cost: `R × (C−1)` page-tree scans | read `fn table_column_resize_handles` (its `for row … for col …` over `layout.cell_rect`) together with `LayoutSnapshot::cell_rect` in `crates/casual-doc-layout/src/hittest.rs` (its `for page … for placed …`) |
| Ribbon: ~288px Home headroom, 120px floor, 1017px minimum viewport | `webapp/tests/e2e/ribbon-width-budget.spec.mjs` — grow the last `.rgroup` with a pad in 4px steps, predicate = nothing exiled **and** no hscroll; logged as `RIBBON_HOME_HEADROOM_AT_1280` |
| ONLYOFFICE: ±3px border tolerance, three select zones, no mobile border hit-testing | `reference/sdkjs/word/Editor/Table.js:3515` (`IsTableBorder`) and `CTable.prototype.private_CheckHitInBorder` (`nRadius = GetMMPerDot(3)`, `RowSelection` / `ColumnSelection` / `CellSelection`, `IsMobileVersion()`) |
| WCAG target sizes: 24×24 (2.5.8), 44×44 (2.5.5) | W3C WCAG 2.2, not a competitor claim |

**Every Google Docs behaviour in this document is tagged `[K]` and is knowledge, not a
citation** — see §0.2. There is no Docs source in this environment and it cannot be run from
here. Where a `[K]` row turns out to be wrong, the row is wrong; the anchors on our side are not.
