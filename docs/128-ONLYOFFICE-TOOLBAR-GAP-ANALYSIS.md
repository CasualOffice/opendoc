# 128 — ONLYOFFICE toolbar gap analysis

**Status:** Analysis, complete. **Opened:** 2026-09-27. **Owner:** unassigned.
**Scope:** the ribbon/toolbar command surface of ONLYOFFICE's **document** editor against
OpenDoc's, measured from both products' source, plus the rows to file from it.

**Why this exists.** The owner's words: *"onlyoffice's design is tried and tested… also their
toolbar has many features which we don't have."* Both halves are right, and until now there
was no defensible list. This document is that list. It feeds a build queue; it is not itself
a build, and it deliberately does not edit `104`, `105`, `109` or `14` — §9 states the rows
to file and the owner applies them centrally, because `109`'s `#` column renumbers.

## 0. Method, and why every number here is reproducible

**ONLYOFFICE's published documentation is not evidence.** Their help site and marketing
pages are not cited anywhere below; every claim is `file:line` into one of:

- `/Users/sachin/Desktop/melp/reference/web-apps` — their client shell (the ribbon lives here)
- `/Users/sachin/Desktop/melp/reference/sdkjs` — their engine

Our side is cited into this repository. **A control existing is not a feature working**
(`SKILL.md` §9.4), so every "we have this" claim below names the file and line where the
command is *declared*, and — where it matters — whether it is `enabled` or ships
disabled-with-a-reason. Three of our ribbon controls ship deliberately disabled; they are
marked as such rather than counted as present.

### The counting rule for ONLYOFFICE

One control = one component rendered into one `btn-slot` (or group-slot) in a ribbon panel's
template. A split button counts once; its menu items are listed, not counted; labels and
spinners are separate slots and are reported separately from the interactive total; the
automatic `⋯` overflow button (`apps/common/main/lib/component/Mixtbar.js:703-712`) is
excluded because the responsive layout creates it on demand rather than the tab declaring it.

### The counting rule for OpenDoc

The repository already has one, and it is the guard's — `webapp/tests/e2e/ribbon-command-faces.spec.mjs:95-105`:

```js
[...panel.querySelectorAll("button, input, select")].filter((el) => el.offsetParent !== null)
```

So each half of a split button is its own control (`#underline` + `#underlineMenuBtn`,
`#textColorApply` + `#textColor`, …), `#fontSize` counts as one `<input>` and its
`<datalist>` options do not, and menu items do not count because every gallery, colour picker
and font list lives *outside* the `.ribbon-panel` elements (`editor.html:1200-1290`).
`#ribbonOverflowBtn` (`editor.html:860`) is excluded: it is outside every panel and `hidden`.

