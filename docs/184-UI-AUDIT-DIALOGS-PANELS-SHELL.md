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
`.dialog-card-wide`), plus one exception: Keyboard shortcuts, a reference card read whole,
keeps the 60rem width it had (`.dialog-card-reference`), since at L its four columns stacked
230px past a 1280×720 laptop. Max-height `calc(100dvh - 2rem)`; only the body scrolls and the
footer is pinned. Radius 0.75rem. Header padding 1.5rem 1.5rem 0.5rem, title 0.9375rem/600
(owner, 2026-11-02: the first 1.25rem title was too large), an optional
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

*Status (2026-11-06, lane L8, `109` UX-066):* built. Until now `modal.mjs` stacked a second
modal over the first and the stacking spec did not exist. It now hides the dialog underneath
(keeping its state: no `onClose`, so a half-filled form survives) while the second is open,
and shows it again, with focus on the control that opened the second, when the second
closes. One mechanism for every registered dialog: Settings ▸ AutoCorrect options…, an
add-in's permission prompt from Settings, a confirmation from any dialog.

**D2 — Page setup rebuilt** (Google Docs + Word/ONLYOFFICE). One column, M width. An "Apply
to" select (the scopes the engine supports) replaces every per-property Apply button.
Orientation is a segmented Portrait/Landscape; Paper size and Page colour selects in a row;
Margins as preset chips Normal / Narrow / Moderate / Wide / Custom with Top/Bottom/Left/Right in
one row of four; an "Advanced" disclosure for gutter, mirror margins and columns. Footer:
Cancel, Apply ("Set as default" only if the engine has a default; it has not, so it is
omitted). The Layout ribbon tab's Margins ▾, Size ▾, Orientation ▾ and Columns ▾ are preset
menus, each ending with "Custom…", which opens this dialog.

*Amended 2026-11-06 (lane L8, approved mockup `v2-dialog`, `109` UX-066):* the card is L, not
M: the fields on the left and the page preview on the right. Paper size is a select of named
sizes ("A4 · 210 × 297 mm") plus "Custom size", which alone shows Width and Height; a size is
recognised within 1 mm, so an A4 page no longer reads "Custom" (audit DIALOGS-10). Page colour
is a swatch control shaped like a select. "Apply to" sits on the footer's left. It lists the
scopes the engine writes in one undo step, which is one section: a one-section document reads
"Whole document"; with several, the caret's section reads "This section (n)" and the others
"Section n". A true whole-document Apply over several sections waits for an engine call that
writes them as one undo step. The Layout tab's preset menus (Margins ▾, Size ▾ …) are lane L4's
ribbon work and are not built here.

**D3 — Settings with a left navigation** (Word Options + Docs). L width, a 12.5rem
`tablist` (arrow keys move) with General / Proofing / AutoCorrect / Saving / AI and add-ins /
About. One option per row, min-height 3rem, label left and control right, at most one short
help line; explanations behind "Learn more". The AI provider and Manage add-ins are rows of the
"AI and add-ins" page. Changes apply as they are made. Footer: "Reset to defaults" (utility,
left) and Done.

*Amended 2026-11-06 (lane L8, approved mockups `v2-settings-light/dark`, `109` UX-066):* built.
The card takes the XL rung (60rem, `.dialog-card-reference`), as the mockup draws it (880px):
at L every row's help line wrapped beside its 13.75rem control. The frame has ONE height,
`min(34rem, 100dvh - 2rem)`, on every page; only the page scrolls (audit DIALOGS-09, held by
`dialog-anatomy.spec.mjs`). "Settings" heads the navigation; each page has its own title and
the close button. Switches (`role="switch"` checkboxes) for on/off rows. The footer is Reset to
defaults, "Changes apply as you make them" and Done. The add-in list is rows with switches on
the AI and add-ins page, with its own install row (DIALOGS-08); Manage add-ins remains a dialog
for Review ▸ Add-ins and the palette. The AI provider keeps one press-to-save button, now
secondary ("Save provider"), because its fields only make sense together; Done is the only
primary. AutoCorrect's switches are a page; its replacement lists stay in the AutoCorrect
options dialog, which takes Settings' place (D1a) rather than moving in. About shows the engine
version, licence and links.

