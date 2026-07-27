# 57 — Phase 1G Implementation Plan (Viewer → Editor)

**Status:** Implementation plan; build-level companion to doc 56.
**Date:** 2026-07-27
**Architecture:** `docs/56-EDITOR-SHELL-AND-RENDER-ARCHITECTURE.md` (the *what/why*).
This doc is the *how*: crates, the exact `casual-doc-wasm` API, the JS↔WASM data
contract, the frontend/host structure, the build/distribution pipeline, and the
milestone-by-milestone plan with acceptance criteria and gates.

Doc 56 fixes the decision (OnlyOffice-style fat client, Rust→WASM, canvas paint,
custom interaction, self-hostable SDK). Nothing here reopens it.

## 1. Build on what already exists

The engine and — crucially — the **editing substrate** are already in the
workspace. This plan mostly *exposes* them across the WASM boundary; it does not
rebuild them.

| Capability | Where it lives today | Bridge work |
| --- | --- | --- |
| Import `.docx` → `v1::Document` | `casual-doc-import::import_package` | call it |
| Pagination (per-section geometry, running content, fields) | `casual-doc-layout::document_layout::paginate_document` → `PaginatedLayout` | call it |
| Page → paint IR | `casual-doc-layout::compose::compose_page(&Page) -> DisplayList` | call it |
| Rasterize IR → pixels | `casual-doc-render::render(&DisplayList, &mut Surface, dpi, fonts, media)` | call it; return RGBA |
| Coord ↔ model position, caret + selection geometry | `casual-doc-layout::hittest::LayoutSnapshot` (`hit_test`, `caret_rect`, `selection_rects`, `move_vertical`) | call it |
| Editing session: snapshot, selection, insert/delete/split/join, undo/redo, export, events | `casual-doc-sdk::DocumentSession` | call it |
| Closed op set + inverses + position mapping (I1/I2/I3) | `casual-doc-transaction` (`Operation`, `apply`, `PositionMap`) | already behind `DocumentSession` |
| Selection model + position mapping across edits | `casual-doc-selection` / `sdk::{Position, Range, SelectionSnapshot}` | call it |
| Fonts (bundled + host-registered) | `casual-doc-render::RegistryFontSource`, `casual-doc-layout` shaper registry | feed host fonts in |

**Implication:** P1G is a bridge + frontend + host, not a new engine. Milestones
6–7 (caret/editing) are *wiring the existing `DocumentSession` and
`LayoutSnapshot`*, not writing an edit engine.

## 2. Crate and directory layout

```
crates/
  casual-doc-wasm/         # NEW: wasm-bindgen façade over sdk + layout + render
    src/lib.rs             #   the exported surface (§4)
    src/convert.rs         #   Rust types ↔ serde/JS payloads (§5) — later milestone
    Cargo.toml             #   crate-type = ["cdylib", "rlib"]; wasm-bindgen dep
webapp/                    # NEW: browser-first surface (no framework; static)
    index.html             #   upload/drop a .docx → canvas viewer
    src/                   #   core (WASM transport), render, overlay, input, chrome
    build.sh               #   wasm-pack build --target web → webapp/pkg
    pkg/                   #   generated wasm-pack output (git-ignored)
  desktop/                 # LATER (P1G-004): Tauri shell (native host services)
    src-tauri/             #   commands: open/save file, enumerate OS fonts
packages/
  sdk/                     # LATER (built artifact): self-hostable embed bundle
```

> **Direction (decided during P1G-001).** The interactive layer is **web-first**:
> the whole viewer→editor is built and fine-tuned in the browser (`webapp/`),
> deployable as static files to **GitHub Pages** as a developer **test harness**
> (`webapp/README.md`). The engine ships **OSS-first**, ahead of any product. The
> **Tauri desktop shell is deferred** (P1G-004) — it exists only for what a browser
> tab cannot do (direct local-file open, OS-font enumeration); the browser gets
> file upload + network-fetched fonts instead. This replaces the earlier
> `app/frontend` (Vite) sketch above with a dependency-light `webapp/` — reflowed
> here rather than left to drift.

