# 64 — Editor Toolbar / Ribbon — Competitive Analysis & Design

Status: proposal (awaiting confirmation before build)
Companion to [doc 63](63-EDITOR-UI-UX-DESIGN-SYSTEM.md) (shell & design system).
Reference: `template.png`.

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
| **Notion** | No persistent toolbar — **floating selection bar** + `/` slash menu | minimal, content-first | weak for print/office fidelity | add a floating selection bar (doc 63) + we already have ⌘K |
| **Vellum (reference)** | Word-style ribbon + menu bar + floating bar + ⌘K + right context panel | the target aesthetic | full build is large | our north star; build the subset we can make functional |

**Synthesis.** The reference is a Word/OnlyOffice **tabbed ribbon**. Word's mistake is
empty tabs; Google's insight is *contextual* toolbars and not showing what you can't
do. So: adopt the **ribbon's grouped, labeled, single-row look**, but with **only the
tabs we can fill with functional controls**, and make table controls **contextual**.

## 2. Refresh — what our toolbar has today vs the ribbon

Everything below is already **functional** (this is a re-organisation, not new engine
work): paragraph style · font · size · B/I/U/strike · text colour · highlight ·
super/sub · align L/C/R/justify · line & paragraph spacing · indent ± · lists ·
paragraph options (indent/shading/borders/tabs/breaks) · insert-table (grid) · table
& cell formatting · zoom · outline · settings · undo/redo (⌘Z) · save · open · ⌘K.

Gap vs the reference ribbon: **Clipboard** group (we have cut/copy/paste via keys, no
buttons), **Find/Replace** (not built), **menu bar**, **floating selection bar**.

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

Deferred (each its own analysed slice, functional when shipped): **Find/Replace**
(unlocks the Editing group + a ⌘K/⌘H entry), **Clipboard buttons** (cut/copy/paste),
**menu bar** (File/Edit/…), **floating selection toolbar**.

## 4. Build order (after sign-off)
1. Ribbon scaffold: tab bar + Home tab (re-home today's controls into labeled groups).
2. Insert + View tabs.
3. Table tab (contextual) — move the Table popover's controls inline.
4. Floating selection toolbar.
5. Find/Replace → Editing group.
6. Menu bar (only once its items are all functional).

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
