# 171 — Merged table cells: one grid map, and the table chrome around it

Status: §1–§6 built on `tables/merged-cells-and-chrome` (row HF-340, #858). §7's ranked
chrome defects 3–8 are fixed on `tables/chrome-targets` (rows HF-341..HF-346, §7.1); §7.2
lists what that work found and left open (HF-347..HF-349). §8 (grips and the table move
handle) is not built.

The owner's words: *"for table our current approach fails in case of merged cells"*, and the
hover and selection action menus *"need to be seriously looked into"*.

## 1. What failed, measured

`fixtures/generated/merged-tables.docx` (generator: `crates/casual-doc-ooxml/examples/generate_fixtures.rs`,
`merged_tables_document`) holds one table per merged-cell family. Every cell's text names
the grid rectangle it covers (`r0-2c1` is rows 0–2 of column 1), so a grid read back as text
says whether a merge survived. The families:

| Table | What it exercises |
| --- | --- |
| H-MERGE | `w:gridSpan` in the middle and at both edges of a row |
| V-MERGE | `w:vMerge` restart/continue: a 3-row band in column 1, a 2-row band in column 3 |
| COMBINED | a 2×2 block (span plus vMerge) under a horizontal merge |
| EDGES | merges on the first row, the first column, the last column and the last row |
| RAGGED | short rows (`w:gridBefore`, `w:gridAfter`) |
| NESTED | an outer table with a merged top row, holding an inner table with both kinds |

`crates/casual-doc-wasm/src/table_merge_matrix_tests.rs` drives every operation through the
facade a host calls, on every VISIBLE cell of every family, and classifies each run:

- **ok**: the result is a well-formed grid, keeps the content it should, undoes exactly,
  redoes exactly, and its merges survive a DOCX save and reopen;
- **refused**: it declined with a reason and changed nothing;
- **CORRUPT**: it applied and left a table nobody meant. Examples are a row that no longer
  covers the grid, an orphan continuation, lost text, a merged cell that shrank, an undo that
  does not restore, or a round trip that changes the merges.

Run it with
`TABLE_MATRIX_OUT=<file> cargo test -p casual-doc-wasm --lib the_merged_cell_matrix`.
Before is commit `8cea072a` (the fixture, before any fix). After is this branch.

| Operation | H-MERGE | V-MERGE | COMBINED | EDGES | RAGGED | NESTED-OUTER | NESTED-INNER |
| --- | --- | --- | --- | --- | --- | --- | --- |
| delete column | 13 refused → 13 ok | 13 refused → 13 ok | 12 refused → 12 ok | 9 refused → 8 ok, 1 refused | 10 refused → 10 ok | 3 refused → 2 ok, 1 refused | 7 refused → 7 ok |
| delete row | 13 ok → 13 ok | 6 ok, **7 CORRUPT** → 13 ok | 9 ok, **3 CORRUPT** → 12 ok | 5 ok, **4 CORRUPT** → 9 ok | 10 ok → 10 ok | 3 ok → 3 ok | 5 ok, **2 CORRUPT** → 7 ok |
| drag column border | 9 refused → 9 ok | 10 ok → 10 ok | 3 ok, 5 refused → 8 ok | 6 refused → 6 ok | 8 ok → 8 ok | 1 refused → 1 ok | 3 ok, 2 refused → 5 ok |
| insert column left | 13 refused → 13 ok | 13 refused → 13 ok | 12 refused → 12 ok | 9 refused → 9 ok | 10 refused → 10 ok | 3 refused → 3 ok | 7 refused → 7 ok |
| insert column right | 13 refused → 13 ok | 13 refused → 13 ok | 12 refused → 12 ok | 9 refused → 9 ok | 10 refused → 10 ok | 3 refused → 3 ok | 7 refused → 7 ok |
| insert row above | 13 ok → 13 ok | 13 ok → 13 ok | 12 ok → 12 ok | 9 ok → 9 ok | 10 ok → 10 ok | 3 ok → 3 ok | 7 ok → 7 ok |
| insert row below | 13 ok → 13 ok | 6 ok, **7 CORRUPT** → 13 ok | 9 ok, **3 CORRUPT** → 12 ok | 5 ok, **4 CORRUPT** → 9 ok | 10 ok → 10 ok | 3 ok → 3 ok | 5 ok, **2 CORRUPT** → 7 ok |
| merge down | 9 refused → 9 ok | 9 refused → 9 ok | 8 refused → 8 ok | 6 refused → 6 ok | 5 refused → 5 ok | 1 refused → 1 ok | 4 refused → 4 ok |
| merge right | 9 refused → 9 ok | 10 refused → 10 ok | 8 refused → 8 ok | 6 refused → 6 ok | 7 refused → 7 ok | 1 refused → 1 ok | 5 refused → 5 ok |
| move column right | 9 refused → 9 refused | 10 ok → 10 ok | 8 refused → 8 ok | 6 refused → 6 refused | 8 refused → 8 refused | 1 refused → 1 refused | 5 refused → 5 ok |
| move row down | 9 ok → 9 ok | 9 refused → 9 ok | 8 refused → 8 ok | 7 refused → 1 ok, 6 refused | 7 ok → 7 ok | 1 ok → 1 ok | 4 refused → 4 ok |
| set column width | 13 refused → 13 ok | 13 refused → 13 ok | 12 refused → 12 ok | 9 refused → 9 ok | 10 refused → 10 ok | 3 refused → 3 ok | 7 refused → 7 ok |
| unmerge | 3 ok → 3 ok | 2 ok → 2 ok | 2 ok → 2 ok | 4 ok → 4 ok | n/a → n/a | 1 ok → 1 ok | 1 ok, 1 refused → 2 ok |

Totals over 710 runs: **233 ok, 445 refused, 32 CORRUPT** before, and **678 ok, 32 refused,
0 CORRUPT** after.

Every refusal left after the fix has a reason, and every one is right:

- **Deleting every column.** The caret is in a full-width merged cell (EDGES `r0c0-3`,
  NESTED `o0c0-1`). The refusal says to delete the table instead.
- **Moving a column where no column band can move.** In H-MERGE and EDGES the merges chain
  across every column, so the whole table is one band. RAGGED's short rows have no cells to
  move.
- **Moving the EDGES rows 1–3 band down.** It is already the last band.

Three behaviours outside that matrix, measured the same way:

- **Tab** entered the hidden continuation under a vertical merge, where the caret was
  invisible and typing went into a cell nobody can see.
- **Select column** returned no cells at all on any merged table.
- **The accessibility mirror** announced each continuation as an empty cell of its own and
  each merged cell as one grid cell. A screen reader's "row 2, column 3" did not match the
  page.

## 2. Root cause

**Every table feature translated between OOXML's physical cells and the grid the user sees
with its own arithmetic, and each one assumed one cell per grid column.**

A row's `cells[i]` is grid column `i` only until three things happen:

- some cell carries `w:gridSpan`;
- a row starts after `w:gridBefore`;
- a `w:vMerge` continuation sits under a merged cell.

There were at least fourteen copies of the translation, each wrong in its own way:

| Copy | What it did | What broke |
| --- | --- | --- |
| `table_cell_anchor` (wasm) | stepped by cell index | Tab entered covered continuations |
| `empty_row_like` (wasm) | copied the anchor row's `w:vMerge` verbatim | a row below a merge's top started a second merge and tore the first |
| `Operation::DeleteRow` + `delete_row` | removed the row | an orphan `continue`, and the merged cell's text, lost |
| `ensure_regular_table` + `insert_column` / `delete_column` | refused any merge | 9 capabilities lost to one merge (`docs/141` §1.12) |
| `merge_regular_table_selection` | refused any merge | a second merge was impossible |
| `split_table_cell` | scanned the vMerge chain by CELL INDEX | unmerge from the wrong cell |
| `table_selection_anchors` | `table_is_regular` gate | Select column selected nothing |
| `set_table_column_width_at`, `applyTableProperties`, `distribute_*` | wrote `cells[col]` or refused | widths on the wrong cell |
| `active_column_width` | `cells[col]` | the inspector read the wrong cell |
| `move_table_column_boundary` | refused a straddled boundary | a border a merge crosses could not be dragged |
| `table_column_resize_handles` | `table_is_regular` gate | no handles on a merged table |
| `move_table_row` / `move_table_column` | refused vMerge rows, any gridSpan | no reorder on merged tables |
| selection's `TableGrid` | joined a continuation by START column | disagreed with layout, which needs the same RANGE |
| layout: `resolve_vertical_merge_roles`, cell placement, column solver, border conflicts, `table_column_count`, `table_chrome_on_page` | own span sums, no `w:gridBefore`; chrome counted CELLS as columns | ragged rows painted from column 0; every boundary after a merged cell numbered wrong |
| `a11y_row_cells` + the mirror | one `<td>` per physical cell; selection by cell index | the screen-reader grid did not match the page |

## 3. The pattern: a normalized grid map

The pattern is the **HTML table model's "forming a table" algorithm**. A table becomes a
grid of slots, each slot points at the cell that owns it, and every question is asked of the
grid rather than of the rows.

`casual_doc_model::v1::TableGridMap` (`crates/casual-doc-model/src/v1/table_grid.rs`) is that
grid, and it is the **one** translation:

- **Per physical cell**: its first grid column, span, region, and whether it is a joined
  continuation.
- **Per region** (a cell as the user sees it): its rectangle, its anchor cell, and whether it
  restarted.
- **Lookups**:
  - `cell_at(row, column)` and `region_of`;
  - `anchor_of` (a continuation resolves to the merged cell above it);
  - `step` (Tab order, which visits each region once);
  - `regions_in_rows` and `regions_in_columns`;
  - `straddles`;
  - `is_regular` (the single definition `table_is_regular` now calls).
- **The vertical-merge rule** is layout's, because layout decides what is painted. A
  continuation joins only a restart (or a joined continuation) directly above it over exactly
  the same grid range. An orphan stands alone with its content visible.
- **`w:gridBefore` offsets** a row's cells. `w:gridAfter` widens the grid.
- **Cost and the hostile-file bound.** It never allocates a dense rows × columns array, since
  a `w:gridSpan` may legally be 16384. Building it is O(cells) and O(1) in document size.
  `table_grid_cell_visits` meters it.

The readers that now go through it:

- **Selection.** `casual-doc-selection`'s `TableGrid` is built from the map, so the merge
  rectangle and the painted cells agree.
- **Layout.** These all read the map:
  - `resolve_vertical_merge_roles` and `table_column_count`;
  - cell placement and the column solver, which start at `w:gridBefore`;
  - the border-conflict helpers;
  - `table_chrome_on_page`'s column count and edges.
- **Structural edits.** `casual_doc_edit::table_structure`: one pure `Table -> Table`
  function per edit (§4).
- **The facade.** Tab, select row/column, every width and resize command, `tableInfo`,
  resize handles, chrome boundaries, and the accessibility projection.

## 4. What each edit does

The Word, Google Docs and ONLYOFFICE 10.0.0 behaviour below was measured live against
ONLYOFFICE, or read in its `v10.0.0.169` source. Word and Docs are from knowledge. Where the
three differ, the choice is stated.

| Edit | Rule here | The others |
| --- | --- | --- |
| Insert row | Copies the anchor row's spans and formatting. A vertically merged cell the new row lands INSIDE grows over it. One it lands above or below is untouched. | ONLYOFFICE `Table.js:10305-10353`, Word: the same |
| Delete rows | A merged cell whose top row goes moves its content and properties to its first surviving row. | ONLYOFFICE promotes the continuation but DROPS the content (`Table.js:10446-10467`) |
| Insert column | A cell that straddles the new line grows to cover it, so the grid stays a grid. Every other row gets a cell, formatted like its neighbour. | Docs: the same. ONLYOFFICE and Word put the new cell beside the merged one and let rows fall out of alignment. |
| Delete columns | The caret cell's grid columns go (for a merged cell, all it spans). A partly covered cell shrinks. | ONLYOFFICE widens the range to the selected cells' spans (`Table.js:10603-10645`) |
| Merge | Any rectangle, expanded to whole merged cells. Each non-empty cell's content moves in, in reading order. A table that already has merges merges again. | All three |
| Unmerge | From ANY row of the merged cell. One cell per grid cell it covered. Content stays top-left. | All three |
| Move rows / columns | The unit is the BAND: the rows (columns) no merged cell crosses out of. A step or drop inside another band snaps past it. Nothing is ever torn. | ONLYOFFICE has no reorder. Word moves rows with Alt+Shift+Up/Down. |
| Column border drag | Every boundary moves the line between two grid columns. A merged cell over it keeps its total width. | ONLYOFFICE and Docs: the same. It used to refuse. |
| Set / distribute column width | Grid columns. Each cell carries the sum of the columns it spans (`sync_cell_widths`). | Word: the same |

**Clean-up after every edit.** `normalize_vertical_merges` removes the two degenerate
states an edit can leave: a `restart` nothing continues, and a `continue` that joins
nothing. No edit can write a `w:vMerge` that another reader would see differently.

**How each edit is committed.** A regular table still commits the fine-grained `InsertRow`,
`DeleteRow`, `InsertColumn` or `DeleteColumn`, which a concurrent edit transforms against.
An edit those cannot express (a merged table) commits one `ReplaceTable`. That is the
existing whole-table vehicle that merge, split and resize already used, under
`HistoryKind::TableStructure`, so it is one undo step.

**Recorded consequence for co-editing.** Two people editing the same MERGED table
concurrently conflict at the table, not at the cell. That is the same granularity merge
already had.

**Still deliberately refused.** Sort (as Word does, on vertically merged tables) and table
formulas still need a regular grid. Splitting a vertically merged cell into a grid of more
than one row, which `split_table_cell_counts` refuses, also stays: unmerge first.

## 5. Tab selects the destination cell (`docs/174` OL-03)

Tab and Shift+Tab move to the next visible cell in reading order (`TableGridMap::step`) and
SELECT its contents (`webapp/src/table_tab.mjs`, through the engine's `cellTextRange`), so
typing replaces them, as in Word, Docs and ONLYOFFICE.

An empty cell gets a caret. A Tab out of the last cell still appends a row. Two
`table-cell-context` specs pressed Shift+Tab and then Select All. They now collapse the new
selection first, because what they guard is "Select All starts from a caret".

## 6. Screen-reader reach

The accessibility projection emits one cell per VISIBLE cell, with `colSpan`, `rowSpan` and
`gridColumn`. The mirror sets `colspan`/`rowspan` and measures the ARIA grid's
`aria-selected` in grid columns.

## 7. Table chrome: compared with Word, Docs and ONLYOFFICE 10.0.0

Measured live in our editor and in ONLYOFFICE at `localhost:8090`. The hit-target sizes
below are ours at 100% zoom. ONLYOFFICE source citations are to `sdkjs` `v10.0.0.169`.

| Affordance | Ours | Word [K] | Docs [K] | ONLYOFFICE |
| --- | --- | --- | --- | --- |
| Column border resize | 11 px zone; **disarmed on a merged table** (fixed here) | any border; ruler markers | hover border | 7 px; works on merged tables |
| Row border resize | 7 px zone on a 19 px row | yes | yes | 7 px |
| Row select strip | 17 px; arms only after the pointer enters the table | left selection bar | yes | cursor only, unbounded left |
| Column select strip | 17 px; **removed on a merged table** (fixed here); overlaps the line above | top-border arrow | yes | yields to content above |
| Insert "+" | 16 px disc, **7 px live** | desktop: edge "+" | hover "+" (unverified) | none |
| Table move handle | none | four-arrow box, top-left | none | 13 px box, corner-flipping |
| Corner resize | none | bottom-right square | none | 6 px square |
| Row/column drag | gutter drag, one undo step | Alt+Shift+Up/Down | drag handles | none |
| Keyboard | Alt+Shift+Arrow sizes; palette and menu for all | full | partial | full |
| Screen reader | chrome has no role; commands do | UIA table pattern | a11y mode | canvas, nothing |

Defects in OUR chrome below that standard, ranked:

1. **The merge cliff in the chrome:** column zones, column strips, touch pills and the
   keyboard step all keyed off whole-table `regular`, and the band and menu disabled column
   commands on any merged table. **Fixed here.**
2. **Wrong boundary numbering on merged tables.** `table_chrome_on_page` numbered column
   boundaries by CELL index. **Fixed here** (§3).
3. **The "+" disc paints 16 px but responds over 7 px** (`insertTolerance`,
   `table_gutter_zones.mjs`). Pressing the painted lower half selects the row instead.
   **Fixed (HF-341).**
4. **Targets under 24 px for a mouse or pen** (WCAG 2.5.8): the strip (17 px), the disc,
   and both boundary zones. **Fixed (HF-342)**, with the limits in §7.2.
5. **The row strip arms only after the pointer has entered the table**, so it is invisible
   on approach from the margin, where no text lives. **Fixed (HF-343).**
6. **The column strip overlaps the paragraph line above** (14 px reach). **Fixed (HF-344).**
7. **Stale hover after a wheel scroll**: no pointermove fires, so the band stays lit.
   **Fixed (HF-345)**, and after a zoom.
8. **Structural edits from the menu say nothing**: only the disc path announces. **Fixed
   (HF-346)**, from the band, the Table menu, the context menu and the palette.

### 7.1 How 3–8 were fixed

The named pattern is the **hit-target lane** (a gutter of fixed-size targets beside the
content, as a spreadsheet's header strip or an editor's line-number gutter), with **hover
hysteresis** for the disc: the target that is painted is the target that answers.

- **Two lanes, each one target thick** (`table_gutter_zones.mjs` `gutterLanes`). Outward
  from the leading border: the select strip (24 px, 3 px of it past the border) and the
  insert lane (24 px) where the `+` discs sit on the boundaries. A finger gets 44 px for
  both. A point in the insert lane resolves to the nearest boundary within half a target.
- **The painted disc is its own hit box.** While a disc is shown its box answers first
  (`gutterAt`'s `sticky`), so on a 19 px row, where the next boundary is nearer than this
  disc's centre over part of its box, every painted pixel still inserts here.
- **The lanes live in free space measured by layout.** `tableChromeOnPage` now reports
  `clearAbove` and `clearBefore` per table (`hittest.rs` `clearance`, O(lines on the
  page)): the gap up to the nearest line box above the table over its width, and to the
  nearest line box beside it. A lane that does not fit is not drawn there. Where only the
  strip fits, discs share it on boundaries whose neighbouring bands are two targets long.
  Where not even the strip fits, it shrinks to the free space and the border: it never
  covers a line.
- **Arming on approach** is safe because of that: the row strip and its disc show as the
  pointer comes in from the margin, before it enters the table.
- **Boundary zones are ±12 px** (24 px), still clamped to a fifth of the shorter band so the
  middle 60 % of every cell places the caret, and they start where the strip's inside
  reach ends, so no pixel arms both a strip and a resize (unit-tested over a grid). The
  caret table's resting column grip is exactly the armed zone's width.
- **Scroll and zoom** re-place the hover from the last pointer position: a capture-phase
  `scroll` listener in the gutter module, and a scale check in its paint.
- **Announcements** come from one table, `TABLE_EDIT_ANNOUNCEMENTS` in `table_band.mjs`,
  read by the band, the command tree and the merge button, after the edit lands and never
  on a refusal. Nine new sentences, in all 19 catalogues.

Guards: `table-chrome-targets.spec.mjs` (six specs, each seen red under a mutation of the
code it guards; the commit lists them), `table_gutter_zones.test.mjs` (lanes, the sticky
disc, no text cover, no contested pixel), `table_commands.test.mjs` (every structural
command announces, and says nothing when refused), and
`chrome_reports_the_free_space_above_and_before_a_table` in `casual-doc-layout`.

### 7.2 Found and left open

- **HF-347: RTL tables keep the gutter on the left.** A `w:bidiVisual` table is laid out
  mirrored, so its leading edge is on the right, where Word puts the row selection. The
  lanes still measure free space on the left, so they cover nothing; they are on the
  trailing side. Needs the table's direction in the chrome payload.
- **HF-348: the column strip is under 24 px below an ordinary paragraph.** The free space
  above a table is the paragraph's spacing after plus the table's: 19 px measured under a
  heading, and 8 pt (about 11 px) under Word's default Normal paragraph. The strip shrinks to that rather than covering the
  line, and below 16 px it carries no `+` discs; column insert is then on the menu, the
  context menu, the palette and the band. Word draws its column `+` over the text above;
  we will not. A flip below the table, or a column handle inside the top border on hover,
  is the open design question.
- **HF-349: boundary zones on short rows stay under 24 px, and the touch pills are 24, not
  44.** A row border's zone is a fifth of a 19 px row either side (about 7 px), because 24
  there would take the whole row from the caret; Alt+Shift+Arrow and the menu size a row
  without aiming. The touch pills (`table_chrome.mjs` `TOUCH_PILL_PX`) predate this work.

## 8. Grips and the table move handle

The owner asked for the six-dot (⋮⋮) grip pattern, aligned with the object hover-menu lane
(`docs/172`).

- **Row grip.** A six-dot grip in the row strip, shown on hover beside the row the pointer is
  level with.
  - **Visuals and timing are shared** with the object and list grips (`docs/172`): the same
    glyph, hover delay and drop indicator (a 2 px accent line at the band boundary).
  - **Behaviour.** A drag moves the row's BAND (§4) and snaps the drop line to band
    boundaries, so a merged cell is never torn. One drop is one `moveTableRow`, one undo
    step.
  - **Keyboard.** The existing `table.move.rowUp` and `table.move.rowDown` commands.
- **Column grip.** The same grip in the column strip, moving column bands. On a table whose
  merges bind every column into one band, the grip is shown disabled, with the reason "These
  columns are joined by a merged cell".
- **Table move handle.** Word's four-arrow box at the table's top-left (ONLYOFFICE's is a
  13 px box that flips corners near the page edge, `DrawingDocument.js:134`/`:142`). Here it
  is 24 px.
  - **Shown** on hover over the table or when the caret is in it.
  - **Click** selects the table; Shift extends.
  - **Drag** moves the table as a block (cut and paste of the table node at the drop caret,
    one undo step).
  - **Keyboard and screen reader.** A `table.select.table` command already exists. The move
    gets `table.move.up` and `table.move.down` by block, and the handle carries `role=button`
    and a name.
- **Docs has no move handle.** This one follows Word.

Nothing in §8 is built. It depends on the grid map, which is now in place, and on the
shared grip contract in `docs/172`.

## 9. Guards, each seen red by mutation

| Guard | Mutation | Red |
| --- | --- | --- |
| `the_merged_cell_matrix_has_no_corruption` | insert row never continues (`inside && false`) | "r0-2c1 shrank from 3x1 to 1x1" (8 cases) |
| same | delete rows drops the merged content | "lost text" on 4 families |
| `tab_visits_every_visible_cell_once_in_every_family` (expected list read from the FILE, not the map) | continuations never join | "V-MERGE: Tab does not end" |
| `table_structure` unit tests (11) | written red-first, with the edit pinned | each asserts the exact grid as text |
| `structural_edits_are_linear_in_the_table` | n = 200 vs 400 rows | grid work ≤ 2.1× |
| `table-merged-cells.spec.mjs` (9 specs) | six engine mutations in one build (no continuation, no content move, no straddle widening, no a11y continuation filter, column select returns nothing, Tab by cell index) | the 8 specs written first all red, e.g. `r0c1-2:c2` for `r0c1-2:c3`, rowspan 3 for 4, `<w:vMerge/>` 1 for 2 |
| same, border-drag spec | written before the chrome numbering fix | red ("the border inside the merged table is armed": no zone), which is how §2's last layout row was found |
| same, Tab spec | the shell places a caret instead of selecting | "Xr1c2" for "X" |

## 10. Deliberately not done

- **Layout chrome under `w:gridBefore`.** `table_chrome_on_page` still counts columns from
  a row's first painted cell, because the layout fragment carries no `w:gridBefore`. Layout
  painting honours it now.
- **`split_table_cell_counts` keeps its own span arithmetic** for splitting into counts. It
  refuses every case the grid map would answer differently.
- **Sort and formulas on merged tables** still refuse, with their existing reasons.
- **§8** is the next PR on this lane; §7.2 lists what the chrome work left open.
