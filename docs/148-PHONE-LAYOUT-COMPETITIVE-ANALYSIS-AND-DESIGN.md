# 148 — The phone layout: competitive analysis and design

**Status:** §1–§8 accepted; §7's foundation implemented, §9's remainder open.
**Opened:** 2026-09-30. **Decision:** [ADR-044](08-ADR-REGISTER.md).
**Advances:** `105` UX-019 (no breakpoint below 620px), partially `105` UX-018.
**Depends on:** `105` UX-001 (the editable focus owner), **closed** — a phone can
already raise a keyboard and accept a character, which is what makes a phone
layout worth building now rather than earlier.

## 0. What the owner asked for

> "mobile view of the app is not there.. mobile view means.. not web view on
> mobile .. but mobile will have completely separate controls and layout and ..
> no horizontal scroll. first do a competitive analysis first"

Three instructions, and the third governs the first two: **the analysis comes
first**, so §1–§4 are what the references do and §5 onwards is what we do about
it. "Completely separate controls and layout" is read as a claim about the
*experience*, not about the *codebase* — §5.1 and ADR-044 say why those are
different questions and why the answer to the second is "one shell".

## 1. Where this shell is today, measured

Chromium, `editor.html` with a document open, viewport 390×844, on `main` at
`bae5355e`. Numbers from `webapp/tests/e2e/` instrumentation, not from reading
the stylesheet:

| Element | `scrollWidth` | `clientWidth` | What that means |
| --- | ---: | ---: | --- |
| `document.scrollingElement` | 390 | 390 | The frame itself is sound |
| `.ribbon-tabs` | **550** | **109** | 5 of 8 tabs behind a sideways drag |
| `#viewport` | **794** | **326** | The document pans 468px |
| `.app-menu-bar` | clips under a fade mask at 360px (`responsive-shell.spec.mjs` asserts the mask, i.e. the clip is *known*) |

`#viewport`'s client width is 326 in a 390 window because the rail takes 40px
and the viewport's own padding takes the rest.

Three further facts frame the work:

- **There is no rung below 620px** (`105` UX-019). The 620px block sheds
  captions, labels and half the status bar; below it nothing further happens.
- **The whole e2e suite runs one Playwright project, `Desktop Chrome`**
  (`playwright.config.mjs:53`). Five specs set a narrow viewport by hand and
  exactly one — `touch-targets.spec.mjs` — turns on touch emulation.
- **The engine side is already sound.** Page rasters are virtualised and
  DPR-capped, 54 pointer handlers already receive touch, and `(pointer: coarse)`
  already raises menu rows, palette rows and inputs. The gap is the *shell*.

## 2. Google Docs on a phone

Sources: `support.google.com/docs/answer/` 2375082, 1663349, 65129, 6367684,
10296604, 179738.

- **Two surfaces, and Google steers away from one of them.** Answer 2375082:
  *"For the best experience on your mobile device, use the Google Workspace apps
  built specifically for Android, iPhone, and iPad devices."* Mobile web is the
  fallback. *We do not have that escape hatch* — `18-SUPPORT-MATRIX.md` says the
  browser build **is** our mobile story, and `SKILL.md` §1 puts native app shells
  out of scope. So the phone browser has to be as good as their app, not as good
  as their mobile web.
- **View versus edit.** Until July 2023 a document opened read-only with a
  pencil FAB to enter editing. It now opens in edit mode directly. Both states
  still exist conceptually, and in edit mode the formatting toolbar appears.
- **Formatting lives in a bottom sheet.** Answer 1663349: the **Format ("Aa")**
  control opens a panel with a **Text** section (Style, Font, Size, Text colour,
  Highlight colour) and a separate **Paragraph** tab (alignment, line spacing,
  indentation). Google's own text does not use the phrase "bottom sheet" and
  does not state that it coexists with the keyboard; the tab split is quotable,
  the sheet behaviour is observed rather than documented.
- **A toolbar row sits above the keyboard** while editing: undo/redo (also a
  three-finger swipe), B/I/U, an insert `+`, the Aa format control, comment, and
  a keyboard dismiss. The *elements* are corroborated across sources; no single
  Google page enumerates their left-to-right order, so this document does not
  claim one.
