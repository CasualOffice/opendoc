# Phase 1C–1E — Layout, Pagination & Rendering Engine Design

**Status:** Proposed — for owner sign-off before implementation
**Date:** 2026-07-26
**Depends on:** `42-RENDERING-ARCHITECTURE-RESEARCH.md` (prior art + ecosystem),
`40-FONT-MANAGEMENT-DESIGN.md` (font resolution), `41-PHASE-1B-EXIT-REPORT.md`
(the model this consumes)
**Covers:** Phase 1C (typography + line/block layout), 1D (pagination), 1E
(renderer + hit-testing) as one cohesive engine, implemented in slices.

This design turns the research recommendations into a concrete architecture,
crate layout, data structures, interfaces, delivery order, and acceptance gates.
It proposes answers to the seven open questions from doc 42; each is marked
**[Proposed]** and may be overridden by the owner before implementation begins.
Per the working contract, **no rendering code lands until this is accepted.**

**This is a production, Word-grade layout/rendering engine — not a prototype or
MVP.** The full scope is the deliverable: correct typography (bidi, complex
scripts, RTL, justification, tab stops), tables across pages (row splitting,
header repetition), footnotes, floating/anchored objects with wrap, section
geometry changes, incremental re-pagination that stays responsive on large
documents, and native + WASM + GPU rendering with interactive caret/selection.
Nothing on that list is optional. The phasing below is **delivery order**, and
**every slice ships at production quality for its scope** — no slice is a
throwaway. Production-grade quality is the baseline, not a later milestone.

---

## 1. Decision summary (resolving doc 42's open questions)

| # | Question | **[Proposed] decision** | Rationale |
|---|---|---|---|
| 1 | Line engine | Wrap a `LineShaper` trait; default impl over **`parley`** | Reuse the Linebender shape/font/bidi stack; the trait lets us swap to `cosmic-text` without touching the paginator. |
| 2 | First backend | **CPU raster (`tiny-skia`)** + **HTML5 canvas (WASM)**, both behind one display list; **`vello`/`wgpu`** GPU backend as a later 1E slice | CPU + canvas cover web, Tauri-webview, headless, and print on day one and are universally WASM-safe; GPU is a performance add-on, not a prerequisite. |
| 3 | Tauri rendering | Paint into the **webview canvas** (WASM executes the display list on a `<canvas>`); native `wgpu` child-surface deferred to the GPU slice | Matches OnlyOffice; avoids a custom window early; one code path for web + Tauri. |
| 4 | Fidelity bar | **Word/LibreOffice line- and page-break parity** is the accepted end state; delivery ratchets a *measured* differential gate from correct-and-readable up to break-parity | The product target is Word-grade fidelity, not "good enough". Ratcheting is the *delivery mechanism* (each slice raises the measured bar via the M-002 differential harness), not a lowered goal. |
| 5 | Web fonts | **Bundle + subset a deterministic core font set** for WASM; system fonts used opportunistically on native | Determinism constraint (same document/fonts → same layout) requires known fonts; ties into doc 40. |
| 6 | Phase numbering | **1C** = typography + line/block layout; **1D** = pagination; **1E** = renderer + hit-testing (per current roadmap) | Keeps the accepted roadmap; this doc spans all three but implementation is sliced along them. |
| 7 | (new) Threading | Layout **synchronous on the caller thread** initially; a worker/off-main path added when the frame budget demands it | WASM threading is constrained; correctness first, then move layout off the UI thread. Deferred in *time*, not scope. |

---

## 2. Workspace additions

Two new crates, following the existing `casual-doc-*` convention and the engine
boundary in `00-README.md` (engine owns layout/pagination/hit-testing; host owns
windows/UI):

- **`casual-doc-layout`** — the layout + pagination engine. Input: a `v1::Document`
  + a resolved font set + a viewport/page config. Output: an immutable
  **paginated layout** (page fragments) and a **display list** per page. Owns the
  line engine integration, the block/flow engine, the paginator, incremental
  relayout, and hit-testing queries. No pixels, no backend types.
- **`casual-doc-render`** — the display-list **backends**. Consumes a display list
  and paints it: `tiny-skia` (CPU → buffer), a canvas backend (WASM →
  `CanvasRenderingContext2D`), and later `vello`/`wgpu` (GPU). No layout logic.

