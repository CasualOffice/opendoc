# 184 — UI audit: dialogs, side panels, header and status bar

**Status:** Approved by the owner 2026-11-01 (decisions D1–D8 below). Built on
`program/ui-v2-audit`. **Rows:** `docs/109` UX-053 (dialogs, D1–D3), UX-054 (header and
status bar, D6), UX-055 (side panels, D4–D5), UX-056 (accessibility floor and rem, D7–D8).
**Builds on:** `docs/63` §7 (design system v2, UX-052), which this audit extends from tokens
to the anatomy of every surface.

## 1. What was measured

Measured in the running editor (Chromium, 1280×800 and 390×844, both themes) before any
change:

| Surface | Measured |
| --- | --- |
| Dialogs | 51 dialogs in **16 widths**; **8 header heights** (22–116px); **11 section gaps**; titles 13px/600; body padding 11.2px 22px (a `clamp()` of the viewport inside a root that already follows the viewport); buttons 34px beside 36px inputs; **7 footer finish patterns** |
| Page setup | 32 controls in 4 columns at 756×720; "Apply mirrored margins to document" wrapping inside its own button; one Apply button per property |
| Settings | two columns of 9 explanatory paragraphs; 3 unrelated buttons in the footer (AI provider, Manage add-ins, Reset) and no Done; opening AI provider or Add-ins stacked a second modal over it |
| Side panels | 10 panels in **5 widths** (252/280/320/360/400); docked left (Outline, Pages, Compare) or right (the rest) with no rule; drawn as floating rounded cards that **cover the page** |
| Panel behaviour | Paragraph properties opens scrolled mid-way; Outline has no empty state |
| Targets and text | **22 of 83** visible controls under 24×24px (split-button arrows 22×30, status-bar Editing/Suggesting toggles 23px tall, ruler tab marker 17×17); two 9px chrome texts; two unlabelled custom-colour inputs |
| Units | 109 non-hairline px declarations in `style.css`; 165 px styles written from JavaScript |

## 2. What each reference does, and what we adopt

| Reference | Pattern | Adopted as |
| --- | --- | --- |
| Google Docs | Dialogs in a few fixed widths, 20px title, actions bottom-right with the specific verb; settings as a navigated page; side panels docked to the window edge, the page re-centred, never covered; Share hidden when there is nothing to share | D1, D3, D4, D6 |
| Word (desktop and web) | Page Setup as one form with an "Apply to" scope; File ▸ Options with a left navigation; preset galleries (Margins ▾, Size ▾, Orientation ▾, Columns ▾) each ending in "Custom…"; the editing-mode switch once, in the header | D2, D3, D6 |
| ONLYOFFICE | One right-hand settings panel whose tab follows the selection (paragraph, table, image, shape) — `RightMenu` | D5 |
| WCAG 2.2 | 2.5.8 target size 24×24 CSS px; 1.4.3/1.4.11 contrast; 4.1.2 name, role, value | D7 |

Competitor screenshots were looked at and are deliberately not committed.

## 3. Decisions (approved by the owner 2026-11-01)

Chrome lengths are rem on the fluid 16px root (`docs/63` §7.1).

**D1 — Dialog anatomy, for every `.dialog-overlay`/`.dialog-card`.** Widths only S 25rem /
M 35rem / L 45rem, chosen by class (`.dialog-card-compact`, `.dialog-card`,
`.dialog-card-wide`). Max-height `calc(100dvh - 4rem)`; only the body scrolls and the footer
is pinned. Radius 0.75rem. Header padding 1.5rem 1.5rem 0.5rem, title 1.25rem/600, an optional
one-line description at 0.8125rem. Body padding 0.75rem 1.5rem 1.5rem; 1.5rem between groups,
0.75rem within a group, 0.375rem from label to field; labels above fields. One control height,
2.25rem (`--h-input`), for inputs, selects, buttons and segmented controls, 2.75rem on a coarse
pointer. Footer padding 1rem 1.5rem over a hairline: utility actions (Reset, Delete all, Set as
default, Forget, Test connection) on the left in text style (`.dialog-button-utility`), then
Cancel, then the primary — the specific verb (Insert, Apply, Save, Convert) — on the right;
read-only dialogs have one Close. At ≤390px a dialog is a full-width bottom sheet. Help text is
at most one line under a title; longer explanations go behind "Learn more". The off-scale
`--h-field` 36 / `--h-search` 34 / `--h-control` 30 leave dialogs; the first two survive only
as plugin-contract aliases of `--h-input`.