- **Comments are a panel reached from a top-bar icon**, with a numeric badge for
  open action items and an unread dot; swipe moves between threads. Commenting
  itself is select text → **More → Add comment** (answer 65129). Suggesting mode
  is toggled from the **⋮ overflow**.
- **The outline opens at the bottom.** Answer 6367684, verbatim: *"Tap the More
  menu, then tap Document outline, and the outline will open on the bottom."*
- **Selection is the platform's.** Android teardrop handles (22dp), the floating
  Cut/Copy/Paste/Select-all action bar (`material.io/archive/guidelines/patterns/selection`),
  and the system Magnifier (`developer.android.com/develop/ui/views/text-and-emoji/magnifier`,
  API 28+) while a handle is dragged. Docs inherits all three rather than
  building them.
- **The page is a user choice.** Answer 10296604: **⋮ → Print layout** toggles
  pagination; **⋮ → Page setup → Pageless** removes page boundaries and reflows
  content as a continuous scroll. In Pageless, a wide table gets its own
  sideways scroll — the overflow is contained to the table rather than given to
  the page.

## 3. ONLYOFFICE on a phone — source-verified

Read from `/Users/sachin/Desktop/melp/reference/{sdkjs,web-apps}`. AGPL-3.0:
**behaviour and structure only, no code taken.**

**They ship a second front end, and only a second front end.**
`apps/documenteditor/mobile` is a Framework7-React application beside the
ExtJS desktop one (`src/view/app.jsx`, `src/page/main.jsx`), with its own
Toolbar, its own router and its own view tree — and there are sibling copies for
the spreadsheet, presentation and Visio editors. What it does **not** have is a
second engine: `src/controller/Main.jsx:179-180` loads the same
`sdkjs/word/sdk-all-min.js` the desktop app loads, and the only difference is a
runtime flag — `common/apiBase.js:66`, `this.isMobileVersion = (config['mobile'] === true)`,
set at `Main.jsx:509-517`. The `--mobile` flag in `build/build.py` is a
*different*, native-embedding SDK variant and is not what the web mobile editor
uses.

So even ONLYOFFICE — who pay for two front ends across four products — draw the
line at the engine. **They have one engine and two shells. We intend one of
each**, and §5.1 says why.

**Chrome.** A top `Navbar` (`src/view/Toolbar.jsx`) carrying back, title,
undo/redo (left on iOS, right on Android, branched on `Device.ios`), search, an
**Edit** button, an **Add** button, collaboration and a settings gear. The
**Edit** panel is a **bottom Sheet on a phone** and a Popover on anything wider
(`src/view/edit/Edit.jsx:278-302`), and inside it is a **tab bar keyed to the
selected object type** — text, paragraph, image, shape, table, chart, header,
TOC (`EditingPage.jsx:68-190`) — that drills down into one-setting-per-screen
sub-pages. **Add** is a full-screen Popup on a phone with the same tab-then-push
shape (`src/view/add/Add.jsx:101-127`). The context menu is a Popover anchored
to an invisible zero-size target the controller moves to the SDK-reported
`(x, y)` on `asc_onShowPopMenu` (`apps/common/mobile/lib/controller/ContextMenu.jsx:140-165`).
Comments and review are a shared **Collaboration sheet** with its own nested
router, reused by all four mobile editors.

**Touch.** One class, `CMobileTouchManagerBase`
(`common/Scrolls/mobileTouchManagerBase.js`, 2,774 lines), specialised per
editor. A mode machine — `None, Scroll, Zoom, Select, InlineObj, FlowObj,
Cursor, TableMove, TableRuler, SelectTrack` — with four tunables worth copying:
`ReadingGlassTime = 750`ms before a held finger becomes a cursor drag,
`MoveMinDist = 20`px before a drag is a scroll, `TrackTargetEps = 20`px of
hit radius around a selection handle, and `MOBILE_SELECT_TRACK_ROUND = 14`
(a 7px drawn radius inside a 20px target — **the same draw-small/hit-big
separation this repository already uses**). There is a real magnifier
(`CheckGlass`, a 100px circle at 2× drawn 25px above the finger, with an
offscreen intermediate canvas to route around a Safari `drawImage` bug).
Double-tap-to-select-word is **not** separate touch code: `onTouchStart` calls
the same `check_MouseDownEvent` click-counter the desktop mouse uses
(`common/Drawings/WorkEvents.js:432-505`), so two taps become a double-click.

