# 123 — The ribbon chrome and the File page: the visual contract

**Status:** Implemented. **Opened:** 2026-09-24. **Closes:** `109` UX-016 (two mode
controls for one state). **Adds:** `109` UX-025 (the mode control is on a surface no
reference uses), `109` UX-026 (the File page's pane only ever shows Info).

## 0. Why this document exists

[Doc 105](105-DUAL-CHROME-DESIGN.md) settled the fidelity question — *"similar to
OnlyOffice or LibreOffice… not exact copy"* — and then specified **structure only**:
which tabs exist, which groups they hold, what the File surface contains. [Doc
122](122-ONE-AXIS-NAVIGATION-DESIGN.md) did the same for navigation, and did it from
ONLYOFFICE source.

Neither says how any of it should **look**. Band height, control height, icon size,
group padding, whether a group carries a caption, whether a control carries a label,
how the File page is laid out — none of that was ever designed. It was assembled, and
every pull request verified it with assertions rather than by opening the editor.

The result, in the owner's words:

> "UI is fukced up .. for ribbon.. whole UX.. do you have have desing and comepedetivve
> anlaysis.. when even you have a whole code base"

> "move these file, home layout.... where in compact view .. file, view and other lies"

> "also ceck comple file ui.. etf is that .. are you kidding me"

> "do a UX research and dont fuck it up.. you have whole code base and internet access"

This note is the missing half: the numbers, the rules, and the source for each. It is
about the **ribbon chrome only**. The compact chrome was measured against Google Docs in
#591 and is out of scope; the structure settled in 122 (`109` UX-014) is out of scope.

---

## 1. Sources, and how far each can be trusted

| Source | What it gives | Confidence |
| --- | --- | --- |
| **ONLYOFFICE `web-apps` checked out locally** at `/Users/sachin/Desktop/melp/reference/web-apps` | LESS variables, templates, view controllers | **First-hand, source-verified.** AGPL-3.0: measurements and behaviour only, **no code taken** |
| **ONLYOFFICE Docs 9.4.1 running** in the vendor's own Playground (`api.docs.onlyoffice.com/9.4.1-e9f43897e5cfcbabaf9c2dac6f595fee/…`) | the shipped, compiled `app.css` with every LESS variable resolved; screenshots of the Home band and the File page | **First-hand, measured** |
| **Google Docs**, a live document at 1408×723 | element rects and computed styles read with JavaScript in the page | **First-hand, measured** |
| **Microsoft Word** | group captions, the Quick Access Toolbar, the backstage's back arrow | **Not first-hand.** Word for the web needs an account this session does not have. Only claims that hold across every published screenshot of the product are used, and no Word pixel value is quoted anywhere below |

Two of my own earlier statements were wrong and are corrected here:

- ONLYOFFICE does **not** always put the tab strip on the title row. They have two
  header layouts and ship both (§3).
- The dead space at the right of a ribbon band is **not** a defect. Both references
  have it (§4.6).

---

## 2. The measured comparison

Everything in the ONLYOFFICE column is first-hand from the checkout or the running
9.4.1 build; the file and line are given so each can be re-read. Everything in the
Google Docs column is first-hand from a live document. Blank means the product has no
such thing.

| | **ONLYOFFICE 9.4.1** | **Google Docs** | **opendoc, before** | **opendoc, after** |
| --- | --- | --- | --- | --- |
| Title row | 28px (`variables.less:103` `@document-title-height`) | 24px (measured: input top 10, h 24) | 27.5px | 24px |
| Navigation row | tab strip **28px** when it shares the header block, **32px** standalone (`variables.less:104-105`) | menu bar 23.5px inside a 33px block, **in the header under the title** | tab strip 34px, **its own row** | tab strip **28px, in the header under the title** |
| Header block total | 28 + 28 = **56px** (two-level) or **32px** (one-level, tabs beside the title) | **~59px** | 58 + 34 = **92px** | **58px** |
| Band height | **66px** (`colors-table.less:529` `@toolbar-height-controls`) | 40px toolbar | 90.5px measured / 92px token | **74px** |
| Group box inside the band | **52px** (`@toolbar-group-height`), 7px above and below | — | 74px (2 control rows + a caption row) | 63px |
| Group captions | **none.** `.group` is a bare `display: table-cell` with 6px of left padding (`toolbar.less:498`); `Toolbar.template` has no caption element | none | an uppercase 9px caption under **every** group | **none** (kept in the DOM, visually hidden) |
| Group separation | 6px padding + a 1px `.separator` with 6px margin — 12px of air (`toolbar.less:498,545`; `separator.less:8`) | 4px gaps, no rules | 2px a side + a `--line` hairline | 6px a side + a `--line-strong` hairline |
| Small button | **20×20**, icon 20px, radius 1px (`@x-small-btn-size`) | 30×30, icon 20px, 1px margin | 26–30px | unchanged (30px; see §4.4) |
| Tall captioned button | 52px tall, min-width 35px, **icon 28px**, caption 11px in a 24px box (`buttons.less` `.btn.icon-top.x-huge`) | — | 44px min-width, icon 22px, caption 10.5px | unchanged |
| Which controls get a caption | **only Insert / Layout / Draw objects.** All 21 `x-huge icon-top` buttons in `Toolbar.js` are inserts, page setup, or the draw tools. **Home has none** | none | Paste, Cut, Copy, Find, Replace | **Paste only** (§4.3) |
| Stacked rows in one group | `.elset`, 20px tall, 8px between rows (`toolbar.less:530`) | — | ad hoc | `.stackrows`, same idea |
| Tab side padding | 12px (`toolbar.less` `li > a`) | 7px | 14px | 12px |
| Tab font | 12px | 14px | 13.5px | 13.5px (our token) |
| Mode selector | top right of the **tab row** | right end of the **toolbar** | **in the band AND in the status bar** | status bar only (§4.5) |
| Fold control | — (a "Hide toolbar" item) | `^` at the toolbar's right end | right end of the tab row | right end of the nav row |
| **Chrome above the page** | **~179px** measured in the running editor (title + tabs + band + ruler) | **145px** (page top at 1408px) | **228.8px** | **178px** |
| File / backstage page | full window below the tab strip, `bottom: 0` (`toolbar.less:940`); two columns, never empty | a **dropdown**, not a page | an overlay that did not cover the work area, one centred 720px column | full window below the header; two columns |
| File page nav column | **260px**, `@background-pane`, 1px right border (`#file-menu-panel .panel-menu`) | menu 322px wide | a 720px block centred, 344px of dead left margin | 320px column at the left gutter |
| File page row | **28px tall, 4px below → 32px pitch**, 12px type (`li.fm-btn`) | 32px pitch, 14px type | 34px tall, 36px pitch | 28px tall, **32px pitch**, 13px type |
| "Back" | **first row, top left**, `←` + label (`FileMenu.template` `#fm-btn-return`) | — | a button at the top **right** | **first row, top left** |

### 2.1 What is deliberately not adopted

- **ONLYOFFICE's 20px buttons and 52px group.** Ours stay at 30px. A 20px control fails
  WCAG 2.2 SC 2.5.8 (Target Size (Minimum), 24×24 CSS px), and this repo's UI floor
  (SKILL.md §10) is touch-usable. This is why our band is 74px where theirs is 66px: the
  8px is the accessibility margin, not slack.
- **ONLYOFFICE's 11–12px type.** This stylesheet has one type scale (`--fs-body: 13px`,
  `--fs-caption: 10px`). A second body size for one surface is a fork, not a decision.