**D4 — Docked side panels** (Google Docs). Every side panel is docked to the window edge at full
workspace height: no floating card, no radius, no shadow, a hairline border. Default width
20rem, resizable 17.5–30rem and remembered. LEFT is navigation (Outline, Pages, Find results);
RIGHT is context and collaboration (Properties, Comments and review, Version history, Compare,
the AI/plugin panel, Symbol/Emoji). One panel per side. The page re-centres in the remaining
space and is never covered. Header 3.5rem with a 1.0625rem/600 title, actions and close; body
padding 1rem 1.25rem; rows at least 2.25rem; a panel opens scrolled to the top and has an empty
state; the rail button shows the active state; close or Esc returns focus to the invoker. At
≤390px a panel is a bottom sheet.

*Amended 2026-11-06 (UI redesign programme, lane L5, `109` UX-055; the
integrator's default where the owner did not answer, stated in the programme
brief): every docked panel is drawn as in the approved v2 mockups — a ROUNDED CARD
ON THE DESK (radius `--r-md`, a hairline ring and `--e1`, 0.375rem from the
edges), docked to its side at full workspace height, the page re-centred beside
it and never covered. This replaces "no floating card, no radius, no shadow" above;
the rule that matters — docked, one per side, the page never covered — is
unchanged. One per side is kept by a dock manager (`dock_manager.mjs`) that
watches every panel's `hidden`, so no opener can bypass it; Compare is on the
right; the comment column is a right-slot panel that a right panel displaces and
gives back; Find is a compact bar at the top right, beside a docked right panel.
The add-in panel keeps its edge-docked frame (`docs/183` §21). Built: one width
per side with the splitter and memory, Escape from anywhere inside, focus to the
invoker or the editor, the phone bottom sheet with a grab handle. Not built:
zoom-to-fit when both sides leave less than a page, sheet detents, a find
results list in the left slot. Tokens and anatomy: `docs/63` §7.4.*

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

*Status (2026-11-03, `program/ui-redesign` lane L2, `109` UX-054):* built except "Compact/Ribbon
is one control", which goes with the header restructure (lane L4), and opening a contextual tab
automatically on insert. Share is hidden without a host that can share; the header selector is the
only mode switch, at every width including the phone; contextual tabs are hidden until their
object is selected and drawn in the accent. `e2e/shell-sparse.spec.mjs` exists and holds it.

*Status (2026-11-06, lane L5, `109` UX-065):* Suggesting has no banner any more: the header chip
is green and a 2px green rule runs along the top of the canvas, so the page moves 0px on entering
it (`e2e/review-layout.spec.mjs`). Viewing still swaps the ribbon for the menu bar (audit
REVIEW-02); keeping the chosen chrome with commands disabled in place is not built.

**D7 — Accessibility floor, enforced by tests.** Every interactive target ≥1.5rem (24px) in
both dimensions, ≥2.75rem on a coarse pointer; chrome text ≥0.75rem; a 2px accent focus ring
with an offset; every control has an accessible name; AA contrast in both themes.

**D8 — rem everywhere in chrome.** px only for 1–2px hairlines, borders, shadows, the document
page region (laid out in points; `tests/css_regions.mjs` separates it) and positions computed
from the pointer in JavaScript. `chrome_units.test.mjs` holds the stylesheets;
`chrome_js_units.test.mjs` refuses a px type or spacing length, or any px literal, written
from script.

**D12 — The canvas: one floating surface, short menus, zoom keeps its place** (the
2026-11-03 redesign audit's decision D-12, owner-approved with the v2 mockups; Word's mini
toolbar, Docs' link card, Word's 14-entry table menu). Of the link card, the object bar, the
table insert markers, the selection mini-toolbar and the paste chip, at most one shows, in
that priority, and none covers the selection (`floating_arbiter.mjs`: the owners keep showing
and placing their own element, and one observer settles after every change, so no path can
forget to ask; only a `position: fixed` surface is moved off the selection, because the table
insert disc is page-relative and is its own hit box, so it is only ever suppressed). A click on an external link places the caret and shows the card (Ctrl/Cmd+click
follows it; the pointer is an I-beam until the chord is held, and the sheet's tooltip names the
target and the chord); an internal link, a contents entry, still jumps on a plain click. The
mini-toolbar appears only for a pointer-made selection and fades as the pointer leaves it,
gone once fully faded; a keyboard selection, Tab into a cell and a link click raise none. ⌘/
(Casual AI on the selection) earns it for the selection it acts on, and its menu hangs off
the selection if the bar cannot show. The hand over a link follows the Ctrl/Cmd key itself. Context menus follow the
approved right-click anatomy (`docs/63` §3) and one policy for every builder: no Undo/Redo,
no refused top-level row (several rows hidden for one reason leave that reason as a note), at
most 16 rows and 480px with the trailing groups folding into "More"; a table cell offers
Insert ▸, Delete ▸, Merge or Unmerge when it applies, Split cell, Select ▸, one Table ▸ and
Table properties…; a picture one Arrange ▸ and one Crop ▸. The spelling menu leads with at
most three bold suggestions ranked by a weighted edit distance (doubled letters, swaps,
keyboard neighbours and vowels cost less) plus a word-frequency term. The paste chip is a
collapsed icon at the end of the paste, opened by a click or by Ctrl alone, offering Keep
source formatting, Match destination formatting, Keep text only and Picture. A zoom keeps the
point the reader is on (the pointer for Ctrl+wheel and pinch, the caret when it is in view,
else the middle of the view), and a page then wider than the window keeps at most a 24px side
gutter; PageDown steps one screen less a line and keeps the caret's
place on screen; "Page X of Y" names the page most in view while scrolling, and the caret's
page while the caret is on screen.

