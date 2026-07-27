# OpenDoc WASM viewer — **testing harness only**

> [!IMPORTANT]
> **This is a developer test harness, not a product and not a supported viewer.**
> It exists so we can open real `.docx` files and eyeball the engine's rendering
> and pagination in a browser — a fast, deployable feedback loop for finding and
> fine-tuning fidelity issues. It is intentionally minimal, unstyled beyond the
> essentials, and carries **no stability guarantees**. Do not build on it or link
> to it as a public feature. The real, self-hostable viewer/editor SDK is tracked
> separately (docs 56 & 57, Phase 1G).

## What it does

Loads `casual-doc-wasm` (the OpenDoc engine — import → paginate → render —
compiled to WebAssembly) and renders a document you pick **entirely in your
browser**. Nothing is uploaded to a server. It is the browser-first surface the
viewer→editor is developed and fine-tuned on (doc 56 decision: fat client,
Rust→WASM, canvas paint).

Milestones landed here:

- **P1G-001** (WASM bridge + first pixel): `open`, `pageCount`, `pageSize`,
  `renderPage`.
- **Web font provisioning**: CJK / complex-script fallback fetched over the
  network (see below).
- **P1G-003** (copy in view): drag to select, ⌘C to copy. The caret and
  selection highlight are **drawn from the engine's own geometry** (`hitTest`,
  `caretRect`, `selectionRects`), so they match the painted glyphs exactly. This
  is the first read-only slice of the scalable interaction pipeline
  (`docs/58-INTERACTION-SELECTION-EDITING-ARCHITECTURE.md`) that editing —
  text, then tables/images/floats/headers — extends.

Native find/AT overlay, hyperlink-open, and editing arrive in later milestones.

## Run it locally

```bash
cd webapp
./build.sh                     # builds ./pkg via wasm-pack
python3 -m http.server 8080    # or any static server
# open http://localhost:8080
```

`./pkg/` is generated (git-ignored); `build.sh` and the CI workflow rebuild it.

## Deployment

`.github/workflows/pages.yml` builds the WASM and publishes this directory to
**GitHub Pages** on pushes to `main` that touch `webapp/` or the engine crates.
The deployed site is, again, **only a testing surface**.

## Fonts (CJK / complex scripts)

The WASM build ships only bundled **Latin** faces, so CJK / complex-script text
would otherwise render as blank/`.notdef` tofu (▯). On open, the viewer inspects
`missingCoverage()`, works out which scripts are needed, and **fetches the
matching Noto face(s) over the network** (jsDelivr) — Japanese/Korean/Simplified
Chinese (CJK OTFs, ~16 MB each, fetched once and cached), plus Arabic,
Devanagari, Hebrew, Thai — registers them via the engine's fallback seam, and
re-renders. This is the "browser = network-fetched fonts" half of the
font-provisioning strategy.

Not yet covered: **color emoji** (a separate font case) and high-quality
**Japanese line breaking** (parley currently has no `ja` segmentation
dictionary).

## Notes / limitations

- The `.wasm` is large (bundled fonts + shaping/ICU data). Acceptable for a test
  harness; size trimming is deferred.
- All pages render eagerly. Virtualized scroll, zoom-to-fit, and a page-bitmap
  cache are P1G-002.
- Worker threading (`SharedArrayBuffer`) needs COOP/COEP headers GitHub Pages
  cannot set, so rendering runs on the main thread here (P1G-005 territory).

## Attribution

Toolbar icons are from **Material Symbols** by Google, licensed under the
**Apache License 2.0**, inlined as SVG paths.
