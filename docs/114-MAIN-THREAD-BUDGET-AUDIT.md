# 114 — Every operation whose cost scales with the document

**Status:** audit complete; the three quadratic reads are fixed and guarded, the
rest is enumerated with measurements and sequenced. **Opened:** 2026-09-21.
**Row:** `109` **HF-077**. **Related:** `113` (windowed layout), `111` (model
memory), `107` §4 (the editing budgets).

## 1. Why this document exists rather than another hotfix

The owner reported a frozen tab three times in one session — opening a large
file, opening the **Outline** panel, and an ordinary **click** — and each was
being met with a fix for that one symptom. That is the reactive loop, and it
cannot terminate, because the three reports are not three bugs. They are three
instances of one rule being broken:

> **Anything the main thread does whose cost grows with the size of the document
> freezes the tab on a large enough document.**

So this is the enumeration, with a measured cost for each, and the shape each
one should have instead. A fourth report should be findable in this table rather
than in the owner's afternoon.

## 2. The two shapes, named before anything was fixed

**Shape A — a whole-document scan per node (quadratic).** The model has no id
index. Resolving a `NodeId` to its paragraph, cell, table or surface means
walking every block surface: `paragraph_properties`, `find_paragraph_any`,
`surface_of` and the twelve other by-id readers in `casual-doc-edit` all go
through `surface_block_lists`, which walks the body itself to find inline text
boxes. One of those per user action is fine. One per *node* is
`O(blocks²)` — and at 1.3M paragraphs that is ~1.7 × 10¹² block visits, which is
not "slow", it is "never finishes". This is the shape that produced two of the
three reports.

The textbook fixes are **indexing** (build the id → node map once) or **carrying
the node instead of its id** (one walk, node in hand). The second is what landed:
it needs no index to invalidate, and it makes the quadratic version
unrepresentable at the call site rather than merely slower.

**Shape B — one pass over the whole document per interaction (linear).** Not
quadratic, but still forbidden by `107` §4: per-interaction work is O(1) in
document size. A 200 ms pass is invisible at 2,000 blocks and is the interaction
at 1.3M.

The textbook fixes are **windowing** (only the visible part), **caching with
incremental invalidation**, and **moving the pass off the interaction** (compute
in the background with progress, or on demand).

## 3. Where the open time actually goes: import 0.83 s, pagination 7.78 s

Measured natively (release, macOS arm64) on the owner's own 41 MB,
1,303,306-paragraph file, because the split decides whether lazy pagination is
worth building at all:

| phase | time | share |
| --- | ---: | ---: |
| unzip + parse + normalize (the whole import registry) | **0.83 s** | 9.4% |
| shaper construction | 0.21 s | 2.4% |
| `measure_document` — the measure-tier pass over 1.3M paragraphs | **7.78 s** | **88.1%** |
| `window_of` at page 0 / 14,810 / 29,619 | 0.00 / 0.01 / 0.01 s | ~0% |
| **total** | **8.84 s** | |

Three things follow, and they settle the design question `113` §4 Q1 left open:

1. **Import is not the problem.** Producing the model for 1.3M paragraphs costs
   under a second. Whatever open does after that is the cost.
2. **The measure pass is essentially all of it** — and `113` §4 Q1 chose to run
   it over the *whole* document at open, deliberately, to make `Page X of Y`
   exact from the first frame. That choice is the 30–50 s open.
3. **A window costs 10 ms at any page.** Once the measures exist, showing page 1
   or page 29,619 is free. So the first screen does not need the other 29,620
   pages measured — only the count does.

Browser figures are ~2× these, and the split is a property of the work rather
than of the target. Reproduce with the committed artifact:

```
cargo run --release --example open-phase-timing -p casual-doc-wasm -- <file>
```

### 3.1 And the measure pass runs TWICE per open

Measured through the picker on the owner's file after the §6 fixes, with the
ceiling probe now reporting main-thread blocking (§8):

```
| 1,303,306 | opened | 17.2 s | blocked 17077 ms | tasks [17077, 16981, 114, 92]
| 504 MB | heap 67 MB | pages Page 1 of 25556 | scrollHeight 8,000,090 px
```

