# 69 — Editor Shell Competitive Design Guide (Reference Prototype, Icons, Typography)

Status: accepted direction; requested shell, typography, and icon slices implemented
Date: 2026-07-30 (revised same day — re-grounded per owner feedback, see §0)
Depends on / extends: [doc 63](63-EDITOR-UI-UX-DESIGN-SYSTEM.md) (shell & design system, accepted
direction), [doc 64](64-EDITOR-TOOLBAR-RIBBON-DESIGN.md) (toolbar/ribbon competitive read), [doc
67](67-EDITOR-UX-GAP-ANALYSIS.md) (authoritative P0–P3 gap table), [doc 68](68-COMMENTS-AND-SUGGESTIONS-DESIGN.md)
(comments UX, referenced in §1)
Grounded in: `webapp/editor.html` (DOM/icons), `webapp/src/style.css` (tokens/components), and
`docs/assets/template-reference-prototype.html` — **the actual coded prototype that generated
`template.png`**, not a screenshot guess (found in the owner's Downloads as `Vellum.dc.html`,
copied into the repo here so it's a durable, checked-in reference alongside `template.png`).

## 0. Scope, and why this doc changed shape mid-review

This doc originally framed §1–§2 around Microsoft Word 2026 as the primary benchmark. Owner
feedback during review was direct: that framing produced weak, bikeshed-y questions, and the
thing that actually matters is `template.png` — the project's **own already-accepted** reference
(doc 63: "Status: accepted direction... Reference: template.png") — not an external competitor.
Word material below is now trimmed to a few sentences of context, kept only where it adds
something the reference itself doesn't cover (§1.3). Everything load-bearing is grounded in the
actual coded source behind `template.png`, not in reading pixels off a screenshot.

**One more correction worth stating up front**: the reference prototype is a rough, hand-built
visual mock (a small custom component — `DCLogic`/`x-dc`/`sc-for`/`sc-if` templating, editing via
raw `document.execCommand` on a `contentEditable` div) used to produce one screenshot, not a real
product. It gets the **visual/spatial** design right and is treated as authoritative for that.
It is **not** authoritative for behavior — several of its own buttons do nothing (§1.5, §2.4), and
its editing mechanism (`execCommand`) is the exact deprecated, browser-inconsistent approach
OpenDoc's model-backed WASM engine was deliberately architected to avoid (docs 58/59). Take the
spacing, icon choices, and layout from it; do not take "a button exists" as evidence a feature
should ship without real behavior behind it — that would violate doc 63 §0's functional-only rule,
which is already a **stricter** discipline than the reference itself follows.

## 1. Layout

### 1.1 The reference, precisely

`docs/assets/template-reference-prototype.html` renders at a fixed 1440×900 preview. Chrome font
is **Inter** (400/500/600/700). Icons are **Material Symbols Outlined** (§2.1).
Document content renders `'Calibri','Carlito',sans-serif` at `11pt` — the same Carlito-for-Calibri
substitution already noted as a deferred viewer-fidelity option in the P1G-010 tracker entry; this
is independent confirmation it's the right substitution to eventually wire up, not a new finding.
Accent color throughout is `#1a73e8` (blue) — per doc 63 §5 and this doc's own §5, OpenDoc keeps
its own orange, so exact reference hex values are not proposed for adoption anywhere below.

### 1.2 Exact layout spec, extracted from source (replaces screenshot estimates)

| Region | Reference (exact, from source) | OpenDoc today | Note |
|---|---|---|---|
| Title bar | flex row, `padding:7px 14px 5px`; 26×26 logo (`rx:9`); doc-title input `font-size:15px/500`; saved-indicator `12px` text + 16px icon | 44px `.bar`, doc-title `13px/550` | Comparable height/density; OpenDoc's is a touch larger overall (44px total vs ~top-bar content ~34px), reasonable given OpenDoc's bar also hosts Open/Save/Settings buttons the reference splits into a separate row. |
| Menu row (File/Edit/View/Insert/Format/Tools/Help) | `font-size:13px`, `padding:2px 8px`, `border-radius:4px`, directly under the title row | absent | Still a real gap (doc 64 §4 item 6 already defers this); now has an exact micro-spec if/when built. |
| **⌘K search trigger** | a real, visible button: `height:34px; border:1px solid #dadce0; border-radius:8px; background:#fff`, icon (`search`, 17px) + "Search" label (13px) + kbd chip (`⌘K`, 10.5px/600, bg `#f1f3f4`, radius 4px, padding `1px 5px`) — sits in the title bar, right side, before the avatar stack | Implemented as `#searchTrigger`, opening the existing command palette with ARIA state and focus restoration | Closed. The trigger collapses to its icon at narrower widths without introducing page overflow. |
| Ribbon container | `background:#fff; border:1px solid #dadce0; border-radius:9px; padding:6px 4px; min-height:76px` | `.ribbon` is an inset white card with a 1px border and 9px radius | Aligned; OpenDoc retains its own orange accent and working tab set. |
| Ribbon tabs | `font-size:13.5px; padding:7px 13px 8px`; active = accent color + `font-weight:600` + `border-bottom:2.5px solid` accent | `.ribbon-tab` uses 13.5px text and the existing accent underline | Aligned. |
| Ribbon groups | vertical stack (icon row(s) + bottom-aligned caption), `gap:1px` between buttons in a row, caption `font-size:10.5px; color:#80868b`, **sentence case, not uppercase** (e.g. "Clipboard", "Font", "Paragraph") | `.rgroup-label` uses 10.5px/500 sentence-case captions | Aligned. Labels describe the commands actually present; Undo/Redo is not mislabeled “Clipboard.” |
| Group divider | a 1px `background:#e8eaed` spacer div, `margin:2px 4px` | `.rgroup` right-`border:1px solid var(--line)` | Same visual effect (hairline vertical rule between groups), different implementation (spacer div vs border). No functional difference; no change needed. |
| Nav rail | container `width:56px`; each button `width:46px; padding:7px 0; border-radius:9px`; icon 20px; **9px text label under the icon** (e.g. "Outline", "Pages", "Search", "Comments", "Marks") | The functional Outline destination has a 20px icon and visible 9px caption on the neutral rail surface | Closed for the currently implemented destination; future destinations remain gated on real behavior. |
| Rail panel | `width:236px`; header `padding:13px 14px 10px`, title `15px/600`, close icon 18px; a built-in **search-this-document** box (distinct from the ⌘K command palette) when the "Search" rail item is active | `#outlinePanel` — similar header pattern, no in-panel document-search mode | The reference's rail unifies "find things in this doc" (headings via Outline, text via Search) in one panel; OpenDoc's ⌘F find is a separate floating panel instead. Both are reasonable structures — flagging as a design choice to make consciously if/when a rail "Search" destination is ever added, not as an error. |
| Context panel | `width:314px`; 3 tabs (Comments/Properties/Styles), tab style `13.5px`, active `font-weight:600` + `border-bottom:2.5px` accent (same pattern as ribbon tabs) | **does not exist** — no right `<aside>` in `editor.html` | Still 0% built (doc 63 §2 region 4); this is the single largest unbuilt region, now with an exact width/tab spec ready for whenever it's scheduled (doc 68 already specs the Comments tab's content in detail). |
| Floating selection toolbar | dark pill: `background:#202124; border-radius:9px; padding:4px 6px`, buttons `height:30px`, icons 16px on `#e8eaed`; B/I/U + divider + color/highlight + divider + link/comment | `.sel-toolbar` now uses the dark-pill treatment for its real B/I/U/S, color, and highlight controls | Visual treatment aligned; unimplemented link/comment commands were not added. |
| Status bar | `height:30px; padding:0 14px; font-size:12px`; page icon+label, **words**, **characters** (not paragraphs), language dropdown, a mode chip (Editing/Viewing), a panel-toggle icon, zoom `−`/`%`/`+` (22×22 buttons, 4px radius), divider, "Synced" (`cloud_done`, green) | `.footer` is 30px/12px on the neutral canvas; words · paragraphs (left), Page X of Y · zoom (right) | Spatial treatment aligned. Language/mode/sync remain correctly absent because those states are not yet modeled. |
| Find bar | floating card, `top:150px; right:34px; border-radius:10px`, icon+input(13px)+match-count+prev/next+close | `.find-panel` — floating card, `top:84px; right:18px; border-radius:var(--radius)` (8px) | Same anchoring pattern (top-right floating card) — already aligned structurally; only the exact offset/radius differ, not worth chasing. |
| Command palette | **centered-top modal**: `position:fixed;inset:0` backdrop, panel `width:560px; border-radius:14px`, top `padding-top:120px`, icon 20px + input 15px + "ESC" chip | `#cmdPalette` — centered modal overlay | **Correction to this doc's earlier screenshot-based read**: the annotated `template.png` crop made the palette look bottom-right-docked; the actual source shows it's a centered-top modal with a dimmed backdrop — i.e. essentially the **same** pattern OpenDoc already uses. Not a gap; the earlier note in this doc that these "differ" was wrong and is corrected here. |

