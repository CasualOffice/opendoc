# 63 — Editor UI/UX Design System & Shell

Status: accepted direction (living document)
Reference: the newer `Vellum.dc.html` from the owner's `Vellum Design Book`
bundle (recorded precisely in doc 64). The checked-in
`docs/assets/template-reference-prototype.html` is an older revision and remains
historical reference material, not the exact toolbar source.

This note is the **single reference for the editor's UI/UX**. Every feature we add
to `webapp/` is placed and styled per the shell and tokens below, so the editor
grows into one coherent product instead of a pile of toolbars.

## 0. Governing principle — functional only

**We never ship a non-functional button, icon, region, or layout.** No placeholder
rails, no empty "coming soon" panels, no dead controls. A region appears in the
shell only once it has real behaviour. The target layout in §2 is the *map*; the
*territory* is built incrementally (§6), each slice fully working. This matches the
project's production-grade bar (not an MVP).

## 1. Design language

Borrowed from the reference, adapted to the CasualOffice brand:

- **Accent** — a single settable accent (Settings ⚙). Brand default is orange
  (`--accent`, casualoffice.org "Studio"); the reference's blue is one of the
  presets. Everything tints from `--accent` via `color-mix`.
- **Type** — self-hosted Inter for deterministic cross-platform chrome, with the
  native system stack as fallback. Chrome text is 13px; headings and titles are
  slightly heavier. Tabular numerals remain in use for measures (ruler, zoom,
  counts).
- **Icons** — self-hosted Material Symbols Outlined at the v2 16/20/24px grid
  (§7). Bold/italic/underline/strike remain literal typographic
  glyphs, matching the reference. Font and license files are checked in; editor
  chrome does not depend on a runtime font request.
- **Neutrals** — near-white surfaces on a light-grey canvas; hairline dividers
  (`--line`); soft, low-contrast shadows for lifted surfaces (menus, panels).
- **Shape & space** — superseded by the v2 scale in §7 (2026-11-01): a 4px
  spacing grid, 24/28/32/44px control heights and 4/6/8/12/pill radii.
  `style.css` is the source of truth for token *values*; this document owns their
  *roles*. Change them in the same PR or this drift recurs.
- **Light & dark** — both first-class (`:root[data-theme]` + `prefers-color-scheme`).
- **Motion** — quick (120ms) fades/slides for popovers and panels; nothing bouncy.

## 2. Shell regions (target layout)

The reference calls out six regions; our shell is a CSS grid of:

```
┌───────────────────────── top bar ─────────────────────────┐
│ brand · doc title + local state · File/Edit/…  settings · save │
├──────────────────────── toolbar/ribbon ───────────────────┤   (1)
│  grouped formatting controls (evolves toward tabs)         │
├──────┬───────────────── document viewport ─────┬──────────┤
│ nav  │  ⌜ h-ruler ⌝                             │  context │
│ rail │  ┌───── page(s) + engine overlay ─────┐ │  panel   │   (3)(4)
│ (3)  │  │  v-ruler │      caret/selection     │ │  (right) │
│      │  └──────────────────────────────────────┘ │          │
├──────┴──────────────────── status bar ─────────┴──────────┤   (5)
│ page X/Y · words · language · mode · view · zoom · sync    │
└────────────────────────────────────────────────────────────┘
Overlays (float above): command palette ⌘⇧P (6), floating selection
toolbar, context menus, toolbar popovers.
```

| # | Region | Purpose | Contents (functional, added over time) |
|---|--------|---------|----------------------------------------|
| 1 | **Top bar + toolbar** | identity + primary actions + formatting | brand; two-row document block with title, honest local `Opened`/`Edited`/`Downloaded` state, and functional File/Edit/View/Insert/Format/Tools/Help menus; Open/Save/Properties/Settings; tabbed formatting ribbon. |
| 3 | **Left nav rail + panel** | document navigation | **Outline** (heading tree → scroll-to) first; later Pages (thumbnails), Search/Find. The rail only shows an icon once its panel is functional. |
| 4 | **Right context panel** | inspect/act on the selection | later: Styles (apply), Properties (paragraph/table inspector), Comments (needs a comments model — a later phase). Collapsible; never empty. |
| 5 | **Status bar** | document state | page X of Y, word/paragraph counts, zoom (−/＋/%), later language, edit mode, view toggles, save/sync state. |
| 6 | **Command palette (⌘⇧P)** | fast command access | fuzzy search over every action (insert table, apply style, export, …). `⌘K` remains the document-standard Add/Edit Link shortcut. |
| — | **Floating selection toolbar** | quick format at the selection | mini B/I/U · highlight · color · link · comment above a range; complements our engine-drawn selection. |
| — | **Rulers** | measure + indent/tabs | horizontal (done: margins, indent markers, tab stops); vertical (top/bottom margins) later. |

## 3. Component patterns

Reuse these; don't invent per-feature variants.

- **Toolbar control** — 30px icon button (`.fmt`) or labelled `.ctl` select;
  active state via `aria-pressed`. Groups separated by hairline dividers.
