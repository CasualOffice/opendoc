# 56 — Editor Shell & Render Architecture

**Status:** Accepted architecture decision; implementation staged viewer-first.
**Decision date:** 2026-07-27
**Code baseline:** `main` after PR #161 (through the doc 54 rendering series).
**Related:** doc 42 (rendering research), doc 43 (layout/render engine), doc 45
(extensibility seams I1–I4), doc 46 (fidelity gaps), doc 55 (current gap audit),
the `font-provisioning-strategy` note.

## Purpose

This document records the architecture for the interactive application layer:
the on-screen document surface, the viewer we ship first, and the editor it grows
into. It fixes the **shape** (where the engine runs, how pages are painted, how
text is selected, how edits apply) so that the viewer we build now is the
editor's foundation and not throwaway work.

The engine — import, model, layout/pagination, rasterization, hit-testing,
selection, and transactions — already exists in the Rust workspace. This layer is
the **shell** around it, plus the WASM bridge that lets a browser/webview drive it.

## Goal and non-goal

- **End goal:** a Word-grade, paginated **editor** that runs in a desktop shell
  (Tauri) and, from the same code, in a browser tab.
- **First delivery:** a read-only **viewer** — open, scroll, zoom, **select and
  copy text**, follow hyperlinks — built on the editor architecture so every
  piece carries forward.
- **Not a goal:** `contenteditable`/DOM-based editing. No serious paginated editor
  uses it; it cannot give deterministic pagination or cross-browser-identical
  layout. Ruled out.

## Prior art and the decision

Two proven architectures bracket the design space:

| | Engine runs | Painting | Selection / copy | Live backend required |
| --- | --- | --- | --- | --- |
| **OnlyOffice** — *fat client* | Browser (JS `sdkjs`) | HTML5 Canvas | Client-side, custom | No (offline-capable) |
| **Collabora / LibreOffice Online** — *thin client* | Server (native LibreOffice) | Server → 256×256 bitmap **tiles** | Server round-trip (browser holds pixels, not text) | **Yes, per session** |

**Decision: the OnlyOffice fat-client model** — the document model, layout, and
paint run client-side; the canvas is the surface; interaction is custom — **but
realized as Rust compiled to WebAssembly instead of hand-written JavaScript.**

Rationale:

- OnlyOffice had to *rewrite* an office engine in JS to get a fat client. We do
  not: the engine is already Rust (layout, raster, hit-test, `casual-doc-selection`,
  `casual-doc-transaction`). We **compile it to WASM** — no re-port, and WASM runs
  these tight numeric loops near-native with **no GC pauses** (JS's main source of
  editor jank). We are not conceding speed to OnlyOffice's JS; we are ahead on
  compute.
- The thin-client (Collabora) model is explicitly rejected: it needs a live
  backend for every session, adds a server round-trip per keystroke, and — because
  the browser receives pixels, not text — makes **local copy, find, and
  accessibility a server round-trip**. That directly violates the "copy text in
  view" requirement and the offline-desktop goal.

This is also the Figma pattern (C++/WASM core, canvas paint, JS interaction),
generalized.

## Layer contract

Three layers with a hard boundary between them.

### 1. Core (Rust → WASM) — the source of truth

Owns everything that must be deterministic and identical across hosts:

- import → model → semantic export;
- layout / pagination (incremental — only changed pages reflow);
- **rasterize a page → RGBA bitmap** (the existing `compose_page` → `DisplayList`
  → `casual_doc_render` path);
- **emit a per-page text layer** (positioned text runs, already in the
  `DisplayList`) for selection/copy/find/accessibility;
- hit-testing (`hittest.rs`): screen coord ↔ `NodeId`/`ModelPos`; caret and
  selection-rectangle geometry;
- edit operations + undo/redo via `casual-doc-transaction`.

The edit path **is** the realization of the doc 45 invariants: every mutation goes
through the single choke point (**I1**) as a member of the closed, invertible op
set (**I2**), anchored on `NodeId`/`ModelPos` (**I3**), with derived/AI data in a
sidecar (**I4**). Building the editor here keeps OT/CRDT, agentic, and RAG layers
additive by construction.

