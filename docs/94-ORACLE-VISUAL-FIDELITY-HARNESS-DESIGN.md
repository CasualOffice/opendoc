# 94 — Oracle-Based Visual Fidelity Harness

**Status:** Proposed; foundation slice. Recommended before the remaining geometry-subtle rendering fixes (page-top spacing compat, numbering tab-suffix, `w:hideMark`, run shading), which cannot be verified safely without a visual oracle.
**Scope:** A new dev/CI-only crate (`casual-doc-oracle`, or a `tests/visual` harness) plus a pinned fixture corpus and a gated CI job. No change to the shipping crates' behavior; the harness only *reads* the existing deterministic CPU render path (`casual-doc-render::render` → `Surface::encode_png`).
**Relates to:** doc 44 (rendering pipeline), doc 18 (support matrix — which the audit showed overstates coverage), doc 40 (font management — the bundled metric-compatible faces are what make oracle parity possible), the rendering-fidelity audit backlog.

## Problem

The support matrix and per-feature unit tests assert *structural* facts (a marker exists, a border composes, a page paginates at the section size). They do not catch **visual** regressions or fidelity gaps: a marker in the wrong place, spacing that is a few points off, a glyph that falls back to the wrong face, content aligned to the wrong edge. The audit that produced the current backlog found several "modeled but mis-painted" and "rendered wrong" gaps that only a *reference-image* comparison surfaces, and it found the public matrix claiming "full" where paint was incomplete.

Several remaining backlog items are **geometry-subtle** — the correct value depends on Word's exact behavior (page-top `space_before` suppression, the tab stop a numbering suffix advances to, an empty `hideMark` cell's collapsed height, autospacing sizing). Implementing them "by reasoning" risks trading one wrong pixel for another. We need an **oracle**: an independent, trusted renderer we can diff against.

## Goals

1. **Detect visual regressions** in CI: a change that moves/re-colors/re-sizes painted content against a pinned baseline fails the build.
2. **Quantify fidelity** against an independent reference (LibreOffice headless) so "full" in the matrix is *measured*, not asserted.
3. **De-risk geometry-subtle fixes**: a developer implementing page-top spacing or tab-suffix advance can see the pixel/geometry delta vs the oracle before and after.
4. **Deterministic and hermetic**: same inputs → same bytes, on CI and locally, with no network and pinned tool/font versions.

## Non-goals

- Pixel-perfect equality with LibreOffice (impossible — different rasterizers, hinting, anti-aliasing). We compare within **tolerance bands** and on **structured geometry**, not exact bytes.
- Replacing unit tests. The harness is a coarse safety net and a fidelity meter; targeted unit tests remain the precise specification.
- Testing the WASM/canvas paint path. The oracle diffs the **CPU** `casual-doc-render` output, which shares the layout galley with WASM; canvas-only concerns (e.g. highlight/shading painted in JS) are out of scope until that paint moves into the shared renderer.

## Architecture

Three artifacts per fixture, all at a fixed DPI (proposed **150**):

```
fixture.docx  ──▶  our layout+render  ──▶  ours/fixture.pXX.png   (casual-doc-render)
              ──▶  our layout dump    ──▶  ours/fixture.geom.json (page/section/block rects)
              └──▶  soffice --headless --convert-to pdf ─▶ pdftoppm ─▶ oracle/fixture.pXX.png
```

### 1. Fixture corpus (`tests/visual/fixtures/*.docx`)

A curated, version-controlled set of small, single-concern `.docx` files (bullets, roman/letter numbering, restarted lists, page numbering formats, vAlign, contextualSpacing runs, tables with borders/spanning, headers/footers). Each is minimal and authored to exercise one or two behaviors so a diff localizes the cause. The user's complex sample corpus (see [[opendoc-visual-fidelity-corpus]]) feeds a second, "smoke" tier that is diffed at looser tolerance.

### 2. The oracle (pinned LibreOffice)