Naming follows doc 00 (`casual_doc_*`). `casual-doc-wasm` builds `cdylib` for the
web/webview target and `rlib` so a native desktop backend can link the same façade
(the doc 56 native escape hatch) without WASM.

> **P1G-001 as built.** The façade exposes `open(bytes)`, the `pageCount` getter,
> `pageSize(i)` (per-section page box), and `renderPage(i, dpi) -> PageBitmap`
> (`{ widthPx, heightPx, rgba: Uint8ClampedArray }`) — the §4.1–4.3 subset — over
> the same `import_package → paginate_document → compose_page → render` path the
> native renderer uses. Fallible engine work lives in `_inner` methods returning
> `Result<_, String>`; the `#[wasm_bindgen]` wrappers convert to a thrown JS
> `Error` at the boundary (so native `cargo test` runs the engine without a JS
> host). `webapp/` is the harness; CI is `.github/workflows/pages.yml`. Deferred:
> structured `SdkError` code/severity on errors (§5.5) and §4.4+ (text layer,
> hit-testing, editing).

## 3. Units and coordinate model (normative)

- All engine geometry is **`Twip` (1/1440 in)**. The bridge speaks Twip on the
  model side and **device px** only at the raster boundary.
- `render_page(i, dpi)` rasterizes at `dpi`; device px = `twip / 1440 * dpi`.
- `LayoutSnapshot` queries take **page-local Twip** points and return page-local
  Twip rects with a 1-based page number. The frontend converts px↔Twip using the
  same `dpi` it rendered the page at, and page-local↔viewport using the page's
  scroll offset. **One conversion module owns this; no ad-hoc math elsewhere.**

## 4. `casual-doc-wasm` API surface

One exported handle, `WasmDocument`, wrapping an open document + its current
`PaginatedLayout` + a `DocumentSession` (editor phase). All methods are
`#[wasm_bindgen]`. Errors map to a thrown JS `Error` carrying `SdkError`'s
`ErrorCode`/`ErrorSeverity` (see §5.5). Bitmaps are returned as `Uint8Array` /
`Uint8ClampedArray`; structured payloads as JSON (serde) until we adopt a binary
codec (§9).

### 4.1 Lifecycle
- `open(bytes: Uint8Array) -> WasmDocument` — import + initial pagination.
- `WasmDocument.free()` — wasm-bindgen drop (frees Rust memory).
- `repaginate()` — after a font registration or edit; recomputes `PaginatedLayout`.

### 4.2 Document info
- `page_count() -> u32`
- `page_size(i: u32) -> PageSize` — `{ width_twip, height_twip }` (per-section).
- `revision() -> u64` — current model revision (for edit request bases).

### 4.3 Render (viewer core, P1G-001/002)
- `render_page(i: u32, dpi: f32) -> PageBitmap` — `{ width_px, height_px, rgba:
  Uint8ClampedArray }`. Internally: `compose_page(page)` → `Surface` →
  `render(...)` → raw RGBA (not PNG; the frontend blits via `putImageData` /
  `createImageBitmap`). PNG encode is available behind `render_page_png` for the
  fallback `<img>` path and thumbnails.
- Background: honors `document.background()` (page fill) exactly as
  `render_gallery` does.

### 4.4 Text layer + hit-testing (copy-in-view, P1G-003)
- `text_layer(i: u32) -> TextLayer` — positioned selectable text for the overlay
  (§5.2), derived from the page's `DisplayList` `Glyphs` items.
- `hit_test(page: u32, x_twip: i32, y_twip: i32) -> HitPayload` — wraps
  `LayoutSnapshot::hit_test`; returns `{ node, offset, zone }`.
- `caret_rect(node, offset) -> CaretRect` — `{ page, x_twip, y_twip, w_twip,
  h_twip }` from `LayoutSnapshot::caret_rect`.
- `selection_rects(range) -> SelectionRects` — `Vec<{ page, rect }>` from
  `LayoutSnapshot::selection_rects` (highlight geometry).
- `copy_text(range) -> string` — plain text between two model positions (rich/HTML
  clipboard deferred to the editor phase).
- `link_at(page, x_twip, y_twip) -> Option<Hyperlink>` — `{ target, external }`
  for click-to-open (hyperlinks are first-class `InlineNode`s).

