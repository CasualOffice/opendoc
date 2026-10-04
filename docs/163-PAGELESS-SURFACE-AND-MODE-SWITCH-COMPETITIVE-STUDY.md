# 163 — The pageless SURFACE and the mode switch: a competitive study of Google Docs' pageless view against our reflow

**Status:** Research only. **No implementation, no code, no PR.** **Opened:** 2026-10-04.
**Question asked:** the owner's judgement is that reflow's **UX is wrong**, and that the model
to study is **Google Docs' pageless view**. This document answers seven specific questions
before any further implementation.

**Relationship to what already exists.** `151-REFLOW-PAGELESS-LAYOUT-DESIGN.md` designed the
mechanism and `154-READING-VIEW-MEASURE-AND-DOCUMENT-FOLDING-COMPETITIVE-ANALYSIS.md`
re-did the competitive analysis **of the measure** (how wide a line may be) and **of folding**.
ADR-046 and ADR-048 record the decisions. This document deliberately does not revisit either:
**the measure is settled and the arithmetic in `154` is sound.** What neither document examined
is the **surface** — what colour it is, what is drawn on it, what is reachable on it — and the
**mode switch** as an interaction. Those are where the owner's complaint lands, and they are
what is studied here.

> ## 0. How to read the evidence in this document
>
> `SKILL.md` §9 exists because `webapp/fidelity.html` published fabricated claims twice. Every
> paragraph below is tagged with one of five provenances, and the tag is load-bearing:
>
> | Tag | Means |
> | --- | --- |
> | **[G1]** | **First-party Google**, fetched: a `support.google.com` help page, a `workspaceupdates.googleblog.com` post, or `developers.google.com`. The URL is given. |
> | **[G2]** | **Secondary**, fetched: a third-party article. Named and linked. Used only where no first-party page covers the point, and labelled as such every time. |
> | **[M]** | **Measured from our own code** at commit `ac0a9873` on branch `investigate/reflow`, with a file and a line or symbol so it can be re-checked rather than believed. |
> | **[O]** | **Read from ONLYOFFICE's source**, checked out locally, with an absolute path, a line number and an identifier. AGPL-3.0: **behaviour and structure only, no code taken.** |
> | **[U]** | **Unsourced.** Either an inference, labelled as one with its premises, or an outright gap — in which case the experiment that would settle it is named. **Nothing in this document is recollection presented as fact.** |
>
> There is **no [R] (recollection) section in this document**: every claim carries one of the
> five tags above, and where I had only recollection I wrote **[U]** and named the experiment
> instead. `159` §7.3's house shape asks for a paragraph that says plainly when nothing was
> read from a source; this is that paragraph.
>
> **Correction to the premise this study was commissioned under, recorded because it has now
> been got wrong more than once.** The ONLYOFFICE checkouts are **not** inside this repository:
> there is no `reference/` directory under `services/opendoc`. They are at
> `/Users/sachin/Desktop/melp/reference/sdkjs` and
> `/Users/sachin/Desktop/melp/reference/web-apps` — a sibling of the services tree, which is
> what `151` §2.1's own header says. Every **[O]** path below is absolute for that reason.

---

## 1. Background colour

### What Google Docs does

