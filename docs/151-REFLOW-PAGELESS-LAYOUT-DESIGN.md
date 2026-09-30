# 151 — Reflow (pageless) layout: what the engine owes, and what the shell does with it

**Status:** **Engine half implemented** (`casual-doc-layout`'s `LayoutView` +
`casual-doc-wasm`'s `setLayoutView`); the shell half (§6) is specified here and
still waits on nobody. **Opened:** 2026-09-30. **Decision:**
[ADR-046](08-ADR-REGISTER.md).
**Advances:** `148` §6 and §9 item 1, `105` UX-019, `01-ORD.md` §137
("continuous layout as a host option").
**Depends on:** nothing. Blocks: the retirement of `#viewport`'s exemption from the
no-horizontal-scroll rule (`148` §6).

> **Read this first.** §4 is built; §6 is not. The engine work landed as specified
> except in **four** places, all corrected in place below and each marked
> **CORRECTED** with the evidence: §4.5 row 1's vertical metrics, §4.5 row 2's
> `columns.rs`/`paginate.rs` edits, §4.5 row 3's flag, and §4.5 row 7's windowed
> variant. A fifth thing the design did not know about at all — fixed-height tiles
> still showing a blank band at every cut — is §4.4a. Every claim about existing
> code below carries a file and a symbol so it can be checked rather than believed.
>
> **The shell half is now the whole of what is left, and it is load-bearing.**
> `webapp/tests/e2e/phone-no-horizontal-scroll.spec.mjs`'s second tripwire ("the
> reflow seam has not landed without the shell that spends it") is RED as of the
> engine landing, by design: it fires on the day `setLayoutView` exists precisely
> so a landed engine API cannot sit unreachable. Implementing §6 and deleting that
> test is what turns it green.

## 1. The problem, stated as a measurement

At 390×844 with a document open, `#viewport` reports `scrollWidth 794` against
`clientWidth 326`. The document pans 468px sideways. That is the one surface the
phone tier (`148`, ADR-044) could not bring under the owner's "no horizontal
scroll" rule, and it is named as an exception rather than hidden — in
`webapp/tests/e2e/phone-no-horizontal-scroll.spec.mjs` (`DOCUMENT_SURFACE`), in
`webapp/src/style.css`'s phone block, in `148` §6 and in ADR-044.

A Letter page's text column is 6.5in — **624 CSS px at 96dpi**. It cannot be both
390px wide and readable. There are exactly three answers and only three:

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

Their touch side for that mode is a separate, much simpler `CReaderTouchManager`
(`word/Drawing/mobileTouchManager.js:833-915`) with `SelectEnabled = false` and
`TableTrackEnabled = false` — i.e. **their reader mode is not editable.** §3 says
why ours must be.

### 2.2 Google Docs — Pageless

Answer 10296604: **⋮ → Page setup → Pageless** "removes page boundaries and reflows
content as a continuous scroll". Two properties matter for §4:

- It is a **document-level setting that persists and syncs**, not a per-viewer view
  toggle — Google made it part of the file. We deliberately do not (§3.3).
- **A wide table gets its own sideways scroll.** The overflow is contained to the
  element that genuinely cannot fit rather than handed to the page. That is exactly
  the arbitration this shell already uses for a ribbon band, and §6.3 adopts it.

### 2.3 Word

`148` §4 found no reflow toggle in Microsoft's own documentation, in either
direction, and recorded it as unverified rather than as an absence. Word for
Windows' **Web Layout** view is the nearest thing and is not documented for the web
or mobile clients. So the vote is Docs and ONLYOFFICE for, Word abstaining.

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

All of this is `webapp/` and waits on §4.8's setter. It is specified now so the
engine lane knows what shape its API is being consumed in.

### 6.1 The toggle

A command `view.reflow` — label carrying its state, the shape `view.compactRibbon`
already uses ("Reflow: on" / "Reflow: off"), filed in
`APP_MENU_SECTIONS.view`'s `menuGroup.show` band beside `view.outline` and
`view.showChanges`, plus a View-band ribbon face. Persisted in `prefs.mjs`.
Default on below `PHONE_MAX_WIDTH`, off above it (§3.4).

Naming: Google says "Pageless", ONLYOFFICE says "Reader mode", Word says "Web
Layout". Ours is **"Reflow"** because it is the only one of the three that is
accurate for a mode that is still editable and still cut into tiles — and because
"Reader mode" would promise ONLYOFFICE's read-only behaviour, which §3.2 rejects.

### 6.2 Feeding the width back

`renderAll()` (`main.js:3665`) calls `doc.setLayoutView(...)` before `pageCount` /
`pageSize` when reflow is on. The width it passes is the viewport's client width
minus the gutters, converted to twips — and it is **quantised** (proposed: to the
nearest 8px) and **debounced** so that a drag or a rotation does not run an
O(document) pass per frame (§5). `buildPageBand` already takes `options.gap`
(`page_scroll.mjs:95`), so tiles at `gap: 0` are a call-site change, not a new
mechanism.

### 6.3 Wide tables keep their own scroller

Google's answer (§2.2), and this shell's own existing arbitration: the overflow is
contained to the element that genuinely cannot fit rather than handed to the page.
A table whose minimum width exceeds the reflow width gets a horizontal scroller of
its own. **This is the one place a horizontal scroll survives, and it is a
different thing from the one being retired:** the reader can see that a *table* is
wider than the screen, which is true, rather than that the *document* is, which
is a lie about the document.

### 6.4 What the chrome does differently

- The **ruler** is meaningless (there are no page margins to drag) and is hidden.
  This is the second reason to give tab stops a command home, beside the phone one.
- The **Pages panel** thumbnails are tiles, not pages; it is withheld in reflow
  rather than shown lying.
- **Page setup** and **header/footer settings** still edit the document and still
  work; they simply have no visible effect until reflow is off. They must say so.
- **Print and PDF export force `Paged`**, unconditionally and regardless of the view
  preference. This is a correctness requirement, not a nicety: printing what is on
  screen would print tiles.
- `--page-width` (`main.js:3722`) still drives the review gutter and still works,
  because a tile has a width.

### 6.5 The honest bit about page numbers

In reflow a `PAGE` field resolves to a tile index and the status bar's "Page 3 of
12" counts tiles. Both are **wrong as page numbers** and must not be printed as
though they were right. The shell shows neither in reflow: the page counter is
replaced by the reader's position (Word and Docs both do something like this), and
a `PAGE` field renders with the paginated value carried over from the last `Paged`
layout if one is available, or as a refusal if not. **Where behaviour deliberately
differs from Word, say so in the code** (`SKILL.md` §8) — this paragraph is that
statement and the implementation must cite it.

## 7. How the exemption retires

`webapp/tests/e2e/phone-no-horizontal-scroll.spec.mjs` ends with a test whose whole
job is to fail when this lands:

> `the document surface is still the only exemption, and still needs to be`

It asserts `#viewport`'s `scrollWidth > clientWidth` at 390px. That is the *outcome*
tripwire and it fires on the day reflow is on by default at the phone rung.

A second tripwire is added by the lane that wrote this document, because the outcome
one fires too late to be useful to the engine lane: the same spec now also asserts
the **engine seam is still absent** (it reads the wasm binding's own prototype and
expects no `setLayoutView`/`setReflowWidth`/`setLayoutMode`). The moment §4.8 ships,
that assertion goes red and names this document — so the shell work in §6 cannot be
forgotten behind a landed engine API, which is exactly the "built and unreachable"
failure `SKILL.md` §9.4 calls the most expensive recurring pattern here.

**That tripwire is RED now.** `setLayoutView` exists as of the engine half landing,
and the engine lane deliberately did not silence it: the spec belongs to the shell
lane, and editing it from here would have removed the one signal that says the API
needs a consumer. The fix is §6, not a change to the test — and then deleting it,
because at that point the outcome tripwire above is the one that matters. Note also
that this spec's own prose cites `docs/149` where it means `docs/151`: two document
numbers were in flight when it was written and the reflow design landed as 151.
`docs/148` §9 and ADR-046 carried the same stale citation; ADR-046's is corrected,
and the other two belong to lanes that own those files.

When both fire, the fix is: implement §6, delete `DOCUMENT_SURFACE` from that spec,
fold `#viewport` back into the general assertion, and strike the exception from
`148` §6, ADR-044 and `webapp/src/style.css`'s phone block.

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