- **Popover** (`.context-menu` + the shared popover manager in `main.js`) — anchored
  under its trigger, one open at a time, dismiss on outside-pointerdown / Escape;
  used for spacing, paragraph options, table & cell, insert-table.
- **Dialog** (`.dialog-overlay` + `.dialog-card`) — centered, modal, titled, and
  focus-contained for multi-section document actions such as Document Properties
  and Page Setup. Dialogs use a dimmed backdrop, explicit Close/Cancel/Apply
  actions, restore focus to their trigger, and keep the action row reachable while
  the body scrolls on narrow screens. Do not force form-heavy workflows into
  toolbar-sized popovers. Use one of three widths: compact 380px (a short decision
  such as Split Cell), standard 640px (metadata/forms), or wide 760px (forms with
  a preview such as Page Setup). All share a 12px radius, 16px title, 30px Close,
  36px fields, and 34px footer actions; mobile width is viewport-bounded.
- **Application menu** (`.app-menu-bar` + `.app-menu-popover`) — top-level
  File/Edit/View/Insert/Format/Tools/Help triggers backed by the shared command
  registry. Arrow keys move within and between menus, Escape restores the active
  trigger, unavailable commands expose a reason, and narrow layouts contain the
  full strip in a local horizontal scroller rather than overflowing the page.
- **Panel** — an inset, bordered surface (left/right) with
  `--radius-popover`, low desktop elevation, a heading row (title + close), and
  a scrollable body. Narrow-screen inspectors become viewport-inset drawers
  with the same radius/border and stronger popover elevation. Docking changes;
  surface shape does not. Empty states remain *useful* (never dead).
- **Find / Replace card** (`.find-panel`) — a 440px top-right popover with a
  heading/status row, dedicated search row, wrapping option chips, replacement
  field, and trailing action row. It becomes an 8px-inset viewport card on narrow
  screens. Previous/Next select a model-backed range and center its engine-drawn
  highlight in the document viewport; the card itself remains fixed.
- **Segmented** (`.segmented`) — mutually-exclusive choices (theme, vertical align).
- **Chips / mini-grids** — border presets, insert-table grid picker.
- **Inputs** — dialog and Find fields are 36px high; numeric fields
  (indent/spacing) are right-aligned with tabular numerals.

## 4. Feature-placement map

Where things live, so we stay consistent as we add features:

- **Character & paragraph formatting** → toolbar (+ floating selection toolbar).
- **Paragraph structure** → compact spacing popover for the frequent line/paragraph
  spacing choices; shared live right inspector for indentation, shading, borders,
  and line/page-break controls; ruler for direct indents and tabs.
- **Tables** → Insert-table (toolbar grid picker); contextual Table ribbon for
  select/structure/merge; compact cell-format popover; right properties inspector
  for measurements and layout. The right-click structure menu remains an
  alternative pointer path.
- **Navigation** (outline, pages, find) → left rail/panel + menus + ⌘⇧P.
- **Inspect/apply** (styles, properties, comments) → right context panel.
- **Document actions** (open/save/export/print, page setup) → top bar + menus + ⌘⇧P.
- **View** (zoom, page/width, dark mode) → status bar + Settings.

## 5. Decisions

- Keep the **single settable accent**; brand default orange, reference blue is a
  preset. Structure/spacing follow the reference; colour does not have to.