Measured two independent ways this round, which agree: in a live Chromium after boot with the
selector above, and statically over `webapp/editor.html` with HTML comments stripped (the
Styles group's comment contains a literal `<select>` that inflates a naive grep by one):

```sh
python3 - <<'PY'
import re
html = re.sub(r'<!--.*?-->', '', open('webapp/editor.html').read(), flags=re.S)
for p in ['panelHome','panelInsert','panelLayout','panelReferences','panelTable','panelView','panelReview']:
    s = html.find('<div id="%s"' % p)
    print(p, len(re.findall(r'<(button|select|input)\b', html[s:html.find('<div id="panel', s+1) if '<div id="panel' in html[s+1:] else len(html)])))
PY
```

### What a fair comparison must exclude

A control only a paying desktop user sees is not a web-parity gap. From their source:

| Excluded because | Controls | Guard |
| --- | --- | --- |
| **Desktop-app only** | Paste split-menu; Print split + Quick print; File ▸ Print-with-preview; File ▸ Save As; File ▸ Open/Close; **digital signatures (both kinds)** | `Toolbar.js:286-287`; `Toolbar.js:202-203` + `controller/Main.js:1736` (`!isMac && isDesktopApp`); `FileMenu.js:435` + `Main.js:1735`; `FileMenu.js:429`; `FileMenu.js:508`; `Main.js:1754` (`isEdit && isDesktopApp && isOffline && …`) |
| **Licence-gated** (`canLicense`, `Main.js:1704`) | Mail Merge; **Version History**; **Chat**; **comments at all**; "New style from selection"; Forms ▸ Submit | `Main.js:1723`; `:1720`; `:1728-1729`; `:1725`; `Toolbar.js:3414` + `Main.js:1733`; `Main.js:1778-1779` |
| **PDF/forms documents only** | the entire Forms tab (17 editor-visible controls) | `controller/Toolbar.js:4044-4045`; `Main.js:602` `isFormCreator` |
| **Dead in their own build** | `btnProtectForm` (`Protection.js:174` `if(0 && …)`), `btnSaveForm` (`Main.js:1799` `false && …`), `btnFinal` (`FormsTab.js:131` `class="hidden"`), `btnEditMode` (commented out, `Toolbar.js:2052-2062`) | as cited |

So a like-for-like figure is **the steady state a licensed cloud user editing a `.docx`
sees**, which is what §1 compares.

## 1. Headline counts

| | ONLYOFFICE 9.4 document editor | OpenDoc |
| --- | --- | --- |
| Tabs declared | **13** — 12 panels + File (`haspanel:false`, `view/Toolbar.js:182`) | **8** — File (a route) + 6 standing panels + 1 contextual |
| Tabs a cloud `.docx` editor sees | 9–10 (File, Home, Insert, Draw, Layout, References, Collaboration, Protection, View, and Plugins when one is installed) | 7 (File, Home, Insert, Layout, References, Review, View) + Table when the caret is in a table |
| All declared ribbon slots, every panel | **206** (186 panel slots + 19 File-menu items + 1 text label) | 122 |
| Reachable interactive ribbon controls | **168** | 122 |
| **Steady-state interactive controls a cloud `.docx` editor sees** | **120** | **122** |
| File-menu / backstage entries | 19 static + 4 conditional | 12 File-page rows (4 direct, 8 panes) — `webapp/src/command_taxonomy.mjs:46-71` |
| Named keyboard actions | **147** (`sdkjs/word/apiDefines.js:223-371`; id 142 is absent from the sequence) | 36 chords over 35 distinct commands — `webapp/src/keymap.mjs:83-155` (`⌘⇧Z` and `⌘Y` both run `edit.redo`) |
| UI locales | 46 (`apps/documenteditor/main/locale/`) | 19 (`webapp/locales/`) |

Of our 122 controls, **113 carry `data-command` and 9 carry `data-command-family`**, naming
**106 distinct command ids** — the 7 duplicate faces are deliberate (indent ± on Home and
Layout, Find and Replace both naming `edit.find`, Bookmark and Field on Insert and References,
Comment on Insert and Review, the comments pane on View and Review). None of those attributes
is in the markup: they are stamped at boot from `webapp/src/ribbon_faces.mjs:191-206` and from
four declaration tables in `main.js` (`INSERT_SURFACE:8004`, `LAYOUT_SURFACE:8066`,
`REFERENCE_SURFACE:8119`, `REVIEW_SURFACE:8248`), applied at `main.js:9394-9421`.

**Our command registry has no single size.** `editorCommands()` (`main.js:12073`, returning at
`:12610`) is a literal array plus ~15 generated blocks that read live state — export formats,
field kinds, spacing and list presets scraped out of the popover markup, zoom presets out of
`#zoomMenu`, the installed font inventory, `doc.listStyles()`, `doc.listTableStyles()`. So a
figure only means something with its state named:

| State | Commands |
| --- | --- |
| No document open | **11** |
| `?fixture=rich`, caret in body, no range | 214 |
| `?fixture=rich`, caret in a table, range selected | **233** |
| `sample.docx` (40 paragraph styles), caret in a table, range | 237 |
| Union of all states observed | **269** |

Prefix histogram of the 269: `format` 72, `style` 51, `paragraph` 26, `insert` 21, `table` 19,
`view` 16, `review` 16, `layout` 15, `file` 12, `edit` 8, `reference` 5, `help` 3, `tools` 3,
`object` 2. The often-quoted "~231" is in the right band and is not reproducible as stated.

ONLYOFFICE's steady-state total of 120 is derived as Home 31 + Insert 20 + Draw 5 + Layout 20
interactive + References 10 + Collaboration 16 + Protection 2 + View 16.

**The headline finding is that breadth by count is a dead heat — 122 against 120 — and the
gap is composition, not quantity.** Two structural differences carry almost all of it:

1. **They spread formatting across a 10-panel right sidebar** (`view/RightMenu.js:117-321`)
   and ship almost no contextual ribbon — no Table Design, no Picture Format, no Shape
   Format. We ship a contextual Table tab *and* property panels, so our information
   architecture is not behind; it is differently shaped.
2. **Their breadth is concentrated in three areas we have not built at all**: document
   flow authoring (breaks, hyphenation, TOC/TOF), object arrangement (align, distribute,
   group, merge shapes, z-order), and the collaboration/protection surface. Those, not the
   tab count, are the gap.

A count of 122 against 120 is also the reason this document ranks by *what a user reaches
for* rather than by control count: adding fourteen buttons to reach 136 would not close a
single one of the P1 rows in §7.

## 2. Tab-by-tab comparison

Per tab: what they offer (cited), our status, and our command id where one exists.
*Present* / *present but weaker* / *reachable differently* / *absent*.

### 2.1 File

Theirs is a full-screen backstage (`view/FileMenu.js:100-327`, panels wired at `:409-413`);
every item is an entry point, so a capability we reach another way is not behind.

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| Save | `FileMenu.js:111-121` | `file.save` (`main.js:12085`) | Present |
| Download As | `FileMenu.js:139-148` | Export pane, format tiles (`123` §5.3) + `exportCommands` (`webapp/src/export_commands.mjs`) | Present |
| Save Copy As | `FileMenu.js:150-159` | Export pane | Reachable differently |
| Create New + templates | `FileMenu.js:237-246` | `file.new` (`main.js:12084`), four templates (`123` §5.4) | Present |
| **Open Recent** | `FileMenu.js:227-235` | — | **Absent** (OO-002 remainder) |
| Print | `FileMenu.js:183-193` | `window.print()` over rasters | Present but weaker (OO-010) |
| Print with preview | `FileMenu.js:171-181` | — | Excluded — desktop-only (`Main.js:1735`) |
| Rename | `FileMenu.js:195-205` | — | Absent, minor |
| **Protect** | `FileMenu.js:211-221` | — | **Absent** (OO-011) |
| **Version History** | `FileMenu.js:270-283` | — | **Absent** (HF-068), and licence-gated on their side |
| Info / document properties | `FileMenu.js:248-257` | Document properties pane | Present |
| Advanced Settings | `FileMenu.js:285-294` | Settings pane | Present |
| Access Rights | `FileMenu.js:259-268` | — | Absent; needs a host contract, not a feature |
| Help | `FileMenu.js:296-305` | Keyboard shortcuts + About panes | Present but weaker |
| Open / Close / Switch to Mobile | `FileMenu.js:508-572` | — | Excluded — desktop-only |

**`docs/105` §4.3's "File … Gap — UX-011, OO-002, OO-011" is stale**: the backstage, New and
templates shipped in #596/#542. Recent files, Protect and History are the remainder.

### 2.2 Home — theirs 31, ours 41

| Their group | Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- | --- |
| Font | font name, font size, grow, shrink | `Toolbar.js:1750`, `:1739`, `:340`, `:354` | `format.family`, `format.size`, `format.grow`, `format.shrink` | Present |
| | Change Case (5 items) | `:533-550` | `format.case.*` | Present |
| | bold / italic / underline / strike / super / sub | `:368`–`:453` | `format.bold` … `format.subscript` | Present |
| | highlight (16 fixed colours + No Fill) | `:460-483`, palette `:3296-3316` | `format.highlight` | Present |
| | font colour (auto + eyedropper) | `:486-498` | `format.color` | Present but weaker — no eyedropper |
| | Clear Style | `:1575-1583` | `format.clear` | Present |
| Paragraph | bullets (9 library bullets, change level, list settings) | `:811-824`, picker `:3152-3173`, menu `:2696-2720` | `paragraph.list.bullet`, `paragraph.listFormat.bullet` | Present but weaker — **no list-settings dialog** |
| | numbering (presets from `resources/numbering/numbering-lists.json`, change level, settings) | `:828-841`, `:2727-2750` | `paragraph.list.numbered`, `paragraph.listFormat.` | Present but weaker |
| | **multilevel list gallery** | `:845-855`, picker `:3243-3254` | — | **Absent** (OO-021) |
| | indent ± | `:617-638` | `paragraph.indent.decrease/increase` — and these change *list level* inside a list (`main.js:9657-9664`) | Present; list level is reachable differently |
| | line spacing (6 presets + before/after + Options) | `:645-671` | `paragraph.spacing.` | Present |
| | **text direction LTR/RTL** | `:674-691` | — | **Absent** — §4.1 |
| | align left/centre/right/justify | `:553-610` | `paragraph.align.*` | Present |
| | **show formatting marks** (split: hidden chars / hidden table borders) | `:786-804` | — | **Absent** — §4.2 |
| | paragraph shading | `:501-514` | Paragraph properties ▸ Fill (`editor.html:1897`+) | Reachable differently |
| | paragraph borders (10 edge presets, 7 widths, colour, **horizontal line**) | `:518-530`, menu `:2836-2951`, horizontal line `:2828-2833` | Paragraph properties ▸ Shading & borders | Reachable differently; **horizontal line absent** |
| Styles | gallery + per-style update/delete/restore/restore-all/delete-all; New style from selection | `:1770-1838`, `:1869-1879`, `:1762-1766` | `style.` chooser, `style.updateFromSelection` (`main.js:12369`), `style.createFromSelection` (`:12378`) | Present; ours is a curated short list by design (`docs/115`) |
| Editing | Replace, Select All | `:326-333`, `:312-319` | `edit.find` (both Find and Replace are faces of it), `edit.selectAll` | Present |
| — | — | — | **checklist** (`paragraph.list.checklist`, `main.js:12221`), **restart / continue numbering** (`:12225`, `:12226`), **format painter** (`format.painter`) | **Ours only** — §5 |

### 2.3 Insert — theirs 20, ours 17

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| **Blank Page** | `Toolbar.js:979-989` | — | **Absent** (needs a page break — §4.3) |
| **Breaks** — page, column, section ▸ next page / continuous / even / odd | injected `:2245-2249`; menu `:2382-2388`; section submenu `:2372-2380` | — | **Absent, engine included** — §4.3 |
| Table (8×10 picker, custom, draw, erase, text-to-table, **Insert Spreadsheet**) | `:878-899`, picker `:3280-3286` | `insert.table`, and `table.*` on the contextual tab | Present but weaker — no draw/erase table, no OLE spreadsheet; text-to-table absent |
| Image (file / URL / storage) | `HeaderFooterTab.js:281-283`, menu `:326-331` | `insert.image` | Present but weaker — file only |
| Shape | `:992-1005` | `insert.shape` | Present |
| SmartArt (159 layouts) | `:1008-1020`, sections `:2959-3025` | — | **Absent** (OO-014); their editing is formatting-only — §5 |
| Chart | `:902-914`, picker `:2789-2807` | — | **Absent** (OO-014 / FID-R-08) |
| Comment | `controller/Toolbar.js:4085-4089` | `review.comment` (on Review) | Reachable differently |
| Link | `Links.js:178-180` | `insert.link` | Present |
| Header & Footer (edit/remove header, edit/remove footer) | `HeaderFooterTab.js:261-262`, menu `:343-351` | `insert.header`, `insert.footer` | Present but weaker — **no Remove header/footer** |
| Page Number (6 positions, current position, of-pages, **Page Numbering** format dialog) | `HeaderFooterTab.js:265-266`, menu `:359-380`, positions `:394-401` | `insert.field.page` | Present but weaker — no position grid, no numbering format (§4.4) |
| Text Box (horizontal / vertical) | `:917-931`, menu `:3049-3070` | `insert.textbox` | Present but weaker — no vertical text box |
| **Text Art** | `:934-951`, picker `:3027-3047` | — | **Absent** |
| Drop Cap (none / in text / in margin / advanced) | `:1063-1106` | `insert.dropCap` | Present; advanced options remain (OO-006) |
| Date & Time | `HeaderFooterTab.js:269-271` | `insert.field.date` | Present |
| **Text from File** (local / URL / storage) | `:954-965`, menu `:2611-2617` | — | **Absent** |
| Field | `HeaderFooterTab.js:275-277` | `insert.field` (dialog, `field_kinds.mjs`) | Present |
| Equation | `:1023-1036` | — | **Absent** (OO-009) |
| Symbol (25 recents + full table) | `:1039-1060`, `:3073-3099` | `insert.symbol`, **`insert.emoji`** | Present; emoji picker is ours only |
| **Content Controls** (7 types + edit + highlight) | `:1109-1192` | — | **Absent** (OO-012) |

### 2.4 Layout — theirs 20 interactive (+4 labels), ours 13

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| Margins (4 presets + last custom + custom) | `Toolbar.js:1301-1352` | `layout.margins` → Page setup ▸ Margins | Present |
| Orientation | `:1257-1288` | `layout.orientation` | Present |
| Size (13 presets + custom) | `:1362-1484` | `layout.size` | Present but weaker — fewer presets |
| Columns (5 presets + custom) | `:1196-1254` | `layout.columns` | Present |
| Breaks (second slot of the Insert control) | `:2245-2249` | — | **Absent** — §4.3 |
| Line Numbers (none/continuous/restart page/restart section/suppress + custom) | `:1487-1537` | `layout.lineNumbers` (#609) | Present |
| **Hyphenation** (none / auto / custom) | `:1540-1572`; engine is complete — `sdkjs/word/Editor/Paragraph/TextHyphenator.js`, breaker `Paragraph_Recalculate.js:4335`, painted `RunContent/Text.js:405` | — | **Absent, engine included** (FID-L-02 / OO-006) |
| indent left/right, spacing before/after spinners | `:694-783` | `layoutIndentFieldsBtn`, `layoutSpacingFieldsBtn` | Present |
| Wrapping (8 modes + edit wrap boundary) | `:2336-2337`, menu `:2544-2606` | `layout.arrange.wrap` → object inspector | Reachable differently; no wrap-boundary editor |
| **Bring Forward / Send Backward** (front/forward, back/backward) | `:1655-1678`, menus `:2395-2419` | `layout.arrange.bringForward` **ships disabled** — `main.js:8108-8116`, reason *"needs a z-order operation the engine does not expose yet"* | **Absent** — §4.5 |
| **Align** (6 alignments + distribute h/v + align to page / margin / objects) | `:1619-1629`, menu `:2459-2494` | — | **Absent** — §4.5 |
| **Group / Ungroup** | `:1631-1641`, menu `:2529-2539` | — | **Absent** — §4.5 |
| **Merge Shapes** (union, combine, fragment, intersect, subtract) | `:1643-1653`, menu `:2497-2526` | — | **Absent** |
| Watermark (edit / remove) | `:1680-1702` | `layout.watermark` (`main.js:8084`) | Present |
| **Page Color** | `:1704-1723` | — | **Absent, and modelled** — §4.4 |
| **Colors** (theme colour schemes, 24+) | `:1601-1616`, generated `:3419-3463` | — | **Absent** |
| — | — | `layout.firstPageVariant`, `layout.evenOddVariant` (`main.js:8103-8104`) | Reachable on Layout here, on the contextual Header & Footer tab there |

### 2.5 References — theirs 10, ours 9

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| **Table of Contents** (2 previews + settings + remove) | `Links.js:172-174`, menu `:289-297` | `reference.tableOfContents` **ships disabled** — `main.js:8120-8127` | **Absent** (OO-001) |
| **Add Text** (don't show in TOC / level *n*) | `Links.js:197-209`, built `:441-460` | — | **Absent** (part of OO-001) |
| **Update Table** (all / page numbers only) | `Links.js:183-194`, menu `:313-318` | — | **Absent** (part of OO-001) |
| Footnote / Endnote, **convert to endnotes / to footnotes / swap**, **delete all notes**, **note settings**, go-to next/prev note | `Links.js:175-177`, menu `:332-398` | `insert.footnote`, `insert.endnote` | Present but weaker — §4.6 |
| Link | `Links.js:178-180` | `insert.link` | Present |
| Bookmark | `Links.js:212-221` | `insert.bookmark` → Bookmark manager (`docs/78`) | Present |
| Caption | `Links.js:224-233` | `reference.caption` (`main.js:8135`) | **Present** — shipped; `docs/109` row 77 is stale (§8) |
| Cross-reference | `Links.js:236-245` | `reference.crossReference` (`main.js:8147`) | **Present** — shipped |
| **Table of Figures** + its Update | `Links.js:248-257`, `:260-269` | — | **Absent** (OO-001) |
| — | — | `reference.updateCaptionNumbers` (`main.js:8159`) | Ours only — they renumber on field update, we offer it explicitly |
| — | — | `reference.updateFields` **ships disabled** (`main.js:8166-8175`) | — |

### 2.6 Review (ours) vs Collaboration (theirs) — theirs 16, ours 15

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| Track changes (on/off for me, on/off global) | `ReviewChanges.js:340-351`, menu `:569-600` | `review.mode.suggesting` | Present but weaker — no per-user vs global split |
| Display Mode (markup / simple markup / final / original) | `ReviewChanges.js:383-433` | `view.showChanges` + `docs/93` policy | Present but weaker — two states, not four |
| Previous / Next change | `:356-375` | `review.previous`, `review.next` | Present |
| Accept (current / all) · Reject (current / all) | `:287-310`, menus `:604-631` | `review.acceptNext`, `review.rejectNext`, `review.acceptAll`, `review.rejectAll` | Present |
| Comment | `controller/Toolbar.js:4085` | `review.comment` | Present |
| Remove comment (current / mine / all) | `:498-508`, menu `:717-737` | `review.comment.delete` (current only) | Present but weaker — §4.7 |
| Resolve comment (current / mine / all) | `:511-521`, menu `:739-759` | `review.comment.resolve` (current only) | Present but weaker — §4.7 |
| **Compare / Combine** (file / URL / storage + settings) | `:314-337`, menus `:637-660` | — | **Absent** (OO-007) |
| **Sharing** | `:439-447` | — | **Absent** (OO-018) |
| **Co-editing Mode** (Fast / Strict) | `:452-462` | — | **Absent** (HF-114); and it needs their server |
| **Chat** | `:483-492` | — | **Absent**, and licence-gated on their side (`Main.js:1728-1729`) |
| **Version History** | `:470-478` | — | **Absent** (HF-068), licence-gated on their side |
| **Mail Merge** | `:527-544` | — | **Absent** (OO-013), licence-gated on their side |
| — | — | `tools.spellCheck` (`main.js:12345`) | Present both sides — theirs is `spell.wasm` |
| — | — | **`tools.grammarCheck`** (`main.js:12346`, `webapp/src/grammar.mjs`) | **Ours only** — §5 |
| — | — | `tools.smartQuotes` (`main.js:12340`) | Ours only on the ribbon; theirs is an AutoCorrect setting |
| — | — | `review.toggle` (comments/suggestions pane) | Present both sides |

### 2.7 View — theirs 16 (edit mode), ours 8

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| Headings / navigation panel (promote, demote, insert heading before/after, expand to level 1-9, font size, wrap) | `ViewTab.js:202-211`; panel `Navigation.js:76-238` | `view.outline` | Present but weaker — no promote/demote, no expand-to-level |
| Zoom combo (10 presets) | `ViewTab.js:416-439` | `view.zoom` + footer zoom menu | Present |
| Fit to page / Fit to width | `:216-238` | `view.zoom.fitPage`, `view.zoom.fitWidth` | Present |
| Zoom to 100% | `:326-334` | `view.zoom.100` (`#viewZoomActual`) | **Present** — `105` §4.3's "lacks zoom-to-100%" is stale (§8) |
| **Multiple pages** | `:313-323` | — | **Absent** — §4.8 |
| Interface theme (8) | `:240-250` | `view.settings` ▸ theme (2) | Present but weaker (OO-020) |
| **Dark document** | `:253-262` | — | **Absent** — §4.8 |
| **Rulers** show/hide | `:303-310` | ruler is built unconditionally (`main.js:3688-3689`, `webapp/src/ruler.mjs`) | **Absent** — §4.8 |
| **Status bar / Left panel / Right panel** visibility | `:265-300` | `view.compactRibbon` only | **Absent** — §4.8 |
| Always show toolbar | `:275-282` | `view.compactRibbon` | Reachable differently |
| Macros / Record / Pause | `:342-372` | — | **Absent** (OO-017); config-gated on their side |
| Hand / Select tool | `:375-401` | — | Excluded — removed in edit mode (`ViewTab.js:509`) |
| — | — | `layout.pageSetup` (`#pageSetupBtn`) | Reachable differently — theirs is on Layout |

### 2.8 Tabs with no OpenDoc counterpart

| Theirs | Controls | Verdict |
| --- | --- | --- |
| **Draw** (Select, 2 pens, highlighter, eraser; 20-colour palette, mm size) | 5 — `apps/common/main/lib/view/Draw.js:153-162`, `:122-135`, `:140-150` | **Absent** (OO-019). Minimal even for them: whole-stroke eraser only, no ink-to-shape |
| **Protection** (Encrypt, change/delete password, Signature ▸ invisible / signature line, Protect Document) | 5 declared / 3 reachable / **2 visible in a browser** — `Protection.js:137-169`, `DocProtection.js:80-89` | **Absent** (OO-011). Signatures are desktop+offline-only (`Main.js:1754`), so only the password half and Protect Document are web-parity gaps |
| **Forms** | 21 declared / 17 editor-visible — `FormsTab.js:365-710` | **Non-goal** — their form designer is PDF/DOCXF-only (`Main.js:602`). §6 |
| **Plugins** | dynamic; 0 declared statically — `view/Plugins.js:102-122`, built `controller/Plugins.js:418-505` | **Absent** (OO-017), deliberately |
| **Header & Footer** (contextual) — Header&Footer, Page Number, Date&Time, Field, Image, Header-from-top spinner, Footer-from-bottom spinner, Different odd/even, Different first page, **Same as previous**, Close | 13 slots (11 interactive + 2 labels) — `HeaderFooterTab.js:53-90`, `:199-219`, `:259-260` | Capability mostly ships without the tab: `insert.header`/`insert.footer`, `layout.firstPageVariant`, `layout.evenOddVariant`. **Header/footer distance and Same-as-previous are absent** — §4.4, §4.9 |
| **Chart Design** (contextual) — Chart Elements (9 submenus, 45 leaves), Edit Data, Update Data, styles gallery, Chart Type, Advanced, size spinners, lock ratio, 3-D rotation | 14 — `apps/common/main/lib/view/ChartTab.js:566-782` | **Absent**, gated on Q3 (charts are not drawn at all) |

## 3. Where their advantage is real, and where it is not

Three findings that change how the list above should be read:

1. **Their References tab is 10 controls and ours is 9.** The old verdict — `105` §4.3,
   "References — **Largest single IA gap**" — is no longer true. We ship the tab, and
   captions and cross-references work. What is missing is specifically the *field-generated*
   half: TOC, Add Text, Update Table, Table of Figures.
2. **Their Layout tab's extra 7 controls are almost all object arrangement**, not page
   layout. Page setup is at parity; object arrangement is not built at all here.
3. **Their most-used absent control is not a feature at all but a view**: the formatting
   marks toggle, which is one of the few controls in a word processor a user reaches for
   dozens of times a session. It has a named keyboard action on their side
   (`ShowAll`, `sdkjs/word/apiDefines.js:325`) and no equivalent here.

## 4. The gaps, in detail

Ranked in §7. Each is classified **UI only** / **facade** (a WASM export over an existing
operation) / **engine** (a new operation or a layout/render capability) / **model**.

### 4.1 Paragraph direction (LTR / RTL) — facade + UI

Theirs: `Toolbar.js:674-691`, two items at `:684-685`.

Ours: `bidi` is a typed paragraph property (`crates/casual-doc-model/src/v1/properties.rs:1041`),
cascades (`crates/casual-doc-layout/src/cascade.rs:548-549`), reaches the shaper as `rtl`
(`crates/casual-doc-layout/src/flow.rs:7147`), is honoured in hit-testing
(`crates/casual-doc-layout/src/hittest.rs:1162`) and is even **reported as a formatting
difference** by the inspector (`crates/casual-doc-wasm/src/lib.rs:18657-18659`). It is
read-only: `Operation::SetParagraphProperties` replaces the whole `ParagraphProperties`
(`crates/casual-doc-edit/src/lib.rs:409-414`), so the operation already carries it and the
missing piece is a WASM setter plus two menu items.

**This is the sharpest gap in the document**: we ship 19 UI locales including `ar.json`, and
an Arabic-speaking user cannot set a paragraph right-to-left.

### 4.2 Formatting marks (¶ · → ) — engine + UI

Theirs: `Toolbar.js:786-804`, a split button whose menu separates hidden characters
(`:797`) from hidden table borders (`:798`), with a named keyboard action
(`sdkjs/word/apiDefines.js:325`).

Ours: nothing. `grep -rin 'formatting marks|nonprinting|invisible'` over `webapp/src`,
`webapp/editor.html` and `webapp/locales/en.json` returns no feature hit, and the layout and
render crates have no invisibles primitive — the only "invisible" occurrences in
`casual-doc-layout` are comments. So this needs a display-list layer (pilcrow at paragraph
end, mid-dot per space, arrow per tab, a page-break rule) before any UI. The engine does
already draw a pilcrow-sized marker for paragraph-mark revisions (`main.js:1478-1491`),
which is a precedent for the geometry but not the feature.

### 4.3 Breaks — page, column, section — engine + facade + UI

Theirs: one split button rendered into **both** Insert and Layout
(`controller/Toolbar.js:2245-2249`); menu `Toolbar.js:2382-2388` (page / column / section);
section submenu `Toolbar.js:2372-2380` (next page / continuous / even page / odd page).

Ours: **there is no break-insertion operation.** `Operation` has 50 variants
(`crates/casual-doc-edit/src/lib.rs:326-916`) and none inserts a break; the only break
command is `insert.lineBreak` (`main.js:12252`). `BreakKind::{Line, Page, Column}` *is*
modelled (`crates/casual-doc-model/src/v1/properties.rs:324-332`), so imported breaks
paginate correctly — we simply cannot author one. A section break additionally needs a new
section with inherited properties, which is why `Blank Page` (theirs: `Toolbar.js:979-989`)
is blocked behind the same work.

This is the highest-ranked absent capability in the document: a page break is a keystroke
away in every word processor, and `docs/99` §3 already names section-break insertion.

### 4.4 Section and page properties that are modelled, laid out, and unreachable — mostly UI only

This is the cheapest class in the repository, and the owner is right that it is large.

| Property | Modelled | Laid out / painted | Operation carries it? | Their UI |
| --- | --- | --- | --- | --- |
| Header distance from top | `definitions.rs:354` `header_twips` | yes | **yes** — `SetSectionGeometry` takes the whole `PageMargins` (`casual-doc-edit/src/lib.rs:655-667`) and `setPageSetup` accepts it as JSON (`casual-doc-wasm/src/lib.rs:6998-7026`) | `HeaderFooterTab.js:65-66`, label `:504` |
| Footer distance from bottom | `definitions.rs:361` `footer_twips` | yes | **yes**, same | `HeaderFooterTab.js:69-70`, label `:505` |
| Gutter (binding margin) | `definitions.rs:364` `gutter_twips` | yes | **yes**, same | `PageMarginsDialog.js:87-90`, `:199-215` |
| Gutter position (top vs left) | — | — | no | `PageMarginsDialog.js:217-232` |
| Mirror margins | import `casual-doc-import/src/settings.rs:233`, export `casual-doc-export/src/semantic.rs:3118`, laid out (`casual-doc-layout/tests/section_geometry.rs:313`, FID-L-16) | yes | document settings, not the section op | `PageMarginsDialog.js:290`, `:306` |
| Page borders | `definitions.rs:556`, `:848`; resolved `casual-doc-layout/src/page_border.rs:30`; painted `compose.rs:308-309`, `:420` | **yes** | **no** — not a field of `SetSectionGeometry` | **They have no dialog either** — §5.4 |
| Page vertical alignment | `definitions.rs:833` `PageVerticalAlignment` | yes | **no** | — |
| Page numbering format + start-at | `definitions.rs:435-446`; consumed in pagination `casual-doc-layout/src/paginate.rs:1733-1760` | **yes** | **no** | `PageNumberingDlg.js:132-210` |
| Page background colour | `casual-doc-model/src/v1/document.rs:51`, setter `:112`; painted `casual-doc-render/src/lib.rs:103-105` | **yes** | document-level | `Toolbar.js:1704-1723` |

Our Page setup dialog offers section, orientation, width/height, four margins and columns
(`webapp/editor.html:985-1036`), and its apply path **spreads the untouched margin fields
straight back through** — `webapp/src/page_setup.mjs:280` `...current.pageMargins` — which is
exactly why header distance, footer distance and gutter survive a round-trip while being
unauthorable. Three number inputs close three of these rows with no engine change.

A lane is building this now, so §7 files the **remainder** (gutter position, mirror margins,
page borders, vertical alignment, numbering format, page colour) rather than the whole class.

### 4.5 Object arrangement — engine + UI

Theirs: Bring Forward / Send Backward (`Toolbar.js:1655-1678`), Align with 6 alignments +
distribute horizontally/vertically + align-to page / margin / objects
(`Toolbar.js:1619-1629`, menu `:2459-2494`), Group / Ungroup (`:1631-1641`), Merge Shapes
with 5 boolean operations (`:1643-1653`).

Ours: `layout.arrange.bringForward` is declared and **ships disabled with a reason**
(`main.js:8108-8116`) — *"`objectOrder()` READS paint order; nothing writes it, and there is
no z-order op in the wasm facade"*. There is no align, distribute, group or merge command at
all; `object.selectNext` / `object.selectPrevious` (`main.js:12585`, `:12594`) are the only
object commands outside the inspector. `SetGroupGeometry` exists
(`casual-doc-edit/src/lib.rs:517`) so groups can be *resized*, not *formed*.

### 4.6 Note management — facade + UI

Theirs: convert to endnotes / to footnotes / swap (`Links.js:385-395`), delete all notes
(`:384`), note settings (`:396`), and go-to next/previous footnote and endnote
(`:401-416`).

Ours: `insert.footnote` and `insert.endnote` only. `Operation::RemoveNote`
(`casual-doc-edit/src/lib.rs:778`) exists, so delete-all is facade work; conversion and swap
are new operations.

### 4.7 Comment and change scope — UI only

Theirs: remove comment ▸ current / mine / all (`ReviewChanges.js:717-737`), resolve ▸
current / mine / all (`:739-759`), accept/reject ▸ current / all (`:604-631`).

Ours: accept/reject have both scopes (`review.acceptAll`, `review.rejectAll`); comments have
only the current scope (`review.comment.resolve`, `review.comment.delete`). The "mine" scope
needs the author identity we already carry (`docs/82`), so this is UI work over an existing
model.

### 4.8 View-tab breadth — UI only

Rulers show/hide (`ViewTab.js:303-310`), multiple pages (`:313-323`), dark document
(`:253-262`), status bar / left panel / right panel visibility (`:265-300`). Ours has
`view.compactRibbon` and nothing else. The ruler is built unconditionally
(`main.js:3688-3689`); hiding it is a class toggle. Multiple-pages view is the one with real
cost, because it changes the sheet layout the viewer paints.

### 4.9 Same-as-previous header/footer — facade + UI

Theirs: `HeaderFooterTab.js:84`, `:219` (`chSameAs`).

Ours: `Operation::SetSectionRunningRef` exists (`casual-doc-edit/src/lib.rs:819`) but the
facade uses it only when *creating* a header or footer body
(`casual-doc-wasm/src/lib.rs:3473-3479`). There is no way to link or unlink a section's
header from the previous section's, so a multi-section document's header inheritance is
fixed at import.

### 4.10 Advanced character formatting — model-adjacent engine + facade + UI

Theirs, in the Paragraph Advanced Settings dialog's Font tab: Small caps, All caps,
character spacing, character position — `ParagraphSettingsAdvanced.js:484-512`, assembled at
`:770`.

Ours: all four are typed and **laid out**:

| Property | Model | Layout |
| --- | --- | --- |
| `all_caps` | `properties.rs:1264` | `cascade.rs:461`, `flow.rs:6393` |
| `small_caps` | `properties.rs:1267` | `cascade.rs:462`, `flow.rs:6419-6420`, `:6518` |
| `character_spacing_twips` | `properties.rs:1292` | `cascade.rs:470`, `flow.rs:6172`, `:6209`, `:6284` |
| `position_half_points` | `properties.rs:1301` | `flow.rs:6370` |
| `kerning_half_points` | `properties.rs:1298` | unapplied (FID-L-15) |

**But the operation cannot carry them.** `FormatDelta`
(`crates/casual-doc-edit/src/lib.rs:96-122`) has exactly eleven fields — bold, italic,
underline, underline colour, underline style, strike, colour, highlight, size, vertical
alignment, font — and none of these. So unlike §4.4 this is *not* UI-only: it needs fields on
`FormatDelta` and `apply_to`, then a facade export, then the UI. Worth stating plainly
because "modelled and laid out" invited the cheaper estimate.

## 5. What we have that ONLYOFFICE does not — re-verified

Each claim below was re-checked against their source this round. **An out-of-date advantage
claim is as damaging as a missed gap** (`SKILL.md` §9.6), and two of the seven claims in
`docs/105` §4.2 needed correction.

### 5.1 Still true, and can be sharpened

| Claim | Verdict | Their source | Ours |
| --- | --- | --- | --- |
| **No table sorting** | **Holds, strongly.** No sort API exists in their word engine; their *spreadsheet* engine has one, so this is a product decision, not a missing repo. Nothing in their table context menu (`DocumentHolderExt.js`) or Table panel (`TableSettings.js`) mentions sort | absent from `sdkjs/word/**`; present at `sdkjs/cell/api.js:4633` `asc_sortCells`, `:4656`, `:1148` | `crates/casual-doc-wasm/src/lib.rs:6288` `sort_table`; ribbon `table.sort.ascending` / `.descending` |
| **No accessibility checker** | Holds — 4 case-insensitive "accessibility" hits in the whole `web-apps` tree, none a feature; the nearest thing is a *setting*, "Turn on screen reader support" | `FileMenuPanels.js:506-508`, label `:1290` | **Neither product has one.** Not an advantage — do not publish it as one |
| **No native citation manager** | Holds, and stronger than recorded: zero bibliography or citation strings in the document editor; the only engine token is an SDT type constant; Zotero/Mendeley exist only as help HTML for out-of-repo, online-only plugins | `sdkjs/word/Editor/Serialize2.js:1368` `sdttypeBibliography: 0`; 114 "zotero" hits, **0 outside `resources/help/`** | Neither has one; ours is also absent |
| **SmartArt editing is formatting-only** | Holds. Their entire SmartArt API is 11 symbols of which **two are actions** — insert a preset and render previews. No add-shape, promote, demote, change-layout or style gallery; no `SmartArtSettings.js`; `ShapeSettings.js` *removes* capability inside one | insertion `sdkjs/word/api.js:13093-13110`; `ShapeSettings.js:901`, `:905`, `:911`, `:2331-2332`; layout engine `sdkjs/common/SmartArts/SmartArtTree.js`; 159 layouts = `grep -c '"Common.define.smartArt.text' locale/en.json` | We do not draw SmartArt at all (FID-R-08), so this bounds the *authoring* bar we would have to clear, not a present advantage |
| **Table formulas: 18 functions over 4 directions** | Accurate, no change needed | `sdkjs/word/Editor/Paragraph/FormulaParser.js:62` (one regex, 18 names), directions `:1765-1788`, 7 number formats `sdkjs/word/api.js:12518-12521` | Ours: `SUM`/`AVERAGE`/`MIN`/`MAX` over `ABOVE`/`LEFT` (OO-008) |
| **Hyphenation and line numbering are complete on their side** | Accurate — do not weaken. Hyphenation is a real dictionary engine consumed by the line breaker and painted; line numbers are recalculated, cached and painted with all restart types | `common/libfont/engine.js:983`, `TextHyphenator.js`, breaker `Paragraph_Recalculate.js:4335`, `:4370`, painted `RunContent/Text.js:405`; line numbers `sections/sect-pr.js:1330-1362`, painted `Paragraph.js:3710+` | Line numbering shipped (#609); hyphenation absent (§4, FID-L-02) |

### 5.2 Newly verified advantages, not previously recorded

| Ours | Evidence they lack it |
| --- | --- |
| **Grammar checking** — `tools.grammarCheck` (`main.js:12346`), `webapp/src/grammar.mjs`, guarded by `webapp/tests/grammar.test.mjs` | **The only "grammar" token in the whole `sdkjs` tree is a dead colour branch**: `sdkjs/word/Drawing/Graphics.js:2169` picks blue for a `handlerId` containing "grammar", and **nothing anywhere produces such a handler id**. Outside `resources/help/`, `web-apps` has zero "grammar" hits — no command, no string, no setting. They ship spelling (`spell.wasm`) and reserved the squiggle colour for grammar they never built |
| **Checklists** — `paragraph.list.checklist` (`main.js:12221`), ribbon `#checkList` | No checklist list kind in `sdkjs/word/Numbering/` or `locale/en.json` |
| **Explicit caption renumbering** — `reference.updateCaptionNumbers` (`main.js:8159`), offered when captions go stale rather than only on a field update | Theirs renumbers only through Update Table / field update (`Links.js:313-318`) |
| **Emoji picker** — `insert.emoji` (`main.js:12267`), and colour-glyph rendering (`docs/102`) | Their Symbol control is a symbol table only (`Toolbar.js:1039-1060`) |
| **Decimal and bar tab stops that actually lay out** | See §5.3 — theirs are preserved and never laid out |

### 5.3 Needs rewording — true about the UI, attackable as written

**"No decimal tab stops and no bar tab stops"** (`docs/105:449`). Their *enum* has all six
kinds — `sdkjs/common/commonDefines.js:2092`, exported `:5610-5616` (Bar, Center, Clear,
Decimal, Num, Right, Left), aliased `sdkjs/word/Editor/Paragraph/RunContent/Tab.js:38-44` —
so a reader with the source can rebut the sentence. What is unrebuttable:

> Decimal and bar tab stops are **preserved but never laid out**. Their own source carries
> the TODO — `Tab.js:63`, *"Реализовать табы по точке и с чертой (tab_Bar tab_Decimal)"* —
> the tab dialog offers three kinds (`ParagraphSettingsAdvanced.js:124-128`, default `:634`),
> the ruler's type selector cycles the same three (`sdkjs/word/Drawing/HtmlPage.js:1298-1310`),
> and the ruler **mis-paints** an imported decimal or bar tab as a centre tab because of a
> copy-paste bug, `else if (tab_Center === tab_Center)` (`sdkjs/word/Drawing/Rulers.js:2542-2557`).
> They do round-trip the value (`Serialize2.js:9585-9591`, `:2658`).

Ours is real: aligned on the decimal position at `crates/casual-doc-layout/src/tabs.rs:561`,
all six kinds typed (`properties.rs:229-243`), imported `casual-doc-import/src/properties.rs:49-50`,
exported `casual-doc-export/src/semantic.rs:7933-7934`.

### 5.4 Now false — and false in our own favour

**"ONLYOFFICE ships only 14 field codes, so the bar is low."** This appears in
`docs/106:273` and `docs/109:280` (RM-01). It is wrong twice over.

- **They have 18 field types**, not 14 —
  `sdkjs/word/Editor/Paragraph/ComplexFields/types.js:42-62`: MERGEFIELD, PAGENUM,
  PAGECOUNT, FORMTEXT, TOC, PAGEREF, ASK, REF, HYPERLINK, TIME, DATE, FORMULA, SEQ,
  STYLEREF, NOTEREF, ADDIN, FORMCHECKBOX, FORMDROPDOWN (plus `fieldtype_UNKNOWN` and the
  `PAGE`/`NUMPAGES` aliases at `:49-50`). Their instruction parser recognises **16 keywords
  plus the `=` formula form** (`ComplexFieldInstruction.js:1406-1473`), and the display
  dispatch agrees (`sdkjs/word/Editor/Field.js:564-634`).
- **The sentence flatters us.** We classify **10** field kinds plus `Other`
  (`crates/casual-doc-model/src/v1/body.rs:1496-1548`, keyword match `:1592-1627`) and
  *evaluate* far fewer: `PAGE` and `NUMPAGES` recompute at pagination
  (`casual-doc-layout/src/text.rs:346-353`, `flow.rs:4287-4289`) and `SEQ` renumbers on the
  caption path (`casual-doc-edit/src/references.rs:711`, `:744`); everything else is
  passthrough from the producer's cached result. `reference.updateFields` ships disabled for
  exactly this reason (`main.js:8166-8175`).

The honest framing is: **they recognise 16 field instructions and evaluate them; we classify
10 and recompute 3.** RM-01 is still the right work; its justification is not.

Also, one guard rail. **"No page-borders dialog" is only safe if it says *dialog*.** They
have no dialog — there is no `PageBordersDialog.js` beside `PageMarginsDialog.js`,
`PageSizeDialog.js`, `PageNumberingDlg.js`, `LineNumbersDialog.js` and `HyphenationDialog.js`,
and their only `put_Borders` call site is *paragraph* borders (`ParagraphSettingsAdvanced.js:812`
→ `sdkjs/word/api.js:3893-3901` `SetParagraphBorders`). But they **model, round-trip and
paint** `w:pgBorders`: `sdkjs/word/Editor/sections/sect-pr.js:265-331`, `Serialize2.js:401`,
`:1029`, `:2805`, JSON `fromToJSON.js:4855-4880`, and `DrawPageBorders` at
`sdkjs/word/Editor/Document.js:5519+`, handling both `offsetFrom=page` and `offsetFrom=text`.
So do not let this become a claim about page-border *support*: on that axis we are level, and
neither product lets a user author them.

## 6. Explicit non-goals — not gaps

| Their surface | Why it is not a gap |
| --- | --- |
| **Forms tab** (21 slots) | Their form designer targets PDF forms and the retired DOCXF/OFORM formats — `controller/Toolbar.js:4044-4045`, `Main.js:481` `canFeatureForms = asc_isSupportFeature("forms")`, `Main.js:602` `isFormCreator`. PDF editing and PDF forms are out of scope (`SKILL.md` §1) |
| **Spreadsheet features** — Insert Spreadsheet/OLE (`Toolbar.js:893`), their whole `cell/` engine, chart editing over the embedded XLSX | `opencalc` (`../sheets`) owns spreadsheets. The *insertion* of an OLE spreadsheet object stays in scope as preservation, not authoring |
| **Presentation features** | A future sibling owns these |
| **Sharing, Access Rights, Chat, Co-editing Mode, Version History as they build them** | All require their mandatory server. Collaboration is in scope (ADR-033, OT) but **no mandatory server** is a structural advantage to protect (`SKILL.md` §1), so the *shape* is a non-goal even where the capability is not |
| **Digital signatures** | `Main.js:1754` gates both kinds on `isDesktopApp && isOffline`. Not a web-parity gap; revisit only with a desktop shell (P1G-004) |
| **Print with preview, Quick print** | Desktop-only, and excluded on macOS desktop too (`Main.js:1735-1736`). Our print gap is OO-010, which is about a dialog, not about these |
| **Plugin marketplace, macros** | OO-017; the plugin ABI is an open decision (ADR-030) and their macros are per-document with no global library |
| **Mail Merge** | OO-013 stays P3. Theirs is licence-gated (`Main.js:1723`), `.xlsx`-only, portal-bound and capped at 100 recipients — a weak parity argument |

## 7. The gaps, ranked

Ranked by **what a real user reaches for in a word processor**, not by cost. "Needs" is the
cheapest honest classification.

| # | Gap | Needs | Existing row |
| ---: | --- | --- | --- |
| 1 | **Insert a page break** (and column break, and section break with 4 kinds) | **engine** — no break-insert operation exists among the 50; `BreakKind` is already modelled — then facade, then UI | **new** |
| 2 | **Show formatting marks** (¶, space dot, tab arrow, page-break rule) + hidden table borders | **engine** — a display-list layer; then UI + a chord | **new** |
| 3 | **Table of contents and table of figures**, with Add Text and Update Table | engine (paragraph-spanning `TOC` field) + UI | OO-001 (open; control already ships disabled) |
| 4 | **Paragraph direction LTR/RTL** | **facade + UI only** — the operation already carries `bidi` | **new** — and we ship `ar.json` |
| 5 | **Hyphenation** | engine (line breaker) + UI | FID-L-02, OO-006 |
| 6 | **Section/page properties with no UI**: gutter and gutter position, mirror margins, header/footer distance, page vertical alignment, page numbering format and start-at, page borders, page colour | **UI only** for header/footer distance + gutter (`setPageSetup` already takes them); **facade + UI** for page colour; **engine op field** for page borders, vertical alignment, `pgNumType` | partly in flight (a lane is building this); **new** for the remainder |
| 7 | **Advanced character formatting**: small caps, all caps, character spacing, character position | **engine** — `FormatDelta` carries none of the four — then facade, then UI. All four already lay out | **new** (distinct from OO-021's ligature clause) |
| 8 | **Object arrangement**: z-order, align (6), distribute (2), align-to page/margin/objects, group/ungroup | **engine** (no z-order or group operation) + UI. `layout.arrange.bringForward` already ships disabled with this reason | **new** |
| 9 | **Word count dialog and selection-scoped counts** | UI only — `documentStats` already exposes words/characters/paragraphs (`main.js:2855`) | HF-051 / OO-015 |
| 10 | **Recent files** | UI + a host storage contract | OO-002 remainder, RM-03 |
| 11 | **Note management**: convert to endnotes/footnotes, swap, delete all notes, note settings, go-to next/previous note | facade (delete-all; `RemoveNote` exists) + engine (convert/swap) + UI | **new** |
| 12 | **Multilevel list gallery and list settings** (number format, start-at, restart, follow-number-with, tab stop) | UI over the existing numbering model (`definitions.rs:220-248` carries level, start, `num_fmt`, `lvl_text`, `lvl_jc`, `suff`) | OO-021 — sharpen with these citations |
| 13 | **Print dialog**: range, duplex, colour/mono, margins, preview | UI + print plumbing. **Its "blocked-by RM-04" note is stale** — real-text PDF shipped | OO-010 |
| 14 | **Comment scope**: remove/resolve mine and all | UI only | **new** |
| 15 | **Header/footer "same as previous"** | facade + UI — `SetSectionRunningRef` exists | **new** |
| 16 | **View-tab breadth**: rulers show/hide, dark document, status bar / panel visibility | UI only | **new** |
| 17 | **Multiple-pages view** | UI + viewer layout | **new** (part of #16's row or its own; `105` §4.3 already notes it) |
| 18 | **Content control authoring** (7 types) | engine + UI; `w:sdt` already models, round-trips and paints checkbox state | OO-012 |
| 19 | **Document protection and password** (4 restriction levels) | model (`w:documentProtection`) + enforcement + UI. Signatures excluded — desktop-only there | OO-011 |
| 20 | **Compare and combine** | engine (offline transform) + UI | OO-007 |
| 21 | **Insert breadth**: Blank Page (needs #1), Text from File, text-to-table, horizontal line, vertical text box, Remove header/footer, page-number position grid | mixed; mostly UI once #1 lands | OO-021 — extend |
| 22 | **Navigation panel breadth**: promote/demote, insert heading before/after, expand-to-level | facade + UI | **new** (small) |
| 23 | **Theme colour schemes** (24+) and 8 interface themes vs our 2 | UI + theme model | OO-020 |
| 24 | **Text Art / WordArt** | engine (text paths — the open half of FID-L-10) + UI | FID-L-10 |
| 25 | **Equation editor** | engine + an authority ADR first (`99` §2) | OO-009 |
| 26 | **Charts and SmartArt** | engine, gated on Q3 | OO-014, FID-R-08 |
| 27 | **Merge Shapes** (5 boolean operations) | engine (path booleans) + UI | **new**, low |
| 28 | **Draw tab** (pen, highlighter, eraser) | engine (ink) + UI | OO-019 |
| 29 | **Collaboration surface**: sharing, presence, co-editing mode, chat, version history | ADR-033 / OT; chat and history are licence-gated on their side | HF-114, HF-068 |
| 30 | **Plugins and macros** | open ABI decision | OO-017 |

## 8. Existing rows that are now stale

**Do not re-file these under new ids.** Each needs a status or wording correction by whoever
owns the tracker.

| Row | What is wrong | Evidence |
| --- | --- | --- |
| `109` row 77 — **OO-005** "No captions and no cross-references", Open, blocked-by RM-01 | **Shipped.** Both commands are declared and enabled | `main.js:8135` (`reference.caption`, `requires: "bodyCaret"`), `:8147` (`reference.crossReference`, `requires: "caret"`); dialogs `webapp/src/caption_dialog.mjs`, `webapp/src/cross_reference_dialog.mjs`; specs `webapp/tests/e2e/references-captions.spec.mjs`, `webapp/tests/cross_reference_model.test.mjs`; commit `be26345` cites OO-005 by name |
| `109` row 90 — **FID-L-10** titled "Watermarks do not appear", Open | Title is false and the status disagrees with its own source row, which reads **"Partly fixed"**. The real remainder is general WordArt text paths | `105:314`; watermark ships: `crates/casual-doc-layout/src/watermark.rs`, painted `compose.rs:314`, authored `main.js:8084`; PRs #611/#612 fixed live watermark defects |
| `109` row 99 — **OO-010** "blocked-by RM-04 for real-text output" | **RM-04 is closed** — real-text PDF export shipped and is reachable from File ▸ Export as PDF | `109` "How it was settled" table, RM-04 row |
| `109` row 102 / `105:517` — **OO-020** "46 locales vs 1"; "18 bound chords here" | Both numbers are stale in our favour's opposite direction. **19 locales**, **36 chords** | `ls webapp/locales/` → 19; `grep -c 'chord:' webapp/src/keymap.mjs` → 36; theirs: 46 locale files, **147** named actions (`sdkjs/word/apiDefines.js:223-371`) |
| `109:280` — **RM-01** "ONLYOFFICE ships only 14 field codes, so the bar is low" (also `106:273`) | **False, and false in our favour.** 18 types / 16 recognised instructions there; 10 classified and 3 recomputed here | §5.4 |
| `105` §4.3 tab table — "OpenDoc ships **five**: Home, Insert, Table, View, Review"; "References — **Largest single IA gap**"; "Layout — Gap, and the cheapest one"; "OpenDoc lacks multi-page view **and zoom-to-100%**" | Four errors. We ship **8** tabs; References and Layout both exist; zoom-to-100% exists | `editor.html:83-107`; `#panelLayout` 13 controls, `#panelReferences` 9; `view.zoom.100` / `#viewZoomActual` (`ribbon_faces.mjs:143`) |
| `105` §4.4 **OO-002** "No New document, no recent files, no templates, no backstage" | Three of four shipped; only Recent files remains. `109` row 68 already says so — `105` does not | #542, #596; `123` §5.3–5.4 |
| `105:518` — **UX-005** "~23 of ~90" ribbon controls carry a command id | Superseded twice: the row's own 2026-09-20 recount said 39 of 109, and `ribbon_faces.mjs` measured **52 of 120** before landing the fix. **All 122 controls now carry `data-command` or `data-command-family`** | `webapp/src/ribbon_faces.mjs:1-13`, `:202`; guard `webapp/tests/e2e/ribbon-command-faces.spec.mjs` |
| `106` §4 "Where we already stand" | Stale in five cells: "Ribbon 5 tabs" (8), "97 command ids", "no PDF" (shipped), "no spell check" (shipped), "one locale" (19). Also "47-operation closed set" — the enum has **50** | `crates/casual-doc-edit/src/lib.rs:326-916`; `106` is an archive, so this is expected — but `106` §4 is cited as a measurement |

## 9. The rows to file

Id class, lane, priority and effort follow the conventions of `109` (`105` effort scale:
S = under a day, M = up to a week, L = more than a week). **Fifteen new rows**, each with the
`#7` rank it came from. Nothing here duplicates an open row; where an existing row covers the
work, §7 names it instead.

| Proposed id | Lane | Pri | Eff | Description, in the tracker's voice | §7 |
| --- | --- | --- | --- | --- | --- |
| OO-022 | Audit | P1 | M | No way to insert a page, column or section break — the operation set has no break-insert variant, so `Blank Page` is blocked behind it too | 1 |
| OO-023 | Audit | P1 | M | No formatting-marks view — no pilcrow, space dot, tab arrow or page-break rule, and no invisibles primitive in the display list | 2 |
| OO-024 | Audit | P1 | S | Paragraph direction is read-only: `bidi` imports, cascades, lays out and is diff-reported, but no setter reaches it — and we ship an Arabic UI locale | 4 |
| OO-025 | Audit | P2 | S | Section properties still with no UI after the page-setup lane: gutter position, mirror margins, page vertical alignment, page numbering format and start-at, page colour | 6 |
| OO-026 | Audit | P2 | S | Page borders are resolved, laid out and painted but no operation carries them, so they cannot be authored — and ONLYOFFICE has no dialog either, which makes this a lead rather than parity | 6 |
| OO-027 | Audit | P2 | M | `FormatDelta` cannot carry small caps, all caps, character spacing or character position, all four of which already lay out | 7 |
| OO-028 | Audit | P2 | M | No object arrangement: no z-order operation (so `layout.arrange.bringForward` ships disabled), no align, no distribute, no group/ungroup | 8 |
| OO-029 | Audit | P2 | S | Notes can be inserted and nothing else — no convert to endnote/footnote, no swap, no delete-all, no note settings, no go-to next/previous note | 11 |
| OO-030 | Audit | P2 | S | Comments have only the current scope — no "resolve mine", "resolve all", "remove mine" or "remove all", while changes already have both scopes | 14 |
| OO-031 | Audit | P2 | S | A section's header/footer cannot be linked to or unlinked from the previous section: `SetSectionRunningRef` is only used when creating a body | 15 |
| OO-032 | Audit | P2 | S | View offers nothing but compact ribbon — no rulers show/hide, no dark document, no status-bar or panel visibility | 16 |
| OO-033 | Audit | P2 | M | No multiple-pages view, so a reader cannot see spreads | 17 |
| OO-034 | Audit | P3 | S | The headings panel has no promote/demote, no insert-heading-before/after and no expand-to-level | 22 |
| OO-035 | Audit | P3 | M | No theme colour schemes, and two interface themes against eight | 23 |
| OO-036 | Audit | P3 | L | No shape boolean operations (union, combine, fragment, intersect, subtract) | 27 |

Plus two rows that are **not** ONLYOFFICE gaps but came out of the measurement (§11). They are
listed separately so the owner can file them in the Hotfix lane, where they belong:

| Proposed id | Lane | Pri | Eff | Description, in the tracker's voice |
| --- | --- | --- | --- | --- |
| HF-190 | Hotfix | P2 | S | The context menu wires five leaf capabilities under ids the registry does not answer (`link.add`, `comment.add`, `paragraph.bullets`, `paragraph.numbering`, `paragraph.properties`), so a cross-surface parity guard cannot see those rows and the menu is a second implementation free to drift — it already shipped the compact chrome with no list buttons and no Add comment (`docs/115`:314-323) |
| HF-191 | Hotfix | P3 | S | `ribbon_faces.mjs:1-13` and `ribbon-command-faces.spec.mjs:3-9` carry a hand-maintained "120 controls / References 7/7" count that is now 122 / 9-of-9, and it is the source of the figure quoted elsewhere |

### Corrections to apply to existing rows, not new rows

| Row | Change |
| --- | --- |
| OO-005 (`109` row 77) | **Close.** Captions and cross-references shipped; see §8 |
| FID-L-10 (`109` row 90) | **Restate to "WordArt text paths are not typed, so warped text does not paint"** and carry `105`'s "Partly fixed". The watermark half is done |
| OO-010 (`109` row 99) | Drop "blocked-by RM-04" — RM-04 closed. Keep the row |
| OO-020 (`109` row 102) | Re-measure in place: 46 locales vs **19**; **36** chords vs their **147** named actions (`sdkjs/word/apiDefines.js:223-371`) |
| OO-021 (`109` row 103) | Extend with the citations in §2.3/§2.4 and add: horizontal line, Text from File, text-to-table, vertical text box, Remove header/footer, page-number position grid. Move its ligature clause to sit beside OO-027 |
| RM-01 (`109` row 122, `106:273`) | **Fix the justification.** They recognise 16 field instructions (18 types); we classify 10 and recompute 3. The work stands; "the bar is low" does not |
| UX-005 (`105:518`) | Note the measurement is now historical: all 122 ribbon controls are declared |
| `105` §4.2 decimal-tab row | Reword per §5.3 to "preserved but never laid out", citing `Tab.js:63` |
| `105` §4.2 page-borders row | Constrain to "no page-borders **dialog**" and record that they model, round-trip and paint `w:pgBorders` |
| `105` §4.3 tab table | Correct four cells per §8 |
| `105` §4.2 | Add the two new advantages from §5.2 — grammar checking and checklists |

## 10. How our side was verified, and what is still open

**Verified by reading the declaration and its enablement, and — where a guard exists — by
driving the control in a browser. Not by assuming a button works.**

What was run: `cd webapp && ./build.sh` (exit 0; `pkg/casual_doc_wasm_bg.wasm` 21.4 MB,
`demo.docx` and `sample.docx` staged — all three untracked, so this is a prerequisite, not
hygiene), `npm ci` in the worktree, two throwaway Playwright probes on an explicit
`PW_PORT=39411` (both deleted), then the repo's own guards:
`node --test tests/keymap.test.mjs tests/ribbon_faces.test.mjs tests/menu_taxonomy.test.mjs`
(33/33) and `npx playwright test one-axis-navigation ribbon-command-faces
command-shortcut-coverage` (19/19 in 4.7 min, no retries).

- **The 122 total was measured twice, two ways, and agrees** — live DOM with the guard's own
  selector, and statically over the markup (§0). Cross-checked against the declaration tables:
  `HOME_FACES` 41, `VIEW_FACES` 8, `TABLE_FACES` 19 (read in node from
  `webapp/src/ribbon_faces.mjs`), `INSERT_SURFACE` 17, `LAYOUT_SURFACE` 15,
  `REFERENCE_SURFACE` 5, `REVIEW_SURFACE` 15. Table sizes do not equal panel sizes because
  one command can stamp faces on two panels — four Insert commands also stamp References
  buttons, and two Layout rows render on the Insert band.
- **No dead ids.** All 106 distinct ribbon ids are in the registry; all 35 keymap commands
  are in the registry; all 9 families have live members. **The `⌘⌥M` → `comment.add` defect
  cited as the cautionary example is already fixed** — `webapp/src/keymap.mjs:151` now binds
  `review.comment`, the fix is documented in place at `:146-150`, and
  `command-shortcut-coverage.spec.mjs:97` asserts the chord adds a comment.
- **Only four commands are palette-only**, and the repo already names all four
  (`webapp/tests/e2e/one-axis-navigation.spec.mjs:291-308`): `object.selectNext` and
  `object.selectPrevious` by design (Tab and Shift+Tab *are* the affordance), and
  `review.acceptAtCaret` / `review.rejectAtCaret`, which are the recorded gap HF-179 — we
  have the Accept/Reject faces and no split-button dropdown behind them.
- Five controls are recorded as **disabled with a reason** rather than present:
  `layout.arrange.bringForward` (`main.js:8108-8116`), `reference.tableOfContents`
  (`:8120-8127`) and `reference.updateFields` (`:8166-8175`) permanently, via the
  `requires: "missing"` contract (`main.js:8061`, `:8186`); `layout.arrange.wrap` and
  `layout.arrange.position` situationally, on selection.
- **Asymmetric evidence, stated plainly.** `ribbon-command-faces.spec.mjs:261` drives **each**
  Home, View and Table control with a real pointer and asserts the same observable effect as
  running its id from the palette — so those 68 controls are proven to *act*. There is **no
  such guard for Insert, Layout, References or Review**: those 54 controls are proven to
  *name a live command*, not proven to act. Every "Present" verdict on those four tabs should
  be read at that strength.
- Engine claims were checked at three levels: the model (`crates/casual-doc-model`), whether
  layout or render consumes it, and whether an `Operation` and a WASM export reach it. The
  §4.4-versus-§4.10 split exists precisely because that third level differs: page margins
  are carried by an existing operation and character spacing is not.

**Open questions, recorded rather than resolved:**

1. **Whether their spell check has features ours lacks** was not adjudicated — only that
   grammar is absent on their side. `spell.wasm` and our SCOWL path were not compared
   option-for-option.
2. **The Insert-tab count asymmetry** (their 20, our 17) is partly a slot-versus-button
   artefact: their Breaks and Image buttons are injected into two panels each. The tab-level
   numbers in §1 are the reproducible ones.
3. **Whether #16 and #17 (View breadth and multiple pages) should be one row** is an owner
   call; they are filed separately because multiple-pages view touches the viewer's layout
   and the rest are class toggles.
4. **Whether a formatting-marks layer belongs in layout or in the renderer** is a design
   question this document does not settle. It affects whether OO-023 is M or L.
5. `docs/106` §4's measurement table is stale in five cells (§8). `106` is an archive, but it
   is the document the roadmap's own scale estimate rests on.

## 11. Found while measuring — not ONLYOFFICE gaps

Three findings that fall out of the measurement and belong to whoever owns the file. **Nothing
here was changed** — three lanes are live in `webapp/src/main.js`, `editor.html`, the locales,
`crates/**` and `packages/**`, and this document is read-only in all of them.

### 11.1 The context menu has its own command-id namespace — a real defect

Five *leaf capabilities* are wired twice, under an id the registry does not answer:

| Context-menu `data-command-id` | Declared | Registry id for the same capability |
| --- | --- | --- |
| `link.add` | `main.js:7421` — the comment at `:7425` admits it: *"The same capability as `insert.link` under the annotate surface's own id"* | `insert.link` |
| `comment.add` | `main.js:7447`, `run: () => openReviewComposer()` | `review.comment` |
| `paragraph.bullets` | `main.js:7473`, its own `doc.toggleList(…)` call | `paragraph.list.bullet` |
| `paragraph.numbering` | `main.js:7481`, its own `doc.toggleList(…)` call | `paragraph.list.numbered` |
| `paragraph.properties` | `main.js:7535` | `layout.paragraph` |

(Six further divergent ids — `format.menu`, `paragraph.list`, `table.insert`, `table.delete`,
`table.select`, `table.layout` — are submenu parents, which is defensible: they are choosers,
not commands.)

Two consequences. First, any cross-surface parity guard asking *"is `insert.link` on ≥2
surfaces?"* **cannot see the context-menu row**, so `docs/105` UX-004's "every capability
reachable from ≥2 surfaces" is measured against an incomplete surface list. Second, this is a
second implementation free to drift, and **it has already shipped a defect**:
`docs/115-STYLES-CONTROL-AND-DROPDOWN-CONSISTENCY.md:314-323` records that the compact
toolbar's layout table named `paragraph.bullets`, `paragraph.numbering` and `comment.add`,
`renderCompactToolbar`'s `if (!command) continue` dropped all three silently, and the compact
chrome shipped **with no bulleted list, no numbered list and no Add-comment button**.

Worse, the divergent names are *pinned by tests*: `webapp/tests/e2e/context-menu.spec.mjs:69-70`
asserts `comment.add` and `link.add`, so a rename has to move the guard too. Not tracked as an
open row anywhere. §9 proposes it as HF-190.

### 11.2 Two committed comments carry the stale count

`webapp/src/ribbon_faces.mjs:1-13` and `webapp/tests/e2e/ribbon-command-faces.spec.mjs:3-9`
both read *"52 of 120 visible ribbon controls … Insert (17/17), Layout (13/13), References
(7/7) and Review (15/15)"*. The measurement is now **122**, and References is **9/9**.
`SKILL.md` §8 — *counts in docs must be derived, not hand-maintained* — applies to comments
that get cited as evidence, and this one is: it is where the "120 controls / 7 tabs /
`data-command` on 111" figure in circulation comes from. A one-line comment fix belongs in a
webapp-lane PR, not here.

### 11.3 A claim this round got wrong, corrected

A sub-lane reported that `main.js` now has exports, citing `main.js:16589` and concluding that
`SKILL.md` §11 and HF-085's status cell were stale. **That is wrong, and it is recorded here
rather than quietly dropped.** Line 16589 passes `editorCommands` as a *property of an options
object* into `createCompactToolbar` — dependency injection, not an ES module export.
`grep -n '^export ' webapp/src/main.js` returns nothing, and none of the 26 occurrences of the
string `export` in the file is an export statement. **`main.js` still has zero exports;
`SKILL.md` §11, `command_taxonomy.mjs:4-5` and HF-085 are all still accurate.**
