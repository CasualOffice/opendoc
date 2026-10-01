# 130 — ONLYOFFICE toolbar gap analysis

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
(`SKILL.md` §9.4), so every "we have this" claim below names where the command is *declared*
and — where it matters — whether it is `enabled` or ships disabled-with-a-reason.

**CORRECTED 2026-10-01: no ribbon control ships permanently disabled any more.** This paragraph
used to name three — `layout.arrange.bringForward`, `reference.tableOfContents` and
`reference.updateFields` — as permanently disabled through the `requires: "missing"` contract.
All three are live, and `requires: "missing"` now appears **zero** times in `webapp/src/`
(the machinery survives in `ribbon_surface.mjs` for the next such case and no row uses it).
Each is gated on a real precondition, which is the never-a-dead-control contract working
rather than a gap: `bringForward` on `requires: "object"`, `tableOfContents` on
`requires: "bodyCaret"`, `updateFields` on `requires: "tocField"`. §10 and the three rows
below carry the detail, and `SKILL` §9 rule 6 is why this is corrected rather than left:
understating is also false, and a reason left standing after the gap it describes has closed
is the same lie as a claim that is too generous.

**Our side is cited by grep anchor, not by line number, and that is a correction.** The first
draft of this document cited `webapp/src/main.js` by line in about thirty places. `main.js` is
16,617 lines and grew by 666 in the day between that draft and this revision, so every one of
those citations was stale within twenty-four hours — including the ones a reader would use to
check a "we already ship this" verdict. A line number into that file is not a reproducible
citation (`SKILL.md` §9.1), so each is now the string a reader can `grep` for: `main.js`,
`id: "reference.caption"`. Their side keeps `file:line`, because the reference checkouts are
pinned and do not move under us.

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
`#ribbonOverflowBtn` is excluded: it sits outside every panel and is `hidden`.

Measured two independent ways, which now agree exactly: in a live Chromium after boot with the
selector above, and statically over `webapp/editor.html`. The static recipe needs three
exclusions, and the first draft of this document got the third one wrong — its static pass
returned one control more than the browser did:

1. **Strip HTML comments.** The Styles group's comment contains a literal `<select>`.
2. **Bound each panel by the next `<div id="panel…">` in *document* order**, not by a
   hand-written list order, and drop `panelFile` — the File page is a route, not a ribbon tab,
   and it carries 271 controls that would swamp the count.
3. **Drop `#ribbonOverflowBtn`.** It is *outside* `#panelReview` in the DOM but *between*
   `#panelReview` and `#panelFile` in the file, so a naive slice charges it to Review. That is
   the entire gap between the first draft's static 123 and its live 122, and the reason this
   recipe is written out rather than described.

```sh
cd webapp && python3 - <<'PY'
import re
html = re.sub(r'<!--.*?-->', '', open('editor.html').read(), flags=re.S)
panels = [(m.start(), m.group(1)) for m in re.finditer(r'<div id="(panel\w+)"', html)]
total = 0
for i, (start, name) in enumerate(panels):
    if name == 'panelFile':
        continue
    end = panels[i + 1][0] if i + 1 < len(panels) else len(html)
    controls = [m for m in re.finditer(r'<(button|select|input)\b([^>]*)', html[start:end])
                if 'ribbonOverflowBtn' not in m.group(2)]
    print(name, len(controls))
    total += len(controls)
print('total', total)
PY
```

The live measurement is the authority, because a control can be in the markup and hidden. It is
the guard's own `controlsOf()` helper, run over the seven tabs with the caret in a table and a
range selected — the state that makes the most of the ribbon reachable at once:

```
PROBE_PER_TAB {"home":41,"insert":18,"layout":13,"references":9,"view":8,"review":15,"table":19}
PROBE_TOTAL 123  data-command 114  family 9  unclassified 0  distinct-ids 107
```

**Every count in this document is stamped with the commit it was taken at**, because the ribbon
grows: measured at `f8233e0` (2026-09-27), and the same recipe returns **122** at `2d90b02`
earlier the same day, where the first draft was written. The one control between them is
`#headerFooterSettingsBtn` on Insert — see §4.4, which the same landing partly closes.

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

| | ONLYOFFICE document editor, the reference checkout (§12 — the release is **not** recorded anywhere in their tree, so no version is claimed here) | OpenDoc at `f8233e0` |
| --- | --- | --- |
| Tabs declared | **13** — 12 panels + File (`haspanel:false`, `view/Toolbar.js:182`) | **8** — File (a route) + 6 standing panels + 1 contextual |
| Tabs a cloud `.docx` editor sees | 9–10 (File, Home, Insert, Draw, Layout, References, Collaboration, Protection, View, and Plugins when one is installed) | 7 (File, Home, Insert, Layout, References, Review, View) + Table when the caret is in a table |
| All declared ribbon slots, every panel | **206** (186 panel slots + 19 File-menu items + 1 text label) | 123 |
| Reachable interactive ribbon controls | **168** | 123 |
| **Steady-state interactive controls a cloud `.docx` editor sees** | **120** | **123** |
| Per tab, steady state | Home 31 · Insert 20 · Draw 5 · Layout 20 · References 10 · Collaboration 16 · Protection 2 · View 16 | Home 41 · Insert 18 · Layout 13 · References 9 · Table 19 · View 8 · Review 15 |
| File-menu / backstage entries | 19 static + 4 conditional | 12 File-page rows (4 direct, 8 panes) — `webapp/src/command_taxonomy.mjs:46-71` |
| Named keyboard actions | **146** (`sdkjs/word/apiDefines.js:223-371` — ids run 1…147 with 142 unused, so the highest id is not the count; the first draft published 147 and contradicted itself in the same cell) | 36 chords over 35 distinct commands — `webapp/src/keymap.mjs` (`grep -c 'chord:'`; `⌘⇧Z` and `⌘Y` both run `edit.redo`) |
| UI locales | 46 (`apps/documenteditor/main/locale/`) | 19 (`webapp/locales/`) |

Of our 123 controls, **114 carry `data-command`, 9 carry `data-command-family`, and 0 are
unclassified**, naming **107 distinct command ids** — the 7 duplicate faces are deliberate
(indent ± on Home and Layout, Find and Replace both naming `edit.find`, Bookmark and Field on
Insert and References, Comment on Insert and Review, the comments pane on View and Review). None
of those attributes is in the markup: they are stamped at boot by
`stampRibbonFaces` (`webapp/src/ribbon_faces.mjs`) and from
four declaration tables in `main.js` (`INSERT_SURFACE`, `LAYOUT_SURFACE`,
`REFERENCE_SURFACE`, `REVIEW_SURFACE`), applied at `main.js`, `entry.run && !entry.ownsClick`.

**This is the same measurement as `docs/105` UX-005's, one lane later.** UX-005 closed with
*"120 ribbon controls, 111 with `data-command` plus 9 declaring a `data-command-family`, 0
unclassified"*; three controls have landed on the ribbon since. The two figures do not
disagree — they are the same recipe at two commits, which is exactly why the recipe and the
commit are printed above and the guard
(`webapp/tests/e2e/ribbon-command-faces.spec.mjs`) deliberately asserts *nothing is
unclassified* rather than pinning a number. Treat 111-of-120 as UX-005's closing measurement
and 114-of-123 as this document's; do not copy either forward without re-running the probe.

**Our command registry has no single size.** `editorCommands()` (`main.js`, `function editorCommands`) is a literal array plus ~15 generated blocks that read live state — export formats,
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

### Normalising the two counting rules — 123 against 120 is not apples to apples

The two rules differ on split buttons: theirs counts a split button once, ours counts each
half. That asymmetry flatters us, so both normalisations are given:

| Rule | ONLYOFFICE | OpenDoc |
| --- | ---: | ---: |
| A split button is **one** control (their rule) | 120 | **118** |
| Each half is its own control (our rule) | **146** | 123 |

Our surplus under their rule is 5: `#underline`/`#underlineMenuBtn`,
`#bulletList`/`#bulletListMenuBtn`, `#numberedList`/`#numberedListMenuBtn`,
`#textColorApply`/`#textColor`, `#highlightApply`/`#highlight`. Their surplus under ours is
**26** split buttons visible in the steady state. **The first draft's citation list for this
number was wrong in nine of its entries** — it cited lock-array lines instead of the
declarations, attributed three Draw pens to `createPen` (which is a later hint-setter, not
the constructor), and listed three conditional splits as if they were unconditional. The
count survives the correction; the evidence is re-derived here, all of it from their own
`split:` flags, so `grep -n 'split *[:=]'` over these five files reproduces it:

| Where | Split buttons | Lines |
| --- | ---: | --- |
| `view/Toolbar.js` — highlight, font colour, paragraph shading, borders, formatting marks, bullets, numbering, text box, equation, bring forward, send backward | 11 | `:468`, `:491`, `:507`, `:525`, `:792`, `:819`, `:836`, `:925`, `:1030`, `:1660`, `:1672` |
| `view/Toolbar.js` — Breaks, injected with `split=true` (6th argument of `Common.Utils.injectButtons`, `apps/common/main/lib/util/utils.js:1155`) into **two** panels, Insert and Layout | 2 | `:2245` |
| `view/Links.js` — Table of contents and Footnote, both injected with `split=true`; Update table declared with `split: true` | 3 | `:172`, `:175`, `:189` |
| `lib/view/ReviewChanges.js` — Compare, Combine, Remove comment, Resolve comment | 4 | `:317`, `:330`, `:501`, `:514` |
| `lib/view/ReviewChanges.js` — Accept, Reject (`split: !canUseReviewPermissions`) and Track changes (`split: !isReviewOnly`). Both conditions hold in the default cloud steady state: `canUseReviewPermissions` needs `canLicense` **and** a configured `reviewGroups` (`controller/Main.js:1816`) | 3 | `:290`, `:303`, `:345` |
| `lib/view/Draw.js` — the `penOptions.forEach` at `:113-135` constructs one `split: true` button per preset, and `penOptions` has three: two pens and a highlighter | 3 | `:127` |

Print and Paste are excluded — both are in the static strip and desktop-gated
(`:202`, `:287`, `:1900`). `Protection.js:151` (Signature) is excluded as desktop+offline-only.
`ReviewChanges.js:853`, `:1134` and `:1155` are excluded because they are not ribbon controls:
`:853` is the status-bar review button and the other two are inside the floating review popover
(`this.$window.find('#id-review-button-accept')`). `Links.js`'s Table of Figures and its Update
(`:248`, `:260`) are **not** split, which is why they are absent from the table.

