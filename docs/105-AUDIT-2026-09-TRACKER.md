# 105 — September 2026 Audit Tracker

> **Archive, closed to new rows (2026-09-20).** The single working queue is
> **`109-BACKLOG.md`** — every open row below appears there, in the order it will be
> worked, and `109` is the only queue to work from. This document is not deleted and no row
> is removed from it: it holds the audit evidence, the measurements, the file-and-line
> citations and the ONLYOFFICE source analysis that `109` deliberately does not duplicate.
> **Do not add a new row here** — add it to `109`. When a row below closes, update its
> Status here *and* remove it from `109`.

**Status:** Living record. **Opened:** 2026-09-15. **Owner:** unassigned.

**Scope:** the findings of the 2026-09 audit round — an editor UI/UX audit, a DOCX
rendering-and-round-trip fidelity audit, and an ONLYOFFICE Document Editor fit-gap
comparison built from ONLYOFFICE's own client source rather than its marketing pages.

**Product goal this serves (owner, 2026-09-15):** OpenDoc is to be the **Apache-2.0
alternative to ONLYOFFICE for documents and document collaboration** — DOCX, ODT, TXT,
JSON and the other document formats. Spreadsheets and presentations are explicitly **out
of scope** (they are opencalc's and a future sibling's problem). Collaboration is a
first-class v1 outcome, not deferred breadth. The phased plan that consumes this tracker
is `106-ONLYOFFICE-ALTERNATIVE-ROADMAP.md`; rows here are its input, and each roadmap
phase names the rows it closes.

This tracker does **not** replace its neighbours, and rows are not duplicated across
them:

| Document | Owns |
| --- | --- |
| `14-EXECUTION-TRACKER.md` | Per-slice execution state |
| `104-HOTFIX-TRACKER.md` | The pre-existing ranked defect queue (HF-xxx) |
| `99-REMAINING-WORK-AUDIT.md` | Unfinished *capability*, ranked by owner priority |
| `18-SUPPORT-MATRIX.md` | Public support claims |
| `46` / `55` / `60` | Rendering-fidelity audits, each pinned to its own code baseline |
| **this document** | The 2026-09 audit's findings, and the competitive fit-gap |

Where a finding restates a `104` row, it is recorded here only as a cross-reference and
the HF row stays authoritative.

## Priority and effort vocabulary

Identical to `104` so the two queues can be merged-sorted:

| Priority | Meaning |
| --- | --- |
| P0 | Data loss, corruption, unrecoverable state, hang, security hole, or a **false public claim**. Fix before other work. |
| P1 | User-visibly wrong on a common path, an accessibility blocker, or an interaction clearly below the Word/Docs/ONLYOFFICE bar. |
| P2 | Real defect on a less common path, or a breadth gap a competitor closes. |
| P3 | Minor. Worth fixing, not worth reordering for. |

**Effort:** S = under a day · M = up to a week · L = more than a week, or needs a design
decision first.

**Status:** Open · In progress · In review · Fixed (names the PR) · Won't fix (with reason).

## Why this round happened, and what it says about the last one

The audit was commissioned as a competitive comparison. The most serious thing it found
was not a missing feature but a **truth defect**: `webapp/fidelity.html` — a public,
canonical, indexed page whose entire premise is "measured, not claimed" — carried five
claims the code contradicts. That is the second time this page has done this. The first
occurrence (fabricated fixture rows, 2026-08-09) is recorded in `99` §6 as process debt;
this occurrence proves the lesson did not take, because **the page was never brought
under test**. `webapp/tests/fidelity_data.test.mjs` guards `webapp/src/fidelity.js` (the
matrix data) and never touches the page that wraps it, which is where every fabricated
claim lived both times.

The corrective is EV-001 through EV-004 below, and the rule it establishes:

> A number on a public page must be generated from a committed artifact, or it must not
> be on the page. Prose that describes a CI gate must name the workflow, and a test must
> assert the gate is armed.

A second, quieter theme runs through both audits: **capability that is modeled but never
consumed.** Eight separate constructs are fully typed, cascaded, and round-tripped while
no layout code reads them — footnote number formats, line numbering, `w:kern` thresholds,
`w:kinsoku`, embedded `.odttf` faces, cell `noWrap`/`fitText`/`hideMark`, `w:gutter`, and
section parity breaks. Each looks finished from the model side and is invisible to a user.
The model-completeness work (`95`) counted these as done; they are done as *model* rows
and not as *features*, and nothing in the tracker distinguished the two. FID-P-01 proposes
the guard.

## Summary

### Current state — read this first

**Last reconciled against `main`: 2026-09-16, after #542.** The counts below are derived
from the rows; a row's Status cell names the PR that closed it so the claim can be checked
rather than trusted.

Nothing is blocking `main`. FID-L-22 (no caret in a blank header) was fixed by #543; see
its row.