**D1a — Never a modal over a modal** (owner correction, 2026-11-01: "popup over a popup —
who designed it"). At most one `.dialog-overlay` is visible at any time. A second step is
either a page inside the first dialog (Settings ▸ AI and add-ins holds the provider form and
the add-in list inline), or the first closes and the second opens in its place, or a
confirmation becomes inline inside the same dialog. Guarded in `modal.mjs` and by
`e2e/dialog-stacking.spec.mjs`.

**D2 — Page setup rebuilt** (Google Docs + Word/ONLYOFFICE). One column, M width. An "Apply
to" select (the scopes the engine supports) replaces every per-property Apply button.
Orientation is a segmented Portrait/Landscape; Paper size and Page colour selects in a row;
Margins as preset chips Normal / Narrow / Moderate / Wide / Custom with Top/Bottom/Left/Right in
one row of four; an "Advanced" disclosure for gutter, mirror margins and columns. Footer:
Cancel, Apply ("Set as default" only if the engine has a default; it has not, so it is
omitted). The Layout ribbon tab's Margins ▾, Size ▾, Orientation ▾ and Columns ▾ are preset
menus, each ending with "Custom…", which opens this dialog.

**D3 — Settings with a left navigation** (Word Options + Docs). L width, a 12.5rem
`tablist` (arrow keys move) with General / Proofing / AutoCorrect / Saving / AI and add-ins /
About. One option per row, min-height 3rem, label left and control right, at most one short
help line; explanations behind "Learn more". The AI provider and Manage add-ins are rows of the
"AI and add-ins" page. Changes apply as they are made. Footer: "Reset to defaults" (utility,
left) and Done.

**D4 — Docked side panels** (Google Docs). Every side panel is docked to the window edge at full
workspace height: no floating card, no radius, no shadow, a hairline border. Default width
20rem, resizable 17.5–30rem and remembered. LEFT is navigation (Outline, Pages, Find results);
RIGHT is context and collaboration (Properties, Comments and review, Version history, Compare,
the AI/plugin panel, Symbol/Emoji). One panel per side. The page re-centres in the remaining
space and is never covered. Header 3.5rem with a 1.0625rem/600 title, actions and close; body
padding 1rem 1.25rem; rows at least 2.25rem; a panel opens scrolled to the top and has an empty
state; the rail button shows the active state; close or Esc returns focus to the invoker. At
≤390px a panel is a bottom sheet.

**D5 — One Properties panel** (ONLYOFFICE `RightMenu`). Paragraph, Table and Picture/Shape
properties are one right panel titled "Properties" whose tab follows the selection; only the
relevant tabs are shown. Every capability stays reachable from at least two surfaces, and the
existing command ids and host contract keep working.

**D6 — A sparse header and status bar** (Google Docs + Word). Share is hidden, not disabled,
when sharing is not configured. The editing mode (Editing / Suggesting / Read only) is only in
the header chip; the status bar's duplicate is removed. Compact/Ribbon is one control.
Contextual tabs (Table, Chart, Header & Footer, Picture…) appear only while their object is
selected, styled as contextual, never as greyed placeholders. The status bar is page x of y,
word count, language and zoom, plus the contextual chips that already exist.

**D7 — Accessibility floor, enforced by tests.** Every interactive target ≥1.5rem (24px) in
both dimensions, ≥2.75rem on a coarse pointer; chrome text ≥0.75rem; a 2px accent focus ring
with an offset; every control has an accessible name; AA contrast in both themes.

**D8 — rem everywhere in chrome.** px only for 1–2px hairlines, borders, shadows, the document
page region (laid out in points; `tests/css_regions.mjs` separates it) and positions computed
from the pointer in JavaScript. `chrome_units.test.mjs` holds the stylesheets;
`chrome_js_units.test.mjs` refuses a px type or spacing length, or any px literal, written
from script.

## 4. Guards

| Decision | Guard |
| --- | --- |
| D1 | `tests/dialog_density.test.mjs` (a card's width is a rung, chosen by class; bands read the tokens); `tests/e2e/dialog-anatomy.spec.mjs` (every dialog at 1280×720 and 390×844 in both themes: S/M/L or full-width sheet, 20px title, pinned footer inside card and window, no wrapped button label, no sideways scroll, equal control heights) |
| D1a | `e2e/dialog-stacking.spec.mjs`, and `modal.mjs` refusing to open a second modal |
| D4–D5 | `e2e/panel-dock.spec.mjs` |
| D6 | `e2e/shell-sparse.spec.mjs` |
| D7 | `e2e/a11y-floor.spec.mjs`; `theme-contrast.spec.mjs` |
| D8 | `tests/chrome_units.test.mjs`, `tests/chrome_js_units.test.mjs`, `tests/design_scale.test.mjs` |

Implementation status per decision is kept in the `docs/109` rows, not here.
