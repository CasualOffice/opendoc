# 105 — September 2026 Audit Tracker

**Status:** Living record. **Opened:** 2026-09-15. **Owner:** unassigned.

**Scope:** the findings of the 2026-09 audit round — an editor UI/UX audit, a DOCX
rendering-and-round-trip fidelity audit, and an ONLYOFFICE Document Editor fit-gap
comparison built from ONLYOFFICE's own client source rather than its marketing pages.

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

| Class | Rows | P0 | P1 | P2 | P3 | Open |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| EV — evidence and public claims | 6 | 4 | 1 | 1 | 0 | 2 |
| UX — editor UI/UX | 24 | 1 | 12 | 10 | 1 | 24 |
| FID — rendering and round-trip fidelity | 25 | 0 | 9 | 14 | 2 | 25 |
| OO — ONLYOFFICE fit-gap | pending | — | — | — | — | — |
| **Total** | **55** | **5** | **22** | **25** | **3** | **51** |

Four EV rows are closed by the PR that opens this tracker; they are recorded rather than
deleted so the correction is auditable.

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
| UX-001 | **There is no editable focus owner, so the editor cannot accept text on a touch device and its IME path cannot fire.** `#pages` is a plain `div[tabindex="0"]`; all text entry is a `document` `keydown` listener. There is no `<textarea>`, no `contenteditable`, no `inputmode` proxy anywhere. Consequences: (a) tapping the page on iOS/Android raises **no soft keyboard** — while `18` declares mobile/tablet browsers a supported target (owner decision 2026-09-02); (b) `compositionstart/update/end` do not fire from a non-editable element, so the whole IME preedit path is dead for real CJK/Korean/Vietnamese input; (c) no dictation, no platform text services, and no seam for spellcheck. **This is the unlock row** — the mobile half of UX-018/UX-019, the IME half of this row, dictation, and HF-035 all sit behind it. | P0 | L | `webapp/editor.html:1072`; `webapp/src/main.js:2461-2467, 2478-2480, 14714`; `18-SUPPORT-MATRIX.md` mobile row | Open |
| UX-002 | **The IME test is green while the feature is broken.** `ime-preedit.spec.mjs` dispatches synthetic `CompositionEvent`s on `document`, which a real IME cannot do against a non-editable element. The suite therefore certifies a path no user can reach. A guard that cannot fail on its own subject is worse than no guard. | P1 | S | `webapp/tests/e2e/ime-preedit.spec.mjs:33-52` | Open |
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
| UX-010 | **No Layout tab; page and section commands are scattered across four surfaces.** Page setup is on the **View** ribbon and in the **Tools** menu. Paragraph properties is on **Home**, in **Format**, and in **Tools**. Header/footer and its first-page/odd-even variants are under **Insert**. Meanwhile the palette already declares a `group: "Layout"` for four commands that have no Layout home. Columns, margins, orientation, page size, breaks, indent and spacing all ship — that is a filled Layout tab. `64`:87 rejected a Layout tab on "only tabs we can fill" grounds; that premise has expired. | P1 | M | `webapp/editor.html:492`; `webapp/src/main.js:11654-11683, 11914`; `64-EDITOR-TOOLBAR-RIBBON-DESIGN.md:87` | Open |
| UX-011 | **No File backstage, no New document, no recent files.** There is no `file.new` anywhere; the only way in is the OS picker or the auto-loaded `sample.docx`. The editor cannot author a document from scratch — the most basic word-processor task — so every session begins by borrowing someone else's file. Cross-ref `HF-016`, `HF-073`. | P1 | M | `webapp/editor.html:80`; `webapp/src/main.js:11531, 11873-11878, 2676` | Open |
| UX-012 | **No Table menu on the menu bar, and the palette hides table commands on complex tables.** `APP_MENU_SECTIONS` contains zero `table.*` ids, so browsing the menus tells the user the editor has no table editing. The palette surfaces the 22 table commands only when `plainTableInfo(...)` is truthy — so inside a **merged** table it goes silent too, leaving the contextual ribbon tab and right-click as the only routes. | P2 | M | `webapp/src/main.js:11872-11914, 11791-11806, 6457-6590` | Open |
| UX-013 | **Print has no visible chrome** — File menu, ⌘P and the palette only. Because the menu bar is hidden until a document loads, a new user has no discoverable print affordance at all. | P2 | S | `webapp/src/main.js:11537`; `webapp/editor.html:32` | Open |
| UX-014 | **Menu taxonomy matches neither Word nor Docs:** Format painter under **Edit**; Page setup and Paragraph properties under **Tools**; Settings under **Tools**; header/footer under **Insert**; review mode duplicated in **View** and **Review**. | P2 | S | `webapp/src/main.js:11884, 11888, 11893, 11903, 11914` | Open |
| UX-015 | **Single-surface capabilities:** Pages panel (rail only), compact-ribbon toggle (chevron only — `HF-094`), table style gallery, line/paragraph spacing (in no menu), format painter (no context menu), Settings (not on the View ribbon, contra `64`:128). | P2 | M | `webapp/editor.html:1045-1049`; `webapp/src/main.js:822-862` | Open |
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