### 1.3 Word 2026, briefly, as secondary context

Kept short deliberately (see §0). Two Word conventions are worth knowing even though the reference
doesn't show them: a **Quick Access Toolbar** (Undo/Redo/Save, always visible regardless of active
ribbon tab), and a title-bar **Search** box that replaced the old ribbon-embedded "Tell Me." The
reference's own title-bar "Search ⌘K" button (§1.2) is effectively the same idea as Word's Search
box — good independent validation that a visible command-search entry point is the right pattern,
from two different sources now, not just Word. The QAT idea is treated in §1.4 below.

### 1.4 Concrete gaps and recommendations

1. **⌘K visible entry point — closed.** The implemented 34px Search control opens the same real
   command palette as the keyboard shortcut and exposes/restores focus correctly.
2. **Persistent Undo/Redo (Word's QAT idea) — optional, not reference-backed.** The reference
   doesn't show a persistent Undo/Redo cluster either (Undo/Redo live inside its Home-tab ribbon,
   same as OpenDoc today). This is a Word-only convention; do it only if the owner independently
   wants it, not because either the reference or a competitive-parity argument requires it.
3. **Ribbon-as-card — closed.** The working ribbon is now a bounded 9px-radius surface, inset from
   the viewport without changing command behavior.
4. **Rail item captions — closed for implemented destinations.** Outline carries a visible
   caption; no empty Pages/Search/Comments/Bookmarks controls were introduced.
