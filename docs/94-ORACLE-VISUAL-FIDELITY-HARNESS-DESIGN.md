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
- **H2 — LibreOffice oracle for the core corpus.** *(scaffolding landed)* Diff our page geometry against a pinned-LibreOffice reference. See §H2 concretely below.
- **H3 — Complex/smoke tier + matrix wiring.** Diff the user's complex corpus at looser tolerance; feed measured scores into the matrix.

## H2 concretely (as built)

The H2 scaffolding is in place; the reference data is produced by the re-bless job when it first runs.

- **Corpus:** the existing redistributable `fixtures/corpus/real-producer-libreoffice.docx` (Apache-2.0, already LibreOffice-authored) is the first oracle fixture — no new corpus needed. More fixtures join the `FIXTURES` map in the re-bless workflow and the `oracle_geometry` test.

- **The compared quantity — say it once, precisely.** Both sides reduce a page to the **text region**: the union, over every painted piece of text, of

  ```text
  x: [pen position before the first non-space glyph, pen position after the last]
  y: [baseline − the face's ascent,                  baseline + the face's descent]
  ```

  This is what `pdftotext -bbox` reports per *word*, and it is **neither ink nor the block boxes**. Poppler derives a word box from the text state, not the glyph outlines: its `yMin`/`yMax` are the PDF font descriptor's ascent/descent scaled to the run's size (measured on this corpus: LibreOffice's 12pt Liberation Serif word boxes are 13.284pt tall, exactly `(1824+443)/2048 × 12pt`; lowercase ink is nowhere near that), and its `xMin`/`xMax` are pen positions. Because our shaped runs carry the *same* two numbers per run, the oracle's quantity is reproducible exactly — there is **no** ink-vs-layout fudge factor, and none is applied.

  Leaving this underspecified is what made the gate red the first time it ran. The original text said the oracle unioned "word boxes" while our side "reduces our placed body fragments", and the two were silently different quantities: paragraph box rects include `w:spacing` before/after and span the whole flowed column width. On this corpus that alone mis-measured `y0` by −241 twips, `y1` by +284, and `x1` by +4420 — none of it a layout defect.

- **Font-parity scoping.** The font-parity argument above (both renderers shape with the bundled metric-compatible faces) holds only for the code points those faces **cover**. This corpus fixture also carries CJK (`日本語`) and Arabic (`العربية`) runs, which none of Liberation/Carlito/Caladea covers: each side substitutes whatever it can find, with different advances — and a wider substitute also shifts every word *after* it on the same line. So both sides group their text boxes into **lines** (by vertical overlap) and drop any line containing text the pinned faces cannot shape — the oracle by code point, the engine by resolved face (a dynamically interned fallback, or bundled-but-not-metric-compatible Roboto, is not pinned). The number of dropped lines is recorded per page and compared **exactly**, so the exclusion is a gate rather than a hole.

- **Reference geometry (resolved open question #1):** auto-extracted from the LibreOffice PDF, *not* hand-blessed. `scripts/oracle/extract-geometry.sh` runs pinned `soffice --convert-to pdf`, then `pdftotext -bbox`, and reduces the word boxes as above to `{ "schema": 2, "pages": [ { "sizeTwips": [w,h], "contentBboxTwips": [x0,y0,x1,y1]|null, "excludedLines": n } ] }` (PDF points → twips, origin top-left). Committed under `fixtures/oracle/<id>.geom.json`. **`schema` is the contract stamp:** bump it in the script and in the test's `ORACLE_SCHEMA` whenever the *meaning* of a field changes, so a reference blessed under the old meaning is never compared against the new one.

- **Comparison:** `crates/casual-doc-render/tests/oracle_geometry.rs` imports the fixture, paginates, reduces the page's **composed display list** (`compose_page` — so body text, running headers/footers, footnote bodies, table cells and text boxes are already flattened into absolute page coordinates by the one implementation that owns those transforms) to the same quantity, and diffs within a **±40-twip (2pt)** tolerance band. Page count and the excluded-line count are exact; page size and each content-bbox edge are within tolerance. Every out-of-tolerance edge is reported by name.

- **Residual and why the tolerance is 40 twips.** With both sides measuring the text region over the pinned-parity lines, the corpus fixture's residual is `x0` 2, `y0` 6, `x1` 0, `y1` 5 twips. Two effects account for all of it, and the tolerance is sized to bound *them*, not to clear today's numbers:
  - **PDF coordinate rounding, ≈2 twips** — LibreOffice writes text origins at 0.1pt.
  - **Half the face's line gap** — `parley` splits a face's line gap around the text box (half above the ascent) where LibreOffice puts all of it below the descent, so our baselines sit `lineGap/2` lower. For the bundled faces `lineGap ≈ 0.0327 em`, i.e. ≈0.016 em: 4 twips at 12pt, 8 at 24pt, 16 at 48pt, scaling linearly with the largest font on the page.

  40 twips therefore holds for any bundled face up to ≈115pt with ~5× headroom over the observed worst edge, while staying far below the errors worth catching (a one-line vertical slip at body size is 276 twips). A fixture that needs more must record *why* here — raising the number to turn a red edge green is not a fix. The `lineGap/2` term is a real, if small, fidelity difference from LibreOffice/Word; closing it belongs in the shaper, not in this tolerance.

- **Inert until blessed:** a fixture with no committed reference is **skipped**, so the test never reddens the main CI before the oracle job has run; it becomes a live fidelity gate the moment a reference lands. A reference at an *older* schema is compared only on page count and page size — the quantities no schema bump has changed — and the test prints the re-bless instruction, so a semantics change degrades the gate loudly instead of reddening main or being quietly hand-patched.
- **Hermetic main CI + reviewed re-bless:** `.github/workflows/oracle-geometry.yml` is a manual (`workflow_dispatch`) job that installs a pinned LibreOffice and **only** the bundled metric-compatible faces (Liberation/Carlito/Caladea — the font-parity crux), regenerates the references, and opens a PR whose geometry diff a maintainer reviews. The everyday CI stays hermetic (no LibreOffice, no network) and just compares against the committed references.
- **Platform:** the geometry comparison is pinned to Linux/macOS and skipped on Windows, whose text stack shapes differently (the same reason H1 is Windows-gated — see the H1 test and PR #316).

## Open questions

1. ~~Geometry extraction from the oracle~~ — **resolved:** auto-extract from the LibreOffice PDF via `pdftotext -bbox` (see §H2). Per-*block* correspondence between the two renderers is unstable, so H2 compares **page-level** geometry (size + content bbox); per-block diffing is deferred unless a stable correspondence is found.
2. Perceptual metric: tolerance-banded pixel-fraction is the floor; is SSIM worth the dependency, or is pixel-fraction + geometry enough? (Leaning: start without SSIM. The H2 image-diff gate is not yet built — geometry lands first.)
3. Corpus location and licensing of any real-world sample docs (must be redistributable to live in-repo). H2 starts on the already-vetted `real-producer-libreoffice.docx`; expanding to the user's complex corpus (H3) still needs a redistribution check.
4. **Corpus coverage vs. the pinned font set.** `real-producer-libreoffice.docx` spends its last paragraph on CJK + Arabic, which the pinned faces do not cover, so a quarter of its lines are excluded from the geometry comparison (see §H2 *Font-parity scoping*). A small Latin-only fixture exercising the geometry-subtle cases (page-top spacing, numbering tab-suffix, `w:hideMark`) would give H2 real coverage; the alternative — adding CJK/Arabic faces to the oracle container — is *not* equivalent, because the engine's bundled set would still have to match them face-for-face.
