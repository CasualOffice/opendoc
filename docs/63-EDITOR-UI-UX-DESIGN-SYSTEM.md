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
- **Icons** — self-hosted Material Symbols Outlined at a 16/18/20px compact,
  control, and rail grid. Bold/italic/underline/strike remain literal typographic
  glyphs, matching the reference. Font and license files are checked in; editor
  chrome does not depend on a runtime font request.
- **Neutrals** — near-white surfaces on a light-grey canvas; hairline dividers
  (`--line`); soft, low-contrast shadows for lifted surfaces (menus, panels).
- **Shape & space** — rounded surfaces (`--radius` 8px, small controls 6–7px),
  generous whitespace, 4px control gaps, comfortable 28–32px control heights.
- **Light & dark** — both first-class (`:root[data-theme]` + `prefers-color-scheme`).
- **Motion** — quick (120ms) fades/slides for popovers and panels; nothing bouncy.

## 2. Shell regions (target layout)

The reference calls out six regions; our shell is a CSS grid of:

```
┌───────────────────────── top bar ─────────────────────────┐
│ brand · (menu/title) · doc title ✓saved   settings · save  │
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
Overlays (float above): command palette ⌘K (6), floating selection
toolbar, context menus, toolbar popovers.
```

| # | Region | Purpose | Contents (functional, added over time) |
|---|--------|---------|----------------------------------------|
| 1 | **Top bar + toolbar** | identity + primary actions + formatting | brand, document title + save state, Open/Save/Settings; formatting controls grouped (Style/Font/Size · B I U · align/spacing/indent · lists · tables). Later: menu bar, ribbon tabs. |
| 3 | **Left nav rail + panel** | document navigation | **Outline** (heading tree → scroll-to) first; later Pages (thumbnails), Search/Find. The rail only shows an icon once its panel is functional. |
| 4 | **Right context panel** | inspect/act on the selection | later: Styles (apply), Properties (paragraph/table inspector), Comments (needs a comments model — a later phase). Collapsible; never empty. |
| 5 | **Status bar** | document state | page X of Y, word/paragraph counts, zoom (−/＋/%), later language, edit mode, view toggles, save/sync state. |
| 6 | **Command palette (⌘K)** | fast command access | fuzzy search over every action (insert table, apply style, export, …). |
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
  toolbar-sized popovers.
- **Panel** — an inset, bordered surface (left/right) with
  `--radius-popover`, low desktop elevation, a heading row (title + close), and
  a scrollable body. Narrow-screen inspectors become viewport-inset drawers
  with the same radius/border and stronger popover elevation. Docking changes;
  surface shape does not. Empty states remain *useful* (never dead).
- **Segmented** (`.segmented`) — mutually-exclusive choices (theme, vertical align).
- **Chips / mini-grids** — border presets, insert-table grid picker.
- **Inputs** — numeric fields (indent/spacing) right-aligned, tabular numerals.

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
- **Navigation** (outline, pages, find) → left rail/panel + ⌘K.
- **Inspect/apply** (styles, properties, comments) → right context panel.
- **Document actions** (open/save/export/print, page setup) → top bar + ⌘K.
- **View** (zoom, page/width, dark mode) → status bar + Settings.

## 5. Decisions

- Keep the **single settable accent**; brand default orange, reference blue is a
  preset. Structure/spacing follow the reference; colour does not have to.
- **Tabbed ribbon**: the working Home/Insert/Table/View ribbon is now the primary
  toolbar. Tabs and controls appear only when the underlying command is real;
  unavailable contextual commands remain visibly disabled.
- **Comments / collaboration / Share** in the reference depend on a comments model
  and multi-user sync (later phases). We do **not** stub them; the right panel ships
  with only its functional tabs.

## 6. Delivery phases (incremental, each fully functional)

1. **Shell + restyle** — grid scaffold, refined tokens, restyled top bar / toolbar /
   status bar. Plus the first functional left region: **Outline** (heading tree that
   scrolls the document). *(this slice)*
2. **Command palette (⌘K)** — over existing actions.
3. **Right context panel — Styles** (apply a paragraph style) then **Properties**
   (live selection inspector).
4. **Floating selection toolbar.**
5. **Find / Replace** (rail + ⌘K).
6. **Vertical ruler.**
7. **Ribbon tabs** (if/when control count warrants).
8. **Comments** (needs a comments model — gated on that phase).

Each phase is a PR; the shell in §2 does not change, only fills in.