- **ONLYOFFICE's always-visible static group** (Save/Print/Copy/Paste/Undo/Redo left of
  every tab's panel). Still not adopted — doc 122 §5.3 already recorded that, and it is a
  structural change, not a visual one.

---

## 3. The navigation row moves into the header

**The rule.** *The chrome's one navigation axis is always in the same place: the header,
directly under the document title.* Compact chrome puts its menu bar there. Ribbon chrome
now puts its tab strip there. A person switching chromes finds the navigation in the same
place either way.

**Why this is not an invention.** ONLYOFFICE ships exactly two header layouts and the
choice is one config flag, `DocumentEditor/apps/documenteditor/main/app/controller/Viewport.js:197`:

- `config.twoLevelHeader && !config.compactHeader` → the title gets a 28px row of its own
  and the tab strip is 28px beneath it;
- otherwise the logo, the quick-access buttons, the document title **and the tab strip
  share one 32px row** — `Toolbar.template` puts `.extra left`, the tab list and
  `.extra right` inside a single `.box-tabs`.

Either way, ONLYOFFICE spends **56px or 32px** on title-plus-navigation. We were spending
**92px**. Google Docs spends ~59px on title-plus-menu-bar, and its menu bar is measurably
*inside* the header block, left-aligned with the title (title input top 10 h 24; menu bar
top 26 h 33; first menu item top 34 h 23.5).

