# 94 — Oracle-Based Visual Fidelity Harness

**Status:** H1 landed. **H2 landed and armed** — the LibreOffice geometry references are committed under `fixtures/oracle/` and the comparison runs on every pull request (`.github/workflows/ci.yml`, job `test`, step "Oracle geometry gate"); see §H2 concretely. **H2b landed** — `opendoc-fidelity compare` measures any document, not only the six gated fixtures, and reports the difference as numbers rather than as an impression; see §H2b. H3 not started. Recommended before the remaining geometry-subtle rendering fixes (page-top spacing compat, numbering tab-suffix, `w:hideMark`, run shading), which cannot be verified safely without a visual oracle.
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
- **H2 — LibreOffice oracle for the core corpus.** *(landed and armed)* Diff our page geometry against a pinned-LibreOffice reference. See §H2 concretely below.
- **H3 — Complex/smoke tier + matrix wiring.** Diff the user's complex corpus at looser tolerance; feed measured scores into the matrix.

## H2 concretely (as built)

**Status: armed.** The references are committed under `fixtures/oracle/`, the comparison runs on every pull request (`.github/workflows/ci.yml`, job `test`, step "Oracle geometry gate (docs/94 H2, FID-P-01)"), and `the_oracle_gate_is_armed` fails the build if a reference is deleted, staled or voided. Before that, the harness had been merged and described as protecting rendering fidelity while never having executed once — the failure skill §9 rule 2 exists for, tracked as FID-P-01.

Arming it measured three real divergences from LibreOffice and one comparability defect; both are recorded below rather than absorbed.

- **Corpus:** the existing redistributable `fixtures/corpus/real-producer-libreoffice.docx` (Apache-2.0, already LibreOffice-authored) is the first oracle fixture — no new corpus needed. More fixtures join the `FIXTURES` map in the re-bless workflow and the `oracle_geometry` test.