- `soffice --headless --convert-to pdf` then `pdftoppm -r 150 -png`, both **version-pinned** (a Docker image digest in CI; a documented local version). Output is cached as committed reference PNGs so a normal CI run does **not** invoke LibreOffice — it compares against the committed oracle images. LibreOffice runs only on an explicit "re-bless" workflow.
- **Font parity is the crux and is already solved**: the oracle must render with the *same* faces we do. We bundle the metric-compatible families (Liberation Sans/Serif/Mono for Arial/Times/Courier, Carlito for Calibri, Caladea for Cambria) precisely so that both renderers pick identical metrics. The oracle container installs exactly those families and *only* those, so line breaking and glyph advances match. This is why doc 40's bundling decision makes an oracle viable.

### 3. Our render

`paginate_document` → build the display list per page → `casual-doc-render::render` → `Surface::encode_png`. This path already exists and is deterministic (no `Date`/random; pinned fonts). A parallel **geometry dump** serializes each page's `page_size`, `content_area`, and top-level block rects (twips) to JSON.

### 4. Comparison

Two independent gates, because each catches what the other misses:

- **Geometry diff (primary, precise):** compare our `geom.json` block/page rects to values extracted from the oracle (page size + text-block bounding boxes via the PDF text layer, or a committed hand-verified geometry baseline). Assert within a **twip tolerance** (proposed ±1pt = 20 twips for placement, exact for page/section size). Geometry is the signal that survives rasterizer differences and pinpoints *layout* bugs (the class the audit found).
- **Perceptual image diff (secondary, holistic):** compare `ours/*.png` to `oracle/*.png` with a tolerance-banded metric — per-pixel diff with an anti-aliasing-tolerant threshold, aggregated to a **fraction-of-pixels-changed** score, plus an optional SSIM. Each fixture has a committed threshold; exceeding it fails. Catches color/coverage/decoration bugs geometry can't see.

## Determinism & tolerance

- **DPI, page size, fonts** pinned; anti-aliasing/hinting differences absorbed by the pixel tolerance band (a small per-channel delta is not a diff).
- Baselines (`oracle/*.png`, thresholds) are committed and reviewed; a diff either reveals a regression (fix the code) or an intended change (re-bless via the explicit workflow, reviewed like any baseline change).
- Reference images are small (150dpi, single pages, mostly text) — committed directly first; escalate to git-LFS only if the corpus grows past a size budget (proposed 5 MB total).

## CI integration

- A **gated job** (`visual-fidelity`) that runs our render + geometry dump and compares to committed baselines. No LibreOffice, no network — fast and hermetic.
- A separate **manual/scheduled** `reblesss-oracle` workflow (pinned LibreOffice container) regenerates `oracle/*.png` when a reviewer intends it; its output is committed via PR and diff-reviewed.
- Failure output uploads the (ours, oracle, diff-heatmap) triptych as artifacts so the delta is inspectable.

## Metrics surfaced

Each run emits a per-fixture report: max/mean geometry delta (twips), changed-pixel fraction, SSIM. Aggregated, this becomes the *measured* basis for the support matrix's per-family fidelity claim (closing the doc 18 / audit finding that "full" was asserted, not measured).

## Phasing

- **H1 — CPU render + geometry dump + pixel diff, self-referential.** Land the harness comparing our render to *committed baselines of our own output* (a pure regression gate, no oracle yet). Immediate value: locks current behavior; catches accidental visual regressions in the remaining fixes.
- **H2 — LibreOffice oracle for the core corpus.** Add the pinned container, generate `oracle/*.png`, wire the perceptual + geometry gates for the single-concern fixtures. This is where geometry-subtle fixes get their reference.
- **H3 — Complex/smoke tier + matrix wiring.** Diff the user's complex corpus at looser tolerance; feed measured scores into the matrix.

## Open questions

1. Geometry extraction from the oracle: PDF text-layer bounding boxes vs a committed hand-verified geometry baseline for H2? (Leaning: committed baseline, re-blessed like the PNGs — simpler and avoids a PDF-parsing dependency.)
2. Perceptual metric: tolerance-banded pixel-fraction is the floor; is SSIM worth the dependency, or is pixel-fraction + geometry enough? (Leaning: start without SSIM.)
3. Corpus location and licensing of any real-world sample docs (must be redistributable to live in-repo).
