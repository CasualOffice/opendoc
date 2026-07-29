# 69 — MS Word 2026 Competitive Design Analysis & Guide

Status: proposal (research + recommendations; awaiting owner sign-off before any build)
Date: 2026-07-30
Depends on / extends: [doc 63](63-EDITOR-UI-UX-DESIGN-SYSTEM.md) (shell & design system, accepted direction),
[doc 64](64-EDITOR-TOOLBAR-RIBBON-DESIGN.md) (toolbar/ribbon competitive read — Word/Docs/OnlyOffice/Notion),
[doc 67](67-EDITOR-UX-GAP-ANALYSIS.md) (authoritative P0–P3 gap table, 2026-07-29)
Grounded in: `webapp/editor.html` (DOM/icons), `webapp/src/style.css` (tokens/components),
and `template.png` (repo root — the project's own accepted design reference per doc 63) as of
this writing.

## 0. Scope and what this doc is not

Doc 64 already did the competitive read for the **ribbon/toolbar model** (tabbed ribbon vs
menu-bar-toolbar vs floating-bar) and the owner already picked tabbed ribbon. Doc 67 already
has the authoritative **behavioral** gap list (editing, selection, clipboard, IME, etc.). This
doc does **not** re-litigate either. It covers the remaining three axes the owner asked for that
neither doc treats in depth: **shell chrome layout parity**, **icon design language**, and
**typography/spacing system** — benchmarked against Word 2026 desktop, Microsoft 365 Word for
the web, **and** `template.png` itself (§1.5, §2.6) — with concrete recommendations and an
explicit do-not-change list. No editor code changes ship with this doc; it is research + design
only.

**Important distinction kept throughout:** Word 2026 is an *external* competitive benchmark —
matching it is optional, brand-dependent. `template.png` is the project's **own already-accepted**
design target (doc 63: "Status: accepted direction... Reference: template.png"); gaps against it
are not optional in the same way — they are unfinished work against a decision already made.
Sections §1.5 and §2.6 below re-examine the actual image (not just docs 63/64's prose summary of
it) and are the more load-bearing findings in this doc.

## 1. Layout

### 1.1 Word 2026 desktop chrome — anatomy

- **Title bar**: app icon/name, then a centered **Search** box (magnifying glass + "Search" /
  ⌥Q). This *is* the old "Tell Me" box — Microsoft relocated it from the ribbon's right edge into
  the title bar during the Fluent ribbon refresh; it is not tab-scoped, always visible, and is
  the single command-discovery surface (search a command, get a definition, or launch a
  Smart Lookup). Right of it: co-authoring avatars, Share, and the account menu.
- **Quick Access Toolbar (QAT)**: a small, user-customizable, **always-visible** strip (Save,
  Undo, Redo, plus pinned commands) that sits above or below the ribbon tab row, independent of
  which ribbon tab is active.
- **Ribbon**: tabbed (File, Home, Insert, Draw, Design, Layout, References, Mailings, Review,
  View, Help, + contextual tabs like "Table Design"/"Layout" that only appear with a table
  selected, painted with a tinted band above the tab to visually group them). Each tab is a
  single row of **labeled groups** separated by hairline dividers; many groups have a small
  **dialog-launcher arrow** (⤡) in the bottom-right corner opening the full legacy dialog (e.g.
  Font, Paragraph). A "Collapse the Ribbon" toggle and a simplified single-row mode are both
  user-selectable.
- **View tab**: Read Mode/Print Layout/Web Layout, Immersive (Learning Tools), Ruler/Gridlines/
  Navigation Pane checkboxes, Zoom group (Zoom %, 100%, One/Multiple Page, Page Width), Window
  group (split/arrange).
- **Status bar** (bottom): page **X of Y**, word count (click for full count/readability stats),
  a language indicator, proofing (spelling/grammar) icon, and — at the **far right** — View
  shortcuts (Read/Print/Web) then a **Zoom slider + %** control. This left-to-right ordering
  (document meta → view mode → zoom) is consistent across every Office app.

### 1.2 Word for the web (Microsoft 365)

Collapses to a **single-row** ribbon per tab (no dialog launchers, fewer groups), keeps the same
title-bar Search box and tab set (minus deep desktop-only features), and adds a top-right
**Comments/Share** cluster. Status bar is thinner: word count + language only; zoom moves to a
bottom-right slider. This is the more relevant comparison target for OpenDoc (both are browser
apps), though OpenDoc should still track desktop Word's chrome since doc 63/64 already aim at a
"Vellum/Office-grade" ceiling, not the web's simplified floor.

### 1.3 OpenDoc's current shell vs Word — side by side

| Region | Word 2026 desktop | Word for the web | OpenDoc today (`editor.html`/`style.css`) |
|---|---|---|---|
| Title/top bar | title-bar Search box (center), QAT (left, always visible), Share/account (right) | same, minus QAT | 44px `.bar`: brand + "testing" badge (left), doc title (center-left, hidden until a doc loads), Open/Save/Settings (right). **No visible search/palette affordance, no QAT-equivalent.** |
| Command search | Search box, always visible, title bar | same | ⌘K/Ctrl+K command palette exists (`#cmdPalette`, 35+ commands, fuzzy filter) but has **no visible trigger** anywhere in the DOM — keyboard-only. Doc 67 already flags this under "Command discoverability" (P1). |
| Always-visible undo/redo/save | QAT — visible regardless of active ribbon tab | same | Undo/Redo buttons live **inside the Home ribbon panel only** (`data-panel="home"`); switching to Insert/Table/View hides them from view (⌘Z still works, but the buttons don't). Save lives in the persistent top bar (this part **does** match QAT behavior). |
| Ribbon | tabbed, dialog-launcher arrows, contextual tinted tabs | tabbed, single row, no launchers | Tabbed (Home/Insert/Table/View, per doc 64), single row, **no dialog launchers** (not needed — popovers substitute), Table tab is contextual via `disabled` state (not a tinted band). |
| View tab controls | Read/Print/Web modes, Ruler/Gridlines/Navigation toggles, Zoom group, Window group | reduced subset | Only Outline toggle + zoom −/＋. No page-layout view modes (not applicable — OpenDoc has one continuous-page view), no ruler visibility toggle (ruler is always shown once a doc is open), no gridlines. |
| Status bar | page X/Y · words · language · proofing · view-mode icons · zoom (far right) | words · language · zoom | `words · paragraphs` (left) · `Page X of Y · zoom −/＋/%` (right) — **same left-meta/right-view ordering convention as Word**, minus language/proofing (not yet modeled features). |
| Comments/Share | top-right cluster (web) | top-right cluster | Not present — comments are a separate design-in-progress track ([doc 68](68-COMMENTS-AND-SUGGESTIONS-DESIGN.md) is on comments UX; no Share/collab feature exists per doc 63 §5, correctly not stubbed). |

### 1.4 Concrete gaps and recommendations

1. **No persistent Undo/Redo (and arguably Save) across ribbon tabs.** This is a real gap
   against Word's QAT model: a user on the Insert/Table/View tab loses the visible undo
   affordance. **Correction on re-checking `template.png` directly (§1.5):** the project's own
   accepted reference does **not** show a persistent Undo/Redo cluster anywhere in its chrome
   either (no QAT row, nothing near the app icon) — so this is a Word-derived nice-to-have, not
   a template-fidelity gap. Downgraded accordingly: *optional* — hoist Undo/Redo into the top
   `.bar` or a slim strip left of the ribbon tabs only if the owner independently values it, not
   because the reference requires it.
2. **⌘K has no visible entry point.** Word's discoverability fix (moving Tell Me into a
   permanent, obvious title-bar box) is exactly the gap doc 67 already names. *Recommend*: a
   small search-icon-affordance in the top `.bar` (e.g. next to Settings) that opens the same
   `#cmdPalette` — no new logic, just a visible trigger for the existing feature. Low effort,
   closes a real discoverability gap, and is consistent with "functional only" (it activates a
   fully-working feature, not a stub).
3. **Quick Access Toolbar itself — do not build a customizable QAT.** Word's QAT is
   user-configurable pin/unpin; that is a meaningfully bigger feature (persisted per-user
   command list) than "always show Undo/Redo/Save." Recommend the fixed 2–3-button version only;
   defer true customization indefinitely (no evidence of demand, adds real complexity for a
   thin win).
4. **View tab is thin next to Word's** — but this is *expected*, not a gap: OpenDoc has a single
   continuous-page view (no Read/Web modes to switch between) and no gridlines feature yet. Adding
   view-mode buttons that all do the same thing would violate the functional-only rule. Leave as
   is until a second view mode (e.g. page-width vs whole-page fit) actually exists.
5. **Status bar ordering already matches Word's convention** (doc meta left, view/zoom right) —
   this is a "keep," not a gap; call it out so it isn't accidentally "fixed" into something else
   during a future refactor.

### 1.5 OpenDoc vs `template.png` itself — the project's own accepted reference

Docs 63/64 summarize `template.png` in prose; this re-examines the actual image against the
current `editor.html`/`style.css`, since — unlike Word — closing gaps here is unfinished work
against a decision already made, not an optional parity choice.

| Region | `template.png` (Vellum) | OpenDoc today | Status |
|---|---|---|---|
| Menu bar | File Edit View Insert Format Tools Table Window Help, above the ribbon | absent | Known gap, already deferred to "menu bar (only once its items are all functional)" — doc 64 §4 item 6. Still true. |
| Ribbon tabs | 7: Home/Insert/Draw/Layout/References/Review/View | 4: Home/Insert/Table/View | Expected — doc 64 explicitly scoped to "only tabs we can fill with functional controls." Draw has no engine support; References/Review gate on TOC-authoring and doc 68 (comments/track-changes) respectively. |
| Clipboard group | visible Cut/Copy/Paste/Format Painter buttons in Home | none (cut/copy/paste are keyboard-only) | Known gap, doc 64 §2 already names it. Still open. |
| Styles control | a **live-preview gallery** — Normal/Heading 1/2/3/Title rendered each in its own real type style | a plain `<select id="paragraphStyle">` with plain-text option labels | **Not previously called out this precisely.** A real, visible fidelity gap: the gallery pattern is materially richer than a dropdown and is one of the more recognizably "Word-like" details in the reference. |
| Right context panel | Comments / Properties / Styles tabs, populated | **does not exist** — no right-hand `<aside>` in `editor.html` at all | The single largest unbuilt region from doc 63's own shell map (§2, region 4). Correctly phased for "later" (doc 63 §6 step 3), but worth stating plainly: this is 0% built, not partially built. |
| Left nav rail | 5 icons: Outline, Pages, Comments, Bookmarks, Search | 1 icon: Outline | 4 of 5 rail destinations don't exist yet. Expected per the phased plan (doc 63 §6); quantified here for the first time. |
| Status bar | page X/Y · words · **language selector** · **Track Changes: Off toggle** · **4-icon view-mode cluster** (list/single-page/two-page/grid) · zoom slider · **"Synced" badge** | words · paragraphs · Page X of Y · zoom −/＋/% | Missing: language selector, track-changes toggle (blocked on doc 68), view-mode icon cluster (blocked on a second view mode existing at all), a distinct sync/saved badge separate from the doc-title area. |
| In-canvas page-break chip | a floating "Page Break / Page N of M · word count" tag sits right at the break point on the page | not present (page/word counts live only in the footer) | Small, low-cost polish item not previously identified; genuinely differentiating detail of the reference. |
| Floating selection toolbar | `Style ▾  B I U  highlight  color  🔗  💬  ⋯` | `B I U S · highlight · text color` (doc 64 §6) | Correctly, deliberately narrower — doc 64 already reasons link/comment out for lack of a link-edit UI/comments model. Consistent, not a new gap. |
| Command/search launcher | a docked bottom-right "Search commands (Ctrl+K)" box with a "Suggested" list | centered modal overlay (`#cmdPalette`) | Different anchoring, same function; no evidence either is objectively better — not flagging as a gap, just a layout difference worth knowing about if the shell is ever restructured. |

## 2. Icons

### 2.1 Word 2026's icon system (Fluent 2)

Microsoft 365's ribbon icons are Fluent 2: predominantly **outline/stroke-based** ("regular"
weight, ~1.5–2px stroke at a 24px frame, thinner at 16/20px), rounded corner terminals, drawn on
a strict **16 / 20 / 24px** grid with consistent optical sizing (a 20px icon and a 24px icon read
as the "same size" glyph at different weights, not just a scale-up). Many commands ship a
**filled variant** that swaps in for the active/toggled or hovered state (e.g. a comment icon
goes from outline to filled when a thread is open) instead of relying purely on a background-tint
pill. This is a distinct, deliberate visual language from Google's Material Design (which OpenDoc
currently uses).

