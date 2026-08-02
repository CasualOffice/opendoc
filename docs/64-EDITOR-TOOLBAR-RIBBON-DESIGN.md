# 64 — Editor Toolbar / Ribbon — Competitive Analysis & Design

Status: Home band correction and functional Vellum-style application menu bar
implemented (P1G-UI-RIBBON-CORRECTION, P1G-UI-APP-MENUS).
Insert/Table/View tabs and the contextual Table tab retained.
Companion to [doc 63](63-EDITOR-UI-UX-DESIGN-SYSTEM.md) (shell & design system).
Reference: the newer `Vellum.dc.html` shipped inside the owner's
`Vellum Design Book` bundle (SHA-256
`3fd9ca86078d224ad83b12115ff138dbbcce8e0ab21317aaeb6a4cc5f35be16e`).
The checked-in `docs/assets/template-reference-prototype.html` is an older
revision and must not be treated as the exact toolbar source.

## Title and application-menu contract

The title bar follows the reference's two-row document block without copying its
prototype-only behavior:

- row one contains the editable download filename and an honest local lifecycle
  state: **Opened**, **Edited**, or **Downloaded**. It never claims cloud save or
  synchronization;
- row two exposes **File · Edit · View · Insert · Format · Tools · Help**;
- each menu is populated from the same `editorCommands()` descriptors as the
  command palette and contextual surfaces, preserving dynamic labels, shortcuts,
  disabled reasons, and command transaction paths;
- Up/Down/Home/End navigate a menu, Left/Right switches categories, Escape closes
  and restores the trigger, outside pointer-down dismisses, and the popover is
  clamped to the viewport;
- the menu strip scrolls locally at narrow widths, while the header and document
  never gain page-level horizontal overflow;
- the reference's collaborator avatars and Share button remain omitted until a
  host-owned collaboration identity/sync contract exists;
- Search remains **⌘⇧P** because **⌘K** is the implemented Add/Edit Link shortcut.

The Vellum file routes every top-level label to one generic palette. That is
reference scaffolding, not product behavior; OpenDoc's categories are real.

## Home band — corrected placement contract

The Home tab is a single **no-wrap** band of divider-separated groups with a
small centered label under each, matching the reference's group composition:

- **Undo** — Undo above Redo in a two-row stack, plus Clear Formatting. Format
  Painter remains omitted because it is not implemented.
- **Clipboard** — **Paste**, **Cut**, and **Copy** use uniform vertical tiles
  (icon above label). All three call the same clipboard actions the command
  palette/keyboard use. *Format Painter is omitted — not implemented.*
- **Font** — two rows: (1) font family and size; (2) B I U S,
  sub/super, text color, highlight. *Grow/shrink font and change-case are omitted
  — not implemented.*
- **Paragraph** — two rows: (1) bullets, numbering, checklist, indent −/＋, ¶
  (paragraph properties); (2) align L/C/R/J, line-spacing,
  restart/continue-numbering.
  *Multilevel list and paragraph sort are omitted — not implemented.*
- **Styles** — a visible all-styles selector backed by `doc.listStyles()` plus a
  four-card quick gallery. The selector guarantees every document style remains
  reachable; the cards are previews, not the sole navigation mechanism.
- **Editing** — **Find** / **Replace** use the same icon-above-label tiles (both
  open Find & Replace).
  *Select is omitted — no functional select menu.*
- **Editing mode** — Edit/Suggest/Read only is visible in the Home band and is
  mirrored persistently in the footer. Both surfaces drive the same mode state.

**No horizontal scrollbar.** Groups that do not fit collapse, right-to-left,
into a "⋯" command surface (`#ribbonOverflowBtn`/`#ribbonOverflowMenu`). The
surface must reflow groups on narrow screens, move keyboard focus into its first
enabled command, close and restore focus on Escape, and retain tooltips after
groups move outside the ribbon DOM subtree. Overflow is recomputed on resize and
synchronously on tab switch. Clipboard and Editing are primary pinned groups;
the compact Mode group is also kept inline whenever the viewport can contain
all three (and remains available in the footer at smaller widths).

**Compact ↔ ribbon view.** A toggle in the tab strip (`#ribbonViewToggle`)
collapses the band to just the tabs (compact view — more document room) and
expands it back, mirroring Word's "collapse the ribbon". The choice persists
(`localStorage`); clicking any tab while collapsed restores the band.

Owner directives driving this note:
- The current flat toolbar does **not** look like the reference; move toward it.
- **Competitive analysis + a feature "refresh" before developing** any feature.
- **Consistency** — everything follows doc 63's tokens/components.
- **Functional only** — no dead controls ever ship.

## 1. Competitive analysis — the formatting chrome

| Product | Model | Strengths | Costs | Takeaway for us |
|---|---|---|---|---|
| **MS Word** | Tabbed **ribbon** (Home/Insert/Draw/Layout/References/Review/View) + menu, labeled groups, single-row-no-wrap | familiar, discoverable, scales to hundreds of commands | heavy, tall, many tabs feel empty without deep features | the *look* the owner wants; adopt grouped ribbon, but only tabs we can fill |
| **Google Docs** | **Menu bar** + one compact toolbar, overflow "⋯", **contextual** bars (table/image) | approachable, fast, low chrome | less "powerful" feel; menus hide depth | keep a rich single row per tab; make table/image contextual |
| **OnlyOffice** | Tabbed ribbon (compact), file-menu tab | Office-parity in a browser, dense but tidy | tab overload for light docs | proof a browser ribbon works; keep tabs lean |
| **Notion** | No persistent toolbar — **floating selection bar** + `/` slash menu | minimal, content-first | weak for print/office fidelity | keep the implemented floating selection bar + command palette |
| **Vellum (reference)** | Word-style ribbon + menu bar + floating bar + command search + right context panel | the target aesthetic | full build is large | our north star; build the subset we can make functional |

