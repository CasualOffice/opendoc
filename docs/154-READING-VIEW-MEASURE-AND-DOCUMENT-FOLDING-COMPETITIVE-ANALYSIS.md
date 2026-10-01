# 154 — The reading view: measure, and document folding. Competitive analysis and recommendation

**Status:** Analysis and recommendation, 2026-10-01 — nothing built then, deliberately,
because the owner's instruction was competitive analysis first and `SKILL.md` §8 says name
the established solution before inventing one. **§5.1 and §5.2 BUILT 2026-10-02** on the
owner's instruction to fix it: the cap, the four-step per-viewer control, the centred
column, and the ceiling that closes §3.3's refusal. ADR-048 is now **Accepted** and carries
the record of what implementation changed about it. **§5.3 (folding): ADR-049 is **Accepted** as of
2026-10-02 and its PERSISTENCE tier is built** — `w15:collapsed` parsed, modelled
tri-state, written, round-tripped, with `fixtures/generated/collapsed-headings.docx` and six
mutation-proven guards. The layout filter and the chrome are **not** started. Up to that
point ADR-049 stayed Proposed while ADR-048 was implemented, which is the independence claim
§5.3 makes being honoured rather than merely asserted. §8 below marks each item.
**Corrected and extended by** `157-ONLYOFFICE-LARGE-DOCUMENT-AND-FOLDING-SOURCE-FINDINGS.md`
— three citation/enumeration corrections (its §6) and the evidence that closed ADR-049.
**Opened:** 2026-10-01.
**Proposes:** [ADR-048](08-ADR-REGISTER.md) (the measure is capped; two width policies over
one layout mechanism) and [ADR-049](08-ADR-REGISTER.md) (folding is a per-viewer block
visibility filter, not a layout view).
**Corrects:** `151-REFLOW-PAGELESS-LAYOUT-DESIGN.md` §1, §2.2, §2.3, §3.2 and §6.2, and
ADR-046's Word/ONLYOFFICE evidence. Each correction is marked in place in those documents
and cross-referenced from §6 here.
**Depends on:** nothing. Blocks: any further reflow work, because §5 changes which number
the shell passes to a seam that is already built and already correct.

> **What this document concludes, before the evidence.**
>
> 1. The reading column is **uncapped**, and that is a defect with a number, not a matter
>    of taste. At a 1440px window it is **1408 CSS px — 241 characters** of the document's
>    default face. Every reference caps it. §3 and §4.
> 2. The same missing cap makes reflow **fail outright** above a reachable window width:
>    `setLayoutView` refuses a column over 22in, which a **2160px window at 100% zoom**, or
>    a **1104px window at 50% zoom**, already asks for. §4.3.
> 3. `151` §2.3's "Word abstains" is **wrong**. Word ships four relevant things and one of
>    them — Immersive Reader — has an explicit four-step **Column Width** control whose
>    documented purpose is line length. Word is the strongest vote *for* capping. §2.2.
> 4. `151` §3.2 and ADR-046's claim that **ONLYOFFICE's reader mode is read-only is wrong**,
>    in source. The two flags it cites are touch-gesture flags. Our editable choice was
>    right; the reason given for it was not. §2.3.
> 5. **Folding is not part of reflow** and must not be built into it. It is a per-viewer
>    block visibility filter keyed on outline level, orthogonal to `LayoutView`, and it has
>    a document-level seed we do not currently parse (`w15:collapsed`). §5.3.
> 6. Reading and pageless authoring are **two width policies over one layout mechanism**,
>    not two modes and not one. §5.2.

## 1. The owner's challenge, and which part of it is arithmetic

The owner's words, 2026-10-01:

> *"reflow is not how google docs handles it.. its for reading purpose and acts like prose
> or modern readers, like collapsible and expandable based on outlines like VS Code does
> with code... and the complete view and wider content is not at all suitable for reading.
> do a competitive analysis first."*

Three claims are in there and they have different standing:

| Claim | Standing |
| --- | --- |
| "the complete view and wider content is not at all suitable for reading" | **A measurement.** §3 gives the number. Not a preference. |
| "collapsible and expandable based on outlines like VS Code does with code" | **A missing feature**, present in both of the two references that have it, absent here entirely. §5.3. |
| "reflow ... its for reading purpose" | **A product question** — whether what we built is the reading view, the authoring view, or a conflation of both. §5.2. |

## 2. Prior art, in source or in first-party documentation

Sourcing discipline, because this repository has published false competitor claims twice
(`105` EV-007, `99` §9): every row below is marked **[SRC]** (read in checked-out source,
with `file:line`), **[1P]** (the vendor's own documentation, fetched), **[2P]** (secondary,
converging), or **UNVERIFIED**. §7 collects every unverified claim in one list so that no
reader has to scan for them. **Nothing in §5's recommendation rests on a [2P] or
UNVERIFIED row**; where one is cited it is cited as context.

### 2.1 Google Docs — Pageless, Text width, and collapsible headings

**Pageless.** File → Page setup → Pageless, or Format → Switch to pageless format.
Content "continuously scrolls without page breaks"; images adapt to the viewer's screen;
a wide table gets a horizontal scroll of its own rather than wrapping. Unavailable in
pageless, and restored on switching back: headers, footers, page numbers, watermarks,
columns, margins, page breaks. [1P — `support.google.com/docs/answer/11528737`]
Announced in the Smart Canvas update of 2022-02-15. [1P —
`workspaceupdates.googleblog.com/2022/02/new-smart-canvas-features-in-google-docs.html`]

**Pageless is a document property.** It is in the Docs API: `DocumentStyle.documentFormat`
carries a `DocumentMode` of `PAGES` or `PAGELESS`, and `pageSize`, `flipPageOrientation`,
the four margins, the header/footer ids and `pageNumberStart` all carry the note that they
are "not rendered" when the mode is `PAGELESS`. [1P — Google's own generated client
reference for `DocumentStyle`] So every viewer of a pageless document sees it pageless.

**Text width is NOT a document property — and this is the half `151` missed entirely.**
View → Text width, for pageless documents, offering **Narrow, Medium or Wide**, and the
support page states in as many words: *"Your text width choice won't affect how
collaborators see your docs."* [1P — `support.google.com/docs/answer/11528737`] There is
**no text-width field anywhere in the Docs API** — consistent with it being view state, not
document state. [1P, by absence; a negative cannot be proven exhaustively]

So **Google split the two decisions**, and split them the opposite way round in each case:

| | Google | opendoc today |
| --- | --- | --- |
| Whether the document is pageless | **document property**, shared | per-viewer preference (`prefs.mjs` `docReflow`) |
| How wide the reading column is | **per-viewer**, three steps, not in the file | **not a choice at all** — always the window |

`151` §2.2 records the first row and calls it a deliberate divergence (§3.4), which it is
and which §5.4 keeps. It never mentions the second row, and the second row is the one that
matters: **Google's answer to "how wide" is a capped, reader-chosen measure.** A design
document that cites Google's Pageless as prior art and omits its width control has cited
half the prior art.

**Collapsible headings.** Announced 2023-05-16 (Rapid Release). Hovering a heading reveals
a chevron; clicking it collapses the heading's content. The persistence model is explicitly
two-tier: *"Editors of a document will have the ability to set the default state of headers
to expanded or collapsed for all users"* — saved in the file — while *"Users with view and
comment access are able to expand and collapse content when they have the document open,
and when they close the document their expand/collapse changes will not be saved."* [1P —
`workspaceupdates.googleblog.com/2023/05/expand-and-collapse-content-google-docs.html`]

That is **the same split as Pageless-vs-Text-width**: a document-level default, and a
per-viewer live state that is not persisted. §5.3 adopts it, because it is the only model
that answers both "my collaborator collapsed my document" and "Word saved a collapsed
state into the file".

Folding in Docs is **Pageless-only**. [2P, consistently reported, e.g. 9to5google
2023-05-17; not pinned to a first-party sentence — UNVERIFIED as first-party] §5.3 treats
that restriction as a limitation of Google's implementation rather than as a model, because
Word's folding works in its paginated view and because coupling two orthogonal settings is
the specific mistake this document exists to stop.