### 2.2 OpenDoc's current icon set

Every icon in `editor.html` is an inlined `<svg viewBox="0 0 24 24">` with a **single solid-fill
path** — e.g. bold (`M15.6 10.79c.97-.67...`), undo (`M12.5 8c-2.65 0-5.05...`), align-left,
bulleted list. This is Google's classic **Material Icons "filled"** family (Apache-2.0, already
noted in the P1G-012 tracker entry), not the newer variable-weight "Material Symbols" outline
family the task brief anticipated — worth correcting: it's flat-fill silhouettes, sharper and
more geometric than Fluent's rounded stroke language, with **no outline/filled state pairing** —
active state is communicated entirely by the `.fmt[aria-pressed="true"]` background tint
(`--accent-soft` fill + `--accent-line` border), never by swapping the glyph itself.

### 2.3 Does the mismatch matter?

**No, not on its own.** Doc 63 §1 is explicit: OpenDoc has its own brand (graphite + orange, a
settable accent) and deliberately does not clone the reference's palette; the same logic extends
to iconography. A wholesale Fluent-icon replacement would cost real effort for a visual-parity
goal the owner never asked for, and Material's solid-fill glyphs are legible, consistent with
each other (same source family, same visual weight), and already reviewed as functional in every
P1G ribbon slice. **What does matter**, independent of Fluent-vs-Material, is internal
consistency of *sizing* — see §2.4 and §4.2, where the current set has a real (if minor) defect:
icon render size varies by container (15/17/18/20px) with no shared grid, which Fluent's
16/20/24 discipline avoids regardless of stroke-vs-fill style.