### 4.5 Editing (P1G-006/007) — thin pass-through to `DocumentSession`
- `snapshot() -> DocumentSnapshot`, `selection() -> SelectionSnapshot`,
  `set_selection(req)`.
- `insert_text(base_revision, at, text, marks) -> TransactionResult`
- `delete_range(base_revision, range) -> TransactionResult`
- `split_paragraph(base_revision, at, new_id) -> TransactionResult`
- `join_paragraphs(base_revision, first, second) -> TransactionResult`
- `undo(base_revision) / redo(base_revision) -> TransactionResult`
- `export_normalized_json() -> Uint8Array`
- `subscribe()` / `drain_events(max) -> EventBatch` — change notifications so the
  frontend knows which pages are dirty and must re-render.

`TransactionResult` carries the new revision and the **dirty page set** (which
pages changed → the only pages the frontend re-rasterizes). If the layout does not
yet surface a dirty-page set, P1G-007 adds it; until then, re-paginate + diff.

### 4.6 Fonts (host seam, P1G-004)
- `register_font(bytes: Uint8Array) -> FontFaceInfo` — push a host-provided face
  (desktop OS face bytes, or a web-fetched face) into the shaper/render registry,
  then `repaginate()`. This is the single seam from `font-provisioning-strategy`;
  bundled base faces are always present.

### 4.7 What is NOT exported
No raw model mutation outside `DocumentSession` (preserves I1). No direct
`Operation` construction from JS — the frontend calls the semantic request methods
(`insert_text`, …) which the session turns into the closed op set (I2), anchored on
`NodeId`/`ModelPos` (I3).

## 5. JS↔WASM data contract

### 5.1 PageBitmap
`{ width_px: number, height_px: number, rgba: Uint8ClampedArray }` — RGBA8888,
row-major, premultiplied per tiny-skia. Blit: `new ImageData(rgba, w, h)` →
`putImageData`, or `createImageBitmap(imageData)` for worker/OffscreenCanvas.

### 5.2 TextLayer (drives the transparent overlay)
```
TextLayer = { page: u32, runs: TextRun[] }
TextRun = {
  x_twip, y_twip,        // top-left of the run box, page-local
  w_twip, h_twip,        // run box (for the transparent span geometry)
  baseline_twip,         // for exact vertical placement
  text: string,          // the run's characters (selection/copy source)
  dir: "ltr" | "rtl",
  node, offset           // ModelPos of the run start (selection ↔ model bridge)
}
```
The overlay places one transparent, correctly-sized element per run so browser
selection/copy/find/AT operate on real text aligned over the painted glyphs. `node`
+ `offset` let a browser selection map back to model positions for a rich copy
later.

### 5.3 Hit/caret/selection payloads
```
HitPayload    = { node: string /*NodeId u128 as dec string*/, offset: u32,
                  zone: "content" | "outside" }
CaretRect     = { page: u32, x_twip, y_twip, w_twip, h_twip }
SelectionRects= { page: u32, x_twip, y_twip, w_twip, h_twip }[]
```
`NodeId` is a `u128`; JS has no 128-bit integer, so it crosses the boundary as a
**decimal string** and is only ever compared/passed back, never arithmetic'd in JS.

### 5.4 Position / Range (editor)
Mirror `sdk::{Position, Range, Affinity}`: `Position = { node: string, offset:
u32, affinity: "before"|"after" }`, `Range = { start: Position, end: Position }`.

### 5.5 Error model
Every fallible export throws a JS `Error` whose `.code`/`.severity`/`.message`
carry `SdkError`'s `ErrorCode`/`ErrorSeverity`. Import/admission failures surface
the compatibility report so the host can show "opened with N unsupported
constructs" — never a silent empty document.

## 6. Frontend architecture (vanilla TS + Vite)

No UI framework: a canvas app's DOM is chrome + the text overlay, so a framework
earns little and costs bundle weight and an indirection layer over the render loop.
Modules:

- **`core/`** — loads the WASM module, owns the `WasmDocument` handle, and is the
  *only* code that talks to WASM. Exposes a typed async facade. When threading
  lands (§9) this module moves behind a Worker with the same interface.