So the owner's instruction lands on the arrangement both references use, and on the one
our own compact chrome already shipped.

**The numbers.** `3 + 24 + 28 + 3 = 58`. The header keeps its 58px token; the title row
takes Docs' 24px; the tab strip takes ONLYOFFICE's 28px (`@toolbar-height-tabs-top-title`);
the header's vertical padding goes from 5/4 to 3/3. These are scoped to
`body.ribbon-mode`, so the compact header is byte-for-byte what #591 measured.

**Overflow.** The strip is `overflow-x: auto` with the scrollbar suppressed and the same
trailing fade `.app-menu-bar` uses, because the 58px header is fully committed and a
visible scrollbar would come out of the strip's own height (`109` HF-097 is the defect
where a clipped name had no cue at all). Measured: every tab fits down to **1024px**;
below that the strip scrolls and the tabs stay reachable by scroll, by `←`/`→` on the
strip, and by the command palette.

**The fold chevron** is now a sibling of the tab list rather than a child of it. `role="tablist"`
may only contain tabs, and a control inside a scrolling strip scrolls away with it.

---

## 4. The band

### 4.1 No group captions

`ONLYOFFICE/web-apps`, `apps/common/main/resources/less/toolbar.less:498`:

> `.group { position: relative; display: table-cell; vertical-align: middle; white-space: nowrap; .padding-left-6(); font-size: 0; }`

That is the whole rule. There is no caption element in `Toolbar.template` and no caption
class anywhere in their band. Grouping is carried by the `.separator` vertical rules and
the 6/10px padding.

Word **does** caption its groups. The owner asked for ONLYOFFICE, twice, by name. The
caption row cost 13.5px of text plus a 3px gap out of a band; deleting it is the single
largest saving here.

**The text stays in the DOM, visually hidden.** It is the group's accessible name
(doc 105 P3 asks for exactly that), and it is what the `⋯` overflow menu prints as its
section heading — where there is room for it, and where Word shows the same words.
`#ribbonOverflowMenu .rgroup-label` un-hides it there.

### 4.2 Group separation

6px of padding a side and a 1px rule, which is ONLYOFFICE's `.group` padding plus their
`.separator`'s 6px margin. The rule moved from `--line` to `--line-strong` because with
the caption gone it is the only thing left saying where a group ends.

### 4.3 The labelling rule — when a control carries a visible label

> **A control carries a visible label only when it is the headline action of its tab.
> Everywhere else the icon carries it and the name lives in the tooltip, the menu and
> the palette.**

Derived, not invented. Every one of the 21 `cls: 'btn-toolbar x-huge icon-top'` buttons
in ONLYOFFICE's `apps/documenteditor/main/app/view/Toolbar.js` is an Insert, Layout or
Draw command — `inserttable`, `insertchart`, `inserttext`, `insertshape`, `insertsymbol`,
`insertequation`, `columns`, `pageorient`, `pagemargins`, `pagesize`, `selecttool`,
`handtool`, and so on. **Their Home tab has none at all.** Word adds exactly one to Home:
the big Paste.