### 2.4 Icon-by-icon comparison (highest-traffic controls)

| Command | Word 2026 (Fluent 2) | OpenDoc today (Material-derived) | Notes |
|---|---|---|---|
| Bold / Italic / Underline | Outline "B"/"I"/"U" glyphs, 1.5px stroke | Filled "B"/"I"/"U" glyphs (solid) | Both instantly recognizable; letterform commands don't benefit much from outline vs fill — low-value to change. |
| Undo / Redo | Outline curved arrow, rounded cap | Filled curved-arrow glyph | Equivalent legibility; keep. |
| Alignment (L/C/R/Justify) | Outline horizontal bars, rounded ends | Filled horizontal bars, square ends | Word's rounded bar-ends read slightly softer; a low-cost refinement *if* a broader rounded-corner icon pass ever happens (see §4.2), not worth a one-off change. |
| Bulleted / numbered list | Outline dots/numbers + bars | Filled dots/numbers + bars | Equivalent; keep. |
| Insert table | Outline grid, rounded corners | Filled grid, square corners (`M20 3H4c-1.1 0-2...`) | Same silhouette concept either way; no action needed. |
| Find | Outline magnifying glass | Filled magnifying glass | Same shape family as the command-palette's search icon already (`#cmdInput`'s `.cmd-search-icon` reuses the same path) — internally consistent; keep. |
| Save | Outline floppy-disk-derived glyph (legacy metaphor, still used) | Filled floppy-disk glyph | Both apps keep the (anachronistic but universally understood) save icon; no change warranted. |
| Comment (not yet built) | Outline speech-bubble, **filled swap when a thread is open/active** | N/A — no comment UI yet ([doc 68](68-COMMENTS-AND-SUGGESTIONS-DESIGN.md) is designing this) | **This is the one place worth adopting Fluent's outline→filled active-state pattern** even without a full icon-language switch: when doc 68 ships, give the comment-indicator icon a filled variant for "thread open/selected" instead of (or in addition to) a background tint — cheap, and reads clearly at a glance in a margin full of small icons. |