Closed this session, with the PR that did it: EV-001…EV-004 and EV-006 (#528) —
EV-005 is still `In progress`, so this list is five EV rows, not six — FID-P-02 groundwork and
the oracle harness (#532/#535/#541, FID-P-01 still partly open), FID-L-01 (#534),
FID-L-03 (#536), FID-L-09/FID-L-14 (#541), FID-R-01/FID-R-06 (#541), FID-R-04 (#540),
UX-001/UX-002 (#537), UX-010/UX-011 (#542 — the ribbon IA commit `d2bd9cd`; #541
touched no ribbon file), plus the per-page running-content fix and
HF-094. FID-L-16 and FID-L-18 are partly closed with the remainder stated in their rows.

| Class | Rows | Open | Closed this session |
| --- | ---: | ---: | --- |
| EV — evidence and public claims | 7 | 1 | 6 (#528, this PR) |
| UX — editor UI/UX | 24 | 20 | 4 (#537, #542) |
| CQ — engineering quality | 10 | 10 | 0 |
| FID — fidelity and round-trip | 34 | 26 | 6 (#534, #536, #541, #543) |
| OO — ONLYOFFICE fit-gap | 21 | 21 | 0 — analysis only, no implementation yet |
| **Total** | **96** | **78** | **16** |

**These counts are derived from the rows, not maintained by hand** — re-derive them rather
than editing them, per CQ-007. (The first draft of this table said 55 rows and understated
FID by 7, which is the exact drift CQ-007 describes; it was caught by deriving.) "Open"
counts `Open`, `Partly fixed` and `In progress`; every OO row is open by definition, since
none of that capability exists yet. Five of the six EV rows are closed by the PR that opens
this tracker, recorded rather than deleted so the correction stays auditable.

---

## 1. EV — evidence, public claims, and test honesty

The highest-priority class in this round. Each row is a statement the project publishes
that the code does not support, or a guard that cannot fail.

| ID | Finding | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- |
| EV-001 | **`fidelity.html` claimed a CI-enforced oracle diff that does not exist.** "Every fixture renders through the CPU backend and is diffed against LibreOffice. Page counts, collision gates, and geometry are CI-enforced; regressions fail the build." In fact the only oracle test covers one fixture and opens with an early `return` when its reference is absent — and `fixtures/oracle/` **does not exist in the repository**, so the assertion has never executed. `.github/workflows/oracle-geometry.yml` is `workflow_dispatch`-only and has evidently never been run. `tools/opendoc-fidelity` appears in no workflow. | P0 | S | `webapp/fidelity.page.html:26-33`; `crates/casual-doc-render/tests/oracle_geometry.rs:186-190`; `ls fixtures/oracle` → absent | Fixed (this PR) |
| EV-002 | **Headline metrics were unsourced and contradicted their own cited source.** "3/5 exact page-count parity" and "±1 worst-case page delta" appear in no document; `60` §"Baseline and post-fix result" records **4/5 parity and a worst delta of +2**. The fixture table listed Medical form at 3 pages / Δ −1 (the *pre*-fix number, superseded by `P1F-DOCGRID-LINES`) and Chinese SDS at 17 / Δ +1 (matching neither the pre-fix 17-vs-16 nor the post-fix 18-vs-16). The tiles therefore simultaneously understated parity and understated the worst delta. "7 ms incremental page repaint" has no source at all: the committed baseline holds four cases — package open, part read, model load, SDK typing — and **no layout, render, or repaint case**. | P0 | S | `webapp/fidelity.page.html:34-49, 100-118`; `docs/60-FIDELITY-CORPUS-RENDERING-AUDIT.md:62-67`; `benchmarks/baselines/mac16-12-m4-10c-16gb.json` | Fixed (this PR) |
| EV-003 | **The page denied a shipped feature.** "Color fonts and color emoji are not supported… the CPU rasterizer… has no color glyph-table (COLR/CBDT/sbix) path." The rasterizer dispatches `sbix`/`CBDT` bitmap strikes and then `COLR` v0/v1 paint graphs with the default `CPAL` palette *before* falling back to outlines. Understating support is cheaper than overstating it but is the same defect: the page is not derived from the code. | P0 | S | `webapp/fidelity.page.html:203`; `crates/casual-doc-render/src/lib.rs:593-605, 771` | Fixed (this PR) |
| EV-004 | **"No silent data loss" was a total claim over a partial mechanism,** and image attribution was wrong ("Comparison images are produced locally by `tools/opendoc-fidelity`" — that tool is a text-only word-multiset differential that produces no images and no page counts). Silent-loss counterexamples are FID-R-01…FID-R-06. | P0 | S | `webapp/fidelity.page.html:57, 209`; `tools/opendoc-fidelity/src/main.rs:1-10` | Fixed (this PR) |
| EV-005 | **The matrix drift guard cannot detect an overstatement, and does not cover the page.** `fidelity_data.test.mjs` pinned ~15 named cells and the family list — a shape check. A newly-raised cell on any unpinned row passed green, and the guard never loaded `fidelity.page.html`, which is where both fabrication incidents occurred. Partly closed: this PR adds per-family honesty invariants that each name the code fact that must change first, and proves three of them go red under mutation. **Still open:** nothing asserts the *page's* prose or stat tiles against a generated artifact, and nothing asserts the oracle gate is armed. | P1 | M | `webapp/tests/fidelity_data.test.mjs`; `webapp/tests/e2e/fidelity-page.spec.mjs` | In progress |
| EV-006 | **Seven construct families were absent from the public matrix, and absence read as coverage.** Hyphenation, line numbering, watermarks/WordArt, vertical/rotated text, bidi/RTL/CJK grid, fonts/fallback/colour glyphs, and drop caps had no row. A reader seeing 19 rows dominated by full/partial infers breadth the engine does not have. Six of the seven are `none` or weak. | P2 | S | `webapp/src/fidelity.js` | Fixed (this PR) |
| EV-007 | **The landing page published claims the code contradicts, in both directions.** It said "three of five corpus documents match page counts exactly" while the fidelity page said 4/5 and `60` records 4/5 as the only committed figure; it showed a corpus row for `tables-nested.docx`, which is not in `fixtures/manifest.json`; it advertised "Sub-10 ms incremental repaint" with no repaint benchmark (CQ-006); and it listed text wrap around floats, inline math and multi-column layout under "Not yet" although all three render. The same audit found the fidelity page understating two families: line numbering graded `rendered: none` ("never generated or painted") and embedded fonts "never de-obfuscated" — both shipped (#541, #534), and `fidelity_data.test.mjs` was pinning the false grades. | P1 | S | `webapp/index.page.html` (pre-redesign) lines 140, 153, 241, 344; `webapp/src/fidelity.js` line numbering and fonts rows; `crates/casual-doc-layout/src/line_number.rs`, `font_registry.rs:339` | Fixed (this PR) — every figure on the page is tagged `data-claim` and re-derived by `webapp/tests/site_claims.test.mjs` from `fidelity.js`, `60` and the wasm exports; each "Not yet" item must cite a family graded `none` or an open `105` row; timings are rejected without a benchmark; 8 guards, each driven red by restoring the defect it covers. Fidelity grades corrected with evidence |

### EV corrective actions taken in this PR

- `webapp/fidelity.page.html`: the CI-enforcement claim is replaced with what is true —
  a dated manual measurement, an explicitly **inert** automated gate, and no pixel-diff
  gate. Tiles now read 4/5 parity, +2 worst delta, 19→26 families graded, and **0**
  page-count/pixel gates in CI. Medical form and Chinese SDS rows carry `60`'s real
  numbers. The colour-emoji denial is replaced with the shipped behaviour and its limits.
  The data-loss claim is scoped to what the mechanism actually covers and links here.
- `webapp/src/fidelity.js`: `Comments.rendered` full→partial (the engine paints a comment
  highlight only under the read-only markup view); `Lists & numbering.modeled` full→partial
  (picture bullets are unmodeled and dropped unreported); the Tables note now names the
  whole unconsumed set; seven new families added.
- `webapp/tests/fidelity_data.test.mjs`: honesty invariants for every new family, each
  citing the code fact that gates an upgrade. Verified red under three separate mutations
  (colour glyphs → none, Comments → full, Hyphenation → partial) and green on restore.
- `webapp/tests/e2e/fidelity-page.spec.mjs`: row count 19 → 26.

### EV rules this round establishes

1. A public number is generated from a committed artifact or it is not published.
2. Prose describing a CI gate names the workflow, and a test asserts the gate is armed.
3. A construct family absent from the matrix is an overstatement by omission; the matrix
   enumerates families, not successes.
4. Every honesty invariant is proven red by mutation before it is trusted (`99` §6 and
   the standing "tests must be able to fail" rule).

---

## 2. UX — editor UI/UX

From the editor UI/UX audit. Rows the audit found already closed in `104` are not
repeated; see §2.4 for the `104` corrections that came out of it.

### 2.1 Structural — fix before the breadth rows

| ID | Finding | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- |
| UX-001 | **There is no editable focus owner, so the editor cannot accept text on a touch device and its IME path cannot fire.** `#pages` is a plain `div[tabindex="0"]`; all text entry is a `document` `keydown` listener. There is no `<textarea>`, no `contenteditable`, no `inputmode` proxy anywhere. Consequences: (a) tapping the page on iOS/Android raises **no soft keyboard** — while `18` declares mobile/tablet browsers a supported target (owner decision 2026-09-02); (b) `compositionstart/update/end` do not fire from a non-editable element, so the whole IME preedit path is dead for real CJK/Korean/Vietnamese input; (c) no dictation, no platform text services, and no seam for spellcheck. **This is the unlock row** — the mobile half of UX-018/UX-019, the IME half of this row, dictation, and HF-035 all sit behind it. | P0 | L | `webapp/editor.html:1072`; `webapp/src/main.js:2461-2467, 2478-2480, 14714`; `18-SUPPORT-MATRIX.md` mobile row | Fixed (#537) — editable focus-owner proxy; soft keyboard, real IME and dictation are now reachable. 4 guards, all mutation-proven |
| UX-002 | **The IME test is green while the feature is broken.** `ime-preedit.spec.mjs` dispatches synthetic `CompositionEvent`s on `document`, which a real IME cannot do against a non-editable element. The suite therefore certifies a path no user can reach. A guard that cannot fail on its own subject is worse than no guard. | P1 | S | `webapp/tests/e2e/ime-preedit.spec.mjs:33-52` | Fixed (#537) — `ime-preedit.spec.mjs` now dispatches on the real focus owner instead of `document` |
| UX-003 | **`main.js` is 15,951 lines with zero exports** (verified: `grep -c "^export"` → 0) — 91% of the webapp's JS, 471 top-level `const`, 458 top-level functions, 360 module-scope `getElementById`. This is `HF-085`, recorded here for the four UX consequences that are each independently actionable: command-surface parity is unenforceable (UX-004/UX-005), apply paths diverge (UX-012), there is no embed seam (`HF-109`), and there is no i18n seam (UX-009). Note the evidence *for* extraction already in the repo: the four extracted modules (`modal.mjs`, `context_menu.mjs`, `keyboard.mjs`, `contrast.mjs`) are the only parts of the UI with unit tests, and `modal.mjs` closed a whole defect theme (T-04). | P1 | L | `webapp/src/main.js`; cross-ref `HF-085` | Open |

### 2.2 Command surface, discoverability, and shortcuts

| ID | Finding | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- |
| UX-004 | **The two tests named for command-surface parity do not enforce it.** `command-surface-and-recovery.spec.mjs` asserts a **hardcoded 7-id list** with `toContain` (a superset passes) against the context menu — a frozen snapshot of the 7 `contextMenu: true` declarations. An eighth declared command omitted from the menu passes green. The file's own header claims it "reads membership from the declaration"; it does not. `command-shortcut-coverage.spec.mjs` contains no enumeration at all — one palette row is asserted, for the four other commands its header claims to cover there are zero assertions. This is the repo's stated cure for its top recurring defect class (`opendoc command-surface parity`), and it is not installed. Fix: scrape the live registry via `page.evaluate` and require **set equality** per surface with a reviewed exemption list — `dialog-contract.spec.mjs:302-313` already shows the pattern. | P1 | M | `webapp/tests/e2e/command-surface-and-recovery.spec.mjs:22-23, 40-48`; `command-shortcut-coverage.spec.mjs:66-80` | Open |
| UX-005 | **Only ~23 of ~90 ribbon controls carry a command id.** `INSERT_SURFACE` (13 rows) and `REVIEW_SURFACE` (10 rows) are declarative and stamp `dataset.command`; everything on Home, View and Table is bound by one of 132 ad-hoc `addEventListener("click")` calls. Parity is machine-checkable for exactly the two tabs that are declarative — and every drift row below sits on the three that are not. That correlation is the argument for extending the declaration, not an accident. | P1 | M | `webapp/src/main.js:7714-7743, 7757-7769, 8630, 8640, 1023`; `67-EDITOR-UX-GAP-ANALYSIS.md:69` | Open |
| UX-006 | **Standard word-processor shortcuts are unbound.** 18 `shortcut:` declarations exist in total. Unbound: **⌘E/⌘L/⌘R/⌘J** (alignment — the highest-frequency Word chords after B/I/U), ⌘⇧L and ⌘⇧7/8 (lists), ⌘⌥1-6 (heading styles), ⌘] / ⌘[ (indent), ⌘0/⌘+/⌘− (zoom), ⌘G/⌘⇧G (find next/previous), ⌘⏎ (page break — see `HF-127`), ⌘⇧8 (formatting marks), ⌘N. Strike, superscript, subscript, grow/shrink font have no chord either. | P1 | M | `webapp/src/main.js:14817-14872, 6184-6205` | Open |
| UX-007 | **⌘⇧E toggles Suggesting but is advertised nowhere** — the descriptor declares no `shortcut:`, so the palette row, the Review menu row, and the ribbon tooltip are all silent about a chord that works. The converse of UX-006 and the same root cause: bindings and labels are not derived from one table. | P2 | S | `webapp/src/main.js:6187`, descriptor at `:11698` | Open |
| UX-008 | **`insert.table` is two different products behind one command id.** From the ribbon it opens a grid picker; from the menu bar and the command palette it silently inserts a fixed 3×3. A user who learns the menu route can never choose a size. | P2 | S | `webapp/editor.html:369`; `webapp/src/main.js:10528-10632` vs `:11652` | Open |
| UX-009 | **70 hardcoded ⌘ glyphs are shown to Windows and Linux users** (52 in `main.js`, 18 in `editor.html`), plus 15 `⇧/⌥/⌫/⏎`. The *handlers* correctly accept `metaKey \|\| ctrlKey` and `keyboard.mjs` already detects the platform — so only the **labels** are wrong, for every shortcut in the product, for the majority of desktop users. With 117 English `setStatus` literals and a hardcoded `<html lang="en">`, this is also the i18n seam. Cross-ref `HF-025`, `HF-081`; raised to P1 because ONLYOFFICE ships ~30 locales and platform-correct chords. | P1 | M | `webapp/src/main.js` ×52, `webapp/editor.html` ×18; `webapp/src/keyboard.mjs:9` | Open |
| UX-010 | **No Layout tab; page and section commands are scattered across four surfaces.** Page setup is on the **View** ribbon and in the **Tools** menu. Paragraph properties is on **Home**, in **Format**, and in **Tools**. Header/footer and its first-page/odd-even variants are under **Insert**. Meanwhile the palette already declares a `group: "Layout"` for four commands that have no Layout home. Columns, margins, orientation, page size, breaks, indent and spacing all ship — that is a filled Layout tab. `64`:87 rejected a Layout tab on "only tabs we can fill" grounds; that premise has expired. | P1 | M | `webapp/editor.html:492`; `webapp/src/main.js:11654-11683, 11914`; `64-EDITOR-TOOLBAR-RIBBON-DESIGN.md:87` | Fixed (#542) — Layout tab; Page setup buttons open the dialog focused on their own fieldset. All groups inline at 1280px, asserted by `layout-references-surface.spec.mjs:87,120`. (The "+700px headroom" figure came from a mutation in the commit body, not from a test, and is not a standing claim — the Home band's real slack at 1280px is single digits, which is how Styles later landed in the overflow menu; see `ribbon-home.spec.mjs`.) |
| UX-011 | **No File backstage, no New document, no recent files.** There is no `file.new` anywhere; the only way in is the OS picker or the auto-loaded `sample.docx`. The editor cannot author a document from scratch — the most basic word-processor task — so every session begins by borrowing someone else's file. Cross-ref `HF-016`, `HF-073`. | P1 | M | `webapp/editor.html:80`; `webapp/src/main.js:11531, 11873-11878, 2676` | Fixed (#542) — File ▸ New builds a real minimal DOCX package in JS (5 parts, deterministic bytes) and opens it through the ordinary path. **Recent files NOT done** — `<input type=file>` yields no re-openable handle, so it would be a dead control |
| UX-012 | **No Table menu on the menu bar, and the palette hides table commands on complex tables.** `APP_MENU_SECTIONS` contains zero `table.*` ids, so browsing the menus tells the user the editor has no table editing. The palette surfaces the 22 table commands only when `plainTableInfo(...)` is truthy — so inside a **merged** table it goes silent too, leaving the contextual ribbon tab and right-click as the only routes. | P2 | M | `webapp/src/main.js:11872-11914, 11791-11806, 6457-6590` | Open |
| UX-013 | **Print has no visible chrome** — File menu, ⌘P and the palette only. Because the menu bar is hidden until a document loads, a new user has no discoverable print affordance at all. | P2 | S | `webapp/src/main.js:11537`; `webapp/editor.html:32` | Open |
| UX-014 | **Menu taxonomy matches neither Word nor Docs:** Format painter under **Edit**; Page setup and Paragraph properties under **Tools**; Settings under **Tools**; header/footer under **Insert**; review mode duplicated in **View** and **Review**. | P2 | S | `webapp/src/main.js:11884, 11888, 11893, 11903, 11914` | Open |
| UX-015 | **Single-surface capabilities:** Pages panel (rail only), compact-ribbon toggle (chevron only — `HF-094`), table style gallery, line/paragraph spacing (in no menu), format painter (no context menu), Settings (not on the View ribbon, contra `64`:128). | P2 | M | `webapp/editor.html:1045-1049`; `webapp/src/main.js:822-862` | **Partly fixed** (#542) — the compact-ribbon toggle is no longer chevron-only: `view.compactRibbon` is a View-menu and palette command whose label reads back its state (`main.js:12187`, `:12421`), guarded by `file-new-and-ribbon-mode.spec.mjs:130,164`. That is HF-094. **Still open:** Pages panel (rail only), table style gallery, line/paragraph spacing, format painter, and Settings absent from the View ribbon |
| UX-016 | **Two mode controls with different labels for one state** — the ribbon says "Edit / Suggest / Read only", the footer says "Editing / Suggesting / Read only". | P3 | S | `webapp/editor.html:346-348` vs `:1334-1336` | Open |

### 2.3 Interaction, feedback, accessibility, responsive

| ID | Finding | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- |
| UX-017 | **The only feedback channel is `display:none` at phone widths — silently and inaudibly.** All 117 `setStatus()` calls write one line in `.foot-left`, and both live regions (`#statusLiveRegion`, `#statusAlertRegion`) are **inside it**. `.foot-left` is hidden at ≤620px, and `display:none` prunes live regions from the accessibility tree. So below 620px "That file could not be opened", "This structural change cannot be tracked in Suggesting mode", and every other refusal are invisible on screen **and** unavailable to a screen reader: the editor does nothing and says nothing. There is no toast system (`grep -c toast` → 0). | P1 | M | `webapp/src/style.css:5417-5421`; `webapp/editor.html:1304-1318` | Open |
| UX-018 | **Zero touch code.** No `touchstart`/`touchmove`/`touchend`/`pointerType`/`gesturestart`/`maxTouchPoints` anywhere. The only zoom gesture keys off `ctrlKey \|\| metaKey` + wheel, which a trackpad pinch produces and a **touchscreen pinch does not** — so on a tablet, pinch zooms the browser and detaches the fixed chrome from the viewport. No selection handles, no `touch-action` on `.pages`. With UX-001 this makes the product unusable on touch while `18` declares it supported. Cross-ref `HF-088`. | P1 | L | `webapp/src/main.js:15398-15407`; `18-SUPPORT-MATRIX.md` mobile row | Open |
| UX-019 | **No breakpoint below 620px; at 390px the editor is one dropdown.** The overflow collapser keeps only the Clipboard group inline, so the entire Home tab — font, size, B/I/U, colour, alignment, lists, styles, find — becomes a single `⋯` menu; the feedback channel disappears (UX-017); and there is no way to type (UX-001). Playwright runs chromium-desktop only; one spec exercises 390px. | P1 | L | `webapp/src/style.css:5391`; `webapp/src/main.js:840-862`; `webapp/playwright.config.mjs:53` | Open |
| UX-020 | **The accessibility mirror is read-only and unanchored to the caret.** The projection itself is good — real `h1-h6`/`ul`/`ol`/`table`+`th[scope]` with correct nested-list stacking. But (a) it is `replaceChildren`-rebuilt on every content-change frame, resetting the screen-reader virtual cursor on every keystroke (`HF-071`); (b) there is **no relationship between it and the caret** — no `aria-activedescendant` from `#pages`, no reading-cursor sync; (c) it walks the body only, so headers/footers, footnotes, text boxes and comments are invisible to AT. Net: a screen-reader user can read the document and operate the chrome but cannot tell where the caret is or verify an edit landed — so cannot edit. | P1 | L | `webapp/editor.html:1080`; `webapp/src/main.js:10644-10763, 2583` | Open |
| UX-021 | **Four of five `role="radiogroup"` containers own toggle buttons, not radios.** `#pageOrientationSeg`, `#cellVAlign`, `#paraPanelAlign`, `#tableAlign` are declared `radiogroup` with children marked `aria-pressed` only; a screen reader announces "radio group, 4 items" then "toggle button, not pressed" — a direct contradiction — and arrow keys do nothing. Only `#themeSeg` was converted to `role="radio"`/`aria-checked`/roving tabindex. The fix is already written: extract it as one helper and apply it four more times. | P1 | S | `webapp/editor.html:569, 640, 1159, 1253`; `webapp/src/main.js:15485-15494` | Open |
| UX-022 | **Engine boot and boot failure have no state design.** One footer line reads "Loading engine…" while a large wasm binary downloads, with every control rendering enabled-looking but `disabled`. On failure the code writes one sentence and returns — no dialog, no retry, no copyable diagnostic — having already bound to a document that will never exist. | P1 | M | `webapp/editor.html:1305`; `webapp/src/main.js:2643-2651` | Open |
| UX-023 | **Invalid and mismatched ARIA on popup triggers:** `aria-haspopup="region"` ×2 (not a valid token, so the Symbol/Emoji buttons announce no popup at all); `#fontFamily` declares `aria-haspopup="listbox" aria-controls="fontMenu"` while `#fontMenu` is `role="dialog"`; `#stylesMoreBtn` is a `<button>` child of a `role="listbox"`; `#settingsBtn` has `aria-expanded` with no `aria-haspopup` and no `aria-controls`. Also `role="dialog"` on eleven non-modal popovers that are never focused on open (`HF-070`). | P2 | S | `webapp/editor.html:413, 414, 249, 687, 332, 94` | Open |
| UX-024 | **Focus and target-size gaps:** `.pages:focus { outline: none }` with no substitute, so the skip-link target and editing surface have **no visible focus indicator**; menu-row `:focus-visible` substitutes only an `--accent-soft` tint at roughly 1.2:1, under the WCAG 1.4.11 3:1 floor; the table column-resize handle is 10px and ruler indent markers are ~21×15px, while object handles were correctly grown to 24px with an outward-only pseudo-element — the fix exists and was not carried over. Also: two tooltip systems, only the ribbon's appearing on keyboard focus; no `axe`/`@axe-core/playwright` anywhere, which is why UX-021 and UX-023 survived; reduced-motion is a single rule covering one animation. | P2 | M | `webapp/src/style.css:4479-4481, 2069-2073, 4953-4969, 4597-4616` vs `4737-4765`; `webapp/src/main.js:922, 1020-1021` | Open |

**Not a work item, recorded so it is not re-litigated:** the design-token system is the
strongest part of the webapp and should be left alone. 1,151 `var(--)` references; exactly
**one** raw hex outside the token blocks in 6,481 CSS lines, and it is inside a comment;
**2** `!important`; all three theme blocks define the identical 38-token set with no gaps;
backed by `style_tokens.test.mjs` and an AA contrast sweep over every text node in both
themes. `modal.mjs` is likewise a genuinely correct modal contract across all 10 dialogs.

### 2.4 `104-HOTFIX-TRACKER.md` corrections arising from this audit

The UI/UX audit re-read the code behind `104`'s open rows and found four marked Open that
are in fact closed, and three whose prescribed fix or evidence is wrong. These are
corrected in `104` by this PR; they are listed here because a stale tracker row is the
specific process debt `99` §6 calls out.

| HF row | Correction |
| --- | --- |
| HF-069 | Closed. `onButton` now `preventDefault`s mousedown and runs on `click`, so a mis-press can be aborted. |
| HF-074 | Closed. The skip link exists at `webapp/editor.html:26`. |
| HF-075 | Closed. The accent and table-border colour pickers have accessible names. |
| HF-076 | Closed for the declaration half — `contextMenu: true` is now read (`main.js:6609-6628`). The Paste-without-formatting / Select-all / checklist membership question is superseded by UX-004 (set equality). |

---

## 2A. CQ — engineering quality to enterprise standard

**Owner assessment, 2026-09-15:** *code quality and UI/UX are not production or
enterprise quality as of now.* That is an assessment of the **current state**, not a change
of target — `10-PROJECT-GOAL-AND-STANDARDS.md` and AGENTS.md still set production-grade as
the baseline, and the memory rule that OpenDoc is never framed as an MVP still holds. The
gap between the standard and the state is what this class tracks.

The rows below are measured, not impressions. Each carries the number that makes it
checkable, so progress is verifiable and the class can be closed on evidence.

| ID | Finding | Pri | Eff | Measurement (2026-09-15) | Status |
| --- | --- | --- | --- | --- | --- |
| CQ-001 | **Two god-files, one per side of the boundary.** `webapp/src/main.js` is 15,951 lines with **0 exports**, 458 top-level functions, 471 top-level consts, 360 module-scope `getElementById`, 132 ad-hoc click bindings, and 11 `document` keydown listeners with no defined precedence. `crates/casual-doc-wasm/src/lib.rs` is **26,374 lines in a single file** carrying all 449 `#[wasm_bindgen]` exports. Neither is reviewable, testable in units, or safely modifiable in parallel. Note `M-001` in `14` reports "every crate root reduced to a ≤64-line wiring file" — true for the four crates it covered (`model`, `ooxml`, `sdk`, `import`), and not true of the workspace: four crate roots exceed 3,000 lines. | P1 | L | 15,951 / 0 exports · 26,374 / 1 file | Open |
| CQ-002 | **The live editing path bypasses the transaction engine, so ADR-005 is not honoured in practice.** "Public mutation must go through commands and transactions" is the rule; `casual-doc-wasm` references `casual_doc_transaction` **0 times** and `casual_doc_edit` 14 times, applying operations directly with a flat capped `Vec<HistoryEntry>` undo stack and no revision chain. There are **two parallel operation sets** — 5 ops with the OT/revision substrate in a crate the editor never calls, 47 ops with inverses in the one it does — and `casual-doc-edit` has no dependency on `casual-doc-transaction`. This is the same failure shape as "modeled but never consumed" (FID-P-04), one layer up: a subsystem recorded as built is unreachable from the product. | P1 | L | 0 vs 14 references; 5 vs 47 ops | Open → `107` §2.1 |
| CQ-003 | **Guards that cannot fail.** Three shipped tests certify behaviour they cannot detect: the command-surface parity test asserts a hardcoded 7-id list with `toContain` (a superset passes) while its header claims it reads the declaration; the shortcut-coverage test enumerates nothing and asserts one palette row; the IME test dispatches synthetic `CompositionEvent`s on `document`, which a real IME cannot do against a non-editable element — so it is green while the feature is unreachable. The fidelity data guard was shape-only until this PR. A guard that cannot fail is worse than no guard, because it is cited as evidence. | P1 | M | 3 confirmed | Partly fixed (fidelity guard closed + mutation-proven) |
| CQ-004 | **Only 23 of ~90 ribbon controls are declarative**; the rest are hand-bound. The two tabs that *are* declarative are the two with real set-equality tests — the correlation is the argument, not a coincidence. | P1 | M | 23 / ~90 | Open → UX-005 |
| CQ-005 | **No i18n seam at all.** 117 English `setStatus` literals, **70 hardcoded ⌘ glyphs** shown to Windows and Linux users, 15 further Mac-only modifier glyphs, and a hardcoded `<html lang="en">`. The competitor ships 46 locales × 4,479 keys. Enterprise procurement treats localisation as a hard requirement, not a feature. | P1 | L | 70 wrong glyphs · 1 locale | Open → UX-009 |
| CQ-006 | **Test surface has structural blind spots.** Browser tests run **headless Chromium only** — no Firefox, no WebKit, therefore no VoiceOver path; there is no `axe`/`@axe-core/playwright` anywhere, which is why four invalid `role="radiogroup"` containers and five ARIA errors survived; **3 of 4 fuzz targets are built but never run** (`HF-090`) and no browser test opens a hostile document; and the committed benchmark baseline has **four cases** — package open, part read, model load, SDK typing — and **no layout, render, or repaint case**, which is how a "7 ms incremental page repaint" claim reached a public page with nothing behind it. | P1 | M | 1 browser · 0 axe · 1/4 fuzz · 0 layout benchmarks | Open |
| CQ-007 | **Hand-maintained numbers drift, and have twice become false public claims.** `104`'s summary read 114 rows / 47 open against an actual 146 / 54 because two later audit sections were appended without updating it; `18` carried eight false or stale rows; `44`/`46`/`55`/`60` are pinned ~992 commits behind `main`; and the public fidelity page carried fabricated evidence **twice**. The corrective is mechanical: derive counts, and never publish a number that is not generated from a committed artifact. | P1 | S | 4 doc classes corrected this PR | **Partly fixed** (#528, then re-checked) — counts are derived per class, and `webapp/tests/tracker_counts.test.mjs` now re-derives every summary cell in `104` and `105` from the rows and fails on drift. That guard was written because this row's own reconciliation drifted again within one session: `104`'s P3 cell said 14 open against 13 derived (the column no longer summed to its own Total), the header said "after #541" against a post-#542 HEAD, and UX-010/UX-011/HF-094 were attributed to #541, which touched no ribbon file. **Still open:** attribution is still hand-written — nothing checks that a cited PR actually contains the change |
| CQ-008 | **A dependency port is blocked at scale.** The quick-xml 0.42 migration measured **910 compile errors** across `casual-doc-import` and `casual-doc-odf`; a compiler-guided pass closes 271, leaving 368 signature and text-decoding changes in fuzz-hardened fail-closed parsers. CI reported only three errors because compilation stops at `casual-doc-ooxml` — so the true cost was invisible until someone built past it. Six routine bumps were held red behind it. | P2 | L | 910 errors | Open (`M-009`) |
| CQ-009 | **Design documents no longer describe the implementation.** `63` specifies `--radius` 8px against a shipped 3px; `63` and `64` both specified a four-tab ribbon against five shipped; `64` contradicts itself on horizontal scrolling within one file. A design system that is not the spec of record cannot answer "is this on-system?", which is how the ARIA and radiogroup defects entered. | P2 | S | 3 drifts | **Re-opened** (#542) — the `--radius` drift was fixed in #528 (`style.css:115-117` and `63`:41-42 both say 3px), but #542 shipped a 7th and 8th tab without touching either design doc: `63`:138-139 still says "five tabs as shipped", `64`:115 says "(As shipped, five tabs…)", `64`:119 draws four, and `64`:136 still declares "Layout — *not built*" against a shipped Layout tab |
| CQ-010 | **No embed surface, so the product cannot be consumed as a library.** `main.js` executes at import and binds ~360 fixed DOM ids; there is no `mount(element, config)`, no custom element, no shadow root, and no published package. The only embedding today is an iframe of the full-chrome `editor.html`. For a project whose stated position is an embeddable runtime, this is the gap that makes the position unclaimable. | P1 | L | 0 exports · 0 packages | Open (`HF-109`, blocked on D-6) |

### What is genuinely production-grade already

Recorded so the assessment is fair and these are not disturbed:

- **The design-token system.** 1,151 `var(--)` references; exactly **one** raw hex outside
  the token blocks in 6,481 CSS lines, and it is inside a comment; **2** `!important`; all
  three theme blocks define the identical 38-token set with no gaps; pinned by
  `style_tokens.test.mjs` and an AA contrast sweep over every text node in both themes.
- **`modal.mjs`** — a correct modal contract (capture-phase Escape, Tab recovery, focusin
  re-capture, backdrop press-and-release arming, focus restoration with a visible fallback)
  applied uniformly across all 10 dialogs, and it closed a whole defect theme (T-04).
- **Engine determinism and resource bounds.** Pure render path, pinned fonts, `Stored` ZIP
  with fixed timestamps, no `HashMap`/`HashSet` in model/import/export/package/ooxml,
  `unsafe_code = "forbid"`, explicit `HARD_MAX_*` package limits, platform-gated geometry
  comparison.
- **Test volume where it exists**: 1,504 Rust tests, 455 browser tests across 114 specs.
  The problem in CQ-003/CQ-006 is blind spots and unfailable guards, not absence of testing.
- **`mc:AlternateContent` handling** and the opaque side table with rels/content-type
  merging — the best-engineered parts of the importer.

### CQ exit gates

The class closes when: no source file exceeds an agreed ceiling (proposal: 2,000 lines, with
recorded exceptions); every mutation goes through a transaction (ADR-005 provably honoured);
every guard in the suite has been proven red by mutation; the ribbon is declarative and
parity is set-equality; a localisation seam exists with at least one non-English locale
shipped; browser tests cover three engines plus an accessibility rule engine; all four fuzz
targets run in CI; layout/render/repaint benchmarks are baselined; and every published number
is generated from a committed artifact.

---

## 3. FID — rendering and round-trip fidelity

From the fidelity audit. The engine is **stronger** than `46`/`55`/`60` describe (those are
pinned to `main@cde11ff`, 992 commits behind HEAD) and **weaker** than the public page
claimed before EV-001…EV-004. The staleness corrections are §3.4.

### 3.1 Process — the two highest-leverage rows are not rendering work

| ID | Finding | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- |
| FID-P-01 | **No oracle reference is committed, so the geometry gate is inert.** The workflow is written and ready; one dispatch, a review of the blessed references, and a merge converts a disarmed gate into a live one. This is the cheapest credibility win in the repository and it gates the EV-005 assertion that the gate is armed. | P1 | S | `crates/casual-doc-render/tests/oracle_geometry.rs:186-190`; `.github/workflows/oracle-geometry.yml` | Fixed (#558) — armed on pull requests with six committed references; three fixtures held out for a platform-dependent font-parity rule, tracked separately |
| FID-P-02 | **There is no Microsoft-Word-produced fixture anywhere in the repository.** All 21 fixtures are generator output, handwritten minimal packages, or LibreOffice conversions. Word is the stated compatibility reference (`12` §Market Groups); LibreOffice is a layout *proxy* chosen in `46`. Until a rights-reviewed Word-produced corpus exists (`23`), no claim about Word-grade fidelity rests on anything a build can reproduce. | P1 | M | `fixtures/manifest.json` | Open |
| FID-P-03 | **Round-trip tests are a fixed point and cannot detect lossy import.** `assert_corpus_round_trip` asserts `reopen(source) == reopen(write(reopen(source)))`. Because the left side is itself the importer's output, **anything dropped on first import is a perfect fixed point and passes**. ~200 `*_survive_the_semantic_round_trip` tests share this blind spot; nothing in the suite compares output against the source XML. Fix: assert that no source element local-name disappears without a corresponding report entry. | P1 | M | `crates/casual-doc-export/src/lib.rs:162-167` | Open |
| FID-P-04 | **"Modeled" is counted as done while nothing consumes it.** Eight constructs are typed, cascaded and round-tripped with zero layout consumers: footnote `NoteProperties` (number format / restart / position), `w:lnNumType`, the `w:kern` size threshold, `w:kinsoku`, embedded `.odttf` faces, cell `noWrap`/`fitText`/`hideMark`/`textDirection`, `w:gutter`/`w:mirrorMargins`, and `evenPage`/`oddPage`. Each reads as finished from the model side and is invisible to a user. Fix: a model-row template field naming the consumer, and a check that a `Done` model row either has one or is explicitly marked preservation-only. | P1 | S | see FID-L-* rows below | Fixed (#558) — every model row names field, file and symbol, all three verified against source; 17 of 40 are modelled with no consumer, as a ratchet |

### 3.2 Layout and rendering gaps

| ID | Finding | Family | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- | --- |
| FID-L-01 | Embedded fonts (`.odttf`) are modeled and imported and then **never used** — no de-obfuscation code exists anywhere in layout, render, or wasm. Embedding is common in corporate and branded documents, and the substitution table cannot rescue a face the document carried. De-obfuscation is ~10 lines (`40` §3.3) plus registry wiring: **the cheapest large fidelity win in the repo.** | Fonts | P1 | S | `grep "deobfusc\|odttf\|EmbeddedFace"` over layout+render+wasm → 0; model `properties.rs:472-495`, import `font_table.rs:152-163` | Fixed (#534) — `.odttf` de-obfuscation, registered above the metric substitute; 11 mutations |
| FID-L-02 | **No hyphenation at all** — no hyphenator, no dictionary, no consumer for `w:autoHyphenation`/`w:hyphenationZone`; only `w:suppressAutoHyphens` is cascaded, with nothing to suppress. Automatic hyphenation is on by default in many European templates, and it changes line breaking and therefore pagination. | Typography | P1 | M | `crates/casual-doc-layout/src/cascade.rs:560-561` | Open |
| FID-L-03 | **`evenPage`/`oddPage` section breaks insert no parity blank page** — `SectionType` is read at exactly two sites, both testing `Continuous \| NextColumn`. Every book and report chapter break hits this. | Pagination | P1 | S | `crates/casual-doc-layout/src/columns.rs:238`; `flow.rs:6402` | Fixed (#536) — parity blank page keeps the preceding section's config, so running content follows for free |
| FID-L-04 | **~180 DrawingML preset shapes collapse to bounding rectangles**, and `a:custGeom` is architecturally unpaintable: `ShapeGeometry` has 7 presets + `Other`, and the display list has **no path/Bézier primitive** (`Rect`, `Ellipse`, `RoundedRect`, `Polygon`, `Line` only). Any document with real diagrams hits this. Fix order: add `PaintItem::Path` + `a:path` command evaluation, then table-drive the presets. | Shapes | P1 | M | model `body.rs:750-767`; `crates/casual-doc-layout/src/display.rs:114-138, 177-276` | Open |
| FID-L-05 | **Footnote number format, restart, and position are entirely unconsumed** — `NoteProperties` is fully modeled and `notes.rs` has no reader, so notes are always decimal, always continuous, always page-bottom. Authored `w:separator`/`w:continuationSeparator` are ignored in favour of a synthesized rule. Legal and academic documents depend on these. | Notes | P1 | S | model `definitions.rs:658-672`; `crates/casual-doc-layout/src/notes.rs` | **Partly fixed** (#544) — number format, start, restart (`continuous`/`eachSect`/`eachPage`) and placement (footnote `pageBottom`/`beneathText`, endnote `sectEnd`/`docEnd`) are consumed by `crates/casual-doc-layout/src/note_numbering.rs`, resolved field by field from the section then the `w:settings` default, with 10 tests in `tests/note_options.rs`. `eachPage` is a bounded second pagination pass because the marker width feeds line breaking. **Still open:** authored `w:separator`/`w:continuationSeparator` are still ignored for a synthesized rule (`notes.rs:540`, `separator: None`) |
| FID-L-06 | **The shaper's paragraph base level cannot be forced,** so an RTL paragraph whose text carries no strong RTL character reorders LTR; the resolved level is also collapsed to a single RTL flag, discarding embedding depth needed for nested-bidi caret placement. The `Start→Right` alignment remap is a correct partial workaround, and the code says so itself. Needs an upstream API or pre-reordering before the shaper. | Bidi | P1 | L | `crates/casual-doc-layout/src/shape.rs:480-505, 1125` | Open |
| FID-L-07 | **Floating tables (`w:tblpPr`) render inline** — zero hits in the layout crate. Newsletters, forms, and legal documents use them. | Tables | P1 | M | `grep "tblpPr\|float_position"` over `crates/casual-doc-layout/src` → 0 | Open |
| FID-L-08 | **Vertical and rotated text is entirely absent** (`w:textDirection` tbRl/btLr, `bodyPr vert`) — `text_direction` appears only as `None` in test scaffolding. Needs a second writing-mode axis through flow, composition, hit-test, and caret. | CJK/tables | P1 | L | `crates/casual-doc-layout/src/{notes.rs:511, document_layout.rs:1198, paginate.rs:1612}` | Open |
| FID-L-09 | **Line numbering is never generated** — `w:lnNumType` is modeled and its only three layout references are `Default::default()` in fixtures. A post-pagination margin pass mirroring `page_border.rs` closes it. Legal pleadings require it. | Page furniture | P2 | S | as above | Fixed (#541) — new `line_number.rs` post-pagination pass; also fixed `overlay_paragraph` having NO arm for `suppressLineNumbers`, so even a paragraph's own flag was lost |
| FID-L-10 | **Watermarks do not appear.** No watermark concept exists anywhere, and a Word watermark is a header shape carrying warped text (`v:textpath` / `a:prstTxWarp`) — neither text-path form is typed, so the text vanishes and only the shape box can paint. | Shapes | P2 | M | `grep -ri watermark crates/` → 0 | Open |
| FID-L-11 | **`nextColumn` is treated as `continuous`**, and the final page of a multi-column section is not balanced — the residual +2 on the Chinese SDS. Unequal columns share one galley flowed at the widest column. | Sections | P2 | M | `crates/casual-doc-layout/src/columns.rs:238`; `60` §6 | Open |
| FID-L-12 | **Tight/through wrap uses the square bounding box, not `wp:wrapPolygon`** contours; the page-coupled reflow fixed point is capped at 3 passes and falls back to a conservative envelope. Deterministic, but a deterministic approximation. | Floats | P2 | M | `crates/casual-doc-layout/src/document_layout.rs:678-698` | Open |
| FID-L-13 | **Cell `noWrap`, `fitText`, `hideMark`, and cell `textDirection` are unconsumed** — zero hits in the layout crate. Forms and dense tables depend on `noWrap` in particular. | Tables | P2 | M | as FID-L-07 | Open |
| FID-L-14 | **Emphasis marks, outline, shadow, emboss, imprint, and run borders are modeled and cascaded but unpainted.** Emphasis marks are near-universal in Japanese text; they and run borders are small additions to the decoration pass. | Run format | P2 | S | `crates/casual-doc-layout/src/cascade.rs:469` | **Partly fixed** (#541) — emphasis marks (`compose_emphasis_marks`, `compose.rs:483`) and run borders (`compose_run_border`, `compose.rs:413`) are painted as geometry, 5 guards at `compose.rs:2291-2440` (the bundled faces lack U+3001/U+25CB, so the marks are drawn rather than shaped). **Still open:** `w:outline`, `w:shadow`, `w:emboss`, `w:imprint` — cascaded at `cascade.rs:475-478` with ZERO paint sites in `compose.rs`/`display.rs`/`casual-doc-render`; `run_decoration` (`flow.rs:5841`) has no arm and says so. 4 of the 6 constructs this row names are unpainted, so `Fixed` overstated it |
| FID-L-15 | **No OpenType feature control, and the `w:kern` threshold is unapplied.** Zero feature-tag or variation-axis sites in the layout crate, so `w:ligatures`, stylistic sets, `locl` forms and old-style figures are unreachable; and because the modeled kerning threshold is never read, the engine kerns text Word would leave unkerned. | Typography | P2 | S | no `font_features` sites in `crates/casual-doc-layout`; `cascade.rs:472` vs sole consumer `casual-doc-export/src/semantic.rs:6557` | Open |
| FID-L-16 | **`w:gutter` and `w:mirrorMargins` never reach the page configuration** — `PageConfig` has no gutter field and every `gutter` reference is test scaffolding. Bound documents print with the wrong margins. | Sections | P2 | S | `crates/casual-doc-layout/src/paginate.rs:55-136` | Partly fixed (#536) — gutter and `mirrorMargins` reach page geometry. **`w:gutterAtTop` still open**, and deliberately: `DocumentSettings` has no field and the importer routes it to the report, so closing it without an export arm would turn a REPORTED loss into a silent one. Three-crate unit |
| FID-L-17 | **`w:kinsoku` is cascaded and never consumed**, and line breaking is UAX #14 via ICU with no Word-compatible kinsoku table, so CJK line breaks diverge from Word's. | CJK | P2 | M | `crates/casual-doc-layout/src/cascade.rs:548` | Open |
| FID-L-18 | **`w:jc="distribute"` silently collapses to ordinary justification**, and there is no kashida for Arabic or inter-character distribution for CJK. The import maps it to `Justify` and the call site treats that as fully mapped, so no finding is raised (see FID-R-04). | Justification | P2 | S | `crates/casual-doc-import/src/properties.rs:518`; export `semantic.rs:6722` | Partly fixed (#541) — the loss is now REPORTED instead of silent. Real inter-character distribution still open: needs an `Alignment::Distribute` variant, import arm, shaper support, and export arm |
| FID-L-19 | **Character-grid snapping is not applied.** `w:docGrid` line pitch reaches layout correctly (with exact/paragraph/table precedence and `w:snapToGrid` gating), but `w:charSpace` and `linesAndChars` character snapping do not. | CJK | P3 | M | `crates/casual-doc-layout/src/flow.rs:233-236, 6362-6364` | Open |
| FID-L-20 | **EMF/WMF metafiles and browser-build SVG paint a placeholder.** SVG rasterizes on native only (`resvg` is a `cfg(not(wasm32))` dependency) and `usvg/text` is disabled, so SVG `<text>` never renders on any build. Needs a metafile interpreter or a host rasterization seam. | Images | P3 | L | `crates/casual-doc-render/Cargo.toml`; `lib.rs:404, 459-474` | Open |
| FID-L-21 | **`rich` and `table-merges` bottom edge diverges ~240 twips from LibreOffice** — and from BOTH 26.2.4.2 and 24.2.7.2, so it is ours, not version drift. `rich` y1 3748 vs 3501/3508; `table-merges` y1 3758 vs 3813/3830. Far past the documented 4–8 twip `lineGap/2` residual, so it is not leading distribution. **This is the first genuine engine signal the oracle gate has produced.** | Tables | P2 | M | measured locally against both LO builds; **not reproducible from the repo** — `fixtures/oracle/` is uncommitted, so `oracle_reference()` (`oracle_geometry.rs:399`) returns `None` and the content gate skips both fixtures. See #541 and FID-P-01 | Open |
| FID-L-22 | **The editor cannot place a caret in an empty running band, so a blank header cannot be typed into.** `zoom-editing-context.spec.mjs:50` and the `[footer]` cases of `surface-editing-matrix.spec.mjs` fail on `main` right now. Latent for a long time; exposed by #538, which correctly stopped an absent `first` variant from falling back to `default`. Per MS-OI29500 §17.10.6 the fix is to take the spec literally — entering a band whose variant has no reference CREATES it, one empty paragraph, undoable. **Do not revert #538's F1**: `titlePg` with no first-page reference must render blank, which is how "no header on page 1" works. | Running content | P1 | M | bisected: `63dd2c4` passes, `c4c71dc` (#538 alone) fails 3/3. **Diagnosis:** `running_content_caret` (`casual-doc-wasm/src/lib.rs:986-992`) still walks `[variant, Default]` and its comment claims to mirror `HeaderFooter::select` — a fallback #538 removed. It therefore returns a caret in a story page 1 does not paint, so `data-running-edit` is set with no caret geometry. The create-on-entry path already exists (`main.js:5015,5264`; `create_running_content`, `wasm/src/lib.rs:2819`) but is shadowed by that stale `Some`. The `[header]` cases fail identically — `scrollTo` always takes `.page-wrap .page` `.first()`, and `sample.docx` §1 sets `w:titlePg` with only a `default` reference | Fixed (#543, commit `c77237b`) — `running_content_caret` no longer falls back to the `default` variant, matching `HeaderFooter::select` and ECMA-376 §17.10.6: a page that paints a blank `first` band now gets `None`, which routes the host to `createRunningContent`, the "shall create a new blank header" half of the same sentence. Re-verified on current `main`: `zoom-editing-context`, `surface-editing-matrix` and `header-footer-editing` all pass (100/100). The diagnosis text above describes the pre-fix state and is kept as history |

### 3.3 Round-trip and disposition-reporting gaps

| ID | Finding | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- |
| FID-R-01 | **The DOCX exporter has no compatibility-reporting path at all** — it returns `CompatibilityReport::default()` unconditionally. The ODF exporter reports, and even carries a residual check. So export-side loss on the primary format is structurally unreportable. | P1 | M | `crates/casual-doc-io/src/docx.rs:160-164`; contrast `casual-doc-odf/src/export.rs:4603-4606` | Fixed (#541) — both the Semantic and PreserveWhenSafe paths report; `CompatibilityReport::default()` now survives only on `ExactIfUnchanged`, which returns the source bytes unaltered (`docx.rs:200-257`). Dispositions are one `Disposition` enum over doc 35's nine legal pairs, so an illegal pair is unrepresentable; `every_disposition_is_one_of_the_nine_legal_pairs` (`export/src/report.rs:230`) proves all nine reachable. **Correction:** an earlier draft of this row claimed "all 14 fixtures export an empty report" — no such sweep exists. The empty-report guard is ONE fixture (`real-producer-rich.docx`), asserted in the writer and the adapter. Widening it to the 8 corpus DOCX files is the remaining work |
| FID-R-02 | **`ModelOutcome` is hardcoded `Omitted` at all three construction sites** — `Degraded` and `Mapped` are never constructed on the DOCX path — and `RetentionOutcome` is a per-*mode* constant, not a per-construct fact (Retention stamps everything `Preserved`, Semantic stamps everything `NotRetained`); `Blocked`, `Rejected` and `NotApplicable` are never constructed anywhere. `35`'s entire purpose — distinguishing "degraded, remainder preserved" from "degraded, remainder lost" — is currently inexpressible. There is also no preservation ledger, no ledger-ID field, and no validator of the 9 legal combinations `35` mandates. | P1 | M | `crates/casual-doc-import/src/report.rs:129, 138, 164`; `lib.rs:1044-1047` | Open |
| FID-R-03 | **Unknown *attributes* are outside the report vocabulary entirely.** `Reporter::report` takes an element local-name and `CompatibilityEntry` has no attribute field, so `w:rsidR`/`rsidRPr`/`rsidDel`/`rsidTr` — present on nearly every `w:p`, `w:r` and `w:tr` — plus `mc:Ignorable` and `w14:paraId` cannot be described even in principle. | P1 | M | `crates/casual-doc-import/src/report.rs` | Open |
| FID-R-04 | **Three parsers have zero reporting and their parts are regenerated, so loss is permanent and invisible:** `theme.rs`, `font_table.rs`, `comments_ext.rs` have no `.report(` sites. Concretely `a:objectDefaults`, `a:extraClrSchemeLst`, `a:extLst` and `a:theme/@name` are silently dropped on every semantic save. Every document has a theme and a font table. Also unreported: unknown children of `w:styles` (`styles.rs:426` skips the subtree silently, in a file that reports densely elsewhere), `w:numPicBullet` (falls to a bare catch-all), and `w:background` (imported but never emitted, with no finding). | P1 | S | `crates/casual-doc-import/src/{theme.rs, font_table.rs, comments_ext.rs}` → 0 report sites; `numbering.rs:520-523`; `lib.rs:1218-1222` | **Partly fixed** (#540/#541) — `theme.rs`, `comments_ext.rs`, `styles.rs`, `numbering.rs` now report (6/10/45/17 sites). **Still open, and both are named in this row's own headline:** `font_table.rs` has ZERO report sites and is not even passed a reporter (`import/src/lib.rs:1072`), so unknown `w:fonts`/`w:font` children are still dropped silently from a part regenerated on every save (`font_table.rs:61,126`); and `w:background` is imported (`lib.rs:1218-1221`) but still never emitted — #541 only NAMES the loss as `docx.export.background` (`semantic.rs:794-799`), which converts a silent loss into a reported one, not a fixed one |
| FID-R-05 | **Retained opaque parts are never invalidated on edit** — `casual-doc-edit` and `casual-doc-transaction` have no knowledge of the side table. A stale thumbnail, a stale `stylesWithEffects.xml` contradicting a regenerated `styles.xml`, and stale data-bound `customXml` all survive into the saved package. | P2 | M | `crates/casual-doc-{edit,transaction}` vs `casual-doc-import/src/opaque.rs:50-63` | Open |
| FID-R-06 | **Missing media on export writes a zero-byte part with a valid relationship** — Word then shows a broken-image box rather than reporting a problem. Same pattern for `.odttf` parts. | P2 | S | `crates/casual-doc-export/src/semantic.rs:704-707, 716` | Fixed (#541) — the part, its relationship, its content-type entry and the body reference are omitted TOGETHER and reported, rather than emitted empty (a dangling `r:embed` is repair-the-file in Word). Also caught a worse case: a semantic chart export wrote a live relationship to an absent part |
| FID-R-07 | **Sub-part "retention" is not byte-exact:** retained OMML subtrees and theme `a:fmtScheme` are re-serialized through a `quick_xml::Writer`, normalizing quoting and self-closing forms. Lossless in content, not in bytes — the distinction the public page now draws must hold here too. | P3 | S | `crates/casual-doc-import/src/body.rs:1738-1750` | Open |
| FID-R-08 | **Charts, SmartArt and OLE are preserved but never drawn** — an embedded preview if the file supplies one, else a typed text placeholder. Correct today and correctly disclosed; recorded because it is the largest single capability gap against ONLYOFFICE and needs a scope decision, not a bug fix. | P2 | L | `crates/casual-doc-layout/src/flow.rs:2831, 3002-3022` | Open |

### 3.4 Fidelity-document staleness corrections

`46`, `55` and `60` are pinned to `main@cde11ff` (2026-07-27), **992 commits** behind HEAD,
and `44` self-declares as a historical register. They systematically *understate* the
engine. Corrected by this PR with a staleness banner on each, listing what has since
shipped:

| Document | Now-stale gap claims — these shipped |
| --- | --- |
| `46` §F7 | Drop caps (a real paragraph-float exclusion, plus margin mode) |
| `55` §7 | Double strike-through |
| `55` §8 | GIF/BMP/TIFF/WEBP decode (the doc says "PNG and JPEG only") |
| `55` §9 | Shape rotation and flip |
| `55` §11 | Review view policy (`ReviewView::{Editing, Markup}` with struck deletions and author colour) |
| `55` §12 | Page borders (`display`/`offsetFrom` policies honoured); section page vertical alignment including `Both` |
| `60` §8 | Content-control checkbox checked-state glyphs (legacy `w:ffData` and `w14:checkbox`) |
| `18` Notes row | Footnote/endnote layout — reference markers, page-bottom bands, separator rules, per-column bands, cross-page continuation, endnote append |

`60`'s parity table is itself still correct and is now the cited source for the public
page's numbers: Class notes 1/1, Medical form 4/4, Chinese SDS 18 vs 16, Sample 26/26,
demo 8/8 → **4/5 exact, worst delta +2**, measured 2026-07-27 and not re-measured since.

---

## 4. OO — ONLYOFFICE Document Editor fit-gap

**Reference:** ONLYOFFICE Docs **9.4.0** (released 2026-05-20), Document Editor only.
Built from ONLYOFFICE's own sources rather than its marketing pages: the client repo
`ONLYOFFICE/web-apps` (~208 KLOC for the client shell of one editor family; 76.6k JS LOC
in `documenteditor/` alone), its UI string table `apps/documenteditor/main/locale/en.json`
(4,479 keys, 662 of them `DE.Views.Toolbar.*` — the authoritative list of every button,
tooltip and dialog field the product ships), plus `sdkjs`, `core`, and `server` for the
architecture claims. The help-centre ribbon pages were checked and are near-useless for a
granular inventory — each is a screenshot plus a short bullet list.

`sdkjs` could not be read directly: the tarball arrived truncated with no `word/`
directory. Engine claims below are from the UI's side of the `asc_*` boundary and from
`sdkjs` paths verified via raw file reads, and are marked where inferred.

### 4.1 Read this before the gap tables

Three findings reframe the comparison, and each cuts against the assumption that a
feature list is the thing to close.

**(a) ONLYOFFICE's web client is structurally incapable of local-first operation.**
This is not a default, a licence tier, or a configuration choice. Format I/O is `x2t`, a
native C++ binary that converts OOXML to an internal `Editor.bin` (the format family is
literally named `AVS_OFFICESTUDIO_FILE_CANVAS 0x2000` in `core/Common/OfficeFileFormats.h`),
and **`core/X2tConverter/build/` contains only `Android/` and `Qt/` — there is no
WebAssembly build of x2t.** So the browser cannot open or save a document without a running
DocumentServer. The document `key` is a server session identifier invalidated after save;
there is no durable local document model. Persistence is the host's `callbackUrl` handler —
the editor never owns the bytes. The only way to get all of this locally is to ship the
native core inside a Chromium shell, which is exactly what the desktop and mobile apps do,
**and which ONLYOFFICE does not offer as an embeddable client library.** A narrow
`Asc.Addons.ooxml` addon can read and write DOCX in-browser given `document.directUrl`, but
it is capability-gated, not the Community path, and not a general offline mode.

OpenDoc's entire premise — `12` §Product Position, "a local-first document runtime… without
requiring a bundled editor UI, cloud service, framework, or collaboration vendor" — is
therefore not a smaller version of ONLYOFFICE. It is the thing ONLYOFFICE cannot do. Feature
rows below must not be closed in a way that costs this.

**(b) The conversion round-trip is a structural fidelity disadvantage for them, and the
clearest available win for us.** DOCX → `Editor.bin` → DOCX means every save re-serializes
through an intermediate model, so anything that model does not represent is dropped rather
than preserved. ONLYOFFICE acknowledges this in its own API surface —
`customization.compatibleFeatures`, `forceWesternFontSize`, a `textConvertEquation` prompt
asking users to convert legacy equations to a supported form, and a whole `docxf`/`oform`
format family invented because the model could not carry form metadata DOCX lacked. An
engine that reads and writes OOXML directly and keeps unknown parts verbatim — which is
what the opaque side table and retention floor are for — wins here by construction. That
makes the FID-R-01…FID-R-04 reporting rows (§3.3) *competitive* work, not hygiene: the
advantage only holds if loss is actually detected and reported.

**(c) Their co-editing is not what it is usually described as, which matters for ADR-030.**
A grep of `DocsCoServer.js` for `transform` returns zero hits. The real model is a
**server-serialized change log plus pessimistic object-level locking plus client-side
undo/rebase**: a global 60-second save lock serializes writers, a monotonic `puckerIndex`
totally orders the log, and each client rolls back its local changes, replays the
server-ordered log, then re-collects and re-locks (`sdkjs/common/CollaborativeEditingBase.js`,
lock types `kLockTypeNone/Mine/Other/Other2`). There is **no OT, no CRDT, and no
peer-to-peer path**; transport is socket.io, not raw WebSocket. So the still-open OT-vs-CRDT
decision (`08`, `45`) cannot cite ONLYOFFICE as precedent for either. It is precedent for a
third option — ordered log plus locks — which is cheaper than both and which a local-first
engine with stable `NodeId`/`ModelPos` anchors (invariant I3) could implement without a
mandatory server.

**What ONLYOFFICE should be treated as precedent for is its renderer, not its topology.**
Canvas painting with a WASM font engine doing its own shaping and rasterization
(`common/libfont/engine/fonts.wasm`, 3.6 MB, FreeType 2.10.4, with a 6.9 MB asm.js
fallback), its own line breaking, bidi and grapheme handling, plus a WASM PDF/DjVu/XPS
reader (`pdf/src/engine/drawingfile.wasm`, 10.2 MB). Spell checking is likewise now
client-side WASM (`sdkjs/common/spell/spell/spell.wasm`) — the server-side SpellChecker
service is retired. That is the same architectural bet OpenDoc has already made with
parley/harfrust/skrifa and `tiny-skia`, and it is independent evidence the bet is right.

### 4.2 Where OpenDoc is already ahead

Recorded so these are not "closed" by accident while chasing parity, and so the report does
not read as a one-way deficit list.

| Capability | OpenDoc | ONLYOFFICE 9.4 |
| --- | --- | --- |
| **Local-first / offline** | Whole engine is local; no server exists or is needed | **Impossible in the browser** — no WASM x2t build (§4.1a) |
| **Embeddable as a library** | The stated v1 goal; WASM facade with 449 exports today | Not offered. An iframe plus a server, or the native core in a Chromium shell |
| **Format preservation** | Direct OOXML read/write; unmodeled parts kept byte-for-byte via the opaque side table | Lossy by construction through `Editor.bin` (§4.1b) |
| **Table sorting** | Ships (`sort_table`, `73`) | **Does not exist** — no Sort command anywhere in the Document Editor |
| **Decimal tab stops** | Ship, and render (`TabAlignment::Decimal`) | **Left / Center / Right only** — no decimal, no bar tab |
| **Colour glyphs** | `sbix`/`CBDT` strikes + `COLR` v0/v1 with `CPAL` | Not a documented capability |
| **Loss reporting as a typed contract** | A two-axis disposition taxonomy (`35`), however incompletely enforced today | Ad-hoc `compatibleFeatures`-style flags |
| **Memory safety / resource bounds** | `unsafe_code = "forbid"`, fuzzed parsers, explicit `HARD_MAX_*` package limits | C++ core plus a 208 KLOC JS client |

Two more where **neither** product has the capability, so they are not fit-gap rows at all
and should not be written up as ONLYOFFICE advantages: a native citation/bibliography
manager (ONLYOFFICE is Zotero/Mendeley plugins only) and an accessibility checker.

### 4.3 Ribbon and IA comparison

ONLYOFFICE ships **11 tabs** — File, Home, Insert, Draw, Layout, References, Forms
(PDF files only), Collaboration, Protection, View, Plugins — plus **two** contextual tabs,
Header & Footer (9.3) and Chart Design (9.4), and an AI tab injected by a plugin (there is
no `AI` string in the Document Editor locale, so it is not a native tab). OpenDoc ships
**five**: Home, Insert, Table, View, Review.

The structural difference is not tab count. **ONLYOFFICE has almost no contextual ribbon —
no Table Design, Table Layout, Picture Format or Shape Format tab — and puts object
formatting in a right sidebar of 13 typed panels instead.** OpenDoc already has both halves
of that pattern (a contextual Table tab *and* Paragraph/Table properties panels), so the
IA gap is narrower than the 11-vs-5 count suggests. The real gaps are the two tabs that
correspond to capability OpenDoc has but has not surfaced (**Layout** — UX-010) or does not
have at all (**References**).

| ONLYOFFICE tab | OpenDoc equivalent | Verdict |
| --- | --- | --- |
| File (backstage: New, Open Recent, Save Copy As, Info, Version History, Protect, Advanced Settings) | App menu File section only | **Gap** — UX-011, OO-002, OO-011 |
| Home | Home | Close. Gaps are specific controls (OO-020, OO-021) |
| Insert | Insert | Close for objects; gaps are TOC/caption/cross-ref (References) and content controls (OO-014) |
| Draw (Pen/Highlighter/Eraser) | — | Gap, low value — OO-019 |
| Layout | Scattered across View / Tools / Insert | **Gap, and the cheapest one** — UX-010 |
| References | — | **Largest single IA gap** — OO-001, OO-005, OO-006 |
| Forms | — | **Not a DOCX gap.** ONLYOFFICE's form designer is PDF-only; DOCXF/OFORM are retired |
| Collaboration | Review (tracking, comments) | Partial — OO-003, OO-004, OO-007, OO-018 |
| Protection | — | Gap — OO-012 |
| View | View | Close; OpenDoc lacks multi-page view and zoom-to-100% |
| Plugins | — | Gap, deliberately deferred — OO-017 |
| Header & Footer (contextual) | Insert ▸ Header/Footer + band editing | Capability ships; the contextual tab does not |
| Chart Design (contextual) | — | Gated on FID-R-08 (charts are not drawn at all) |

### 4.4 OO rows

Ranked within priority by (how often a real document workflow needs it) × (cost of not
having it). **Effort is engine-inclusive**, so several rows are L where the UI alone would
be S — noted per row.

| ID | Gap vs ONLYOFFICE 9.4 | Pri | Eff | What ONLYOFFICE ships | OpenDoc state | Depends on | Status |
| --- | --- | --- | --- | --- | --- | --- | --- |
| OO-001 | **No table of contents.** The single most-expected long-document feature. | P1 | L | Insert TOC from outline levels 1–9 or selected styles; page numbers, right-align, leader, format-as-links; 5 layout presets; *Update entire table* / *Update page numbers only*; per-level `toc N` styles; a Headings navigation panel with promote/demote | `TOC1` styles round-trip and a tabbed TOC row is clickable, but nothing **generates** or **updates** a TOC. Needs a field engine for `TOC`/`PAGEREF` | FID-L-05 pattern; field engine | Open |
| OO-002 | **No New document, no recent files, no templates, no backstage.** | P1 | M | File tab with Blank document, Create from Template (desktop), Open Recent, Save Copy As, Info, Rename | Nothing — the only way in is the OS picker or the bundled `sample.docx` | UX-011 (same row) | Open |
| OO-003 | **No spelling or grammar check.** Less feedback than a plain `<textarea>`. | P1 | L | Spell check with red underline, Ignore/Ignore all/Add to dictionary, per-word language, ignore-UPPERCASE and ignore-with-numbers options. **Now client-side WASM** (`spell.wasm`) — the server service is retired, which is direct evidence this is doable locally | Nothing. `HF-035` | UX-001 (needs the editable focus owner first) | Open |
| OO-004 | **No version history and no autosave/crash recovery.** A tab crash loses everything. | P1 | L | Versions vs revisions, author + timestamp, per-contributor change colouring, Restore, Download version, Preview, Mark as version | Nothing. `HF-011`, `HF-068` | Host storage decision (owner decision 2026-09: storage YES) | Open |
| OO-005 | **No captions and no cross-references.** Blocks any figure- or table-numbered document. | P1 | L | Captions with custom labels, Before/After, exclude label, include chapter number, separator choice, auto-created Caption style; cross-references across 7 reference types with per-type targets, insert-as-link, above/below | Neither. Needs `SEQ`/`REF`/`PAGEREF` field evaluation | field engine | Open |
| OO-006 | **No hyphenation, line numbering, watermark, or drop-cap authoring** — four Layout/References features where the *model* mostly exists and the *consumer* does not. | P1 | L | Hyphenation (auto, hyphenate-CAPS, hyphenation zone, consecutive-hyphen limit); line numbers (continuous / restart page / restart section / suppress for paragraph); watermark dialog (text templates, 8 languages, font, semitransparent, diagonal/horizontal, or image with scale); drop cap in-text/in-margin with height-in-rows and full frame settings | Drop caps **render** but are not authorable; line numbering is modeled and unconsumed; hyphenation and watermarks do not exist | FID-L-02, FID-L-09, FID-L-10, and the drop-cap row | Open |
| OO-007 | **No document comparison or combine.** | P2 | L | Compare/Combine from file, URL or storage; character-level or word-level setting; pre-existing tracked changes accepted on compare, merged on combine; result saved as a new version | Nothing. The revision model that a diff would emit into already exists, which makes this tractable | OO-004 | Open |
| OO-008 | **Table formulas are 4 functions over 2 directions; ONLYOFFICE has 18 over 4.** | P2 | M | ABS, AND, AVERAGE, COUNT, DEFINED, FALSE, IF, INT, MAX, MIN, MOD, NOT, OR, PRODUCT, ROUND, SIGN, SUM, TRUE; `A1` and `A1:B3` refs; bookmarks as arguments; ABOVE/LEFT/BELOW/RIGHT; 7 number formats. Manual recalc only (F9) — so **not** a live spreadsheet | `SUM`, `AVERAGE`, `MIN`, `MAX` over `ABOVE` or `LEFT` only (`=SUM(ABOVE)`); no cell refs, no bookmarks, no number formats | — | Open |
| OO-009 | **No equation editor.** | P2 | L | 12 gallery groups (86 symbols, 41 bracket forms, 40 large operators, 27 accents, 27 functions, 22 matrices); **UnicodeMath and LaTeX input**; Professional/Linear display; per-construct context commands; MathML insertion; 600+ Math AutoCorrect codes | OMML is fully typed and the common arms typeset; authoring is explicitly excluded by `86`. `99` §2 already requires an authority ADR first — do not let UI-only synthesis silently replace unsupported math | `99` §2 P1 design gate | Open |
| OO-010 | **No print dialog.** Print emits a 150-DPI raster through the browser. | P2 | M | In-editor print-with-preview: printer, copies, range (All/Current/Selection/Custom), size, orientation, margins, **duplex with long/short-edge flip**, colour vs black-and-white, Print to PDF, system dialog, Quick Print. PDF output is real text | `window.print()` over engine-rendered rasters — no selectable text, no page range, no duplex, and a long document exhausts the tab. `HF-030`, `HF-036`, `HF-105` | `98`/ADR-031 for real PDF | Open |
| OO-011 | **No document protection, password, or digital signature.** | P2 | M | Protect Document with 4 restriction levels (Read only / Comments / Filling forms / Tracked changes) + password; file encryption; invisible digital signature and visible signature line (desktop only) | Nothing. Note the restriction levels map cleanly onto the existing Editing/Suggesting/Viewing mode model, so the UI cost is low; the DOCX `w:documentProtection` model and enforcement are the work | — | Open |
| OO-012 | **No content-control authoring.** | P2 | M | 7 control types (plain text, rich text, picture, combo box, drop-down, date picker, check box) with a full settings dialog — title, tag, placeholder, show-as, colour, locking, item lists, date format, checked/unchecked symbols | `w:sdt` models and round-trips, content edits as ordinary paragraphs, and checkbox controls paint their state glyph. No authoring, no chrome, no placeholder text | — | Open |
| OO-013 | **No mail merge.** | P3 | L | Data from an `.xlsx` only; edit recipient list, insert merge field, highlight fields, preview with record navigation, merge to DOCX/PDF/Email, scope all/current/range. Capped at 100 recipients, portal-bound, no wizard, no rules, no envelopes or labels | Nothing. Note how narrow theirs is — this is a weak parity argument and it needs a host data-source contract, hence P3 | host data contract | Open |
| OO-014 | **Charts and SmartArt are not drawn, so there is nothing to author.** | P2 | L | A full chart editor over the **embedded XLSX directly** (9.1+), ~40 insertable chart types, combo charts, secondary axis, 3-D rotation, trendlines, error bars, data tables, and a Chart Design contextual tab. SmartArt has **159 named layouts** — but editing is **formatting-only**: no text pane, no add/promote/demote, no change-layout, no styles gallery | Preserved byte-for-byte; painted as the embedded preview if present, else a text placeholder. Same row as FID-R-08 — needs a scope decision, not a bug fix. The SmartArt limitation above means *authoring* parity is a much lower bar than it looks | FID-R-08, FID-L-04 | Open |
| OO-015 | **No word count dialog, and no selection-scoped counts.** | P3 | S | Status bar plus Document Info statistics: words, symbols, symbols with spaces, paragraphs, pages | Footer shows words/chars/paras; no dialog, no selection scope. `HF-051`. The engine already exposes `document_stats`, `words`, `characters`, `characters_with_spaces` — this is UI-only | — | Open |
| OO-016 | **AutoCorrect is smart quotes only.** | P3 | M | Four tabs — Math AutoCorrect (600+ codes), Recognized Functions, AutoFormat as you type (smart quotes, hyphens→dash, auto-hyperlink, auto bulleted/numbered lists, period on double-space), Text AutoCorrect (capitalize sentences and table cells, per-language exceptions) | Smart quotes, with a known bug after non-ASCII characters (`HF-055`). Autoformat list triggers do not exist | — | Open |
| OO-017 | **No plugin or macro surface.** | P3 | L | A plugin marketplace (nothing bundled since 8.2), 6 plugin types, `executeMethod` with 122 documented methods, and JavaScript macros with **recording** (9.2) — though macros are per-document only, with no global library and no autostart | Nothing, deliberately: the plugin ABI is an open decision (`08`) and ADR-030/`45` reserve the seams. Recorded for completeness, not proposed | ADR-030, plugin ABI decision | Open |
| OO-018 | **No collaboration, presence, sharing, or roles.** | P1 | L | Fast and Strict co-editing, presence, per-user cursor labels, comment mentions, chat, 7 sharing roles. But see §4.1c — ordered log plus locks, not OT or CRDT, and a mandatory server | Nothing, and correctly sequenced behind a stable command vocabulary (`99` order 6). `HF-114`. The finding that changes the decision is §4.1c | ADR-030 OT-vs-CRDT | Open |
| OO-019 | **No freehand drawing (Draw tab).** | P3 | M | Pen, Highlighter, Eraser with colour and mm size. Minimal even by their standards — whole-stroke eraser only, no ink-to-shape, no ink-to-math, no lasso | Nothing. `w:contentPart` ink is `44` Tier 4, preserved opaque | — | Open |
| OO-020 | **Ribbon/UI breadth rows worth copying cheaply:** 8 interface themes vs 2; 46 locales vs 1 (UX-009); user-remappable shortcuts over a closed 147-command set, synced across tabs via the `storage` event — against 18 bound chords here (UX-006); tooltips that render the *live* binding rather than a hardcoded glyph (UX-009); an Alt-key access-key overlay with auto-assigned, localised hint letters; a two-level roving-focus layer over declared regions (UX-020/UX-021); declarative once-only onboarding tips. | P2 | M | see left | Each maps onto an existing UX row; ONLYOFFICE supplies the proven shape | UX-003 (`commands.mjs`) | Open |
| OO-021 | **Specific Home/Insert controls absent here:** paragraph borders and shading gallery (borders exist in the paragraph inspector but not as a Home control), horizontal line, multilevel-list gallery, list settings dialog (number format, start-at, restart, follow-number-with, tab stop), OpenType ligature control (16 modes — cf. FID-L-15), character spacing and position, small-caps/all-caps toggles, eyedropper, Blank Page, section-break insertion (`99` §3), Text from File, Insert Spreadsheet (OLE). | P2 | M | see left | Mixed: some are model-complete and unsurfaced, some absent | FID-L-15 for ligatures | Open |

### 4.5 Rows deliberately not opened

| Not following | Why |
| --- | --- |
| DOCX form designer / Forms tab | ONLYOFFICE's is **PDF-only**; DOCXF and OFORM are retired. Not a DOCX gap |
| Mandatory document server, `callbackUrl` persistence, server-held document `key` | Rejected by `12` §Market Groups — "do not compete by shipping another mandatory document server". These are the cost of their model, not features |
| `Editor.bin` intermediate format | Directly contrary to the preservation floor (§4.1b) |
| Citation/bibliography manager | Neither product has one natively; theirs is Zotero/Mendeley plugins |
| Accessibility checker | Neither has one. OpenDoc's own gap is `HF-064`, judged against Word, not against ONLYOFFICE |
| AI tab, AI agent, MCP server | Plugin-delivered and out of the runtime's scope. Note for the extensibility seams (`45`): their AI surface is worth studying as a *consumer* of a document API, not as editor capability |
| 25 colour schemes, 11 texture fills, Text Art transform gallery | Breadth with no fidelity or safety consequence; revisit only after the P1/P2 rows |


## 5. Recommended order

Ranked by (unblocking value) × (cost of leaving it wrong), not by severity alone.

| Step | Work | Why here |
| --- | --- | --- |
| 1 | EV-001…EV-004 | Done in this PR. A false public claim outranks everything: it is the one defect class that makes every other number in the project unciteable. |
| 2 | FID-P-01, FID-P-02 | Two cheap moves that convert "measured against an oracle" from aspiration to fact, and they gate EV-005. Neither is rendering work. |
| 3 | UX-001 | The unlock. Soft keyboard, real IME, dictation, touch selection and any future spellcheck all sit behind one editable focus owner. Also closes UX-002. |
| 4 | UX-003 step 1 (`commands.mjs`), then UX-004 | Extract the registry, then make parity a set-equality test. In that order — the test is unwritable until the registry is exportable. Unblocks UX-005…UX-008 and UX-015. |
| 5 | UX-009 (`strings.mjs` + `shortcutLabel`) | Mechanical, and it stops showing the wrong keyboard to most desktop users. Opens the i18n seam. |
| 6 | FID-L-01, FID-L-03, FID-L-05, FID-L-09, FID-R-04 | The S-effort fidelity wins: embedded fonts, parity breaks, note options, line numbers, and reporting on three silent parsers. |
| 7 | UX-017, UX-020, UX-021, UX-022 | Feedback and accessibility: the editor must be able to say something, and a screen-reader user must be able to find the caret. |
| 8 | FID-P-03, FID-P-04, FID-R-01…FID-R-03 | The loss-reporting substrate. Do this before the long-tail construct work, so new gaps report themselves. |
| 9 | UX-010…UX-014, UX-018, UX-019 | Layout/File tabs, menu taxonomy, then touch and phone layout. |
| 10 | FID-L-02, FID-L-04, FID-L-06…FID-L-08, FID-R-08 | The M/L engine work: hyphenation, shape paths, bidi base level, floating tables, vertical text, and the charts/SmartArt scope decision. |

## 6. Open questions for the owner

1. **Charts and SmartArt (FID-R-08).** Preserve-and-placeholder is honest and cheap; live
   rendering is a subsystem each. ONLYOFFICE renders and authors both. Is drawing them in
   scope before a stable SDK, or is preservation the v1 answer?
2. **Word-produced corpus (FID-P-02).** Acquiring rights-cleared Word output is a
   licensing and privacy question, not an engineering one. Generate with a licensed copy
   and review for redistribution, or keep the corpus local and publish only measurements?
3. **`w:rsid*` policy (FID-R-03).** Revision-save IDs are high-volume and low-value.
   Report them per-construct, report once per document as a class, or declare them
   explicitly out of scope in `35`?
4. **Mobile commitment (UX-001, UX-018, UX-019).** `18` declares mobile and tablet
   browsers supported. Meeting that is an editable focus owner, touch selection, a pinch
   path, and a phone layout tier — a substantial slice. Confirm it stays a v1 target, or
   downgrade the support matrix so the claim matches the product.
5. **Hyphenation dictionaries (FID-L-02).** Bundling per-language dictionaries has size,
   determinism and licensing consequences. Bundle a versioned set, or require the host to
   supply them through a registry seam as with fonts?