**So the real finding is sharper than a count.** Widget for widget the two ribbons are the
same size — 120 against 118 — but **26 of their widgets pack a variant or scope menu behind
the main action, against 5 of ours.** That is the density the owner meant by *"their design is
tried and tested"*: Accept carries current-versus-all, Remove comment carries
current/mine/all, Breaks carries page/column/section, Bring Forward carries
front-versus-forward. We ship the same number of *places to click*, and our answer to "and the
variants?" is usually a separate button or nothing at all.

No arithmetic is offered for *how much* less capability that is, and an earlier draft of this
paragraph claimed "roughly 20% less". That figure had no derivation — the menus behind those 26
buttons hold between two and nine items each and several of the items are ones we ship
elsewhere, so the honest statement is the 26-against-5 structural one and nothing more.

The composition gap sits on top of that. Two structural differences carry it:

1. **They spread formatting across a 10-panel right sidebar** (`view/RightMenu.js:117-321`)
   and ship almost no contextual ribbon — no Table Design, no Picture Format, no Shape
   Format. We ship a contextual Table tab *and* property panels, so our information
   architecture is not behind; it is differently shaped.
2. **Their breadth is concentrated in three areas we have not built at all**: document
   flow authoring (breaks, hyphenation, TOC/TOF), object arrangement (align, distribute,
   group, merge shapes, z-order), and the collaboration/protection surface. Those, not the
   tab count, are the gap.

Near-parity on widget count is also the reason this document ranks by *what a user reaches
for* rather than by control count: reaching their total by adding buttons would not close a
single one of the P1 rows in §7. **The split-button finding, by contrast, is directly actionable** — it says that
a disproportionate amount of their apparent breadth is scope and variant menus behind controls
we already ship, and several §7 rows (comment scope, note management, accept/reject at caret,
bring-forward-versus-front) are exactly that shape.

### The row census — derived from the verdict column, not counted by hand

**The single most important number in this document is not a control count — it is how many of
their controls we genuinely lack, as against how many we ship somewhere else.** A capability
reachable from exactly one surface is this repository's most expensive recurring defect
(`docs/105` UX-004), and a *reachability* row is an afternoon's work where a *missing* row can
be a quarter's. The first draft mixed the two in free prose, so no census was possible. Every
verdict cell in §2 now begins with one of seven fixed tokens, and gap rows carry a `needs`
classification, so the census below is regenerated rather than maintained:

```sh
python3 - <<'PY'
import glob, re
from collections import Counter
# Glob, not a literal path: this document's number moves when the docs/ series is
# renumbered, and a recipe that stops running after a rename is a hand-maintained
# count with extra steps.
doc = open(glob.glob('docs/*-ONLYOFFICE-TOOLBAR-GAP-ANALYSIS.md')[0]).read().split('\n')
start = next(i for i, l in enumerate(doc) if l.startswith('## 2. Tab-by-tab'))
end = next(i for i, l in enumerate(doc) if l.startswith('## 3.'))
verdicts, needs = Counter(), Counter()
for line in doc[start:end]:
    if not line.startswith('|'):
        continue
    cells = [c.strip() for c in line.strip().strip('|').split('|')]
    if len(cells) < 2 or cells[-1].startswith('---') or cells[-1] == 'Verdict':
        continue
    m = re.match(r'\*\*(\w[\w-]*)\*\*', cells[-1])
    verdicts[m.group(1) if m else cells[-1][:12]] += 1
    n = re.search(r'needs \*\*([^*]+)\*\*', cells[-1])
    if n:
        needs[n.group(1)] += 1
print(dict(verdicts)); print(dict(needs))
PY
```

At `f8233e0` that prints, over **119** compared rows:

| Verdict | Rows | What it means |
| --- | ---: | --- |
| **Present** | 39 | we ship it, on this surface, at parity |
| **Absent** | 43 | we do not ship the capability anywhere |
| **Weaker** | 19 | we ship the capability and less of it — usually their variant menu is missing |
| **Reachable** | 9 | **we ship the capability, just not from this control.** These are reachability rows, not missing-feature rows |
| **Ahead** | 4 | on the ribbon, ours only. §5 carries the full advantage list, which is longer |
| **Excluded** | 3 | desktop-only or removed in edit mode on their side — not a web-parity gap |
| **Non-goal** | 1 | the Forms tab (§6) |
| *(one placeholder row)* | 1 | the `reference.updateFields` note, which has no counterpart to compare |

And the needs split over the 62 gap rows (43 Absent + 19 Weaker):

