# 151 — Reflow (pageless) layout: what the engine owes, and what the shell does with it

**Status:** Design accepted; **not implemented**. The engine half is specified here
and is entirely inside `crates/**`; the shell half is specified here and waits on it.
**Opened:** 2026-09-30. **Decision:** [ADR-046](08-ADR-REGISTER.md).
**Advances:** `148` §6 and §9 item 1, `105` UX-019, `01-ORD.md` §137
("continuous layout as a host option").
**Depends on:** nothing. Blocks: the retirement of `#viewport`'s exemption from the
no-horizontal-scroll rule (`148` §6).

> **Read this first.** Nothing described in §4–§8 is built. This document exists
> because the phone lane established *precisely* what the engine needs and could not
> build it in its own lane, and because a boundary that is only described in a chat
> message is a boundary nobody can act on next week. Every claim about existing code
> below carries a file and a symbol so it can be checked rather than believed.

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

### 4.5 The engine work item list

| # | File | Change |
| --- | --- | --- |
| 1 | `casual-doc-layout/src/document_layout.rs` | `LayoutView` (§4.3); `reflow_page_config()` beside `section_page_config` (148) — the section's vertical metrics, `page_size.width = content_width + 2·gutter`, `margin_start/end = gutter`, `header_height = footer_height = 0`; thread the view through `build_section_plans` (266, esp. the `content_width` at 284), `push_section_run` (550) and `build_body_galley` (615); new `paginate_document_in` beside `paginate_document_view` (686); reflow arms in `finish_pagination_pass` (1087) |
| 2 | `casual-doc-layout/src/columns.rs` | force `ColumnLayout::single` (97) under reflow; `section_starts_new_page` (256) and `section_start_parity` (288) return "no break" |
| 3 | `casual-doc-layout/src/paginate.rs` | `Paginator` (807) suspends `page_break_before`, `keep_next`, `keep_lines` and `MIN_WIDOW_ORPHAN`; `resolve_fields` (1655) / `page_number_labels` (1721) resolve `PAGE`/`NUMPAGES` against tiles and **flag it**, so §6.5 can be honest |
| 4 | `casual-doc-layout/src/running.rs`, `page_border.rs`, `line_number.rs`, `watermark.rs` | suppressed under reflow |
| 5 | `casual-doc-layout/src/anchor.rs` | page- and margin-relative float frames have no referent; fall back to paragraph/column-relative, or inline. **This is the only genuinely open design question in the engine half** — see §8 |
| 6 | `casual-doc-layout/src/flow.rs` | **untouched**, except §6.3's wide-table decision if it is taken in the engine rather than the shell |
| 7 | `casual-doc-layout/src/windowed.rs`, `measure.rs` | a reflow variant, or a `NotWindowable` refusal. A refusal means a large document loses reflow, which on a phone is the wrong way round — this should be a variant |
| 8 | `casual-doc-wasm/src/lib.rs` | `setLayoutView(kind, contentWidthTwip, tileHeightTwip)` + a `layoutView` getter; a field on `WasmDocument` (451); honoured by the private `repaginate` (12766) and by `finish_edit` (~13391); reflow arms in `page_size_inner` (12680), `page_ruler_geometry_inner` (8155) and `set_show_changes_inner` (12670). **Must issue no `Operation`, bump no revision, and never reach the export path** (§3.1) |
| 9 | tests | a new `casual-doc-layout/tests/reflow.rs`; the existing `geometry_snapshot.golden` must **not** move (`LayoutView::Paged` is the default and the output is byte-identical — that is the guard that the parameter is inert when unused) |

### 4.6 The guard the engine must ship with

`SKILL.md` §4: it has to be possible to drive it red.

1. **Inertness.** `paginate_document(d, s)` and
   `paginate_document_in(d, s, Editing, LayoutView::Paged)` produce identical
   `PaginatedLayout`s on the whole fixture corpus. Mutate `reflow_page_config` to be
   reached unconditionally and this goes red immediately.
2. **The width is honoured.** Reflow at width *W* produces no line box wider than
   *W*. Mutate the driver to keep using `section_page_config` and it goes red.
3. **Editability.** A caret placed by `hitTest` in reflow, given a character, lands
   at the same model position it would have landed at in `Paged`. This is the guard
   that stops (B) from quietly becoming a read-only view.
4. **No mutation.** After a reflow pass, the document's serialised bytes and its
   revision are unchanged. This is §3.1 made into a test rather than a promise.

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
the **engine seam is still absent** (`typeof doc.setLayoutView !== "function"`). The
moment §4.8 ships, that assertion goes red and names this document — so the shell
work in §6 cannot be forgotten behind a landed engine API, which is exactly the
"built and unreachable" failure `SKILL.md` §9.4 calls the most expensive recurring
pattern here.

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
3. **Tile height.** 11in is proposed (§4.4) with no source beyond "large relative to
   a viewport, small relative to a canvas limit". Worth measuring rather than
   settling by taste.
4. **Whether `Paged` should also accept a viewport.** If `LayoutView` exists, the
   embedding playground's 750px frame and `FIT_ON_OPEN_FLOOR`'s refusal become the
   same question. Not in scope here; named so it is not rediscovered.