### 2.5 What a Fluent-inspired refinement would concretely look like (if ever wanted)

If the owner later wants closer visual parity: (a) swap solid-fill paths for 1.5–2px stroke
outlines with rounded caps/joins on the same 24px canvas — a mechanical redraw, not a rethink of
which commands get icons; (b) adopt outline-for-default / filled-for-active as a second signal
alongside (not instead of) the existing `--accent-soft` tint, particularly useful once
comments/suggestions ship; (c) standardize on the 16/20/24 render grid from §4.2 regardless of
stroke style. None of this is recommended *now* — flagged only so a future decision has a
concrete "what would it take" answer instead of a vague "make it more Fluent."

## 3. Typography

### 3.1 Word 2026 chrome type

Windows Word 2026 renders ribbon/status-bar chrome in **Segoe UI Variable** (the modern
replacement for classic Segoe UI, still a system font, not web-downloaded); Mac Word uses
**SF Pro**. Sizes are dense: ribbon **group captions** ("Font", "Paragraph") run ~11px, command
labels/tab labels ~12px, status-bar text ~12px — a narrow, compact type scale by design (the
ribbon must fit dozens of labeled commands in one row without wrapping).

### 3.2 OpenDoc's current type scale — as actually used

`webapp/src/style.css` uses the system-UI stack `-apple-system, BlinkMacSystemFont, "SF Pro
Text", "Segoe UI", Roboto, sans-serif` at a `13px/1.5` body base — **this already matches Word's
own philosophy** (native OS font, not a bespoke webfont) rather than needing to be brought in
line with it; see §3.4.