**Two tasks of ~17 s, not one.** The first is the open. The second is the web
fonts landing: `openBytes` paints from the bundled metric-compatible faces
first, then fetches the named families and calls `registerFonts`, and
`register_fonts_inner` ends in `repaginate()` — which for a windowed body
re-measures the **whole document** to keep the page count honest. So the reader
waits out the measure pass, gets their document, and then loses the tab for as
long again.

It is not fixable by skipping the re-measure: page boundaries depend on advance
widths, and reporting a count derived from faces that are no longer in use is
the silent-wrong-number failure. It is fixable by the same lazy measure §7
describes — rebuild the *window* now, re-measure in the background — which is
another reason that is the change to make rather than a second special case
here.

## 4. The class, measured

Engine costs are measured by calling the engine directly from the page
(release wasm, headless Chromium) on synthetic documents of N plain paragraphs;
host costs come from the same harness driving the real UI. "20k → 40k" is the
scaling probe: ~2× is linear, ~4× is quadratic.

### 4.1 Quadratic — fixed in this pass

| operation | reached by | 20,000 blocks | 40,000 | shape | now |
| --- | --- | ---: | ---: | --- | ---: |
| `documentOutline` | Outline panel, and every edit while it is open | 1,442.8 ms | 5,565.2 ms (3.86×) | A | **1.3 ms** |
| `objectAt` | **every pointerdown** | 1,436.9 ms | — | A | **0.7 ms** |
| `accessibilityTreeWindow` | every click and every keystroke | 33.7 ms | — | A (600 × scan) | **0.4 ms** |

At the owner's size the first row is ~1.6 hours of arithmetic for a document
with no headings in it. That is the "Outline froze the tab" report.

On the owner's real file, driving the real UI, the same three operations after
the fix — longest single main-thread task in brackets:

| operation | wall | longest task |
| --- | ---: | ---: |
| open (unchanged by this work) | 11.4 s | **11,322 ms**, then 9,422 ms |
| a click into the page | 240 ms | 132 ms |
| opening the Outline panel | 212 ms | 55 ms |
| opening the Pages panel | 382 ms | 227 ms |

Outline goes from "never" to 55 ms and a click from 1.4 s to 132 ms on a
1,303,306-paragraph document. Open is untouched, because open is §7.

### 4.2 Quadratic — found, not yet fixed

Each is `paragraph_properties` / `find_paragraph_any` / `locate_table_row` inside
a loop over nodes. Ordered by how easily a user reaches it.

| # | site | reached by |
| --- | --- | --- |
| Q1 | `can_continue_list` (`casual-doc-wasm` ~4877) | ribbon list state, **every selection change** |
| Q2 | `selection_paragraph_state` (~6617) | paragraph/style panel, every selection change |
| Q3 | `selection_format` (~4213) / `selection_run_style` (~8869) | two scans per selected paragraph, every selection change |
| Q4 | `apply_paragraph_props_as` (~10750) | any paragraph command over a multi-paragraph selection (⌘A → O(n²)) |
| Q5 | `apply_indent_props` (~10816) | indent/ruler; **two** scans per selected paragraph |
| Q6 | `decide_all_revisions` (~8552) + `paragraph_decision_ops` (~15800) | Accept All / Reject All, plus a whole-document clone |
| Q7 | `delete_comment` (~7259), `decide_move_pair` (~8460) | deleting a comment, accepting a move |
| Q8 | `copy_rich_runs_inner` (~11844) | ⌘C over a multi-paragraph selection |
| Q9 | `suggest_range_edit` (~8031/8040/8047/8104) | suggesting a deletion over several paragraphs |
| Q10 | `page_setup_sections` (~6497) | Page Setup dialog |
| Q11 | list renumbering family (`continue_list_inner`, `restart_list`, `set_list_format`) | list commands |
| Q12 | `create_style_from_selection` (~9153) | New style from selection |

All twelve are the same fix as §4.1 — carry the node, or resolve the set of ids
in one walk — and all twelve are covered by the guard in §6 the moment they are
added to its list.

### 4.3 Linear per interaction — the `107` §4 violation