- **Tabbed ribbon**: the working Home/Insert/Table/View/**Review** ribbon is now the
  primary toolbar (five tabs as shipped; this doc and `64` both said four until
  2026-09-15). A **Layout** tab is the next one this rule licenses — the page/section
  commands to fill it already ship, scattered across View, Tools and Insert; see `105`
  UX-010. Tabs and controls appear only when the underlying command is real;
  unavailable contextual commands remain visibly disabled.
- **Collaboration avatars / Share** in the reference depend on a host-owned
  multi-user identity and sync contract. We do **not** stub them. Comments and
  suggestions remain available through their implemented local review surfaces.

## 6. Delivery phases (incremental, each fully functional)

1. **Shell + restyle** — grid scaffold, refined tokens, restyled top bar / toolbar /
   status bar. Plus the first functional left region: **Outline** (heading tree that
   scrolls the document). *(this slice)*
2. **Command palette (⌘⇧P)** — over existing actions.
3. **Right context panel — Styles** (apply a paragraph style) then **Properties**
   (live selection inspector).
4. **Floating selection toolbar.**
5. **Find / Replace** (rail + menu + ⌘⇧P).
6. **Vertical ruler.**
7. **Ribbon tabs** (if/when control count warrants).
8. **Comments** (needs a comments model — gated on that phase).

Each phase is a PR; the shell in §2 does not change, only fills in.

## 7. Design system v2 (2026-11-01, `109` UX-052..UX-055)

The 2026-11-01 audit measured the chrome against Google Docs, Word for the web,
opencalc and the ProseMirror docs editor: 30 font sizes and 9 weights in the
stylesheet, 8 control heights in the visible Home chrome, 31 radii, 42 shadows,
two primary colours (graphite Apply buttons beside accent toggles), a disabled
Share button in the header's hero slot, and editing mode and chrome mode each
offered in two places. The tokens existed; nothing made a rule use them. v2 is
one scale per property and a guard that refuses anything else.

**The owner's decisions (2026-11-01):** one primary colour, the accent; Share is
hidden when the host has not configured sharing; editing mode lives only in the
header chip and Compact/Ribbon in one place; contextual ribbon tabs appear only
when relevant; the status bar keeps word count, page, zoom and contextual chips;
every dialog fits 1280×720 and 390×844 with a scrolling body and a pinned footer;
one docked side-panel pattern; dark elevation by lighter surfaces and a hairline,
not shadows.

### 7.1 Tokens

All chrome lengths are rem on the fluid root (`chrome_units.test.mjs`), so the
px figures below are at the 16px root that holds from 1280 to 1440px.

| Family | Tokens | Values | Roles |
| --- | --- | --- | --- |
| Type size | `--fs-xs` `--fs-sm` `--fs-md` `--fs-lg` `--fs-xl` | 11 / 12 / 13 / 15 / 20 | chips, shortcut hints, big-button labels · status, meta, field labels · body, menus, tabs · dialog and panel titles · File page heading |
| Weight | `--fw-regular` `--fw-medium` `--fw-semibold` | 400 / 500 / 600 | body · controls, tabs, labels · titles and headings |
| Icon | `--icon-sm` `--icon-md` `--icon-lg` | 16 / 20 / 24 | status bar, chips, menu trailing · toolbar, menus, panel actions · big ribbon buttons, mobile |
| Radius | `--r-xs` `--r-sm` `--r-md` `--r-lg` `--r-pill` | 4 / 6 / 8 / 12 / pill | checkbox, chip, swatch · buttons, inputs, tool buttons · menus, popovers, cards, panels, toolbar card · dialogs, sheets · avatars, mode chip |
| Height | `--h-dense` `--h-tool` `--h-ctl` `--h-touch` | 24 / 28 / 32 / 44 | status items, chips · toolbar and ribbon buttons · dialog buttons, inputs, selects · coarse pointer |
| Space | `--space-1`..`--space-6` | 4 / 8 / 12 / 16 / 20 / 24 | the 4px grid |
| Elevation | `--e1` `--e2` `--e3` | card · menu/popover · dialog | light: hairline plus soft shadow; dark: `none` / hairline ring only |
| State | `--hover` `--press` `--selected` `--selected-ink` | ink 6% / 10% · accent 12% | washes every control shares |
| Roles | `--surface-raised` `--icon` `--line-control` `--ink-disabled` `--scrim` | | dialogs over panels · toolbar glyphs · input boundary (3:1, WCAG 1.4.11) · disabled text · modal backdrop |

**Primary.** `--primary-bg` is `var(--accent)` in both themes, `--primary-ink`
is `var(--accent-ink)` and hover is `--accent-hover` (the accent darkened toward
black, because white text sits on it in both themes). There is no graphite
primary any more. The palette names a host may override (`brand_contract.mjs`
`OVERRIDABLE`) are unchanged: the v2 roles are derived from them where they can
be, so a white-label re-skins v2 without new keys.

**Palette.** Light: desk and chrome `#f3f4f6`, surface `#ffffff`, sunken
`#f6f7f9`, ink `#1b1e24`, secondary `#555c68`, tertiary `#656b76`, lines
`#e6e8ec`/`#d5d9e0`. Dark: desk `#111317`, chrome `#16181d`, surface `#1d2026`,
raised `#252930`, ink `#e7e9ec`, secondary `#a6acb5`, tertiary `#9aa0a9`, lines
`#2c3037`/`#3a3f48`, scrim 60%. Every role clears its floor in all three palettes
(`style_tokens.test.mjs`, `theme-contrast.spec.mjs`).

### 7.2 The guard

`webapp/tests/design_scale.test.mjs` fails the build when a chrome rule writes a
font size, weight, radius or blurred shadow outside the scale. The document region
(the sheet, overlay, rulers; `tests/css_regions.mjs`, shared with
`chrome_units.test.mjs`) is out of scope, and so are named content previews (the
drop-cap sample, the Styles gallery, the symbol grid, the font list), each listed
with its reason. The old token names (`--fs-body`, `--radius-popover`,
`--shadow-dialog`, `--icon-control`, ...) were swept and deleted, not aliased:
an alias nobody may use is a second name for the same thing.

### 7.3 What v2 deliberately keeps

- The accent `#3355c4` and the settable-accent presets.
- Inter and Material Symbols, self-hosted (`chrome_fonts.test.mjs`).
- Ribbon group captions: the owner rejected the caption-less band once already
  (see the `.rgroup-label` comment in `style.css`), so the audit's "captions off"
  is not taken.
- The paper layer (`--paper-*`): the document is white in both themes.