*Status (2026-11-06, `program/ui-redesign` lane L7, `109` UX-070, UX-071, UX-072):* built as
above. Not built yet, and queued in UX-072: the object bar's five grouped actions and
16px handle hit areas (CANVAS-15), hover-only table edge strips and an 8px resize zone
(CANVAS-11), one tint for cell selections (CANVAS-12), the drag ghost and snapped drop
(CANVAS-14), the CJK fallback and per-run proofing language (CANVAS-17) and the select-line
margin (CANVAS-18).

## 4. Guards

| Decision | Guard |
| --- | --- |
| D1 | `tests/dialog_density.test.mjs` (a card's width is a rung, chosen by class; bands read the tokens); `tests/e2e/dialog-anatomy.spec.mjs` (every dialog at 1280×720 and 390×844 in both themes: S/M/L or full-width sheet, 15px title, pinned footer inside card and window, no wrapped button label, no sideways scroll, equal control heights, a one-line description at 1280, no text in the footer but actions, a labelled footer field, a live status or the declared Settings note) |
| D2 | `tests/page_presets.test.mjs` (an A4 page read back through two-decimal inches is A4) |
| D3 | `tests/e2e/dialog-anatomy.spec.mjs` (the Settings frame is the same height on all six pages) |
| D-13 | `tests/e2e/link-popover.spec.mjs`, `tests/link_popover_model.test.mjs`; `tests/command_ellipsis.test.mjs` (a command that opens a dialog ends in “…”, and nothing else does) |
| D1a | `e2e/dialog-stacking.spec.mjs`, and `modal.mjs` hiding the dialog underneath while a second one is open |
| D4–D5 | `e2e/panel-dock.spec.mjs` (one per side for every panel opened in turn; page gaps equal ±2px at 1280 and 1440; Escape focus); `e2e/review-layout.spec.mjs` (the comment column's bar and cards) |
| D6 | `e2e/shell-sparse.spec.mjs` |
| D7 | `e2e/a11y-floor.spec.mjs`; `theme-contrast.spec.mjs` |
| D8 | `tests/chrome_units.test.mjs`, `tests/chrome_js_units.test.mjs`, `tests/design_scale.test.mjs` |
| D12 | `e2e/overlay-arbiter.spec.mjs` (at most one floating surface, none over the selection, in every state; link click; pointer-only mini-toolbar; Tab into a cell; the paste chip; the disc never moved; the fade with no style write); `e2e/context-menu.spec.mjs` (≤16 rows, ≤480px, no Undo/Redo, no disabled top-level row, on text, table and picture); `e2e/zoom-anchor.spec.mjs` (the caret stays in view at 300%; Ctrl+wheel keeps the pointer's point within 4px; the 24px side gutter at 161%; PageDown and Page X of Y on a multi-page document; Ctrl+Home/End); `e2e/ai-assistant.spec.mjs` (⌘/ menu beside the selection); `e2e/pointer-cursor.spec.mjs` (the hand on the key); `tests/context_menu.test.mjs`, `tests/floating_arbiter.test.mjs`, `tests/zoom_anchor.test.mjs`, `tests/page_in_view.test.mjs`, `tests/spelling.test.mjs` |

Implementation status per decision is kept in the `docs/109` rows, not here.