## 3. FID — rendering and round-trip fidelity

From the fidelity audit. The engine is **stronger** than `46`/`55`/`60` describe (those are
pinned to `main@cde11ff`, 992 commits behind HEAD) and **weaker** than the public page
claimed before EV-001…EV-004. The staleness corrections are §3.4.

### 3.1 Process — the two highest-leverage rows are not rendering work

| ID | Finding | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- |
| FID-P-01 | **No oracle reference is committed, so the geometry gate is inert.** The workflow is written and ready; one dispatch, a review of the blessed references, and a merge converts a disarmed gate into a live one. This is the cheapest credibility win in the repository and it gates the EV-005 assertion that the gate is armed. | P1 | S | `crates/casual-doc-render/tests/oracle_geometry.rs:186-190`; `.github/workflows/oracle-geometry.yml` | Open |
| FID-P-02 | **There is no Microsoft-Word-produced fixture anywhere in the repository.** All 21 fixtures are generator output, handwritten minimal packages, or LibreOffice conversions. Word is the stated compatibility reference (`12` §Market Groups); LibreOffice is a layout *proxy* chosen in `46`. Until a rights-reviewed Word-produced corpus exists (`23`), no claim about Word-grade fidelity rests on anything a build can reproduce. | P1 | M | `fixtures/manifest.json` | Open |
| FID-P-03 | **Round-trip tests are a fixed point and cannot detect lossy import.** `assert_corpus_round_trip` asserts `reopen(source) == reopen(write(reopen(source)))`. Because the left side is itself the importer's output, **anything dropped on first import is a perfect fixed point and passes**. ~200 `*_survive_the_semantic_round_trip` tests share this blind spot; nothing in the suite compares output against the source XML. Fix: assert that no source element local-name disappears without a corresponding report entry. | P1 | M | `crates/casual-doc-export/src/lib.rs:162-167` | Open |
| FID-P-04 | **"Modeled" is counted as done while nothing consumes it.** Eight constructs are typed, cascaded and round-tripped with zero layout consumers: footnote `NoteProperties` (number format / restart / position), `w:lnNumType`, the `w:kern` size threshold, `w:kinsoku`, embedded `.odttf` faces, cell `noWrap`/`fitText`/`hideMark`/`textDirection`, `w:gutter`/`w:mirrorMargins`, and `evenPage`/`oddPage`. Each reads as finished from the model side and is invisible to a user. Fix: a model-row template field naming the consumer, and a check that a `Done` model row either has one or is explicitly marked preservation-only. | P1 | S | see FID-L-* rows below | Open |

### 3.2 Layout and rendering gaps

