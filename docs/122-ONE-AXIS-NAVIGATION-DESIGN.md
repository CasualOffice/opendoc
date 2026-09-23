# 122 — One navigation axis per chrome

**Status:** Implemented. **Opened:** 2026-09-23. **Closes:** `109` UX-014, `109` HF-097.
**Adds:** `109` HF-179 (found by the new reachability guard, not caused by this change).

## 0. What the owner asked for

> "also work on ribbon view .. as we see two menues option one file, edit, view .. and one
> with home insert layout.. once check onlyoffice handles it.. becasue i think they have
> file, home, ... lets adopt thats.. much cleaner and friendly .. and less ocnfusing ..a nd
> also file option will work differnt in bothcompact and ribbon .. as in compact as in
> google drive and in ribbon like in onlyoffice.. what u say .. just trying to make UI ..
> and make less cognative burden"

> "and also see menus .. as i dont think we woudl se layout, insert .. view.. i woudl be
> grouped diferntly there"

The defect in one sentence: **two navigation systems were on screen at once** — an
application menu bar (File / Edit / View / …) in the header and a ribbon tab strip
(Home / Insert / Layout / …) under it — so before a person could look for a command they
had to decide *which of two places* owned it.

## 1. The rule this adopts

> **Each chrome has exactly one navigation axis. Never two on screen together.**

| | **Ribbon chrome** (default) | **Compact chrome** | **Empty state (either chrome)** |
| --- | --- | --- | --- |
| Axis | the ribbon **tab strip** | the **menu bar** | the **menu bar** |
| First entry | `File` **tab** → a full-window **page** | `File` **menu** → a **dropdown** | `File` **menu** → a dropdown |
| Menu bar | hidden (`body.ribbon-mode #appMenuBar`) | shown | shown |
| Ribbon | shown | hidden (`body.compact-mode .ribbon`) | hidden (`body:not(.doc-loaded) .ribbon`) |

The empty-state row is not a hedge. Before a document is open, `body:not(.doc-loaded)` hides
the whole `.ribbon` — bands of disabled controls around a drop card read as a broken shell —
which takes the tab strip and its File tab with it. Hiding the bar there as well would have
left a freshly loaded editor with **no route to Open at all**. That regression was caught by
running the suite, not by reading the rule, and it is now pinned by a test of its own.
ONLYOFFICE arrives at the same shape from the other side: their read-only viewport declares
the File tab and nothing else (`Toolbar.js:1885`).