`casual-doc-sdk` gains a thin facade (`layout()`, `render_page()`, `hit_test()`,
`caret_rect()`) so hosts never touch the engine internals directly. Font
resolution lives in a `casual-doc-fonts` crate per doc 40 (or a module of
`casual-doc-layout` in the first slice, promoted to its own crate as it grows).

---

## 3. Architecture — the four layers

```
 v1::Document  (Phase 1A/1B — done)
      │  resolve fonts (doc 40)               casual-doc-fonts
      ▼
 ┌─────────────────────────────────────────────┐
 │ casual-doc-layout                            │
 │                                              │
 │  ① LineShaper  (parley)  shape+bidi+break    │  one paragraph → lines
 │        + TabResolver (ours, DOCX tab stops)  │
 │  ② Block/Flow engine  ("our PTS")            │  blocks → a galley of
 │        paragraphs, tables, floats, footnotes │  fragments (device-indep units)
 │  ③ Paginator  (CSS-Break-3 mapped from DOCX) │  galley → immutable Page fragments
 │        incremental, stabilization-halt,      │
 │        virtualized for scroll                │
 │  ④ DisplayListBuilder                        │  Page → backend-neutral paint list
 └─────────────────────────────────────────────┘
      │  DisplayList (serializable command stream)
      ▼
 ┌─────────────────────────────────────────────┐
 │ casual-doc-render  (pluggable backend)       │
 │   tiny-skia (CPU) │ canvas (WASM) │ vello 1E │
 └─────────────────────────────────────────────┘
      │  pixels
      ▼        host window / <canvas> / print / PNG (visual-regression)
```

Discipline borrowed from LayoutNG (doc 42 §1.4): **layout produces immutable page
fragments; paint and hit-testing are read-only walks of them.** No backend type
appears above the display list; the display list is the single stable seam.

---

## 4. Core data structures (in `casual-doc-layout`)

Units: layout computes in **device-independent `Twip` (i32, 1/1440 in)**; the
device scale (DPI × zoom) is applied only when building the display list / at
paint. This keeps pagination identical across hosts (determinism constraint).

```rust
// ① line engine output (one paragraph)
struct LineLayout { lines: Vec<Line> }
struct Line {
    runs: Vec<GlyphRun>,      // positioned, visually ordered (post-bidi)
    ascent: Twip, descent: Twip, advance_height: Twip,
    range: ModelRange,        // model positions this line covers (hit-testing)
    break_after: BreakKind,   // soft | hard | end-of-paragraph
}
struct GlyphRun {
    font: FontId, size: Twip,
    glyphs: Vec<Glyph>,       // glyph id + x-advance + cluster→byte map
    color: Color, // + decorations (underline/strike), script, bidi level
}

// ② block/flow output — a galley of fragments in flow order
enum BlockFragment {
    Paragraph { id: NodeId, lines: LineLayout, box_: BoxMetrics },
    TableRow  { id: NodeId, cells: Vec<CellFragment>, can_split: bool, header: bool },
    Float     { id: NodeId, anchor: ModelPos, geometry: FloatGeometry },
    // footnotes tracked separately (page-reserved)
}

// ③ paginator output — immutable, the thing paint + hit-test walk
struct PaginatedLayout {
    pages: Vec<Page>,
    page_index: PageIndex,    // model-pos → page, and page → model-range
}
struct Page {
    number: u32,
    section: SectionId,       // geometry + header/footer set (may change mid-doc)
    content_area: Rect,       // page box minus margins/header/footer/footnotes
    placed: Vec<PlacedFragment>,   // fragment + its Rect on this page
    footnotes: Vec<PlacedFragment>,
    start_pos: ModelPos, end_pos: ModelPos,  // for incremental halt + hit-test
}

// ④ backend-neutral paint list
enum PaintItem {
    GlyphRun { font: FontId, glyphs: Vec<Glyph>, transform: Transform, color: Color },
    Rect { rect: Rect, fill: Option<Color>, stroke: Option<Stroke> },
    Image { media: MediaId, rect: Rect },
    PushClip(Rect), PopClip,
}
struct DisplayList { items: Vec<PaintItem> }   // one per page (or damage region)
```

---

## 5. Layer ① — line engine (`LineShaper` + `TabResolver`)