| ID | Finding | Family | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- | --- |
| FID-L-01 | Embedded fonts (`.odttf`) are modeled and imported and then **never used** — no de-obfuscation code exists anywhere in layout, render, or wasm. Embedding is common in corporate and branded documents, and the substitution table cannot rescue a face the document carried. De-obfuscation is ~10 lines (`40` §3.3) plus registry wiring: **the cheapest large fidelity win in the repo.** | Fonts | P1 | S | `grep "deobfusc\|odttf\|EmbeddedFace"` over layout+render+wasm → 0; model `properties.rs:472-495`, import `font_table.rs:152-163` | Open |
| FID-L-02 | **No hyphenation at all** — no hyphenator, no dictionary, no consumer for `w:autoHyphenation`/`w:hyphenationZone`; only `w:suppressAutoHyphens` is cascaded, with nothing to suppress. Automatic hyphenation is on by default in many European templates, and it changes line breaking and therefore pagination. | Typography | P1 | M | `crates/casual-doc-layout/src/cascade.rs:560-561` | Open |
| FID-L-03 | **`evenPage`/`oddPage` section breaks insert no parity blank page** — `SectionType` is read at exactly two sites, both testing `Continuous \| NextColumn`. Every book and report chapter break hits this. | Pagination | P1 | S | `crates/casual-doc-layout/src/columns.rs:238`; `flow.rs:6402` | Open |
| FID-L-04 | **~180 DrawingML preset shapes collapse to bounding rectangles**, and `a:custGeom` is architecturally unpaintable: `ShapeGeometry` has 7 presets + `Other`, and the display list has **no path/Bézier primitive** (`Rect`, `Ellipse`, `RoundedRect`, `Polygon`, `Line` only). Any document with real diagrams hits this. Fix order: add `PaintItem::Path` + `a:path` command evaluation, then table-drive the presets. | Shapes | P1 | M | model `body.rs:750-767`; `crates/casual-doc-layout/src/display.rs:114-138, 177-276` | Open |
| FID-L-05 | **Footnote number format, restart, and position are entirely unconsumed** — `NoteProperties` is fully modeled and `notes.rs` has no reader, so notes are always decimal, always continuous, always page-bottom. Authored `w:separator`/`w:continuationSeparator` are ignored in favour of a synthesized rule. Legal and academic documents depend on these. | Notes | P1 | S | model `definitions.rs:658-672`; `crates/casual-doc-layout/src/notes.rs` | Open |
| FID-L-06 | **The shaper's paragraph base level cannot be forced,** so an RTL paragraph whose text carries no strong RTL character reorders LTR; the resolved level is also collapsed to a single RTL flag, discarding embedding depth needed for nested-bidi caret placement. The `Start→Right` alignment remap is a correct partial workaround, and the code says so itself. Needs an upstream API or pre-reordering before the shaper. | Bidi | P1 | L | `crates/casual-doc-layout/src/shape.rs:480-505, 1125` | Open |
| FID-L-07 | **Floating tables (`w:tblpPr`) render inline** — zero hits in the layout crate. Newsletters, forms, and legal documents use them. | Tables | P1 | M | `grep "tblpPr\|float_position"` over `crates/casual-doc-layout/src` → 0 | Open |
| FID-L-08 | **Vertical and rotated text is entirely absent** (`w:textDirection` tbRl/btLr, `bodyPr vert`) — `text_direction` appears only as `None` in test scaffolding. Needs a second writing-mode axis through flow, composition, hit-test, and caret. | CJK/tables | P1 | L | `crates/casual-doc-layout/src/{notes.rs:511, document_layout.rs:1198, paginate.rs:1612}` | Open |
| FID-L-09 | **Line numbering is never generated** — `w:lnNumType` is modeled and its only three layout references are `Default::default()` in fixtures. A post-pagination margin pass mirroring `page_border.rs` closes it. Legal pleadings require it. | Page furniture | P2 | S | as above | Open |
| FID-L-10 | **Watermarks do not appear.** No watermark concept exists anywhere, and a Word watermark is a header shape carrying warped text (`v:textpath` / `a:prstTxWarp`) — neither text-path form is typed, so the text vanishes and only the shape box can paint. | Shapes | P2 | M | `grep -ri watermark crates/` → 0 | Open |
| FID-L-11 | **`nextColumn` is treated as `continuous`**, and the final page of a multi-column section is not balanced — the residual +2 on the Chinese SDS. Unequal columns share one galley flowed at the widest column. | Sections | P2 | M | `crates/casual-doc-layout/src/columns.rs:238`; `60` §6 | Open |
| FID-L-12 | **Tight/through wrap uses the square bounding box, not `wp:wrapPolygon`** contours; the page-coupled reflow fixed point is capped at 3 passes and falls back to a conservative envelope. Deterministic, but a deterministic approximation. | Floats | P2 | M | `crates/casual-doc-layout/src/document_layout.rs:678-698` | Open |
| FID-L-13 | **Cell `noWrap`, `fitText`, `hideMark`, and cell `textDirection` are unconsumed** — zero hits in the layout crate. Forms and dense tables depend on `noWrap` in particular. | Tables | P2 | M | as FID-L-07 | Open |
| FID-L-14 | **Emphasis marks, outline, shadow, emboss, imprint, and run borders are modeled and cascaded but unpainted.** Emphasis marks are near-universal in Japanese text; they and run borders are small additions to the decoration pass. | Run format | P2 | S | `crates/casual-doc-layout/src/cascade.rs:469` | Open |
| FID-L-15 | **No OpenType feature control, and the `w:kern` threshold is unapplied.** Zero feature-tag or variation-axis sites in the layout crate, so `w:ligatures`, stylistic sets, `locl` forms and old-style figures are unreachable; and because the modeled kerning threshold is never read, the engine kerns text Word would leave unkerned. | Typography | P2 | S | no `font_features` sites in `crates/casual-doc-layout`; `cascade.rs:472` vs sole consumer `casual-doc-export/src/semantic.rs:6557` | Open |
| FID-L-16 | **`w:gutter` and `w:mirrorMargins` never reach the page configuration** — `PageConfig` has no gutter field and every `gutter` reference is test scaffolding. Bound documents print with the wrong margins. | Sections | P2 | S | `crates/casual-doc-layout/src/paginate.rs:55-136` | Open |
| FID-L-17 | **`w:kinsoku` is cascaded and never consumed**, and line breaking is UAX #14 via ICU with no Word-compatible kinsoku table, so CJK line breaks diverge from Word's. | CJK | P2 | M | `crates/casual-doc-layout/src/cascade.rs:548` | Open |
| FID-L-18 | **`w:jc="distribute"` silently collapses to ordinary justification**, and there is no kashida for Arabic or inter-character distribution for CJK. The import maps it to `Justify` and the call site treats that as fully mapped, so no finding is raised (see FID-R-04). | Justification | P2 | S | `crates/casual-doc-import/src/properties.rs:518`; export `semantic.rs:6722` | Open |
| FID-L-19 | **Character-grid snapping is not applied.** `w:docGrid` line pitch reaches layout correctly (with exact/paragraph/table precedence and `w:snapToGrid` gating), but `w:charSpace` and `linesAndChars` character snapping do not. | CJK | P3 | M | `crates/casual-doc-layout/src/flow.rs:233-236, 6362-6364` | Open |
| FID-L-20 | **EMF/WMF metafiles and browser-build SVG paint a placeholder.** SVG rasterizes on native only (`resvg` is a `cfg(not(wasm32))` dependency) and `usvg/text` is disabled, so SVG `<text>` never renders on any build. Needs a metafile interpreter or a host rasterization seam. | Images | P3 | L | `crates/casual-doc-render/Cargo.toml`; `lib.rs:404, 459-474` | Open |

