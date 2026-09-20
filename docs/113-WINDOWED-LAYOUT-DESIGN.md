# 113 — Windowed layout: laying out only the pages someone is looking at

**Status:** Steps 1-5 landed in the engine and the host now holds a window (§8);
`MAX_VIEWER_BLOCKS` moved 262,144 → **700,000** on a browser measurement, and the
owner's 1,303,306-paragraph file now opens in the browser (2,476 MB, §8.4) but is still
refused because the host cannot scroll to all of it (§8.4-8.5). **Opened:** 2026-09-20.
**Owner:** unassigned.
**Row:** `109` **HF-162** (P1, L). **Depends on:** HF-161 / HF-163 (`docs/111`).
**Supersedes:** `docs/111` §4 "stage 2", which sketched this and listed four open
questions. Those four are answered here.

## 1. What this has to achieve

A 41 MB file with **1,303,306 paragraphs** is refused. `docs/111` measured why: a
paragraph costs **14.4 KB** of resident memory once modelled, shaped and paginated, so
`MAX_VIEWER_BLOCKS` at 262,144 is ~3.8 GB — the wasm32 ceiling — and that file projects
to 18.7 GB.

The model half is being dealt with separately (HF-161 landed, HF-163 outstanding). This
document is about the other **12.8 KB**: 4.7 KB of shaped galley and 8.1 KB of paginated
layout, per paragraph, for the entire document, built before the first page is shown.

The goal is not to make that smaller. It is to make it **non-resident**: the model stays
whole, the *rendered form* of it does not.

## 2. A thing that already exists, and does not solve this

`crates/casual-doc-layout/src/incremental.rs` already defines `PageRange`,
`ViewportLayout`, `VisiblePage`, `viewport_of` and `paginate_viewport`.

Two facts about them, both load-bearing:

1. **They do not bound memory.** `viewport_of` windows an *already-computed*
   `PaginatedLayout`; `paginate_viewport` calls `paginate` over the *whole* galley and
   then windows the result. Both require the full 12.8 KB/paragraph to exist first. Their
   own doc comment is accurate about this — "the viewport bounds the **downstream** cost
   — composition and paint run only for the returned pages" — and downstream cost is a
   real thing to bound. It is not this thing.
2. **They have no consumer.** `grep` for all four names across `crates/` returns zero
   references outside the file that defines them. Built and unreachable — `docs/105` §9
   rule 4, the most expensive recurring pattern in this repository.

So: do not read the presence of a "viewport" API as this work being half done, and do
not delete it either. Wiring it up is worth doing on its own merits, as a separate
smaller row, because painting fewer pages is cheaper than painting all of them even when
they are all in memory.

### 2.1 What step 4/5 decided about them

Split, deliberately, rather than kept or dropped wholesale:

- **`PageRange`, `VisiblePage`, `ViewportLayout` — reused.** They are the right shape
  for the scroll seam, and they now have a consumer: `windowed::window_of` returns a
  `ViewportLayout`. Two paths can produce one, which is the point — a document the
  windowed driver refuses still scrolls through the same type, so a host has **one**
  scroll seam whichever path built the pages.
- **`viewport_of` — kept**, unchanged, as that second path: window an already-resident
  layout when the document is small enough to hold whole.
- **`paginate_viewport` — removed.** It paginated the entire galley and then returned a
  slice of the result, so asking it for one page cost a whole document's paint tier. It
  had no consumer and its name invited exactly the misreading this section warns about.
  `window_of` is what it looked like it was.

## 3. The two facts that make this tractable

### 3.1 Pagination is already resumable, it just has no resume API

`paginate.rs` states the invariant plainly: pagination is a **forward fill**, and **each
page begins at a fresh content-top cursor**. That is why the incremental path can reuse
every page above an edit verbatim.

The consequence: the state needed to start paginating at page *N* rather than page 0 is
small and already exists as `Paginator`'s own fields at a page boundary —

| field | what it is |
| --- | --- |
| `at: FlowPos` | flow position of the next content to place |
| index into `fragments` | where in the galley that position sits |
| page number | for `Page::number` and page-number fields |
| `current_table: Option<NodeId>` | the table whose rows are being placed, if one spans the boundary |
| `table_headers: Vec<BlockFragment>` | that table's repeated `w:tblHeader` rows |

