# Rendering & Layout Architecture Research (Phase 1C prep)

**Status:** Research — for discussion, not yet a design
**Date:** 2026-07-26
**Audience:** opendoc maintainers, owner sign-off before the Phase 1C design
**Scope:** prior-art survey + Rust-ecosystem survey + algorithm/standards survey
for the layout, pagination, and rendering engine that follows the completed
Phase 1B semantic writer (`41-PHASE-1B-EXIT-REPORT.md`).

This document exists so we design the rendering path from proven prior art
rather than first principles. It makes **no implementation commitment**; it ends
with recommendations and open questions for the owner. The engine constraints
from `00-README.md` hold: native-first + WASM-compatible, no DOM as source of
truth, deterministic for the same document/fonts/viewport/version, Tauri +
web + headless hosts.

---

## 1. Prior art — how the incumbents build a paginated editor

Four architectures matter to us: LibreOffice Writer (open source, we can read
it), Microsoft Word (the fidelity target), OnlyOffice (the closest peer — a
canvas-rendered, WASM-capable, single-engine editor), and Chromium LayoutNG (the
best-documented modern rewrite of paginated text layout).

### 1.1 LibreOffice Writer — the `sw` frame tree

Writer separates the **document model** (a flat node array: `SwTextNode`,
`SwTableNode`, …) from the **layout** (a tree of *frames*). The frame tree is
rooted at `SwRootFrame`, whose children are `SwPageFrame`s; each page contains an
`SwBodyFrame` (and header/footer/margin frames), which contains the flowed
content frames: `SwTextFrame` (a paragraph), `SwTabFrame`/`SwRowFrame`/
`SwCellFrame` (tables), `SwSectionFrame` (columns/sections), and `SwFlyFrame`
(floating/anchored objects). A *layout frame* is simply a frame that owns lower
frames ([SwFrame reference](https://docs.libreoffice.org/sw/html/classSwFrame.html),
[SwPageFrame](https://docs.libreoffice.org/sw/html/classSwPageFrame.html),
[SwRootFrame](https://docs.libreoffice.org/sw/html/classSwRootFrame.html)).

Key properties we should borrow:

- **Model ≠ layout.** One text node can map to *several* text frames (e.g. a
  paragraph split across a page boundary). This model/layout split is the single
  most important structural decision — it is what makes pagination, multi-column,
  and repeated table headers expressible.
- **A `SwTextFrame` formats itself into lines and "portions".** Text within a
  line is a sequence of *portions* (`SwLinePortion` and subclasses: text, tab,
  field, drawing, hole). Line breaking and (increasingly) HarfBuzz shaping happen
  here.
- **Incremental reformat by invalidation.** Frames carry "please reformat"
  flags; on an edit only invalidated frames are reformatted, driven by an idle
  loop — this is why Writer stays responsive on large documents.
- **Painting via `OutputDevice`.** The same frame tree paints to screen, printer,
  or PDF through the VCL `OutputDevice` abstraction — a *backend-neutral* paint
  target. Hit-testing maps a pixel to a model position via the cursor shell.

Pitfalls to avoid: the codebase is old and tightly coupled; the node-array ↔
frame-tree cross-referencing is intricate; much global state. We want the same
*structure* (model / layout tree / neutral paint) with cleaner ownership.

### 1.2 Microsoft Word — Line Services + PTS

Word factors layout into two reusable subsystems, both of which we should treat
as a conceptual blueprint:

- **Line Services (LS)** — line-level: line breaking, shaping, justification,
  runs, math. It is callback-based (the client answers "what's the next run of
  text/its properties"). LS drives line layout in Word, PowerPoint, OneNote,
  RichEdit, WordPad, even the Win10 Calculator
  ([Murray Sargent, "LineServices"](https://devblogs.microsoft.com/math-in-office/lineservices/),
  [MSDN archive](https://learn.microsoft.com/en-us/archive/blogs/murrays/lineservices)).
- **Page/Table Services (PTS)** — page-level: flowing content into pages and
  columns, tables, figures/floats, footnotes. Microsoft's own account is that
  *"LineServices alone was not adequate… PTS is needed too"* — the page/flow
  problem is genuinely separate from the line problem
  ([patents on page/table formatting services](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7310771)).

The lesson: **two layers, line vs page.** A line engine (shape + break + justify
one paragraph into lines) and a page/flow engine (place lines/tables/floats/
footnotes into fixed pages) are distinct problems with distinct data structures.
Conflating them is the classic mistake.

### 1.3 OnlyOffice — the closest peer (canvas + one engine)

OnlyOffice is the most directly comparable system: an **HTML5-canvas-rendered**
editor whose core was written in C++ and compiled to JS/asm.js/WASM, with a
**single engine** powering the web, desktop, and mobile editors, and OOXML as the
native model ([sdkjs](https://github.com/ONLYOFFICE/sdkjs),
[architecture overview](https://deepwiki.com/ONLYOFFICE/sdkjs)). It does **not**
use the DOM for document content — it lays text out itself and paints glyphs to a
canvas, which is exactly our "no DOM as source of truth" constraint.

Its document/layout objects mirror the model: `CTable`/`CTableRow`/`CTableCell`,
each cell a `CDocumentContent` of paragraphs and nested tables
([tables & layout](https://deepwiki.com/ONLYOFFICE/sdkjs/4.2-tables-and-layout)).
The takeaways: (a) a retained model + a self-owned layout + immediate-mode canvas
paint is a proven, shippable architecture for exactly our target; (b) one engine
for all hosts is achievable and is what keeps pagination identical everywhere;
(c) canvas/immediate-mode painting (vs a retained DOM/scene graph) is the norm
for this class of editor.

### 1.4 Chromium LayoutNG — fragmentation as a first-class citizen

LayoutNG is Blink's rewrite of block+inline layout with **fragmentation,
interruptibility, and caching** designed in from the start
([RenderingNG: LayoutNG](https://developer.chrome.com/docs/chromium/layoutng),
[block fragmentation deep-dive](https://developer.chrome.com/docs/chromium/renderingng-fragmentation)).
Directly relevant design choices:

- **Immutable fragment tree, produced by layout, consumed by paint + hit-test.**
  In legacy Blink, fragmentation (splitting a box across columns/pages) was a
  messy pre-paint step; in LayoutNG the *layout* produces fragments, and paint +
  hit-testing simply *walk* them. This is the model we want: **layout emits a
  fragment/display structure; paint and hit-testing are read-only walks of it.**
- **Paragraph-level caching + shape-per-paragraph.** Inline layout caches at the
  paragraph level and shapes across element/word boundaries so font features
  apply correctly; bidi uses ICU. Relayout reuses cached paragraph shaping.
- **Constraint-space input → fragment output.** Each layout call takes a
  constraint space (available inline size, fragmentation context) and returns an
  immutable fragment; this functional shape makes caching and re-fragmentation
  tractable.

---

## 2. Layout algorithms & standards (the correctness spec)

### 2.1 Inline / line model
A paragraph lays out as a stack of **line boxes**; each line is a sequence of
positioned glyph runs (plus tabs, inline objects). This is the CSS inline
formatting model and Word's paragraph→line→portion model — the same shape LS and
LayoutNG use. Store, per line: the glyph runs with x-advances, baseline, ascent/
descent, and the model range it covers (needed for hit-testing, §5).

### 2.2 Line breaking — UAX #14, greedy vs optimal
Break *opportunities* come from the Unicode line-breaking algorithm
([UAX #14](https://www.unicode.org/reports/tr14/); Rust: `unicode-linebreak`).
Two break *strategies*:

- **Greedy / first-fit** — fill each line, break, move on. What browsers and Word
  use. O(n), predictable, and — critically for us — **it is what reproduces
  Word's line breaks.**
- **Optimal / Knuth-Plass** — dynamic-programming minimization of total "badness"
  across the whole paragraph; what TeX and InDesign use for superior rivers/
  justification ([Knuth-Plass](https://en.wikipedia.org/wiki/Knuth%E2%80%93Plass_line-breaking_algorithm);
  Rust prior art: [knuth-plass-wrap](https://github.com/currentspace/knuth-plass-wrap)).

**Recommendation:** greedy first, because matching Word/LibreOffice line breaks is
the fidelity bar; Knuth-Plass is a later, optional "high-quality justification"
mode. Hyphenation via Liang's algorithm / TeX patterns (`hyphenation`/`hypher`
crates) plugs into either.

### 2.3 Bidi, segmentation, shaping
Bidirectional reordering is [UAX #9](https://www.unicode.org/reports/tr9/) (Rust:
`unicode-bidi`); grapheme/word/sentence boundaries are
[UAX #29](https://www.unicode.org/reports/tr29/) (`unicode-segmentation`/`icu4x`).
Order of operations (the standard pipeline): **itemize** (by script/bidi/font) →
**shape** each run (HarfBuzz) → **break** into lines → **reorder** visually per
line. Shape-then-break (not break-then-shape) is required for correct kerning/
ligatures across break points.

### 2.4 Justification & tab stops
Full justification distributes slack as inter-word (and, for CJK/Arabic, inter-
character / kashida) expansion on all but the last line. DOCX **tab stops**
(left/center/right/decimal/bar + dot/underscore leaders) must be resolved *during*
line layout against the paragraph's tab-stop list and the section's default tab
width — a DOCX-specific must-have that generic text engines do not provide, so we
will implement tab resolution ourselves on top of whatever line engine we pick.

### 2.5 Fragmentation / break control — CSS Break-3 as our model
The [CSS Fragmentation Module Level 3](https://www.w3.org/TR/css-break-3/) is the
cleanest formal model of page breaking and maps directly onto DOCX semantics:

- **Forced breaks** ↔ `w:br w:type="page"`, `w:pageBreakBefore`, section breaks.
- **`break-inside: avoid`** ↔ `w:keepLines` (keep paragraph together),
  `w:cantSplit` (table row).
- **keep-with-next** ↔ `w:keepNext`.
- **`orphans`/`widows`** ↔ Word's widow/orphan control: the min line boxes that
  must remain before/after a break.
- **Class A/B/C break points** — the spec's rule that a break between line boxes
  (class B) is allowed only if orphan/widow counts are satisfied *and* no ancestor
  says `break-inside: avoid` ([Break-3 §breaking-rules](https://www.w3.org/TR/css-break-3/),
  [MDN](https://developer.mozilla.org/en-US/docs/Web/CSS/CSS_fragmentation)).

Adopting this model gives us a rigorous, testable specification for pagination
behavior instead of ad-hoc rules.

### 2.6 Units & coordinates
DOCX is in **twips** (1/1440 in) and **EMUs** (1/914400 in). Layout should compute
in a resolution-independent unit (twips or f32 logical px at 96 dpi) and apply the
device scale (DPI / zoom / HiDPI) only at paint time, with subpixel glyph
positioning. Keeping layout device-independent is what makes the same document
paginate identically on every host (our determinism constraint).

---

## 3. Pagination & incremental re-pagination (the hardest part)

### 3.1 The flow model
Treat laid-out content as a continuous **galley** of blocks (paragraphs → their
lines; tables → their rows) that a **paginator** slices into fixed-height page
content areas. The content area per page comes from the active section's page
size/margins (which can change mid-document at a section break), minus header/
footer and footnote reservations. This is Word's PTS role and LibreOffice's
page-frame splitting.

### 3.2 Break kinds (all resolved by the paginator)
- **Explicit:** `w:br type=page` and `w:pageBreakBefore` force the next block to a
  new page; section breaks (`nextPage`/`evenPage`/`oddPage`) additionally change
  page geometry and header/footer set; `continuous` starts a section without a
  page break.
- **Implicit:** a block that does not fit the remaining content height breaks to
  the next page (a table row or paragraph may split at a line/row boundary unless
  `cantSplit`/`keepLines`).
- **Break control:** `keepNext`/`keepLines`/widow-orphan can *push* a block (or its
  tail) forward, following the CSS Break-3 class-A/B/C rules (§2.5).

### 3.3 Hard cases
- **Table row splitting** across a page, with `w:tblHeader` header rows repeated
  at the top of each continuation, and `w:cantSplit` rows moved whole.
- **Footnotes** — placed at the bottom of the page that references them, which
  *shrinks* that page's content area and can push the referencing line (and thus
  the footnote) to the next page: a genuine circular dependency requiring a
  fixed-point/iterative solve, bounded to avoid oscillation.
- **Anchored/floating objects** with text wrap, and unbreakable content taller
  than a page (must overflow, not loop).

### 3.4 Incremental re-pagination — the performance core
Naive full re-pagination is O(document) per keystroke and unacceptable on long
documents. The proven approach (LibreOffice's invalidate-and-idle-reformat and
LayoutNG's cached fragments) is:

1. **Dirty tracking at block granularity.** An edit marks only the touched
   paragraph(s)/row(s) dirty. Unchanged blocks keep their cached line layout.
2. **Re-flow from the first dirty block, forward.** Re-lay-out changed blocks,
   then re-run the paginator from the page where the change starts.
3. **Stabilization halt (the key trick).** Continue re-paginating forward only
   while page boundaries *shift*; the moment a page's start position and content
   re-land identically to the cached pagination (a fixed point), **stop** — the
   tail of the document is unchanged. Most edits touch one page and halt within a
   page or two; only edits that change total height near a boundary ripple far.
4. **Virtualized / lazy pagination for scroll.** Fully paginate + render only the
   visible pages plus a small lookahead; estimate total page count from average
   block height and refine as the user scrolls. Trades an exact up-front page
   count for near-constant per-edit cost.
5. **Determinism.** Pagination must be a pure function of (model, fonts, section
   geometry, engine version) so reload and co-editing reproduce identical breaks
   — required by our determinism constraint and by future collaboration.

### 3.5 Data structures for cheap re-pagination + hit-testing
Per page: start model-position, content height used, the list of block fragments
placed, footnote assignments, and the active section/header-footer set. Per line
(cached on the block): glyph runs with x-advances, baseline, and covered model
range. These make both "which page/line is this pixel in" and "where does this
model position paint" O(log n)/O(1) lookups.

---

## 4. The Rust ecosystem — what we build on

Bias: pure-Rust, permissive license (we are Apache-2.0), actively maintained,
WASM-capable. Two mature families exist, both usable and both **not** complete
document engines (neither does pagination, DOCX tab stops, tables, or floats — we
build that layer regardless).

### 4.1 Shaping + fonts (do NOT reinvent)
- **HarfBuzz shaping:** `rustybuzz` (pure-Rust HarfBuzz port) and its successor
  **`harfrust`** — the canonical Rust shapers.
- **`skrifa`** (Google Fonts) — glyph outlines + metrics; **`ttf-parser`** —
  low-level font parsing.
- **`fontique`** (Linebender) — font enumeration + fallback across platforms;
  **`fontdb`** — in-memory font database. System-font access differs per OS and is
  limited in the browser (we may bundle/subset fonts for WASM).
- **`swash`** — shaping + scaling + color glyphs in one crate (used by cosmic-text).

### 4.2 Rich-text line layout (the "line engine" candidates)
- **`parley`** (Linebender) — rich-text layout over `fontique` + `harfrust` +
  `skrifa` + `icu4x`; includes selection/editing utilities. The Linebender stack
  is explicitly aimed at this problem
  ([parley](https://github.com/linebender/parley)).
- **`cosmic-text`** (System76) — shaping + layout + editing in one abstraction,
  custom safe-Rust layout with bidi, shaping via HarfRust; battle-tested in the
  COSMIC desktop ([cosmic-text](https://github.com/pop-os/cosmic-text)).

Both give us *paragraph → lines* (shape, bidi, break, selection). Neither gives
pagination, DOCX tab stops, tables, or floats. **Recommendation:** adopt one as
the line engine and build the block/page/table/float layer (our "PTS") on top.
Lean toward **parley** for its clean layering over the same font/shape crates we
would otherwise wire ourselves, with cosmic-text as the fallback if parley's
editing/rich-run model proves too limiting; a thin internal trait over "shape one
paragraph into lines" lets us swap without disturbing the paginator.

### 4.3 Rasterization / paint backend
- **`vello`** (Linebender) — GPU-compute 2D renderer over `wgpu`; excellent
  performance, but **the web is not yet a primary target** and WebGPU is still
  experimental in some browsers ([vello](https://github.com/linebender/vello)).
- **`tiny-skia`** (Linebender) — CPU Skia-subset; rock-solid and WASM-friendly,
  but the slowest option in WASM benchmarks
  ([2D web rendering in Rust](https://medium.com/lagierandlagier/2d-web-rendering-with-rust-4401cf133f31)).
- Others: `femtovg` (GL canvas), `skia-safe` (C++ Skia bindings — heavier),
  `glyphon` (glyph atlas for `wgpu`), `zeno` (path rasterization).

The backend must be swappable (see §5.2). **Recommendation:** target a
backend-neutral display list; start with a CPU rasterizer (`tiny-skia`) for
correctness and universal WASM support, and add a `vello`/`wgpu` GPU backend for
native (and web once WebGPU stabilizes) behind the same display-list interface.

### 4.4 Unicode building blocks
`unicode-linebreak` (UAX #14), `unicode-bidi` (UAX #9), `unicode-segmentation`
(UAX #29), `icu4x` (segmentation/collation) — all pure-Rust and WASM-safe. Parley
already pulls `icu4x`.

---

## 5. Rendering architecture — display list, backends, hit-testing

### 5.1 The layering (proposed)
```
Model (v1 Document)               ← Phase 1A/1B (done)
   │  layout (dirty-tracked)
Layout tree / galley              ← blocks → lines (line engine) + tables/floats
   │  paginate (dirty-tracked, stabilization halt)
Page fragments                    ← immutable, produced by layout (LayoutNG-style)
   │  build display list
Backend-neutral display list      ← ordered paint items: glyph runs, rects,
   │  execute                        borders, images, clips (WebRender-style)
Pluggable backend                 ← tiny-skia (CPU) | vello/wgpu (GPU) | canvas
```
Layout *produces* immutable page fragments; **paint and hit-testing are read-only
walks** of them (the LayoutNG discipline). The display list is an abstract,
serializable command stream (glyph run + transform + color, filled/stroked rect,
image blit, push/pop clip) — no backend types leak into it.

### 5.2 Backend-neutral across native + WASM + Tauri
The binding constraint: **Tauri renders in a system webview**, which by default
has no direct GPU surface. Three ways to put pixels on screen, all fed by the same
display list:
- **(a) HTML5 canvas from WASM** — the display-list executor calls
  `CanvasRenderingContext2D` (fillText/fillRect/drawImage). Universal, simplest
  for web/Tauri-webview, matches OnlyOffice.
- **(b) `wgpu`/`vello` surface** — a GPU canvas embedded in a native window (or a
  Tauri sidecar/child window); best performance on native.
- **(c) CPU raster (`tiny-skia`) → buffer** blitted to a `<canvas>` (`ImageData`)
  or a native window (`softbuffer`). Backend-of-last-resort, always works.

**Recommendation:** define the display list first; ship backend (a) canvas + (c)
CPU raster early (covers web, Tauri-webview, and headless/print), then add (b)
GPU for native performance. One layout engine, three executors.

### 5.3 Incremental relayout & invalidation
Mirror LibreOffice/LayoutNG: an edit produces a damage set at block granularity;
re-lay-out only dirty blocks (cached line layout for the rest), re-paginate from
the first affected page with the stabilization halt (§3.4), and repaint only the
damaged page rectangles. Cache aggressively at the paragraph level (shaping +
lines) as LayoutNG does.

### 5.4 Hit-testing & selection
With per-line glyph-run x-advances and covered model ranges (§3.5): pixel →
(page, line, glyph cluster) → model position for caret/click/drag; model position
→ caret rectangle for rendering the caret; selection highlight = the union of
per-line rectangles over the selected range. These are the same structures the
paginator already stores, so hit-testing adds no new layout cost.

---

## 6. Recommendations (for discussion, not yet committed)

**Architecture** — adopt the four-layer split proven by Word (LS/PTS),
LibreOffice (frame tree), and LayoutNG (fragment tree):
1. **Line engine** — shape + bidi + break + justify one paragraph into lines
   (reuse `parley`/`cosmic-text` + HarfBuzz stack; add our own tab-stop resolver).
2. **Block/flow engine ("our PTS")** — paragraphs/tables/floats/footnotes into a
   galley; *we build this*, it is the DOCX-specific core.
3. **Paginator** — slice the galley into pages per CSS-Break-3 rules mapped from
   DOCX; dirty-tracked with a stabilization halt; virtualized for scroll.
4. **Backend-neutral display list + pluggable backend** — canvas + CPU raster
   first, GPU (`vello`/`wgpu`) later.

**Correctness bar** — match Word/LibreOffice **line breaks and page breaks** on
the round-trip corpus (greedy line breaking, CSS-Break-3 page breaking). Optimal
(Knuth-Plass) justification is an optional later mode.

**MVP scope (proposal)** — single-section, LTR, greedy line breaking, paragraphs
+ direct runs + basic tab stops, one CPU/canvas backend, full pagination of the
visible viewport (no incremental yet), caret + click hit-testing. Then, in order:
incremental dirty relayout + stabilization-halt pagination; tables; bidi/complex
scripts; floats/footnotes; GPU backend; Knuth-Plass.

**Determinism** — layout computes in device-independent units; pagination is a
pure function of (model, fonts, section geometry, version); device scale applied
only at paint. Fonts must be pinned/bundled for reproducibility (ties into
`40-FONT-MANAGEMENT-DESIGN.md`).

---

## 7. Open questions for the owner

1. **Line engine:** adopt `parley` (Linebender stack) vs `cosmic-text` vs a thin
   swap-trait over both? (Recommend: swap-trait, default `parley`.)
2. **First backend:** canvas-from-WASM vs CPU raster (`tiny-skia`) vs both? Do we
   want a GPU (`vello`) native backend in the first design or as a later slice?
3. **Tauri rendering strategy:** paint into the webview canvas, or run a native
   `wgpu` child surface? (Affects the backend abstraction.)
4. **Fidelity bar for MVP:** "opens and reads correctly" vs "page breaks match
   Word within N%"? The latter is much harder and drives the footnote/keep/widow
   work earlier.
5. **Fonts on the web:** bundle/subset a core font set for WASM determinism, or
   rely on system/webfonts (non-deterministic)?
6. **Phase numbering:** confirm this becomes **Phase 1C** (typography/layout),
   with pagination 1D and renderer/hit-testing 1E per the current roadmap, or a
   consolidated "rendering" phase.

## Sources

- LibreOffice `sw`: [SwFrame](https://docs.libreoffice.org/sw/html/classSwFrame.html),
  [SwPageFrame](https://docs.libreoffice.org/sw/html/classSwPageFrame.html),
  [SwRootFrame](https://docs.libreoffice.org/sw/html/classSwRootFrame.html),
  [SwTextFrame](https://docs.libreoffice.org/sw/html/classSwTextFrame.html).
- Microsoft: [LineServices (Math in Office)](https://devblogs.microsoft.com/math-in-office/lineservices/),
  [LineServices (MSDN archive)](https://learn.microsoft.com/en-us/archive/blogs/murrays/lineservices),
  [PTS patent](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7310771).
- OnlyOffice: [sdkjs](https://github.com/ONLYOFFICE/sdkjs),
  [architecture](https://deepwiki.com/ONLYOFFICE/sdkjs),
  [tables & layout](https://deepwiki.com/ONLYOFFICE/sdkjs/4.2-tables-and-layout).
- Chromium: [LayoutNG](https://developer.chrome.com/docs/chromium/layoutng),
  [block fragmentation](https://developer.chrome.com/docs/chromium/renderingng-fragmentation),
  [LayoutNG blog](https://developer.chrome.com/blog/layoutNg-2).
- Standards: [CSS Fragmentation 3](https://www.w3.org/TR/css-break-3/),
  [UAX #14 line breaking](https://www.unicode.org/reports/tr14/),
  [UAX #9 bidi](https://www.unicode.org/reports/tr9/),
  [UAX #29 segmentation](https://www.unicode.org/reports/tr29/),
  [Knuth-Plass](https://en.wikipedia.org/wiki/Knuth%E2%80%93Plass_line-breaking_algorithm).
- Rust ecosystem: [parley](https://github.com/linebender/parley),
  [cosmic-text](https://github.com/pop-os/cosmic-text),
  [vello](https://github.com/linebender/vello),
  [tiny-skia (via 2D-web-rendering survey)](https://medium.com/lagierandlagier/2d-web-rendering-with-rust-4401cf133f31),
  [knuth-plass-wrap](https://github.com/currentspace/knuth-plass-wrap).