5. **Dark floating-selection-toolbar pill — closed.** The reference treatment was adopted around
   the existing functional formatting controls only.

### 1.5 What's broken in the reference itself — do not copy as-is

Reading the actual component logic (not just the rendered markup) turns up real, owner-relevant
bugs and dead controls in the mock. None of these should be treated as "the reference does this,
so we should too":

- **Format Painter is a no-op** (`paint:()=>{}`) — the icon and button exist, clicking does
  nothing.
- **"Checklist" is mislabeled** — its handler calls the same command as the Bulleted List button
  (`insertUnorderedList`); it is not a real task-list/checkbox feature, just a second bullet
  button with a different icon and label.
- **The entire Draw tab is dead** — all five tools (Pen, Highlighter, Eraser, Lasso, Shapes) are
  empty handlers.
- **Most of Layout, References, and Review tabs are dead** — Margins, Orientation, Spacing,
  Footnote, Citation, Caption, Spelling, Track Changes, and Language all render a real button with
  a real icon and do nothing when clicked. Only a handful of items per tab (Contents→outline jump,
  Comment, the View tab's zoom/panel/mode toggles) are actually wired up.
- **The underlying editing mechanism is `document.execCommand` on a `contentEditable` div** — a
  deprecated API with well-known cross-browser inconsistencies (this is precisely the class of
  problem docs 58/59 designed OpenDoc's model-as-source-of-truth WASM engine to avoid). OpenDoc's
  engine is already better-architected than the mock's; there is nothing to "catch up" to here.

**Implication**: doc 63 §0's functional-only rule ("no placeholder buttons ever ship") is not just
consistent with the reference — it is a **stricter, better discipline than the reference itself
practices**. When pulling ideas from this file for a future tab (Draw, Layout, References, Review),
treat each icon/button as a candidate for a real feature to design and build, never as something
to wire up as a decoration because "the reference has it."

## 2. Icons

### 2.1 The reference's actual icon system (now known precisely, not guessed)

The source imports Google Fonts: `family=Material+Symbols+Outlined:opsz,wght,FILL,GRAD@20..48,400,0,0`
— i.e. **Material Symbols Outlined**, Google's modern variable-font icon family (successor to the
classic static "Material Icons"), with the `FILL` axis at `0` (outline) by default. Icons are
ligature text nodes: `<span class="ms">search</span>`, `<span class="ms">undo</span>`, etc. — the
glyph is selected by the literal command name, not an inline SVG path. This is a materially
different, more precise finding than this doc's earlier "looks Fluent-ish" guess from the
screenshot: it's Google's own family, one generation newer than what OpenDoc currently inlines —
**not** a jump to Microsoft's Fluent 2 design language at all.

Rendered sizes in the source are **not** on a strict grid either: 15px (status-bar page icon),
16px (dropdown carets, editing-group text-row icons, status/zoom icons), 17px (title-bar search
icon), 18px (most ribbon icons), 19px (Undo/Redo specifically), 20px (nav-rail icons, palette
search icon, panel-close icons), 24px (other-tab tile icons), 34px (empty-state icon). That's
**8 distinct sizes** in the reference itself — as many as OpenDoc's own type scale in §3, and more
than OpenDoc's current 4 icon-render sizes. The takeaway: neither the reference nor Word actually
enforces a strict grid in practice; §4.2's recommendation to consolidate OpenDoc's own icon sizes
stands on its own internal-consistency merits, not because it would match either external source
exactly.

### 2.2 Icon-by-icon migration inventory

| Command | Reference (Material Symbols Outlined name) | Pre-migration OpenDoc state |
|---|---|---|
| Undo / Redo | `undo` / `redo` | custom filled curved-arrow paths |
| Format painter | `format_paint` | not built |
| Clear formatting | `format_clear` | not built |
| Bold/Italic/Underline/Strike | literal "B"/"I"/"U"/"S" glyphs (no icon) | same — literal glyphs, matches |
| Text color | `format_color_text` (+ a color swatch bar under it) | custom "A" + swatch, same concept |
| Highlight | `ink_highlighter` | custom highlighter swatch |
| Link | `link` | custom link-chain path |
| Comment | `add_comment` | not built (doc 68 in progress) |
| Insert image | `image` | not built |
| Horizontal rule | `horizontal_rule` | not built |
| Bulleted / numbered list | `format_list_bulleted` / `format_list_numbered` | custom filled dot/number paths |
| Checklist | `checklist` (**but mislabeled — see §1.5**) | not built |
| Indent decrease/increase | `format_indent_decrease` / `format_indent_increase` | custom filled arrow-bar paths |
| Align L/C/R/Justify | `format_align_left/center/right/justify` | custom filled bar paths |
| Quote | `format_quote` | not built |
| Find / Replace | `search` / `find_replace` | custom filled magnifying-glass path (Replace not built) |
| Insert table | `table_chart` | custom filled grid path |
| Save / Saved state | `cloud_done` / `cloud_sync` (title bar) | custom filled floppy-disk path (Save is a standalone button, not a status chip) |
| Zoom in/out | `add` / `remove` | custom filled +/− path |
| Outline / TOC | `toc` | custom filled outline-lines path |
| Bookmarks | `bookmark_border` | not built |
| Comments (rail) | `forum` | not built |
| Settings/panel toggle | `vertical_split` | gear icon (`settings`-style path), different concept (Settings popover, not a panel toggle) |
| Close (panel/menu) | `close` | custom filled X path |
| Resolve (comment) | `check_circle` | not built |
| Dropdown caret | `arrow_drop_down` | custom SVG chevron background-image on `<select>` |

### 2.3 Adopted icon system

The owner explicitly extended the reference request to “fonts and icons and others,” resolving
the earlier open direction. OpenDoc now uses **Material Symbols Outlined** for its built chrome
commands, without changing the functional-only rule:

1. **Self-host the font; do not runtime-fetch it.** The reference's own `<link
   href="https://fonts.googleapis.com/...">` is fine for a one-off mock but wrong for OpenDoc's
   local-first posture (`editor.html`'s own copy: "rendered entirely in your browser... Nothing is
   uploaded"). The Apache-2.0 font and license text are checked in under
   `webapp/assets/fonts/`; `webapp/src/fonts.css` provides it locally to both routes.
2. **Map only existing commands to exact ligature names.** Every built toolbar/header/rail/footer
   icon is migrated. Bold/Italic/Underline/Strike stay as literal glyphs, matching the reference.
   The `<select>` caret remains a tiny CSS background asset rather than introducing a custom DOM
   control solely to replace native select behavior.
3. **Keep active state semantic and visible.** Existing `aria-pressed` state plus accent
   background/border remains the primary signal. The font is configured with `FILL=0`; a filled
   variant can be added later where it carries real state rather than decoration.
4. **Test the local boundary.** Unit coverage verifies the font binaries, license files, local CSS
   URLs, and route links; Playwright verifies both faces load and no Google Fonts request occurs.

### 2.4 What's broken/misleading in the reference's own icon usage — do not copy the bug

Beyond the dead controls already listed in §1.5: the **Checklist** icon/label pairing is real UI
debt in the source itself (a checklist icon wired to a plain-bullet command). If/when OpenDoc ever
builds a real checklist/task-list feature, build the actual behavior (checkbox state per list
item, distinct from bullets) — do not use the reference as evidence that "checklist = bullets with
a different icon" is an acceptable shortcut.

## 3. Typography

### 3.1 The reference's own type scale (a second data point, not just Word)

Chrome font is **Inter** at: 9px (rail label), 10.5px (group captions, kbd hints, comment
timestamps), 12px (status bar, menu row items, doc-property labels), 12.5px (comment body,
ribbon-dropdown option text), 13px (menu/font-dropdown buttons), 13.5px (ribbon tabs, context-panel
tabs, command-palette results), 15px (doc-title input, command-palette search input, panel
titles). That is **7 distinct sizes** — essentially the same order of granularity as OpenDoc's own
8 (§3.2), and meaningfully more steps than Word's ~3. **This softens the original framing**: the
recommendation below is not "shrink toward Word's leaner scale" — it's "make OpenDoc's own steps
individually consistent," full stop, since neither external reference actually runs a leaner scale
in practice.

### 3.2 OpenDoc's current type scale — as actually used

`webapp/src/style.css` now uses self-hosted `Inter` first, followed by the native system-UI
fallback stack, at a `13px/1.5` body base.

| Size | Where used | Weight(s) used there |
|---|---|---|
| 9px | ruler tick numbers, tab-stop glyph | 400 (numbers), 800 (tab glyph) |
| 10–10.5px | `.rgroup-label` (sentence-case ribbon captions), `.testing` badge, tab-corner glyph | 500–700 |
| 11px | `.settings-label`, `.menu-heading` (popover section captions), `.rgroup-hint`, `.oss` | 700 (labels), 400 (hint/oss) |
| 12px | footer/status bar, cmd-list group/hint text, find-status, zoom select | 400–550 |
| 13px | body base, `.btn`, `.doc-title`, `.ribbon-tab`, `.menu-item`, `.cmd-item`, find-panel inputs | 550 |
| 14px | `.ctl.swatch` "A" glyph | 700 |
| 15px | `.brand .mark`, `.cmd-search input` | 600–700 |
| 16/18/20px | compact, standard-control, and rail Material Symbols (icon scale, not text) | 400 |

The concrete inconsistency worth fixing isn't the step count (§3.1 shows the reference has a
similarly dense scale) — it's that **conceptually identical elements use different values**:
Ribbon group captions deliberately follow the reference at 10.5px/500 sentence case, while
`.settings-label` and `.menu-heading` remain the stronger uppercase “eyebrow” pattern. They are
different roles now rather than accidental variants of one role.

### 3.3 Recommendation — consolidate to a named scale, don't shrink further

```
--fs-micro:   9px   /* ruler ticks, tab-stop glyphs — leave as-is, already dense */
--fs-caption: 10px  /* ALL uppercase "eyebrow" labels: rgroup-label, settings-label,
                        menu-heading, .oss — currently split across 10/11px, pick one */
--fs-small:   12px  /* status bar, cmd-list secondary text, find-status, zoom % */
--fs-body:    13px  /* default chrome: buttons, menu items, ribbon tabs, inputs — the
                        existing body base, unchanged */
--fs-medium:  15px  /* command palette search input, brand wordmark */
```

Collapses 8 raw values to 5 named ones; moves `.settings-label`/`.menu-heading` onto the same
`--fs-caption` as `.rgroup-label`. Weight collapses toward 2–3 steps (600 = default UI emphasis,
700 = caption/label emphasis); `.btn`'s 550, `.acc`'s 650, and the tab-glyph's 800 are minor
outliers with no clear rationale and fold into 600/700 without visible regression.

### 3.4 Adopt Inter without adding a network dependency

The owner explicitly selected the reference's font direction. Inter is therefore self-hosted as
a bounded Latin WOFF2 and loaded by both site routes through `webapp/src/fonts.css`, with the
system stack retained as fallback. Its SIL OFL 1.1 text is checked in beside the binary. This adds
cross-platform visual consistency without copying the reference's runtime Google Fonts request
or affecting the separate document-content font pipeline.

## 4. Design guide additions

### 4.1 Spacing / sizing grid

OpenDoc's shell uses named control-height tokens: 30px (`--h-control`), 34px
(`--h-search`/`--h-tab`), 44px (`--h-rail-control`/`--h-header`), and the reference-aligned 30px
`--h-footer`. Equivalent roles no longer repeat unrelated bare height literals.

### 4.2 Icon grid

Material Symbols render on three named steps: **16px** (`--icon-compact`), **18px**
(`--icon-control`), and **20px** (`--icon-rail`). This deliberately simplifies the reference's
eight icon sizes while retaining its outlined visual language.

### 4.3 Elevation / radius conventions for floating surfaces

`--radius` (8px) and `--radius-sm` (7px) exist but drift on floating surfaces: `.settings-panel`
(12px), `.context-menu` (10px), `.cmd-box` (12px), `.find-panel` (correctly `var(--radius)`, 8px).
For context: the reference itself is **not** more disciplined here either — its own radii span
6px (buttons), 8px (dropdown menus), 9px (ribbon container, rail buttons), 10px (find bar), 11px
(comment cards), 14px (command palette) — six different values. Neither external source justifies
chasing a specific number; *recommend* OpenDoc pick **its own** clean `--radius-popover` (10px,
splitting its own current 8/10/12 spread) and one `--shadow-popover` token, purely for internal
consistency. Pure refactor, no functional change.

### 4.4 Contextual tab styling — no change recommended

OpenDoc's Table tab (present, `disabled` outside a table) already matches doc 64's "disable,
don't hide" decision. The reference doesn't have contextual tabs at all (all 7 of its tabs are
always present); no new evidence here to act on.

### 4.5 Group separators — keep as is

Hairline dividers between ribbon groups match the reference's own spacer-div pattern in effect
(§1.2). No change.

### 4.6 Hover / pressed states — keep as is

`.fmt`/`.ribbon-tab`/`.rail-btn` hover/pressed states are simple, legible, and consistent. No
comparable evidence from the reference (its own `tb()`/`railBtn()` helpers use a similarly simple
background-swap pattern — `#e8f0fe` active bg, no elaborate depth effect) suggests otherwise.

## 5. What NOT to change

- **Icon family** — the requested Material Symbols migration is complete; do not mix in Fluent,
  ad hoc third-party sets, or decorative icons for unimplemented commands.
- **Accent color** — orange default, user-customizable presets (doc 63 §1); the reference's blue
  (`#1a73e8`) and Word's blue are both external, not adopted.
- **Font-loading boundary** — Inter and Material Symbols are local assets with checked-in
  licenses. Do not replace them with runtime Google Fonts links.
- **Tabbed-ribbon structure and "disable, don't hide" contextual tabs** — already decided in doc
  64; not revisited here.
- **Functional-only governing rule** (doc 63 §0) — reinforced, not weakened, by this doc: §1.5/§2.4
  show the reference itself doesn't reliably follow this discipline. Do not treat "the reference
  has this button" as license to ship a dead one.
- **`document.execCommand`/`contentEditable` as an editing mechanism** — never adopt this; it's
  what the reference uses and it's the exact failure mode OpenDoc's WASM/model engine avoids.
- **Floating selection toolbar's trimmed command set** — narrower than the reference by design
  (doc 64 §6); the dark-pill styling does not authorize dead Link/Comment actions.

## 6. Remaining product questions

The owner request resolves the font, icon, rounded-ribbon, group-caption, status-surface, and dark
selection-toolbar direction. The remaining question is scope rather than styling: the menu row,
Clipboard commands, Styles gallery, right context panel, additional rail destinations, and richer
status states each require real underlying behavior and their own design/build slice. They remain
deferred per doc 63 §0/§6 instead of being copied as inert reference chrome.

## 7. Implementation status / next steps

The interrupted frontend pass was recovered and completed on 2026-07-30. It remains
functional-only and changes no engine or document-model behavior:

1. **Slice A — done:** the 34px visible Search/⌘K control opens the existing real command
   palette, exposes dialog state through ARIA, and restores focus on close.
2. **Slice B — done:** named typography, control-height, popover-radius, and popover-shadow
   tokens replace equivalent one-off shell values.
3. **Slice C — done:** chrome icons use the 16/18/20px compact/control/rail grid.
4. **Slice D — done:** Inter and Material Symbols Outlined are self-hosted with license texts;
   both routes load the local faces and every built chrome icon is migrated.
5. **Slice E — done:** the working floating selection toolbar uses the reference's dark pill.
6. **Slice F — done:** the functional Outline rail destination has a visible 9px caption.
7. **Slices G+ (each its own doc-63-§6-style phase)** — menu row, Clipboard group, Styles-as-gallery,
   the right context panel, remaining rail destinations, richer status bar. Each needs its own
   design note before a build PR, per doc 63 §6's existing convention; not re-planned here.

The completion pass also removes the accidental "Clipboard" label from an Undo/Redo-only group,
keeps the no-document header limited to useful actions, and bounds the header at 390px/720px so
the new Search control cannot create page-level horizontal overflow. Permanent Playwright
coverage lives in `webapp/tests/e2e/shell-reference-polish.spec.mjs`. Verification: `webapp/build.sh`,
`npm run test:unit` (15 passed), `npm run test:e2e` (29 passed), `node --check
webapp/src/main.js`, and `git diff --check`.