**The document outline panel is a different feature from folding**, in Docs as everywhere
else: the left-hand panel has always had its own expand/collapse arrows over the *outline
tree*, which is navigation and does not hide body content; the 2023 chevron collapses the
*body*. [2P, converging; the first-party blog post describes only the in-document chevron]

### 2.2 Microsoft Word — four different things, and `151` got this section wrong

`151` §2.3 says, in full, that Microsoft documents no reflow toggle "in either direction",
that Web Layout "is not documented for the web or mobile clients", and therefore that "the
vote is Docs and ONLYOFFICE for, Word abstaining." **That is wrong in three ways**, and it
is wrong in the direction that mattered: it removed from the record the one reference with
an explicit, named, documented line-length control.

**(a) Web Layout is the analogue of what we built — including its flaw.** Web Layout shows
the document as it would appear as a web page: no page boundaries, no margins, no discrete
pages, headers and footers hidden, page-number insertion greyed out, and **text wrapped to
the window width rather than to a measure**. It is an ordinary editing view, alongside Draft
and Outline. [2P — converging secondary write-ups of Microsoft's own View-tab tooltip and of
Microsoft Q&A answers; direct fetches of `support.microsoft.com` returned HTTP 403] So:
Word does have an editable, uncapped, window-width pageless view — and **we have built
exactly it, flaw included.** Word does not abstain; Word supplies the counter-example.

**(b) Read Mode reflows into adjustable columns and is not the same thing.** [1P —
`support.microsoft.com/en-us/word/read-documents-in-word`, fetched] *"Read Mode
automatically fits the page layout to your device, using columns and larger font sizes,
both of which you can adjust."* It exposes a **Column Width** choice (reported as Narrow /
Wide, with a Paper Layout toggle) and a **Page Color** choice (White / Sepia / Inverse).
[2P for the exact option labels; the behaviour — adjustable columns and larger type — is
1P from the quoted sentence] Note what the 1P sentence establishes on its own: Word's
reading view reaches a comfortable measure **by two levers at once, columns and type size**,
and lets the reader move both.

**(c) Immersive Reader has an explicit measure control, and this is the decisive row.**
[1P — `support.microsoft.com/en-us/accessibility/word/use-immersive-reader-in-word`,
fetched] Text Preferences → **Column Width**, with four named steps — **Very Narrow,
Narrow, Moderate, Wide** — and the page states the purpose outright: it *"changes line
length to improve focus and comprehension."* Alongside it: Text Size, Text Spacing, page
colour including Sepia and Inverse, Line Focus (one, three or five lines), Read Aloud,
Syllables, Parts of Speech, Picture Dictionary, Translation.

**And Immersive Reader is not read-only**: the same page states that *"Once you click in
your Word document to read or edit, the Immersive Reader ribbon will minimize."* [1P] That
matters for §5.2: `151` §3.2 treated "editable" as our divergence from the field. It is not
a divergence. Microsoft's own reading view is editable too.

**(d) Focus mode is chrome-hiding, not reflow.** View → Immersive → Focus hides the ribbon
and the status bar and leaves the text; pagination and measure are unchanged, and it stays
editable. [2P, converging; the primary page 403'd] So Focus is the analogue of the
*reduced-chrome* half of a reading view and of nothing else — which is why §5.2 treats
chrome reduction as a third, independent setting rather than as part of a layout mode.

**(e) Word for the web was pageless first.** Microsoft's own Microsoft 365 Insider blog
announced *"View your document across separate pages in Word for the web"*, describing
switching "back and forth between Separate Pages view and the original continuous page
view". [1P — `techcommunity.microsoft.com`; the article body was only reachable in summary,
so the exact phrase is 1P-sourced but not re-verified word for word] The plain reading is
that Word for the web's original rendering was continuous and pagination was added as an
option. `151` §2.3's "not documented for the web" is therefore stale.

### 2.3 ONLYOFFICE — source-verified, and `151` §3.2's premise is false

Read from `/Users/sachin/Desktop/melp/reference/sdkjs` and
`/Users/sachin/Desktop/melp/reference/web-apps`. AGPL-3.0: **behaviour and structure only,
no code taken.**

`151` §2.1 describes the mechanism correctly and §5.1 here does not repeat it. Four things
it got wrong or did not know:

**(a) Their reader mode does NOT cap the measure.** `SetNewMobileMode` takes the **first
section's own page width** — `sectPr.GetPageWidth() / AscCommon.AscBrowser.retinaPixelRatio`
(`word/Drawing/HtmlPage.js:1099-1100`) — and `CDocumentReadView.Set` applies a flat 5mm
margin on all four sides (`word/Editor/Layout/ReadView.js:79`). `SectPr.SetPageSize` clamps
only a **minimum** (`word/Editor/sections/sect-pr.js:1127-1138`). `GetScaleBySection`
(`ReadView.js:156-169`) only *shrinks* a section wider than the view, so for a
single-section document it evaluates to 1. There is no maximum column width, no
characters-per-line constant, no `max-width` and no `ch`-unit cap anywhere in `word/`.
**[SRC]**

**(b) They reach a reading measure by the other lever: type size.** The reader font ladder
is `[12, 14, 16, 18, 22, 28, 36, 48, 72]` points, default index 2 = 16pt
(`HtmlPage.js:104-105`), and `nScale = ReaderFontSizes[cur] / 16` (`HtmlPage.js:1102`) is
fed to `CDocumentReadView.GetFontScale()` (`ReadView.js:90-93`) and consumed at
`word/Editor/Paragraph.js:19515`. **[SRC]** Characters per line is column ÷ advance; they
hold the column and grow the advance. That is a real second answer to the same question,
and §5.1 says how it composes with ours rather than competing with it.

**(c) Their measure is coupled to the display's PIXEL DENSITY, not to its size.** Because
`W` is the paper width divided by `retinaPixelRatio` (`HtmlPage.js:1100`), the same document
on two phones of identical physical width gets a reader page of different widths if their
device pixel ratios differ. **[SRC]** This is worth recording because it is the kind of
mechanism a reader would mistake for a considered measure and it is not one. Do not copy it.