Call that a **checkpoint**. It is a few dozen bytes except when a table spans the
boundary. Keeping one every *K* pages makes any page reachable by paginating forward
from the nearest checkpoint at or before it — bounded work, no full pass.

### 3.2 Pagination needs heights, not glyphs

The paginator asks fragments how tall they are and where they may break. It does not
read glyph runs. But a `BlockFragment` today carries its `LineLayout`, and a `LineLayout`
carries the `GlyphRun`s — 96 B each — which is where the 4.7 KB/paragraph lives.

So the galley splits in two:

- **Measure tier** — per fragment: total height, per-line heights, break opportunities,
  keep-with-next/keep-lines flags, and the node id. No glyphs. Resident for the whole
  document. Order of ~100 B/paragraph rather than ~4,700.
- **Paint tier** — the full `LineLayout` with glyph runs, for the fragments in the
  window only. Evictable.

The measure tier is what makes exact page count affordable, which is the question
`docs/111` could not answer.

## 4. The four open questions from `docs/111` §4, answered

**Q1 — page count before full layout.** Exact, not estimated. Build the measure tier for
the whole document at open (one shaping pass, glyphs discarded as they are produced),
paginate over it to get the true page count and the checkpoint table, then discard
everything but heights and checkpoints. `Page X of Y`, the scrollbar and the status bar
are all correct from the first frame, and no estimate ever has to be revised under the
user. The cost is one shaping pass — which is what open already pays — at a fraction of
the peak memory.

**Q2 — resumable pagination.** The checkpoint of §3.1, stored every *K* pages. Start
value: K = 64, tuned against measurement, not taste. A table spanning a checkpoint
boundary carries its header fragments in the checkpoint; if that proves heavy, the
fallback is to refuse to place a checkpoint inside a table and let the interval stretch.

**Q3 — eviction.** LRU over paint-tier fragments, budgeted in **bytes, not pages**,
because page cost varies by two orders of magnitude between prose and a dense table.
Start with a budget around 256 MB and a resident window of the visible pages plus one
screenful either side. Dragging a scrollbar across 60,000 pages must not thrash: coalesce
scroll to the settled position and only build the window the user lands on, exactly as
the existing debounce-on-scroll behaves.

**Q4 — one mechanism with `GalleyCache`, not two.** `GalleyCache` already keys shaped
fragments by `NodeId`, tracks liveness, and drops fragments whose paragraph left the
document — it is already a cache with an eviction story, and it already backs the edit
path (`paginate_document_cached`: 477 ms at 200k paragraphs against 4,471 ms for a full
re-pagination). It becomes the paint tier, gaining a byte budget and LRU order. The
measure tier is a new, non-evictable sibling. `DirtySet` invalidates both. There is no
second cache.

## 5. What has to stay true

- **Identical output.** A windowed layout of page *N* must equal page *N* of a full
  `paginate_document`, field for field. That is the property the incremental path already
  asserts with its `incremental_equals_full_*` goldens, and the same shape of test
  applies here. It is the only defence against a subtle divergence that shows up as a
  document that paginates differently depending on where the user scrolled.
- **Editing stays light.** `docs/107` §4: per-keystroke work is O(1) in document size.
  Windowing must not add a full pass to the edit path.
- **The refusal path stays.** There is still a ceiling; it must still say what was found,
  what the limit is, and what to do. `MAX_VIEWER_BLOCKS` moves **when the measurement
  moves**, not when this design lands.
- **No silent data loss** and no dependence on the DOM as source of truth (AGENTS.md).

## 6. Sequencing