Applied:

| Control | Before | After | Source |
| --- | --- | --- | --- |
| Paste | icon + label | **icon + label** | Word's Home ▸ Clipboard |
| Cut, Copy | icon + label | small, icon-only, stacked beside Paste | Word's Clipboard; ONLYOFFICE's two-`.elset` clipboard group |
| Find, Replace | icon + label | small, icon-only, stacked | ONLYOFFICE's Home has no captioned control |
| Insert ▸ Table, Picture, Shapes, Text box | icon + label | unchanged | these are the Insert tab's headline objects — exactly ONLYOFFICE's `x-huge` set |

### 4.4 Control height stays at 30px

See §2.1. This is the one row where we knowingly differ from ONLYOFFICE by a measurable
amount, and the reason is WCAG 2.2 SC 2.5.8.

### 4.5 The duplicated mode control

`Edit / Suggest / Read only` was a group in the Home band **and** `Editing / Suggesting /
Read only` in the status bar — the same three-state control twice on one screen. The band
copy is deleted. That is 159.5px of the Home band's width budget returned and one fewer
answer to "which of these is the real one".

**This is a partial fix and the remainder is recorded.** All three references put the
mode selector in the **top** chrome — ONLYOFFICE at the right of the tab row, Google Docs
at the right of the toolbar, Word at the right of the ribbon — and **none** puts it in a
status bar. Ours stays in the status bar because that is where the compact chrome's copy
lives, and moving it is a change to the compact chrome, which is out of scope here.
Recorded as `109` UX-025.

### 4.6 The empty right end of the band is not a defect

ONLYOFFICE's band ends where its groups end and reserves the right edge for the `More`
overflow (`.more-box { position: absolute; right: 0 }`); Word does the same with its
collapse caret. Our `#ribbonOverflowBtn` already sits there.

What is true is that our Home band is **emptier than theirs**, and the reason is
recorded elsewhere rather than accidental: doc 114 replaced the styles **gallery** with a
single 227px trigger, and doc 105 §3 measures our Home coverage at roughly 30 of Word's
38 controls. Measured at 1280×800 with a document open:

| | Groups sum | Panel client | Slack | `⋯` |
| --- | ---: | ---: | ---: | --- |
| Home, before | 1253.6 | 1272 | **18.4** | hidden |
| Home, after | 1003.0 | 1272 | **269.0** | hidden |

The 1280px budget (`109`, doc 115) was single-digit-tight and is now not tight at all.
Doc 122 §6 deferred `edit.selectAll` and `edit.pasteText` from Home ▸ Editing *explicitly
because there were only 18px to spare*; that reason has expired, and the two deferrals
should be revisited as their own change rather than folded into a visual pass.

---

## 5. The File page

### 5.1 It covers the work area — this was a bug, not only a gap

The page anchored to `--chrome-bottom`, which is where the chrome ends **when a band is
showing**. On the File tab there is no band: the page replaces it. So the page began 74px
below the chrome's real bottom edge, and the ruler, the nav rail and the top of the sheet
stayed on screen above it. It read as a half-open drawer.

There was a second, independent cause. `body` is a flex column, so `.ribbon`'s
`z-index: 2` makes it a **stacking context**; `#panelFile` lived inside it, and its own
`z-index: 6` could therefore never rise above the ruler's `z-index: 4`. Raising the
page's z-index did nothing — verified by raising it to 50 and re-running the hit test.
The page is now a **sibling** of the ribbon, which is also where ONLYOFFICE keeps theirs:
`Viewport.template` declares `#file-menu-panel.toolbar-fullview-panel` in its own
`<section>`, ahead of `#app-title` and `#toolbar`.

