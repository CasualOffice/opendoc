# 113 — Windowed layout: laying out only the pages someone is looking at

**Status:** Design. **Opened:** 2026-09-20. **Owner:** unassigned.
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
2. Split the measure tier out of `BlockFragment`; prove page counts identical on the
   existing corpus.
3. Checkpoints + resumable `paginate_from`; prove page-for-page equality with a full
   paginate.
4. Byte-budgeted LRU on `GalleyCache`; wire the window to the scroll position.
5. Re-measure, and only then move `MAX_VIEWER_BLOCKS`.

Steps 2 and 3 are each independently testable against a full paginate, which is what
keeps this from becoming one unreviewable change.

## 7. Known unknowns

- Whether the measure tier can be derived without shaping twice for scripts needing
  full shaping to know line breaks (complex scripts, `w:kinsoku`). If it cannot, the
  open-time cost is a second pass over the window only — acceptable, but measure it.
- Footnotes and `eachPage` note renumbering run a bounded fixed point over *pages*
  (`NOTE_PAGE_RESTART_PASSES`), which assumes all pages exist. A document with
  `w:numRestart="eachPage"` may have to fall back to full pagination; `paginate_document_cached`
  already takes that fallback and it is the honest precedent.
- Anchored floats resolve onto pages in a post-pass. Whether that pass is checkpointable
  or must run per window is unresolved.