### 3.3 Round-trip and disposition-reporting gaps

| ID | Finding | Pri | Eff | Evidence | Status |
| --- | --- | --- | --- | --- | --- |
| FID-R-01 | **The DOCX exporter has no compatibility-reporting path at all** — it returns `CompatibilityReport::default()` unconditionally. The ODF exporter reports, and even carries a residual check. So export-side loss on the primary format is structurally unreportable. | P1 | M | `crates/casual-doc-io/src/docx.rs:160-164`; contrast `casual-doc-odf/src/export.rs:4603-4606` | Open |
| FID-R-02 | **`ModelOutcome` is hardcoded `Omitted` at all three construction sites** — `Degraded` and `Mapped` are never constructed on the DOCX path — and `RetentionOutcome` is a per-*mode* constant, not a per-construct fact (Retention stamps everything `Preserved`, Semantic stamps everything `NotRetained`); `Blocked`, `Rejected` and `NotApplicable` are never constructed anywhere. `35`'s entire purpose — distinguishing "degraded, remainder preserved" from "degraded, remainder lost" — is currently inexpressible. There is also no preservation ledger, no ledger-ID field, and no validator of the 9 legal combinations `35` mandates. | P1 | M | `crates/casual-doc-import/src/report.rs:129, 138, 164`; `lib.rs:1044-1047` | Open |
| FID-R-03 | **Unknown *attributes* are outside the report vocabulary entirely.** `Reporter::report` takes an element local-name and `CompatibilityEntry` has no attribute field, so `w:rsidR`/`rsidRPr`/`rsidDel`/`rsidTr` — present on nearly every `w:p`, `w:r` and `w:tr` — plus `mc:Ignorable` and `w14:paraId` cannot be described even in principle. | P1 | M | `crates/casual-doc-import/src/report.rs` | Open |
| FID-R-04 | **Three parsers have zero reporting and their parts are regenerated, so loss is permanent and invisible:** `theme.rs`, `font_table.rs`, `comments_ext.rs` have no `.report(` sites. Concretely `a:objectDefaults`, `a:extraClrSchemeLst`, `a:extLst` and `a:theme/@name` are silently dropped on every semantic save. Every document has a theme and a font table. Also unreported: unknown children of `w:styles` (`styles.rs:426` skips the subtree silently, in a file that reports densely elsewhere), `w:numPicBullet` (falls to a bare catch-all), and `w:background` (imported but never emitted, with no finding). | P1 | S | `crates/casual-doc-import/src/{theme.rs, font_table.rs, comments_ext.rs}` → 0 report sites; `numbering.rs:520-523`; `lib.rs:1218-1222` | Open |
| FID-R-05 | **Retained opaque parts are never invalidated on edit** — `casual-doc-edit` and `casual-doc-transaction` have no knowledge of the side table. A stale thumbnail, a stale `stylesWithEffects.xml` contradicting a regenerated `styles.xml`, and stale data-bound `customXml` all survive into the saved package. | P2 | M | `crates/casual-doc-{edit,transaction}` vs `casual-doc-import/src/opaque.rs:50-63` | Open |
| FID-R-06 | **Missing media on export writes a zero-byte part with a valid relationship** — Word then shows a broken-image box rather than reporting a problem. Same pattern for `.odttf` parts. | P2 | S | `crates/casual-doc-export/src/semantic.rs:704-707, 716` | Open |
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