1. HF-163 — box the large enum variants, so the resident model is affordable at all.
   **Landed** (#554): model 1,452 B → 1,004 B per paragraph.
2. Split the measure tier out of `BlockFragment`; prove page counts identical on the
   existing corpus. **Landed** — see §6.1.
3. Checkpoints + resumable `paginate_from`; prove page-for-page equality with a full
   paginate. **Landed** — see §6.2.
4. Make the flow engine emit measures directly, and materialize one window's
   paint tier from a checkpoint. **Landed** — see §6.3.
5. Byte-budgeted LRU on `GalleyCache`, scroll coalescing, re-measure, and only
   then move `MAX_VIEWER_BLOCKS`. **Engine landed, host wiring open** — see §6.4.

Steps 2 and 3 are each independently testable against a full paginate, which is what
keeps this from becoming one unreviewable change.

### 6.1 Step 2 as landed — one paginator, two tiers

The dangerous way to build a measure tier is to write a second, cheaper paginator for
heights: a document then paginates differently depending on which one answered. Instead
the paginator is now **generic over a `Paginable` trait** (`crates/casual-doc-layout/src/measure.rs`),
and both tiers implement it. There is one walk, one set of break rules, one row/paragraph
splitter. The tiers differ only in what a *placed fragment* is and what a *finished page*
is: a `PlacedFragment`/`Page` for the paint tier, a `NodeId`/`PageOutline` for the measure
tier. `PageOutline` is `Page` minus its painted content — same number, section, page size,
content area, model start/end and flow span — so "the tiers agree" is a field-for-field
`assert_eq!`, not a judgement call.

`Paginable`'s method list is the proof surface for §3.2's claim. It asks only for heights,
per-line heights and forced breaks, keep flags, node ids, row split flags and cell margins.
If a glyph-bearing accessor ever has to be added to it, the measure tier has stopped being
sufficient and §3.2 has to be revisited.

**Measured, with the committed `crates/casual-doc-layout/examples/layout_footprint.rs`**
(macOS arm64, release, 100,000 single-run 130-character paragraphs at a 9,360-twip content
width — the same synthetic shape `docs/111` §2 used; each phase runs in its own child
process so the resident-set reading is clean). 4,546 pages in both tiers:

| resident | model only | + shaped galley + `paginate` | + measure tier + `paginate_measures` |
| --- | ---: | ---: | ---: |
| per paragraph | 1,003 B | **7,994 B** | **1,190 B** |
| the layout half alone | — | **6,991 B** | **187 B** |
| 1.3M-paragraph projection | 1.22 GiB | **9.70 GiB** | **1.44 GiB** |

The layout half is **37× smaller**; total resident is 6.7× smaller. Stable across sizes
(1,005 / 8,021 / 1,136 B per paragraph at 200,000). The 1,003 B model figure reproduces
`model_footprint.rs`'s 1,004 B independently, which is the cross-check that the probe is
measuring what it says.

Two honest qualifications:

- This is 7,994 B per paragraph, not `docs/111` §2's 14,420 B, for two reasons: the model
  half shrank (stage 1a/1b landed), and the probe measures `paginate`, not the full
  `paginate_document` driver with its running-content, float, note and field post-passes.
  The two numbers are not comparable line for line; the *ratio between the two tiers*,
  measured by one probe in one run, is the claim.
- **Resident is not yet peak.** The flow engine still emits `BlockFragment`s, so the
  production path builds the shaped galley and projects it. The probe's `measure` phase
  shapes in 4,096-block chunks and drops each chunk after projecting it, which is a valid
  stand-in only because the synthetic body has no cross-chunk flow state (no list
  numbering, no section breaks) — the phase asserts the chunked tier equals the
  whole-body tier before measuring. Making the flow engine emit measures directly, so the
  full galley is never built, belongs to step 4.

### 6.2 Step 3 as landed — what a checkpoint holds, and where one may sit

`Checkpoint` is `{ page_index: u32, at: FlowPos, current_table: Option<NodeId>,
table_headers: Vec<u32> }` — **80 bytes** measured, plus four per repeated header row; 71
checkpoints (5,680 B) for the 4,546-page probe document at K = 64. The header rows are
named by **galley index**, not copied, which keeps a checkpoint small and makes it valid
in either tier: the measure pass produces the checkpoints and the paint pass consumes
them, which is the whole point.

`paginate_from(fragments, config, checkpoint)` returns exactly
`paginate(fragments, config).pages[checkpoint.page_index..]`, field for field, page
numbers included.

**K stays at 64.** At that interval the checkpoint table is 0.005% of the measure tier's
resident cost, so nothing in this measurement argues for changing it; the number that
would move it is the cost of *replaying* up to K pages when the user lands on one, and
that is not measurable until step 4 wires the window to the scroll position.

**Where a checkpoint may sit** turned out to need a third condition beyond the two
`safe_resume_page` uses, and it was found by the equality test rather than by reasoning:

- `line == 0` and the predecessor does not keep with next — the two known ones. The
  keep-group condition is deliberately conservative; it is not established that every
  resume inside a keep-group diverges, only that the walk would re-form a *different*
  group, and refusing costs at most a checkpoint.
- **Not inside a row being cut across the boundary.** When `split_table_row` closes a
  page mid-row, the flow position it records names the *row*, not the chunk within it —
  `repeat_headers_if_needed` deliberately re-anchors `page_start` to the body row so the
  page's provenance stays on real content. Such a boundary therefore *looks* resumable
  (`line == 0`) and is not: restarting there places the whole row again instead of its
  tail. The paginator now marks that span (`mid_fragment`) and takes no checkpoint in it.

### 6.3 Step 4 as landed — the engine emits measures, and a window is materialized

Two halves.

**The flow engine writes into a sink.** `flow_blocks` used to append to a
`Vec<BlockFragment>`; it is now generic over `measure::GalleySink`, and both tiers go
through one `flow_body_into`. `MeasureSink` projects each fragment onto
`FragmentMeasure` and drops its glyphs as soon as the engine can no longer touch it.
"As soon as" is a stated contract rather than a hope: the engine mutates an
already-pushed fragment in exactly **one** place — the `w:contextualSpacing` collapse,
which zeroes the previous paragraph's space-after — so the sink holds two shaped
fragments and **panics** rather than silently mis-applying a collapse it can no longer
honor. `ReachSpy` in `tests/streaming_measures.rs` watches the real engine through the
same public seam and fails if it ever reaches further back than two.

This is what makes §3.2's claim true of **peak** and not only of resident memory, and
it retires §6.1's honest qualification: the 4,096-block chunked stand-in is still in the
probe as the `measure` phase, but the production seam is `build_measures_for_blocks`
and it carries no cross-chunk assumption at all, because there are no chunks.

**One window's paint tier.** `windowed::window_of` finds the nearest checkpoint at or
before the window, re-flows only the blocks the window needs (`flow_body_range`, a sink
that keeps the wanted galley range and drops the rest, over a block slice that stops one
block past the window — one, because the contextual collapse and the drop-cap pair each
reach exactly one block forward), and runs `paginate_from_based`, which is
`paginate_from` with the checkpoint translated into window coordinates on the way in and
the emitted `FlowSpan`s translated back out. At base zero it is `paginate_from`
identically, which is how the translation is asserted.

**Re-flowing from the middle is a classified decision, not an assumption.** Flowing is a
forward walk carrying list counters, the previous paragraph's style, wrap carries and
drop-cap pairing. Rather than snapshot that state — every field of which is a place for
two paths to drift — `measure_document` classifies the document: `FlowResume::AnyBlock`
when the body contains no numbering, no contextual spacing, no drop cap and no paragraph
style (so any block boundary reproduces the full build exactly), `FlowResume::FromStart`
otherwise, which re-flows from block zero into the same retaining sink — memory stays
bounded to the window, *time* is `O(document)` per window, and that cost is stated rather
than hidden.

**Measured**, with the committed `crates/casual-doc-layout/examples/layout_footprint.rs`
(macOS arm64, release; each phase in its own child process; **peak** is a 4 ms RSS sample
of that child, reported alongside resident because peak is what fails an allocation).
100,000 single-run 130-character paragraphs, 4,546 pages in every phase:

| 100,000 paragraphs | resident | peak RSS |
| --- | ---: | ---: |
| `model` — the document alone | 1,003 B/para | 103 MB |
| `full` — galley + `paginate` | 7,993 B/para | **830 MB** |
| `measure` — chunked stand-in (step 2) | 1,189 B/para | 147 MB |
| `stream` — engine emits measures (step 4) | 1,085 B/para | **136 MB** |
| `window` — measures + one 6-page window | 1,119 B/para | 133 MB |

The peak is the row that changed: **830 MB → 136 MB, 6.1×**, and the layout half of it
(net of the 103 MB model) is 727 MB → 33 MB, **22×**.

### 6.4 Step 5 as landed — byte budget, eviction, coalescing; and the constant did not move

**A byte budget, counted.** `BlockFragment::paint_bytes` counts a fragment's lines, glyph
runs, glyphs, inline bars/images/field markers and, recursively, its cells' blocks.
`GalleyCache::with_budget` holds a running total and, at the end of each build, evicts
least-recently-used entries until it fits; `WindowPolicy` applies the same accounting to
a window, charging the visible pages first and adding lead pages outward while the budget
allows. The default is the 256 MiB §4 Q3 names and a lead of two pages either side.
Eviction can never change an answer — an evicted paragraph is re-shaped — which is why it
is safe to apply to live entries, and the guard asserts a budgeted build produces the
same galley a fresh build does.

**Coalescing, without a clock.** `ScrollCoalescer` is a tick-driven state machine: a
position inside the built window asks for nothing, the first window is built immediately
(a settle delay on first paint is a blank page, not a saving), and a position that moves
resets the settle counter. The guard drags across 60,000 pages in 600 reported positions
with a tick between each and asserts **exactly one** window is built, on landing. It is
clock-free on purpose — `docs/105` records that clock-bound tests degrade under load
however sound the code is.

**`MAX_VIEWER_BLOCKS` did not move, and the measurement says why.** Measured on the
owner's own paragraph shape at its own size (1,303,306 paragraphs of the single line that
file repeats; the file is the owner's and is not in the repository, so the shape is
reproduced in the probe — `file(1)` calls it CRLF ASCII text, `sort -u` yields exactly
one distinct line and `wc -l` counts 1,303,305 of them):

