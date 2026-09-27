# 141 — Google Docs table experience: gap analysis and design

**Status:** Design, complete. **Opened:** 2026-09-28. **Owner:** unassigned.
**Scope:** the *interaction* quality of table editing in OpenDoc measured against Google
Docs, plus the design for the three lanes that close most of it. Docs-only: this document
changes no code.

**Why this exists.** The owner's words: *"table experience is pathetic in our platform .. we
need complete table editing experience of google docs and try to provide that experience"*.

**This is not a feature inventory.** We already ship 20 table commands plus 4 submenu
containers (§0.3), a contextual Table ribbon tab with 19 controls, a live right-side table
properties inspector, a grid-picker table inserter, table sorting and table formulas — the
last two of which ONLYOFFICE does not have at all. Listing commands again would answer a
question nobody asked. The complaint is about **affordances, directness and feedback**, and
that is what is measured here.

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

```sh
cd <repo>
grep -o '"table\.[a-zA-Z.]*"' webapp/src/main.js | sort -u        # the ids
grep -o '"table\.[a-zA-Z.]*"' webapp/src/main.js | sort -u | wc -l # => 24
```

24 ids, and the widely-quoted "24 table commands" is **four too many**: `table.insert`,
`table.delete`, `table.select` and `table.layout` are submenu containers built in
`tableToolCommands` with a `submenu:` and no `run`, so they are not commands. The honest
figures are **20 leaf table commands**, plus `insert.table` in the insert namespace = **21
table-affecting commands**. Every one of them appears in `webapp/src/command_taxonomy.mjs`
(grep `table: [`) and is therefore reachable from the contextual Table ribbon band, the
application Table menu, the canvas context menu and the command palette — four surfaces,
comfortably past the ≥2 floor (`SKILL.md` §10).

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
derived by `webapp/tests/e2e/ribbon-width-budget.spec.mjs`, which measures each band's
content width against the 1280px viewport and publishes the headroom: **Home ~288px, Insert
537px, Layout 559px**, with a 120px floor the spec enforces. The failure mode is a
**horizontal scrollbar on the band**, not the `⋯` overflow button — that button tracks
viewport width, not content width. **Do not quote the old "~55px of slack"**: it was wrong
by five-fold and was being used to reject additions (`SKILL.md` §11).

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