**(d) `151` §3.2 and ADR-046 say their reader mode is read-only. It is not, and the
evidence cited does not support it.** `CReaderTouchManager` does set `SelectEnabled = false`
and `TableTrackEnabled = false` (`word/Drawing/mobileTouchManager.js:837-838`) — the two
flags `151` §2.1 cites. But `SelectEnabled` has **exactly two consumers in the entire
tree**, both inside the touch manager base: `CheckSelectTrack`
(`common/Scrolls/mobileTouchManagerBase.js:928`, whose own comment reads *"onTouchStart =>
check if we hit selection anchors, to avoid starting scrolls/zooms"*) and `CheckSelect`
(`mobileTouchManagerBase.js:1873`). They gate **touch long-press selection handles and
table-resize handles**, not document mutation. `GetDocumentLayout()` — the object that
distinguishes Read from Print — is consulted in exactly **three** places in the whole word
editor (`ParaDrawing.js:1851`, `DocumentContentElementBase.js:1507`,
`Paragraph.js:19515`), none of which gates any insertion or removal command. Read-only in
ONLYOFFICE is a separate, orthogonal mechanism: `turnOnViewerMode()`
(`web-apps/apps/documenteditor/mobile/src/controller/Toolbar.jsx:296-304`) calls
`appOptions.changeViewerMode(true)` and
`api.asc_addRestriction(Asc.c_oAscRestrictionType.View)`, while `changeMobileView()` —
eight lines below it, at `Toolbar.jsx:306-311` — calls `api.ChangeReaderMode()` and touches
no restriction at all. **They are two different functions doing two different things.** The
mobile toolbar's edit buttons are gated on `props.isEdit && … && !isViewer &&
!props.isDrawMode` (`mobile/src/view/Toolbar.jsx:148`); the only thing `props.readerMode`
gates anywhere in that file is the **search** link (`Toolbar.jsx:159`). **[SRC — each line
above re-read directly, not taken from a report]**

Two further facts from the same audit:

- **Reader mode is mobile-only.** `ChangeReaderMode`/`ReaderMode`/`isReaderMode` have zero
  hits anywhere under `apps/documenteditor/main` (the desktop web client); every hit is
  under `apps/documenteditor/mobile`. The user-facing control is labelled "Mobile View",
  not "Reader Mode", and `changeMobileView()` calls `api.ChangeReaderMode()` directly
  (`mobile/src/controller/Toolbar.jsx:305-310`). **[SRC]** So ONLYOFFICE has **no desktop
  reading view at all** — which is precisely the surface the owner is complaining about
  here, and on it ONLYOFFICE is not a reference because they do not ship one.
- **It still paginates.** `Set()` still calls `SetPageSize(nW, nH)`, so content is laid out
  onto discrete pages of roughly the original size with 5mm margins and no header/footer
  reservation. `GetCalculateTimeLimit()` returns 100 — a shorter recalculation slice
  (`ReadView.js:152-155`). **[SRC]** `151` §2.1's "Refuse" bullet said this already and was
  right.

**Folding: ONLYOFFICE has none.** A case-insensitive search for `collapsed` across `sdkjs`
returns hits only in `cell/` (spreadsheet row/column outline grouping) and in the web-apps
version-history UI. **Zero hits in `word/`.** `CDocumentOutline`
(`word/Editor/DocumentOutline.js:42-488`) exposes
`SetUse`/`UpdateAll`/`GoTo`/`Demote`/`Promote`/`InsertHeader`/`SelectContent` and no
Collapse or Expand at all.

> **Two enumeration corrections, 2026-10-02 (`157` §6); the conclusion is unchanged and was
> re-verified.** (a) The sdkjs hits are **exclusively** under `cell/`; the `web-apps` hits
> are help text, locale strings, `border-collapse` CSS, ribbon/toolbar fold
> (`Mixtbar.js`), the macros dialog, and the panel `TreeView` — the *widget* is shared with
> `History.js` but the literal term is not in it. (b) `DocumentOutline.js` is **512** lines
> and its export block at `:501-512` lists **twelve** methods, not seven; still no Collapse
> or Expand. Also stronger than stated here: `grep -rinI "collaps" sdkjs/word` is **0 hits**
> for the substring, not just for the word, across all 232 `.js` files — and the absence of
> `w15:collapsed` from their *model* is measured against three closed lists (`CParaPr`'s
> constructor, its `Write_ToBinary` bitmap, and the `c_oSerProp_pPrType` interchange enum),
> which is why `157` §3.2 can close §7's open question 14. Their Navigation panel's Expand/Collapse menu items
(`main/app/controller/Navigation.js:314-339`) call `expandAll`/`collapseAll`/`expandToLevel`
on `apps/common/main/lib/component/TreeView.js` — **the same generic tree widget the version
history panel and the hyperlink dialog use.** It collapses the panel's list; no code path
hides a paragraph in the body. **[SRC]** So on folding the field is: **Word yes, Docs yes,
ONLYOFFICE no, us no.** We are behind the two references that matter and level with the one
we are replacing.

### 2.4 Modern readers and prose editors — the class the owner named

This is the weakest-sourced section in the document and it is marked as such. The
"600–700px / 65–75 characters" figure that circulates for Medium, Ghost and Substack did
**not** survive first-party verification in any case; every reachable source was a
third-party blog. **Do not publish a per-product number for Medium, Ghost, Substack or
Notion on this evidence.** What did verify:

- **Bear** ships a **Line width** preference (Settings → Typography) alongside font, size,
  line height and paragraph spacing. [1P — `bear.app/faq/typography-options/`] The commonly
  quoted 32em/40em values come from Bear's community forum, not the FAQ — UNVERIFIED.
- **iA Writer**'s own writing on responsive typography says *"optimal readability requires a
  certain amount of control over the measure (column width) of the text"* and gives **no
  number**. [1P — `ia.net/topics/responsive-typography-the-basics`] Useful as a statement of
  the principle by a company whose product is a prose editor; useless as a figure.
- **Notion**'s toggle headings and toggle lists are manual and per-toggle: *"you'll need to
  open and close toggles manually — there's no way to open and close them all at once, or
  make them open or closed by default."* [1P — `notion.com/help/columns-headings-and-dividers`]
  Whether the open/closed state is shared between collaborators is not addressed —
  UNVERIFIED. That collapsed toggles are dropped from a PDF export is [2P] only, and §5.3
  rejects that behaviour on its merits rather than copying it.
- **VS Code folding** — the owner's stated analogue. Fold `Ctrl+Shift+[`, Unfold
  `Ctrl+Shift+]`, Toggle Fold `Ctrl+K Ctrl+L`, plus Fold/Unfold Recursively, Fold All,
  Unfold All and Fold Level N. [1P — `code.visualstudio.com/docs/editor/codebasics`] Search
  **does** look inside folded regions and **auto-unfolds** the region holding a match
  [1P-adjacent — Microsoft's own issue tracker, `microsoft/vscode#59776`; not stated on any
  docs page]. Selection and copy across a folded region: **no source either way.
  UNVERIFIED** — and §5.3 therefore decides it from our own invariants rather than by
  imitation.

**What folding actually buys a reader, as opposed to a writer.** This is the owner's
question and it deserves an answer rather than a feature list. For a *writer*, folding is
structural editing: it puts two distant sections side by side so a move is one drag, and it
is why VS Code's is bound to Promote/Demote-adjacent commands. For a *reader*, folding does
something different and more valuable: it converts a linear document into a **navigable
one**, so that "where am I and what else is there" is answerable without scrolling, and it
is the only affordance that makes a 300-page document's shape visible on a phone. That is
also why it belongs to the outline and not to the layout: the thing it operates on is the
*heading tree*, which exists identically on paper and in reflow.

### 2.5 The standards, which supply the only hard number

**WCAG 2.1 Success Criterion 1.4.8 Visual Presentation, Level AAA.** [1P — quoted verbatim
from `w3.org/WAI/WCAG21/Understanding/visual-presentation.html`, fetched]

> For the visual presentation of blocks of text, a mechanism is available to achieve the
> following:
> 1. Foreground and background colors can be selected by the user.
> 2. **Width is no more than 80 characters or glyphs (40 if CJK).**
> 3. Text is not justified (aligned to both the left and the right margins).
> 4. Line spacing (leading) is at least space-and-a-half within paragraphs, and paragraph
>    spacing is at least 1.5 times larger than the line spacing.
> 5. Text can be resized without assistive technology up to 200 percent in a way that does
>    not require the user to scroll horizontally to read a line of text on a full-screen
>    window.

Four things follow, and they carry the whole of §5.1:

1. **80 characters — 40 for CJK — is a first-party, quotable, normative number.** It is the
   only one in this document. Everything else about measure is either a vendor's product
   decision or folklore.
2. It asks for **a mechanism**, not a default. A capped default satisfies it; so does an
   uncapped default with a width control. A design with **neither** — which is what we have
   — satisfies it in no configuration, at any window width above about 470px.
3. Item 3 is a separate finding we are also failing: reflow honours the document's `w:jc`,
   so a justified document reflows justified with no mechanism to stop. Out of scope here;
   recorded in §8.
4. Item 5 is the *reflow* requirement and we now pass it — that is what ADR-046 retired
   (`151` §7). Note that item 5 is why the two halves are easy to confuse: satisfying 5 is
   what `151` built, and it says nothing at all about 2.

**WCAG 2.1 SC 1.4.10 Reflow (Level AA)** is the criterion `151` was actually serving:
content presentable at 320 CSS px without two-dimensional scrolling. Reflow as built passes
it. **Passing 1.4.10 is not evidence about 1.4.8**, and `151` never distinguished them.