| 1,303,306 paragraphs | `paginate_document` | `measure_document` + one window |
| --- | ---: | ---: |
| pages | 29,621 | 29,621 |
| per paragraph | 3,378 B | 1,021 B |
| resident | 4.10 GiB | 1.24 GiB |
| **peak RSS** | **4.14 GiB** | **1.23 GiB** |
| time | 8.6-34.6 s | 8.3-33.7 s |
| scroll to page 22,215 | — | 3-29 ms, 264 KB, 485 fragments re-shaped |

Both paths report **29,621 pages**, which is the cross-check that the measure tier agrees
with the full driver at this scale and not only on the corpus.

Timing is reported as a range because it is the noisy half of this measurement:
six runs on a shared 16 GiB laptop gave 8.3-38 s for the same work, while the
memory figures reproduced to within 0.4%. Both columns move together — the
windowed path pays the **same single shaping pass** the production path pays,
measured back to back at 34.6 s and 30.7 s under identical load. Windowing does
not make opening faster; it makes it fit.

The production column is the one to read carefully: **3,377 B/paragraph measured at
300,000, 600,000 and 1,303,306 paragraphs**, the same figure at all three, so the total
is a measurement and not an extrapolation. Under memory pressure its resident reading
*falls* (the OS reclaims), which is itself the symptom of a 16 GiB machine at its limit
— another reason the number that matters is the peak.