- **`LineShaper` trait**: `fn shape_paragraph(&self, runs: &[StyledRun], constraints: LineConstraints) -> LineLayout`. Default impl wraps **`parley`** (fontique + harfrust + skrifa + icu4x): itemize → shape → **greedy** UAX #14 break → bidi reorder per line. The trait isolates us from parley's API and lets a `cosmic-text` impl drop in.
- **Greedy breaking** [Proposed] to match Word/LibreOffice line breaks (doc 42 §2.2). Knuth-Plass is a later optional `JustificationMode::Optimal`.
- **`TabResolver`** (ours): DOCX tab stops (left/center/right/decimal/bar + leaders) resolved during line layout against the paragraph tab list + section default tab width — parley/cosmic-text do not do this, so it is engine code.
- **Caching:** `LineLayout` is cached per paragraph keyed by (content hash, resolved run props, available width). Re-edit of one paragraph re-shapes only that paragraph (LayoutNG paragraph-level cache, doc 42 §1.4).

## 6. Layer ② — block/flow engine ("our PTS")

Turns `BlockNode`s into `BlockFragment`s in a continuous **galley** (not yet
paged): paragraph box metrics (margins/spacing/border/shading from
`ParagraphProperties`), tables (fixed vs auto width per `tblLayout`; rows as
splittable units honoring `cantSplit`/`tblHeader`), floats/anchored objects with
wrap, and footnote bodies (held aside for the paginator to place). This is the
DOCX-specific core we build; no third-party engine provides it.

## 7. Layer ③ — the paginator (the hard part)

Slices the galley into `Page`s using the **CSS Fragmentation 3** model mapped from
DOCX (doc 42 §2.5, §3):

- **Forced breaks:** `w:br type=page`, `w:pageBreakBefore`, section breaks
  (nextPage/even/odd change geometry + header/footer set; continuous does not
  page-break).
- **Avoid/keep:** `keepLines` → `break-inside: avoid`; `keepNext` → keep-with-next;
  `cantSplit` → unbreakable row; widow/orphan → min lines before/after a break
  (class A/B/C rules).
- **Hard cases:** table row splitting with header-row repetition; the
  **footnote↔reference circular dependency** solved by a bounded fixed-point
  iteration (place footnote → shrink content area → re-check the reference line,
  cap iterations to avoid oscillation); content taller than a page overflows
  rather than loops.