- **The compared quantity — say it once, precisely.** Both sides reduce a page to the **text region**: the union, over every painted piece of text, of

  ```text
  x: [pen position before the first non-space glyph, pen position after the last]
  y: [baseline − the face's ascent,                  baseline + the face's descent]
  ```

  This is what `pdftotext -bbox` reports per *word*, and it is **neither ink nor the block boxes**. Poppler derives a word box from the text state, not the glyph outlines: its `yMin`/`yMax` are the PDF font descriptor's ascent/descent scaled to the run's size (measured on this corpus: LibreOffice's 12pt Liberation Serif word boxes are 13.284pt tall, exactly `(1824+443)/2048 × 12pt`; lowercase ink is nowhere near that), and its `xMin`/`xMax` are pen positions. Because our shaped runs carry the *same* two numbers per run, the oracle's quantity is reproducible exactly — there is **no** ink-vs-layout fudge factor, and none is applied.

  Leaving this underspecified is what made the gate red the first time it ran. The original text said the oracle unioned "word boxes" while our side "reduces our placed body fragments", and the two were silently different quantities: paragraph box rects include `w:spacing` before/after and span the whole flowed column width. On this corpus that alone mis-measured `y0` by −241 twips, `y1` by +284, and `x1` by +4420 — none of it a layout defect.

- **Font-parity scoping.** The font-parity argument above (both renderers shape with the bundled metric-compatible faces) holds only for the code points those faces **cover**. This corpus fixture also carries CJK (`日本語`) and Arabic (`العربية`) runs, which none of Liberation/Carlito/Caladea covers: each side substitutes whatever it can find, with different advances — and a wider substitute also shifts every word *after* it on the same line. So both sides group their text boxes into **lines** (by vertical overlap) and drop any line containing text the pinned faces cannot shape — the oracle by code point, the engine by resolved face (a dynamically interned fallback, or bundled-but-not-metric-compatible Roboto, is not pinned). The **vertical extent** of the dropped lines is recorded per page and compared within the same tolerance as every edge, so the exclusion is a gate rather than a hole.

  The extent, not the line *count*, is the compared quantity, and that changed when the gate was first armed against real references (schema 2 → 3). Line grouping is per-renderer: on `real-producer-table-list` LibreOffice's two bullet lines sit 0.8pt apart and group as two, while our list marker's box is tall enough to overlap the following line, so the same two lines group as one. The counts read 1 vs 2 with no fidelity difference behind them; the extents read 557 vs 571, a 14-twip agreement, while a genuinely swallowed body line still moves the extent by ≈276 twips. `excludedLines` is still emitted as review context and is not gated.

- **Font provenance is read off the artifact.** Each reference records `fonts`: the faces the producing PDF actually embedded (`pdffonts`, subset prefix stripped). A reference naming a face that is neither a pinned metric-compatible family nor the one reviewed non-parity face (`Symbol`, LibreOffice's list-bullet face, whose `U+F0B7` lines both sides exclude before any advance is compared) is refused. This replaces the previous `fc-match` transcript, which described the producing *machine* rather than the document and is meaningless on macOS, where LibreOffice ships and uses its own copies of exactly these families. The `fc-match` check survives as a pre-flight in the re-bless job, where failing early is cheaper than producing a bad reference.

- **Known divergences are registered, not absorbed.** Three fixtures are genuinely outside the 40-twip band against LibreOffice 26.2.4.2, all on the page's bottom text edge, i.e. accumulated vertical drift: `rich` +263 twips (the paragraph after the nested table sits a line low), `table-merges` −55, and `table-list` −60 (drift accumulates monotonically down that page: table rows −10 then −25, list items −65, closing paragraph −60). These are the FID-L-21 signal, now reproducible from the repository rather than only from a local run. Each is listed in `KNOWN_DIVERGENCES` in the test with its measured delta and tracker row, and the comparison for that one edge is re-centred on `oracle + delta` **within the unchanged 40-twip band** — so the edge stays pinned to ±2pt, and the gate goes red if the divergence grows, shrinks, or is fixed. `known_divergences_are_still_divergent` refuses an entry small enough that the plain comparison would have passed, so the registry cannot decay into general slack. Neither the tolerance nor any reference was touched to make the gate green.

- **Reference geometry (resolved open question #1):** auto-extracted from the LibreOffice PDF, *not* hand-blessed. `scripts/oracle/extract-geometry.sh` runs pinned `soffice --convert-to pdf`, then `pdftotext -bbox`, and reduces the word boxes as above to `{ "schema": 3, "fonts": [..], "pages": [ { "sizeTwips": [w,h], "contentBboxTwips": [x0,y0,x1,y1]|null, "excludedLines": n, "excludedExtentTwips": n } ] }` (PDF points → twips, origin top-left). Committed under `fixtures/oracle/<id>.geom.json`. **`schema` is the contract stamp:** bump it in the script and in the test's `ORACLE_SCHEMA` whenever the *meaning* of a field changes, so a reference blessed under the old meaning is never compared against the new one.

- **Comparison:** `crates/casual-doc-render/tests/oracle_geometry.rs` imports the fixture, paginates, reduces the page's **composed display list** (`compose_page` — so body text, running headers/footers, footnote bodies, table cells and text boxes are already flattened into absolute page coordinates by the one implementation that owns those transforms) to the same quantity, and diffs within a **±40-twip (2pt)** tolerance band. Page count is exact; page size, each content-bbox edge, and the excluded extent are within tolerance. Every out-of-tolerance edge is reported by name.

- **Residual and why the tolerance is 40 twips.** With both sides measuring the text region over the pinned-parity lines, the corpus fixture's residual is `x0` 2, `y0` 6, `x1` 0, `y1` 5 twips. Two effects account for all of it, and the tolerance is sized to bound *them*, not to clear today's numbers:
  - **PDF coordinate rounding, ≈2 twips** — LibreOffice writes text origins at 0.1pt.
  - **Half the face's line gap** — `parley` splits a face's line gap around the text box (half above the ascent) where LibreOffice puts all of it below the descent, so our baselines sit `lineGap/2` lower. For the bundled faces `lineGap ≈ 0.0327 em`, i.e. ≈0.016 em: 4 twips at 12pt, 8 at 24pt, 16 at 48pt, scaling linearly with the largest font on the page.

  40 twips therefore holds for any bundled face up to ≈115pt with ~5× headroom over the edges observed when it was set (the `lineGap/2` term is now fixed, so the real headroom is larger), while staying far below the errors worth catching (a one-line vertical slip at body size is 276 twips). A fixture that needs more must record *why* here — raising the number to turn a red edge green is not a fix. The `lineGap/2` term **was** a real fidelity difference and has been closed in the box model (§H2b) rather than absorbed here, which is what "belongs in the shaper, not in this tolerance" meant.

- **Armed, and asserted to be armed:** all six references are committed, so the content comparison is live. A fixture with no committed reference is still **skipped** — that is what let the harness land before the oracle had run — but `the_oracle_gate_is_armed` now fails the build if any fixture in `ORACLE_FIXTURES` lacks a trustworthy, current-schema reference, and if `.github/workflows/ci.yml` stops running the gate by name on pull requests. The skip path can no longer become the normal path without someone noticing. A reference at an *older* schema is compared only on page count and page size — the quantities no schema bump has changed — and the test prints the re-bless instruction, so a semantics change degrades the gate loudly instead of reddening main or being quietly hand-patched.
- **Hermetic main CI + reviewed re-bless:** the everyday gate is `.github/workflows/ci.yml`, job `test`, step "Oracle geometry gate (docs/94 H2, FID-P-01)" — `cargo test -p casual-doc-render --test oracle_geometry`, no LibreOffice and no network, on every pull request. `.github/workflows/oracle-geometry.yml` is a manual (`workflow_dispatch`) job that installs a pinned LibreOffice and **only** the bundled metric-compatible faces (Liberation/Carlito/Caladea — the font-parity crux), regenerates the references, and opens a PR whose geometry diff a maintainer reviews. The everyday CI stays hermetic (no LibreOffice, no network) and just compares against the committed references.
- **Where the references were blessed, stated plainly:** the committed set was produced with LibreOffice 26.2.4.2 on **macOS/arm64** (its app bundle ships the Liberation/Carlito/Caladea faces, and each reference's `fonts` records that those are what it embedded), because that is the pinned build that was actually available. The re-bless job regenerates them on Linux with the same pinned build and now prints the diff against the committed set, so its first run is also the first measurement of whether the oracle is platform-stable. If the two disagree beyond the band, the honest answer is to bless on one platform and say so in `fixtures/oracle/README.md` — or to generate references in CI per run — not to widen the tolerance. Our own side of the comparison is not platform-sensitive between Linux and macOS: it shapes with bundled faces and is already pinned identically on both by the H1 golden.
- **Platform:** the geometry comparison is pinned to Linux/macOS and skipped on Windows, whose text stack shapes differently (the same reason H1 is Windows-gated — see the H1 test and PR #316).

## H2b — the arbitrary-document comparison harness (`opendoc-fidelity compare`)

H2 gates six fixtures. It cannot answer *"the customer says text boxes look
wrong"* or *"our page 3 holds fewer words per line than LibreOffice's"*, because
those are asked of documents that are not in the corpus and never will be. Until
this existed, the only way to answer them was to render two PNGs and look at
them, which cannot tell a regression from a taste difference and cannot say by
how much.

```sh
cargo run -p opendoc-fidelity -- compare <file.docx> [--tolerance TWIPS] [--page N]
```

One command, two renderers, differences as **measurements**: page count, per-page
text extents with a signed per-edge delta, comparable line counts, per-line bottom
edges and right edges, words per line, and the faces each side actually resolved.
`--page N` adds a side-by-side dump of that page's lines so the first line at
which the two orders stop describing the same text is visible.

**One mechanism, two consumers.** Our side of the reduction lives in
`casual_doc_layout::text_region` and is used *both* by this harness and by the H2
gate (`crates/casual-doc-render/tests/oracle_geometry.rs`, whose `page_geometry`
is now a three-field projection of it). Two implementations of one rule diverge
invisibly — the gate would keep passing while the harness reported different
numbers for the same page.

**Two implementations of the oracle reduction, pinned to each other.** The
blessing path stays `scripts/oracle/extract-geometry.sh` (Python, page-level, what
`fixtures/oracle/*.geom.json` holds). The investigation path is
`tools/opendoc-fidelity/src/oracle.rs` (Rust, line-level, no committed reference).
They are held together by `the_rust_reduction_reproduces_the_blessing_scripts_numbers`,
which runs the Rust reducer over a committed sample of the script's own inputs
(`fixtures/oracle/samples/real-producer-hyperlinks.{bbox.html,fonts.txt}`) and
asserts it reproduces the script's committed output
(`…reduced.json`). It needs neither LibreOffice nor Python, so it runs in ordinary
CI, and it goes red the moment either reduction's semantics move.

### ONLYOFFICE: what is and is not possible

The obvious third column cannot be automated the way LibreOffice is. Their web
client cannot open a file at all without a server — format I/O is the native `x2t`
binary and `core/X2tConverter/build/` ships only `Android/` and `Qt/`, so there is
**no WASM build** and nothing in a browser converts a `.docx`. Automating them
means standing up Document Server (AGPL-3.0, Docker, a conversion API), which is a
different kind of dependency from "run a binary over a file".

What *is* possible: that server's conversion endpoint produces a PDF, and the
reducer takes `pdftotext -bbox` output rather than LibreOffice specifically — so
adding ONLYOFFICE is a matter of supplying the PDF, not of rewriting the
comparison. Until somebody stands that server up, this harness has **one**
reference and says so rather than implying three.

### What arming it measured, immediately

- **FID-L-21 is reproducible from the repository.** `real-producer-rich` `y1`
  +263 and `real-producer-table-merges` `y1` −55, identical to the deltas
  registered in `KNOWN_DIVERGENCES`. The tracker row said this signal was "not
  reproducible from the repo"; it is, twice over — by the gate and by this
  command.
- **`rich`'s +263 has a shape, not just a size.** The first two lines agree to
  within 8 twips on every edge; everything after the nested table does not. The
  oracle's third band is 585 twips tall (`1165..3855 / 2270..2855`) against our
  266, so its word grouping merged content we report as two lines — the "5 vs 4"
  comparable-line count is partly that, and is **not** by itself evidence of an
  extra line. What the dump does establish is that the divergence begins at the
  nested table and that our content below it ends 263 twips lower. The page-level
  bbox could only ever report the +263.
- **Two engine defects of the same kind, found by the instrument measuring
  itself.** A `GlyphRun` built with `ascent`/`descent` of zero means "use the
  line's" — and the line's are the metrics of the *tallest* run sharing it, which
  is the exact thing per-run metrics exist to prevent. Two run kinds were built
  that way:
  - a **tab leader** (`tabs.rs`), deliberately and correctly, because a leader has
    no shaped face of its own. The reduction now treats a leader as pinned by its
    face and vertically neutral instead of unmeasurable. Before that, every line
    carrying one dropped out of the comparison: on a real 15-page agreement, that
    was all **14 lines of its table of contents** — a whole page invisible while
    the harness reported nothing wrong.
  - a **recomputed field** (`flow.rs`, `shape_field_run`), where the metrics were
    *already computed two statements above* and simply not carried onto the run.
    Fixed: an 8pt `PAGE` field beside 28pt text now reports its own extent rather
    than its neighbour's — which is also a latent caret-height defect, since
    `GlyphRun::ascent` exists precisely so a caret is as tall as the text at the
    insertion point. Guarded by
    `a_recomputed_field_run_carries_the_faces_metrics_not_zero`.

  Together these took a real document's per-page bottom-edge disagreement from
  **−869…−2268 twips to +10** (the footer), because the footer is what the
  excluded field run had been hiding.

- **A word count that was wrong on every multi-run line.** A box's `x0`/`x1` are
  the pen positions of its first and last *non-space* glyphs, so the whitespace
  between two runs survives in neither box's flags, and concatenating them fused
  the last word of one to the first of the next. On the footnote corpus fixture
  that was every single line, each reporting one word fewer than the oracle. A
  finding that fires constantly is noise, and noise is how a gate gets ignored.
  The discriminator is exact rather than tuned: whitespace (or a tab, or a cell
  boundary) advances the pen past `x1`, so the next box starts strictly to its
  right, whereas a run split for formatting mid-word leaves it starting exactly at
  `x1`. Findings dropped from 6 to 2 on `footnotes`, 4 to 1 on `table-merges` and
  4 to 1 on `table-list` — and what is left on the last two is exactly FID-L-21's
  −55 and −60.

### The first engine defect the instrument settled: `Calibri Light`

The open question this lane was pointed at was a customer document whose
paragraphs wrapped differently from LibreOffice's rendering of the same file, with
font substitution as the working hypothesis. The document is under NDA and neither
it nor anything derived from it is in the repository;
`fixtures/corpus/synthetic-declared-family-substitution.docx` reproduces its
*shape* instead — one Latin-only paragraph per declared face, same text, same
size, **left-aligned**, with `word/fontTable.xml` declaring each face's
`w:family`. Left-aligned matters: the real document's `Normal` style is
`w:jc="both"`, and justification erases the natural width, leaving only the word
count to differ.

Measured on that fixture, before and after (the "before" figure re-measured from
the final fixture by removing the fix, not carried forward from an earlier one):

| | comparable lines ours/reference | worst line right-edge Δ | findings |
| --- | --- | --- | --- |
| before | 9 / 10 | 965 twips | 10 |
| after | 10 / 10 | **21 twips** over an 8,900-twip measure | 2 |

Every line's word count now matches, line for line, and the two remaining
findings are the *same* offset on `y0` and `y1` (+44 each) — the whole text block
sits 44 twips low with its internal geometry identical, which is the page-top
spacing signal below and nothing else. The `Calibri` and `Carlito` control
paragraphs agreed to within 21 twips *before* the fix as well — metric
substitution works — which is what isolated the cause to one paragraph.

The fixture deliberately carries **no** Times New Roman paragraph even though the
real document declares one. Every face in it must be absent from both renderers'
hosts or the comparison stops measuring substitution: macOS has the real Times New
Roman, so this engine (OS font fallback on, as the native build ships it)
correctly uses it, those lines then leave the comparison for want of font parity
while LibreOffice keeps them, and the report fills with findings about the
measuring machine. A face the host happens to own is not a control.

**The harness must measure the engine the product ships.** `opendoc-fidelity`
takes `casual-doc-layout` with `system-fonts` on for exactly that reason. Without
it, the mixed-script corpus fixture's CJK fell back to a *bundled* Liberation
face, which the font-parity filter counts as pinned, so the harness compared a
line of `.notdef` boxes against LibreOffice's real CJK face and reported a
521-twip "divergence" that was nothing of the kind. With it, that fixture reports
**no differences beyond 40 twips** and names the interned faces as
`dynamic(#…)` so they are visibly not bundled.

**The cause.** `known_family` keys on exact names, so every *variant* of a
partnered family fell through to `classify_generic`, whose substring heuristic
reads only `mono`/`sans`/`serif`. `Calibri Light` contains none of them, and the
document declares it `<w:family w:val="roman"/>` — so a variant of a **sans**
family resolved to **Liberation Serif**, and the paragraph wrapped in three lines
where LibreOffice (which substitutes Carlito) took four.

This corrects the scope of the earlier fix, not its direction. Classifying a
missing face from `w:family` rather than from its name was right, and is what
made the page count match at 15; it simply must not outrank knowledge of the
*family* a name belongs to. `known_family_variant` now trims trailing words until
the remainder is a known name, so `Calibri Light`, `Cambria Math`, `Arial Nova`
and `Times New Roman PS MT` all follow their family. The result is
`SubstituteKind::Generic`, **not** `MetricCompatible`: only the base name's
metrics are known, so a Light or Condensed cut is a better guess and still a
guess, and it keeps reporting as a fallback loss.

### Closed with it: cell borders occupied no vertical space (FID-L-21)

The first fidelity gap the instrument closed, and the cause was one rule missing
in one place. A cell's horizontal borders reserved **nothing**: the engine already
pays for a *paragraph* border's band (Word: `BaseLineOffset += Brd.Top.Space +
Brd.Top.Size`) and charged nothing for a cell's, so every bordered row came out
short by its borders' thickness and the error accumulated down the page.

Isolated on synthetic probes rather than reasoned about. Against the pinned
LibreOffice, the shortfall was exactly twice the authored border width per row for
`single` at `w:sz` 2, 4, 8, 16 and 24, and six times it for `double` at 2, 4 and
8 — so a `double` edge occupies 3× its authored width. Single-edge probes showed
the top edge alone pushes content down by its full width, the bottom edge alone
grows the row without moving content, and left/right edges do nothing vertically.
A three-row table grew by 80 twips where three independent rows would have needed
120, so each **collapsed** edge is paid for once: a cell pays for its own top
always and its bottom only in the table's last row, which charges a shared
boundary to the lower row and the table's perimeter to the first and last.

| | before | after |
| --- | --- | --- |
| `real-producer-table-merges` y1 | −55 | **+5** |
| `real-producer-table-list` y1 | −60 | **−15** |
| 12 border probes, worst y1 | −115 | **+5** |

Both fixtures' `KNOWN_DIVERGENCES` entries are gone, and the gate itself forced
the review: it went **red** when their registered divergences stopped diverging,
which is the property that registry was built to have.

`rich` is not closed, and its residual is now isolated and is a different thing:
LibreOffice **discards** the paragraph ECMA-376 §17.4.66 requires after a nested
table inside a cell. We keep it — as Word does, and as the caret needs, since it
is the only insertion point after a nested table — and that accounts for the whole
remaining +323 (a probe isolates +333, present only when the paragraph is empty
*and* directly follows a nested table, absent when it carries text or when no
nested table precedes it). Matching the oracle here would cost Word fidelity and
an editing anchor, so the entry stays registered with that cause rather than being
"fixed".

### Closed with it: leading was centred (CSS) instead of below the baseline (OOXML)

The largest single difference on the owner's real document, and the same defect as
the "4–8 twip `lineGap/2` residual" this document already described and deferred —
which turned out not to be a curiosity but the 1× case of a rule that scales with
line spacing.

`parley` places a line's baseline by the CSS half-leading rule: extra height split
evenly above and below. OOXML does not — Word and LibreOffice put leading
**below** the baseline. The box heights and the line-to-line pitch were already
right; only the baseline's position inside the box was wrong, so the error was
invisible in any gate that pins block rects (which is why the H1 golden did not
move — a real coverage gap in H1).

Measured on two-line paragraphs at 12pt Liberation Serif, natural box 276 twips:

| `w:spacing` | first line's top should be | before | after |
| --- | --- | --- | --- |
| (none) | margin | +5 | **0** |
| `auto` `w:line="240"` (1×) | margin | +5 | **0** |
| `auto` `w:line="360"` (1.5×) | margin | +74 | **0** |
| `auto` `w:line="480"` (2×) | margin | +143 | **0** |
| `atLeast` `w:line="480"` | margin + 204 | −199 | **0** |

Every line of every probe now agrees to the twip. The rule the measurements
support: leading goes below the baseline, and **only an `atLeast` floor's own
share goes above** it — `atLeast`'s extra is exactly the offset LibreOffice puts
the text at (480 − 276 = 204). `apply_line_rule` now returns a `LineBox` that says
where the baseline sits, so one function owns the whole box model instead of the
height coming from us and the baseline from the shaper.

On the owner's 15-page agreement (1.5-spaced `Normal` style): `y0` went from
**+132 on 13 of 15 pages to +53**, page 2 to **0**, and findings from 556 to 496.
On the corpus, `real-producer-hyperlinks` went from `y0 +6 / y1 +5` to
`−1 / 0`. It also removed an accumulating rounding error: on
`visual-containment` the first baseline of pages 3, 4 and 5 used to drift
972 → 975 → 978 and is now exactly 971 on every page, with a uniform pitch.

`exact` is deliberately **unchanged and still divergent** (−170 at `w:line="480"`).
LibreOffice appears to scale the natural ascent into the exact box proportionally,
but that is one measurement at one value — not enough to implement without
fitting, so it is recorded rather than guessed at.

The residual +53 on the real document is the **separate** Carlito question below:
LibreOffice uses `sTypoAscender` for faces whose `hhea` line gap is 0, we use
`hhea`, and none of the bundled faces sets `USE_TYPO_METRICS` to ask for it.

### Closed with it: cell borders on the horizontal axis

The counterpart of the vertical reserve, and the same physical fact: a border
occupies space, so a bordered cell's content box is narrower and its text wraps
earlier. It took none at all, so a bordered cell fitted text no other renderer
fits.

Measured against LibreOffice 26.2.4.2 on a single 6000-twip cell with zero
margins, reading the content box off a left-aligned and a right-aligned paragraph
inside it — the right-aligned one gives the box's right edge directly, which a
left-aligned paragraph cannot:

| cell's vertical borders | content narrows by |
| --- | --- |
| left + right, 120 twips each | 120 (60 + 60) |
| left only, 120 twips | 60 |
| right only, 120 twips | 60 |

So **half** of each edge, which is the collapsed-border model (only the inner half
lies inside the cell). Confirmed at `w:sz` 4/8/16/24/48 and for `double` at 4/8/16,
where it is half of the *total* 3× thickness. Every one of those eleven probes now
agrees to 3 twips — the same constant offset the no-border case has — against up
to +117 before.

The wrapping followed, which is the part that matters: a probe that wrapped
9/8/9/6 words against the oracle's 8/8/8/8 now wraps 8/8/8/8 too, and on
`visual-containment` one word repaginated across the page-4/5 boundary.

**The two axes take different shares, and that is the oracle's asymmetry, not a
simplification.** A horizontal edge charges the row its full thickness (measured:
a one-row table with 120-twip top and bottom borders grows by 240); a vertical
edge charges the content width half of its own. LibreOffice also narrows the box
**without moving its left edge** — a 120-twip left border does not shift its text
right at all, at any width tested — so the reserve comes off the width only and no
text moves horizontally. Physically the inner half of a collapsed left border does
overlap the first glyph and Word may inset instead; that is recorded as an open
question rather than invented, because only the width is measurable here.

### Measured and decided against: `w:lineRule="exact"`

Now pinned at **ten** data points rather than one, and the answer is why it is not
being implemented. LibreOffice places an `exact` line's baseline at exactly
**0.8 × the authored height**, from the box top:

| `w:line` | 200 | 300 | 360 | 480 | 720 | 480 @24pt | 720 @24pt |
| --- | --- | --- | --- | --- | --- | --- | --- |
| baseline / height | 0.80 | 0.80 | 0.80 | 0.80 | 0.80 | 0.80 | 0.80 |

Exact at five heights and **font-size-independent** — 12pt and 24pt give the same
ratio, so it is not derived from the face's metrics at all. A bare 4/5 that no
font explains is an implementation constant, and copying it would be fitting the
proxy rather than implementing the format. Word's own rule for fixed line spacing
is described as positioning the baseline from the box *bottom* by the font's
descent, which this is not. So the measurement is recorded and the behaviour left
alone until there is a Word-produced fixture to settle it (FID-P-02).

### Measured and found already correct: indents

Six probes — no indent, `w:ind@start`, `hanging`, `firstLine`, a numbered list
with a hanging marker, and a list whose paragraph overrides the numbering's
indent — all agree with the oracle to 2 twips with identical wrapping. So the
−290/−362 twip `x0` divergence on five pages of the customer document is **not**
indent handling, and it is **not reproduced** by any probe yet. Recorded as open
rather than guessed at.

### Signals this instrument has measured and not yet closed

Each is reproducible with one command; none is a judgement by eye.

1. **`hhea` ascent versus `sTypoAscender` for a zero-line-gap face, +44 twips at
   11pt, +53 at 13pt.** All that is left on the synthetic fixture, and all that is
   left of the real document's per-page header offset.
   On the synthetic fixture this is the *whole* of what is left: `y0` and `y1` are
   both +44, so the text block is translated down with its internal geometry
   identical — same 268-twip box height on both sides, same 269-twip line pitch.
   Only where the first baseline sits differs.

   The cause is measurable to the twip and was measured. Carlito (UPM 2048) has
   `hhea.ascender` 1950 and `OS/2.sTypoAscender` 1536; the difference is 414
   units, which at 11pt is **44.5 twips** against an observed **44**. Liberation
   Serif gives 405 units, 43.5 twips. Both sides' word boxes are
   `baseline ± win/hhea` metrics (268.6 twips predicted, 268 observed on each
   side), so the fonts agree; what differs is that we set the first baseline at
   `top margin + hhea ascent` while LibreOffice sets it at roughly
   `top margin + typo ascent`, which puts its word-box top 44 twips *above* the
   margin.

   **This one must not be "fixed" toward the oracle without deciding what is
   right.** LibreOffice is a layout *proxy* chosen in `docs/46`; Word is the
   compatibility reference, and Word's line layout is conventionally described as
   `hhea`/`usWin`-based — the metric we already use. So this may be a case where
   we are closer to Word than the proxy is, and matching LibreOffice would make
   us *less* compatible while making this report greener. It also moves
   `geometry_snapshot.golden` and every page of every document. Open as a
   question with its evidence, not as a defect.

   The customer document's **+132** is recorded separately and is *not* this: its
   first text on those pages is running-header content at a different size, and
   132 twips does not fall out of the same ratio there. It needs its own
   reproduction before anything is claimed about it.
2. **Table-of-contents line pitch, −36 twips per row.** Ours 499, LibreOffice's
   535, accumulating to 471 twips over 14 rows on the customer document's
   contents page. Visible only because the leader fix above unblinded those
   lines, and measured from their tops: ours 1834 → 2333, the oracle's
   1837 → 2372, with identical 266-twip line boxes on both sides.

   Read the *positions* on a contents row, not the word count. The two renderers
   tokenise a run of leader dots differently, so a leader line's word count
   differs by construction — we report one more than the oracle there — and that
   is a property of the instrument, not a fidelity gap.
3. **Left indent, −290 and −362 twips**, on five pages of the customer document:
   two paragraph shapes where our text starts left of LibreOffice's, one of them
   left of the body margin entirely (a hanging list marker LibreOffice does not
   hang).
4. **Comparable line count**, ours short by 1–3 on five pages. Remains after the
   substitution fix, so it is not that; the per-line dump localises it.
5. ~~**Left and right cell borders take no horizontal space either**~~ —
   **fixed**, see below.

## Open questions

1. ~~Geometry extraction from the oracle~~ — **resolved:** auto-extract from the LibreOffice PDF via `pdftotext -bbox` (see §H2). Per-*block* correspondence between the two renderers is unstable, so H2 compares **page-level** geometry (size + content bbox); per-block diffing is deferred unless a stable correspondence is found.
2. Perceptual metric: tolerance-banded pixel-fraction is the floor; is SSIM worth the dependency, or is pixel-fraction + geometry enough? (Leaning: start without SSIM. The H2 image-diff gate is not yet built — geometry lands first.)
3. Corpus location and licensing of any real-world sample docs (must be redistributable to live in-repo). H2 starts on the already-vetted `real-producer-libreoffice.docx`; expanding to the user's complex corpus (H3) still needs a redistribution check.
4. **Is the oracle platform-stable?** The committed references were blessed with the pinned LibreOffice 26.2.4.2 on macOS/arm64; the re-bless job runs the same build on Linux. Nobody has yet measured whether the two agree inside the 40-twip band, because that job has never run. It now prints the diff against the committed set, so its first run answers this. If they disagree, the choices are: bless on one platform and document it, or stop committing references and generate them in CI per run (hermetic-ness lost, flap risk gained) — not a wider tolerance.
5. **Corpus coverage vs. the pinned font set.** `real-producer-libreoffice.docx` spends its last paragraph on CJK + Arabic, which the pinned faces do not cover, so a quarter of its lines are excluded from the geometry comparison (see §H2 *Font-parity scoping*). A small Latin-only fixture exercising the geometry-subtle cases (page-top spacing, numbering tab-suffix, `w:hideMark`) would give H2 real coverage; the alternative — adding CJK/Arabic faces to the oracle container — is *not* equivalent, because the engine's bundled set would still have to match them face-for-face.