`inset: var(--h-header) 0 0 0` — the navigation row's bottom edge, full width, down to and
including the status bar. ONLYOFFICE: `$filemenu.css('top', toolbar-height-tabs)`
(`Viewport.js:182`) over a `position: absolute; bottom: 0; width: 100%` panel.

`webapp/tests/e2e/file-page-coverage.spec.mjs` asserts it from **geometry**: the ruler's
centre, the rail's centre, the sheet's centre and the status bar's centre must all hit
the File page under `document.elementFromPoint`. Nothing about that guard can pass while
the page half-opens.

### 5.2 One left column, at the gutter

ONLYOFFICE's page is two columns — a 260px navigation rail and a content pane that shows
the selected category (`filemenu.less:2-12`: `.panel-context { width: 100%; padding-left: 260px }`,
`.content-box { height: 100%; padding: 0 20px }`). Ours is the same two columns.

**320px, not 260px** for the rail, because our rows are menu rows that carry a chord on the
right, and the same roster rendered as the compact chrome's dropdown is Google Docs' File
menu, which measures 322px. At 260px "Recover unsaved work ⌘⇧D" ellipsises. The rail is
`--bg` with a 1px right border against the pane's `--surface`, which is ONLYOFFICE's
`@background-pane` / `@background-normal` pairing; each column scrolls itself.

**The pane is a selected category, and it is never empty.** `FileMenu.js:390` opens the
page on Save-As when the document can be downloaded and on **Info** otherwise; ours can
always download, so `export` is the pane it opens on.

### 5.3 Panes, not dialogs — `109` UX-026 closed

The owner's instruction was "basically in this view replace dialogs with this space", and
it is also ONLYOFFICE's shape: their File page has no dialogs at all — Advanced Settings,
Info and Help are panes (`FileMenu.js:412-415`). Seven categories now render into the pane:

| Rail row | Pane | Where the content comes from |
| --- | --- | --- |
| Export | a grid of format tiles, one per format the engine can write | `EXPORT_COMMANDS` × `formatInfo` |
| New document | a gallery of template cards | `DOCUMENT_TEMPLATES` |
| Document properties | `#propertiesPanel`, moved in | the document's own metadata |
| Settings | `#settingsPanel`, moved in | — |
| Find a command | `#cmdPalette`, moved in | the live command registry |
| Keyboard shortcuts | `#shortcutsDialog`, moved in | `shortcutGroups()` |
| About OpenDoc | `#aboutDialog`, moved in | `engineVersion()` |

Four rules hold this together, each of them a defect that was reported first:

1. **The panel is MOVED, never copied**, and released before anything else writes into
   the pane. `replaceChildren` does not move a borrowed node out — it DROPS it, so
   switching Properties → Settings deleted `#settingsPanel` from the document permanently
   and every later attempt to show it found nothing. The release is unconditional.