**Incremental re-pagination** (doc 42 §3.4), the performance core:
1. Edit → damage set at block granularity (from the transaction's changed range).
2. Re-lay-out only dirty blocks (cached lines for the rest).
3. Re-paginate forward from the first affected page; **stabilization halt** — stop
   as soon as a page's `start_pos` + content re-land identically to the previous
   pagination (fixed point). Bounds work to the neighborhood of the edit.
4. **Virtualized scroll:** fully paginate + build display lists only for visible
   pages + a lookahead; estimate total page count from average block height,
   refine on scroll. (Delivered in slice 1D-4; earlier 1D slices paginate the
   whole document up front — correct and production-quality, not yet virtualized.)
5. **Determinism:** pagination is a pure function of (model, resolved fonts,
   section geometry, engine version).

## 8. Layer ④ — display list + backends (`casual-doc-render`)

`DisplayListBuilder` walks a `Page` and emits `PaintItem`s (device scale applied
here). The **`RenderBackend` trait**: `fn execute(&mut self, list: &DisplayList,
target: &mut Surface)`. Backends [Proposed order]:
1. **`tiny-skia`** (CPU → RGBA buffer) — universal, WASM-safe, also drives
   **visual-regression PNGs** and print/PDF raster.
2. **Canvas (WASM)** — executes `PaintItem`s via `CanvasRenderingContext2D`
   (`fillText` from a glyph atlas / `fillRect` / `drawImage`); the web + Tauri
   path.
3. **`vello`/`wgpu`** (GPU) — native performance, added in 1E behind the same
   trait; web GPU once WebGPU stabilizes.

## 9. Hit-testing & selection (1E)

From `PaginatedLayout` + per-line glyph advances (already stored): **pixel →
model position** (page → line → glyph cluster) for caret/click/drag; **model
position → caret `Rect`** for the caret; **selection highlight** = union of
per-line rects over the selected `ModelRange`. Reuses `casual-doc-selection`.
No new layout cost — these are walks of the immutable page fragments.

---

## 10. Public API (SDK facade)

```rust
// casual-doc-sdk
let layout = engine.layout(&document, &LayoutConfig { fonts, page, zoom });
let list   = layout.display_list(page_index);      // feed a RenderBackend
let pos    = layout.hit_test(page_index, point);   // caret placement
let rect   = layout.caret_rect(model_pos);
// after a transaction:
let layout = engine.relayout(layout, &damage);     // incremental
```
Hosts render `list` with whichever `casual-doc-render` backend suits the platform;
the engine never opens a window.

---

## 11. Slices & acceptance gates

**Phase 1C (typography + line/block layout)**
- 1C-1 Font resolution (bundled core set, production-grade per doc 40) +
  `LineShaper`/parley shaping of a styled paragraph → lines; visual-regression PNG.
- 1C-2 Run properties in layout (bold/italic/size/color/underline/strike, fonts).
- 1C-3 Paragraph properties (alignment, indentation, spacing, tab stops, borders/
  shading) + `TabResolver`.
- 1C-4 Block/flow galley for paragraphs; **exit gate:** a multi-paragraph document
  lays out and renders to a PNG that matches a LibreOffice-rendered reference
  within a tolerance (differential harness, extends M-002).

**Phase 1D (pagination)**
- 1D-1 Single-section paginator (page box, implicit overflow breaks) → multi-page
  PNG.
- 1D-2 Break control (forced breaks, keepNext/keepLines/widow-orphan) per CSS
  Break-3; **exit gate:** page-break parity with LibreOffice on the corpus within
  tolerance.
- 1D-3 Tables across pages (row split, header repeat, cantSplit); footnotes.
- 1D-4 Incremental relayout + stabilization halt + virtualized scroll; **exit
  gate:** per-keystroke relayout is O(neighborhood), not O(document), measured by
  the benchmark harness.

**Phase 1E (renderer + hit-testing)**
- 1E-1 Canvas (WASM) backend; 1E-2 hit-testing + caret + selection; 1E-3 GPU
  (`vello`/`wgpu`) backend; **exit gate:** interactive caret/selection on a
  paginated document in the Tauri webview.

Each slice: design-note → implement → gates (fmt 1.96.0 / clippy `-D warnings` /
tests / **new: visual-regression PNGs**) → adversarial review → PR.

## 12. Testing & determinism

- **Visual regression:** render fixtures to PNG with the CPU backend; store golden
  PNGs; diff with a tolerance (anti-aliasing/subpixel). Gated in CI once fonts are
  pinned (this is the "visual regression: requires renderer + fixed fonts" gate
  already noted in `15-CI-AND-RELEASE-GATES.md`).
- **Differential fidelity:** extend `tools/opendoc-fidelity` (M-002) from text to
  **line-break and page-break** comparison against `soffice` output.
- **Determinism test:** same (document, fonts, config) → identical
  `PaginatedLayout` and identical PNG across runs and platforms.
- **Property/fuzz:** layout must never panic or loop on adversarial input
  (content taller than a page, zero-width columns, pathological tables).

## 13. Risks & mitigations

- **Break-fidelity vs Word** is the deepest risk — greedy breaking + CSS-Break-3
  gets close but Word has quirks; mitigate by ratcheting the differential gate and
  keeping a tolerance rather than exact match early.
- **Footnote/keep circular pagination** can oscillate — bound the fixed-point
  iteration and log non-convergence.
- **WASM performance** of CPU rasterization — mitigate with the glyph-atlas canvas
  backend and, later, GPU.
- **Font determinism on the web** — bundle/subset; never depend on system fonts
  for layout metrics on WASM.
- **Sequencing, not scope-cutting** — the full engine (bidi/RTL/complex scripts,
  tables across pages, footnotes, floats, incremental pagination, GPU) is all in
  scope and tracked; the risk is *delivery order*, mitigated by shipping each
  slice production-complete for its scope rather than by dropping features.

## 14. Open items for owner before implementation

Confirm or override the **[Proposed]** decisions in §1 (especially: `parley` as the
default line engine, CPU+canvas as the first backends, and the "reads correctly
first, break-parity ratcheted" fidelity bar). On sign-off, the first slice is
**1C-1** (production font resolution + shape a styled paragraph → PNG), tracked as
new rows in `14-EXECUTION-TRACKER.md`.