The actual **sizes** in use, extracted directly from the stylesheet, are far more numerous than
Word's compact scale:

| Size | Where used | Weight(s) used there |
|---|---|---|
| 9px | ruler tick numbers, tab-stop glyph | 400 (numbers), 800 (tab glyph) |
| 10px | `.rgroup-label` (ribbon group captions), `.testing` badge, tab-corner glyph | 600–700 |
| 11px | `.settings-label`, `.menu-heading` (popover section captions), `.rgroup-hint`, `.oss` | 700 (labels), 400 (hint/oss) |
| 12px | footer/status bar, cmd-list group/hint text, find-status, zoom select, menu-item(?)/cmd-empty note | 400–550 |
| 13px | body base, `.btn`, `.doc-title`, `.ribbon-tab`, `.menu-item`, `.cmd-item`, find-panel inputs | 550 |
| 14px | `.ctl.swatch` "A" glyph | 700 |
| 15px | `.brand .mark`, `.cmd-search input` | 600–700 |
| 17px | header `.btn svg` render size (not text, but on the same "chrome scale") | — |

That's **8 distinct pixel sizes** and **5 distinct weights** (400/550/600/700/800) doing work
that Word accomplishes with roughly 3 sizes (11/12/13-ish equivalents at 96–100% zoom) and 2–3
weights. The more telling problem isn't the count — it's that **conceptually identical elements
use different values**: `.rgroup-label` (ribbon group caption) is 10px/600, while
`.settings-label` and `.menu-heading` (both also "small uppercase section captions" in a
popover) are 11px/700. Three instances of the same design *pattern* ("eyebrow" label) at two
different sizes and two different weights is the concrete inconsistency worth fixing, not the
existence of a dense scale (Word's is dense too, by design).

### 3.3 Recommendation — consolidate to a named scale, don't shrink further

Introduce scale tokens and map every current use onto them (pure refactor, no visual redesign):

```
--fs-micro:   9px   /* ruler ticks, tab-stop glyphs — leave as-is, already Word-dense */
--fs-caption: 10px  /* ALL uppercase "eyebrow" labels: rgroup-label, settings-label,
                        menu-heading, .oss — currently split across 10/11px, pick one */
--fs-small:   12px  /* status bar, cmd-list secondary text, find-status, zoom % */
--fs-body:    13px  /* default chrome: buttons, menu items, ribbon tabs, inputs — the
                        existing body base, unchanged */
--fs-medium:  15px  /* command palette search input, brand wordmark */
```

This collapses 8 raw values to 5 named ones and — critically — moves `.settings-label` and
`.menu-heading` onto the same `--fs-caption` as `.rgroup-label` so the "eyebrow label" pattern
reads identically everywhere it appears. Weight should similarly collapse toward 2–3 steps
(600 = default UI emphasis, 700 = caption/label emphasis); the current 550/650/800 one-off values
(`.btn` 550, `.acc`/misc 650, tab-glyph 800) are minor outliers with no clear rationale and can
fold into 600/700 without a visible regression.

### 3.4 Do NOT introduce a new web font

Doc 63 §1 already made this call deliberately: "system UI stack (SF Pro / Inter / Segoe)."
This is not a gap vs Word — it is the *same* choice Word itself makes (native OS font, zero
network font weight, perfect platform-native rendering/hinting). Do not recommend a bespoke
UI webfont chasing Word's Segoe UI Variable; there is no parity gain (users on Mac already see
SF Pro either way) and it would add load weight and a font-loading flash Word doesn't have.