**Keyboard.** A hidden `<textarea id="area_id">` (`common/text_input2.js`),
transparent, 8px font, `scaleX(0.2)` — the shape `105` UX-001 already adopted.
**No `visualViewport` anywhere in either repository.** Instead
`src/index_dev.html:6` sets
`interactive-widget=resizes-content` and `Main.jsx:880` listens for a plain
`window` resize.

**Zoom and overflow.** Their viewport meta is
`user-scalable=no, maximum-scale=1, minimum-scale=1, viewport-fit=cover,
interactive-widget=resizes-content`, plus JS suppression of `touchmove` with two
fingers and of iOS `gesturestart/change/end` outside the canvas — native zoom is
gone entirely and all zoom goes through their own pinch handler
(`ZoomValueMin = 50`, `ZoomValueMax = 300`). And, decisively for §8:
`Main.jsx:452-453` calls `this.api.Resize(); this.api.zoomFitToWidth();`
**unconditionally on every open**, overriding the saved zoom. They also ship a
**reflow view**: `api.ChangeReaderMode()` switches to a non-paginated layout
served by a separate, simpler `CReaderTouchManager`
(`word/Drawing/mobileTouchManager.js:833-915`), selected by
`LocalStorage['mobile-view']` / `customization.mobile.standardView`.

## 4. Word on a phone

- **Word for the web** narrows to the **single-line simplified ribbon**:
  *"This is the default ribbon mode and shows your commands in a single line.
  However, with less available space for commands, not all of the buttons you're
  used to seeing will fit on the single line ribbon"* — overflow goes behind
  per-button chevrons and a **More options (…)** menu, with Search as the escape
  hatch (`support.microsoft.com/…/using-the-simplified-ribbon-in-word-for-the-web-c145ac47`).
  Microsoft documents no phone-*specific* mechanism, so "the simplified ribbon
  is what a phone browser gets" is inference and is flagged as such.
- **The Word mobile app moves the ribbon to the bottom.** On an Android phone
  you *"expand or collapse the ribbon by tapping the edit icon or by tapping the
  up and down arrows"*; on iPhone the edit icon shows it and a down arrow hides
  it (`…/show-or-hide-the-ribbon-in-word-for-mobile-devices-b8a5b4c3`). The tab
  set inside it is the desktop's — Home, Insert, Draw, Layout, Review, View —
  one tab's worth at a time. And explicitly: *"If you want to type, first hide
  the ribbon"* (`…/word-for-android-phones-animated-tips-e3b5608a`).
- **No reflow toggle** was found in Microsoft's own documentation, either way.
  Recorded as unverified rather than as an absence.

## 5. What the phone layout IS, for this product

### 5.1 The pattern, named before the design

`SKILL.md` §8 requires the established solution to be named first.

1. **Region presets over one shell** — not a second application.
   `137-DUAL-CHROME-DESIGN.md` is the prior art and it is unusually direct: the
   owner cancelled two selectable desktop chromes, and the sibling editor that
   *did* build them **deleted them**, replacing the idea with "not which of two
   layouts, but **which regions of the one layout are present** — a `chrome`
   preset over a feature map, resolved into per-region visibility flags in one
   file." `chrome_regions.mjs` already implements exactly that mechanism for
   host capability withholding. The phone is a third preset of it.
   §5.4 says why a device class is not the same argument the owner rejected.
2. **One navigation axis per chrome** — `122-ONE-AXIS-NAVIGATION-DESIGN.md`.
   The phone's axis is the **menu bar**, because the phone runs the compact
   chrome; the ribbon is not an axis a phone can hold (§1: 550px in 109px).
3. **Bottom sheets, and a command surface at the bottom of the screen.** All
   three references agree: Google's Aa sheet and keyboard-attached row, Word
   mobile's bottom ribbon, ONLYOFFICE's Edit Sheet. Material 3's modal bottom
   sheet is the specified form. This shell already has one — `.review-sheet`,
   HF-088 — so the phone tier generalises a shape it already ships.