**Synthesis.** The reference is a Word/OnlyOffice **tabbed ribbon**. Word's mistake is
empty tabs; Google's insight is *contextual* toolbars and not showing what you can't
do. So: adopt the **ribbon's grouped, labeled, single-row look**, but with **only the
tabs we can fill with functional controls**, and make table controls **contextual**.

## 2. Refresh — what our toolbar has today vs the ribbon

Everything below is already **functional** (this is a re-organisation, not new engine
work): paragraph style · font · size · B/I/U/strike · text colour · highlight ·
super/sub · align L/C/R/justify · line & paragraph spacing · indent ± · lists ·
paragraph options (indent/shading/borders/tabs/breaks) · insert-table (grid) · table
& cell formatting · zoom · outline · settings · undo/redo (⌘Z) · save · open · ⌘⇧P.

The former Clipboard, Find/Replace, menu-bar, and floating-selection-bar gaps are
implemented. Remaining reference-only chrome is limited to capabilities with no
host/runtime contract yet, chiefly collaboration avatars and Share.

## 3. Proposed design — a lean contextual ribbon

A **single-row tabbed ribbon** replacing the flat toolbar, styled per doc 63
(hairline group dividers, tiny uppercase group labels, no wrap, overflow scrolls):

```
┌ Home │ Insert │ Table* │ View ────────────────────────────────────┐
│ ⤺ ⤻ │ Style ▾ Font ▾ Size ▾ │ B I U S  A▾ 🖍▾ x² x₂ │ ≡≣≢≡ ⋯     │  (Home)
│ Undo   Styles·Font            Format                 Paragraph      │
└────────────────────────────────────────────────────────────────────┘
```

- **Home** — Undo/Redo · Styles (Style select or a small gallery) · Font · Size ·
  B/I/U/S · colour · highlight · super/sub · align · spacing · indent± · lists ·
  paragraph-options (¶). Groups: *Undo · Text · Font · Paragraph*.
- **Insert** — Insert table (grid picker); later: image, page break, link, symbol.
- **Table** — *contextual*: enabled only in a table; the cell/table-format controls
  (shading, borders, valign, table borders/align) inline + row/column ops.
- **View** — zoom −/＋/%, Outline toggle, Settings (theme/accent).

Rules:
- Only tabs with functional controls appear. **Table** is present but **disabled**
  unless the caret is in a table (contextual, not dead — matches Google Docs).
- Group labels are the tiny uppercase captions under each cluster (reference style).
- Overflow: the row never wraps; it horizontally scrolls on very narrow widths.
- Everything reuses doc 63 components (`.fmt`, `.ctl`, popovers, segmented).

Deferred (functional when shipped): collaboration/Share chrome and commands whose
underlying document operations do not yet exist. Dead placeholders remain forbidden.

## 4. Build order (after sign-off)
1. Ribbon scaffold: tab bar + Home tab (re-home today's controls into labeled groups).
2. Insert + View tabs.
3. Table tab (contextual) — move the Table popover's controls inline.
4. Floating selection toolbar. *(implemented)*
5. Find/Replace → Editing group. *(implemented)*
6. Menu bar, backed only by functional shared commands. *(implemented)*

## 5. Open decision (for the owner)
- **A. Tabbed ribbon** (this proposal) — closest to `template.png`; risk: light tabs.
- **B. Rich single toolbar + contextual bars** (Google-Docs model) — cleaner for our
  current control count; less like the reference.
Recommendation: **A**, since the owner referenced the ribbon; keep it lean/contextual
so no tab feels empty.

## 6. Floating selection toolbar — competitive analysis & design

Competitive read (selection-time formatting):
- **MS Word** — mini-toolbar fades in by the cursor: font/size/B/I/U/colour/
  highlight/styles. Fast but busy.
- **Google Docs** — no floating bar; relies on the top toolbar, but shows
  link/comment affordances on a selection.
- **Notion** — floating bar: B/I/U/S/code/link/colour/comment/turn-into. The
  cleanest, most-copied model.
- **Medium** — minimal: B/I/link/H/quote. Content-first.
- **Reference (`template.png`)** — `Normal▾  B I U  🖍 A▾  🔗 💬  ⋯`.

Synthesis / decision: a **compact bar that appears above a non-empty selection**
with only the **functional** inline actions we have today — **B I U S · highlight ·
text colour**. Link/comment are deferred (no link-edit UI / comments model yet), so
they are **not** shown (functional-only). It complements the ribbon (doesn't replace
it) and reuses the ribbon's handlers verbatim.

Behaviour: shows on a range selection, centred just above the selection's bounding
box; hides when the selection collapses, on scroll/zoom, and during edits; never
covers the first line (flips below when there's no room above). Reuses `.fmt`
buttons + the popover tokens from doc 63.

## 7. Formatting reflection contract

Toolbar state reflects effective document formatting, not only direct run
properties. The bridge resolves document defaults, paragraph-style inheritance,
character-style inheritance, and direct formatting before reporting font, size,
and run toggles. A multi-run selection reports a value only when it is uniform.

The font control displays the authored/requested family, including a family
resolved from a document theme. When the document declares no family at all, it
displays the engine's implicit default, Roboto. Imported families not present in
the starter dropdown are admitted as a temporary reflected option. Layout
substitution and per-glyph coverage fallback remain renderer diagnostics; they
must not replace an authored family in the control because selecting or resaving
that physical fallback would change document intent.