## 4. Design guide additions

### 4.1 Spacing / sizing grid

Word's compact ribbon works on tight, consistent increments. OpenDoc's current control heights
are already fairly disciplined but not tokenized: 30px (`.btn`, `.fmt`, `.ctl select`), 34px
(`.ribbon-tabs`/`.ribbon-tab`), 36px (`.rail-btn`), 32px (`.footer`), 44px (`.bar`) all appear as
bare numeric literals repeated across selectors rather than named tokens. *Recommend* promoting
these to custom properties (`--h-ctl: 30px; --h-tab: 34px; --h-rail-btn: 36px; --h-footer: 32px;
--h-header: 44px`) — a pure refactor (no size changes), but it makes the grid legible and
prevents future drift (a new control silently picking 29px or 31px instead of the established
30px).

### 4.2 Icon grid

Current rendered icon sizes, pulled directly from the stylesheet: **15px** (`.zbtn svg`, footer
zoom), **17px** (`.btn svg`, header), **18px** (`.fmt svg`, ribbon/menu), **20px** (`.rail-btn
svg`, nav rail). Four ad hoc values with no shared logic, vs Word/Fluent's disciplined 16/20/24.
*Recommend* consolidating to **three** steps: **16px** (compact contexts: footer zoom, any future
dense inline control), **18px** (standard ribbon/menu/header controls — unifies today's 17 and
18 into one value), **20px** (nav rail — unchanged, it's already a deliberately larger touch
target). This is the single highest-value, lowest-risk fix in this doc: three CSS edits, no
markup change, immediately visible consistency win.

### 4.3 Elevation / radius conventions for floating surfaces

`--radius` (8px) and `--radius-sm` (7px) are defined and mostly honored, but the **floating
surfaces** (settings popover, context menus, command palette, find panel) drift from them:
`.settings-panel` uses a bare `12px`, `.context-menu` a bare `10px`, `.cmd-box` a bare `12px`
(inferred from the same pattern), while `.find-panel` correctly uses `var(--radius)` (8px). Four
conceptually-identical "floating card" surfaces at three different corner radii. Likewise the
popover drop-shadow is hand-written 3–4 times (`0 12px 40px rgba(20,22,28,.22)`, `0 10px 34px
rgba(20,22,28,.24)`, `0 12px 36px rgba(20,22,28,.22)` — all *slightly* different) instead of one
`--shadow-popover` token. *Recommend*: one `--radius-popover` (suggest 10px, splitting the
difference) and one `--shadow-popover` token, applied to all four floating surfaces. Pure
refactor, no functional change, removes an easy source of future one-off drift.

### 4.4 Contextual tab styling