| site | cost | when |
| --- | ---: | --- |
| `insertPlainTextAs` | 165 ms at 20k blocks | **every keystroke** |
| `paragraph_text` / `ordered_paragraphs` (a `String` per paragraph, every surface) | O(blocks) with allocation | every backspace, arrow key, selection resolve, `order_endpoints` |
| `documentStats` | 3 ms at 20k → ~200 ms at 1.3M | every edit (status bar) |
| `main.js` `paintReviewMarkers` → `listComments` + `listRevisions` + one `selectionRects` per item | O(comments + revisions) | every click, every edit, **every scroll frame that moves the window** |
| `main.js` `paintChecklistMarkers` | O(all markers in the document) | same |
| `main.js` `paintSelection` | O(lines in selection) → whole document after ⌘A | same |
| `main.js` `renderReviewMarginItems` | O(items), and O(comments²) via `reviewCardSignature` | every `drawSelection`, every resize |
| `main.js` `renderAll` | one `pageSize` crossing per page — 25,556 on the owner's file | open, zoom, resize-to-fit, any page-count change |
| `main.js` `buildOutline` DOM build | O(headings) | every edit while the panel is open |
| `main.js` `scanAllMatches` | O(blocks) per keystroke in the find box (capped at 5,000 matches) | typing a query |

`ordered_paragraphs` also appends headers, footers and both note stores twice —
`collect_block_text_all_surfaces` already includes them — so every one of those
paragraphs is walked and allocated twice in an ordering the caret relies on.

### 4.4 Linear on a user command, unbounded and unyielded

| site | cost | note |
| --- | ---: | --- |
| `print.mjs` `printDocument` | 25,556 full-resolution rasters, synchronous | `109` HF-105; no progress, no cancel, no page range |
| draft autosave `takeDraftSnapshot` | a whole DOCX serialization | on a 5 s quiesce timer while editing |
| `exportDocumentAs` | whole-document serialize | expected, but on the main thread with no progress |
| `copySelection` | serializes the selection **three times** (text, rich runs, structured) | ⌘A ⌘C |
| Pages panel | capped at 40 thumbnails | **already correct** — the shape the other panels should copy |

### 4.5 Already bounded — verified, so nobody re-fixes them

The page raster, the page band (`page_scroll.mjs`), the Pages panel
(`PAGES_PANEL_WINDOW = 40`), the accessibility mirror's 600-block window, the
hit-test path, `populateStyles`, and the pointer-move page lookup are all
O(visible). `113` §8.6 did that work; the overlay layer is what it did not
reach.

## 5. What each one's right shape is

- **A panel that lists document-derived rows** (Outline, Pages, review margin,
  find results) virtualizes: one mechanism, a windowed list bound to a scroll
  position, as `pages_panel.mjs` already does.
- **An overlay that paints markers** asks only for the markers on the pages in
  the window — the engine already indexes by `Page::number`.
- **A whole-document computation** (the measure pass, print, export, find-all)
  goes through one background-pass seam with one progress and cancel contract,
  and reports an estimate labelled as an estimate until it completes.
- **A per-interaction read** resolves ids once, never per node, and is bounded by
  the selection or the window rather than the document.

## 6. What landed, and the guard

Landed (commit "Stop three document-wide reads from scanning once per node"):
`visit_paragraphs` as the single traversal, `documentOutline` and the
accessibility mirror deciding heading level from properties already in hand,
`heading_level_of` taking `&ParagraphProperties` so the id lookup cannot come
back, and `resolve_object_boxes` building its paragraph → objects map in one
walk.

The guard is **complexity, not timing**: a wall-clock budget cannot tell a
quadratic shape from a loaded machine, and this repository has shipped
green-but-wrong guards before (`105` CQ-003).
`casual_doc_edit::document_scans` is a thread-local counter incremented by
`surface_block_lists` and `surface_of` — the two entry points every by-id read
goes through — and
`document_wide_reads_do_not_scan_the_document_once_per_node` asserts, for each
host-reachable read, that the count is bounded by a constant **and identical at
n and 2n blocks**.

The timing half is `webapp/tests/e2e/main-thread-budget.spec.mjs`: one spec
parameterised over an `OPERATIONS` list, asserting that **no single main-thread
task** during each operation exceeds its budget on a 40,000-block document.
Adding a panel means adding a row. Both halves were driven red.

It was driven red once per fix:

```
documentOutline scanned the whole document 402 times on a 400-block document;
a document-wide read resolves ids once, not once per node

objectAt - every pointerdown scanned the whole document 402 times on a
400-block document; ...

accessibilityTreeWindow scanned the whole document 400 times on a 400-block
document; ...
```

and the browser half twice, by rebuilding the engine with the same two
mutations:

```
a click into the page blocked the main thread for 9490 ms against a 500 ms budget
opening the Outline panel blocked the main thread for 5857 ms against a 800 ms budget
```

Adding a row to those lists is how a new panel is admitted.

## 7. The open path: what is owed, and why it is its own change

Open is still linear in the document and still blocks the main thread for its
whole duration, because §3 says 88% of it is one uninterruptible call. Two
designs were weighed:

- **A Web Worker.** Correct by construction, and rejected *for this change*: the
  `WasmDocument` would live in the worker, and every synchronous call the editor
  makes today — `caretRect`, `hitTest`, `renderPage`, 26 call sites through three
  accessors — becomes a message round trip. That is a rewrite of the host's
  relationship with the engine, not a perf fix, and half of it is worse than
  none.
- **Yielding on the main thread.** Cheap, and it does not make the work smaller:
  a 40 s open becomes a 40 s open with a moving bar. Worth having, but only as
  the *presentation* of something that is already fast enough to be worth
  waiting for.

Neither is the actual answer, which §3 makes plain: **do not do the work at
open.** The measure pass exists to make the page count exact from the first
frame; that is a trade `113` §4 Q1 made explicitly, and the measurement says it
costs 88% of the open. So:

1. Measure a **prefix** of the document at open — enough for the first screen
   plus a lead — and show it. `window_of` is already 10 ms from any checkpoint.
2. Report `Page X of ~Y` from mean measured block height, refined as the
   background pass extends the measure tier, and **never present an estimate as
   exact** (`AGENTS.md`: no silent wrong numbers). `NUMPAGES`, printing and the
   scrollbar read the same flag.
3. Extend in the background with a time budget, interruptible by a scroll, so
   jumping to page 20,000 measures what it needs first. `ScrollCoalescer`
   (`113` §8.5 item 4) is the seam that already exists for this and still has no
   consumer.
4. Cancel restores the previously open document, which is already the rule
   `openBytes` follows for a failed parse.
5. A document whose flow state crosses block boundaries
   (`FlowResume::FromStart`) cannot extend cheaply; it measures whole at open, as
   today. The classification already exists and is the same mechanism, not a
   second one.

This is a design for the next change on this branch, not a claim that it is
done. It is written here because `113` §4 Q1's reasoning is what has to be
reversed, and the reversal should be recorded where the original decision is.

## 8. The ceiling

`MAX_VIEWER_BLOCKS` is 1,800,000 and the discipline has been that it moves only
when a measurement moves (`113` §8.4). The measurement that moved it last was
*time to complete*, and that is the wrong question: a document that completes in
110 s in a harness is a hung tab in a browser. The number that decides the
ceiling is **time to interactive** — how long the main thread is unavailable —
and until §7 lands that is the same as time to complete, because the open is one
synchronous call.

Measured now, on the owner's file, through the committed probe: **the main thread
is unavailable for 17,077 ms, and then for 16,981 ms again** (§3.1). By the
stated standard — no task beyond a few hundred milliseconds — 1,303,306 blocks is
not usable, and neither is anything close to it.

So the honest position, stated so it is not mistaken for an oversight: **the
constant is admitting documents the editor cannot open responsively, and no value
of the constant fixes that.** The open cost is ~13 µs/block of uninterrupted main
thread, so a budget of 300 ms is ~23,000 blocks — two orders of magnitude below
any ceiling this product can have, and far below documents that open perfectly
well today (262,145 blocks blocks the thread for ~3.5 s and is genuinely usable).
Lowering the constant to something "interactive" would refuse the ordinary
documents this editor exists to open; lowering it part-way — to 700,000, say —
would still leave a ~9 s freeze *and* refuse the owner's file, which is the worst
of both.

The lever is therefore §7, not the constant, and the constant stays at 1,800,000
with that said out loud rather than implied. What must not happen is the third
option: leaving it there and calling the freeze acceptable. Concretely, the
ceiling moves next when **time to interactive** is the measurement — the number
the probe now prints — and not before.