**Bringhurst's 45–75 characters, 66 as the single-column ideal** is the figure everyone
quotes, and it is **not verified against his text** — no reachable source quotes the
sentence with a page citation. **[2P / folklore-strength]** Likewise **Dyson & Haselgrove**
(*IJHCS* vol. 54, pp. 585–612, on line length and reading from screen) is a real paper, and
the ~55-characters-optimal figure attributed to it comes from secondary summaries only —
UNVERIFIED. Both are cited here as corroboration of WCAG's direction, never as the basis for
a number.

## 3. Our own measurement — what the shipped code actually produces

### 3.1 There is a minimum and no maximum

`webapp/src/reflow_view.mjs`'s `reflowMeasure`:

```text
totalPx          = floor(clientWidthPx / 16) * 16
gutterTwip       = round(16 / cssPerTwip)
contentWidthTwip = floor(totalPx / cssPerTwip) - 2 * gutterTwip
```

with a **floor** — `REFLOW_MIN_CONTENT_TWIP = TWIPS_PER_INCH`, carrying a nine-line comment
explaining itself — and **no ceiling of any kind**. The reading column is the window minus
two 16px gutters, at every width. `REFLOW_QUANTUM_PX`'s own doc comment shows the shape of
the omission: every word of it is about not *exceeding* the viewport. Nothing asks whether
the viewport is a sensible measure.

### 3.2 The numbers, and how they were derived

Characters per line is the unit every source in §2 uses, so the column has to be converted
into it. The conversion is `column ÷ mean advance`, and the mean advance was **measured
from the bundled faces' own `hmtx` tables** over a fixed 674-character English prose sample
(the openings of *Pride and Prejudice* and *On the Origin of Species*, public domain), so
that spaces and punctuation are weighted as they actually occur rather than an alphabet
being measured:

| Face (bundled) | Metric-compatible with | Mean advance |
| --- | --- | --- |
| `Carlito-Regular` | Calibri — **Word's default** | **0.3991 em** |
| `LiberationSerif-Regular` | Times New Roman | 0.3930 em |
| `Caladea-Regular` | Cambria | 0.4156 em |
| `LiberationSans-Regular` | Arial | 0.4310 em |

Reproduce with `python3` and `fontTools` against
`crates/casual-doc-layout/fonts/`; the method is in §9. **The spread across four base text
faces is 0.393–0.431 em**, i.e. under 10%, which is why a single em-based approximation is
usable as a fallback (§5.1). 11pt = 14.667 CSS px, so Carlito at 11pt is **5.853 px per
character**.

At 100% zoom, with `cssPerTwip = 96/1440`:

| Viewport | Column (twip) | Column (CSS px) | Characters, 11pt Carlito | Verdict |
| --- | --- | --- | --- | --- |
| 390 (phone rung) | 5,280 | 352 | **60** | inside 45–75. **The phone is correct.** |
| 768 | 11,040 | 736 | 126 | 1.6× the WCAG 80 |
| 1024 | 14,880 | 992 | 169 | 2.1× |
| 1280 | 18,720 | 1,248 | **213** | 2.7× |
| 1440 | 21,120 | 1,408 | **241** | **3.0×** |
| 1920 | 28,320 | 1,888 | 323 | 4.0× |
| 2160 | 31,920 | — | — | **refused**, see §3.3 |

For scale, the document's own paper is not innocent either: a Letter page's 6.5in text
column is **624 px — 107 characters at 11pt Calibri**, already above WCAG's 80. That is a
real constraint on §5.1's answer and it is why the cap cannot simply be "the paper".

**The phone rung is the one place the design was evaluated, and it is the one place the
missing cap does not bite.** 60 characters at 390px is a good measure by every source in
§2. `151` §7's before-and-after table measures 390×844 and nothing else; `reflow.spec.mjs`'s
claims are at 390px; `reflow_view.test.mjs` asserts a 390→1280px drag produces *one pass*,
which is a cost assertion and not a measure assertion. So every guard in the repository
passes, and the defect is invisible to all of them — an instance of `SKILL.md` §4's
"a test that passes on arrival tells you less than you think", and of §8's
"'completes in a harness' is not 'usable in a tab'".

### 3.3 The missing cap is also an outright failure above a reachable width

`LayoutView::reflow` refuses `content_width > MAX_REFLOW_COLUMN`
(`crates/casual-doc-layout/src/document_layout.rs:273`, `MAX_REFLOW_COLUMN = Twip(31_680)`
= 22in). Its doc comment explains the intent: *"wider than any paper this engine supports.
A caller asking for more has converted units wrongly."* It was written as a
unit-conversion sanity check. **With no cap upstream it is reachable by a correct caller.**

`reflow_chrome.mjs`'s `sync` catches the throw, sets `chosen = "0"`, and shows the engine's
message as an error. So the reader turns Reflow on and it turns itself back off with a
message about twips. First refused viewport width, by zoom (`ZOOM_STEPS` includes 0.5, and
`FIT_ON_OPEN_FLOOR` is 0.5, so 50% is two clicks away):

| Zoom | First viewport width refused |
| --- | --- |
| 50% | **1,104 px** |
| 75% | 1,632 px |
| 100% | **2,160 px** |
| 150% | 3,216 px |
| ≥200% | not reachable below 4,000 px |

A 1440px laptop at 50% zoom, and a maximised window on a 2560px monitor at 100%, are both
inside this. **A cap removes the failure as a side effect**, because a capped column can
never approach 22in — which is an argument for the cap that does not depend on taste at all.

### 3.4 Folding: nothing, anywhere

> **Superseded in part, 2026-10-02.** The model/import/export half of this section is
> **fixed**: `ParagraphProperties::collapsed` exists, `apply_paragraph_property` has a
> `b"collapsed"` arm, the writer emits `w15:collapsed`, and
> `fixtures/generated/collapsed-headings.docx` is the fixture this section asked for. The
> second bullet's "unknown" is now **measured**, and the answer was the better of the two:
> the drop was already **reported** (`OmittedNotRetained`, 3 occurrences) through `body.rs`'s
> generic `w:pPr` long-tail arm, not silent — so the coverage gate had stayed green rather
> than being blind. A reported loss is still a loss, which is why it is now modelled. The
> third bullet (the shell) and the first clause of this section's title still stand: nothing
> hides a folded subtree. `157` §4.3 has the record.

- **Not in the layout or the model.** `collapsed` has no hits as a parsed property in
  `crates/`; `crates/casual-doc-import` never reads a `collapsed` element. Word persists a
  collapsed heading as **`w15:collapsed`** (`CT_OnOff`, namespace
  `http://schemas.microsoft.com/office/word/2012/wordml`) in the heading paragraph's
  `w:pPr` — *"When a collapsed element is added to a paragraph (pPr) of a particular heading
  level and its value is true/on/1, immediately subsequent paragraphs with a higher heading
  level number appear collapsed when the document is opened."* [1P — Microsoft Open
  Specifications, MS-DOCX] So the state has a standard home in the format and we drop it.
  Note the namespace: it is `w15:`, not the bare `w:collapsed` this project's own notes
  guessed.
- **Whether that drop is reported is unknown, and the gate cannot tell us.** The
  loss-coverage gate (`crates/casual-doc-export/tests/source_element_coverage.rs`) refuses
  an element name that vanishes without a report entry — but **no `.docx` in the repository
  carries a `collapsed` element** (38 packages scanned, every XML part, zero hits), so the
  gate has never had the opportunity to fire on it. This is `SKILL.md` §9.3's
  "absence from a support matrix is an overstatement by omission" in its purest form.
- **Not in the shell.** `webapp/src/outline_panel.mjs` renders a **flat list of buttons**
  with `lvl-1`…`lvl-6` classes, `aria-current="location"` on the active row, and no
  disclosure control, no `aria-expanded`, no `role="tree"`. There is no fold state
  anywhere in `webapp/src`.

## 4. What the field does, in one table