4. **Draw small, hit big.** `style.css` already separates a 9px resize grip from
   a 24px target and a 6px gutter bar from a 14px/24px zone, and ONLYOFFICE
   independently does the same (a 7px selection dot in a 20px radius). WCAG 2.5.8
   Target Size (Minimum), Level AA: *"The size of the target for pointer inputs
   is at least 24 by 24 CSS pixels"*. The phone tier follows the existing habit
   rather than scaling the chrome up.

### 5.2 The chrome that stays, and the chrome that goes

| Region | At ≥ 621px | At ≤ 620px | Why |
| --- | --- | --- | --- |
| Brand mark | shown | **gone** | Names the product to someone already inside it; the only header cell that can be spent without taking a capability with it |
| Ribbon band + tab strip | shown (ribbon chrome) | **gone** | Two navigation systems do not fit; the strip measured 550px in a 109px box |
| Toolbar-mode toggle | shown | **gone** | Offers a chrome the rung does not allow — a live control that cannot work is worse than an absent one |
| Application menu bar | one scrolling row under a fade mask | **wraps to two rows** | It is the axis; an axis may not be abbreviated. Seven short names wrap at 320px and fit at 390px |
| Navigation rail | a column, 40px wide | **a horizontal strip above the document** | Same four destinations; costs height (which a phone has) rather than width (which it has not). §5.3 |
| Compact toolbar | under the header | **docked to the bottom**, above the status bar, above the keyboard | Where all three references put it, and where the thumb is |
| Status bar | in flow | fixed, tracks the keyboard | Carries the language escape hatch, which `style.css` already refuses to shed |
| Side panels | in-flow columns / 340px drawers | **bottom sheets** | A 340px drawer at 390px leaves the document 50px |
| Dialogs | centred cards | **bottom sheets, full width** | A card can then never be the thing that overflows |
| Menus hung off the bottom bar | dropdowns | **upward sheets** | There is nothing below a bottom bar to open into |
| Comment column | margin column | bottom sheet **(already, at 700px)** | HF-088, unchanged |

### 5.3 Two decisions that look wrong and are not — **both since reversed, §5.3a**

**The rail survives.** The obvious move is to delete a 40px column on a 390px
screen, and Google and Word both do without one. It stays because
`#pagesPanel`'s rail tile is that panel's **only** surface — `railPages` carries
no `data-command`, so Pages is in no menu, no band and no palette row. Hiding the
rail would make a panel unreachable at 390px that is reachable at 1280px, which
is the "never a dead control" rule seen from the other side. What changes is the
rail's **axis**, not its presence. Giving Pages a command id is §9's first item,
and `phone_chrome.test.mjs` fails once that happens, so the exemption cannot rot.

**The ruler survives, for the same reason and it is the weaker case.** A ruler
showing 0–3in of an 8.5in page on a 390px screen is not much of a ruler, and
neither reference shows one. It stays because `ruler.mjs` is the only place a
**tab stop** can be set or a positional indent dragged — `setTabStop` has no
other call site and no command id — so hiding it would orphan a capability
exactly as hiding the rail would. Unlike the rail this one is worth revisiting:
a tab-stop command would free 24px of a phone's height, and §9 carries it.

**The menu bar survives, and grows.** It is the compact chrome's single
navigation axis (doc 122), and it was a 21px strip with `overflow-x: auto` and a
fade mask. `responsive-shell.spec.mjs` already measures it genuinely clipping at
360px — the guard asserts the *mask*, i.e. the clip is known and signalled, not
absent. Signalling a clip is the right answer when the window is a laptop that
has been narrowed. It is the wrong answer when the window is the only window
there is: reaching the last menu needs a sideways drag on a bar nobody will
think to drag, and the owner's instruction forbids exactly that. Wrapping costs
the header ~24px and costs the reader nothing.

### 5.3a Both of those exemptions have now been paid off

