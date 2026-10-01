# 151 — Reflow (pageless) layout: what the engine owes, and what the shell does with it

**Status:** **Implemented, both halves.** The engine half
(`casual-doc-layout`'s `LayoutView` + `casual-doc-wasm`'s `setLayoutView`) landed
first; the shell half (§6) landed 2026-10-01 and retired `#viewport`'s exemption
from the no-horizontal-scroll rule (§7). **Opened:** 2026-09-30. **Decision:**
[ADR-046](08-ADR-REGISTER.md).
**Advances:** `148` §6 and §9 item 1, `105` UX-019, `01-ORD.md` §137
("continuous layout as a host option").
**Depends on:** nothing. Blocked, and has now released: the retirement of
`#viewport`'s exemption from the no-horizontal-scroll rule (`148` §6).

> **Read this first.** §4 and §6 are both built. The engine work landed as specified
> except in **four** places, all corrected in place below and each marked
> **CORRECTED** with the evidence: §4.5 row 1's vertical metrics, §4.5 row 2's
> `columns.rs`/`paginate.rs` edits, §4.5 row 3's flag, and §4.5 row 7's windowed
> variant. A fifth thing the design did not know about at all — fixed-height tiles
> still showing a blank band at every cut — is §4.4a. Every claim about existing
> code below carries a file and a symbol so it can be checked rather than believed.
>
> **The shell half landed 2026-10-01 and §6 records what it corrected.** Two
> tripwires in `webapp/tests/e2e/phone-no-horizontal-scroll.spec.mjs` fired and
> have been deleted: the outcome one, and the one that fired the day
> `setLayoutView` existed so that a landed engine API could not sit unreachable.
> §6.2 records where this document's proposed quantisation was wrong and why, and
> §7 records the before-and-after measurement and two things §7 did not
> anticipate.
>
> **CORRECTED (154), 2026-10-01 — five things, and one of them is a defect in the
> shipped behaviour.** `154-READING-VIEW-MEASURE-AND-DOCUMENT-FOLDING-COMPETITIVE-`
> `ANALYSIS.md` re-did the competitive analysis at the owner's instruction and
> found this document wrong in five places, each marked **CORRECTED (154)** below
> with a pointer to the section of `154` that carries the evidence:
>
> 1. **§1's trichotomy is false** — there is a fourth answer, capping the measure,
>    and it is the one every reference uses. This is the root error: stating the
>    space of answers as closed is what made a design with no maximum look
>    complete.
> 2. **§6.2 / `reflow_view.mjs` cap the column at nothing.** At a 1440px window
>    the reading column is **1,408 CSS px — 241 characters** of the document's
>    default face, against WCAG 2.1 SC 1.4.8's 80. Worse, `setLayoutView`
>    **refuses outright** above 22in, which a 2160px window at 100% zoom or a
>    1104px window at 50% already asks for. `154` §3.
> 3. **§2.2 omits Google's Text width control entirely** — Google's own cap, and
>    the half of their design we should have copied. `154` §2.1.
> 4. **§2.3's "Word abstaining" is wrong three ways.** Word is the strongest vote
>    *for* capping: Immersive Reader has a four-step Column Width control whose
>    documented purpose is line length. `154` §2.2.
> 5. **§3.2's premise that ONLYOFFICE's reader mode is read-only is false in
>    source.** The decision to stay editable was right; the evidence for it was
>    not. `154` §2.3(d).
>
> What survives is most of it, listed explicitly in `154` §5.4: §4 entire, §4.4a,
> §4.6, §5, §6.2's two numbers, §6.3, §6.4, §6.5 and §7.

## 1. The problem, stated as a measurement

At 390×844 with a document open, `#viewport` reports `scrollWidth 794` against
`clientWidth 326`. The document pans 468px sideways. That is the one surface the
phone tier (`148`, ADR-044) could not bring under the owner's "no horizontal
scroll" rule, and it is named as an exception rather than hidden — in
`webapp/tests/e2e/phone-no-horizontal-scroll.spec.mjs` (`DOCUMENT_SURFACE`), in
`webapp/src/style.css`'s phone block, in `148` §6 and in ADR-044.

A Letter page's text column is 6.5in — **624 CSS px at 96dpi**. It cannot be both
390px wide and readable. There are exactly three answers and only three:

> **CORRECTED (154): there are four, and the fourth is the one every reference
> uses.** This trichotomy is the root error of this document. The missing answer is
> **4. Cap the measure and centre the column**, leaving desk on both sides — Google's
> Text width, Word's Immersive Reader Column Width, and every reader in `154` §2.4.
> Because the list below is stated as closed and reflow is picked out of it, the
> design that follows never asks *how wide* the reflowed column should be; it only
> asks that it not exceed the window. That is how `reflow_view.mjs` came to ship a
> minimum with no maximum and a 241-character line at 1440px (`154` §3). Answer 3 is
> still correct **and it is not complete without answer 4.**

1. **Shrink the page to fit.** `webapp/src/view_zoom.mjs`'s `FIT_ON_OPEN_FLOOR = 0.5`
   already computes this and refuses it, with the reason in the file: a phone fits a
   Letter page at about **31%**, and the first version of that code shipped exactly
   that — "a postage stamp of a document where the defect had at least left readable
   text to pan across." ONLYOFFICE take this answer anyway: `Main.jsx:452-453` calls
   `this.api.Resize(); this.api.zoomFitToWidth();` unconditionally on every open,
   overriding the saved zoom.
2. **Pan.** What the shell does today.
3. **Reflow** — lay the text out at the width that is actually available and stop
   drawing a page. **Two of the three references answer a too-wide page this way**,
   and the third does not document either way (`148` §4).

Answer 3 is correct, and this document is what it costs.

## 2. Prior art, named before the design (`SKILL.md` §8)

The established solution has a name in both references and it is the same solution:
**substitute the section geometry at layout time and leave the flow engine alone.**
It is not a second layout algorithm.

### 2.1 ONLYOFFICE — source-verified

Read from `/Users/sachin/Desktop/melp/reference/sdkjs`. AGPL-3.0: **behaviour and
structure only, no code taken.**

Their reader mode has two generations in the tree and the *new* one is the
instructive one. `word/Drawing/HtmlPage.js:1090` — `SetNewMobileMode()`:

- takes the **first** section's page width and height,
- divides by the retina ratio,
- computes a font scale from a reader font-size ladder, and
- calls `CDocument.SetDocumentReadMode(W, H, scale)` (`word/Editor/Document.js:17253`).

`SetDocumentReadMode` swaps `this.Layout` from `Layouts.Print` to `Layouts.Read`,
recompiles every run's, paragraph's and table's compiled properties, and
recalculates from the start. `Layouts.Read` is `CDocumentReadView`
(`word/Editor/Layout/ReadView.js`), a subclass of `CDocumentLayoutBase`, and it is
**forty lines of overrides**:

| Override | What it does |
| --- | --- |
| `Set(W, H, scale)` | builds a **synthetic `SectPr`** with `SetPageSize(W, H)` and `SetPageMargins(5, 5, 5, 5)` |
| `IsHeaders()` | `false` |
| `GetSectionHdrFtr()` | `{ Header: null, Footer: null, SectPr: <the synthetic one> }` |
| `GetPageContentFrame` / `GetColumnContentFrame` | the synthetic frame, with `ColumnSpaceBefore/After = 0` |
| `GetSectionByElement` / `CheckSectPr` / `GetFinalSectPr` / `GetSectionIndex` | always the synthetic section, index 0 |
| `GetFontScale()` | the reader scale |
| `GetScaleBySection(sectPr)` | a ratio that shrinks a section wider or taller than the view |
| `calculateIndent(ind, element)` | **rescales indents proportionally**: `ind * W / sectPr.GetPageWidth()`, and clamps a negative indent at `-2` |
| `GetCalculateTimeLimit()` | 100 (a shorter recalculation slice — a phone) |

Three things are worth taking from this and one is worth refusing:

- **Take:** reflow is a *view* object consulted for geometry, not a mutation of the
  document. Nothing in `SetDocumentReadMode` writes to the document's own `SectPr`.
- **Take:** headers, footers and columns are suppressed by the view, not by a second
  pipeline.
- **Take:** indents are rescaled rather than kept, so a 2in first-line indent does not
  eat half a phone's width.
- **Refuse:** their view still paginates onto pages of size `W × H` where `H` is the
  *original* page height. So their "reflow" is really "repaginate onto
  viewport-width pages". That is a sound mechanism (§4.4 adopts it) but their
  `H` choice makes page breaks appear in arbitrary places on a phone; ours should be
  chosen for the reader, not inherited from the paper.

> **CORRECTED (154): "viewport-width pages" is wrong, and their `W` is stranger than
> this section knew.** `W` is not the viewport at all — it is
> `sectPr.GetPageWidth() / AscCommon.AscBrowser.retinaPixelRatio`
> (`word/Drawing/HtmlPage.js:1099-1100`), the **paper's own width divided by the
> device pixel ratio**, with a flat 5mm margin on all four sides
> (`ReadView.js:79`). So their measure tracks the display's pixel *density* rather
> than its size: two phones of identical physical width get different reading
> columns if their DPRs differ. There is no maximum column width anywhere in
> `word/`. What they *do* vary for the reader is the **type size** — a nine-step
> ladder `[12,14,16,18,22,28,36,48,72]`pt, default 16pt
> (`HtmlPage.js:104-105, 1102`), fed to `GetFontScale()` (`ReadView.js:90-93`). They
> reach a reading measure by growing the advance instead of narrowing the column.
> `154` §2.3(a)–(c) and §5.1's last paragraph, which says how that composes with a
> cap rather than competing with it.

Their touch side for that mode is a separate, much simpler `CReaderTouchManager`
(`word/Drawing/mobileTouchManager.js:833-915`) with `SelectEnabled = false` and
`TableTrackEnabled = false` — i.e. **their reader mode is not editable.** §3 says
why ours must be.