| | Google Docs | Word | ONLYOFFICE | opendoc today |
| --- | --- | --- | --- | --- |
| Pageless/continuous view | Pageless [1P] | Web Layout (desktop); Word for the web continuous [1P/2P] | reader mode, **mobile only** [SRC] | Reflow |
| Is it editable? | yes | yes (Web Layout, Immersive Reader [1P]) | **yes** — not read-only [SRC], §2.3(d) | yes |
| Is the measure capped? | **yes** — Text width, 3 steps, per-viewer [1P] | **yes** — Immersive Reader Column Width, 4 steps [1P]; Read Mode columns [1P]. **No** in Web Layout [2P] | **no** — paper width ÷ DPR; they scale TYPE instead [SRC] | **no** |
| Reader type-size control | not found | yes (Immersive Reader, Read Mode) [1P] | yes — 9-step ladder [SRC] | no (`151` §8.2 deferred) |
| Pageless is a document property? | **yes** — `DocumentMode` [1P] | n/a (a view) | n/a (a view) | no — per-viewer |
| Reading width is a document property? | **no** — explicitly per-viewer [1P] | no | n/a | n/a — no control |
| In-body collapsible headings | **yes** (Pageless) [1P] | **yes**, persisted as `w15:collapsed` [1P] | **no** [SRC] | **no** |
| Outline-panel tree collapse | yes [2P] | yes (Navigation Pane) | yes, generic `TreeView` [SRC] | **no** — flat list |
| Print/export a collapsed doc | UNVERIFIED | collapsed content reportedly does **not** print [2P] | n/a | n/a |

The two columns that agree are Google and Word, they agree on both rows the owner raised,
and we differ from both on both.

## 5. The recommendation

### 5.1 The column must be capped, at 80 characters of the document's default face, resolved by measurement

**The number.** The cap is **a target in characters**, because that is the unit every
source in §2 is stated in and the only unit in which the recommendation transfers across
faces and sizes. The default target is **80 characters — 40 for CJK — from WCAG 2.1 SC
1.4.8, the single normative, first-party, quotable figure available** (§2.5). It is chosen
over Bringhurst's 66 for one reason and it is an evidence reason, not a typographic one:
80 is citable and 66 is not (§7). 80 is also the conservative end of the two, so a design
that defaults to it errs towards the paper the reader is used to.

**The unit conversion, and why it is measurement rather than a table.** Characters are
resolved to twips by the *measured mean advance of the document's default face at its
default size*, which is `docDefaults`/`Normal` — a value the engine already has a shaper
for. `cap_twip = round(target_chars × mean_advance_twip(default_face, default_size))`. At
Word's default (11pt Calibri → Carlito) that is 80 × 5.853px = **468 CSS px = 4.88in =
7,024 twips**.