**2026-09-30, the lane after this one.** §5.3 is kept above as written rather than
rewritten, because the reasoning is the point: two regions survived a rung
*against all three references* on one argument, and that argument was a missing
command id in each case, not a judgement about phones. It said so, and it said the
ruler's case was the weaker of the two and worth revisiting.

Both ids now exist:

- **`view.pages`** joins the View menu's `menuGroup.show` band beside
  `view.outline`, so `#pagesPanel` is reachable from the menu bar and the palette
  and no longer depends on a rail tile.
- **`layout.tabStops`** joins Format's `menuGroup.paragraph` band, where Word has
  filed Tabs… for thirty years, backed by a real dialog. Note this was never only
  a phone defect: `ruler.mjs` held the *only* calls to `setTabStop`, `moveTabStop`
  and `removeTabStop` in the product, so tab stops were a one-surface capability
  (`105` UX-004) at every width, for every user. The phone rung is where it
  became visible.

So `phoneRegions()` now reads `rail: false, ruler: false`, and the phone spends
that height on the document: ~44px of rail strip and ~24px of ruler out of 844px.
The number that matters is not 8% of the window but **~18% of what is left** once
a soft keyboard has taken ~300px of it. Google Docs, Word mobile and ONLYOFFICE
mobile all ship neither, which was the position §5.3 was arguing against and can
now agree with.