| Needs | Rows |
| --- | ---: |
| **engine** (a new operation, or a layout/render capability) | 26 |
| **UI** only | 17 |
| **facade + UI** (a WASM export over an operation that exists) | 5 |
| **UI + host** (needs a host contract: storage, a URL fetch) | 3 |
| **host** only (sharing, chat, access rights — their server's shape, §6) | 3 |
| **model + engine** (document protection and password) | 2 |
| **open decision** (plugin/macro ABI, ADR-030) | 2 |
| **engine + UI**, **facade + engine**, **engine + host**, **model + UI** | 1 each |

**Read it this way.** Their ribbon has 120 steady-state controls; 9 of the rows we might have
called gaps are capabilities we already ship from another surface, and **17 of the 62 gap rows are
UI work over an engine that is already done** — with 5 more needing only a WASM export over an
operation that exists. That is 22 of 62 reachable without touching a crate, plus the 9
reachability rows on top. **The 26 rows marked `engine` are the real deficit**, and §7 ranks
them first.

**Where the census is weakest:** the `needs` classification is this document's own judgement
for every row not covered by §4, and §4 covers ten themes. A row marked `engine` because no
operation exists was checked against `crates/casual-doc-edit/src/lib.rs`; a row marked `UI` was
checked for a model and a layout consumer. Rows where neither check was decisive are named in
§12 rather than guessed at.

## 2. Tab-by-tab comparison

Per tab: what they offer (cited), our status, and our command id where one exists. Every verdict
cell starts with one of **Present** / **Weaker** / **Reachable** / **Absent** / **Ahead** /
**Excluded** / **Non-goal**, and **Absent** and **Weaker** rows carry `needs` — so the census
above can be regenerated and this table cannot drift away from it silently.

### 2.1 File

Theirs is a full-screen backstage (`view/FileMenu.js:100-327`, panels wired at `:409-413`);
every item is an entry point, so a capability we reach another way is not behind.

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| Save | `FileMenu.js:111-121` | `file.save` (`main.js`, `id: "file.save"`) | **Present** |
| Download As | `FileMenu.js:139-148` | Export pane, format tiles (`123` §5.3) + `exportCommands` (`webapp/src/export_commands.mjs`) | **Present** |
| Save Copy As | `FileMenu.js:150-159` | Export pane | **Reachable** |
| Create New + templates | `FileMenu.js:237-246` | `file.new` (`main.js`, `id: "file.new"`), four templates (`123` §5.4) | **Present** |
| **Open Recent** | `FileMenu.js:227-235` | — | **Absent** · needs **UI + host** — OO-002 remainder |
| Print | `FileMenu.js:183-193` | `window.print()` over rasters | **Weaker** · needs **UI** — OO-010 |
| Print with preview | `FileMenu.js:171-181` | — | **Excluded** — desktop-only (`Main.js:1735`) |
| Rename | `FileMenu.js:195-205` | — | **Absent** · needs **UI + host** — minor |
| **Protect** | `FileMenu.js:211-221` | — | **Absent** · needs **model + engine** — OO-011 |
| **Version History** | `FileMenu.js:270-283` | — | **Absent** · needs **engine** — HF-068, and licence-gated on their side |
| Info / document properties | `FileMenu.js:248-257` | Document properties pane | **Present** |
| Advanced Settings | `FileMenu.js:285-294` | Settings pane | **Present** |
| Access Rights | `FileMenu.js:259-268` | — | **Absent** · needs **host** — needs a host contract, not a feature |
| Help | `FileMenu.js:296-305` | Keyboard shortcuts + About panes | **Weaker** · needs **UI** |
| Open / Close / Switch to Mobile | `FileMenu.js:508-572` | — | **Excluded** — desktop-only |

**`docs/105` §4.3's "File … Gap — UX-011, OO-002, OO-011" is stale**: the backstage, New and
templates shipped in #596/#542. Recent files, Protect and History are the remainder.

### 2.2 Home — theirs 31, ours 41

| Their group | Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- | --- |
| Font | font name, font size, grow, shrink | `Toolbar.js:1750`, `:1739`, `:340`, `:354` | `format.family`, `format.size`, `format.grow`, `format.shrink` | **Present** |
|  | Change Case (5 items) | `:533-550` | `format.case.*` | **Present** |
|  | bold / italic / underline / strike / super / sub | `:368`–`:453` | `format.bold` … `format.subscript` | **Present** |
|  | highlight (16 fixed colours + No Fill) | `:460-483`, palette `:3296-3316` | `format.highlight` | **Present** |
|  | font colour (auto + eyedropper) | `:486-498` | `format.color` | **Weaker** · needs **UI** — no eyedropper |
|  | Clear Style | `:1575-1583` | `format.clear` | **Present** |
| Paragraph | bullets (9 library bullets, change level, list settings) | `:811-824`, picker `:3152-3173`, menu `:2696-2720` | `paragraph.list.bullet`, `paragraph.listFormat.bullet` | **Weaker** · needs **UI** — **no list-settings dialog** |
|  | numbering (presets from `resources/numbering/numbering-lists.json`, change level, settings) | `:828-841`, `:2727-2750` | `paragraph.list.numbered`, `paragraph.listFormat.` | **Weaker** · needs **UI** |
|  | **multilevel list gallery** | `:845-855`, picker `:3243-3254` | — | **Absent** · needs **UI** — OO-021 |
|  | indent ± | `:617-638` | `paragraph.indent.decrease/increase` — and these change *list level* inside a list (`main.js`, `applyIndentDelta`) | **Present** — list level is reachable differently |
|  | line spacing (6 presets + before/after + Options) | `:645-671` | `paragraph.spacing.` | **Present** |
|  | **text direction LTR/RTL** | `:674-691` | — | **Absent** · needs **facade + UI** — §4.1 |
|  | align left/centre/right/justify | `:553-610` | `paragraph.align.*` | **Present** |
|  | **show formatting marks** (split: hidden chars / hidden table borders) | `:786-804` | — | **Absent** · needs **engine + UI** — §4.2 |
|  | paragraph shading | `:501-514` | Paragraph properties ▸ Fill (`editor.html:1897`+) | **Reachable** |
|  | paragraph borders (10 edge presets, 7 widths, colour, **horizontal line**) | `:518-530`, menu `:2836-2951`, horizontal line `:2828-2833` | Paragraph properties ▸ Shading & borders | **Reachable** — **horizontal line absent** |
| Styles | gallery + per-style update/delete/restore/restore-all/delete-all; New style from selection | `:1770-1838`, `:1869-1879`, `:1762-1766` | `style.` chooser, `style.updateFromSelection` (`main.js`, `id: "style.updateFromSelection"`), `style.createFromSelection` (`id: "style.createFromSelection"`) | **Present** — ours is a curated short list by design (`docs/115`) |
| Editing | Replace, Select All | `:326-333`, `:312-319` | `edit.find` (both Find and Replace are faces of it), `edit.selectAll` | **Present** |
| — | — | — | **checklist** (`paragraph.list.checklist`, `main.js`, `id: "paragraph.list.checklist"`), **restart / continue numbering** (`paragraph.list.restart`, `paragraph.list.continue`), **format painter** (`format.painter`) | **Ahead** — §5 |

### 2.3 Insert — theirs 20, ours 18

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| **Blank Page** | `Toolbar.js:979-989` | — | **Absent** · needs **UI** — unblocked by #649; two page breaks, no engine work (§4.3) |
| **Breaks** — page, column, section ▸ next page / continuous / even / odd | injected `:2245-2249`; menu `:2382-2388`; section submenu `:2372-2380` | `insertBreak` / `insertSectionBreak` (facade, no surface yet) | **Absent** · needs **UI** — engine shipped in #649 (§4.3) |
| Table (8×10 picker, custom, draw, erase, text-to-table, **Insert Spreadsheet**) | `:878-899`, picker `:3280-3286` | `insert.table`, and `table.*` on the contextual tab | **Weaker** · needs **engine** — no draw/erase table, no OLE spreadsheet; text-to-table absent |
| Image (file / URL / storage) | `HeaderFooterTab.js:281-283`, menu `:326-331` | `insert.image` | **Weaker** · needs **UI + host** — file only |
| Shape | `:992-1005` | `insert.shape` | **Present** |
| SmartArt (159 layouts) | `:1008-1020`, sections `:2959-3025` | — | **Absent** · needs **engine** — OO-014; their editing is formatting-only — §5 |
| Chart | `:902-914`, picker `:2789-2807` | — | **Absent** · needs **engine** — OO-014 / FID-R-08 |
| Comment | `controller/Toolbar.js:4085-4089` | `review.comment` (on Review) | **Reachable** |
| Link | `Links.js:178-180` | `insert.link` | **Present** |
| Header & Footer (edit/remove header, edit/remove footer) | `HeaderFooterTab.js:261-262`, menu `:343-351` | `insert.header`, `insert.footer` | **Weaker** · needs **facade + UI** — **no Remove header/footer**, though `Operation::RemoveHeaderFooterBody` already exists |
| Page Number (6 positions, current position, of-pages, **Page Numbering** format dialog) | `HeaderFooterTab.js:265-266`, menu `:359-380`, positions `:394-401` | `insert.field.page`, and the numbering format and start-at now in Header and footer settings (`#headerFooterSettingsBtn`, `webapp/src/header_footer_settings.mjs`) | **Weaker** · needs **UI** — no position grid; the **numbering format dialog shipped** after the first draft (§4.4) |
| Header-from-top / footer-from-bottom spinners and Different odd/even / first page (theirs are on the contextual Header & Footer tab, `HeaderFooterTab.js:65-70`, `:84`) | `HeaderFooterTab.js:65-70` | `#headerFooterSettingsBtn` — one dialog carrying header/footer distance, gutter, odd/even, first page, page-number format and start-at, and a disabled Link-to-previous | **Present** — landed between the first draft and this revision, which is the one control that moved our total from 122 to 123 |
| Text Box (horizontal / vertical) | `:917-931`, menu `:3049-3070` | `insert.textbox` | **Weaker** · needs **engine** — no vertical text box |
| **Text Art** | `:934-951`, picker `:3027-3047` | — | **Absent** · needs **engine** |
| Drop Cap (none / in text / in margin / advanced) | `:1063-1106` | `insert.dropCap` | **Present** — advanced options remain (OO-006) |
| Date & Time | `HeaderFooterTab.js:269-271` | `insert.field.date` | **Present** |
| **Text from File** (local / URL / storage) | `:954-965`, menu `:2611-2617` | — | **Absent** · needs **facade + UI** |
| Field | `HeaderFooterTab.js:275-277` | `insert.field` (dialog, `field_kinds.mjs`) | **Present** |
| Equation | `:1023-1036` | — | **Absent** · needs **engine** — OO-009 |
| Symbol (25 recents + full table) | `:1039-1060`, `:3073-3099` | `insert.symbol`, **`insert.emoji`** | **Present** — emoji picker is ours only |
| **Content Controls** (7 types + edit + highlight) | `:1109-1192` | — | **Absent** · needs **engine** — OO-012 |

### 2.4 Layout — theirs 20 interactive (+4 labels), ours 13

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| Margins (4 presets + last custom + custom) | `Toolbar.js:1301-1352` | `layout.margins` → Page setup ▸ Margins | **Present** |
| Orientation | `:1257-1288` | `layout.orientation` | **Present** |
| Size (13 presets + custom) | `:1362-1484` | `layout.size` | **Weaker** · needs **UI** — fewer presets |
| Columns (5 presets + custom) | `:1196-1254` | `layout.columns` | **Present** |
| Breaks (second slot of the Insert control) | `:2245-2249` | `insertBreak` / `insertSectionBreak` (facade, no surface yet) | **Absent** · needs **UI** — engine shipped in #649 (§4.3) |
| Line Numbers (none/continuous/restart page/restart section/suppress + custom) | `:1487-1537` | `layout.lineNumbers` (#609) | **Present** |
| **Hyphenation** (none / auto / custom) | `:1540-1572`; engine is complete — `sdkjs/word/Editor/Paragraph/TextHyphenator.js`, breaker `Paragraph_Recalculate.js:4335`, painted `RunContent/Text.js:405` | — | **Absent** · needs **engine** — FID-L-02 / OO-006 |
| indent left/right, spacing before/after spinners | `:694-783` | `layoutIndentFieldsBtn`, `layoutSpacingFieldsBtn` | **Present** |
| Wrapping (8 modes + edit wrap boundary) | `:2336-2337`, menu `:2544-2606` | `layout.arrange.wrap` → object inspector | **Reachable** — no wrap-boundary editor |
| **Bring Forward / Send Backward** (front/forward, back/backward) | `:1655-1678`, menus `:2395-2419` | **LIVE** (corrected 2026-10-01) — `object_arrange_commands.mjs`, `command: "layout.arrange.bringForward"`, `requires: "object"`, over `setObjectZOrder` in `crates/casual-doc-wasm/src/objects.rs`. `sendBackward`, `bringToFront`, `sendToBack`, `group` and `ungroup` are declared beside it. This row read *ships disabled … needs a z-order operation the engine does not expose yet*; the operation exists | **Parity** on z-order · align and distribute still absent — §4.5 |
| **Align** (6 alignments + distribute h/v + align to page / margin / objects) | `:1619-1629`, menu `:2459-2494` | — | **Absent** · needs **engine** — §4.5 |
| **Group / Ungroup** | `:1631-1641`, menu `:2529-2539` | — | **Absent** · needs **engine** — §4.5 |
| **Merge Shapes** (union, combine, fragment, intersect, subtract) | `:1643-1653`, menu `:2497-2526` | — | **Absent** · needs **engine** |
| Watermark (edit / remove) | `:1680-1702` | `layout.watermark` (`main.js`, `id: "layout.watermark"`) | **Present** |
| **Page Color** | `:1704-1723` | — | **Absent** · needs **facade + UI** — §4.4 |
| **Colors** (theme colour schemes, 24+) | `:1601-1616`, generated `:3419-3463` | — | **Absent** · needs **model + UI** |
| — | — | `layout.firstPageVariant`, `layout.evenOddVariant` (`main.js`, in `LAYOUT_SURFACE`) | **Reachable** — on Layout here, on the contextual Header & Footer tab there |

### 2.5 References — theirs 10, ours 9

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| **Table of Contents** (2 previews + settings + remove) | `Links.js:172-174`, menu `:289-297` | **LIVE** (corrected 2026-10-01) — `main.js`, `command: "reference.tableOfContents"`, `requires: "bodyCaret"` because `insertTableOfContents` refuses a caret outside the body as Word does. `table-of-contents.spec.mjs` drives it and asserts the entry reaches the document. This row read *ships disabled* | **Partial** — insertion ships; their style gallery, settings and Remove do not — OO-001 |
| **Add Text** (don't show in TOC / level *n*) | `Links.js:197-209`, built `:441-460` | — | **Absent** · needs **engine** — part of OO-001 |
| **Update Table** (all / page numbers only) | `Links.js:183-194`, menu `:313-318` | — | **Absent** · needs **engine** — part of OO-001 |
| Footnote / Endnote, **convert to endnotes / to footnotes / swap**, **delete all notes**, **note settings**, go-to next/prev note | `Links.js:175-177`, menu `:332-398` | `insert.footnote`, `insert.endnote` | **Weaker** · needs **facade + engine** — §4.6 |
| Link | `Links.js:178-180` | `insert.link` | **Present** |
| Bookmark | `Links.js:212-221` | `insert.bookmark` → Bookmark manager (`docs/78`) | **Present** |
| Caption | `Links.js:224-233` | `reference.caption` (`main.js`, in `REFERENCE_SURFACE`) | **Present** — shipped; `docs/109` row 77 is stale (§8) |
| Cross-reference | `Links.js:236-245` | `reference.crossReference` (`main.js`, in `REFERENCE_SURFACE`) | **Present** — shipped |
| **Table of Figures** + its Update | `Links.js:248-257`, `:260-269` | — | **Absent** · needs **engine** — OO-001 |
| — | — | `reference.updateCaptionNumbers` (`main.js`, in `REFERENCE_SURFACE`) | **Ahead** — they renumber on field update, we offer it explicitly |
| — | — | **LIVE** (corrected 2026-10-01) — `main.js`, `command: "reference.updateFields"`, `requires: "tocField"`, so it is offered once the document carries a generated contents field and says `toc.noneToUpdate` before that. Word's two modes are commands of their own, `reference.updateToc.pageNumbers` and `reference.updateToc.entire`. This row read *ships disabled … `requires: "missing"`* | — |

### 2.6 Review (ours) vs Collaboration (theirs) — theirs 16, ours 15

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| Track changes (on/off for me, on/off global) | `ReviewChanges.js:340-351`, menu `:569-600` | `review.mode.suggesting` | **Weaker** · needs **UI** — no per-user vs global split |
| Display Mode (markup / simple markup / final / original) | `ReviewChanges.js:383-433` | `view.showChanges` + `docs/93` policy | **Weaker** · needs **engine** — two states, not four |
| Previous / Next change | `:356-375` | `review.previous`, `review.next` | **Present** |
| Accept (current / all) · Reject (current / all), as split buttons | `:287-310`, menus `:604-631` | `review.acceptNext`, `review.rejectNext`, `review.acceptAll`, `review.rejectAll` | **Weaker** · needs **UI** — our four ids are four buttons, not two split buttons, and `review.acceptAtCaret` / `review.rejectAtCaret` exist in the registry with **no surface but the palette**. That is the recorded gap **HF-190** (it was HF-179 until that id was found to name two unrelated defects; the pointer-cursor row kept HF-179, PR #641), and it is the only genuine single-surface capability in the whole registry (`webapp/tests/e2e/one-axis-navigation.spec.mjs:301-307`, `docs/122`:240) |
| Comment | `controller/Toolbar.js:4085` | `review.comment` | **Present** |
| Remove comment (current / mine / all) | `:498-508`, menu `:717-737` | `review.comment.delete` (current only) | **Weaker** · needs **UI** — §4.7 |
| Resolve comment (current / mine / all) | `:511-521`, menu `:739-759` | `review.comment.resolve` (current only) | **Weaker** · needs **UI** — §4.7 |
| **Compare / Combine** (file / URL / storage + settings) | `:314-337`, menus `:637-660` | — | **Absent** · needs **engine** — OO-007 |
| **Sharing** | `:439-447` | — | **Absent** · needs **host** — OO-018 |
| **Co-editing Mode** (Fast / Strict) | `:452-462` | — | **Absent** · needs **engine** — HF-114; and it needs their server |
| **Chat** | `:483-492` | — | **Absent** · needs **host** — and licence-gated on their side (`Main.js:1728-1729`) |
| **Version History** | `:470-478` | — | **Absent** · needs **engine** — HF-068, licence-gated on their side |
| **Mail Merge** | `:527-544` | — | **Absent** · needs **engine + host** — OO-013, licence-gated on their side |
| — | — | `tools.spellCheck` (`main.js`, `id: "tools.spellCheck"`) | **Present** — both sides — theirs is `spell.wasm` |
| — | — | **`tools.grammarCheck`** (`main.js`, `id: "tools.grammarCheck"`, `webapp/src/grammar.mjs`) | **Ahead** — §5 |
| — | — | `tools.smartQuotes` (`main.js`, `id: "tools.smartQuotes"`) | **Ahead** — on the ribbon; theirs is an AutoCorrect setting |
| — | — | `review.toggle` (comments/suggestions pane) | **Present** — both sides |

### 2.7 View — theirs 16 (edit mode), ours 8

| Theirs | file:line | Ours | Verdict |
| --- | --- | --- | --- |
| Headings / navigation panel (promote, demote, insert heading before/after, expand to level 1-9, font size, wrap) | `ViewTab.js:202-211`; panel `Navigation.js:76-238` | `view.outline` | **Weaker** · needs **facade + UI** — no promote/demote, no expand-to-level |
| Zoom combo (10 presets) | `ViewTab.js:416-439` | `view.zoom` + footer zoom menu | **Present** |
| Fit to page / Fit to width | `:216-238` | `view.zoom.fitPage`, `view.zoom.fitWidth` | **Present** |
| Zoom to 100% | `:326-334` | `view.zoom.100` (`#viewZoomActual`) | **Present** — `105` §4.3's "lacks zoom-to-100%" is stale (§8) |
| **Multiple pages** | `:313-323` | — | **Absent** · needs **UI** — §4.8 |
| Interface theme (8) | `:240-250` | `view.settings` ▸ theme (2) | **Weaker** · needs **UI** — OO-020 |
| **Dark document** | `:253-262` | — | **Absent** · needs **UI** — §4.8 |
| **Rulers** show/hide | `:303-310` | ruler is built unconditionally (`webapp/src/ruler.mjs`, mounted unconditionally from `main.js`) | **Absent** · needs **UI** — §4.8 |
| **Status bar / Left panel / Right panel** visibility | `:265-300` | `view.compactRibbon` only | **Absent** · needs **UI** — §4.8 |
| Always show toolbar | `:275-282` | `view.compactRibbon` | **Reachable** |
| Macros / Record / Pause | `:342-372` | — | **Absent** · needs **open decision** — OO-017; config-gated on their side |
| Hand / Select tool | `:375-401` | — | **Excluded** — removed in edit mode (`ViewTab.js:509`) |
| — | — | `layout.pageSetup` (`#pageSetupBtn`) | **Reachable** — theirs is on Layout |

### 2.8 Tabs with no OpenDoc counterpart

| Theirs | Controls | Verdict |
| --- | --- | --- |
| **Draw** (Select, 2 pens, highlighter, eraser; 20-colour palette, mm size) | 5 — `apps/common/main/lib/view/Draw.js:153-162`, `:122-135`, `:140-150` | **Absent** · needs **engine** — OO-019. Minimal even for them: whole-stroke eraser only, no ink-to-shape |
| **Protection** (Encrypt, change/delete password, Signature ▸ invisible / signature line, Protect Document) | 5 declared / 3 reachable / **2 visible in a browser** — `Protection.js:137-169`, `DocProtection.js:80-89` | **Absent** · needs **model + engine** — OO-011. Signatures are desktop+offline-only (`Main.js:1754`), so only the password half and Protect Document are web-parity gaps |
| **Forms** | 21 declared / 17 editor-visible — `FormsTab.js:365-710` | **Non-goal** — their form designer is PDF/DOCXF-only (`Main.js:602`). §6 |
| **Plugins** | dynamic; 0 declared statically — `view/Plugins.js:102-122`, built `controller/Plugins.js:418-505` | **Absent** · needs **open decision** — OO-017, deliberately |
| **Header & Footer** (contextual) — Header&Footer, Page Number, Date&Time, Field, Image, Header-from-top spinner, Footer-from-bottom spinner, Different odd/even, Different first page, **Same as previous**, Close | 13 slots (11 interactive + 2 labels) — `HeaderFooterTab.js:53-90`, `:199-219`, `:259-260` | **Reachable** — Capability mostly ships without the tab: `insert.header`/`insert.footer`, `layout.firstPageVariant`, `layout.evenOddVariant`. **Header/footer distance and Same-as-previous are absent** — §4.4, §4.9 |
| **Chart Design** (contextual) — Chart Elements (9 submenus, 45 leaves), Edit Data, Update Data, styles gallery, Chart Type, Advanced, size spinners, lock ratio, 3-D rotation | 14 — `apps/common/main/lib/view/ChartTab.js:566-782` | **Absent** · needs **engine** — gated on Q3 (charts are not drawn at all) |

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
already draw a pilcrow-sized marker for paragraph-mark revisions (`main.js`, `Where a paragraph-level revision is drawn`),
which is a precedent for the geometry but not the feature.

### 4.3 Breaks — page, column, section — SHIPPED in the engine (#649); UI only remains

**Corrected 2026-09-27, and the correction matters because this section's grade was
wrong in a way that inflated the cost.** It said a page break "needs **engine**" work.
It did not. When #649 came to build it, `BreakKind::{Page, Column}` was already
imported, **paginated** (`flow.rs` turns a trailing page break into `LineBreak::Page`
plus `page_break_after`; `columns.rs` answers that with a new page, and
`LineBreak::Column` with the next column), **exported** as `w:br w:type="page"|"column"`,
and degraded-with-a-loss-report on the ODT path. A `w:br` is an inline node, so authoring
it reused the same `InsertInlineObject`/`RemoveInlineObject` pair Shift+Enter already
used: **zero new operations and zero layout changes.** Only the SECTION break needed an
operation, and it needed exactly one.

The lesson for the rest of this document: "needs engine" was inferred here from the
absence of an *authoring command*, not from the state of the engine. Any row graded
`engine` on that reasoning is worth re-checking before it is costed — it is the same
mistake as reading a missing surface as a missing capability.

Theirs: one split button rendered into **both** Insert and Layout
(`view/Toolbar.js:2245-2249`); menu `Toolbar.js:2382-2388` (page / column / section);
section submenu `Toolbar.js:2372-2380` (next page / continuous / even page / odd page).

Ours, as of #649: `Operation` has **51** variants — one added,
`SpliceSectionBoundary { at, boundary }`, a single variant rather than a pair because
`Some`/`None` makes it its own inverse, anchored on `SectionId` rather than a list index
(doc 45 I3). The facade is `insertBreak(node, offset, "page"|"column")` and
`insertSectionBreak(node, offset, "nextPage"|"continuous"|"evenPage"|"oddPage")`. The
inheritance table lives in the `crates/casual-doc-edit/src/breaks.rs` module header so it
cannot drift from the code; ADR-037 records the one-operation decision.

**What is left is the UI**, and it displaces nothing: §7.1's recipe measures **Insert
537px** and **Layout 559px** of headroom, so a mirrored split button fits in both bands
the way theirs does. `Blank Page` (theirs: `Toolbar.js:979-989`) is **unblocked** — two
page breaks, no engine work.

### 4.4 Section and page properties that are modelled, laid out, and unreachable — mostly UI only, and now mostly shipped

This is the cheapest class in the repository, and the owner is right that it is large. **It is
also the one section of this document that a lane closed while it was being written**, so the
"Reachable now?" column below is dated: it is the state at `f8233e0`, after
`webapp/src/header_footer_settings.mjs` and the gutter and vertical-alignment fields landed in
`webapp/src/page_setup.mjs`. Four of the nine rows are no longer gaps.

| Property | Modelled | Laid out / painted | Operation carries it? | Reachable at `f8233e0`? | Their UI |
| --- | --- | --- | --- | --- | --- |
| Header distance from top | `definitions.rs:354` `header_twips` | yes | **yes** — `SetSectionGeometry` takes the whole `PageMargins` (`casual-doc-edit/src/lib.rs:655-667`) and `setPageSetup` accepts it as JSON (`casual-doc-wasm/src/lib.rs:6998-7026`) | **yes** — `#headerFromTop` | `HeaderFooterTab.js:65-66`, label `:504` |
| Footer distance from bottom | `definitions.rs:361` `footer_twips` | yes | **yes**, same | **yes** — `#footerFromBottom` | `HeaderFooterTab.js:69-70`, label `:505` |
| Gutter (binding margin) | `definitions.rs:364` `gutter_twips` | yes | **yes**, same | **yes** — `#pageMarginGutter` | `PageMarginsDialog.js:87-90`, `:199-215` |
| Page vertical alignment | `definitions.rs:833` `PageVerticalAlignment` | yes | the section op now carries it | **yes** — `#pageVerticalAlignment` | — (they have none) |
| Page numbering format + start-at | `definitions.rs:435-446`; consumed in pagination `casual-doc-layout/src/paginate.rs:1733-1760` | **yes** | the page-setup path now carries it | **yes** — `#pageNumberFormat`, `#pageNumberStart`, and a continue-versus-restart radio pair | `PageNumberingDlg.js:132-210` |
| Gutter position (top vs left) | — | — | no | no | `PageMarginsDialog.js:217-232` |
| Mirror margins | import `casual-doc-import/src/settings.rs:233`, export `casual-doc-export/src/semantic.rs:3118`, laid out (`casual-doc-layout/tests/section_geometry.rs:313`, FID-L-16) | yes | document settings, not the section op | no — zero hits for a mirror-margins control in `webapp/src` or `editor.html` | `PageMarginsDialog.js:290`, `:306` |
| Page borders | `definitions.rs:556`, `:848`; resolved `casual-doc-layout/src/page_border.rs:30`; painted `compose.rs:308-309`, `:420` | **yes** | **no** — not a field of `SetSectionGeometry` | no | **They have no dialog either** — §5.4 |
| Page background colour | `casual-doc-model/src/v1/document.rs:51`, setter `:112`; painted `casual-doc-render/src/lib.rs:103-105` | **yes** | document-level, with no WASM export | no | `Toolbar.js:1704-1723` |

Our Page setup dialog offers section, orientation, width/height, four margins, columns, the
gutter and page vertical alignment, and its apply path **spreads the untouched margin fields
straight back through** — `webapp/src/page_setup.mjs`, `...current.pageMargins` — which is
exactly why header distance, footer distance and gutter survived a round-trip for as long as
they were unauthorable. That was the prediction this section made, and it held: **three number
inputs and a select closed four of these rows with no engine change at all.**

So §7 files only the **remainder** — gutter position, mirror margins, page borders, page
colour — and the remainder is no longer "mostly UI only": page borders needs a field on
`SetSectionGeometry`, and page colour needs a WASM export over a setter that already exists.

### 4.5 Object arrangement — engine + UI

Theirs: Bring Forward / Send Backward (`Toolbar.js:1655-1678`), Align with 6 alignments +
distribute horizontally/vertically + align-to page / margin / objects
(`Toolbar.js:1619-1629`, menu `:2459-2494`), Group / Ungroup (`:1631-1641`), Merge Shapes
with 5 boolean operations (`:1643-1653`).

Ours, **corrected 2026-10-01**. This paragraph read: *`layout.arrange.bringForward` is declared
and ships disabled with a reason — "`objectOrder()` READS paint order; nothing writes it, and
there is no z-order op in the wasm facade". There is no align, distribute, group or merge command
at all … `SetGroupGeometry` exists so groups can be resized, not formed.* Two of those three
sentences are now false.

**Z-order ships.** `setObjectZOrder` is in `crates/casual-doc-wasm/src/objects.rs` and
`object_arrange_commands.mjs` declares all four commands over it —
`layout.arrange.bringForward`, `sendBackward`, `bringToFront`, `sendToBack` — each
`requires: "object"`. `object-arrange.spec.mjs` asserts the page really repaints when a shape
is restacked, and that an in-line object is refused with the reason.

**Group and ungroup ship.** `layout.arrange.group` and `layout.arrange.ungroup` are declared
beside them, over `groupObjects` / `ungroupObject` / `canGroupObjects`, so groups can be formed
and not only resized.

**Rotation and flip ship**, which this section did not claim either way:
`layout.arrange.rotateRight`, `rotateLeft`, `flipHorizontal` and `flipVertical`, over
`setObjectRotation` / `setObjectFlip`, reachable from the object chip's rotate menu, the
properties inspector's numeric field and a drag handle.

**Still absent:** align (the six, plus align-to page / margin / objects), distribute, and Merge
Shapes' five boolean operations. There is no align or distribute command of any kind, which is
what `docs/153` files as `object.align-objects`, `object.align-to-page-margin` and
`object.distribute`.

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
(`webapp/src/ruler.mjs`); hiding it is a class toggle. Multiple-pages view is the one with real
cost, because it changes the sheet layout the viewer paints.

### 4.9 Same-as-previous header/footer — engine, and already designed

Theirs: `HeaderFooterTab.js:84`, `:219` (`chSameAs`).

Ours: the control **now exists and ships disabled with a reason** —
`#headerFooterLinkToPrevious` in Header and footer settings, titled *"Unlinking has to copy the
inherited header into this section, which the engine cannot do yet"*. That is the
never-a-dead-control contract working as intended (`SKILL.md` §10), and it means this row is not
a missing-surface row.

**The first draft's classification of this as "facade + UI" was too cheap, and
`docs/129-LINK-TO-PREVIOUS-DESIGN.md` is why.** Re-linking is one
`SetSectionRunningRef { reference: None }` and needs nothing new. *Un*linking has to mint a
section-local body holding a faithful copy of the inherited blocks, and the only deep-copy in
the tree — `Interop::fresh_block` in `casual-doc-wasm`, the structured clipboard's
reconstructor — routes inlines through a `match` with a `_ => {}` arm that silently drops
everything but runs, tabs, breaks and hyperlinks. Shipping this on that helper would be silent
data loss, which `SKILL.md` §12 forbids outright. It adds **no operation** (so ADR-030 I2 holds)
and needs **one lossless clone helper**. Tracked as **HF-192**, with the clone helper itself as
**HF-191** (PR #641). Read `129` before touching it; do not re-derive it here.

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
| **Grammar checking** — `tools.grammarCheck` (`main.js`, `id: "tools.grammarCheck"`), `webapp/src/grammar.mjs`, guarded by `webapp/tests/grammar.test.mjs` | **The only "grammar" token in the whole `sdkjs` tree is a dead colour branch**: `sdkjs/word/Drawing/Graphics.js:2169` picks blue for a `handlerId` containing "grammar", and **nothing anywhere produces such a handler id**. Outside `resources/help/`, `web-apps` has zero "grammar" hits — no command, no string, no setting. They ship spelling (`spell.wasm`) and reserved the squiggle colour for grammar they never built |
| **Checklists** — `paragraph.list.checklist` (`main.js`, `id: "paragraph.list.checklist"`), ribbon `#checkList` | No checklist list kind in `sdkjs/word/Numbering/` or `locale/en.json` |
| **Explicit caption renumbering** — `reference.updateCaptionNumbers` (`main.js`, in `REFERENCE_SURFACE`), offered when captions go stale rather than only on a field update | Theirs renumbers only through Update Table / field update (`Links.js:313-318`) |
| **Emoji picker** — `insert.emoji` (`main.js`, `id: "insert.emoji"`), and colour-glyph rendering (`docs/102`) | Their Symbol control is a symbol table only (`Toolbar.js:1039-1060`) |
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
  passthrough from the producer's cached result. **Corrected 2026-10-01:** this sentence read
  *`reference.updateFields` ships disabled for exactly this reason* and it no longer does. The
  command is live on `requires: "tocField"`, and it updates the CONTENTS field specifically —
  Word's two modes, page numbers only or the entire table, are `reference.updateToc.pageNumbers`
  and `reference.updateToc.entire`. The field-recomputation gap above is unchanged and is still
  RM-01; what changed is that it is no longer the reason a control is dark.

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

### 7.1 Where a new ribbon control's width comes from — measured, not assumed

**The ribbon must fit 1280px with no horizontal scrollbar**, and a band that cannot fit exiles a
whole group into the `⋯` overflow menu (`webapp/tests/e2e/ribbon-home.spec.mjs` asserts both
halves: no scrollbar at 1280, and the overflow menu keeps the exiled controls reachable). That
constraint is not negotiable, so **no row below may be worked by adding a ribbon button without
first saying where its width comes from.**

The headroom is measurable, and it is not the number in circulation. `SKILL.md` §11 says *"~55px
of slack on the Home band"*, and the comment in `ribbon-home.spec.mjs`'s styles-group guard says
*"the Home band had 10px of slack at 1280px"*. **Both are stale.** Measured by binary-searching
the narrowest viewport at which `#ribbonOverflowBtn` is still hidden — a behavioural measurement,
because the observable that matters is "is any group exiled", not any particular pixel width:

| Band | Fits down to | Headroom at 1280px |
| --- | ---: | ---: |
| Home (41 controls, 6 groups) | 1016px | **264px** |
| Table (19, contextual) | 861px | 419px |
| Insert (18) | 743px | 537px |
| Layout (13) | 721px | 559px |
| References (9) | 677px | 603px |
| Review (15) | 552px | 728px |
| View (8) | 401px | 879px |

The recipe, which any reader can re-run and which is how these numbers were obtained:

```js
// In a Playwright spec, per tab: shrink the viewport until anything is exiled.
let lo = 400, hi = 1280;
const overflows = async (w) => {
  await page.setViewportSize({ width: w, height: 900 });
  await page.waitForTimeout(80);
  return page.$eval("#ribbonOverflowBtn", (el) => el.offsetParent !== null);
};
while (hi - lo > 1) { const mid = (lo + hi) >> 1; if (await overflows(mid)) lo = mid; else hi = mid; }
// `hi` is the narrowest fitting viewport; 1280 - hi is the band's headroom.
```

Two consequences for the rows below.

1. **Home is the only band anywhere near its budget, and even Home has 264px** — about three
   icon-only controls, or one labelled split button. So the constraint binds on *Home* and
   almost nowhere else: Insert, Layout, References, Review and View each have room for ten or
   more controls before anything is exiled. A row that wants a control on one of those bands does
   not have a width problem.
2. **A stale budget is its own defect.** Two committed artefacts state a budget an order of
   magnitude tighter than the measurement, and one of them is `SKILL.md`, which is the authority
   agents read first. That is reported to the owner rather than edited here, because this lane
   owns `docs/` only — and because the right fix is not a new number in prose but a guard that
   *derives* the headroom, so it cannot go stale again. Until then, **re-measure before citing
   it**; do not copy 264px forward either.

**Per-row width sourcing.** The rows that want a *new ribbon control* rather than a dialog field,
a menu item or a context-menu row, and where each would go:

| §7 row | Wants on the ribbon | Width comes from |
| ---: | --- | --- |
| 1 | Breaks — one split button | **Insert** (537px headroom), and mirrored into **Layout** (559px) as theirs is. No displacement |
| 2 | Show formatting marks — one toggle | **Home**, in the paragraph group, ~32px of the 264px. This is the one Home addition worth the slack: theirs is on Home and it is reached dozens of times a session |
| 3 | Table of contents — the control already exists, disabled | none needed |
| 4 | Paragraph direction — two toggles, or one split | **Home** paragraph group (~64px of 264px) *or* the paragraph properties dialog. Word puts them on Home; Docs puts them behind a menu. Owner call, and the cheaper option is not a compromise here |
| 5 | Hyphenation — one menu button | **Layout** (559px). No displacement |
| 6 | Page borders, page colour, mirror margins, gutter position | **no new ribbon control** — all four are fields in Page setup, which already holds nine |
| 7 | Small caps / all caps / spacing / position | **no new ribbon control** — the Font group is the Home band's second-widest at 89px and these belong in a font dialog, as they do in both Word and theirs (`ParagraphSettingsAdvanced.js:484-512`) |
| 8 | Object arrangement — align, distribute, group, z-order | **Layout** (559px), as theirs are. The disabled `layout.arrange.bringForward` is already there |
| 9 | Word count | **no new ribbon control** — the status bar already carries counts; the gap is a dialog |
| 11 | Note management | **References** (603px) as a menu behind the existing footnote control |
| 12 | Multilevel list gallery | **Home** — but as a third item in the *existing* list split buttons' menus, not a new button. Zero width |
| 14 | Comment scope | **Review** (728px) — and correctly as menus behind the existing Resolve and Remove faces, which is exactly the split-button density finding in §1. Zero width |
| 16 | Rulers, dark document, panel visibility | **View** (879px headroom, 8 controls). The emptiest band in the product |
| 22 | Navigation promote/demote | inside the outline panel, not the ribbon |
| 23 | Theme colour schemes | **Layout** as theirs is, or the Settings pane. Owner call |

Rows not listed want no ribbon control at all, or are gated on an engine capability that has to
exist before a surface question is meaningful. **Never a dead control** (`SKILL.md` §10): where a
row lands a face before its engine, it ships disabled with a reason, as rows 3, 8 and §4.9
already do.

| # | Gap | Needs | Existing row |
| ---: | --- | --- | --- |
| 1 | ~~**Insert a page break** (and column break, and section break with 4 kinds)~~ **CLOSED 2026-10-01** | This row read "**engine** — no break-insert operation exists among the 50". That was already wrong when it was written, or became so soon after: `insertBreak` and `insertSectionBreak` were both in the facade with their own refusals and engine tests, and nothing in `webapp/` called either — the whole gap was the chrome. Six commands now ship (`layout.break.page`, `layout.break.column` and the four `layout.break.section.*`) on one Insert-band dropdown, an Insert-menu submenu and Ctrl/Cmd+Enter | closed; `docs/153` `page.break.page` / `page.break.section` / `page.break.column` are Parity, and `page.blankPage` is narrowed to the two-breaks-in-one-step command it still needs |
| 2 | **Show formatting marks** (¶, space dot, tab arrow, page-break rule) + hidden table borders | **engine** — a display-list layer; then UI + a chord | **new** |
| 3 | **Table of figures**, and Add Text — the contents table itself, and Update Table, LANDED (corrected 2026-10-01) | UI for Add Text; references collection for the figure table | OO-001 (narrowed: `reference.tableOfContents` and `reference.updateFields` are live, so the remaining gap is the figure table, the style gallery, the settings dialog and Remove) |
| 4 | **Paragraph direction LTR/RTL** | **facade + UI only** — the operation already carries `bidi` | **new** — and we ship `ar.json` |
| 5 | **Hyphenation** | engine (line breaker) + UI | FID-L-02, OO-006 |
| 6 | **Section/page properties with no UI** — now only **gutter position, mirror margins, page borders, page colour**. Header/footer distance, gutter, page vertical alignment and page numbering format + start-at all **shipped** while this document was being written (§4.4) | **facade + UI** for page colour (the model setter exists, no WASM export does); **engine op field** for page borders; **model + UI** for gutter position; **UI** for mirror margins over the existing document setting | the header/footer half landed; **new** for the four that remain |
| 7 | **Advanced character formatting**: small caps, all caps, character spacing, character position | **engine** — `FormatDelta` carries none of the four — then facade, then UI. All four already lay out | **new** (distinct from OO-021's ligature clause) |
| 8 | **Object arrangement**: align (6), distribute (2), align-to page/margin/objects, and Merge Shapes. Z-ORDER, GROUP/UNGROUP and ROTATE/FLIP LANDED (corrected 2026-10-01) | **UI + facade** for align and distribute; the engine op is the question Merge Shapes raises. The "no z-order or group operation" this row claimed is wrong: `setObjectZOrder`, `groupObjects`, `ungroupObject`, `setObjectRotation` and `setObjectFlip` all exist and all have commands | **new** (narrowed) |
| 9 | **Word count dialog and selection-scoped counts** | UI only — `documentStats` already exposes words/characters/paragraphs (`main.js`, `documentStats`) | HF-051 / OO-015 |
| 10 | **Recent files** | UI + a host storage contract | OO-002 remainder, RM-03 |
| 11 | **Note management**: convert to endnotes/footnotes, swap, delete all notes, note settings, go-to next/previous note | facade (delete-all; `RemoveNote` exists) + engine (convert/swap) + UI | **new** |
| 12 | **Multilevel list gallery and list settings** (number format, start-at, restart, follow-number-with, tab stop) | UI over the existing numbering model (`definitions.rs:220-248` carries level, start, `num_fmt`, `lvl_text`, `lvl_jc`, `suff`) | OO-021 — sharpen with these citations |
| 13 | **Print dialog**: range, duplex, colour/mono, margins, preview | UI + print plumbing. **Its "blocked-by RM-04" note is stale** — real-text PDF shipped | OO-010 |
| 14 | **Comment scope**: remove/resolve mine and all | UI only | **new** |
| 15 | **Header/footer "same as previous"** | **engine** — one lossless deep-clone helper, **no new operation**. The control already ships disabled with a reason and the design is written | `docs/129-LINK-TO-PREVIOUS-DESIGN.md`, tracked as **HF-192** with the deep-copy helper it waits on as **HF-191** (PR #641); **not a new row** |
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
| `109` row 77 — **OO-005** "No captions and no cross-references", Open, blocked-by RM-01 | **Shipped.** Both commands are declared and enabled | `main.js`, `REFERENCE_SURFACE` (`reference.caption`, `requires: "bodyCaret"`; `reference.crossReference`, `requires: "caret"`); dialogs `webapp/src/caption_dialog.mjs`, `webapp/src/cross_reference_dialog.mjs`; specs `webapp/tests/e2e/references-captions.spec.mjs`, `webapp/tests/cross_reference_model.test.mjs`; commit `be26345` cites OO-005 by name |
| `109` row 90 — **FID-L-10** titled "Watermarks do not appear", Open | Title is false and the status disagrees with its own source row, which reads **"Partly fixed"**. The real remainder is general WordArt text paths | `105:314`; watermark ships: `crates/casual-doc-layout/src/watermark.rs`, painted `compose.rs:314`, authored `main.js`, `id: "layout.watermark"`; PRs #611/#612 fixed live watermark defects |
| `109` row 99 — **OO-010** "blocked-by RM-04 for real-text output" | **RM-04 is closed** — real-text PDF export shipped and is reachable from File ▸ Export as PDF | `109` "How it was settled" table, RM-04 row |
| `109` row 102 / `105:517` — **OO-020** "46 locales vs 1"; "18 bound chords here" | Both numbers are stale in our favour's opposite direction. **19 locales**, **36 chords** | `ls webapp/locales/` → 19; `grep -c 'chord:' webapp/src/keymap.mjs` → 36; theirs: 46 locale files, **146** named actions (`sdkjs/word/apiDefines.js:223-371`) |
| `109:280` — **RM-01** "ONLYOFFICE ships only 14 field codes, so the bar is low" (also `106:273`) | **False, and false in our favour.** 18 types / 16 recognised instructions there; 10 classified and 3 recomputed here | §5.4 |
| `105` §4.3 tab table — "OpenDoc ships **five**: Home, Insert, Table, View, Review"; "References — **Largest single IA gap**"; "Layout — Gap, and the cheapest one"; "OpenDoc lacks multi-page view **and zoom-to-100%**" | Four errors. We ship **8** tabs; References and Layout both exist; zoom-to-100% exists | `editor.html:83-107`; `#panelLayout` 13 controls, `#panelReferences` 9; `view.zoom.100` / `#viewZoomActual` (`ribbon_faces.mjs:143`) |
| `105` §4.4 **OO-002** "No New document, no recent files, no templates, no backstage" | Three of four shipped; only Recent files remains. `109` row 68 already says so — `105` does not | #542, #596; `123` §5.3–5.4 |
| `105:518` — **UX-005** "~23 of ~90" ribbon controls carry a command id | Superseded twice: the row's own 2026-09-20 recount said 39 of 109, and `ribbon_faces.mjs` measured **52 of 120** before landing the fix. **All 123 controls now carry `data-command` (114) or `data-command-family` (9), 0 unclassified**, measured at `f8233e0` | `webapp/src/ribbon_faces.mjs`; guard `webapp/tests/e2e/ribbon-command-faces.spec.mjs` |
| `106` §4 "Where we already stand" | Stale in five cells: "Ribbon 5 tabs" (8), "97 command ids", "no PDF" (shipped), "no spell check" (shipped), "one locale" (19). Also "47-operation closed set" — the enum has **50** | `crates/casual-doc-edit/src/lib.rs:326-916`; `106` is an archive, so this is expected — but `106` §4 is cited as a measurement |

## 9. The rows to file

Id class, lane, priority and effort follow the conventions of `109` (`105` effort scale:
S = under a day, M = up to a week, L = more than a week). **Thirteen new rows**, each with the
`#7` rank it came from. Two of the first draft's fifteen are withdrawn below because PR #641
filed them while this was being written — the same collision hazard the ids themselves have.
**Count the rows in the table rather than trusting this sentence**, and take every `109` row id,
priority and status **from the file at HEAD**, not from this document and not from `109`'s own
summary table: that table read 129 while the queue held 145. Every `HF-19x` id cited here, and the
137-row figure (58 Hotfix, 66 Audit, 13 Roadmap), come from **PR #641, which is in flight and not
on `main`** — they are reported so this document does not mint duplicates, and they are the one
class of number here that was not derived from a committed artifact. Re-derive them after #641
lands. Nothing here
duplicates an open row; where an existing row covers the work, §7 names it instead.

**The `OO-0xx` numbers below are suggestions, not reservations.** `OO-021` is the highest in `109`
at the time of writing, so `OO-022`…`OO-036` were free — but so were `HF-190` and `HF-191` when
the first draft claimed them, and PR #641 took both within the day. Whoever files these re-derives
the numbers from `109` at HEAD and keeps the descriptions, which are what actually identify the
work.

| Proposed id | Lane | Priority | Effort | Description, in the tracker's voice | §7 |
| --- | --- | --- | --- | --- | --- |
| OO-022 | Audit | P1 | M | No way to insert a page, column or section break — the operation set has no break-insert variant, so `Blank Page` is blocked behind it too | 1 |
| OO-023 | Audit | P1 | M | No formatting-marks view — no pilcrow, space dot, tab arrow or page-break rule, and no invisibles primitive in the display list | 2 |
| OO-024 | Audit | P1 | S | Paragraph direction is read-only: `bidi` imports, cascades, lays out and is diff-reported, but no setter reaches it — and we ship an Arabic UI locale | 4 |
| OO-025 | Audit | P3 | S | Section properties still with no UI after the header-and-footer lane landed: **gutter position and mirror margins only**. Header/footer distance, gutter, page vertical alignment and page numbering format + start-at all shipped — re-check §4.4 before filing, because this row shrank twice in one day | 6 |
| ~~OO-026~~ | — | — | — | **Withdrawn — already filed as `HF-193`** (PR #641): page borders are modelled, imported, laid out, painted *and* exported with no UI at all. The finding stands and this document's §4.4 and §5.4 are its competitive half — ONLYOFFICE has no page-borders dialog either, so this is a **lead**, not parity, and one of the cheapest wins on the board. Do not file a second row | 6 |
| OO-027 | Audit | P2 | M | `FormatDelta` cannot carry small caps, all caps, character spacing or character position, all four of which already lay out | 7 |
| OO-028 | Audit | P2 | M | **Narrowed 2026-10-01.** Read: "no object arrangement: no z-order operation (so `layout.arrange.bringForward` ships disabled), no align, no distribute, no group/ungroup". Z-order, group/ungroup and rotate/flip all ship with commands and specs; what remains is align, distribute and align-to page/margin/objects | 8 |
| OO-029 | Audit | P2 | S | Notes can be inserted and nothing else — no convert to endnote/footnote, no swap, no delete-all, no note settings, no go-to next/previous note | 11 |
| OO-030 | Audit | P2 | S | Comments have only the current scope — no "resolve mine", "resolve all", "remove mine" or "remove all", while changes already have both scopes | 14 |
| ~~OO-031~~ | — | — | — | **Withdrawn — already filed as `HF-192`** (PR #641), with the lossless deep-copy helper it waits on as `HF-191`. "A section's header/footer cannot be unlinked from the previous section" is real, but it is designed in `docs/129-LINK-TO-PREVIOUS-DESIGN.md` and the control already ships disabled with that reason. Filing it again would duplicate, and the first draft's effort estimate would have been wrong: the blocker is a lossless deep clone, not a facade call | 15 |
| OO-032 | Audit | P2 | S | View offers nothing but compact ribbon — no rulers show/hide, no dark document, no status-bar or panel visibility | 16 |
| OO-033 | Audit | P2 | M | No multiple-pages view, so a reader cannot see spreads | 17 |
| OO-034 | Audit | P3 | S | The headings panel has no promote/demote, no insert-heading-before/after and no expand-to-level | 22 |
| OO-035 | Audit | P3 | M | No theme colour schemes, and two interface themes against eight | 23 |
| OO-036 | Audit | P3 | L | No shape boolean operations (union, combine, fragment, intersect, subtract) | 27 |

Plus **three** rows that are **not** ONLYOFFICE gaps but came out of the measurement (§7.1, §11).
They are listed separately so the owner can file them in the Hotfix lane, where they belong.

**They carry no id, deliberately.** The first draft proposed them as HF-190 and HF-191, and both
of those ids have since been taken by PR #641 — which was itself cleaning up the fact that
`HF-179` had been minted twice for two unrelated defects, invisibly, because
`tracker_counts.test.mjs` skipped every row whose `#` carried a letter suffix and so never ran its
own duplicate-id check over 17 of them. **An id proposed in a document is an id nobody has
reserved.** Whoever files these assigns the number from `109` at HEAD; the rows are described
well enough to file without one.

| Proposed id | Lane | Priority | Effort | Description, in the tracker's voice |
| --- | --- | --- | --- | --- |
| *(assign at filing)* | Hotfix | P2 | S | The context menu wires five leaf capabilities under ids the registry does not answer (`link.add`, `comment.add`, `paragraph.bullets`, `paragraph.numbering`, `paragraph.properties`), so a cross-surface parity guard cannot see those rows and the menu is a second implementation free to drift — it already shipped the compact chrome with no list buttons and no Add comment (`docs/115`:314-323) |
| *(assign at filing)* | Hotfix | P3 | S | `webapp/src/ribbon_faces.mjs` and `webapp/tests/e2e/ribbon-command-faces.spec.mjs` open with a hand-maintained *"52 of 120 visible ribbon controls … References (7/7)"*, which is **123 / 9-of-9** at `f8233e0`. It is the source of the "120 controls / `data-command` on 111" figure in circulation, so it is cited as evidence and is wrong — `SKILL.md` §8's derived-not-hand-maintained rule applies to a comment that gets quoted. Fix by deleting the counts, not by updating them: the guard's whole design is that it measures |
| *(assign at filing)* | Hotfix | P2 | S | **The Home band's width budget is stated an order of magnitude too tight in two committed places** and one of them is `SKILL.md` §11 (*"~55px of slack"*; `ribbon-home.spec.mjs`'s styles guard says *"10px of slack"*). Measured at `f8233e0` the Home band fits down to a 1016px viewport, so it has **264px** of headroom at 1280 — and the other six bands have 419–879px (§7.1). The stale figure is load-bearing: it is quoted to reject ribbon additions. Fix by deriving the headroom in a guard rather than restating it in prose, so it cannot go stale a third time |

### Corrections to apply to existing rows, not new rows

| Row | Change |
| --- | --- |
| OO-005 (`109` row 77) | **Close.** Captions and cross-references shipped; see §8 |
| FID-L-10 (`109` row 90) | **Restate to "WordArt text paths are not typed, so warped text does not paint"** and carry `105`'s "Partly fixed". The watermark half is done |
| OO-010 (`109` row 99) | Drop "blocked-by RM-04" — RM-04 closed. Keep the row |
| OO-020 (`109` row 102) | Re-measure in place: 46 locales vs **19**; **36** chords vs their **146** named actions (`sdkjs/word/apiDefines.js:223-371`) |
| OO-021 (`109` row 103) | Extend with the citations in §2.3/§2.4 and add: horizontal line, Text from File, text-to-table, vertical text box, Remove header/footer, page-number position grid. Move its ligature clause to sit beside OO-027 |
| RM-01 (`109` row 122, `106:273`) | **Fix the justification.** They recognise 16 field instructions (18 types); we classify 10 and recompute 3. The work stands; "the bar is low" does not |
| UX-005 (`105:518`) | Note the measurement is now historical: UX-005's closing 111-of-120 and this document's 114-of-123 are the same recipe at two commits (§1). Do not restate either as a standing figure |
| `105` §4.2 decimal-tab row | Reword per §5.3 to "preserved but never laid out", citing `Tab.js:63` |
| `105` §4.2 page-borders row | Constrain to "no page-borders **dialog**" and record that they model, round-trip and paint `w:pgBorders` |
| `105` §4.3 tab table | Correct four cells per §8 |
| `105` §4.2 | Add the two new advantages from §5.2 — grammar checking and checklists |

## 10. How our side was verified, and what is still open

**Verified by reading the declaration and its enablement, and — where a guard exists — by
driving the control in a browser. Not by assuming a button works.**

What was run, **this revision**, at `f8233e0`: `npm ci` in the worktree and
`cd webapp && ./build.sh` (exit 0 — `pkg/` is untracked, so a browser measurement against a
stale engine is the default failure mode here, not an unlucky one), then three throwaway
Playwright probes on explicit ports `PW_PORT=39512/39513/39515`, all three deleted before
committing:

1. the control census — the guard's own `controlsOf()` selector over all seven tabs with the
   caret in a table and a range selected, printing the per-tab counts, the `data-command` /
   `data-command-family` / unclassified split, and the disabled set;
2. a hidden-control probe (`offsetParent === null` inside each panel), which returned **nothing** —
   that is what proved the static recipe's extra control was `#ribbonOverflowBtn` sitting outside
   the panels rather than a control hidden inside one;
3. the width-headroom binary search in §7.1.

The first revision additionally ran the repo's own guards:
`node --test tests/keymap.test.mjs tests/ribbon_faces.test.mjs tests/menu_taxonomy.test.mjs`
(33/33) and `npx playwright test one-axis-navigation ribbon-command-faces
command-shortcut-coverage` (19/19 in 4.7 min, no retries). Those were not re-run this revision:
this revision changed no code, and the numbers it re-derived come from the probes above.

This document is itself an input to a derived artifact — `webapp/dict/glossary.txt` is
regenerated from the repository's own documentation — so `node webapp/tools/build-glossary.mjs`
was re-run and the three new terms it earns (`DOCXF`, `FORMDROPDOWN`, `pilcrow`) are committed
with it. Two more were rejected by changing this document rather than the dictionary: the row
tables originally used `Pri` and `Eff` as column headers, which crossed the generator's
occurrence threshold and would have shipped two non-words to every user's spell checker. The
headers now read `Priority` and `Effort`. **The guard was proven able to fail**: mutating
`pilcrow` to `pilcrowMUTATED` in the committed glossary turned
`tests/glossary_artifact.test.mjs` red — `not ok 1 - the committed glossary is exactly what the
generator derives, today` — and regenerating returned it to 10/10. `npm run test:unit` is
580/580.

- **The 123 total was measured twice, two ways, and now agrees exactly** — live DOM with the
  guard's own selector (123), and statically over the markup (123 once `#ribbonOverflowBtn` is
  excluded; §0 spells out why the first draft's two passes disagreed by one). Cross-checked
  against the declaration tables: `HOME_FACES` 41, `VIEW_FACES` 8, `TABLE_FACES` 19 (read in
  node from `webapp/src/ribbon_faces.mjs`), `INSERT_SURFACE` 16, `LAYOUT_SURFACE` 16,
  `REFERENCE_SURFACE` 5, `REVIEW_SURFACE` 15. Table sizes do not equal panel sizes because
  one command can stamp faces on two panels — four Insert commands also stamp References
  buttons, and two Layout rows render on the Insert band.
- **No dead ids.** All 107 distinct ribbon ids are in the registry; all 35 keymap commands
  are in the registry; all 9 families have live members. **The `⌘⌥M` → `comment.add` defect
  cited as the cautionary example is already fixed** — `webapp/src/keymap.mjs:151` now binds
  `review.comment`, the fix is documented in place at `:146-150`, and
  `command-shortcut-coverage.spec.mjs:97` asserts the chord adds a comment.
- **Only four commands are palette-only**, and the repo already names all four
  (`webapp/tests/e2e/one-axis-navigation.spec.mjs:291-308`): `object.selectNext` and
  `object.selectPrevious` by design (Tab and Shift+Tab *are* the affordance), and
  `review.acceptAtCaret` / `review.rejectAtCaret`, which are the recorded gap HF-190 — we
  have the Accept/Reject faces and no split-button dropdown behind them.
- **Disabled controls, corrected TWICE.** The first draft said "five ... three permanently, two
  on selection". The second said **13** disabled in the measured state, of which **3** were
  permanent — `layout.arrange.bringForward`, `reference.tableOfContents` and
  `reference.updateFields`, via the `requires: "missing"` contract, "three occurrences, one per
  control".

  **That is now wrong on both halves (2026-10-01).** `requires: "missing"` appears **zero**
  times in `webapp/src/`: no row uses it, and the machinery is kept in
  `ribbon_surface.mjs:31` and `:50` for the next case rather than because there is one. All
  three of those controls are live and state-gated —
  `layout.arrange.bringForward` on `requires: "object"` over `setObjectZOrder`,
  `reference.tableOfContents` on `requires: "bodyCaret"` because the engine refuses a contents
  field outside the body as Word does, `reference.updateFields` on `requires: "tocField"` —
  and `table-of-contents.spec.mjs`, `object-arrange.spec.mjs` and
  `layout-references-surface.spec.mjs` each drive one and assert the document moved. The count
  of **13** is a measurement of a tree three of these controls have since left, so it is not
  restated here: it would be a derived number quoted from memory, which is the defect this
  section exists to correct. `SKILL` §9 rule 6 — understating is also false, and a reason left
  standing after its gap closed is the same lie as an overstatement.

  The other ten were, and still are,
  **state-dependent and correct**: `edit.redo` (nothing to redo), `paragraph.list.restart` and
  `paragraph.list.continue` (caret not in a list), `layout.arrange.wrap` and
  `layout.arrange.position` (no object selected), `reference.updateCaptionNumbers` (nothing
  stale), `review.comment.resolve` and `review.comment.delete` (no comment at the caret),
  `table.distribute.rows` and `table.merge` (no multi-cell selection). A state-dependent
  disable is the never-a-dead-control contract working, not a gap — but it is also why "how many
  ship disabled" is not a single number, and the first draft published it as one.
- **Asymmetric evidence, stated plainly.** `ribbon-command-faces.spec.mjs` drives **each**
  Home, View and Table control with a real pointer and asserts the same observable effect as
  running its id from the palette — so those **68** controls (41 + 8 + 19) are proven to *act*.
  There is **no such guard for Insert, Layout, References or Review**: those **55** controls
  (18 + 13 + 9 + 15) are proven to *name a live command*, not proven to act. Every "Present"
  verdict on those four tabs should be read at that strength. This is the §9.4 trap — "built"
  is not "reachable" — and it is the single largest hole in this document's evidence.
- Engine claims were checked at three levels: the model (`crates/casual-doc-model`), whether
  layout or render consumes it, and whether an `Operation` and a WASM export reach it. The
  §4.4-versus-§4.10 split exists precisely because that third level differs: page margins
  are carried by an existing operation and character spacing is not.

**Open questions, recorded rather than resolved:**

1. **Whether their spell check has features ours lacks** was not adjudicated — only that
   grammar is absent on their side. `spell.wasm` and our SCOWL path were not compared
   option-for-option.
2. **The Insert-tab count asymmetry** (their 20, our 18) is partly a slot-versus-button
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

Four findings that fall out of the measurement and belong to whoever owns the file. **Nothing
here was changed** — three lanes are live in `webapp/src/main.js`, `editor.html`, the locales,
`crates/**` and `packages/**`, and this document is read-only in all of them.

### 11.1 The context menu has its own command-id namespace — a real defect

Five *leaf capabilities* are wired twice, under an id the registry does not answer:

| Context-menu `data-command-id` | Declared | Registry id for the same capability |
| --- | --- | --- |
| `link.add` | `main.js`, `"link.add"` — the comment beside it admits it: *"The same capability as `insert.link` under the annotate surface's own id"* | `insert.link` |
| `comment.add` | `main.js`, `"comment.add"`, `run: () => openReviewComposer()` | `review.comment` |
| `paragraph.bullets` | `main.js`, `"paragraph.bullets"`, its own `doc.toggleList(…)` call | `paragraph.list.bullet` |
| `paragraph.numbering` | `main.js`, `"paragraph.numbering"`, its own `doc.toggleList(…)` call | `paragraph.list.numbered` |
| `paragraph.properties` | `main.js`, `"paragraph.properties"` | `layout.paragraph` |

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
open row anywhere. §9 proposes it without an id — see the note there on why.

### 11.2 Two committed comments carry the stale count

`webapp/src/ribbon_faces.mjs` and `webapp/tests/e2e/ribbon-command-faces.spec.mjs` both open
with *"52 of 120 visible ribbon controls … Insert (17/17), Layout (13/13), References (7/7) and
Review (15/15)"*. Measured at `f8233e0` it is **123**, References is **9/9**, and Insert is
18/18. `SKILL.md` §8 — *counts in docs must be derived, not hand-maintained* — applies to
comments that get cited as evidence, and this one is: it is where the "120 controls, 111 with
`data-command`" figure in circulation comes from, and this document was handed that figure as
the authority to normalise against. It was right when it was written and it will be wrong again
next month, so **the fix is to delete the counts, not to update them** — the file's own next
sentence already says the guard "MEASURES rather than pinning a number". Proposed in §9 without
an id; it belongs in a webapp-lane PR, not here.

### 11.3 Section text direction is modelled and nothing rotates

Worth recording because it was nearly published as a §4.4-class row. `SectionBoundary` carries
`text_direction: Option<TextDirection>` (`casual-doc-model/src/v1/definitions.rs:872`,
`w:textDirection`), and `TextDirection::{TbRl, BtLr}` *is* referenced in layout — but the only
reference is `cell_no_wrap_applies` (`casual-doc-layout/src/flow.rs:2608-2617`), where a vertical
direction merely **suppresses** a `noWrap` optimisation. Nothing rotates text anywhere. So
vertical text is `Absent` needing **engine**, not the cheap "modelled and laid out, just
unauthorable" row it looks like from a grep. The check that separated the two was asking *which
function* consumes the property, not whether one does — which is the §4.4-versus-§4.10 discipline
applied to a row that had not earned a section.

### 11.4 A claim this round got wrong, corrected

A sub-lane reported that `main.js` now has exports, citing a line near the end of `main.js` and concluding that
`SKILL.md` §11 and HF-085's status cell were stale. **That is wrong, and it is recorded here
rather than quietly dropped.** Line 16589 passes `editorCommands` as a *property of an options
object* into `createCompactToolbar` — dependency injection, not an ES module export.
`grep -n '^export ' webapp/src/main.js` returns nothing, and none of the 26 occurrences of the
string `export` in the file is an export statement. **`main.js` still has zero exports;
`SKILL.md` §11, `command_taxonomy.mjs:4-5` and HF-085 are all still accurate.**

## 12. What is verified, and what is not — read this before quoting a row

**Their side.** All **159** distinct `file:line` citations into `web-apps` and `sdkjs` were
machine-checked this revision: every one resolves to a file that exists and a line within that
file. That is a weak check — it catches a deleted file and a drifted range, not a
misdescription — so ten of the load-bearing ones were additionally re-read **by content**, and
all ten held exactly as described: the formatting-marks split button and its two menu items
(`Toolbar.js:786-804`), text direction LTR/RTL (`:674-691`), Page Color (`:1704-1723`),
Hyphenation (`:1540-1572`), the Rulers checkbox (`ViewTab.js:303-310`), the 18 formula function
names in one regex (`FormulaParser.js:62`), the 18 field types (`ComplexFields/types.js:42-62`,
including that `PAGE` and `NUMPAGES` really are aliases at `:49-50`), the decimal/bar tab TODO in
Russian (`Tab.js:63`), the `else if (tab_Center === tab_Center)` copy-paste bug
(`Rulers.js:2556`) and the dead grammar-squiggle colour branch (`Graphics.js:2169`).

**Unverified, and flagged rather than dropped:**

1. **No ONLYOFFICE version is claimed.** The first draft's tables said "9.4". Neither
   `web-apps` nor `sdkjs` records a release anywhere a reader can check — no version in
   `build/package.json` (it reads `1.0.1`), and `CHANGELOG.md`'s only heading is `## 5.3`. So
   every statement here is about **the reference checkout**, not about a shipped release, and a
   reader comparing against a different version should expect drift. Their per-file line numbers
   are only meaningful against that checkout.
2. **Their per-tab steady-state counts were not re-derived this revision.** Home 31, Insert 20,
   Draw 5, Layout 20, References 10, Collaboration 16, Protection 2, View 16 — and therefore the
   120 total — come from the first draft's slot-counting pass over their templates. What *was*
   re-derived is the split-button census (§1), which touched five of those files and found the
   count right and nine of its citations wrong. Treat the per-tab decomposition as one pass'
   work, not as two agreeing passes, and re-count it before it is quoted anywhere outside this
   document.
3. **Their Draw-tab characterisation** — "whole-stroke eraser only, no ink-to-shape", "20-colour
   palette", "mm size" — was read from the button declarations, not from their ink engine. The
   pen count (three: two pens and a highlighter) *is* derived, from `penOptions`. The eraser's
   granularity is a claim about behaviour and is **not** source-verified.
4. **Our "Present" verdicts on Insert, Layout, References and Review** are proven to *name* a
   live command, not proven to *act* — 55 of the 123 controls, per §10. The other 68 are driven
   with a pointer by a guard.
5. **The `needs` classification is this document's judgement** for every gap row not covered by
   §4's ten themes. Each was checked against the operation set and against whether layout or
   render consumes the property, but "needs engine" is an estimate of shape, not a costing. §11.4
   is a worked example of that check changing an answer.
6. **The four ONLYOFFICE-gap claims inherited from `SKILL.md` §1** (their co-editing is neither
   OT nor CRDT; spell check is client-side WASM; their text input is a hidden `<textarea>`; the
   host-customization API is licence-gated in code) are **not re-verified here.** They are cited
   because §1 says they were read out of the source, and this document did not repeat that work.

**Nothing in this document is sourced from ONLYOFFICE's help site, marketing pages, or
recollection.** Where a claim could only have come from one of those, it is in the list above.