> **CORRECTED (154): that inference is false, and it was load-bearing.**
> `SelectEnabled` has **exactly two consumers in the whole tree**, both inside the
> touch-manager base — `CheckSelectTrack`
> (`common/Scrolls/mobileTouchManagerBase.js:928`, comment: *"onTouchStart => check
> if we hit selection anchors, to avoid starting scrolls/zooms"*) and `CheckSelect`
> (`:1873`). They gate **touch selection handles and table-resize handles**, not
> document mutation. Read-only in ONLYOFFICE is a separate mechanism:
> `turnOnViewerMode()` calls `api.asc_addRestriction(Asc.c_oAscRestrictionType.View)`
> (`web-apps/.../mobile/src/controller/Toolbar.jsx:296-304`), and
> `changeMobileView()` eight lines below it (`:306-311`) calls
> `api.ChangeReaderMode()` and touches no restriction. Their edit buttons are gated
> on `isEdit`/`isViewer`, not on reader state (`mobile/src/view/Toolbar.jsx:148`).
> **Our decision to stay editable is unchanged and is right** — but the reason is the
> mobile-support one in §3.2's second paragraph, not a divergence from a read-only
> field, because there is no read-only field: Word's Immersive Reader is editable too
> (`154` §2.2(c), first-party). `154` §2.3(d) re-read each line above directly.

### 2.2 Google Docs — Pageless

Answer 10296604: **⋮ → Page setup → Pageless** "removes page boundaries and reflows
content as a continuous scroll". Two properties matter for §4:

- It is a **document-level setting that persists and syncs**, not a per-viewer view
  toggle — Google made it part of the file. We deliberately do not (§3.3).
- **A wide table gets its own sideways scroll.** The overflow is contained to the
  element that genuinely cannot fit rather than handed to the page. That is exactly
  the arbitration this shell already uses for a ribbon band, and §6.3 adopts it.

> **CORRECTED (154): a third property matters more than either of these, and this
> section omits it.** Google ships **View → Text width** for pageless documents —
> Narrow, Medium or Wide — and the support page states *"Your text width choice
> won't affect how collaborators see your docs."* So Google splits the two
> decisions, and splits them **opposite ways**: the *format* is a document property
> (`DocumentStyle.documentFormat`, `DocumentMode.PAGES|PAGELESS`, in the Docs API),
> and the *width* is per-viewer with no API field at all. This document records the
> first row and calls it a deliberate divergence, which it is (§3.4) — and never
> mentions the second row, which is Google's answer to the question §1 forgot to
> ask. A design citing Pageless as prior art while omitting its width control has
> cited half the prior art. `154` §2.1 and §5.1.
>
> Also absent from this section: **Google ships collapsible headings in Pageless**
> (announced 2023-05-16), with the same two-tier persistence — an editor sets the
> saved default for everyone, a viewer's own toggling is not persisted. `154` §2.1
> and §5.3, which places folding outside reflow entirely.

### 2.3 Word

`148` §4 found no reflow toggle in Microsoft's own documentation, in either
direction, and recorded it as unverified rather than as an absence. Word for
Windows' **Web Layout** view is the nearest thing and is not documented for the web
or mobile clients. So the vote is Docs and ONLYOFFICE for, Word abstaining.

> **CORRECTED (154): Word does not abstain. Word is the strongest vote FOR capping,
> and this paragraph removed the best evidence in the field from the record.** It is
> wrong three ways — `154` §2.2 has the sources:
>
> 1. **Immersive Reader has an explicit measure control.** Text Preferences →
>    **Column Width**, four named steps (Very Narrow, Narrow, Moderate, Wide), and
>    Microsoft's own accessibility page states the purpose outright: it *"changes line
>    length to improve focus and comprehension."* Alongside Text Size, Text Spacing,
>    page colour, Line Focus. **And it is not read-only** — *"Once you click in your
>    Word document to read or edit, the Immersive Reader ribbon will minimize."*
>    (first-party, fetched).
> 2. **Read Mode reflows into adjustable columns** — *"Read Mode automatically fits
>    the page layout to your device, using columns and larger font sizes, both of
>    which you can adjust"* (first-party, fetched). Word's reading view moves two
>    levers, columns and type size, and lets the reader move both.
> 3. **Web Layout is the analogue of what we built, flaw included** — editable, no
>    page boundaries, headers/footers hidden, and text wrapped to the **window
>    width** with no cap. And **Word for the web's original rendering was continuous**;
>    "Separate Pages" was added as an option (Microsoft's own Insider blog). So "not
>    documented for the web" is stale.
>
> Four Word things, three aims: **Web Layout** = pageless authoring (what we built),
> **Read Mode / Immersive Reader** = reading (capped measure), **Focus** = chrome
> hiding only. `154` §5.2 keeps those three separable here rather than fusing them
> into one toggle.

## 3. What reflow is here — four decisions

### 3.1 It is a VIEW, not an edit

The document's `SectionBoundary` is never written. This is not a style preference:
`crates/casual-doc-wasm/src/lib.rs`'s `setPageSetup` (line ~8253) issues
`Operation::SetSectionGeometry`, a real undoable, **exportable** mutation. Driving
reflow through it would work today and must not be done — it would pollute undo,
dirty autosave and drafts, and a save would persist a 390px-wide "page" into the
user's DOCX. That is the silent-data-loss class `SKILL.md` §12 forbids outright.
Reflow therefore takes a parameter the layout driver reads and the document never
sees.

### 3.2 The document stays EDITABLE

This is where we diverge from ONLYOFFICE, deliberately and at a cost. Their reader
mode sets `SelectEnabled = false`; a phone user who wants to edit must leave reflow,
at which point they are back to panning a 794px page. The owner's constraint is that
a phone is a supported editing device (`18-SUPPORT-MATRIX.md`; `SKILL.md` §1 puts
native app shells out of scope, so the browser **is** the mobile story), and a
reading mode you have to leave to type is not an answer to §1.

It costs nothing structurally, because of §4.4: if reflow emits ordinary
`page::Page` values, then `hitTest`, `objectAt`, `resolveClick`, the caret, the
overlays and every edit operation keep working unchanged. Editability is a
consequence of choosing the right output shape, not a feature bolted on.

### 3.3 Pagination is ADVISORY, not absent

"Stop paginating" is the wrong instruction to give this engine, and §4.4 explains
why in mechanism terms. What actually happens is:

- **page-shaped *constraints* are suspended** — `page_break_before`, `keep_next`,
  `keep_lines`, widow/orphan control, section-start parity, column breaks;
- **page-shaped *furniture* is suppressed** — headers, footers, page borders,
  watermarks, line numbers, footnote separators-at-the-bottom;
- **the paginator still runs**, cutting the galley into fixed-height tiles at line
  boundaries, because something has to bound a raster.

A `PAGE` field therefore resolves to a tile index, which is not a page number and
must not be presented as one. §6.5 says what the shell does about that.

### 3.4 It is a per-viewer preference, not a document property

Google made Pageless a file setting. We do not, for the reason ADR-044 already
gives: one person's phone and another person's 27in monitor are looking at the same
file, and a view chosen on the phone must not reformat the desktop. It is stored
where the theme and the chrome mode are stored (`webapp/src/prefs.mjs`), and it
**defaults to on below the phone rung and off above it** — which is the only place
in this design where a device class decides anything.

## 4. What the engine owes — precisely

Everything in this section is in `crates/**`. It was **not** built by the lane that
wrote this document; the file and symbol references are current as of
`29470dd3` and should be re-checked before work starts.

### 4.1 The good news: line breaking is already width-parametric

This is the finding that makes the estimate small, and it is worth stating first
because it contradicts the assumption `148` §9 was written under ("a layout pass at
a width that is not the section's page width" sounded like new machinery; it is not).

`crates/casual-doc-layout/src/flow.rs` takes `content_width: Twip` as an **explicit
argument** on every entry point, not from a global:

| Symbol | Line | Note |
| --- | --- | --- |
| `build_galley(document, shaper, content_width)` | 304 | the whole body at an arbitrary width |
| `build_galley_for_blocks(document, shaper, blocks, content_width)` | 396 | an arbitrary slice at an arbitrary width |
| `build_galley_cached(document, shaper, content_width, cache, dirty)` | 868 | the incremental path; its doc comment already says *"The cache is scoped to `content_width`; a width change transparently clears it."* |
| `flow_header_footer(document, blocks, shaper, content_width)` | 759 | a band at a non-page width |
| `flow_anchored_text_box(...)` | 4062 | a text box's inner width |
| `flow_table` / `solve_column_widths(cols, spec, available, layout)` | 1849 / 2717 | `w:tblW` auto/pct resolve against the passed width |
| `paragraph_decor(properties, width)` | 7270 | indents, alignment and justification resolve against the passed width |

So justification, right indents, tab stops, auto-width tables, drop caps, text
boxes and float wrap carry **already** honour whatever width they are handed. This
is the "uniform flow pipeline" invariant this repository has been holding on
purpose, and reflow is the first thing to collect on it.

`ColumnLayout::single(content: Rect)` (`columns.rs:97`) is the one-column
constructor reflow reuses verbatim.

### 4.2 The gap: where the width comes from

The width is *always* `PageConfig::content_area()` (`paginate.rs:140`,
`width = page_size.width - margin_start - margin_end`), and `PageConfig` is *always*
derived from the document's own section by `section_page_config(&SectionBoundary)`
(`document_layout.rs:148`). **There is no layout-options struct anywhere in the
workspace** — `LayoutOptions`, `LayoutConfig` and `LayoutMode` have zero hits.

So the change is a **driver change**, not a flow change.

### 4.3 The shape: one mechanism, not two

`SKILL.md` §8: *"Prefer one mechanism over two. When a design needs a parallel path,
that is evidence the abstraction is wrong."* So: **not** a second paginator, and
**not** a `paginate_document_reflow` that duplicates
`paginate_with_note_labels`'s pipeline. Instead, a view parameter threaded to the one
place geometry is decided, exactly as `ReviewView` is already threaded to the one
place byte space is decided:

```text
paginate_document(document, shaper)
  └─ paginate_document_view(document, shaper, ReviewView)          ← today's entry
       └─ paginate_document_in(document, shaper, ReviewView, LayoutView)   ← new entry
```

with

```text
enum LayoutView {
    /// The document's own paper. Every existing caller.
    Paged,
    /// Lay the body out at `content_width`, cut into `tile_height` tiles.
    Reflow { content_width: Twip, tile_height: Twip, gutter: Twip },
}
```

`LayoutView::Paged` must be the `Default`, and the existing entry points must
delegate with it, so **every existing caller is byte-for-byte unchanged** and the
`equals_manual_wiring` regression test still holds. This also avoids the
`SKILL.md` §5a hazard: adding a field to `PageConfig` would break every literal
that constructs one in another crate's `#[cfg(test)]` module, on a branch that
never sees them.

### 4.4 The output shape — and why "stop paginating" is the wrong instruction

The flow pass **already** produces an unpaginated result: `build_galley` returns
`Vec<BlockFragment>` with no page concept in it. The reason reflow cannot simply
return that is downstream: `compose::compose_page(page: &Page)` (`compose.rs:256`),
all of `hittest.rs`, and every `#[wasm_bindgen]` geometry method are page-indexed,
and `page::Page` (`page.rs:367`) is the only renderable unit.

Two candidate outputs:

- **(A) One tall page.** `page_size.height = total galley height`. Smallest engine
  change — pagination effectively no-ops. **Rejected on a hard limit:** `renderPage`
  rasterizes a whole page into one `Surface` (`casual-doc-render/src/lib.rs:100`),
  and a browser canvas maxes out near 32,767px in either axis. A thirty-page
  document reflowed at 390px is far past that. It would work on the fixture and fail
  on a real document, which is the `MAX_VIEWER_BLOCKS` mistake `SKILL.md` §8 records.
- **(B) Fixed-height tiles — adopted.** Keep the paginator running at a synthetic
  tile height with all break *constraints* disabled, so tiles cut mid-paragraph at a
  line boundary. Output is an ordinary `PaginatedLayout`; `renderPage`, `hitTest`,
  `scaleOf`, the selection rects, `viewport_of` and the whole windowed path keep
  working with no change at all. The shell then draws the tiles with **zero gap and
  no sheet shadow**, and what the reader sees is a continuous column.

Tile height is a **reading** decision, not an inherited one: ONLYOFFICE keep the
original page height (§2.1) and get page breaks in arbitrary places. A tile is an
implementation detail of rasterisation and should be invisible, so pick a height
that is large relative to a viewport (so a scroll rarely crosses one) and small
relative to a canvas limit. **11in (15,840 twips) is the proposed default**, with
the number recorded as a decision rather than a constant with no source.

### 4.4a CORRECTION: zero-gap tiles are not enough — each tile must be trimmed

This is the one thing §4.4 got wrong by omission, and it was only visible once the
tiles were on screen next to each other.

The paginator fills a tile with **whole chunks**: the chunk that does not fit is
carried to the next tile, and the space it would have occupied stays empty at the
bottom of this one. That slack is up to **one line high** (or one table row). So
tiles drawn at `gap: 0` still show a blank band at every cut — the exact artefact
reflow exists to remove, reintroduced at a different frequency.

The fix is a **reflow-only final pass** (`document_layout.rs`'s `trim_reflow_tiles`,
last in `post_pagination_passes`) that cuts each tile's `page_size.height` and
`content_area.height` down to the extent of what is on it: placed body content,
floats resolved onto it, footnotes. Properties that matter:

- **Idempotent**, so it can live in the shared post-pagination pass and be re-run
  over a tile the incremental path reused.
- **Only ever shortens.** A page-anchored float that the §8 item 1 approximation
  left below the text holds its tile open rather than being clipped.
- It makes the painted column **independent of `tile_height`**, which is what turns
  the guard into something exact: stack the tiles of the same document cut at 2in,
  3in and 11in and the painted glyph positions are identical. That is the guard
  `tests/reflow.rs` ships, because it is the only form of "a tile is as tall as its
  content" that has no tolerance in it.

One consequence to know about: a trimmed tile no longer matches the tile height its
`PageConfig` declares, so `paginate::repaginate_at`'s existing geometry check — which
exists to stop a page with a stale content area being reused — declines the
incremental *pagination* resume under reflow and re-tiles from the top. That is an
`O(pages)` walk with no re-shaping; the galley cache still makes a keystroke
`O(edit)` in shaping, which is ~99% of the cost.

### 4.5 The engine work item list

This is the list as designed, annotated with what was actually built. Four rows were
wrong and are **CORRECTED** here; the corrections are the substance of this section
now, because the design's own version would have shipped visible defects.

| # | File | Change |
| --- | --- | --- |
| 1 | `casual-doc-layout/src/document_layout.rs` | `LayoutView` (§4.3); `reflow_page_config()` beside `section_page_config`; thread the view through `build_section_plans`, `push_section_run`, `build_section_runs_cached`, `finish_pagination`, `finish_pagination_pass` and `post_pagination_passes`; new `paginate_document_in` beside `paginate_document_view`, and `paginate_document_after_edit_in` beside `paginate_document_view_after_edit` so a keystroke in reflow stays in reflow. **CORRECTED:** the design said the tile takes "the section's vertical metrics". It must not. A section's 1in top margin applied *per tile* paints a white band across the middle of a paragraph every 11in of scroll — a tile's top and bottom edges are not page edges. A tile is `page_size = (content_width + 2·gutter, tile_height)` with `margin_top = margin_bottom = 0`, `margin_start = margin_end = gutter`, and zero header/footer distance AND height (`PageConfig::content_area` only consults a distance when the matching height is non-zero, so both have to go). The reader's breathing room at the top and bottom of the *document* belongs to the host's scroller, not to every tile |
| 2 | ~~`casual-doc-layout/src/columns.rs`~~ | **CORRECTED: no edit needed, and the driver-side version is better.** Forcing one column is `ColumnLayout::single(config.content_area())` in `push_section_run` — the same constructor the section-less body already uses. Clearing the section start type is `starts_new_page: false` / `start_parity: None` on the `SectionRun` the driver builds. Teaching `columns.rs` about the view would have put the same rule in two places |
| 3 | ~~`casual-doc-layout/src/paginate.rs`~~ (paginators) | **CORRECTED: the paginators need no edit.** The constraints are suspended by clearing the galley's break flags **once**, in the driver, before any paginator sees it (`suspend_page_break_constraints`) — which covers all **three** paginators (`paginate`, `ColPaginator`, `paginate_section_footnotes`) without a line changing in any. Two tiers had to be cleared, not one: a paragraph's `BreakControl` (`w:pageBreakBefore`/`keepNext`/`keepLines`/`widowControl`) **and** a line's `page_break_after`, because `flow.rs`'s `apply_section_break` stamps a `nextPage` section break onto the last line using the same flag an explicit `<w:br w:type="page"/>` uses — so without the second tier a section break still cut a tile. `LineBreak::Page` is left alone, so a page break becomes a line break, which is what Pageless does with one. **CORRECTED on the field half too:** the design wanted the `PAGE`/`NUMPAGES` approximation flagged on `PaginatedLayout`. That struct has 7 literals across two crates and adding a field to it is the `SKILL.md` §5a hazard. The refusal belongs in the labels instead — `resolve_fields_labeled` already takes per-page `PAGE` strings, so reflow passes a refusal token, and only `NUMPAGES`'s total needed widening from `u32` to `&str`. The token is an em dash, and the engine refuses rather than printing a tile count because a host cannot un-print a number the engine already shaped into a glyph run |
| 4 | `casual-doc-layout/src/running.rs`, `page_border.rs`, `line_number.rs`, `watermark.rs` | suppressed under reflow — **and none of those files was edited either.** Headers, footers and page borders are suppressed by building an **empty `SectionPlan`**, so the passes that place them find nothing and are already correct for a section that declares none. Line numbers and the watermark are skipped in `post_pagination_passes`, along with section `w:vAlign` (which distributes slack inside a page, on a tile whose height is about to be trimmed to its content) |
| 5 | `casual-doc-layout/src/anchor.rs` | **Unchanged, and still open.** Page- and margin-relative floats keep their paper-relative positions. Rather than pick an answer quietly, the view **reports** it: `LayoutView::approximations()` returns the sentence, `setLayoutView` hands it to the host. §8 item 1 stands |
| 6 | `casual-doc-layout/src/flow.rs` | **untouched**, as designed. §6.3's wide-table decision remains a shell decision |
| 7 | ~~`casual-doc-layout/src/windowed.rs`, `measure.rs`~~ | **CORRECTED: an honest refusal, not a variant.** The design argued a refusal "means a large document loses reflow, which on a phone is the wrong way round". That reasoning misses what a windowed body already is: **read-only.** `apply_group` refuses every one of the 55 operations on one (`windowed_not_available("Editing")`). Reflow's defining promise — and the whole of §3.2, where we diverge from ONLYOFFICE — is that the document stays editable in it, so that promise cannot hold on a windowed body whatever the layout looks like. A variant would have been a second, read-only reflow with different guarantees, sold under the same name. `setLayoutView` refuses above `MAX_WHOLE_LAYOUT_BLOCKS` with the reason; clearing the view stays available, so a host that persisted the preference is not stuck on an error |
| 8 | `casual-doc-wasm/src/lib.rs` | `setLayoutView(contentWidthTwip, tileHeightTwip, gutterTwip)` returning the view as JSON (with its approximations), plus a `layoutView` getter; a `layout_view` field on `WasmDocument`; honoured by `repaginate`, `finish_edit_with` and `set_show_changes_inner`. Issues no `Operation`, bumps no document revision, never reaches the export path (§3.1). A mode change **discards the galley cache and rebuilds whole**, never resuming a layout built in the other view. **CORRECTED on where the arms go:** `page_size_inner` needs no arm of its own, but `page_box` — which it and `render_page_inner` both call — absolutely does, and it was the one place that mattered. `page_box` looks a tile's size up **by its section id**, not off `Page::page_size`, so without a reflow arm every tile would be reported *and rasterised* at Letter size and the trim would have bought nothing. `page_ruler_geometry_inner` needs one for the same reason (it reads `SectionBoundary`); until §6.4 hides the ruler, the honest answer is the tile's own geometry |
| 9 | tests | `casual-doc-layout/tests/reflow.rs` (17 cases) and eight cases in `casual-doc-wasm`. `geometry_snapshot.golden` does **not** move. One thing found by mutating rather than assumed: the inertness comparison is only **half** a guard, because both of its sides run the same driver and a driver that reflowed unconditionally would move both and leave it green. The golden is the other half — it is a committed artifact and cannot move with the code — and `tests/reflow.rs` now says so rather than claiming inertness on its own |

### 4.6 The guards the engine shipped with

`SKILL.md` §4: it has to be possible to drive it red. Every guard below was driven
red by mutating the production code, and each one's mutation and failure output are
recorded on the test.

The two that matter most are asserted at the **paint tier** — over the
`DisplayList` a renderer executes — rather than over the model, because a
model-level assertion has passed through every real defect in this engine.

1. **Inertness.** `paginate_document(d, s)` and
   `paginate_document_in(d, s, Editing, Paged)` agree page-for-page over eleven
   fixtures (prose, banded, multi-column, bordered, line-numbered, watermarked,
   `vAlign`, two-section, page-broken, tabular, field-bearing), in both review
   views. **This is only half the guard**, and the reason is worth keeping: both
   sides run the same driver, so a driver that reflowed *unconditionally* moves
   both and leaves it green. The other half is `geometry_snapshot.golden`, a
   committed artifact that cannot move with the code — mutating
   `reflow_page_config` to ignore its view leaves the inertness test green and
   turns the golden red, the first fixture going from
   `size=(612.00x792.00) content=(72.00,72.00 468.00x648.00)` to
   `size=(306.00x792.00) content=(18.00,0.00 270.00x792.00)`.
2. **The width is honoured, at the paint tier.** Nothing a reflow layout *paints*
   reaches past `gutter + content_width`, and the tile is exactly
   `content_width + 2·gutter` wide. A glyph run's right edge is its **inked** pen
   extent: a line broken at a space carries that space's advance and is allowed to
   hang past the measure, and counting it reported a 43-twip overhang on a
   correctly justified line. Flowing the galley at
   `section_page_config(boundary).content_area().size.width` while leaving the tile
   geometry correct — the driver's behaviour before this work — fails it with
   `tile 1 paints out to 9652 past its own 6120 width`.
3. **A tile is exactly as tall as its content, at the paint tier.** Asserted in the
   strong, tolerance-free form §4.4a explains: the **stacked painted column is
   identical** whether the document is cut at 2in, 3in or 11in tiles. Deleting
   `trim_reflow_tiles` fails it with `glyph run 9 moved between a 2880-twip tiling
   and a 4320-twip tiling: (360, 3111, 5104, 52) vs (360, 2841, 5104, 52)` — a
   270-twip shift. The companion "nothing falls off a tile" carries **one** stated
   twip of tolerance, because a line box's height is a rounded sum of face metrics
   and a face's descent can land one rounding unit below it (1/1440in — 0.067 CSS
   px, a quarter of a device pixel at 4x). Measured, not tuned; the mutation it
   guards misses by 270.
4. **Editability.** Every model position the paged layout can place a caret at is
   still placeable in reflow, and `hit_test` on its caret rect still returns that
   same position. This is the guard that stops (B) from quietly becoming the
   read-only view ONLYOFFICE ships.
5. **No mutation.** The document's serialised bytes and its section geometry are
   unchanged after a reflow pass; on the wasm side, `log.head()` (the *document*
   revision) does not move, nothing lands on the undo stack, and `pageSetup` reads
   back identically. §3.1 made into a test rather than a promise.
6. **Complexity, by doubling.** Entering reflow costs about twice as much at twice
   the document (1.6–2.4x measured, on `GalleyCache::shaped_last_build`); a
   steady-state keystroke in reflow re-shapes a **flat** number of paragraphs,
   asserted *equal* at *n* and 2*n* rather than merely sub-linear, because the
   behaviour a regression would reintroduce re-shapes every paragraph and a
   doubling bound would accept it.
7. **The suspension never leaks into a paged layout.** A document declaring
   `w:pageBreakBefore` keeps the page that break creates across a reflow round trip
   and across typing in both views. Making the driver suspend in every view fails it
   with `the fixture must gain a page from the break (11 -> 11)`.

**One hazard is documented rather than guarded, and is recorded here rather than
left to be rediscovered.** A reflow pass clears the break flags on the galley it
*retains*, and the incremental path moves that galley's fragments into the next
build. `GalleyCache::begin_build` already drops a retained galley whose width does
not match, which covers every reflow width except one — a host asking for a column
exactly as wide as the document's own text measure. `setLayoutView` discards the
cache outright to close that hole, and **no test in this repository can demonstrate
it**: the galley-reuse fast path is gated behind `!cfg!(debug_assertions)` in
`build_galley_cached`, so a debug build always re-derives the fragment and verifies
its hash, and a release-only defect cannot be reproduced by `cargo test`.

## 5. Complexity, because a phone is the slowest device we support

`SKILL.md` §8 and `docs/107` §4: per-interaction work is O(1) in document size, and
anything O(document) must be off the main thread, cancellable, and show progress.

- **Entering or leaving reflow is O(document)** — it is a full re-shape, because the
  galley cache is width-scoped (`flow.rs:868`) and will correctly self-clear. It is
  therefore a mode change, not an interaction: it must go through the same
  background/progress path an open goes through, and it must be cancellable. It must
  **not** be driven from a resize event directly (§6.2).
- **A keystroke in reflow is O(edit)**, exactly as in `Paged`: the incremental path
  (`paginate_document_view_after_edit`, `document_layout.rs:842`) is unaffected by
  where the width came from.
- **A resize is the trap.** In `Paged` nothing re-lays-out on resize; in reflow every
  resize changes the content width. A phone rotating, a keyboard opening, a URL bar
  retracting and a desktop window drag all fire it. §6.2 is the answer and it is a
  requirement, not a nicety: an un-debounced, un-quantised resize handler here is an
  O(document) pass per animation frame.
- **Guard by doubling, not by milliseconds** (`SKILL.md` §8): build documents of *n*
  and *2n*, reflow both, assert the work roughly doubles. A timing threshold cannot
  tell a slow constant from a quadratic.

## 6. What the shell does with it

All of this is `webapp/` and consumes §4.8's setter. **Built 2026-10-01**, in
`reflow_view.mjs` (the decisions — which widths are the same width, how long to
wait, when the engine will refuse) and `reflow_chrome.mjs` (the wiring), the split
`spelling.mjs` / `spell_check.mjs` already established here. Each subsection below
now says what shipped and, where the specification was wrong, what was corrected.

### 6.1 The toggle

**Shipped as specified.** A command `view.reflow` — label carrying its state, the
shape `view.compactRibbon` already uses ("Reflow: on" / "Reflow: off"), filed in
`APP_MENU_SECTIONS.view`'s `menuGroup.show` band beside `view.outline` and
`view.showChanges`, plus a View-band ribbon face (`#viewReflowBtn`, declared in
`ribbon_faces.mjs`). Persisted in `prefs.mjs` under `docReflow`. Default on below
`PHONE_MAX_WIDTH`, off above it (§3.4) — evaluated from the rung on every
question rather than resolved once at boot, so a rotation across the rung answers
correctly and there is no second copy of the state to go stale.

**Two surfaces, and `ribbon-command-faces.spec.mjs` compares them**: the ribbon
face is driven with a pointer and the id is run from the palette, and the two are
required to have the same observable effect. That guard needed one change to
accept a toggle whose state PERSISTS — it reloads the editor between the two
halves on the premise that neither can decide the other's answer, and a
`localStorage` preference survives a reload by design, so the second half started
from the opposite state. It now forgets preferences between halves, which closes
the same hole for every future persisted toggle.

**Declared in `COMMAND_CONTRACT`** (`host_contract.mjs`) as requiring nothing,
and that is a claim worth making explicitly rather than by omission: reflow is a
layout VIEW and not an edit, so a host that has granted no mutation capability at
all can still offer it — and a host embedding the editor in a 400px column has a
reason to.

**When the engine refuses, the control is disabled WITH the reason.** The one
refusal today is a body laid out one page-window at a time, which the engine
refuses because reflow's promise is that the document stays editable and a
windowed body is already read-only. There is no `reflowUnavailableReason` getter,
so the shell reads `editingUnavailableReason`, which is non-empty under exactly
the same predicate (`self.layout.is_windowed()` in both) — and the call site also
catches the throw and shows the engine's own words, so if the two ever diverge
the failure is a late honest message rather than a dead control. **An engine-side
`reflowUnavailableReason` getter would be strictly better and is reported as
engine work rather than reached for from this lane.**

Naming: Google says "Pageless", ONLYOFFICE says "Reader mode", Word says "Web
Layout". Ours is **"Reflow"** because it is the only one of the three that is
accurate for a mode that is still editable and still cut into tiles — and because
"Reader mode" would promise ONLYOFFICE's read-only behaviour, which §3.2 rejects.

> **CORRECTED (154), twice.** The naming argument's second half rests on §3.2's false
> premise — ONLYOFFICE's reader mode is **not** read-only — so "Reader mode" would
> promise nothing untrue. The name "Reflow" survives anyway, for its first reason.
>
> **The CONTROL SHAPE is the real correction: a binary toggle is wrong.** It fuses
> two questions with different answers — *is the document on paper?* (layout:
> pagination, headers, ruler, Pages panel) and *how wide is the text?* (measure:
> nothing structural). They coincide only on a phone, where the window is narrower
> than any cap, which is why the phone-only evaluation could not see the
> distinction. Google ships them as two controls (Pageless, then Text width) and
> Word ships three separable things (Web Layout, Immersive Reader's Column Width,
> Focus). `154` §5.2 recommends: keep this toggle for the layout question, add a
> four-step per-viewer **width** control, and ship "Reading view" as a **preset**
> that sets layout + width + chrome + folding — one registry row writing four
> preferences, not a second layout mechanism.

### 6.2 Feeding the width back

**Built 2026-10-01.** `renderAll()` calls `reflowView.sync(cssPerTwip)` after the
zoom is resolved and before `pageCount` / `pageSize`; that lives in
`reflow_chrome.mjs` and it is what talks to `doc.setLayoutView(...)`. The width it
passes is the viewport's `clientWidth`, quantised, converted to twips at the
current zoom, with the gutters taken OUT of the bucket rather than added on top —
the tile's TOTAL width is what gets painted, so it is the total that has to fit,
and getting that the other way round is how a gutter becomes an overflow.
`buildPageBand` already took `options.gap` (`page_scroll.mjs`), so tiles at
`gap: 0` were a call-site change as predicted, and `#viewport.is-reflow` in
`style.css` removes the sheet shadow and corner radius that would otherwise draw
a paper edge across a sentence.

> **CORRECTED (154) — THE DEFECT. This section answers "not wider than the window"
> and never asks "how wide should a reading column be", so the shipped width feed has
> a minimum and NO MAXIMUM.** `reflowMeasure` computes
> `floor(totalPx / cssPerTwip) - 2 * gutterTwip` against `REFLOW_MIN_CONTENT_TWIP`
> and nothing else. Measured consequences (`154` §3, with the derivation in `154` §9):
>
> | Viewport @100% | Column | Characters, 11pt Carlito |
> | --- | --- | --- |
> | 390 (phone) | 352 px | **60** — correct, and the only width ever evaluated |
> | 1280 | 1,248 px | 213 |
> | 1440 | 1,408 px | **241** |
> | 1920 | 1,888 px | 323 |
>
> against WCAG 2.1 SC 1.4.8's normative *"Width is no more than 80 characters or
> glyphs (40 if CJK)"*. **And it is not only too wide — above a reachable width it
> fails outright.** `LayoutView::reflow` refuses `content_width > MAX_REFLOW_COLUMN`
> (22in), whose doc comment says a caller asking for more "has converted units
> wrongly" — written as a unit-conversion sanity check and now reachable by a correct
> caller. First refused viewport width: **2,160px at 100% zoom, 1,632px at 75%,
> 1,104px at 50%** (and 50% is a `ZOOM_STEPS` entry and the `FIT_ON_OPEN_FLOOR`
> value). `sync` catches the throw, flips the preference back to paper and shows the
> reader a message about twips.
>
> The fix is one clamp in the **caller** — `min(available, cap)` — and needs **no
> engine change at all**, because ADR-046 put the measure in the seam. `154` §5.1
> recommends the cap as a target in **characters** (default 80, from WCAG 1.4.8 —
> the only normative first-party figure available), resolved to twips by the measured
> mean advance of the document's default face, with the column centred on the desk
> above the cap. A capped column also cannot reach 22in, so the refusal goes away as
> a side effect.
>
> **The two numbers below are still right**, and once a cap exists they bind only
> *below* it — above the cap the column stops tracking the window, which makes most
> desktop resizes free for a second and better reason.

**Two numbers, and this section proposed one of them wrongly.** It said "the
nearest 8px". What shipped is **16px, floored** (`REFLOW_QUANTUM_PX`), and both
halves of that are corrections rather than preferences:

- *Floored, not nearest.* Rounding to the NEAREST bucket rounds up half the time,
  which makes the column wider than the space it was measured against — and a
  column wider than the viewport is a horizontal scroll on `#viewport`, the exact
  defect §7 exists to retire. A rounding rule that can reintroduce the defect it
  is part of the fix for is the wrong rounding rule. Driven red: rounding to the
  nearest reports `8 -> 16 is wider than the space it has`.
- *16, not 8.* Because it is floored, the quantum **is** the safety margin: the
  column is between zero and one quantum narrower than the space available, and
  that margin has to absorb a scrollbar appearing mid-gesture (15px on Windows,
  17px on a desktop GTK theme), sub-pixel rects from a fractional device pixel
  ratio, and the twip rounding. 8px does not cover a scrollbar; 16px does. It
  costs at most 16px of text, which is under two characters at 11pt.

**The debounce is 150ms, trailing, with no leading call** (`REFLOW_DEBOUNCE_MS`):
under the ~200ms at which an interface stops feeling attached to the gesture, and
an order of magnitude above the frame budget, so a burst of `resize` events
collapses into one pass. No leading call, because the first event of a drag is
the least likely to be the width the reader wants — acting on it guarantees two
O(document) passes for one gesture.

**The ORDER of the two is the design, and quantisation is the half that matters.**
Quantising first is what makes the common case cost nothing at all: a resize
inside the bucket the engine already holds is one division and one comparison and
no document work, which is the `docs/107` §4 guarantee. The debounce only bounds
the worst case, a drag that crosses many buckets. `tests/reflow_view.test.mjs`
asserts both against a fake clock — sixty events inside one bucket schedule
nothing at all, and a 390→1280px drag is one pass at the width the reader stopped
on.

**A drag out and back costs nothing**, and that was a real defect found by writing
the test rather than by reasoning: the first version left a pass scheduled by an
earlier width standing when the reader came back to the width already in effect,
so the document relaid out to a width the window no longer was. The last width
wins, including when the last width is the current one.

**At a large enough zoom the column stops shrinking** rather than the view being
refused. `LayoutView::reflow` refuses a column under an inch at the seam, and at
400% a 390px window is under that — so the measure clamps to the engine's floor,
and the tile is then wider than the window. That is the same arbitration §6.3
gives a table too wide to fit, and honest for the same reason. The
no-horizontal-scroll guarantee is stated at the zoom a phone actually opens at,
which `FIT_ON_OPEN_FLOOR` pins at 100%.

### 6.3 Wide tables keep their own scroller

Google's answer (§2.2), and this shell's own existing arbitration: the overflow is
contained to the element that genuinely cannot fit rather than handed to the page.
A table whose minimum width exceeds the reflow width gets a horizontal scroller of
its own. **This is the one place a horizontal scroll survives, and it is a
different thing from the one being retired:** the reader can see that a *table* is
wider than the screen, which is true, rather than that the *document* is, which
is a lie about the document.

### 6.4 What the chrome does differently

**Shipped, with one correction and one finding.**

- The **ruler** is withheld — there are no page margins to drag — and it carries
  its own sentence rather than merely disappearing: `createRuler` takes a
  `withheldReason` and writes it to `data-withheld` on the strip, so "withheld"
  is distinguishable from "no document open", which is the same hidden strip for
  a completely different reason. This is the second reason to give tab stops a
  command home, beside the phone one, and the sentence says where they went.
- The **Pages panel** thumbnails are tiles, not pages; it is withheld rather than
  shown lying. Entering reflow closes it if it is open, disables the rail tile
  and puts the reason on its tooltip, and `view.pages` is disabled with the same
  reason in the palette and the View menu. **Never a dead control.**
- **CORRECTED: two withholdings need two sentences.** The first implementation
  gave the ruler and the Pages panel one shared string. A reader who reached for
  the ruler was then told about a panel they were not reaching for. Each control
  now says what IT is and why reflow has nothing for it; the e2e guard asserts
  the ruler's sentence is the ruler's.
- **Page setup** and **header/footer settings** still edit the document and still
  work; they simply have no visible effect until reflow is off. **Still open** —
  they do not yet say so. Recorded in §8 rather than claimed.
- **Print forces `Paged`**, unconditionally and regardless of the view
  preference, in `print.mjs`'s `withPagedLayout` — beside the two printers rather
  than in the shell, because `printDocument` already has two entry points and a
  rule enforced next to the thing it is a rule about cannot be forgotten by the
  next caller. The restore is in a `finally`, so a printer error cannot strand a
  phone reader on paper they did not ask for.
- **FINDING: PDF export was already `Paged` by construction**, and this section
  asked for something that did not need doing. `export_as_inner`
  (`casual-doc-wasm`) takes `&self.document` and never touches `self.layout` or
  `self.layout_view`: the writer re-paginates from the document's own sections.
  So "PDF export forces `Paged`" is true, and is true because reflow never
  reaches the export path at all — which is ADR-046 §3.1 holding, not a shell
  guard. Verified rather than assumed, and left unwrapped.
- `--page-width` still drives the review gutter and still works, because a tile
  has a width.

### 6.5 The honest bit about page numbers

In reflow a `PAGE` field resolves to a tile index and the status bar's "Page 3 of
12" counts tiles. Both are **wrong as page numbers** and must not be printed as
though they were right. **Where behaviour deliberately differs from Word, say so
in the code** (`SKILL.md` §8) — this paragraph is that statement, and
`status_counts.mjs`'s `pageIndicator` cites it.

**The `PAGE` field half is the engine's and shipped with it**: the driver refuses
to print a tile index as a page number. **The status-bar half is the shell's and
shipped here**: `pageIndicator` takes the reader's position instead of a page
count, and renders `status.readingPosition` — a proportion, which the tile index
CAN answer truthfully.

**CORRECTED, and the correction is the interesting part.** The first
implementation computed the proportion as `tile / tiles`. A document that reflows
to less than one 11in tile has exactly ONE — the `rich` fixture is 232px tall at
390px — so a reader at the very top of it was told "100% through". A position
that is wrong in the obvious case is not an improvement on a page number that is
wrong in the subtle one. `readerPosition` now computes it from the BAND, which
already carries every tile's top and height in document space, so the honest
answer costs one array lookup and is O(1). The same document now reads "7%".

We differ from Word here deliberately: Word shows a count in its equivalent view,
and a count invites being read as a page count.

## 7. How the exemption retired

**Done, 2026-10-01.** Both tripwires in
`webapp/tests/e2e/phone-no-horizontal-scroll.spec.mjs` fired and have been
deleted, which is what a tripwire is for.

The first was the OUTCOME one — *the document surface is still the only
exemption, and still needs to be* — asserting `#viewport`'s
`scrollWidth > clientWidth` at 390px. The second was added by the lane that wrote
this document, because the outcome one fires too late to be useful to the engine
lane: it asserted the engine seam was still ABSENT
(`typeof doc.setLayoutView !== "function"`), so the shell work in §6 could not be
forgotten behind a landed engine API. That is `SKILL.md` §9.4's "built and
unreachable" caught by construction; it went red the day `setLayoutView` landed,
and the engine branch correctly could not merge until §6 was done.

What was then done is exactly what this section prescribed: implement §6, delete
`DOCUMENT_SURFACE` from that spec, fold `#viewport` into the general assertion,
and strike the exception from `148` §6, `148` §8 item 3, `148` §9 row 1 and
`webapp/src/style.css`'s phone block.

**The measurement, before and after, at 390×844 with the `rich` fixture open:**

| | `#viewport` `scrollWidth` | `clientWidth` |
| --- | --- | --- |
| Before | 794 | 326 |
| After | 384 | 390 |

`webapp/tests/e2e/reflow.spec.mjs` now holds the positive claims — no horizontal
scroll at 390px, the document still editable in reflow, and Paged → Reflow →
Paged returning identical layout geometry — and each has been driven red by a
mutation recorded in the landing commit.

**Two things this section did not anticipate, recorded rather than smoothed over.**

1. Folding `#viewport` into the general sweep also pulled in the accessibility
   mirror: a 1px clipped box holding the whole document as text, in which every
   paragraph reports hundreds of pixels of `scrollWidth` inside a 1px client box.
   That is a scroll no reader can perform and no scrollbar exists for. It is
   skipped by CONTAINER now, through the `OFFSCREEN_BY_DESIGN` list the spec
   already kept for its bounding-box pass.
2. The Pages panel is withheld in reflow (§6.4), and a phone defaults to reflow —
   so on a phone the navigator arrives disabled, with its reason, and the reader
   turns Reflow off to use it. `phone-no-horizontal-scroll.spec.mjs` measures the
   panel in that configuration and excludes the document while doing so, because
   a reader who has explicitly asked for pages on a phone has explicitly asked
   for the pan. That is a narrow, stated consequence of a deliberate choice, not
   the blanket exemption coming back.

## 8. Open questions, recorded rather than hidden

1. **Anchored floats.** A drawing anchored to the *page* or to the *margin* has no
   referent in reflow. Falling back to paragraph-relative is the obvious answer and
   changes where the object sits; inlining it changes the text. Word's Web Layout
   keeps page-anchored objects and lets them overlap; Docs' Pageless moves them
   inline. **Undecided.** This is the only engine question §4 does not answer.
2. **The font scale.** ONLYOFFICE scale fonts by a reader ladder (§2.1) so that a
   phone gets larger text as well as a narrower column. We propose **not** to, in
   the first version: a reflow that also changes the type size changes two things at
   once and cannot be evaluated. A reader font-size control is a separate,
   later question.
3. **Tile height.** 11in shipped as `DEFAULT_TILE_HEIGHT`, and §4.4a **removed the
   reason to care**: because every tile is trimmed to its own content, the painted
   column is identical whatever the tile height is — `tests/reflow.rs` asserts that
   over 2in, 3in and 11in. So the choice is now purely about how many rasters a
   scroll costs, not about where the reader sees a break, and the validated range is
   2in to 33in (33in is 12,672px at a 4x device ratio, inside the ~32,767px canvas
   limit that rejected "one tall page"). A host passing `0` takes the default, so it
   is not forced to have an opinion about rasterisation. **Closed**, in the sense
   that mattered.
4. **Whether `Paged` should also accept a viewport.** If `LayoutView` exists, the
   embedding playground's 750px frame and `FIT_ON_OPEN_FLOOR`'s refusal become the
   same question. Not in scope here; named so it is not rediscovered.

Opened by the shell half, 2026-10-01:

5. **Page setup and header/footer settings do not yet say that their effect is
   invisible in reflow** (§6.4 asks them to). They still edit the document and
   still work; what is missing is the sentence. Left open rather than claimed.
6. **Entering and leaving reflow is O(document) and is not yet cancellable, and
   shows no progress.** ADR-046's consequence says a mode change goes through the
   background/progress path. What ships is a synchronous `setLayoutView` inside
   the render pass with a status line — acceptable on every fixture and on a
   phone-sized document, and honest to name as a gap for a large one. The
   quantisation and debounce mean it is not reached from a resize per frame,
   which was the acute risk; the remaining one is a very large document toggled
   deliberately.
7. **`reflowUnavailableReason` should be an engine getter.** The shell derives
   the refusal from `editingUnavailableReason`, which today is non-empty under
   exactly the same predicate. That is faithful, not a guess, and the call site
   still catches the throw — but the engine owning its own sentence is better,
   and this lane does not own `crates/**`. Reported rather than made.
8. **The engine's `approximations` list is collected and not yet surfaced.**
   `setLayoutView` returns what reflow approximates — a page-anchored drawing
   keeping its paper-relative position, a footnote at a tile bottom, a `PAGE`
   field refusing — and `reflow_chrome.mjs` holds them. They are not yet shown to
   the reader anywhere. Recorded so that "the engine reports its approximations"
   is not read as "the product does".

Opened by the competitive re-analysis, 2026-10-01 (`154` §8 is the live list):

9. **The measure is uncapped** — 241 characters at a 1440px window, against WCAG
   2.1 SC 1.4.8's 80. `154` §3.1–§3.2, §5.1. **The headline item**, and the one
   the owner raised.
10. **`setLayoutView` refuses a reachable window width** — 2,160px at 100% zoom,
    1,104px at 50% — and the shell silently reverts to paper with the engine's twip
    message. A defect in its own right, not only a symptom; `MAX_REFLOW_COLUMN`'s
    doc comment should stop describing a correct caller as having mis-converted
    units. `154` §3.3.
11. **Reading and pageless authoring want different widths**, and a binary toggle
    cannot express that. `154` §5.2 — one layout mechanism, two width policies
    (`min(available, document's own measure)` for authoring,
    `min(available, character target)` for reading), and "Reading view" as a preset.
12. **Collapsible headings do not exist anywhere**, and they are **not** part of
    reflow. `w15:collapsed` (`CT_OnOff`, Word's `w15` namespace) is not parsed,
    `webapp/src/outline_panel.mjs` is a flat list with no disclosure, and no
    `.docx` in the repository carries the element — so the loss-coverage gate has
    never had the chance to flag the drop. `154` §3.4 and §5.3.
13. **WCAG 2.1 SC 1.4.8 item 3** — there is no mechanism to un-justify a justified
    document in the reading view. New, and a reading-view concern rather than a
    fidelity one. `154` §2.5.