> **CORRECTED (implementation, 2026-10-02).** This paragraph read **7,020 twips**, and
> that figure was 468 CSS px converted *back* to twips — the pixel rounding counted
> twice. The twip arithmetic the shipped code performs is
> `round(80 × 0.3991 em × 11 pt × 20 twips/pt)` = **7,024**, which is 468.27 CSS px and so
> still **468 px to the pixel**. 468 is unchanged and correct; 7,020 was not. Pinned at
> 7,024 by `webapp/tests/reflow_view.test.mjs` ("the published cap is 80 characters, and
> in twips it is the published number"), which is how the double rounding was found.
> ADR-048 is corrected in place with it.

A fallback for a face whose metrics are unavailable is **0.40 em per
character**, i.e. `cap ≈ 32 em`, justified by the measured 0.393–0.431 em spread across the
four bundled base faces (§3.2) — stated as an approximation with its error, not as a
constant with no source.

**Which clamp, and it is one clamp.** `contentWidthTwip = min(available, policy_cap)` where
`available` is exactly what `reflowMeasure` computes today. **The engine needs no change at
all**: `LayoutView::Reflow { content_width }` already takes the measure as a parameter, and
the cap is a decision about which number the caller passes. That is the cheapest possible
shape and it is cheap because ADR-046 got the seam right. It also closes §3.3's
`ColumnTooWide` refusal, since a capped column cannot reach 22in.

**What fills the space between the cap and the window: centre the column on the app's
desk.** Not a wider tile, not a paper edge, not a second column. `#viewport.is-reflow`
already removes the sheet shadow and the corner radius, so the tile becomes a text column
on the application background, which is what Docs, Immersive Reader and every reader in
§2.4 do. Two things need checking when this is built and neither is a new mechanism: the
review gutter is driven by `--page-width`, which is the tile's width and therefore still
correct; and `page_scroll.mjs`'s band centres a page already.

**Do NOT reach the measure by scaling the type, as ONLYOFFICE do** (§2.3(b)) — at least not
as the primary lever. `151` §8 item 2's reasoning still holds and is now better supported:
a change that alters two variables at once cannot be evaluated. But it should be recorded
that a reader **type-size** control is the *same* control in a different unit, that both
Word and ONLYOFFICE ship one, and that once a character target exists the two compose
trivially — the target is in characters, so changing the size changes the width and the
measure stays put. That is the argument for doing the cap **first**: it makes the font-size
control a one-line consequence instead of a second policy.

**Expose the cap as a control, three or four steps, per-viewer.** WCAG 1.4.8 asks for a
mechanism; Docs offers three steps; Immersive Reader offers four. Recommended: **four**,
labelled by what they do rather than by a number, with the character targets stated in the
code — roughly 55 / 70 / 80 / unlimited. "Unlimited" is a named option on purpose, for two
reasons: a host embedding the editor in a 400px column has already got the narrow case for
free and may want the wide one, and a named Full makes the default's narrowness
discoverable rather than mysterious. Per-viewer, stored where `docReflow` is, because that
is exactly what Google does with Text width and states so [1P, §2.1].

### 5.2 Two width policies over one layout mechanism — not two modes, and not one

`151` shipped a **binary** `view.reflow` toggle. That conflates two questions that have
different answers on a desktop and only coincide on a phone:

- *Is the document laid out on paper?* — a **layout** question. The answer changes
  pagination, headers, footers, page numbers, the ruler and the Pages panel.
- *How wide is the text?* — a **measure** question. It changes nothing structural.

On a phone the two collapse, because the window is narrower than any cap, which is why the
built design could not see the distinction: at 390px `min(available, cap)` **is**
`available` and the phone's 60 characters are already right. On a 1440px window they come
apart completely.

**Recommendation: one layout mechanism, two width policies, and a preset that sets three
independent things.**

1. **`LayoutView::Reflow` stays exactly as built.** One mechanism (`SKILL.md` §8), one
   seam, no second paginator, no reading-specific layout path. Everything below is a choice
   about the number passed to it.
2. **Two width policies**, both of the form `min(available, X)`:
   - **Fit** — `X = the document's own text measure` (the first section's content width;
     624px at Letter default). This is the *pageless authoring* policy. It never makes a
     line **longer** than the paper the author is writing for, which is the property an
     author needs and a reader does not care about; on a phone it reduces to `available`,
     so nothing about the retired exemption changes. It invents no constant — the number
     comes from the document.
   - **Reading** — `X = the character target of §5.1`. The default on a desktop.
3. **"Reading view" is a preset, not a third mode.** What the owner is describing —
   comfortable measure, reduced chrome, folded outline — is *Pageless + Reading width +
   reduced chrome + folding*, four settings that are each independently useful and each
   independently reachable. Ship it as one command that sets them, exactly as Word ships
   Focus (chrome) and Immersive Reader (measure) as separate things that compose. A preset
   is not a parallel path; it is one row in the command registry that writes four
   preferences.
4. **Keep it editable. `151` §3.2's conclusion was right and its stated reason was wrong.**
   It justified editability as a divergence from a read-only field. There is no read-only
   field: ONLYOFFICE's reader mode is not read-only (§2.3(d)), and Word's Immersive Reader
   is explicitly not [1P]. The correct justification is the simple one — a browser is our
   entire mobile story (`18-SUPPORT-MATRIX.md`), and a reading mode you must leave to type
   is not an answer to `151` §1. Nothing about the decision changes; the evidence for it
   changes from false to sound, which matters because the false version was load-bearing in
   an ADR.
5. **Do not make any of it a document property.** Google makes Pageless one
   (`DocumentMode.PAGELESS`) and explicitly does **not** make Text width one. Keep both
   per-viewer, and note the decisive reason `151` §3.4 did not give: **neither DOCX nor ODT
   has anywhere to put it.** A document property we cannot serialise is a sidecar, and a
   sidecar that changes how every collaborator sees the file is a worse trade than a
   per-viewer default. ADR-044's reasoning (one person's phone must not reformat another
   person's monitor) stands and is now the second reason rather than the only one.

**What this costs.** One clamp in `reflowMeasure`, one preference, one control with four
steps, one preset command, and the ruler/Pages-panel withholding already built. No engine
change. The largest piece of work in it is the control's two surfaces and its guard — which
is `SKILL.md` §10's floor, not this feature's overhead.

### 5.3 Folding belongs to the outline, not to the layout view

**It is not part of reflow, and building it there would be the same class of mistake as the
binary toggle.** The evidence:

- Word's folding works in **Print Layout** — its paginated view — and persists as a
  paragraph property [1P]. So folding and pagelessness are demonstrably independent.
- Docs' folding is Pageless-only [2P], and that restriction buys a Docs reader nothing; it
  is an artefact of where Google built it.
- The thing folding operates on is the **heading tree**, which is identical on paper and in
  reflow. `documentOutline()` already produces it, in document order, at `O(rows)`.

**The mechanism, named before it is invented** (`SKILL.md` §8). Folding is a **per-viewer
block visibility filter** — a set of collapsed heading `NodeId`s, consulted by the flow pass
so the blocks in a collapsed subtree contribute no fragments. Three precedents in this
repository, all of which it should reuse rather than parallel:

1. **The filter itself already exists one tier down.** `flow.rs`'s `push_styled_runs`
   returns early when the cascade-resolved `w:vanish` is on
   (`crates/casual-doc-layout/src/flow.rs:6500` — **corrected 2026-10-02:** the function is
   at `:6527` and the early return at `:6539`), so a run in the model contributes nothing
   to the layout. Folding is that filter at the **block** tier. Not a second flow path.
2. **The per-viewer view parameter already exists, twice.** `ReviewView`
   (`flow.rs:102` — **corrected:** `pub enum ReviewView` is at `:106`) and `LayoutView`
   (`document_layout.rs:162`) are both threaded to the one
   place that consumes them, both default to today's behaviour, and both leave every
   existing caller byte-for-byte unchanged. A `FoldSet` is the third, and it is a third
   parameter on one mechanism, not a third mechanism.
3. **The byte-space projection already exists.** A fold changes the layout's byte space in
   exactly the way a hidden deletion does, and that problem is solved:
   `casual-doc-wasm/src/lib.rs:9553` describes the projection "as layout/editing, including
   collapsed deleted ranges", and `lib.rs:32635` is its guard. Folded content must project
   to a **single boundary position**, so a caret can never land inside what the eye cannot
   see. Note the warning attached to `ReviewView::Markup` (`flow.rs:107-110`): it is "never
   fed to caret/selection/hit-test (its byte space differs)". A fold must **not** take that
   escape, because the document has to stay editable while folded — which is the same
   promise ADR-046 made and the reason it chose tiles.

**Where the state lives — Google's two-tier split, and Word's serialisation.**

| Tier | What | Where |
| --- | --- | --- |
| Document default | Word's `w15:collapsed` on a heading's `w:pPr`; Google's editor-set default | the model, parsed, round-tripped, exported |
| Live per-viewer state | what this reader has folded right now | beside `docReflow` in `prefs.mjs`; **not** persisted into the file by a viewer's toggling |

That is Google's model verbatim [1P, §2.1] and it is also the only model that can honour a
Word file. **Both halves of that table are now decided and the first is built** — see the
note at the head of §3.4 and ADR-049's Accepted record. Parsing `w15:collapsed` is
**independently required** of any folding UI: today it
is dropped, and §3.4 shows the loss-coverage gate has never had a fixture that could catch
it. **Adding a fixture that carries `w15:collapsed` is worth doing before any of this**,
because it converts an unknown into either a report or a red gate.

**The behaviours, decided from our own invariants where no source answers.**

- **Find searches folded text and reveals by unfolding.** `findText`
  (`casual-doc-wasm/src/lib.rs:3503`) walks the **model**, not the layout, so it already
  searches folded content by construction; the work is that revealing a match must unfold
  its ancestor headings. Same as VS Code [1P-adjacent, §2.4].
- **A selection that crosses a folded heading takes the whole subtree.** Copy copies it,
  delete deletes it. The alternative — a delete that silently spares content the user
  cannot see, or takes it without saying so — is the ambiguous case, and the resolution is
  the repository's own rule: no silent loss, and say what you did. So a fold-crossing
  delete should be **one undoable operation that states its scope** ("Deleted section and
  N paragraphs"), not a quiet cascade. Word reportedly cascades [2P]; we would too, out
  loud.
- **Print, PDF and DOCX export are always fully expanded.** Word reportedly prints only
  expanded content [2P] and Notion reportedly drops collapsed toggles from a PDF [2P].
  **Both are rejected.** A printed or exported artefact that silently omits content is the
  silent-data-loss class `SKILL.md` §12 forbids outright, and it is worse than the
  page-number lie `151` §6.5 refused to print. The seam already exists: `print.mjs`'s
  `withPagedLayout` forces `Paged` with the restore in a `finally`, and forcing "unfolded"
  belongs in the same wrapper for the same reason — a rule enforced next to the thing it is
  a rule about cannot be forgotten by the next caller. DOCX export writes `w15:collapsed`
  so the *state* survives while the *content* always does.
- **The accessibility mirror must be filtered by the same set.** `webapp/src/a11y_mirror.mjs`
  is model-derived, so an unfiltered mirror would read a screen-reader user the content a
  sighted reader has folded away — the fold would be a lie to one class of reader. It is
  filtered, the disclosure carries `aria-expanded`, and the collapsed heading announces how
  much is hidden.
- **Two surfaces, one state.** `outline_panel.mjs`'s flat list becomes a real
  `role="tree"` of `treeitem`s with `aria-expanded`, and **the panel's disclosure and the
  in-body chevron drive the same `FoldSet`**. That is `SKILL.md` §10's ≥2-surfaces floor and
  it also collapses the distinction §2.1 and §2.3 both had to draw between "panel tree
  collapse" and "body folding" — everyone else ships those as two unrelated features with
  two states, and having one state is a genuine improvement rather than parity.

**One thing folding must not do: become a second way to hide content from a collaborator.**
`w15:collapsed` is a *default*, not an access control. Whatever the fold state, the content
is in the file, in the export, in find and in the a11y mirror.

### 5.4 What of `151` stays

Most of it. To be explicit, because a document with five corrections in it invites
over-reading:

- **§2.1's mechanism description, §4 in its entirety, §4.4a's trim, §4.6's guards, §5's
  complexity analysis, §6.2's quantisation and debounce, §6.3's wide-table arbitration,
  §6.4's withheld chrome, §6.5's refusal to print a tile index as a page number, and §7's
  measurement** are all correct and all survive. The seam ADR-046 chose is the reason §5.1
  costs one clamp.
- **§6.2's two numbers are right for the question they answer** and that question is
  narrower than the section implies. 16px floored, 150ms trailing: still right, still
  needed, and once the cap exists they only matter *below* the cap, where `available` is
  the binding constraint. Above the cap the width does not move with the window at all,
  which makes most desktop resizes free for a second, better reason.
- **§3.2's decision** stands; only its justification is replaced (§5.2 item 4).
- **§3.1 and §3.3** — reflow is a view, not an edit; pagination is advisory — are untouched.

## 6. Exactly which parts of `151` and ADR-046 were wrong

Each is corrected in place in the source document, marked **CORRECTED (154)**, and listed
here so the list is in one place.

| Where | What it says | What is wrong | Fix |
| --- | --- | --- | --- |
| `151` §1 | *"There are exactly three answers and only three"* — shrink, pan, reflow | **The trichotomy is false**, and it is the root error: the fourth answer is the one every reader in §2 uses — **cap the measure and centre it**. Stating the space of answers as closed is what made a design with no maximum look complete | §1 gains the fourth answer and the cross-reference |
| `151` §2.2 | Google's Pageless, "two properties matter" | **Materially incomplete.** Omits **Text width** — Google's own cap — entirely, and so omits that Google splits format (document) from width (per-viewer, stated in as many words). The half it records is the half we diverge from; the half it omits is the half we should have copied | §2.2 gains Text width, the API `DocumentMode` evidence, and the split |
| `151` §2.3 | *"`148` §4 found no reflow toggle in Microsoft's own documentation… So the vote is Docs and ONLYOFFICE for, Word abstaining"* | **Wrong three ways.** (a) Word ships Web Layout, Read Mode, Focus and Immersive Reader, and Immersive Reader has a four-step **Column Width** control whose documented purpose is line length — Word is the strongest vote *for* capping, not an abstention; (b) Word for the web's original rendering was continuous [1P]; (c) the analogue of what we built is **Web Layout**, and we inherited its uncapped window-width behaviour | §2.3 replaced with §2.2 of this document |
| `151` §3.2, §6.1, and ADR-046's "Where we differ" | *"their reader mode sets `SelectEnabled = false`… i.e. their reader mode is not editable"*, and "Reader mode would promise ONLYOFFICE's read-only behaviour" | **The premise is false in source.** Those two flags are consumed only in the touch manager and gate touch selection handles, not mutation; read-only in ONLYOFFICE is `asc_addRestriction(...View)`, which `ChangeReaderMode()` never calls (§2.3(d)). The **decision** was right; the evidence was not, and it was load-bearing in an ADR | §3.2 and ADR-046 corrected; the justification becomes the mobile-support one |
| `151` §4.2, §6.2; `reflow_view.mjs` | the width is the available space, with a floor and no ceiling | **The defect.** §3.1–§3.3: 241 characters at 1440px, and an outright refusal at 2160px / 100% or 1104px / 50% | §5.1; a ceiling beside `REFLOW_MIN_CONTENT_TWIP` |
| `151` §6.1 | a binary `view.reflow` toggle, named for the mechanism | **The control shape is wrong** once a width policy exists — it conflates a layout question with a measure question, which only coincide on a phone (§5.2) | §5.2: layout toggle + width control + a preset |
| `151` §2.1 "Refuse" bullet | their `H` choice is what makes their breaks arbitrary | Right, and **incomplete**: their `W` is the paper width **divided by the device pixel ratio** (§2.3(c)), so their measure tracks pixel density rather than screen size. Worth recording so nobody copies it | §2.3(c) |
| `151` §8 | four open questions plus four from the shell half | **Three more**, none of which it knew: the cap, the `ColumnTooWide` refusal, and folding. Also: WCAG 1.4.8 item 3 (no mechanism to unjustify) | §8 here |
| `151` §8 item 2 | proposes **not** to scale reader fonts, so as not to change two things at once | **Right, and its status improves.** Once the cap is expressed in *characters*, a type-size control changes the width and leaves the measure alone, so it stops being a second policy. Both references ship one | §5.1's last paragraph |

**ADR-046's decision is not reopened.** Its mechanism — a `LayoutView` parameter threaded
to the one place geometry is decided, trimmed tiles, a refusal for `PAGE`, per-viewer,
never an edit — is exactly right, and §5.1 costs one clamp *because* of it. What ADR-048
amends is the *caller* rule ADR-046 left implicit (which width to pass) and one sentence of
its evidence.

## 7. What I could not verify — read this before quoting anything above

Marked here rather than hedged in place, so that a reader looking for the unverified claims
finds all of them together. **No part of §5 depends on any row in this list.**

**Could not confirm against a first-party source:**

1. **Bringhurst's 45–75 characters, 66 ideal.** The numbers are near-universally repeated;
   no reachable source quotes his sentence with a page or edition citation. Cited as
   corroboration only. WCAG 1.4.8's 80 is the number §5.1 rests on.
2. **Dyson & Haselgrove's ~55-characters-optimal.** The paper is real (*IJHCS* 54:585–612);
   the figure comes from secondary summaries. The article itself was not read.
3. **Word's Web Layout behaviour** — page boundaries, window-width wrapping, hidden
   headers/footers, editability. Converging secondary sources and Microsoft Q&A threads;
   every direct fetch of `support.microsoft.com` returned HTTP 403.
4. **Word Read Mode's exact Column Width and Page Color option labels** (Narrow/Wide;
   White/Sepia/Inverse). The *behaviour* is 1P-quoted; the option enumerations are 2P.
5. **Word Focus mode's exact scope.** 2P only.
6. **Word for the web's "original continuous page view".** Microsoft's own blog, reached in
   summary; the phrase was not re-verified against the full article body.
7. **Whether Word prints collapsed content.** 2P only, three converging sources; Microsoft's
   own page 403'd. §5.3 rejects the behaviour on its merits regardless.
8. **Whether Google Docs folding is Pageless-only.** Strongly and consistently reported;
   not pinned to a first-party sentence.
9. **Google's Text width default step**, and whether a fourth "Full" step now exists. The
   first-party support page lists three (Narrow/Medium/Wide); several 2024–25 secondary
   sources claim four. **Unresolved conflict — do not assert either.** §5.1's four steps are
   recommended on our own reasoning.
10. **Whether Google repaginates a pageless document for print/PDF**, and whether Docs
    prints collapsed content, and what Docs' Find does with a match inside a collapsed
    section, and what a selection crossing one does. **No source, first- or second-party,
    either way.** §5.3 decides each from this repository's own invariants and says so.
11. **Notion**: whether toggle state is shared between collaborators; whether a collapsed
    toggle's content is dropped from a PDF export (2P only).
12. **VS Code**: what selection and copy do across a folded region. No documentation either
    way.
13. **Per-product measures for Medium, Ghost, Substack, iA Writer, Notion.** The circulating
    600–700px figures did not survive first-party checking in a single case. **Nothing in
    this document cites one.** iA's own page states the principle with no number; Bear's own
    page confirms a control with no value.

**Could not confirm from the ONLYOFFICE source available:**

14. ~~Whether their OOXML converter parses or round-trips `w:collapsed`/`w15:collapsed`.~~
    **Closed for the editor path, 2026-10-02 (`157` §3.2).** The converter itself
    (`x2t`/`DesktopEditor`) is still **not present** in either tree, so its internals remain
    unverified. But the x2t↔sdkjs contract *is* in the tree: `c_oSerProp_pPrType`
    (`word/Editor/Serialize2.js:243`) is a closed enum of values 0–49 whose only outline code
    is `outlineLvl: 34`, an unallocated record reaches
    `Serialize2.js:9411 default: res = c_oSerConstants.ReadUnknown;` and is discarded, and
    the writer can only emit allocated codes. **So `w15:collapsed` cannot survive an edit
    round-trip through their editor whatever x2t does, and the mechanism of the loss is
    literally `ReadUnknown`.** The absence from the model is measured against three closed
    lists rather than by grep. **[SRC]**
15. Whether a hardware keyboard can insert text while `CDocumentReadView` is active on
    mobile. No code-level block was found and no live caret path was traced. §2.3(d)'s
    conclusion — that reader mode is not *wired* to a read-only gate — does not depend on
    this.
16. Whether an inline body `PAGE` field (outside a header) is recomputed against reader-mode
    pagination.
17. Whether `appOptions.readerMode` (`mobile/src/store/appOptions.js:144-147`) is ever
    toggled from a UI action; no call site was found. Possibly dead code.

**Unverified about our own tree:**

18. ~~**Whether a `w15:collapsed` in a real Word file is reported or silently dropped.**~~
    **ANSWERED 2026-10-02 by adding the fixture, exactly as this item proposed:
    `fixtures/generated/collapsed-headings.docx`. It was REPORTED, not silent** — one finding
    keyed `collapsed`, disposition `OmittedNotRetained`, `occurrences = 3`, produced by
    `body.rs`'s generic `w:pPr` long-tail arm, so the coverage gate had been correct rather
    than blind. A reported loss is still a loss, so it is now modelled, written and
    round-tripped, and the gate is armed for the element family — proven by driving
    `source_element_coverage` red with a writer that drops it. `157` §4.3.
19. **The characters-per-line figures in §3.2 are arithmetic on a measured mean advance, not
    a shaped line count.** The advance is measured (§9); the division assumes a mean and
    ignores kerning, ligatures, justification and per-line variance. Treat them as accurate
    to roughly ±5%, which is far inside the 3× discrepancy they are being used to
    establish. **When §5.1 is built, the guard should count characters on a shaped line
    rather than divide** — that is the form that cannot drift.
    **STILL OPEN after implementation (2026-10-02), and why.** The shell has no per-line
    text API: glyphs are rastered to canvas by the engine, so there is no line box to count
    characters in. What shipped instead is two guards at two tiers — the character→twip
    conversion pinned as a pure function (`reflow_view.test.mjs`: 80 characters of 11pt
    Calibri is 7,024 twips / 468 CSS px) and the **painted geometry** pinned in the browser
    (`reflow.spec.mjs`: the column is 500px of a 1,440px window and centred). Neither counts
    a shaped character, so the ±5% stands. A shaped count would need either a line-box
    getter on the seam or a hit-test sweep across one line; both are real work and neither
    is this lane's.
20. **The 2160px / 1104px refusal thresholds (§3.3) were computed from the shipped formula,
    not observed in a browser.** The formula is `reflowMeasure` reproduced exactly and the
    bound is `MAX_REFLOW_COLUMN`; the arithmetic is in §9. It has not been reproduced by
    resizing a real window, and it should be before the row is cited as a defect report.
    **RESOLVED 2026-10-02 — observed in Chromium.** With the shell's ceiling mutated away
    and Full chosen, a 1,440px window stepped to 50% zoom loses `is-reflow` entirely: the
    engine throws, `sync` reverts the preference to paper, and the guard that expects reflow
    to still be on times out after 30s. With the ceiling in place the same window lays out.
    So the defect was real, it is reachable from the UI by two zoom clicks, and it is now
    closed. The *boundary* figures are still arithmetic, but they are arithmetic a committed
    guard performs: `reflow_view.test.mjs` asserts that 2,160px/100%, 1,104px/50%,
    1,632px/75% and 3,216px/150% each exceed the bound with the ceiling lifted **and that
    one quantum lower does not**, so each is pinned as the FIRST refused width rather than
    merely as a refused one.

## 8. Open questions and work items this analysis opens

1. ~~**The cap** (§5.1).~~ **DONE 2026-10-02.** `REFLOW_MAX_CONTENT_TWIP` and
   `reflowCapTwip` in `webapp/src/reflow_view.mjs`; the clamp is
   `min(available, capTwip, REFLOW_MAX_CONTENT_TWIP)` in `reflowMeasure`. **No `crates/`
   change was required — zero lines.** The engine was not asked to expose a mean advance:
   `stylePreview("Normal")` already yields the default face and size (O(styles)), and the
   advance comes from the measured table in `reflow_view.mjs` with the 0.40 em fallback.
2. ~~**The `ColumnTooWide` refusal** (§3.3).~~ **CLOSED 2026-10-02**, and **reproduced in a
   browser first**, which is what §7 item 20 asked for: with the shell's ceiling lifted and
   Full chosen at 50% zoom on a 1,440px window, `#viewport` loses `is-reflow` because `sync`
   catches the throw and reverts the viewer to paper. It is closed by mirroring the engine's
   bound in the shell and applying it to **every** step including Full — the cap alone would
   have left it reachable through the new control. `MAX_REFLOW_COLUMN`'s doc comment still
   describes a correct caller as having converted units wrongly: **open**, deliberately, as a
   comment-only `crates/` edit of no behavioural value (ADR-048 consequences).
3. ~~**The width control** (§5.1).~~ **DONE 2026-10-02.** Per-viewer
   (`docReflowWidth`), four steps — Narrow 55 / Reading 80 (default) / Paper (the document's
   own column) / Full — on three surfaces: the View band's `#viewTextWidthBtn` popover,
   View ▸ Text width, and four palette rows, declared as four exact rows in
   `COMMAND_CONTRACT`.
4. **The Reading-view preset** (§5.2 item 3). **Still open and deliberately not built**: it
   sets reduced chrome, which is another lane's surface.
5. ~~**`w15:collapsed`**: parse, model, round-trip, export — **and add a fixture that carries
   it**~~ — **BUILT 2026-10-02**, fixture first as this item said, and §7 item 18 is answered.
6. **Folding** (§5.3): `FoldSet`, the flow filter, the byte-space projection, the outline
   tree, the a11y mirror filter, print/export expansion. **Still open, and now specified
   rather than only proposed**: ADR-049 is Accepted and carries the layout specification
   (where the filter sits, `FoldSet` as a layout input rather than derived, the resume state a
   windowed pass needs, page breaks and section breaks inside a folded range, and the real
   complexity bound) plus what `webapp/` owes. The filter itself belongs to
   `casual-doc-layout` and is not started.
7. **WCAG 1.4.8 item 3** — there is no mechanism to un-justify a justified document in the
   reading view. New, out of scope here, and a reading-view concern rather than a fidelity
   one.
8. **A reader type-size control** (`151` §8 item 2), now cheap once the cap is in characters.
9. **Should `Paged` also accept a viewport?** `151` §8 item 4, unchanged and still open;
   §5.2's "Fit" policy is adjacent to it and may answer it.

## 9. Reproducing the numbers

`SKILL.md` §9.1 — a number is generated from an artifact or it is not published. This
document is not among the pages `webapp/tools/build-doc-pages.mjs` publishes, so the rule
binds by intent rather than by the guard; both derivations are therefore given in full so
that neither has to be believed. **When §5.1 is built, both should become committed
guards** — §7 item 19 says what shape the first one has to take.

**The mean advance (§3.2)** — `python3`, `fontTools`, from the repository root:

```python
from fontTools.ttLib import TTFont
SAMPLE = "<a fixed 674-character English prose sample: the openings of Pride and "  \
         "Prejudice and On the Origin of Species, public domain>"
def mean_advance_em(path):
    f = TTFont(path, lazy=True); upm = f["head"].unitsPerEm
    hmtx, cmap = f["hmtx"], f.getBestCmap()
    adv = [hmtx[cmap[ord(c)]][0] for c in SAMPLE if ord(c) in cmap]
    return sum(adv) / len(adv) / upm
# crates/casual-doc-layout/fonts/Carlito-Regular.ttf            -> 0.3991 em
# crates/casual-doc-layout/fonts/liberation/LiberationSerif-Regular.ttf -> 0.3930 em
# crates/casual-doc-layout/fonts/Caladea-Regular.ttf            -> 0.4156 em
# crates/casual-doc-layout/fonts/liberation/LiberationSans-Regular.ttf  -> 0.4310 em
```

Characters per line is then `column_css_px / (mean_advance_em * font_size_pt * 96/72)`.
At 11pt, Carlito: `14.667 * 0.3991 = 5.853` px per character.

**The column and the refusal threshold (§3.1, §3.3)** — `reflowMeasure` reproduced exactly
from `webapp/src/reflow_view.mjs`, with `cssPerTwip = (96/1440) * zoom`:

```text
totalPx    = floor(clientWidthPx / 16) * 16
gutterTwip = round(16 / cssPerTwip)
content    = floor(totalPx / cssPerTwip) - 2 * gutterTwip      # no ceiling
refused    when content > 31_680                                # MAX_REFLOW_COLUMN, 22in
```