2. **Placed first, prepared second.** Every one of these panels fills itself when its
   DIALOG opens — the shortcuts reference is built on open, the About version stamped on
   open, the properties form loaded on open — so as a pane they came up empty. The host
   passes the same preparation in, and it runs *after* the move, because moving a node
   blurs whatever inside it held the keyboard (the command pane's search field).
3. **The pane owns the heading.** A borrowed panel's own `.dialog-head` is hidden: two
   titles saying the same thing over a close button that closes nothing is what made this
   read as a dialog parked in a page. While borrowed the panel is `role="group"` named by
   the pane's heading, not a `role="dialog"` with `aria-modal` inside the page.
4. **One measure.** A dialog card shrink-wraps and centres itself in its overlay; in the
   pane it takes the pane's width and its left edge, so every pane starts where the
   heading does.

Export tiles carry the format's own icon, and `file.export.rtf` joined them — the engine
could always write RTF; only the command row was missing.

### 5.4 Templates preview as pages, not icons

The owner's instruction: "i think having thumbnail of actual default docx inside would be
great, like how google docs show it". The card shows a **miniature of the template's own
first page** — its real paragraphs at their real point sizes on a real Letter sheet,
scaled down as one block — not a drawing.

It cannot drift from the document it opens, because both are generated from one table:
`BLANK_STYLE_METRICS` (Title 28pt, Subtitle 14pt italic, Heading 1/2/3 at 16/13/12pt bold,
Normal 11pt) and `BLANK_PAGE` (612 × 792pt, 72pt margins) are what `word/styles.xml` and
the section properties are written from, and `templateThumbnail()` resolves the same
`[styleId, text]` body against the same table. A preview claiming a 28pt title while the
document opens with a 16pt one is not reachable from here, and
`webapp/tests/blank_document.test.mjs` fails if the XML and the table ever diverge.

Four templates ship: Blank (an empty sheet, as Google Docs' first card is), Letter,
Report and Meeting notes.

### 5.5 Category rows are positional

A category row takes **the place of the command it stands in for**, and records what it
covers in `data-covers`. Both halves matter:

- Positional, so the page and the compact chrome's File dropdown list the same things in
  the same order — New document is the first row of *New and open* on both, not appended
  after *Recover unsaved work*.
- `data-covers`, so the roster stays PROVABLE across the two chromes even though one runs
  a command and the other opens a pane. One `Export` row covers all six export ids;
  `one-axis-navigation.spec.mjs` expands it and compares the full rosters.

### 5.7 Settings is a dialog, and the gear has one destination

The owner: "setting from header is broken .. i meant its height exceeds .. so that panel
doesn't make any sense now .. even for compact view .. see dialog instead."

It was an anchored popover, 268px wide, hanging off the gear. Three things were wrong with
it and only one was the height:

1. It outgrew the window as settings were added — spelling and grammar were the last two —
   so Reset and the autosave controls were unreachable and scrolling it scrolled the
   document behind. Bounding it was a patch, not a fix.
2. It hand-rolled its own Escape handler and its own `pointerdown` light dismiss. That is
   precisely the divergence `modal.mjs` exists to end: nine dialogs used to do this and
   they disagreed with each other in six different ways.
3. With Settings a File-page pane it was a second, smaller copy of the same form.

It is a modal dialog now, in the shared `.dialog-overlay` shell, registered like every
other one — so Escape, the backdrop, the close button and focus restoration are one path,
Tab cannot walk out into the chrome behind the scrim, and application shortcuts stop
firing behind it. Google Docs' Preferences and Word's Options are both modal dialogs;
ONLYOFFICE has no Settings dialog at all, because theirs is the File-page pane we also
have. The shell is also the one the File page already knows how to borrow, so the pane
got simpler rather than more complicated.

`dialog-contract.spec.mjs` has a guard that enumerates every `aria-modal` surface and
fails if one is not in its table. Adding Settings to that table immediately found a
defect the popover had been hiding: the custom-accent swatch is a 26px circle holding a
40px `<input type="color">`, painting 14px outside the box on every side.

**The gear has one destination, whichever that is.** The dialog and the pane are the SAME
element, and while the File page is open that element is parented inside the page — so
opening the dialog would have raised a half-dialog out of a pane. The gear now selects the
Settings pane while the page is open and opens the dialog otherwise. One Settings, one
place, wherever you press it from.

### 5.6 Density and Back

Rows are 28px tall with 4px beneath — a **32px pitch**, which is both ONLYOFFICE's
`li.fm-btn` and Google Docs' File menu item. They were 34px on a 36px pitch.

**Back is the first row, at the top left**, shaped like the rows under it and followed by
ONLYOFFICE's `devider-small` margins (7px above, 11px below). `FileMenu.template` opens
with `<li id="fm-btn-return" class="fm-btn">`; the running 9.4.1 editor renders it as
"← Back" in that corner; Word's backstage arrow is in the same corner. It was a button at
the top **right** — the one corner neither reference uses.

The page no longer prints a "File" heading. Neither reference does, and the tab that
opened it already names it (`aria-labelledby="tabFile"`).

---

## 6. The budget, before and after

Measured at 1408×800 with `?fixture=rich` open, light theme, from Playwright:

| | Before | After | ONLYOFFICE | Google Docs |
| --- | ---: | ---: | ---: | ---: |
| Header block bottom | 92 | **58** | 56 (two-level) / 32 (one-level) | ~59 |
| Band bottom | 184.7 | **136** | ~122 | 105 (toolbar) |
| Ruler top | 198.8 | **148** | ~156 | 113 |
| **Page top** | **228.8** | **178** | **~179** | **145** |

Compact chrome, unchanged and re-measured to prove it: header 58, menu bar top 32.5,
toolbar top 60 h 40, ruler 118, **page 148**.

So the ribbon chrome gives back **50.8px** of document, and now lands within a pixel of
ONLYOFFICE's own ribbon while keeping 30px controls that ONLYOFFICE does not.

---

## 7. Guards

| Guard | What it pins | Driven red by |
| --- | --- | --- |
| `webapp/tests/e2e/file-page-coverage.spec.mjs` | the File page covers the ruler, the rail, the sheet and the status bar; Back is in the left half and above every command row | restoring `inset: var(--chrome-bottom) …` |
| the same spec | the File page's rows sit on a pitch of at most 32px | restoring `min-height: 34px` |
| `webapp/tests/e2e/ribbon-home.spec.mjs` | no group caption is drawn in the band, and every caption is still in the accessibility tree | un-hiding `.rgroup-label` |
| `webapp/tests/e2e/ribbon-home.spec.mjs` | the tab strip is inside the header, and the band clears it | putting the strip back under the ribbon |
| `webapp/tests/menu_taxonomy.test.mjs` | the tab strip still renders exactly `RIBBON_TABS` (doc 122) | unchanged, and it still passes after the move |
| `webapp/tests/e2e/file-page-panes.spec.mjs` | every rail category renders its own pane, with exactly one heading and no close button | restoring `>` in the `.panel-in-page .dialog-head` selector |
| the same spec | a borrowed panel survives pane → pane → pane and is still the only one in the document | removing the unconditional `releaseSettingsPanel()` |
| the same spec | the panes that fill on dialog-open are filled as panes, and the command pane has the keyboard | removing `prepare?.()`; preparing before the move |
| the same spec | every pane starts at the page's left edge | removing the card-width rule; restoring the dialog body's side padding |
| the same spec | each template previews page-shaped, scaled down, showing its own text | dropping the thumbnail; making the frame square; `scale(1)` |
| `webapp/tests/blank_document.test.mjs` | `styles.xml` and the section carry exactly the sizes, spacing and page `BLANK_STYLE_METRICS`/`BLANK_PAGE` declare | offsetting any one of them by a point |
| the same file | a preview block per paragraph, carrying the style's real metrics | previewing everything as body text; dropping empty paragraphs |
| `webapp/tests/e2e/dialog-contract.spec.mjs` | Settings answers to the one modal contract, and every `aria-modal` surface is in that table | un-registering it; dropping its `aria-modal` |
| the same spec | nothing in Settings paints outside the box that holds it | restoring the 40px colour input inside its 26px swatch |
| `webapp/tests/e2e/file-page-panes.spec.mjs` | the gear opens the dialog outside the page and selects the pane inside it | removing the `file-page-open` branch |
| `webapp/tests/e2e/one-axis-navigation.spec.mjs` | the two File surfaces offer the same roster in the same order | letting `Export` cover one format; appending category rows instead of placing them |

## 8. Open rows this raises

- **`109` UX-025** — one mode control, but on the surface no reference uses. All three
  put it in the top chrome; ours is in the status bar.
- **`109` UX-026 — closed** by §5.3: seven categories render into the pane.
- **The header gear's Settings popup — closed.** See §5.7.
- **Doc 122 §6's two deferrals** (`edit.selectAll`, `edit.pasteText` on Home ▸ Editing)
  were deferred for want of 18px. There are now 269px.