4.14 GiB does not fit a wasm32 address space. That is the measured reason the owner's
file is refused rather than merely slow, and 1.23 GiB is why the engine can now open it.
**But `casual-doc-wasm`'s `open_document_as` still calls `paginate_document`**, so the
browser still pays the left-hand column. The constant is a *measured* ceiling — "the
largest size actually measured to open" — so raising it now would trade an honest refusal
for `RuntimeError: unreachable`, the regression HF-158 exists for. What has to happen
first is recorded in §8.

## 8. The host side, as landed — and the ceiling that is left

### 8.1 What was owed

1. **`WasmDocument` must hold a window, not a whole `PaginatedLayout`.** **Landed** —
   §8.2.
2. **The browser-side open cost is a separate ceiling.** **Still true, and it is now the
   *second* ceiling rather than the first** — §8.4.

### 8.2 `WasmDocument` holds a `BodyLayout`

`crates/casual-doc-wasm/src/window.rs`. The field is now

```rust
enum BodyLayout { Whole(PaginatedLayout), Windowed(Box<WindowedBody>) }
```

and `WindowedBody` holds the `DocumentMeasures`, a `WindowPolicy`, the materialized
window, and the **absolute** page range that window covers.

**The surface really was narrower than the file's size suggests, but not for the reason
§8 gave, and the difference is worth writing down because the original claim reads as
broader than it is.** Checked rather than taken on trust: `grep -rn '\.placed' crates`
finds it in **eleven** non-test source files, not two — `paginate.rs` (52 references),
`columns.rs` (24), `document_layout.rs` (11), `notes.rs` (9), plus `line_number.rs`,
`anchor.rs`, `note_numbering.rs` and `windowed.rs`. What is true, and is what §8 meant,
is that every one of those is a **producer**: they are the pipeline that fills a page.
The only downstream **consumers** of a finished page's placed content are `compose.rs`
(one site) and `hittest.rs` (four), and `casual-doc-wasm` itself has exactly one.

