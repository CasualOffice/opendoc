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

This corresponds to milestone **P1G-001** (WASM bridge + first pixel): `open`,
`pageCount`, `pageSize`, `renderPage`. Selection/copy, hit-testing, and editing
arrive in later milestones.

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

## Notes / limitations

- The `.wasm` is large (bundled fonts + shaping/ICU data). Acceptable for a test
  harness; size trimming is deferred.
- All pages render eagerly. Virtualized scroll, zoom-to-fit, and a page-bitmap
  cache are P1G-002.
- Worker threading (`SharedArrayBuffer`) needs COOP/COEP headers GitHub Pages
  cannot set, so rendering runs on the main thread here (P1G-005 territory).