- **`render/`** — the **content layer**: a scrollable, **virtualized** page list.
  Only pages intersecting the viewport (+ a small overscan) are rastered; others
  are placeholders sized by `page_size`. A page-bitmap LRU cache keyed by
  `(page, dpi)` bounds memory. Zoom sets a new `dpi` and re-rasterizes visible
  pages (crisp, not upscaled).
- **`overlay/`** — a transparent layer above each page holding the `TextLayer`
  spans (selection/copy/find/AT). In the editor phase it also draws the caret and
  selection highlight from `caret_rect`/`selection_rects`.
- **`input/`** (editor phase) — keyboard, pointer, IME composition, clipboard →
  semantic session requests.
- **`chrome/`** — toolbar (open, zoom, fit-width, page nav), dialogs.

Render loop: scroll/zoom → compute visible pages → for each uncached visible page,
`await core.render_page(i, dpi)` → blit; request the matching `text_layer(i)` and
mount overlay spans. All coordinate conversion via the one units module (§3).

## 7. Host layer

### 7.1 Desktop (Tauri)
- Commands: `open_file()` (native dialog → bytes → `open`), `save_file(bytes)`,
  `list_system_fonts()` / `load_font(path)` → bytes → `register_font`.
- The webview runs the exact `app/frontend`; Tauri sets response headers, so
  **COOP/COEP are on** and the full worker + SharedArrayBuffer model is available.
- Optional later: run `casual-doc-wasm`'s `rlib` **natively** in the Rust backend
  (no browser memory ceiling) with the same facade — the doc 56 escape hatch.

### 7.2 Web
- File via `<input type=file>` / File System Access API. Fonts fetched over the
  network → `register_font`. Same frontend, different host services — the
  `font-provisioning-strategy` seam.

## 8. Distribution and build pipeline

- **Toolchain:** `wasm-bindgen` + `wasm-pack` (or `trunk`-free manual
  `wasm-bindgen-cli`) producing an ES module + `.wasm`. Frontend bundled by Vite.
- **Self-hostable SDK (`packages/sdk`, primary):** the OnlyOffice Document-Server
  model. A drop-in bundle the customer hosts on their editor server, which sets
  **COOP: same-origin / COEP: require-corp** → cross-origin isolation →
  `SharedArrayBuffer` + worker threads available. Fonts/host services provisioned
  per deployment.
- **CDN / npm embed (secondary):** same bundle; when the embedding host cannot set
  isolation headers it **degrades to main-thread WASM** (no SAB) — reduced
  concurrency, identical output.
- **Size discipline:** `wasm-opt -Oz`, strip, and gate the `.wasm` size in CI.

## 9. Threading upgrade path

1. **v1 main-thread.** `core/` calls WASM synchronously (wrapped in async for API
   stability). One page rasters in a few ms.
2. **Worker.** Move `core/` behind a Worker. The public facade is unchanged
   (already async). Transport:
   - **SharedArrayBuffer** for WASM linear memory (zero-copy model access);
   - **OffscreenCanvas** transferred to the worker so it paints page bitmaps
     directly;
   - **transferable `ImageBitmap`** for any page handed back to the main thread.
3. **Guard:** feature-detect `crossOriginIsolated`; without it, stay main-thread.

Serialization: start with serde-JSON for structured payloads (simple, debuggable);
if profiling shows boundary cost, switch `TextLayer`/hit payloads to a compact
binary codec behind the same TS types. Bitmaps never go through JSON.

## 10. Milestones (each a small, gated PR)

Gates for every code PR: `cargo +1.96.0 fmt --all --check`, `clippy
--workspace --all-targets -D warnings`, `cargo test --workspace`, `RUSTDOCFLAGS
="-D warnings" cargo doc --no-deps`, `wasm32-unknown-unknown` check, `cargo deny
check`. WASM PRs add a `wasm-pack build` step and a headless raster-parity check.