The thing that actually made this tractable is a different one. `casual-doc-wasm`'s
30,000 lines reach the page list through exactly **three** accessors —
`painted_layout`, `editing_layout`, and now `body_page_at` — because an earlier defect
cluster (answering a pixel question from the layout that did not paint it) had already
forced that discipline. Twenty-six call sites, every one of them through one of the
three. That is what made this a reviewable change rather than a rewrite, and it is worth
protecting.

Three properties the type enforces:

- **`page_count` is the document's, always.** A windowed body answers from the measure
  tier. Reporting the window would tell a host with 25,556 pages that it had five, and
  the other 25,551 would not exist for the scrollbar, printing, `NUMPAGES` or the status
  bar.
- **`page_at(index)` is `Option`, and `None` means "not resident", never "empty page".**
  The callers that must not fail — `renderPage` — move the window first.
- **`pageSize(index)` does not move the window.** It is answered from the page's
  `PageOutline`. This is not an optimization, it is the difference between working and
  not: a host builds one sheet per page and asks each one's size *before* rendering
  anything, so routing that through the window re-flowed and re-paginated once per page
  of the document — **121.1 s to open 262,146 blocks, against 35.8 s for the whole
  path**. Fixed, the same open is 17.3 s.

### 8.3 Every all-pages consumer, and what it does now

The full table lives in `window.rs`'s module documentation, next to the code, and is
exhaustive by construction (three accessors, every call site is one of them). In
summary:

| kind | consumers | answer |
| --- | --- | --- |
| **Re-materialised** | `renderPage` (moves the window), `pageSize` (from the outline), print, thumbnails | every page of the document, in any order |
| **Answered from the measure tier** | `pageCount`, `documentStats().pages`, `NUMPAGES` | the document's real count, not the window's |
| **Unaffected — model-derived** | export (`exportDocx`, `exportAs`), find (`findText`), the accessibility mirror, word and paragraph counts | the model stays whole; these never read the layout |
| **Refused loudly** | every edit (one line at `apply_group`, the atomic choke point, so all 47 operations), the show-changes preview | a sentence naming the size, the limit and what to do |
| **Re-measured** | font registration → `repaginate` | one more shaping pass at the bounded peak |
| **Window-local by definition** | caret and selection rectangles, hit-testing, table and checklist chrome, object boxes, running bands | they convert between *painted pixels* and the model; they look pages up by `Page::number`, so they can never answer about the wrong page |

**Editing is refused, not approximated, and that is a deliberate product decision.**
A windowed body cannot re-paginate: `finish_edit` re-runs the whole document — the peak
this path exists to avoid — and re-measuring is a full shaping pass, 85-110 s on a
document this size, per keystroke. So a windowed document is **read-only**, and says so.
The alternative considered and rejected was to let the edit land and leave the layout
stale, which is the silent version of the same limitation.