OpenDoc's Table tab already does the right *behavioral* thing — present but `disabled` outside a
table, matching the "don't hide, disable" philosophy doc 64 chose over Word's dynamic
show/hide contextual tabs. Word additionally paints a **tinted band** above a contextual tab
group when active (visually separating "always there" tabs from "appeared because you're in a
table" tabs). *Optional, P3 polish*: a thin `--accent`-tinted 2px top border on the Table tab
only while it's enabled, echoing that convention without adopting show/hide. Not required —
disabled-vs-enabled state is already clear from the existing opacity treatment.

### 4.5 Group separators — keep as is

`.rgroup` hairline right-borders (`--line`) between ribbon groups already match Word's group
-divider convention exactly. No change.

### 4.6 Hover / pressed states — keep as is

`.fmt`/`.ribbon-tab`/`.rail-btn` hover (`--bg-2` tint) and pressed (`--accent-soft` fill +
`--accent-line` border) states are simpler than Fluent's press-depth shading but perfectly
legible and consistent across every control class. Not a gap worth closing — Fluent's extra
subtlety here is not something a document editor's users would notice or miss.

## 5. What NOT to change

- **Icon family (Material-derived silhouettes)** — no wholesale Fluent redraw; doc 63 already
  establishes OpenDoc's own brand identity, and the current set is internally consistent.
- **Accent color** — orange default, user-customizable presets (doc 63 §1); never adopt Word's
  blue as the default, ship it as a preset only if ever added.
- **System-font stack** — already matches Word's own approach (native OS font); do not add a
  bespoke web font chasing Segoe UI Variable.
- **Tabbed-ribbon structure and "disable, don't hide" contextual tabs** — already decided in
  doc 64; this doc does not revisit that decision.
- **Functional-only governing rule** (doc 63 §0) — no decorative QAT customization UI, no visible
  search box that doesn't open a real palette, no Word-style dialog-launcher arrows unless a
  group actually has a deeper dialog behind it.
- **8/7px rounded-surface radius language for buttons/controls** — distinct from Word's squarer
  Fluent 2 surfaces, and part of the graphite+orange brand; only the *popover-specific* radius
  drift in §4.3 is a bug, not the base token values.
- **Floating selection toolbar's deliberately trimmed command set** — already narrower than
  Word's busier mini-toolbar by design (doc 64 §6); do not pad it out to match Word's scope.

## 6. Open Questions (owner sign-off needed)

Token values, exact pixel sizes, and other implementation-time picks (the `--fs-*` scale in
§3.3, the 16/18/20px icon grid in §4.2, the `--radius-popover` value in §4.3, the Table-tab band
in §4.4) are **not** listed here — those are ordinary engineering judgment calls made when the PR
is actually built, the same way no prior P1G slice asked the owner to approve a specific hex code
or border-radius. What actually needs a call before anyone spends a PR on this:

1. **Is this the right time to spend a PR here at all?** Doc 67 (2026-07-29) still lists several
   **P0** gaps (IME live preedit, structural cross-boundary delete, rich clipboard fidelity) as
   the things that make a session "feel broken." Everything in this doc is P2/P3 chrome polish.
   Should any slice below be scheduled before those P0s close, or does this doc sit as reference
   only until the P0 list is clear?
2. **Of everything here, only two items are functional/discoverability gaps, not cosmetics**:
   Undo/Redo disappearing outside the Home tab (§1.4.1), and ⌘K having no visible entry point
   anywhere in the UI (§1.4.2). Worth pulling those two out as a small standalone slice regardless
   of #1's answer, since they're real "can't find/use a working feature" gaps rather than polish?
3. **Icon language direction, long-term**: is there any actual product interest in moving
   OpenDoc's iconography toward Fluent's outline/rounded language over time (a real multi-PR
   investment across every control), or should §2.5's "what it would take" answer be treated as
   permanently shelved — i.e., close the question instead of leaving it open for every future
   icon addition to re-litigate?

## 7. Verification / next steps

This doc is research and recommendations only — no code changed. If/when the owner greenlights
work from §6, split implementation the same way every other P1G shell slice has shipped (see the
tracker's Phase 1G table): each bullet below is one independently reviewable, functional-only PR,
gated the same as existing slices (`cargo +1.96.0 fmt`, `clippy --all-features --locked`, workspace
tests, and an in-browser Playwright/manual verification pass — no engine changes are needed for
any of these, they are `webapp/` CSS/HTML/JS only):

1. **Slice A — token consolidation (pure refactor, zero visual regression target):** introduce
   `--fs-*` (§3.3), `--h-*` sizing tokens (§4.1), `--radius-popover`/`--shadow-popover` (§4.3);
   remap every existing selector onto them. Verify via before/after screenshot diff across
   ribbon, footer, all four popovers/menus — pixel-identical except the two intentional
   `.settings-label`/`.menu-heading` → 10px caption fixes.
2. **Slice B — icon-size grid (§4.2):** collapse 15/17/18/20px → 16/18/20px. Screenshot-diff
   verification only; no markup change.
3. **Slice C — persistent Undo/Redo/Save cluster (§1.4.1):** real behavioral addition (visible
   regardless of active ribbon tab). Needs its own browser smoke: click Undo/Redo from every one
   of the four ribbon tabs, confirm always visible and wired to the existing handlers.
4. **Slice D — visible ⌘K trigger (§1.4.2):** add the button, wire to the existing `openCmd()`;
   verify it opens the same palette as the keyboard shortcut, no duplicate logic.
5. **Slice E (optional, P3) — Table-tab tinted band (§4.4).**

Each slice, once actually scheduled, gets its own `P1G-*` row in
`docs/14-EXECUTION-TRACKER.md`'s Phase 1G table (this doc's tracker row is the design-only entry
and should move from **Designing** to **Done** once this note is accepted, independent of whether
any implementation slice above is picked up).