**Status: pending.** Two source-grounded audits of the ONLYOFFICE client
(`ONLYOFFICE/web-apps`, ~76.6k JS LOC in the document editor alone, plus `sdkjs`) are in
flight: a complete ribbon/dialog/panel feature inventory, and a UI/UX-plus-architecture
study. This section will carry:

1. the tab-by-tab capability comparison (ONLYOFFICE ships File, Home, Insert, Draw,
   Layout, References, Forms, Collaboration, Protection, View, Plugins and AI tabs against
   this editor's five — Home, Insert, Table, View, Review);
2. ranked OO-xxx rows for each gap worth closing, with the ONLYOFFICE source file that
   shows the pattern;
3. explicit **won't-follow** rows, because several ONLYOFFICE capabilities are the
   consequence of an architecture this project has deliberately rejected — a mandatory
   document server, and a DOCX → internal-binary → DOCX conversion round-trip. Feature
   count is not the objective (`12` §Product Position); the objective is what a local-first
   embeddable engine should reach.

Known-absent-here headlines to be rowed up when the inventory lands: no table of
contents generation, no mail merge, no compare/combine documents, no version history, no
spelling or grammar check, no word count dialog, no accessibility checker, no captions or
cross-references, no bibliography, no watermark UI, no hyphenation UI, no line-numbering
UI, no drop-cap UI, no equation editor, no chart or SmartArt authoring, no text art, no
freehand drawing, no content-control authoring, no forms, no document protection or
signatures, no plugins or macros, no real-time collaboration, and no PDF export.

---

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