- **P1G-001 — WASM bridge + first pixel.** New `casual-doc-wasm` with `open`,
  `page_count`, `page_size`, `render_page(i, dpi)`. A minimal HTML harness (no
  framework yet) blits page 0 of a corpus `.docx` to a canvas.
  *Accept:* page 0 of `Sample Document.docx` renders in-browser pixel-comparable
  to the native `render_gallery` PNG for that page. *Tests:* Rust unit (open →
  page_count matches native), a headless render-parity assertion (WASM RGBA ==
  native RGBA for a fixture page, within tolerance).
- **P1G-002 — Viewer core.** `app/frontend` (Vite): virtualized scroll, zoom
  (re-raster at dpi), fit-width, page count, bitmap LRU. *Accept:* smooth scroll
  through all 26 pages of Sample at 100 %/fit/200 % with bounded memory.
- **P1G-003 — Copy-in-view.** `text_layer`, overlay spans, `link_at`. *Accept:*
  drag-select across lines and pages, ⌘C yields correct text; Ctrl+F finds text;
  screen reader reads content; clicking a hyperlink opens it. *Tests:* `text_layer`
  run positions match `DisplayList` glyph positions; `copy_text` across a range
  equals the model text.
- **P1G-004 — Desktop shell.** Tauri wrapper; native open; `list_system_fonts` →
  `register_font` → `repaginate`. *Accept:* open a local `.docx`; a document using
  an OS-only font shapes with that font on desktop.
- **P1G-005 — Worker threading.** Move `core/` to a Worker with SAB +
  OffscreenCanvas; `crossOriginIsolated` guard with main-thread fallback.
  *Accept:* UI stays responsive (input/scroll) while a large doc paginates;
  no output change vs main-thread.
- **P1G-006 — Editor foundation.** Caret + selection engine on
  `hit_test`/`caret_rect`/`selection_rects`/`move_vertical`; keyboard navigation.
  Overlay retained for a11y/find. *Accept:* click places caret at the right model
  position; shift-arrow/drag selects; selection highlight matches text.
- **P1G-007 — Editing.** Pointer/keyboard/IME → `insert_text`/`delete_range`/
  `split_paragraph`/`join_paragraphs`; dirty-page relayout + repaint; `undo`/
  `redo`; `subscribe`/`drain_events`. *Accept:* type/delete/split/join with
  correct reflow and undo; only changed pages re-raster.
- **(later) P1G-008 — GPU backend** (`wgpu`/Vello behind `DisplayList`) and
  **P1G-009 — native desktop core** (`rlib` in the Tauri backend).

## 11. Testing and CI

- **Raster parity:** a headless harness renders a fixture page via WASM and via
  native `casual-doc-render` and asserts RGBA equality within tolerance — catches
  boundary/format bugs and locks "one renderer" (doc 56).
- **Contract tests:** `text_layer` positions vs `DisplayList`; `copy_text` vs
  model text; `hit_test`/`caret_rect` round-trips.
- **WASM build gate:** `wasm-pack build` + `.wasm` size budget in CI.
- **No new fidelity oracle:** visual fidelity stays measured by the doc-55/83
  pipeline; the viewer inherits it.

## 12. Risks and open decisions

- **Overlay alignment tolerance** — how tightly the transparent text must track
  painted glyphs before selection feels wrong; may need per-glyph (not per-run)
  boxes for justified/complex-script lines. Decide in P1G-003 with real corpus.
- **IME on canvas** — a hidden contenteditable proxy vs a composition API; decide
  in P1G-006/007.
- **Dirty-page set** — whether `paginate`/session already yields changed pages or
  P1G-007 must add it (affects edit repaint cost).
- **Binary codec vs JSON** — defer until P1G-002/003 profiling.
- **Desktop default** — WASM-in-webview first (max code share) vs native-core
  first (max perf); default to WASM, revisit if perf demands.
- **npm-embed COOP/COEP posture** — document the main-thread fallback clearly so
  embedders aren't surprised by reduced concurrency.

## 13. References

- doc 56 — editor shell & render architecture (decision)
- doc 45 — extensibility seams I1–I4 (the edit-path checklist P1G-006/007 satisfy)
- doc 55 — current fidelity gap audit (the pipeline the viewer is a window onto)
- `font-provisioning-strategy` — the one host-populatable font seam