What did **not** change: neither region leaves the DOM. The phone tier hides
regions in CSS, which is what keeps `one-axis-navigation.spec.mjs`'s
palette-orphan guard reading the same surfaces at every width (ADR-044's "surface
parity survives by construction"). And the guard moved with the decision rather
than being deleted: `phone_chrome.test.mjs`'s "the rail is kept, because its Pages
tile is the only surface that panel has" fired, as designed, and its replacement
asserts the implication that can now rot — *a region a phone withholds has no
capability that lives only there* — which fails if either id is removed.

### 5.4 Why this is not the thing the owner cancelled

Doc 137 was cancelled because it proposed **two chromes for the same device,
selectable by configuration** — a Word one and a Docs one — which doubles every
surface, every guard and every decision about where a command lives, forever.
This is not that. There is one command registry, one `command_taxonomy.mjs`, one
set of surfaces and one set of guards; a phone paints fewer regions of them and
arranges the rest differently, the way the 620px rung already paints fewer. Doc
122 §6 lists "Switch to Mobile" among the things not adopted from ONLYOFFICE's
File page, with the reason "one responsive shell, no second build" — **that
reason is upheld here, not overturned.** What changes is that "one responsive
shell" now has a rung that is actually designed rather than merely narrower.

### 5.5 Selection, the caret, and the keyboard

The editable focus owner (`105` UX-001, #537) already exists: a 1px transparent
`<textarea id="editorTextInput">` moved to the caret each repaint. That is
ONLYOFFICE's shape and it is why a phone can type at all today.

The keyboard and the docked toolbar coexist through **two mechanisms feeding one
variable**:

- `interactive-widget=resizes-content` in the viewport meta asks the browser to
  shrink the **layout** viewport, so `bottom: 0` moves on its own. This is
  ONLYOFFICE's entire approach and they use no `visualViewport` at all.
- Browsers that ignore it shrink only the **visual** viewport;
  `phone_chrome.mjs` reads that back and publishes
  `--phone-keyboard-inset`, which the toolbar, the status bar and every bottom
  sheet spend. `keyboardInset()` subtracts `visualViewport.offsetTop` so a
  pinch-panned page is not mistaken for an open keyboard.

Deliberately **not** adopted from ONLYOFFICE: `user-scalable=no,
maximum-scale=1`. Suppressing pinch-zoom is defensible when you ship a
canvas-level pinch to replace it; we do not (`105` UX-018 is open), and blocking
magnification with nothing in its place is an accessibility failure rather than
a decision.

Touch **selection** — handles, long-press-to-cursor, the magnifier, pinch — is
`105` UX-018 and is not in this change. §3 records ONLYOFFICE's four tunables so
that work starts from measured numbers rather than from taste.

## 6. No horizontal scroll: what is guarded and what is not

The owner's requirement, stated as a property:

> **At any phone width, no chrome surface scrolls horizontally** — not the
> frame, not the header, not the menu bar, not the rail, not a panel, not a
> dialog, not a menu, not a sheet.

`webapp/tests/e2e/phone-no-horizontal-scroll.spec.mjs` asserts it at **390px and
320px**, in light and dark, over the frame plus every open dialog, menu, panel
and sheet it can reach, by measuring `scrollWidth` against `clientWidth` and
also by measuring that nothing's bounding box crosses the window's edges. It is
driven red by reintroducing an overflow (see §10).

**The named exception is `#viewport` — the document itself — and it is not an
oversight.** A Letter page's text column is 6.5in; at 96dpi that is 624 CSS px.
It cannot be both 390px wide and readable. There are exactly three answers:

1. **Shrink the page to fit.** `view_zoom.mjs` computes this and then refuses
   it: `FIT_ON_OPEN_FLOOR = 0.5`, with the reason recorded in the file — a
   phone fits a Letter page at about **31%**, and the first version shipped
   exactly that, "a postage stamp of a document where the defect had at least
   left readable text to pan across." ONLYOFFICE take this answer anyway
   (`zoomFitToWidth()` unconditionally, §3).
2. **Pan.** What this shell does today: 794px of content in a 326px box.
3. **Reflow** — lay the text out at the viewport's width and stop drawing pages.
   This is what Google ships as **Pageless** and what ONLYOFFICE ships as
   `ChangeReaderMode()` / `CReaderTouchManager`. Both offer it *alongside* the
   paginated view rather than instead of it.

Answer 3 is the right one and **it is engine work** — a layout pass at a width
that is not the section's page width — in `crates/casual-doc-layout`, outside
this change's lane. So this change does not pretend to solve it: the chrome is
guarded, the document is named, and §9 carries the item. Silently lowering the
zoom floor to make a guard pass would have made the editor worse and the guard
meaningless, which is the failure `SKILL.md` §4 and §9 both describe.

## 7. What is implemented

| File | Change |
| --- | --- |
| `webapp/src/phone_chrome.mjs` | **new.** The responsive ladder (`PHONE_MAX_WIDTH = 620`, `REVIEW_SHEET_MAX_WIDTH = 700`), `MIN_TOUCH_TARGET_PX = 24`, `keyboardInset()`, `phoneRegions()`, and `createPhoneChrome()` which sets `body.phone-mode` and publishes `--phone-keyboard-inset`. Dependencies injected, so it is in `module_seams.test.mjs`'s `PURE_MODULES` and its arithmetic is unit-tested in Node |
| `webapp/src/main.js` | Reads the review rung through the ladder instead of declaring it; `setChromeMode` forces compact at the phone rung without disturbing the stored preference; re-applies silently on crossing. **Net −1 line**, so the ratchet drops 16,348 → 16,347 |
| `webapp/src/style.css` | The `body.phone-mode` rung (§5.2), keyed to the same 620px the existing block uses |
| `webapp/editor.html` | `viewport-fit=cover, interactive-widget=resizes-content` |
| `webapp/tests/phone_chrome.test.mjs` | **new.** The inset arithmetic, and drift guards between the module's constants and the stylesheet's literals |
| `webapp/tests/e2e/phone-no-horizontal-scroll.spec.mjs` | **new.** §6's property at 390px and 320px |
| `webapp/tests/review_layout.test.mjs` | The 700px cross-check follows its constant into the new module; unchanged in what it asserts |
| `webapp/tests/module_seams.test.mjs` | Ratchet re-measured; `phone_chrome.mjs` added to `PURE_MODULES` |

Surface parity is preserved **by construction**: the phone tier hides regions
with CSS and moves none of them out of the DOM, so
`one-axis-navigation.spec.mjs`'s palette-orphan guard reads the same surfaces it
always did and no command loses a home.

## 8. Where this deliberately differs from its references

1. **One shell, not two.** ONLYOFFICE ships a whole second front end (§3). We do
   not, and the reason is the product: embeddability is the wedge
   (`SKILL.md` §1), and a host embedding two bundles with two command registries
   and two sets of gating is not an embeddable library. ADR-044.
2. **Pinch-zoom is not suppressed.** §5.5.
3. **No reflow view yet, and it is said out loud.** Both references have one;
   we have the honest paginated fallback and a named gap. §6, §9.
4. **No Aa/+ split.** Google separates formatting (Aa) from insertion (+) into
   two sheets; ONLYOFFICE separates Edit from Add. Ours keeps one bar whose
   overflow is one sheet, because that bar already exists, already folds rather
   than scrolls, and already renders from the shared roster. Splitting it is a
   data change to `COMPACT_TOOLBAR` when it is wanted, not a rewrite. §9.

## 9. What is left, with its boundary named

| # | Item | Lane | Why not here |
| --- | --- | --- | --- |
| 1 | **Reflow / pageless view** — lay out at viewport width, stop paginating | `crates/casual-doc-layout` + a View toggle | Engine work, outside this lane. Until it lands, the document surface is §6's named exception and the phone shows a real page it can pan |
| 2 | **Touch selection** — handles, long-press-to-cursor, magnifier, pinch zoom, scroll-vs-select | `webapp/` + wasm | `105` UX-018, a lane of its own. §3 records ONLYOFFICE's four tunables so it starts from numbers |
| 3 | **A command id for Pages** | `command_taxonomy.mjs` | Would let the rail be withheld on a phone. `phone_chrome.test.mjs` fails the moment it exists, which is the prompt |
| 4 | **Aa / + as their own sheets** | `compact_toolbar.mjs` data | §8.4 |
| 5 | **A phone Playwright project** | `playwright.config.mjs` | Today five specs set a viewport by hand and one enables touch. A `Pixel 7`-shaped project would run the suite as a phone; that is a CI-time decision, not a layout one |
| 6 | **The keyboard-attached row on a real device** | — | `interactive-widget` and `visualViewport` are both implemented and both unit-tested, but Playwright's desktop Chromium has no soft keyboard, so the *integration* is reasoned rather than measured. Stated plainly rather than claimed |
| 7 | **A command id for tab stops** | `command_taxonomy.mjs` + `ruler.mjs` | Would let the ruler be withheld on a phone, which is 24px of height for a control nobody drags with a finger. §5.3 |
| 8 | **The toast over an open bottom sheet** | `style.css` | The toast now clears the docked command bar (found by looking; guarded). It still paints over an open sheet, because `--z-toast` is above the modal on purpose and a sheet occupies the same bottom-start corner. Pre-existing for dialogs, more visible here. Reserving a band above whatever sheet is open needs the sheet's height, which is content-dependent; not half-built |
| 9 | **The comment sheet covers the command bar** | `style.css` | Pre-existing at the 700px rung and *reference behaviour* — Google's comments panel and ONLYOFFICE's Collaboration sheet both take over the screen. Nothing is orphaned (the menu bar and the rail stay visible, and the sheet has its own close), and raising it would collide with `narrow-review-column.spec.mjs`'s "the unoccluded band is >35% of the window" guard. Recorded rather than changed under another lane's guard |

## 10. What looking at it changed

`SKILL.md` §9.4 and the brief both say a passing spec is not evidence that a
layout is usable, so the rung was screenshotted at 390 and 320, in both themes,
with a menu, a dialog, a selection, the overflow sheet and a simulated keyboard.
Two things came out of that and neither would have failed any assertion written
first:

- **The toast lay across the command bar** in every single frame — "Rendering
  1 page at 100%…" over the style picker. The toast is `pointer-events: none`,
  so nothing about clicking the bar could ever have gone red. It is `109`
  HF-233 one surface later, fixed the same way that was (arbitrate by reserving
  the band, not by arguing about z-index), and now guarded by measurement.
- **The bar's own fold is the phone's formatting sheet**, which the design had
  reasoned about but not seen. At 390px it keeps undo/redo/print/painter and the
  style picker inline and folds the rest into `⋯`; at 320px the style picker
  folds too. The `⋯` sheet holds zoom, font, size, B/I/U, colour, highlight,
  link, comment, image, alignment, lists, indent and clear-formatting — which is
  Google's "Aa" sheet, arrived at without writing one.

## 11. Evidence

Every guard added here was driven red before it was trusted (`SKILL.md` §4); the
mutations and their failure output are recorded in the pull request.