**[G1]** In pageless format the Page setup dialog's *Pages*-only controls are gone and one
control takes their place. Google's own Page setup help page states: *"Important: These
features aren't available in documents that are in pageless format"* — of orientation, paper
size, margins and page colour — and then: **_"If you are in pageless format, you can update the
background color."_**
([support.google.com/docs/answer/10296604](https://support.google.com/docs/answer/10296604?hl=en&co=GENIE.Platform%3DDesktop))

Two things follow from the wording, and they are the finding:

1. The control is renamed. In *Pages* it is **Page color** — the colour of a sheet that sits on
   a desk. In *Pageless* it is **Background color** — the colour of **the surface**. There is no
   second colour, because there is no second region.
2. It is a **document property**, reachable from File ▸ Page setup, and therefore synced to
   collaborators. **[G1]** The Docs API carries it: `DocumentStyle.background` (*"The background
   of the document."*) alongside `DocumentStyle.documentFormat`, whose enum is `PAGES` /
   `PAGELESS`
   ([developers.google.com — Documents REST reference](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents)).

**[G2]** TechRepublic's walk-through of the dialog reports the Pageless tab offering 72 preset
colours plus a Custom colour wheel, against the Pages tab's paper size, margins, orientation
*and* background colour
([techrepublic.com](https://www.techrepublic.com/article/focus-on-content-with-the-new-pageless-option-in-google-docs/) —
secondary; the article itself returns HTTP 403 to automated fetch and the quotation here is from
its indexed text).

**[U] The actual default value.** Multiple secondary write-ups state the default is **white**
and none gives a hex. Google publishes no value. **The inference I will stand behind**, from the
two first-party facts above plus the announcement's framing, is that **pageless has no desk at
all**: one background colour fills the writing surface edge to edge, and the default reads as
white. The premises are (a) the control is "background", not "page", and there is only one of
them; (b) **[G1]** the format *"remove[s] the boundaries of a page"*
([support.google.com/docs/answer/11528737](https://support.google.com/docs/answer/11528737?hl=en&co=GENIE.Platform%3DDesktop)).

> **The experiment that would settle it, exactly.** Open a pageless document at a 1440px
> viewport with View ▸ Text width = Medium. Sample the rendered pixel at x = 20 (outside the
> text column) and at x = 720 (inside it), at the same y. **If the two samples are equal, Docs
> pageless has no desk and the surface is one colour.** If they differ, Docs keeps a desk and
> our shape is right and only our tokens are wrong. Then repeat with Background color set to a
> mid grey to confirm which region the control paints. This is ten minutes of work and it
> decides defect R-2 below, so it should be run before any pixel is changed.

### What ONLYOFFICE does — and this is the uncomfortable half of the finding

**[O]** ONLYOFFICE paints a **page on a desk**, and **their reader mode does not change that at
all.** The canvas fill is `GlobalSkin.BackgroundColor`
(`/Users/sachin/Desktop/melp/reference/sdkjs/word/Drawing/HtmlPage.js:2951-2953`) and the page
fill is decided in `CPage.prototype.Draw`
(`/Users/sachin/Desktop/melp/reference/sdkjs/word/Drawing/DrawingDocument.js:1073-1115`). The
actual values:

| | light | dark interface | dark **document** |
| --- | --- | --- | --- |
| canvas (desk) | `#EEEEEE` (`sdkjs/common/skin.js:50`); CSS `--canvas-background` `#eee` / `#e2e2e2` / `#f3f3f3` per theme | `#666666` (`skin.js:200`); `--canvas-background` `#555` / `#222222` / `#121212` | unchanged |
| page | `#FFFFFF`, hard-coded (`DrawingDocument.js:1075`); `--canvas-content-background` is `#fff` in **every** theme including all three dark ones | **still `#fff`** | `#3A3A3A`, hard-coded in `asc_docs_api.prototype.getPageBackgroundColor` (`sdkjs/word/api.js:14217-14223`), under a `// TODO: get color from theme` |
| page border | `PageOutline` `#BBBEC2` (`skin.js:51`) / `--canvas-page-border` `#ccc` | same | `#616161` (`getPageStrokeColor`, `api.js:14225-14230`) |

**[O]** Inter-page gaps are 20 mm both axes (`HtmlPage.js:155-156`,
`viewBetweenPagesHor/Ver`), and desktop pads the document width by a further 40 mm — explicitly
skipped on mobile (`HtmlPage.js:3190-3191`). **Reader mode swaps only `CDocument.Layout`**
(`SetDocumentReadMode`, `sdkjs/word/Editor/Document.js:17253`); the paint path
(`CEditorPage.paint` → `CPage.Draw`) is untouched. So their reader is a **stack of small white
pages, with borders and 20 mm gaps, on a grey desk** — a repagination onto device-derived paper,
not a reading surface.

**This is the reference our reflow surface actually resembles**, and `151` §2.1 said so without
drawing the conclusion: *"their view still paginates onto pages of size `W × H`"*. We removed the
shadow, the radius and the gap — three more than ONLYOFFICE removes — and kept the two-region
structure. **The owner's complaint is, precisely, that reflow was built against the ONLYOFFICE
reference on this axis while the brief was Docs.**

### What we do today

**[M]** Our reflow surface is **two regions, in both themes**:

| Region | Element | Token | Light | Dark |
| --- | --- | --- | --- | --- |
| the desk | `.viewport` (`style.css:6616-6622`) | `--bg` | `#eef1f7` | `#141619` |
| the tile | `.page-wrap` (`style.css:7006-7012`) | `--paper` | `#ffffff` | **`#ffffff`** |
| tile ink | the raster | `--paper-ink` | `#111318` | **`#111318`** |

`--paper` and `--paper-ink` are declared **once**, in the base `:root` block
(`style.css:105-106`), under a comment that makes the choice explicit and deliberate: *"The
document sheet is white in BOTH themes — the editor themes its chrome, not the page being
authored."* Neither dark block (`:root:not([data-theme]) ` at `style.css:398`, `:root[data-theme="dark"]`
at `style.css:441`) redefines either.

**[M] Reflow changes the surface in exactly one way.** `grep -n "is-reflow" webapp/src/style.css`
returns **two hits, one of them a comment**:

```
7032:#viewport.is-reflow .page-wrap {
9199:   tile styling is `#viewport.is-reflow` above; `FIT_ON_OPEN_FLOOR` is unchanged
```

and the rule is three lines: `box-shadow: none; border-radius: 0;`. Plus `gap: 0`, passed in
JavaScript (`main.js:3647`). **That is the whole of reflow's visual design in an 11,000-line
stylesheet.** Reflow does not change the page colour, the desk colour, the inset, the type, or
anything else.

**[M] What that produces, with the numbers derived rather than recalled.** The default width
step is `reading` (`reflow_view.mjs:334`), whose cap is `80 chars × 0.3991 em × 11 pt × 20` =
**7,024 twips = 468 CSS px** at 96 dpi (`reflow_view.mjs:380-384`, `MEAN_ADVANCE_EM`
`:195-205`), plus two 16px gutters (`REFLOW_GUTTER_PX`, `:102`) = a **500px tile**. Our own
browser guard asserts that figure and asserts there is **"real desk on each side"** of more
than 100px (`tests/e2e/reflow.spec.mjs:407-460`).

So at a 1440px window, **our pageless view is a 500px white rectangle centred on a 940px field
of `#eef1f7`** — a page silhouette with the shadow filed off. In dark theme it is a 500px
`#ffffff` rectangle on `#141619`: a lit strip in a dark room.

### The finding

**Our reflow view is "a page on a desk" with two CSS properties removed.** Docs' pageless is a
**surface**. The `is-reflow` rule removed the two cues that say *sheet* and kept the two that
say *page*: a white rectangle, and a differently-coloured field around it. §6.2's own comment
("What is left is one continuous column of paper-coloured raster, which is what the reader
should see") is an accurate description of what was built and an inaccurate description of what
a pageless view is. **This is the single most visible thing the owner is reacting to**, and it
is not a tuning problem: there is no token, class or branch in the tree that could make the
reflow surface one colour, because reflow has no styling of its own to put one in.

---

## 2. Text colour, contrast, and typography against paged view

### What Google Docs does

**[G1]** Nothing. No first-party page states any difference in text colour, weight, size or
face between pages and pageless. The only typographic lever Google documents for pageless is
line length: *"Go to **View** ▸ **Text Width**. Select an option"* — narrow, medium or wide —
and *"Your text width choice won't affect how collaborators see your docs"*
([answer/11528737](https://support.google.com/docs/answer/11528737?hl=en&co=GENIE.Platform%3DDesktop)).

**[G2] And the text colour is explicitly *not* adapted to the background.** The clearest
evidence is a widely-circulated recipe for "Google Docs dark mode": the author sets *"the
background colour to black/dark grey"* in **File ▸ Page setup** with **Pageless** selected, and
must then separately *"Change the text colour to white"*
([trysmudford.com](https://www.trysmudford.com/blog/google-docs-dark-mode/) — secondary). That
is only a recipe because Docs does no contrast adaptation: changing the background leaves the
ink where it was, and the user is changing a **document property every collaborator and every
export sees**. **A sourced competitive gap, in Google's product, not ours.**

### What ONLYOFFICE does — they DO adapt the ink, and this corrects a claim I first wrote

**[O]** ONLYOFFICE ships a **dark-document mode** that is separate from its dark interface
theme, and it adapts document ink. `asc_setContentDarkMode(isDarkMode)`
(`/Users/sachin/Desktop/melp/reference/sdkjs/common/apiBase.js:5118-5126`) sets a flag; Word's
override drops the page cache and repaints
(`/Users/sachin/Desktop/melp/reference/sdkjs/word/api.js:14213-14221`). Each cached page is then
painted with `g.setDarkMode()`
(`/Users/sachin/Desktop/melp/reference/sdkjs/word/Drawing/DrawingDocument.js:4112-4118`), which
resolves to `_darkMode3`
(`/Users/sachin/Desktop/melp/reference/sdkjs/common/Drawings/GraphicsBase.js:171`).

**The rule, which is more interesting than the existence of the feature:** it is **not** an
inversion. `darkModeCorrectColor2` (`GraphicsBase.js:40-66`) converts to HSL and sets
`L = 255 − (197/255 · L)` — a **lightness flip with a floor**, so pure black ink becomes near-white
and mid-tone author colours move without their hue changing. A separate, stricter rule
(`darkModeCorrectColor`) flips only colours whose three channels are all below
`darkModeEdge = 10`. Form content is deliberately exempted at the first nesting level
(`_darkMode2`, `GraphicsBase.js:150`, with the in-source comment *"don't correct the first level
form"*). The caret gets the same treatment (`DrawingDocument.js:2158-2160`).

Two constraints they impose that matter to any copy of this: **[O]** dark-document is **locked
out unless the interface theme is already dark**
(`/Users/sachin/Desktop/melp/reference/web-apps/apps/documenteditor/main/app/controller/ViewTab.js:281`,
`Common.enumLock.inLightTheme`; same dependency on mobile at
`/Users/sachin/Desktop/melp/reference/web-apps/apps/common/mobile/lib/controller/Themes.jsx:114-128`),
and **dark-document and reader mode are completely independent** — neither references the other
anywhere in the source.

> **CORRECTION.** An earlier draft of this section said *"neither product adapts ink to a reading
> surface"* and *"nobody does it"*. **That was wrong**, and it was wrong in the direction
> `SKILL.md` §9.6 warns about — understating a competitor is as false as overstating one. The
> ONLYOFFICE lane's source read falsified it. Google does not adapt ink (**[G2]**, above);
> ONLYOFFICE does, by HSL lightness flip, gated on a dark UI theme. The claim that survives is
> narrower and still useful: **no reference adapts ink *as a consequence of entering the
> pageless/reader view*. It is an independent control in both products.**

### What we do today

**[M]** Also nothing — and for a stated, defensible reason. `style.css:99-104`: *"Anything
painted onto or over the raster (the caret, the IME preedit, crop handles, the review markers on
the glyphs) therefore has a fixed contrast partner and must NOT follow the theme: a dark-mode
green marker would be drawn onto white paper at ~2:1."* `--paper-ink` `#111318` on `--paper`
`#ffffff` is ~18.8:1; the tracked-change, spelling and grammar marks (`--paper-insert`
`#188038`, `--paper-spelling` `#d93025`, `--paper-grammar` `#1a73e8`) are all calibrated against
white and declared in the same block for the same reason.

**[M] No reflow-specific typography exists.** `reflow_view.mjs` reads the document's default
face and size through `stylePreview("Normal")` (`reflow_chrome.mjs:131`) only to **compute the
cap**; nothing scales the type. That is ADR-046's deliberate choice and `151` §8 item 2 records
it as open: *"a reflow that also changes the type size changes two things at once and cannot be
evaluated."*

### The finding

**On text colour we are behind ONLYOFFICE and level with Docs, and `--paper` is the reason.**
`--paper` is a *fixed light value in both themes by deliberate design*, and the design is sound
for paper — but it means we have **no dark document surface at all**, in any view. ONLYOFFICE has
one with a working ink rule; Docs has a settable background and makes you fix the ink yourself.
We have neither half.

And the coupling is the reason this is not a small change: **the moment the reflow surface
becomes themeable or per-viewer, the whole `--paper-*` family becomes contrast-unsafe.**
`--paper-insert` `#188038`, `--paper-delete`/`--paper-spelling` `#d93025` and `--paper-grammar`
`#1a73e8` are each calibrated against white (`style.css:99-124`), and a dark-mode green marker on
white paper at ~2:1 is the exact failure that comment was written to prevent. ONLYOFFICE's answer
— a lightness flip applied to everything painted outside shapes, with forms exempted — is a
mechanism we could study rather than invent. **Say this out loud before anyone scopes "make the
reflow background themeable".**

---

## 3. The document outline in pageless

### What Google Docs does

**[G1]** The outline is **orthogonal to pageless**. Google's outline help page describes one
behaviour with no mode qualifier, and never mentions pageless
([support.google.com/docs/answer/6367684](https://support.google.com/docs/answer/6367684?hl=en&co=GENIE.Platform%3DDesktop)):

- **Open:** *"click View and then Show outline. The outline opens on the left."*
- **Headings drive it automatically:** *"Google Docs will automatically add headings to the
  outline, but you can also add them manually."* And a documented exclusion: *"Subtitles will
  not appear in the outline."*
- **Per-heading opt-out, both ways:** *"Click Remove from outline"* on a heading, and
  *"Click Add to document outline"* from the text's context menu to restore it.
- **Two closed states, and this is the interesting one.** Temporarily closing it leaves a
  permanent affordance: *"If there's a tick mark next to 'Show outline', the document outline
  icon will still be visible in the top left."* Fully hiding it is View ▸ Show outline again.

So Docs distinguishes **collapsed-but-present** (a persistent one-click icon pinned to the
canvas) from **off** (a menu round-trip to get back). That is a deliberate three-state control
where most products ship two.

**[G1] Collapsible headings are also orthogonal to pageless.** Google's announcement of
expand/collapse never mentions pageless, and states the persistence model: *"Editors of a
document will have the ability to set the default state of headers to expanded or collapsed for
all users"*, while *"Users with view and comment access are able to expand and collapse content
when they have the document open, and when they close the document their expand/collapse changes
will not be saved"*
([workspaceupdates.googleblog.com, 2023-05-16](https://workspaceupdates.googleblog.com/2023/05/expand-and-collapse-content-google-docs.html)).
Two tiers: a saved document default, and an unsaved per-viewer override. **`154` §2.1 already
recorded this and it is re-verified here from the primary post.**

**[U] What happens to the outline when the mode is switched** is not documented by Google. The
inference from everything above — the outline derives from headings, and pageless changes no
heading — is that **nothing happens to it**. The settling experiment is one sentence long: open
a pageless document, show the outline, switch to Pages, observe whether the panel stays open and
whether its scroll position and expansion state survive.

### What ONLYOFFICE does — orthogonal too, and lazily computed

**[O]** `CDocumentOutline` (`/Users/sachin/Desktop/melp/reference/sdkjs/word/Editor/DocumentOutline.js:42`)
holds `this.Use = false` and **`UpdateAll()` early-returns while `Use` is false** (`:61`) — the
outline is not computed at all until the panel is opened, then built from
`GetOutlineParagraphs(..., {OutlineStart: 1, OutlineEnd: 9, SkipTables: true, SkipDrawings: true})`
with a synthetic level-0 "beginning of document" row inserted when the first heading is not at
index 0 (`:72-76`). Driven by `asc_ShowDocumentOutline` / `asc_HideDocumentOutline`
(`sdkjs/word/api.js:11855`, `:11864`). **Not visible by default** — `btnNavigation` is constructed
`disabled: true` with `enableToggle: true` in a `toggleGroup`, so it is also mutually exclusive
with search, comments, chat and thumbnails
(`/Users/sachin/Desktop/melp/reference/web-apps/apps/documenteditor/main/app/view/LeftMenu.js:160-167`),
and the panel is rendered lazily on first open (`LeftMenu.js:291-292`). **Not persisted** — there is
no navigation-open key; the only left-menu key is `de-hidden-leftmenu`. And **unaffected by reader
mode**: neither `controller/settings/Navigation.jsx` nor `view/settings/Navigation.jsx` references
`isMobileView`, `readerMode` or `ChangeReaderMode`.

**[O] Two things worth taking, and one worth noting as a floor.** Take: **the lazy `Use` gate** —
an outline that costs nothing until someone asks is the right shape, and it is the shape
`SKILL.md`'s performance rules demand (our own `documentOutline` once shipped O(n²) for a panel
that returned an empty list). Take: the **synthetic head row**, which answers "where am I before
the first heading". The floor: `role="tree"` has **zero hits** across `web-apps/apps`, so their
`aria-expanded`/`aria-level` sit on elements that are not a tree — we are already past that
(ADR-049).

### What we do today

**[M] Our outline is likewise orthogonal to reflow, and that is correct.**
`grep -n "reflow" webapp/src/outline_panel.mjs` returns **nothing**. Rows come from the engine's
`documentOutline()` as `"{level}\t{node}\t{collapsed}\t{text}"` (`outline_panel.mjs:3`), keyed on
`NodeId` — model positions, not geometry — so a width change cannot invalidate them. Entering
reflow closes the **Pages** panel (`pages_panel.mjs:230`) and leaves the outline alone. **We are
at parity here and should not change it.**

**[M] We are ahead of Docs on one axis.** Our outline is a real ARIA tree since ADR-049:
`role="tree"`, `role="treeitem"`, `aria-level` / `aria-expanded` / `aria-posinset` /
`aria-setsize`, a roving tabindex, and a focusable `<button>` disclosure
(`outline_panel.mjs:14-39`). The same comment records, with citations, that ONLYOFFICE's
`TreeView.js` sets `aria-expanded` and `aria-level` on elements that have **zero** `role="tree"`
hits across `web-apps/apps`, and that its caret is a `<div class="tree-caret">` with no role and
no tabindex.

**[M] And behind on two, both of which bite in reflow specifically.**

1. **There is no persistent affordance for a closed outline.** `#outlinePanel` is `hidden` in
   `editor.html:2653`; the only way back is the rail's `#railOutline` tile
   (`editor.html:2629`). That is one click, so it is not a reachability defect — but **the rail
   turns into a horizontal strip above the document at the phone rung**
   (`style.css:9199`ff, `phone_chrome.mjs`'s `phoneRegions`), which is exactly the rung where
   reflow is on by default. Docs' answer is an icon pinned to the canvas that costs no layout.
2. **The in-document fold chevron is withheld in reflow.** `fold_chrome.mjs:311-337` is explicit:
   *"At the phone rung reflow pulls the text to a 16px inset, so an 18px chevron offset by 20px
   painted at -4px — outside the window."* So the chevron is not painted; and separately, *"The
   chevron is only shown while the outline panel is OPEN"* (`:326-334`, FOLD-006). **Net: in
   reflow on a phone, a heading in the body carries no collapse control at all.** Docs puts the
   chevron next to the heading, in pageless, with no panel open — **[G1]** per the announcement
   above, which gates it on nothing.

   The two comments are each individually right (a clamped chevron that steals a tap is worse
   than none; a chevron whose state nobody refreshes is worse than none). **Together they
   produce a capability that exists and cannot be reached from the document in the view where
   folding is most valuable.** This is `SKILL.md` §9.4 — *"built is not reachable"* — reached by
   two correct local decisions.

---

## 4. "It shows everything without clicking"

The owner's phrase. Read as a design principle, it means: **in pageless, the affordances that
tell you what is in the document and what you can do to it are on the surface, not behind a
menu, a panel or a mode.** Worked out concretely against both products:

### What Docs puts on the surface, by default, in pageless

| Affordance | Evidence |
| --- | --- |
| The whole body, continuously, with no page seam interrupting a sentence | **[G1]** *"remove the boundaries of a page"* (answer/11528737) |
| A heading's collapse chevron, at the heading | **[G1]** the 2023-05-16 announcement, which gates it on no mode and no panel |
| A wide table's own horizontal scrollbar, at the table | **[G1]** *"you can create wide tables and view them by scrolling left and right"* (answer/11528737); **[G2]** *"A horizontal scrollbar will appear at the bottom of the table"* ([digitash](https://digitash.com/google/docs/use-google-docs-pageless-format-ultrawide-tables-images/)) |
| An image at a readable size rather than shrunk to a paper column | **[G1]** *"images will adjust to your screen size"* (answer/11528737) |
| The ruler — **still present in pageless**, carrying the three width options | **[G2]** *"Once your document is in pageless format, you can click on the ruler to hide it. You will also see three width options — narrow, medium & wide"* ([lexnetcg](https://www.lexnetcg.com/blog/google-docs/pageless-documents/)). **Secondary only; see the experiment below.** |
| The outline's persistent icon, once enabled | **[G1]** *"the document outline icon will still be visible in the top left"* (answer/6367684) |
| The fact that a feature is **unavailable** — by the control being absent, not inert | **[G1]** *"you won't be able to add certain features, such as columns, page numbers, headers and footers, and more"* (answer/11528737) |

The last row is the principle, stated by Google in its own voice. **In pageless, Docs removes the
controls that cannot work.** It does not leave them live and silent.

> **[U] Experiment for the ruler row**, because it contradicts one of our decisions and I have
> only a secondary source: open a pageless document and observe whether a horizontal ruler strip
> is drawn above the text, whether it carries indent and tab-stop markers, and whether the
> narrow/medium/wide affordance is on the ruler or only in the View menu. **If the ruler is
> present, our `reflow.rulerWithheld` decision is wrong on the facts** and R-4 below becomes a
> defect rather than a divergence.

### What WE hide, measured

**[M] 1. Exactly one command in the product is gated on reflow.**
`grep -n "reflowView.isOn()\|reflowView.withheldReason()" webapp/src/main.js` returns three
hits: the status bar (`:2862`), the Pages panel's withheld reason (`:10970`) and the
`view.pages` registry row (`:11820`). **Insert ▸ Page number, Insert ▸ Header/Footer, Page
setup, Watermark, Columns, Line numbering and footnote placement are all fully enabled in
reflow and have no visible effect.** `151` §8 item 5 records this as *"what is missing is the
sentence"*; measured, it is worse than a missing sentence — it is **seven-plus live controls
that do nothing a reader can see**, against Google's documented behaviour of removing them.
This is `SKILL.md` §10's *"Never a dead control"* broken the quiet way, and it is the one item
on this list where the competitive standard is a direct first-party quotation.

**[M] 2. The engine's own list of what reflow approximates is collected and shown nowhere.**
`LayoutView::approximations` (`document_layout.rs:312-328`) returns three sentences — a
page/margin-anchored drawing keeping a paper-relative position, a footnote landing at a tile
bottom, a `PAGE`/`NUMPAGES` field printing a refusal. `reflow_chrome.mjs:170` stores them in a
local and exposes `approximations()`; **nothing in `main.js` calls it.** `151` §8 item 8 says so.
So the product *knows* it is approximating and never says it, in any view, behind any click.

**[M] 2a. And the list is a fixed constant, not derived from the document.** It is a
`vec![...]` of three literals keyed only on `is_reflow()`. If it were surfaced tomorrow it would
tell a reader with no footnotes about footnote placement. **Surfacing it is therefore not a
one-line fix**, and that is worth knowing before it is scheduled.

**[M] 3. Two affordances are actively taken away.** The ruler
(`reflow.rulerWithheld`: *"The ruler measures a page and its margins, and there are none in
reflow. Tab stops and indents are on the Layout tab."*) and the Pages panel
(`reflow.pagesWithheld`). Both carry their own sentence, which is good practice and was a
corrected defect (`151` §6.4). **But the ruler's sentence is factually shaky**: indents and tab
stops *are* still live in reflow — `paragraph_decor(properties, width)` (`flow.rs:7270`)
resolves them against whatever width it is handed, which is the whole reason reflow works at all
— so reflow removes the only **direct-manipulation** surface for them in the view where line
length is the reader's active concern. Docs appears to keep it (**[G2]**, above).

**[M] 4. Content that does not fit is not shown, not scrolled, and not reported.** See §5.

### The finding

Three of the four things we hide are hidden **by code that was written on purpose and is locally
well-argued**. That is why this is a UX defect rather than a bug: the design optimised each
withholding in isolation and never asked the question Google answered — *what does a reader need
to see, on the surface, to trust this view?* **Our reflow view currently hides more than our
paged view does, and reports less than the engine knows.**

---

## 5. Wide tables: the per-table horizontal scroller

This is the question with the sharpest answer, and it is not the one `151` claims.

### What Google Docs does

**[G1]** *"In this setting, images will adjust to your screen size, and you can create wide
tables and view them by scrolling left and right."*
([answer/11528737](https://support.google.com/docs/answer/11528737?hl=en&co=GENIE.Platform%3DDesktop)).
Google's announcement frames the whole feature partly around this: pageless *"[adds] more
horizontal space for content like tables and images"*
([workspaceupdates, 2022-02-15](https://workspaceupdates.googleblog.com/2022/02/new-smart-canvas-features-in-google-docs.html)).

**[G2]** The mechanism, from two independent secondary walk-throughs: *"A horizontal scrollbar
will appear at the bottom of the table, allowing readers to scroll through the data without
expanding their browser window"*
([digitash](https://digitash.com/google/docs/use-google-docs-pageless-format-ultrawide-tables-images/));
a wide table gets *"a scroll bar at the bottom so you can view its columns from left to right
easily"*
([howtogeek](https://www.howtogeek.com/803944/pageless-format-google-docs/)).

**[G2]** One further detail that matters for our design, because it is the **edge** behaviour:
when a pageless document is **published to the web**, *"If an image or table is wider than the
text, it will be left-justified and bleed off to the right"*
([lexnetcg](https://www.lexnetcg.com/blog/google-docs/pageless-documents/)). So the overflowing
element is left-aligned to the measure and extends past it — **it is not centred and not
clipped**.

**[U] What I could not source, and it is the important half for us:** how the caret and
selection behave inside a horizontally scrolled table — specifically whether arrowing right
past the visible edge auto-scrolls the table's own scroller, whether a selection drag
auto-scrolls it, whether the scroller is always visible or appears on hover, and whether the
table scrolls independently of the page or the two compose. No first-party or secondary source
covers any of this.

> **The experiment that settles it.** In a pageless Docs document, insert a table of ~15 columns
> so it overflows. Then: (a) click the leftmost cell and hold **Right arrow** — does the table's
> own scroller follow the caret? (b) Drag a selection from the first cell rightwards past the
> edge — does it auto-scroll, and at what rate? (c) Is the scrollbar drawn when the pointer is
> elsewhere? (d) Scroll the table fully right, then press **Home** — does the view return? (e)
> Does the main page scroll at all while the pointer is over the table? These five answers are
> the entire interaction specification, and **they must be obtained before the feature is
> built**, because "a scroller on the table" without caret-following is a trap, not a feature.

### What ONLYOFFICE does — a third answer, and it makes ours the outlier

**[O] They have no per-table scroller either, and the absence is established positively.** The
patterns searched across `sdkjs` and `web-apps/apps` and their hit counts: `TableScroll` 0,
`tableScroll` 0, `scrollTable` 0, `HorizontalScrollTable` 0, `ScrollableTable` 0,
`overflowTable` 0, `tableOverflow` 0, `wideTable` 0, `overflow-x` in any
`apps/documenteditor/**/*.less` **0**. The six hits for `table-scroll` are all the Insert Symbol
dialog's `#symbol-table-scrollable-div`. **Structurally it could not be otherwise**: the body is
one `<canvas>` (`HtmlPage.paint`, `HtmlPage.js:2944`ff), so a table has no DOM node to hang a
scroll container on — **which is exactly our constraint too.**

**[O] Their answer in reader mode is to SHRINK the table.** `getLayoutScaleCoefficient`
(`/Users/sachin/Desktop/melp/reference/sdkjs/word/Editor/DocumentContentElementBase.js:1492`)
reads `CDocumentReadView.GetScaleBySection` (`.../Editor/Layout/ReadView.js:156-169`), which is
`min(W/origW, H/origH)` clamped at 1 — a pure shrink — and it is multiplied into the table grid
(`.../Editor/Table/TableRecalculate.js:281, 879, 1436`) and into cell min/max widths
(`.../Editor/Table/TableCell.js:973`). Because their reader page is `origW / retinaPixelRatio`,
on a 3× phone that coefficient is about **0.33: a wide table is drawn at roughly one third
size**, while the body text around it is *not* shrunk — it is scaled by the independent
`GetFontScale()` reader ladder. They even record the divergence from Word in the source
(`.../Editor/Paragraph.js:19510-19513`: *"In MSWord, in readmode the font size outside a table is
not scaled, it shrinks along with the table itself; we decided for now to scale text inside and
outside tables identically."*)

**[O] And on mobile they disable the document's own horizontal scrollbar outright** —
`checkNeedHorScroll` early-returns with `m_bIsHorScrollVisible = false` when
`this.m_oApi.isMobileVersion` (`HtmlPage.js:1627-1652`). So a too-wide table on their phone is
reached only by pinch and pan.

**The field therefore offers three answers, and ours is a fourth that nobody chose:**

| | answer to "this table is wider than the measure" |
| --- | --- |
| **Google Docs** | contain the overflow in the element: the table gets **its own horizontal scroller** (**[G1]**/**[G2]**, above) |
| **ONLYOFFICE** | **scale the table down** to fit, independently of the body type (**[O]**) |
| **Word** (`151` §2.3 / `154` §2.2, first-party) | Read Mode fits the layout to the device using columns and larger type, both reader-adjustable |
| **us** | **drop the part that does not fit** (**[M]**, below) |

### What we do today — and `151` §6.3 is not implemented

**[M] There is no per-table horizontal scroller anywhere in the product.** The searches, so the
absence is evidence rather than assertion:

- `grep -rni "table.*scroll\|scroll.*table" webapp/src/*.mjs webapp/src/style.css` → two hits,
  neither related (a `page_scroll.mjs` comment about representable positions, and a
  `style.css:8388` bound on a context menu).
- `grep -rn "reflow" crates/casual-doc-layout/src/*.rs` → 40 hits, **none** about table
  overflow.

`151` §6.3 is titled *"Wide tables keep their own scroller"* and is the **only subsection of §6
that does not say "Shipped"**. It was read — here and, I suspect, by the owner — as describing
behaviour. It describes an intention.

**[M] What actually happens to a table that cannot fit.** Three facts compose into one outcome:

1. A table with an explicit `w:tblW` in dxa gets its **declared** width regardless of the space
   available: `solve_column_widths` computes `target` as `WidthSpec::Dxa(v) => v.max(1)`
   (`flow.rs:3093-3103`). `available` is consulted for `Auto` and `Pct` and **ignored** for
   `Dxa`. `distribute_width` only shrinks when `sum > target`, so there is no clamp on this path.
2. A reflow tile's raster is **exactly** `content_width + 2 × gutter` wide
   (`reflow_page_config`, `document_layout.rs:353-379`). Nothing is painted outside it — our own
   engine test asserts that invariant for prose
   (`tests/reflow.rs:554`, `nothing_is_painted_past_the_requested_reflow_column`).
3. The scroll band is the tile's width (`buildPageBand(... )`, `main.js:3647`) and the quantised
   bucket is never wider than `clientWidth` (`reflowMeasure`, `reflow_view.mjs:468-504`), so
   `#viewport.scrollWidth === clientWidth`. **There is no sideways scroll to reach anything
   with**, which is the no-horizontal-scroll guarantee working exactly as designed.

**Therefore: in reflow, a table wider than the column has its right-hand columns dropped from
the raster. They are not clipped-but-scrollable. They are not drawn. There is no scrollbar, no
edge cue, no status message, and `approximations` does not mention it.** The only way to see the
data is to leave reflow — which the product does not tell the reader either.

**[M] The same mechanism applies to an inline image.** `image_item(drawing, ctx)`
(`flow.rs:4924-4938`) takes **no width parameter** and lays the image out at
`extent_to_size(drawing.extent)` — its declared extent, unclamped. Compare `hr_item(rule, width)`
immediately below it (`:4946`), which *does* resolve against the width. So a 7-inch-wide image in
a 468px reading column is painted 7 inches wide and cut off at the tile edge. **Google's
documented behaviour is the opposite** (*"images will adjust to your screen size"*, **[G1]**).

**[M] Our own test suite cannot see any of this.** `a_table_reflows_into_the_column_and_its_tiles_still_trim`
(`tests/reflow.rs:1331`) uses a fixture whose grid is `2_600 + 2_600 = 5_200` twips against a
`COLUMN` of `5_400` (`tests/reflow.rs:70`). **The fixture fits.** The assertion
`right <= GUTTER + COLUMN` is therefore satisfied by arithmetic and could not fail if the
overflow behaviour were arbitrarily bad. This is precisely `SKILL.md` §4 — a guard that cannot
go red — and it is why the gap survived two design documents.

> **The mutation that proves it, to be run when this is fixed:** add a fixture with
> `WidthSpec::Dxa(12_960)` (9in) in a `COLUMN` of 5,400 twips and assert on the painted extent.
> **It should go red today.** If it does not, my reading of `solve_column_widths` is wrong and
> this entire section must be re-derived. I did not run it, because this lane writes no code —
> including no test code — and **that is a stated limit on the confidence of this subsection**:
> it is a careful code read, not an executed experiment.

---

## 6. The complete mode-switch UX

### What Google Docs does

**[G1] It is a document property, set from a dialog, and it syncs.** File ▸ Page setup, choose
Pages or Pageless at the top of the dialog, **OK** — or Format ▸ Switch to pageless format
([answer/11528737](https://support.google.com/docs/answer/11528737?hl=en&co=GENIE.Platform%3DDesktop)).
The API field is `DocumentStyle.documentFormat` with `PAGES` / `PAGELESS`
(**[G1]** [Documents REST reference](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents)),
so it is stored on the file and every collaborator sees the same format.

**[G1] The width, by contrast, is per-viewer and invisible to others:** View ▸ Text width,
*"Your text width choice won't affect how collaborators see your docs."* **Google splits the two
decisions and splits them opposite ways** — format on the document, measure on the viewer.
(`154` §2.1 recorded this; re-verified here.)

**[G1] Pages format has an explicit "Set as default"** button that applies the settings *"to any
drive new documents you create"*
([answer/10296604](https://support.google.com/docs/answer/10296604?hl=en&co=GENIE.Platform%3DDesktop)).
**[U]** Whether the Pageless tab also offers it is not stated first-party; one secondary source
says the dialog offers *"a 'Set as Default' choice for future documents"*
([howtogeek](https://www.howtogeek.com/803944/pageless-format-google-docs/)) without saying which
tab. Experiment: open the Pageless tab and look.

**[G2] It is instant and non-destructive.** *"[You] will see your document update immediately"*,
and switching back from Pageless to Pages restores headers, footers and the rest
([howtogeek](https://www.howtogeek.com/803944/pageless-format-google-docs/)). **[G1]** confirms
the non-destructive half in its own words: hidden elements *"reappear if you switch back to pages
format"*.

**[G1] Nothing is lost by switching**, and this is the design decision underneath it:
*"If your document has certain elements, such as headers and footers, or watermarks, and you
switch it to pageless, you won't be able to **see** those elements"* — they are hidden, not
removed. **[G2]** And they still print: a pageless document is laid out onto pages for
print/PDF/Word export, with page breaks inserted automatically — *"page breaks will be
automatically inserted — but possibly not always where you want them"*
([lexnetcg](https://www.lexnetcg.com/blog/google-docs/pageless-documents/)), and *"Google Docs
formats the printed version to accommodate the elements in the document"*
([howtogeek](https://www.howtogeek.com/803944/pageless-format-google-docs/)).

**[U] Not sourced anywhere, first- or second-party:** whether Docs preserves **scroll position**
or **selection** across the switch, and whether anything is **announced** to a screen reader.
Experiment: place the caret in a known paragraph two thirds down a long document, note the
heading on screen, switch format, and observe (a) which content is on screen, (b) where the
caret is, (c) whether a live region fires — the last needs an AT or a `MutationObserver` on
`[aria-live]`.

### What ONLYOFFICE does — per-user, per-origin, not per document, and announced

**[O]** The choice is **one boolean in `localStorage` under the key `'mobile-view'`** — note: with
**no `de-` prefix**, unlike every other Document-Editor key, so it is shared across all
ONLYOFFICE mobile editors on that origin. Written by `changeMobileView`
(`/Users/sachin/Desktop/melp/reference/web-apps/apps/documenteditor/mobile/src/controller/Toolbar.jsx:306-311`)
and by the Settings toggle
(`.../mobile/src/controller/settings/Settings.jsx:114-121`), both of which do the same three
steps: `LocalStorage.setBool`, flip the store, `api.ChangeReaderMode()`. Read at launch
(`.../mobile/src/controller/Main.jsx:429-436`), where **localStorage wins over the host config**:
`LocalStorage.itemExists('mobile-view') ? getBool(...) : !(customization?.mobile?.standardView ?? false)`.
The host knobs are documented at
`/Users/sachin/Desktop/melp/reference/web-apps/apps/api/documents/api.js:287-291` —
`mobile.forceView` (default `true`), `mobile.standardView` (default `false`),
`mobile.disableForceDesktop`.

**[O] It is announced**, with a sentence rather than a chime: `"textSwitchedMobileView": "Switched
to Mobile view"` / `"textSwitchedStandardView": "Switched to Standard view"`
(`.../mobile/locale/en.json:852-853`), shown as a snackbar from `.../mobile/src/page/main.jsx:356`.

**[O] It is editable, not read-only** — the toolbar icon is shown when
`(isViewer || !Device.phone) && isMobileViewAvailable && …`
(`.../mobile/src/view/Toolbar.jsx:130-141`) and the Settings toggle when
`(Device.phone && !isViewer) || isEditableForms`
(`.../mobile/src/view/settings/SettingsPage.jsx:241-249`); the two gates are complementary, and
neither touches `asc_addRestriction`. **This independently re-confirms `154` §2.3(d)'s correction
of `151` §3.2 from a different direction**, and the decision to stay editable stands.

**[O] A vestigial duplicate worth not copying:** a second `readerMode` observable exists in the
mobile store (`.../mobile/src/store/appOptions.js:145-147`) with no consumer, beside the live
`isMobileView` (`:116-119`) — and `changeMobileView()` is a blind `!this.isMobileView` flip, so
the launch branches are only correct because the seed happens to be `true`. **Two pieces of state
for one fact**, which is the `SKILL.md` §8 "prefer one mechanism" failure in their tree.

**[O] The reader type-size step is NOT persisted and is not reachable from their web UI at all.**
`ReaderFontSizeCur` is a plain field initialised to `2`
(`/Users/sachin/Desktop/melp/reference/sdkjs/word/Drawing/HtmlPage.js:104-105`, ladder
`[12,14,16,18,22,28,36,48,72]`), and `grep -rn "ReaderFontSize"` over `web-apps` returns **zero**
hits. So the nine-step ladder that `151` §2.1 and ADR-048 both cite as ONLYOFFICE's answer to
reading comfort **exists in the SDK and no shipped web user can reach it** — it resets to 16 pt on
every load. That is a material refinement of a claim this repository has made twice: *they grow
the type* is true of the mechanism and **not** of their product's reachable behaviour.

**The scope comparison, then, is 2–1 against Docs rather than 1–1:** ONLYOFFICE stores it
per-user-per-origin and not per document, **as we do**; Google stores it on the document. So our
scope has a precedent, and the real decision in §6a is not "who is right" but which of the two
models we want — and we currently have ONLYOFFICE's scope with a Docs-shaped justification in
ADR-046 §3.4.

### What we do today

| Question | Answer, measured | Where |
| --- | --- | --- |
| Scope of the preference | **Per-viewer and GLOBAL across every document.** One `localStorage` key `docReflow` with no document in it. | `REFLOW_PREF_KEY` `reflow_view.mjs:147`; `readPref`/`writePref` `prefs.mjs:25-40` |
| Width step scope | Per-viewer, global, `docReflowWidth` | `reflow_view.mjs:343` |
| Default when never chosen | `matchMedia('(max-width: 620px)').matches` — see §6a | `reflow_chrome.mjs:102-104` |
| **Zoom preserved?** | **Yes.** `zoomFactor` is untouched by the toggle; `sync(cssPerTwip)` consumes the current zoom. | `main.js:3598-3607` |
| **Selection preserved?** | **Yes.** `setLayoutView` issues no `Operation` and the `selection` variable is not reset; `drawSelection()` re-places it at the new geometry. | `lib.rs` `set_layout_view` doc comment (*"It is a view, never an edit"*); `main.js:3672` |
| **Scroll position preserved?** | **As a pixel offset, not as a document position.** See below. | — |
| **Announced?** | **Yes, and well.** `setStatus` → `statusChannel.publish` → `#statusLiveRegion` (`role="status" aria-live="polite" aria-atomic="true"`), with real sentences: *"Reflow is on: the document is laid out to the window rather than on pages."* | `main.js:2692-2709`; `editor.html:3113`; `locales/en.json` |
| **Instant?** | **No, and knowingly not.** O(document) in both directions because the galley cache is width-scoped; synchronous inside the render pass; **not cancellable and showing no progress** beyond a status line. | `lib.rs` `set_layout_view` (*"Complexity: O(document) … a mode change, not an interaction"*); `151` §8 item 6 |
| **Print** | Forced back to `Paged`, unconditionally, with the restore in a `finally`. **Matches Docs.** | `print.mjs` `withPagedLayout` `:208-228` |
| **PDF export** | `Paged` by construction — the writer re-paginates from the document's own sections and never reads `layout_view`. | `151` §6.4 |
| Reversible / non-destructive | **Yes.** No operation, no revision bump, nothing reaches export. `reflow.spec.mjs:178` asserts Paged→Reflow→Paged returns the same layout. | ADR-046 §3.1 |

**[M] The scroll-position answer, precisely, because it is the one the owner will feel.**
`renderAll` builds the replacement band off-DOM, sizes it, and swaps it in atomically
(`pagesEl.replaceChildren(rulerView.element, bandEl)`, `main.js:3661`), then re-measures
`bandTopInScroller` from the live `scrollTop` (`:3666-3667`). Nothing reads the old scroll
position, nothing translates it into document space, and nothing calls `scrollCaretIntoView`
afterwards — `reflowChrome.set()` is `onChanged()` then `reflect()` (`reflow_chrome.mjs:263-275`).
`grep -rn "scrollAnchor\|preserveScroll\|restoreScroll\|keepPlace" webapp/src` returns **zero
hits**. So the browser keeps the numeric `scrollTop` (clamped to the new scroll height) and the
**content at that offset is different**, because a 468px column and a 624px paper column produce
different galley heights for the same document, and the tiles are trimmed to content
(`trim_reflow_tiles`, `document_layout.rs:488`).

**This is a whole family, not one case** — `MEMORY` already names it ("Fix the family, not the
case — scroll-reset on re-render"). Every `renderAll` has the same property: a zoom change, a
width-step change, a font upgrade, a theme-driven re-render. The reflow toggle is merely the
most violent instance, because it is the one where the mapping from offset to content changes
completely.

> **The experiment:** Playwright, a long fixture, 1440px. Scroll to 50%, read the topmost
> visible heading via the overlay or `documentOutline` + `caretRect`, toggle reflow, read it
> again. **Assert the same heading is at the top.** That spec does not exist
> (`tests/e2e/reflow.spec.mjs` has 15 tests and none mentions scroll), and it should go red on
> `main` today.

### 6a. `PHONE_MAX_WIDTH = 620`: reflow auto-engages on any window ≤ 620px

**[M] Confirmed exactly as the brief states.** `PHONE_MAX_WIDTH = 620`
(`phone_chrome.mjs:50`), and `reflow_chrome.mjs:102-104`:

```js
const wanted = () =>
  chosen === null ? view.matchMedia(`(max-width: ${PHONE_MAX_WIDTH}px)`).matches : chosen === "1";
```

`chosen` is `null` until the reader picks, so **the default is a media query on window width
alone — not a device class, not a pointer type, not a touch capability.** A 1400px desktop with
the browser docked to a quarter of the screen, a 600px embedded iframe, a resized window, a
side-by-side diff: all get reflow with no reader involved. Rotation across the rung re-answers,
because the query is re-evaluated per question rather than resolved at boot — that part is a
correct and deliberate design.

**Is it the right rule? No, on three grounds, and the competitive comparison is the weakest of
them.**

1. **[G1] Docs does not do this.** Pageless is a document property a person sets in a dialog,
   and there is no documented automatic engagement at any width. Google's width-responsive
   behaviour is contained to *within* pageless: *"Line breaks for text will also adjust to your
   screen size, and as you zoom in and out"*
   ([answer/11528737](https://support.google.com/docs/answer/11528737?hl=en&co=GENIE.Platform%3DDesktop)).
   **Docs separates "how the document is formatted" from "how wide the window is". We fuse
   them.**

   **[O] Nor does ONLYOFFICE, and the difference is the axis, not the default.** They *do* default
   their reader on — `mobile.forceView` defaults to `true`
   (`/Users/sachin/Desktop/melp/reference/web-apps/apps/api/documents/api.js:287-291`) — but the
   trigger is **which build and which host config**, i.e. `this.isMobileVersion = (config['mobile'] === true)`
   (`/Users/sachin/Desktop/melp/reference/sdkjs/common/apiBase.js:66`), decided by the embedding
   host once. It is never a CSS media query. **So both references gate on a deliberate signal and
   we gate on `clientWidth`** — ours is the only one of the three where narrowing a window
   changes the document's format.
2. **[M] It contradicts our own ADR.** ADR-046 §3.4's stated reason for per-viewer storage is
   that *"one person's phone and another person's 27in monitor are looking at the same file, and
   a view chosen on the phone must not reformat the desktop."* A width-triggered default means
   **one person's own window, narrowed for a minute, silently reformats their document** — the
   same harm, one scope down.
3. **[M] It is the mechanism that hid the missing cap.** `154` §3.2 and
   `reflow_view.mjs:66-80` both record that the reading column was evaluated **only at 390px**,
   which is why a 241-character line at 1440px shipped. A default that fires on a media query is
   a default whose non-phone cases nobody looks at. `reflow_view.mjs:319-333` says this in as
   many words and then chooses one global default *for the width*, on exactly that reasoning —
   but the **layout** default is still device-class-shaped. **The two halves of the same feature
   draw the opposite lesson from the same incident.**

**[U] What the right rule is, is a decision, not a finding.** Three candidates, and the owner
should pick rather than have one assumed:
(a) **off by default everywhere**, matching Docs, with the phone discovering it through the
command surface — honest, and regresses the phone experience ADR-044 retired an exemption for;
(b) **keep the rung but make it non-sticky and announced**, so a narrow window reflows *for that
window* and never writes a preference — preserves the phone win, removes the surprise;
(c) **gate on `(max-width: 620px) and (pointer: coarse)`**, so a docked desktop window is
excluded — cheapest, and still fuses two questions.
I have no sourced basis for choosing; (b) is the one that satisfies every constraint in ADR-046
and ADR-044 simultaneously, and saying so is a recommendation, not evidence.

---

## 7. The rest of the pageless experience

| Aspect | Google Docs | Us, measured |
| --- | --- | --- |
| **Margins** | **[G1]** Not available in pageless: orientation, paper size, margins and page colour are the features that *"aren't available in documents that are in pageless format"* (answer/10296604). The measure is View ▸ Text width instead. **[G2]** The format *"removes both the empty space around the page and the rigid page-based margins"* (techrepublic). | **[M]** A fixed 16px gutter per side (`REFLOW_GUTTER_PX`, `reflow_view.mjs:102`), passed to the engine as the synthetic page's start/end margin (`reflow_page_config`). Not adjustable. **Four width steps** instead of Docs' three (`REFLOW_WIDTH_STEPS`, `:280-317`). |
| **Column-width control** | **[G1]** Three steps, per-viewer, invisible to collaborators. | **[M]** Four steps — Narrow 55 / **Reading 80 (default)** / Paper / Full — per-viewer, three surfaces (ribbon popover `#viewTextWidthBtn`, View ▸ Text width, four palette rows). **Ahead of Docs here**, with WCAG 2.1 SC 1.4.8 as the sourced basis (ADR-048). |
| **Images** | **[G1]** *"images will adjust to your screen size"*; **[G2]** they no longer shrink to a paper column but *"display at a much larger, readable size"* (howtogeek). | **[M]** Laid out at their declared extent, unclamped (`image_item`, `flow.rs:4924`). A wide image is cut off at the tile edge. **Opposite of Docs.** |
| **Headers / footers / watermarks** | **[G1]** Hidden and un-addable; *"you won't be able to see those elements"*; they reappear on switching back. **Verified — the brief's hypothesis is correct.** **[O]** ONLYOFFICE suppress them the same way, via the view object: `CDocumentReadView.GetSectionHdrFtr` returns `{Header: null, Footer: null, SectPr: <synthetic>}` (`.../Editor/Layout/ReadView.js:94-104`), reached through `CDocument.Get_SectionHdrFtr` (`.../Editor/Document.js:14790`) and consumed by `CHdrFtrController.Recalculate` (`.../Editor/HeaderFooter.js:1737-1790`), which lays out nothing for a `null`. No header margin is reserved either, because `GetPageContentFrame` returns the synthetic 5 mm frame unconditionally. **Do not cite their `IsHeaders()` as the mechanism — it has zero call sites.** | **[M]** Suppressed in layout (`151` §3.3; `reflow_suppresses_every_piece_of_page_furniture`, `tests/reflow.rs:917`) — **but the commands that edit them stay enabled**. See §4 item 1. |
| **Page numbers** | **[G1]** Among the features you *"won't be able to add"*. **[O]** ONLYOFFICE take the worst of the three options: header/footer page numbers vanish with the header, but a `PAGE` field **in the body still renders, numbered against the device-derived reader page grid** — so a reader is shown a page number that matches no printed page. (**[U]** this last step is the ONLYOFFICE lane's inference from read mode still being paginated; it names tracing `ParaPageNum` under `Layouts.Read` as the settling read.) | **[M]** A `PAGE`/`NUMPAGES` field prints a **refusal token** rather than a tile index (`document_layout.rs:136-140`; `a_page_count_field_refuses_rather_than_printing_a_tile_count`), and the status bar shows *"{percent}% through"* instead of "Page 3 of 12" (`status_counts.mjs:69-102`). **Better than Docs' silence, better than Word's count, and better than ONLYOFFICE's wrong number** — the one place our reflow is unambiguously ahead, and now ahead against a measured alternative rather than an assumed one. |
| **Columns** | **[G1]** *"you won't be able to add"* them. | **[M]** A multi-column section is laid out as one column (`a_multi_column_section_reflows_into_one_column`, `tests/reflow.rs:1029`) — but Insert/Layout ▸ Columns stays enabled. Same defect as headers. |
| **Print / PDF** | **[G1]/[G2]** Prints as pages; **[G2]** adjusted to A4, breaks inserted automatically, and hidden headers/footers/footnotes *do* print. | **[M]** `withPagedLayout` forces `Paged` with a `finally` restore; PDF export never reads `layout_view`. **Parity, and ours is structurally sounder** (the export path cannot see the view at all). |
| **Folds and print** | **[G1]** Not documented. | **[M]** `expandFolds(doc)` expands every collapsed heading for the duration of an export, because *"a short PDF is the silent-loss class `AGENTS.md` forbids"* (`print.mjs:231`ff). **Ahead.** |
| **Comments / suggestions in the margin** | **[U] Not sourced.** No first-party or secondary page covers comment placement in pageless. Experiment: anchor a comment in a pageless doc at 1440px with Text width = Medium and observe whether the card sits in a right gutter, and whether the text column shifts off-centre to make room. | **[M]** `--page-width` is published by the renderer and the review gutter is reserved as `padding-right` on `.pages` (`style.css:6665`ff); in reflow `--page-width` is the tile's width, so the mechanism still functions (`151` §6.4). **[U]** Whether the capped, centred column stays centred once the gutter is reserved is not asserted by any test and I did not measure it; `.pages` is `width: max-content` with `align-items: center`, so the arithmetic suggests the column is pushed left by the reservation. Experiment: the existing `paintedColumn` helper in `reflow.spec.mjs`, with the review sidebar open. |
| **Table header-row pinning** | **[G2]** Announced for pageless specifically in 2022 (pin one or more rows as header rows). | **[M]** `w:tblHeader` repetition exists in the flow (`resolve_vertical_merge_geometry` disables it across a crossing merge, `flow.rs:2765`ff) but a tile is not a page, so "repeat on each page" has no referent in reflow. **Not studied here; flagged as unexamined.** |

---

## 8. What this study CONTRADICTS in `151` — the most useful output

`154` already corrected five things in `151` and ADR-048 amended ADR-046. **None of the five is
re-opened here.** These are different, and they are all about the surface and the switch.

| `151` says | This study finds | Provenance |
| --- | --- | --- |
| §6.2 / the `is-reflow` comment: removing the shadow and radius leaves *"one continuous column of paper-coloured raster, which is what the reader should see."* | **Wrong about what the reader should see.** Docs' pageless has one surface colour and a user-settable Background color; ours is a white rectangle on a `#eef1f7` desk, and reflow owns exactly **one CSS rule** in the whole stylesheet. "Paper-coloured raster on a desk" is a page without its shadow. | **[G1]** answer/10296604 + **[M]** `grep -c is-reflow` = 2 |
| **§6.3 "Wide tables keep their own scroller"** — written in the present tense, in a section every one of whose siblings says "Shipped". | **Not built.** No per-table scroller exists in `webapp/` or `crates/`. A `Dxa`-width table wider than the column is **dropped from the raster** with no scrollbar, no cue and no entry in `approximations`. The engine test that looks like it covers this uses a fixture that fits. | **[M]** `flow.rs:3093`, `document_layout.rs:353`, `tests/reflow.rs:70,1331` |
| §6.4: *"The **ruler** is withheld — there are no page margins to drag."* | **The premise is false and the competitive comparison appears to go the other way.** Indents and tab stops resolve against the reflow width (`paragraph_decor(properties, width)`) and are live; **[G2]** Docs appears to keep the ruler in pageless, carrying the three width options. Withholding it removes direct manipulation of the one thing a reflow reader is actively thinking about. | **[M]** `flow.rs:7270` + **[G2]** lexnetcg (secondary — see the §4 experiment) |
| §6.4 / §8 item 5: Page setup and header/footer settings *"still work; they simply have no visible effect … what is missing is the sentence."* | **A missing sentence understates it.** Measured, **exactly one** command in the product is gated on reflow (`view.pages`). Seven-plus live controls do nothing visible. Google's documented behaviour is to make them **unavailable**, which is also `SKILL.md` §10's rule. This is not a copy gap; it is a dead-control gap. | **[M]** `grep reflowView.isOn()` → 3 hits + **[G1]** answer/11528737 |
| §3.4: the per-viewer default *"defaults to on below the phone rung and off above it — which is the only place in this design where a device class decides anything."* | **It is not a device class; it is a media query on window width**, so a docked desktop window and a 600px embed get reflow unasked. It also contradicts §3.4's own justification one scope down, and it is the same evaluation-at-one-width mechanism that hid the missing cap. | **[M]** `reflow_chrome.mjs:102`, `phone_chrome.mjs:50` + **[G1]** answer/11528737 |
| §8 item 8: the approximations are *"collected and not yet surfaced."* | True, **and the list is a fixed three-element constant keyed only on `is_reflow()`**, so surfacing it is not a one-line fix — it would report footnote placement to a document with no footnotes. It also **omits the worst approximation**, which is content dropped for not fitting. | **[M]** `document_layout.rs:312-328` |
| §8 item 1 (anchored floats, *"undecided"*, with *"Docs' Pageless moves them inline"*). | **Still undecided, and I could not source the Docs half.** No first-party page describes anchored-object behaviour in pageless. The **[G2]** data point nearest to it is the published-to-web behaviour: a too-wide element is *"left-justified and bleed[s] off to the right"* — i.e. Docs lets overflow **bleed**, not clip. That is a useful hint and not an answer. **[O]** ONLYOFFICE re-classify forms as inline in read mode (`ParaDrawing.js:1848-1853`, the only other `IsReadMode()` call site in their SDK) — a precedent for the "inline it" answer, for forms only. | **[U]** + **[G2]** lexnetcg + **[O]** |
| §2.1 and ADR-048: ONLYOFFICE *"grow the TYPE instead"* via a *"nine-step ladder … default 16pt"*. | **True of the mechanism, false of their product.** `IncreaseReaderFontSize`/`DecreaseReaderFontSize` are `CEditorPage` methods, **not** on `asc_docs_api`'s export list, and `grep -rn "ReaderFontSize"` over `web-apps` returns **zero** hits — no shipped web UI reaches the ladder and it resets to 16 pt every load. The scale that *is* applied is the constant `16/16 = 1`. So their reader's only real lever is `pageWidth / retinaPixelRatio` plus fit-to-width zoom. **We have been quoting a capability they do not expose.** | **[O]** `HtmlPage.js:104-105, 1090-1107` |
| §6.4: the Pages panel is withheld because *"a navigator whose thumbnails would be TILES"* is wrong to show. | **Right call, and the comparison strengthens it.** **[O]** ONLYOFFICE's left-menu navigator is in a `toggleGroup` with search, comments, chat and thumbnails, so opening one closes the others — and it is untouched by reader mode. Neither reference shows page thumbnails in a non-paged view. **No change needed; recorded so it is not re-litigated.** | **[O]** `LeftMenu.js:160-167` |
| Nothing in `151` or `154` addresses the mode switch as an interaction. | **Scroll position is kept as a pixel offset and lost as a document position**, in every direction, with zero `restoreScroll`-class code in the tree and no test. Zoom and selection survive; the switch is announced properly. | **[M]** `main.js:3661-3667`, `reflow_chrome.mjs:263` |

**What `151` got right and this study confirms:** reflow as a layout **view** and not an edit
(ADR-046 §3.1 — verified in the `setLayoutView` doc comment and in `export_as_inner` never
reading `layout_view`); staying editable; the `PAGE`-field refusal and the `% through` status
line, which are better than both references; print and PDF forced to paper; the width cap and
its four steps (ADR-048); the outline being orthogonal; two withholdings getting two sentences.
**The mechanism is sound. The surface and the switch are not.**

---

## 9. Ranked UX defects, worst first

Each row: what is wrong, the competitive behaviour it violates with its provenance, and a
one-line statement of the fix. **No code, and no implementation is proposed or begun here.**

| # | Defect | Violates | Fix, in one line |
| --- | --- | --- | --- |
| **R-1** | **Content wider than the reading column is silently dropped from the raster** — a `Dxa`-width table's right columns, a wide inline image's right edge. No scrollbar, no cue, no report, no way to reach it but leaving reflow. **[M]** | **[G1]** *"you can create wide tables and view them by scrolling left and right"* and *"images will adjust to your screen size"* (answer/11528737); **[G2]** the table gets its own scrollbar at its bottom; **[O]** ONLYOFFICE scale the table to fit instead — so **all three references keep the content and we are the only one that loses it.** And `AGENTS.md`'s *no silent data loss*. | Keep the content: clamp an inline image's painted extent to the measure, and for a table pick between Docs' per-element scroller (§5's five-part experiment first, and spike the canvas overlay) and **ONLYOFFICE's scale-to-fit, which is cheap, loses nothing and is already strictly better than today** — the one thing that must not ship is the clip. |
| **R-2** | **The reflow surface is a page silhouette** — a `#ffffff` tile on a `#eef1f7` (or `#141619`) desk, with >100px of desk asserted on each side by our own test. Reflow owns one CSS rule. **[M]** | **[G1]** Pageless replaces *Page color* with **Background color**, the colour of the surface, and *"remove[s] the boundaries of a page"*. **[O]** ONLYOFFICE's reader keeps the page-on-desk structure (`#fff` page, `#EEEEEE`/`#666666` canvas, 20 mm gaps, borders) — **so this defect is us matching the wrong reference**, which is the finding rather than a mitigation. | Give reflow a surface: one continuous background behind the measure, so the column is where the text is and not where the paper is — pending the §1 pixel-sample experiment, which decides whether the desk goes or only changes colour. |
| **R-3** | **Controls that cannot work stay live and silent.** Exactly one command is gated on reflow. Page number, Header/Footer, Columns, Watermark, Page setup and line numbering all run and change nothing a reader can see. **[M]** | **[G1]** *"you won't be able to add certain features, such as columns, page numbers, headers and footers, and more"*. And `SKILL.md` §10: *never a dead control*. | One predicate — "this command edits page furniture" — disables each with its own reason in reflow, the way `view.pages` already is. |
| **R-4** | **The ruler is withheld on a false premise.** Indents and tab stops are live in reflow; the ruler is the only direct-manipulation surface for them, and reflow is where line length is the reader's active concern. **[M]** | **[G2]** Docs appears to keep the ruler in pageless, carrying the width options (secondary; **settle it with the §4 experiment first**). | Draw the ruler over the reflow measure with the page-margin handles withdrawn and the indent and tab handles kept — and correct `reflow.rulerWithheld`, which currently states something untrue. |
| **R-5** | **The mode switch loses the reader's place.** The pixel offset survives; the document position does not. No re-anchor, no caret reveal, zero `restoreScroll`-class code, no test. **[M]** | **[U]** Not sourced for Docs (§6 experiment named) — but it violates our own rule directly: `MEMORY`'s *"Fix the family, not the case — scroll-reset on re-render"*. | Anchor on a model position before any `renderAll` and restore to it after — **the whole family** (reflow toggle, width step, zoom, font upgrade), not the reflow case. |
| **R-6** | **Reflow auto-engages on any window ≤ 620px** — a docked desktop window, a 600px embed, a split screen — from a media query alone, and then persists the choice globally for every document. **[M]** | **[G1]** Docs' pageless is set deliberately per document; its only width-responsive behaviour is *within* pageless. And ADR-046 §3.4's own reasoning, one scope down. | Decide between the three candidates in §6a — my recommendation is (b): keep the narrow-window behaviour, make it non-sticky and announced, so a window never writes a preference. |
| **R-7** | **The engine reports what reflow approximates and the product never says it** — in any view, behind any click. And the list is a fixed constant that would over-report if surfaced, and omits R-1. **[M]** | `SKILL.md` §9.4 (*built is not reachable*) and `AGENTS.md` (*unsupported document data must be preserved where safe or reported explicitly*). | Make `approximations` document-derived rather than constant, add the overflow case, and give it one reachable surface. |
| **R-8** | **A heading in the body carries no collapse control in reflow** — the chevron needs a margin wider than 16px *and* the outline panel open. **[M]** | **[G1]** Docs' collapsible headings are gated on no mode and no panel; the chevron is at the heading. | Put the disclosure inside the measure rather than in a margin reflow does not have, and refresh its state without requiring the panel. |
| **R-9** | **Entering and leaving reflow is O(document), synchronous, uncancellable, and shows only a status line.** **[M]** (`151` §8 item 6, still open.) | `SKILL.md` §8's performance rules: anything O(document) is off the main thread, shows progress, and is cancellable. | Route the mode change through the same background/progress path an open uses, as ADR-046's own consequence requires. |
| **R-10** | **No persistent affordance for a closed outline**, and the rail that holds the only one turns into a horizontal strip at exactly the rung where reflow is on by default. **[M]** | **[G1]** *"the document outline icon will still be visible in the top left."* | A canvas-pinned outline affordance that costs no layout, independent of the rail's axis. |

**Where the ranking comes from.** R-1 and R-2 are above the rest because R-1 is data the reader
cannot see or reach and R-2 is the thing the owner reacted to. R-3 is third because it is the
only defect whose fix is both small and quoted verbatim from the competitor. R-5 is below R-4
only because it is a family-wide defect whose correct fix is not reflow-shaped.

---

## 10. Things the owner's description implies that NEITHER Docs NOR we currently do

The brief asks for these explicitly, so a decision can be made rather than assumed. **[U]
throughout: these are gaps in the field, established by the absence of any source, not findings.**

1. **A per-viewer reading background.** Docs' Background color is a **document property** visible
   to every collaborator and present in every export; it is not a reading comfort. **[O]**
   ONLYOFFICE's dark-document mode *is* per-viewer (`content-theme` in `localStorage`, cross-tab
   synced) but is **not** a reading-view concept — it is gated on the interface theme and
   independent of reader mode. We have neither. *"It shows everything without clicking"* with a
   dark chrome implies a surface that follows the reader **and** follows the view, which **neither
   product offers as one thing**. If we want it, the nearest prior art is ONLYOFFICE's mechanism
   with Docs' scope — and §2 names the real cost: the whole `--paper-*` marker family is
   calibrated against white.
2. **Ink adaptation tied to the reading view.** **[O]** ONLYOFFICE adapts ink (HSL lightness flip,
   `darkModeCorrectColor2`) but only in dark-document mode, which a reader must turn on separately
   and which requires a dark UI theme; **[G2]** Docs makes you set the text colour by hand as a
   document property. **So the capability exists in the field and the composition does not.** If a
   reading surface can be dark, the ink question has to be answered, and "the author's colour" and
   "readable" are in genuine conflict for a document whose runs carry explicit colours.
   ONLYOFFICE's form exemption (*"don't correct the first level form"*) is evidence that the
   conflict is real and that they hit it.
3. **Telling the reader what the view is withholding, in one place.** We have per-control
   sentences (good) and an unused `approximations` list; Docs has a help page; **[O]** ONLYOFFICE
   has a snackbar that names the mode (*"Switched to Mobile view"*) and nothing about its
   consequences. **No product has a "what this view is not showing you" surface.** If the owner
   wants one, it is novel and needs designing, not copying.
4. **A reader type-size control.** `151` §8 item 2, still deferred. Word's Immersive Reader ships
   both a size and a column width; Docs ships only the cap (**[G1]** Text width); we ship only the
   cap. **[O] And the ONLYOFFICE comparison needs correcting**: the nine-step ladder this
   repository has twice cited as their answer is **unreachable from their shipped web UI** —
   `IncreaseReaderFontSize`/`DecreaseReaderFontSize` are `CEditorPage` methods that are not on
   `asc_docs_api`'s export list, and `grep -rn "ReaderFontSize"` over `web-apps` is **zero hits**,
   so it resets to 16 pt on every load. So **nobody in the field ships a reachable reader
   type-size control except Word.** ADR-048 notes that with the cap in characters, a size control
   becomes a consequence rather than a second policy — so this is cheap, still unbuilt, and the
   competitive bar is lower than we have been assuming.
5. **Un-justifying a justified document in a reading view.** WCAG 2.1 SC 1.4.8 item 3.
   `151` §8 item 13 / `154` §8 item 7. No reference offers it. Still open.
6. **Caret-following inside a per-element scroller.** If R-1 is fixed with a scroller, the
   caret, selection drag, Find-match reveal and screen-reader focus all have to drive that
   scroller. **No source documents how Docs does it** (§5's experiment), **[O]** ONLYOFFICE has no
   scroller to study, and we have no precedent in the tree. This is the part of R-1 that is design
   work rather than plumbing, and it should be scoped as such.
7. **A scroller over a canvas at all.** **[O]** Both our body and ONLYOFFICE's are a single canvas
   with no DOM node per table, which is *why* neither has a per-table scroller. Docs' editor is
   also canvas-based in its current rendering, so a scroller **is** achievable over a canvas — but
   we have no source for how, and no component in our tree that overlays an interactive scroll
   region on a page raster. **This is the technical unknown that decides R-1's cost**, and it
   should be spiked before R-1 is estimated: the alternative is ONLYOFFICE's answer (scale the
   table), which is cheap, honest, loses nothing, and is strictly better than what we do today.

---

## 11. What I did not do, stated plainly

- **No code was written and no source file was changed.** This document is the only addition.
- **Nothing was executed.** §5's conclusion about `WidthSpec::Dxa` is a careful read of
  `flow.rs:3093-3103`, `document_layout.rs:353-379` and `main.js:3647`, **not an observed
  failure.** The mutation that would prove it is named in §5 and should be the first thing run
  when R-1 is picked up.
- **No Google Docs product was operated.** Every **[G1]** claim is from a fetched Google page and
  every **[G2]** claim is from a fetched third-party article, both with URLs. Eight questions
  that only a live session can answer are named as experiments, in §1, §3, §4, §5, §6, §6a and
  §7 — and four of them (the pixel sample, the ruler, the table caret, the scroll anchor) each
  decide a ranked defect.
- **ONLYOFFICE's source was read in a parallel lane** and is cited inline as **[O]** throughout.
  Two existing repository claims were **re-verified exactly** against it: the reader measure is
  `sectPr.GetPageWidth() / AscBrowser.retinaPixelRatio` with a flat 5 mm margin
  (`HtmlPage.js:1090-1107`, `ReadView.js:72-85`), and `ChangeReaderMode` has **zero** hits under
  `apps/documenteditor/main` — three hits in the whole of `web-apps`, all under
  `apps/documenteditor/mobile` — so **they ship no desktop reading view** and on that surface they
  are not a reference. One existing claim was **refined**: the nine-step type ladder is
  unreachable from their web UI (§10 item 4). One claim **of mine** was **falsified** and the
  correction is kept visible in §2 rather than quietly edited out: they *do* adapt document ink.
  Seven things the lane could not establish from source are listed in its own report, each with
  the read that would settle it; the two that bear on this document are whether a body `PAGE`
  field renumbers to the reader grid (§7) and whether their multi-column collapse is real
  (inferred from the synthetic `SectPr`, not traced to `GetColumnCount`).
- **No ONLYOFFICE code was copied.** Their source is AGPL-3.0; every **[O]** claim above is
  behaviour, structure, an identifier or a colour constant, which is the same standard `151` §2.1
  and `157` already work to.
- **No tracker row was added.** `SKILL.md` §7 requires the row in the **implementation**
  commit; this is a research commit with no implementation, and adding a row for work nobody has
  started would claim progress. The ten R-rows above are what a tracker row should be minted
  from when the work is scheduled.