This also answers the owner's second message. `Layout` and `References` are **ribbon tabs and
not menus**; the menu bar is grouped as a menu bar (Google Docs' shape), not as a copy of the
tab strip.

## 2. What ONLYOFFICE actually does — source-verified

Read from `ONLYOFFICE/web-apps` at `master` (commit `9c0ca538c3b2`, "Merge branch
release/v9.4.0 into master"), paths relative to `apps/documenteditor/main/`. AGPL-3.0:
**behaviour and structure only, no code taken.**

**Tab strip, left to right** — `File · Home · Insert · Draw · Layout · References · Forms ·
Collaboration · Protection · View · Plugins · Header & Footer · Chart design`.

- Five tabs are declared statically at `app/view/Toolbar.js:182-186`:
  `{file, home, ins, layout, links}` — `textTabLinks` is captioned **"References"**.
- The rest are injected by `Common.Mixtbar.addTab(tab, panel, after)` with hard-coded
  indices (2, 5, 6, 7, 8, 10, 12) into a sparse array. **Their tab order therefore appears
  in no single file.** Contextual tabs sit at the far right; the chart tab is `aux: true`,
  commented in `Mixtbar.js` as "show tab at the end of toolbar".
- Captions come from `locale/en.json` `DE.Views.Toolbar.textTab*`.

**There is no menu bar.** Four independent pieces of evidence:
`template/Viewport.template` contains no menubar element; the File page's container class is
`toolbar-fullview-panel` and `Mixtbar.js:89` covers the toolbar when it shows; the File tab is
declared `haspanel: false`, which renders as `x-lone`, and `Mixtbar.js:370` short-circuits
`onTabClick` for it so it never activates a band; and the locale carries **no**
`Edit`/`Format`/`Tools` top-level menu strings — only the twelve `textTab*` keys.

**The File page**, in DOM order from `template/FileMenu.template` (the `items.push()` array in
`FileMenu.js:330-349` is declaration order, *not* visual order):
Back · | Create New, Open Recent, (Open on desktop) · | Save, Edit Document, Download As,
Save Copy As, Save As, Print, Print-with-preview, Rename, Protect · | Info, Version History,
Access Rights, Open File Location, (Close) · | (Switch to Mobile), **Advanced Settings**,
**Help**, Suggest a Feature.

**Their compact mode** is "Hide toolbar" (`Common.Views.Header.textCompactView`), persisted in
`localStorage` as `de-compact-toolbar`. Because the File tab is `x-lone`, **File behaves
identically in compact and normal mode**. We deliberately differ here — see §5.

**Google, for the compact side.** Docs' menu bar is `File, Edit, View, Insert, Format, Tools,
Help` (+ conditional Accessibility and Input Tools), from the menu-open shortcuts documented
at `support.google.com/docs/answer/179738`. It is an **anchored dropdown, not a backstage
page**: `answer/6282736` says that once open you "Use the Right arrow or Left arrow to navigate
to the other menus", which is structurally impossible for a view-replacing page. Drive's
file-level menu is likewise a dropdown with a few one-level submenus (Google's own name for it
is "More actions", `support.google.com/drive/answer/2563044`). *Unverified and flagged:* the
exact item order of Docs' File menu, and Extensions' position in the bar — no Google page
enumerates either.

## 3. Inventory before the change

### 3.1 The application menu bar — 9 menus, 106 rows, 106 distinct ids

| Menu | Rows | Ids |
| --- | ---: | --- |
| File | 12 | `file.new`, `file.open`, `file.save`, `file.export.{pdf,docx,odt,text,json}`, `layout.pageSetup`, `file.print`, `file.properties`, `file.recoverDrafts` |
| Edit | 8 | `edit.{undo,redo,cut,copy,paste,pasteText,selectAll,find}` |
| View | 8 | `view.{outline,showChanges,zoomIn,zoomOut,compactRibbon}`, `review.mode.{editing,suggesting,viewing}` |
| Insert | 16 | `insert.{table,image,shape,textbox,link,bookmark,field,header,footer,footnote,endnote,symbol,emoji}`, `layout.{firstPageVariant,evenOddVariant}`, `review.comment` |
| Format | 31 | `format.*` (16), `paragraph.align.*` (4), `paragraph.list.*` (5), `paragraph.indent.*` (2), `layout.paragraph`, `format.painter`, `style.{updateFromSelection,createFromSelection}` |
| Table | 18 | the whole `table.*` family |
| Review | 7 | `review.{toggle,previous,next,acceptNext,rejectNext,acceptAll,rejectAll}` |
| Tools | 3 | `tools.{spellCheck,smartQuotes}`, `view.settings` |
| Help | 3 | `help.{commands,shortcuts,about}` |

### 3.2 The ribbon — 7 tabs, 29 groups, 109 controls

| Tab | Groups | Controls | Carried a command id |
| --- | ---: | ---: | ---: |
| Home | 7 (undo, clipboard, font, paragraph, styles, editing, mode) | 45 | 0 |
| Insert | 6 (table, illustrations, links, text, header & footer, symbols) | 11 | 11 |
| Layout | 3 (page setup, paragraph, arrange) | 11 | 11 |
| References | 3 (navigation, notes, fields) | 7 | 7 |
| Table | 6 (select, rows & columns, merge, cell format, properties, style) | 19 | 0 |
| View | 3 (show, zoom, page setup) | 5 | 0 |
| Review | 3 (tracking, changes, comments) | 11 | 10 |

(The "carried a command id" column is `109` UX-005, which remains open: 39 of 109. This change
does not close it; it adds 4 stamped controls and depends on no Home/View/Table stamping.)

### 3.3 The cross-product — where each thing actually lived

Both systems: everything on Insert / Layout / References / Review, and every Home and Table
control whose command had a menu row.

**Menu-only, with no ribbon control anywhere** — 21 ids:
`file.new`, `file.open`, `file.save`, `file.export.{pdf,docx,odt,text,json}`, `file.print`,
`file.properties`, `file.recoverDrafts`, `edit.pasteText`, `edit.selectAll`,
`layout.firstPageVariant`, `layout.evenOddVariant`, `tools.spellCheck`, `tools.smartQuotes`,
`view.settings`, `help.commands`, `help.shortcuts`, `help.about`.

**Ribbon-only, with no menu row** — the font family/size fields, the zoom control, the
underline-style menu, the list galleries, the styles gallery, and the Table tab's structural
buttons (which have menu rows through the Table menu but no `data-command`).

## 4. The new structure, and what moved

### 4.1 Ribbon tab strip

`File · Home · Insert · Layout · References · Review · View · [Table]`

Declared **once**, as an ordered list, in `RIBBON_TABS` (`webapp/src/command_taxonomy.mjs`);
`menu_taxonomy.test.mjs` asserts `editor.html` renders exactly that sequence. This is
deliberate: ONLYOFFICE's order is an emergent property of seven `addTab` calls and cannot be
read off one file. Copying their order is worth it; copying that is not.

Two changes from before: **Review now precedes View** (their order, and Word's), and **Table,
being contextual, moved to the right-hand end** (where both references put contextual tabs).
`End` on the strip therefore lands on View rather than Review, and three specs were updated to
say so.

### 4.2 The File surface — one declaration, two renderings

`FILE_SURFACE` in `command_taxonomy.mjs` is the single roster. The ribbon chrome renders it as
a page (`#panelFile`, headed groups); the compact chrome renders it as a dropdown
(`APP_MENU_SECTIONS.file` **is** `fileMenuSections()`). Both go through
`renderCommandRows()` in the new `command_menu.mjs`, so neither can invent its own gating —
the rows are the same `editorCommands()` descriptors the palette and the context menu use.

| Group | Ids | From ONLYOFFICE |
| --- | --- | --- |
| New and open | `file.new`, `file.open`, `file.recoverDrafts` | Create New, Open, Open Recent |
| Save | `file.save`, `file.export.{pdf,docx,odt,text,json}` | Save, Download As |
| Print | `layout.pageSetup`, `file.print` | Print |
| Document | `file.properties` | Info |
| Settings | `view.settings` | Advanced Settings |
| Help | `help.{commands,shortcuts,about}` | Help |

Order follows Google Docs' File menu (New/Open first), which is the order this editor already
shipped and the reference the owner named for compact mode.

### 4.3 The compact menu bar — 9 menus down to 7

`File · Edit · View · Insert · Format · Table · Review`

- **Tools is gone.** `view.settings` → the File surface (ONLYOFFICE's *File ▸ Advanced
  Settings*). `tools.spellCheck` and `tools.smartQuotes` → the **Review** menu and a new
  **Review ▸ Proofing** band group (Word's *Review ▸ Proofing*).
- **Help is gone.** Its three rows → the File surface's Help group (ONLYOFFICE's *File ▸
  Help*).

Those were also the two names that scrolled off the end of the bar behind a hidden scrollbar
(`109` HF-097): the bar no longer clips at 460px, so that guard's viewport moved to 360px
rather than the guard being deleted.

### 4.4 New ribbon faces — 5 controls

| Command | New face | Why there |
| --- | --- | --- |
| `layout.firstPageVariant` | Insert ▸ Header & footer, `#insertFirstPageVariantBtn` | ONLYOFFICE files both in the header/footer surface; Word's Header & Footer tab ▸ Options |
| `layout.evenOddVariant` | Insert ▸ Header & footer, `#insertEvenOddVariantBtn` | same |
| `tools.spellCheck` | Review ▸ Proofing, `#reviewSpellCheckBtn` | Word's Review tab opens with Proofing |
| `tools.grammarCheck` | Review ▸ Proofing, `#reviewGrammarCheckBtn` | same; landed on `main` while this branch was open and would otherwise have been palette-only |
| `tools.smartQuotes` | Review ▸ Proofing, `#reviewSmartQuotesBtn` | same |

Both pairs reflect their state through a new declarative `pressed` field on the surface
tables, which replaced three hand-written `setAttribute("aria-pressed", …)` lines.

### 4.5 Every id in §3.1, accounted for

| Was in | Now reachable from |
| --- | --- |
| File (12) | the File page **and** the File dropdown; `layout.pageSetup` also View ▸ Page setup |
| Edit (8) | Home band (undo/redo/cut/copy/paste/find) + the Edit menu + chords; `edit.pasteText` and `edit.selectAll` from the **context menu** and their chords (see §6) |
| View (8) | View band (outline, zoom ±, review panel, page setup), Home ▸ Mode (the three modes), the tab strip's chevron (`view.compactRibbon`), Review band (`view.showChanges`), + the View menu |
| Insert (16) | Insert and References bands (+2 new) + the Insert menu |
| Format (31) | Home band + the Format menu + the context menu |
| Table (18) | the contextual Table band + the Table menu + the context menu |
| Review (7) | Review band + the Review menu |
| Tools (3) | Review band ▸ Proofing + the Review menu (2); the File surface (Settings) |
| Help (3) | the File surface |

## 5. Where this deliberately differs from its references

1. **File is mode-dependent here; in ONLYOFFICE it is not.** Their File tab is `x-lone` and
   opens the same full-window page in compact and normal mode. Ours is a page in the ribbon
   chrome and a dropdown in the compact chrome, because the owner asked for exactly that and
   because our compact chrome is Google-Docs-shaped — a menu bar over one flat toolbar — and
   neither reference hangs a full-window page off a menu bar.
2. **No Tools or Help menu, though Google Docs has both.** Keeping them would have meant two
   different File rosters (Settings and Help on the page, in menus in the bar), and one
   declaration with two renderings is the whole point of §4.2. ONLYOFFICE's arrangement wins
   here because it is the one that keeps the rosters identical.
3. **No always-visible static group.** ONLYOFFICE shows Save/Print/Copy/Paste/Undo/Redo left
   of every tab's panel (`Toolbar.template:8-36`). Not adopted; out of scope for this change.

## 6. Dropped, deferred, and recorded — nothing silent

**Nothing was dropped.** Every one of the 106 menu rows in §3.1 is reachable from at least one
durable surface plus the palette, and that is now enforced rather than asserted (§7).

Three things are **deferred**, each with its reason, because the alternative was inventing UI
or spending the ribbon's width budget:

| Item | Status | Reason |
| --- | --- | --- |
| `edit.selectAll` on Home ▸ Editing | Deferred | ONLYOFFICE really does keep `Replace \| Select all` in their Home tab (`Toolbar.template:39-91`). We have Find and Replace there and 18px of slack at 1280px (§8) — one more 32px button exiles a whole group into the `⋯` menu. It has three surfaces already: the context menu, `⌘A`, and the palette. |
| `edit.pasteText` as a Paste split button | Deferred | Word's Paste is a split button whose dropdown holds Keep Text Only. Building a split button is real work, and the same width argument applies. Three surfaces already: the context menu, `⌘⇧V`, and the palette. |
| `review.acceptAtCaret` / `review.rejectAtCaret` | **Recorded as `109` HF-179** | Palette-only, and **pre-existing** — found by the new reachability guard, not caused by this change. Word puts them in the dropdown of the Review band's Accept/Reject split buttons; we have the faces and no dropdowns. |

Not adopted from ONLYOFFICE's File page, for want of the thing behind them, not for want of
attention: **Protect** (no document protection model), **Rename** (the title is edited in
place in the header), **Version History** (`docs/107` designs it; not built), **Access
Rights** (no auth model — host policy by design), **Suggest a Feature** (an external link),
**Switch to Mobile** (one responsive shell, no second build), **Close File / Open File
Location** (no host file manager; local-first by construction).

## 7. Surface parity, redefined — and no guard weakened

The old rule, `109` UX-004 and SKILL.md §10, is "every capability must be reachable from ≥2
surfaces". Two tests were named for it and **neither could enforce it**: they assert frozen
`toContain` lists, so they can prove what is present and never what is missing. Three more
tests stood in for it per family, each comparing one ribbon tab's id set against the menu bar's.

Under one axis, "the menu bar" is no longer a surface every chrome has. So the rule is restated
in the form it always meant:

> **Every command in the registry must be reachable from the command palette AND from at least
> one DURABLE surface** — a ribbon control, the File surface, a menu row, the compact toolbar,
> the context menu, or a keyboard chord. The palette alone is never enough.

`webapp/tests/e2e/one-axis-navigation.spec.mjs` enforces it, and derives **both sides from the
running application**:

- the registry, from the palette with an empty query (every row carries `data-command-id`);
- the surfaces, from the DOM: `.ribbon-panel [data-command]` across every tab (hidden panels
  keep their buttons, so this is ribbon-wide and a command may change tab without the guard
  dictating the information architecture), `#filePageBody .file-page-item`, all seven menus,
  `#compactToolbar [data-command-id]`, `.editor-context-menu .menu-item`, and — new —
  `data-command-shortcut` on the palette row.

That last one matters. The palette's visible hint box holds a chord, *or* the command's group,
*or* its refusal reason, depending on state. Reading it as "has a chord" would have counted
`Format` as a keyboard surface and the guard would have passed for **every command in the
registry** — green and worthless, the failure mode `109` CQ-003 records three times. So the
shortcut is now stamped separately, and the guard reads that.

**No guard was weakened.** Specifically:

- `review-surface.spec.mjs` and `insert-surface.spec.mjs` keep their exact-set comparisons;
  they now read the menu homes in the chrome that has menus.
- `menu_taxonomy.test.mjs` kept every existing rule and gained four: the two File renderings
  must be the same roster; every File section must be headed and non-empty; every row the
  dissolved Tools and Help menus held must have a named new home; and the tab strip must
  render exactly `RIBBON_TABS`.
- The new guard carries **two** exemption lists, and both are checkable rather than asserted.
  `VALUE_FAMILIES` exempts rows that name one *value* of a control (`format.family.Georgia`,
  `view.zoom.150`, `style.Normal`, …) and **names the owning control**, which the guard then
  asserts is in the chrome — rename `#fontFamily` and every `format.family.*` row becomes a
  real orphan. `PALETTE_ONLY` holds five ids in two kinds, kept apart on purpose: two are
  by design (object traversal, where the Tab key *is* the affordance, and which deliberately
  carries no `shortcut` in the registry because Tab means something else in a paragraph and in
  a table), and two are the recorded HF-179 gap. A third assertion fails if an exemption is
  no longer needed, so the list cannot rot into cover.

## 8. The width arithmetic — measured, not assumed

`109` records that the Home band has single-digit slack at 1280px and that widening one control
exiles a whole group into the `⋯` overflow. Measured at 1280×800 with a document open, on this
branch:

| Band | Client | Groups sum | Slack | `#ribbonOverflowBtn` |
| --- | ---: | ---: | ---: | --- |
| Tab strip | 1280 | 556 (8 tabs) + 28 (chevron) + 24 (padding) = 608 | **672** | n/a |
| Home | 1272 | 1254 (67+141+314+233+234+105+160) | **18** | hidden — all 7 groups inline |
| Insert | 1272 | 515 (49+167+67+35+131+66) | **757** | hidden |
| Review | 1272 | 409 (67+209+67+66) | **863** | hidden |

- **The Home band is untouched.** No control was added to it, which is why its arithmetic is
  unchanged and why `edit.selectAll` and `edit.pasteText` were deferred rather than squeezed in.
- **The File tab costs 51px** of a strip with 672px spare. `scrollWidth === clientWidth` on the
  strip: no clipping.
- **Insert gained 64px** (its Header & footer group went 67 → 131) and **Review gained ~66px**
  (a fourth group). Both were less than half full; both still show every group inline.

## 9. Files

| File | What changed |
| --- | --- |
| `webapp/src/command_taxonomy.mjs` | `FILE_SURFACE`, `RIBBON_TABS`, `fileMenuSections()`, `fileSurfaceCommandIds()`; `APP_MENU_SECTIONS.file` derived; Tools and Help removed; Review gained Proofing |
| `webapp/src/command_menu.mjs` | **new.** `renderCommandRows()` (shared by the page and the dropdown) and `createMenuBar()` — ~150 lines of behaviour out of `main.js`, which is the `109` HF-085 direction |
| `webapp/editor.html` | File tab + `#panelFile`; Review before View; Table last; 4 new band buttons; Tools and Help buttons removed |
| `webapp/src/style.css` | one axis per chrome; the File tab and page; the empty-state exception |
| `webapp/src/main.js` | `renderFilePage()`, `closeFilePage()`, the File tab's focus rule, `pressed` on two surface tables, `data-command-shortcut` on palette rows, and the compact bar's tooltips localised (a pre-existing HF-025 defect this surfaced). **Net −4 lines** — 17,408 against the 17,412 this branch rebased onto, so the ratchet drops from 17,425 to 17,408 in this commit (re-measured from the merged file, as the MERGE NOTE in `module_seams.test.mjs` requires). The `command_menu.mjs` extraction paid for the File page, the tab wiring, the five new band buttons and three small fixes with four lines to spare. |

## 10. Open question for the owner

The second message — *"i dont think we woudl se layout, insert .. view.. i woudl be grouped
diferntly there"* — is read here as: the menu bar should be grouped as a menu bar, not as a
copy of the tab strip, and `Layout` should not appear as a menu. That is what shipped: there is
no Layout menu, page setup is under File as it is in Docs, and the bar is Docs-shaped minus
Tools and Help.

If the intent was instead a **shorter menu bar** — fewer than seven names, or Insert and View
folded into other menus — say so and it is a small follow-up: the bar's whole taxonomy is one
literal in `command_taxonomy.mjs` and the guards compare it to the registry, so re-grouping is
a data change with a test that will object if anything loses its home.