New crate: **`casual-doc-wasm`** — a thin `wasm-bindgen` façade over
`casual-doc-sdk` exposing: `open(bytes)`, `page_count()`, `render_page(i, dpi) →
bitmap`, `text_layer(i)`, `hit_test(i, x, y)`, `selection_rects(range)`,
`copy_text(range)`, and (editor phase) `apply_op(op)`. The same `casual-doc-sdk`
compiles **natively** for the desktop path (below).

### 2. Frontend (TypeScript) — interaction only

Owns nothing about layout or painting:

- input: keyboard, pointer, **IME composition**, clipboard;
- canvas management: a **content layer** (blit the core's page bitmaps) and a
  transparent **overlay layer** (caret, selection highlight, handles) drawn in JS
  from geometry the core returns;
- scroll, zoom, and **page virtualization** (only visible pages are rastered/held);
- chrome: toolbar, menus, dialogs;
- in the editor phase: translate input into ops and call `apply_op`; receive the
  dirty-page set + new geometry; repaint.

Framework-agnostic core module, with thin Tauri and web entry shells so both hosts
run the same frontend.

### 3. Host — native/web services

- **Desktop (Tauri):** native file open/save; **OS font enumeration** fed into the
  core font registry; native window/menus; and the option to run the core
  **natively** in the backend (below).
- **Web:** file upload / File System Access API; **network-fetched fonts** fed into
  the *same* registry seam.

Fonts follow the accepted `font-provisioning-strategy`: bundled base always;
desktop = OS system fonts; web = network fonts; **one host-populatable registry
seam** for both. The host differs; the core and frontend do not.

## Rendering: one IR, swappable backend

The `DisplayList` is already a backend-agnostic paint IR. The paint backend sits
behind it and is swappable:

- **v1 — CPU (`tiny-skia`):** the renderer tuned against LibreOffice (docs 46/55).
  Rasterize page → RGBA → blit to canvas. **Zero fidelity divergence**; every doc-55
  fix flows into the viewer for free. Zoom stays crisp by **re-rasterizing at the
  target DPI**, not upscaling a bitmap.
- **later — GPU (WebGPU):** a `wgpu`/**Vello**-class backend behind the same
  `DisplayList`, for large documents and smooth zoom/scroll/animation. Held as an
  additive backend, not a v1 dependency.

**Fidelity caveat for the GPU path:** a second renderer must stay pixel-consistent
with the CPU one, or we accept a deliberate *screen-renderer vs print/export-
renderer* split (as mainstream editors do). Either way the CPU raster remains the
fidelity oracle and the export/print path.

## Text selection and copy (required in view mode)

Canvas painting destroys native text selection — painted glyphs are pixels the
browser cannot see. Two proven techniques, both on the editor path, so neither is
throwaway:

1. **Transparent text overlay (PDF.js technique) — ships in the viewer.** Over the
   page bitmap, lay an invisible, correctly-positioned HTML text layer (from the
   core's per-page text layer). The browser then does **selection, ⌘C copy,
   Ctrl+F find, and screen-reader accessibility natively**. Fastest correct path to
   "copy in view," and it delivers find + a11y as a bonus.
2. **Custom selection engine (Google Docs technique) — arrives with editing.**
   Pointer events → core `hit_test` → `selection_rects` drawn on the overlay; ⌘C
   pulls text between two `ModelPos` anchors from the core. This is the editor's
   caret/selection engine.

The overlay **persists** alongside the custom engine as the permanent
accessibility/find layer — exactly as Google Docs keeps an ARIA text layer beside
its canvas.

## Threading model

- **v1: main-thread WASM.** A single page relayouts/rasterizes in a few ms —
  imperceptible. Simplest; correct first.
- **When responsiveness demands: a Web Worker** running the core, with the
  main↔worker boundary kept **zero-copy**:
  - **SharedArrayBuffer** — WASM linear memory shared with the main thread (no
    model copy);
  - **OffscreenCanvas** — the worker paints directly into the page canvas;
  - **transferable `ImageBitmap`** — hand a rastered page over by moving ownership.

  A worker does **not** slow computation (it runs full-speed on its own core); it
  hides latency by keeping the UI thread free for input and scroll. The only cost
  is the boundary, which shared memory / transferables neutralize.

**Cross-origin isolation:** `SharedArrayBuffer` requires COOP/COEP headers. See
distribution — self-hosting sets them; embeds without them **degrade gracefully to
main-thread WASM** (no SAB), which still works.

## Desktop native escape hatch

Because the core is Rust, the **same `casual-doc-sdk` compiles two ways**:

| Target | Core runs as | Ceiling |
| --- | --- | --- |
| Web | Rust → WASM in the tab | browser memory + **4 GB wasm32** address space (mitigated by page virtualization + incremental layout) |
| Desktop (Tauri) | WASM in the webview **or native in the backend** | native = **no browser cap**; full RAM and all cores |

OnlyOffice **cannot** do this — its engine is JavaScript, so even the desktop app
runs that JS engine inside a bundled Chromium. Our native-desktop option is a
structural edge: heavy documents (and any future spreadsheet with real formula
recalc) get a native, unbounded compute path. Both builds sit behind one interface
so the host chooses per target.

**On the "will the tab crash" concern:** that ceiling is a *spreadsheet* problem
(million-cell dependency graphs), not a word-processor one. Our per-frame cost is
layout, which is bounded by document length and **incremental** (only changed pages
reflow), and with page virtualization memory stays bounded. If a spreadsheet is
ever built, its heavy recalc has an obvious home: native on desktop, native/server
offload on web.

## Distribution and packaging

- **Self-hostable SDK (primary) — the OnlyOffice Document Server model.** Ship an
  SDK bundle (WASM + JS glue + assets) that a customer installs and hosts on their
  own editor server. The server sets **COOP/COEP**, so cross-origin isolation is
  satisfied and the **full worker + SharedArrayBuffer** threading model is
  available. This is also how the font/host seam is provisioned per deployment.
- **CDN / npm embed (secondary).** A drop-in bundle for simple embeds. If the
  embedding host cannot set isolation headers, it **degrades to main-thread WASM**
  (no SAB) — reduced concurrency, same correctness.
- **Desktop app (Tauri).** Wraps the same frontend; provides native fonts, file
  IO, and the optional native core.

Proposed layout (names per the doc 00 recommendations):

- `crates/casual-doc-wasm` — `wasm-bindgen` façade over `casual-doc-sdk`;
- `app/frontend` — framework-agnostic TS surface (canvas host, overlay, chrome);
- `app/desktop` — Tauri shell (native host services);
- `packages/sdk` — the self-hostable embed bundle (built artifact).

## Viewer → editor delivery order

Each step is a small, gated increment; the viewer steps are all editor foundation.

1. **WASM bridge + first pixel.** `casual-doc-wasm` façade; render page 0 of a real
   `.docx` to a canvas through the WASM path. Proves the pipeline end to end.
2. **Viewer core.** Multi-page virtualized scroll; zoom (re-raster at DPI);
   fit-width; page count.
3. **Copy in view.** Per-page text overlay → native select / ⌘C / Ctrl+F /
   accessibility. Hyperlink hit-testing → open links.
4. **Desktop shell.** Tauri wrapper; native file open; OS fonts into the registry.
5. **Threading.** Move the core to a Worker (SAB + OffscreenCanvas) behind a flag;
   graceful main-thread fallback.
6. **Editor foundation.** Caret + custom selection engine on `hit_test`; the
   overlay stays for a11y/find.
7. **Editing.** Input → `apply_op` (I1/I2 through `casual-doc-transaction`);
   dirty-page relayout + repaint; undo/redo.
8. **(later) GPU backend** and **native desktop core** as additive options.

## Caveats and open questions

- **One renderer is the invariant.** The CPU raster stays the fidelity oracle and
  export path; any GPU backend must match it or be an explicit screen-only path.
- **Rendering is ongoing (doc 55).** The viewer is a window onto a pipeline still
  gaining fidelity; it does not fork or freeze that work.
- **Open:** exact text-layer alignment tolerance for the overlay; IME strategy on
  canvas; virtualization/cache eviction policy; whether the desktop app defaults to
  WASM-in-webview or native-core first; COOP/COEP posture for the npm embed.

## References

- doc 42 — rendering architecture research
- doc 43 — layout / pagination / rendering engine design
- doc 45 — extensibility & collaboration seams (I1–I4); the edit-path checklist
- doc 46 — rendering fidelity gap analysis
- doc 55 — current DOCX fidelity gap audit
- `font-provisioning-strategy` — desktop OS / web network / bundled base; one
  registry seam