**What "refused loudly" does not yet reach.** The engine's sentence stops at the host:
`webapp/src/edit_errors.mjs` maps every engine refusal to one generic line, *"That edit
isn't supported for this selection yet"*, which for a read-only document is actively
misleading — it sends the user to look at their selection. A getter,
`editingUnavailableReason`, now exists for the host to read so a control can be
**disabled with a reason** rather than look live and refuse (`SKILL.md` §10). Nothing in
`webapp/src` consumes it yet. Named here rather than left to be discovered
(`docs/105` §9 rule 4).

### 8.4 The constant moved: 262,144 → 700,000, and where it stopped

Measured **through the browser** with the committed probe
`webapp/tests/e2e/viewer-ceiling-measurement.spec.mjs`
(`MEASURE_VIEWER_CEILING=1 npx playwright test viewer-ceiling-measurement`; macOS arm64,
headless Chromium, release wasm, the owner's own 32-byte line repeated). "wasm" is
`WebAssembly.Memory.buffer.byteLength` after the open — linear memory never shrinks, so
that reading *is* the high-water mark, and it is the number that decides whether a
document opens at all. "last page" is whether the host can scroll to the final page and
find ink on it.

| blocks | path | open | wasm | JS heap | pages | last page |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| 65,537 | whole | 9.1 s | 359 MB | 38 MB | 1,286 | reached |
| 262,144 | whole | 26.3-42.2 s | 1,222 MB | 57 MB | 5,141 | reached |
| 262,146 | windowed | 17.3-31.2 s | **592 MB** | 33 MB | 5,141 | reached |
| 600,000 | windowed | 21.6-73.0 s | 1,230 MB | 67 MB | 11,765 | reached |
| **700,000** | windowed | 85.9-95.0 s | 1,314 MB | 74 MB | 13,726 | reached |
| 800,000 | windowed | 118.7 s | 1,442 MB | 91 MB | 15,687 | **NOT reached** |
| **1,303,306 — the owner's own file** | windowed | **110.5 s** | **2,476 MB** | 139 MB | **25,556** | **NOT reached** |

The last row is the owner's real 41 MB file, fed through the picker, not a synthetic
stand-in.

**Windowing is what moved the constant.** At the same block count the windowed path
holds **592 MB against 1,222 MB** and opens no slower, which is what makes 700,000
blocks — 2.7× the old ceiling — something a browser can hold.

**The owner's file now opens.** 1,303,306 paragraphs, 2,476 MB of linear memory inside a
wasm32 address space where the whole-layout path needed 4.14 GiB and could not be
attempted, 25,556 pages reported correctly, every one of them individually rasterizable
through `renderPage`, exportable, findable, printable.

**And it is still refused, for a reason that is no longer the engine's.** The viewer
builds one sheet per page, so its scroll container is pages × ~1,078 px, and a browser
stops scrolling at 2^24 = 16,777,216 CSS px. Measured: at 800,000 blocks the container
is 16,910,594 px and the final pages cannot be scrolled to; at 1,303,306 it is
27,549,376 px and the last third cannot. Admitting a document whose final third is
silently unreachable is the failure mode this whole document is written against, so the
ceiling is set at the largest size measured to open **and be wholly reachable**:
700,000.

Note what that ceiling is really a function of: **pages, not blocks.** A block count
cannot bound a page count — a document of long paragraphs makes more pages per block —
so 700,000 is the measured answer for this shape and not a proof for every shape. The
hazard is not new (262,144 blocks of one-page-each blocks was already past the browser's
limit), but it is now the thing in the way.

### 8.5 What is owed next, in the order it blocks the owner

1. **A virtualized scroll container in the host** (`webapp/src`). One sheet per page is
   what puts a 25,556-page document past the browser's scroll limit. Until it is fixed,
   no engine work raises the ceiling past ~15,500 pages. This is the single thing between
   the owner and their file.
2. **`editingUnavailableReason` has no consumer** (§8.3). A read-only document currently
   looks fully editable and then refuses with a sentence about the selection.
3. **Open is still slow and gives no feedback**: 110.5 s for the owner's file, linear in
   blocks, with no budget, progress or cancel. `docs/104` HF-077. Windowing did not
   change this and was never going to — both paths pay the same single shaping pass.
4. **`ScrollCoalescer` still has no host consumer.** `window_of` returns a
   `ViewportLayout` and the coalescer decides when to ask for one; the facade instead
   moves its window from `renderPage`, which is correct and is what the existing host
   drives, but it means a drag across 25,556 pages is bounded by the host's
   virtualization rather than by the coalescer built for it.

## 7. Known unknowns

- ~~Whether the measure tier can be derived without shaping twice~~ — **resolved, and
  the answer is that the question was the wrong one.** The worry was that a
  complex-script paragraph's line breaks are only known after full shaping, so a measure
  tier built without retaining the shaped form would have to shape twice. It does not,
  because step 4 does not avoid shaping — it avoids *retaining*. `MeasureSink` receives
  the same fully shaped `BlockFragment` the paint tier would have received, from the same
  shaper, and discards its glyphs one fragment later. So the height of a complex-script
  paragraph is exactly the height the paint tier computes, for every script, at exactly
  one shaping pass. What a window then pays is a **second** shaping of the paragraphs it
  paints — 485 fragments out of 1,303,306 in the measurement above, 3 ms — and that is a
  per-window cost, as predicted, not a per-document one.
- ~~Footnotes and `eachPage` note renumbering~~ — **resolved by refusing, which is the
  precedent the question itself named.** `measure_document` returns
  `NotWindowable::NotesRestartEachPage` when `resolve_note_labels(...).restarts_each_page()`,
  exactly as `paginate_document_cached` does, and `NotWindowable::BodyFootnotes` when the
  body references a footnote at all — a footnote reserves a band at the bottom of the page
  it lands on, so the content height the measure pass filled is not the height that page
  had. Endnotes are **not** refused: their bodies are appended to the flowed block
  sequence exactly as `build_section_runs_inner` appends them, and the resolved endnote
  list is computed once at open rather than per window (recomputing it was a full-body
  walk on every scroll, which measured at 0.4 s on a 100,000-block document before it was
  cached).
- ~~Anchored floats~~ — **resolved by refusing, and the reason is worse than the one the
  question anticipated.** It is not only that `place_floats` resolves an anchor against
  the page its paragraph landed on while carrying a document-global z-order counter.
  It is that `finish_pagination` runs a **bounded fixed point that re-flows the whole
  body** against computed wrap exclusions, so an anchored float can move page boundaries
  — which means the measure tier's page list would be wrong, not merely incomplete.
  `measure_document` therefore returns `NotWindowable::AnchoredFloats` if the document
  anchors an object anywhere: any depth of the body, or any header/footer part.
  `anchor::document_has_anchored_object` is a deliberate **superset** of what the float
  passes act on (it ignores `behind_doc`, the wrap mode and the anchor kind, and descends
  into cells and content controls that `body_wrap_rects` does not), so `false` proves both
  passes inert while `true` proves nothing. Margin line numbering is refused for the
  related reason that `place_line_numbers` runs a counter across pages.
- ~~`paginate_from` reproduces the paginator's output, not the driver's~~ — **resolved
  by splitting the driver's post-passes into page-local and not.** `window_of` runs the
  four that are page-local given a page's absolute number — section `w:vAlign`, running
  content, `w:pgBorders`, and `PAGE`/`NUMPAGES` resolution — so a windowed page is a
  finished page and the test compares it to a full `paginate_document`'s page *field for
  field*, glyphs and header and footer included, rather than comparing boundaries.
  `resolve_fields_labeled_with_total` takes the total the measure tier supplies instead
  of `layout.pages.len()`, which is the specific trap: a window that paginated perfectly
  but printed "page 3 of 6" on a 29,621-page document would still be showing the user a
  wrong number. `page_number_label_at` computes one page's `PAGE` label in closed form
  rather than folding over every page before it (a fold would allocate 29,621 strings per
  scroll); it is asserted equal to the driver's running fold over 300 pages in five
  `w:pgNumType` configurations. The passes that are **not** page-local are the two above,
  and documents needing them are refused.
