# 114 — Pathological complexity audit

A repository-wide map of accidental super-linear work, written after the owner's
1,303,306-paragraph document made the editor hang. The hang was **not slowness**:
`documentOutline` looped over every node and called `paragraph_properties`, which
is a *linear walk of the whole document to find one paragraph* — about
1.7 × 10¹² block visits for a panel that returns an empty list, because that
document has no headings. It never finishes.

The point of this document is the owner's instruction after the first fix:

> if you see a hole on clothes, before stitching, you search the whole shirt for
> more holes first.

So this is the whole shirt. It lists what was found, what was fixed, and — just
as importantly — what was found and deliberately **not** fixed, with the reason.

## How each row is classified

| verdict | meaning |
| --- | --- |
| **FIX** | Work grows faster than the document. Quadratic, or a document-scale operation on a per-keystroke path. |
| **HANDOFF** | Real cost, but it belongs to another lane (main-thread O(document) work, or a file domain another agent owns). Named here with its location so it is not lost. |
| **FINE** | Bounded by a selection, one page, the viewport, or a fixed set. Left alone deliberately. |

"Bound" is the question that decides the verdict — a linear lookup inside a loop
over three selected paragraphs is fine; the same lookup inside a loop over every
block is the defect. A blanket rewrite would be wrong.

## The lookup family — why it reads like an accessor and is not

`casual-doc-edit` exposes a set of helpers that take a `NodeId` and return the
thing it names. Every one of them is a linear walk of every surface:

| helper | production call sites | definition |
| --- | --- | --- |
| `paragraph_properties` | 58 | `crates/casual-doc-edit/src/lib.rs:3500` |
| `find_table` | 44 | `crates/casual-doc-edit/src/lib.rs:3387` |
| `find_paragraph` | 33 | `crates/casual-doc-edit/src/lib.rs:4264` |
| `find_paragraph_mut` | 33 | `crates/casual-doc-edit/src/lib.rs:5201` |
| `find_paragraph_any` | 30 | `crates/casual-doc-edit/src/lib.rs:3968` |
| `locate_table_cell` | 26 | `crates/casual-doc-edit/src/lib.rs:3466` |
| `locate_table_row` | 23 | `crates/casual-doc-edit/src/lib.rs:3420` |
| `find_shape` | 12 | `crates/casual-doc-edit/src/lib.rs:4130` |
| `find_paragraph_any_mut` | 10 | `crates/casual-doc-edit/src/lib.rs:4018` |
| `surface_of` | 9 | `crates/casual-doc-edit/src/lib.rs:3905` |
| `locate_cell` | 9 | `crates/casual-doc-edit/src/lib.rs:3317` |

Counts are from a scripted sweep of `crates/**/*.rs` excluding `#[cfg(test)]`
modules; 198 production call sites across the family in total.

## The mechanism chosen

Two mechanisms, applied by rule rather than by taste:

1. **Walk once and carry** where the caller already has the paragraph in hand.
   `collect_a11y_blocks` is looking straight at `paragraph.properties` and was
   still calling `heading_level_of(paragraph.id, …)`, which threw the paragraph
   away and searched for it again. `heading_level_of` now takes the properties it
   actually needs, and the lookup disappears rather than getting faster.
2. **One hash index, built once per operation**, where random access by id is
   genuinely needed — `ParagraphIndex` in `casual-doc-edit`. The prior art is
   indexing: replace repeated linear search with a hash map built in one pass.
   It is deliberately **not** cached on the `Document`: it borrows the paragraphs
   it points at, so storing it beside them is self-referential, and a stale index
   is a correctness bug where a rebuilt one is merely another O(n) pass. One build
   per operation already turns the quadratic into a linear pass, which is the
   whole distance that mattered.

Per the working contract's "prefer one mechanism over two", there is no third:
every document-bounded call site below is fixed by one of those two.

## Guards are complexity guards, not timers

A millisecond threshold cannot tell a quadratic from a slow constant, and is
flaky under load. Each guard therefore builds documents of *n* and *2n* blocks,
runs the operation, and asserts the work roughly **doubles** rather than
quadrupling. The work is counted, not timed:
`casual_doc_edit::block_visits()` is a thread-local counter charged by the linear
walks and by the index build (`crates/casual-doc-edit/src/lib.rs`). Thread-local
so guards running in parallel do not see each other's work.

Measured on `reading_every_paragraph_through_the_index_is_linear_in_the_document`:

| | 400 blocks | 800 blocks | ratio |
| --- | --- | --- | --- |
| per-node `paragraph_properties` (the bug) | 160,400 visits | 640,800 visits | **4.0×** |
| one `ParagraphIndex` build | 400 visits | 800 visits | **2.0×** |

## Rust — `casual-doc-wasm`

| location | pattern | what bounds it | cost at 1.3M paragraphs | verdict |
| --- | --- | --- | --- | --- |
| `document_outline` → `heading_level_of`, `lib.rs:9203` | per-node `paragraph_properties` | every paragraph of every surface | ~1.7 × 10¹² block visits; never returns | **FIX** |
| `collect_a11y_blocks` → `heading_level_of`, `lib.rs:9370` | per-node `paragraph_properties` while holding the paragraph | every block of the body | same order; runs on every edit (the a11y mirror is rebuilt) | **FIX** |
| `page_setup_sections`, `lib.rs:6493` | `for (paragraph, _) in self.ordered_paragraphs()` + `paragraph_properties` | every paragraph, up to the caret | ~1.7 × 10¹² worst case; runs when Page Setup opens | **FIX** |
| `collect_changed_review_paragraphs`, `lib.rs:13284` | per-paragraph `find_paragraph_any` over a full copy of every surface | every paragraph | quadratic; every Accept All / Reject All / comment delete | **FIX** |
| `copy_rich_runs_inner`, `lib.rs:11844` | per-node `find_paragraph_any` | selection — **whole document under Select All** | quadratic on Ctrl+A, Ctrl+C | **FIX** |
| `can_continue_list`, `lib.rs:4879` | backward scan of `ordered_paragraphs` with `paragraph_properties` per step | stops at the nearest earlier numbered item — **no numbering means scan to the start** | quadratic, and it drives a *toolbar enabled state*, so it runs on selection change | **FIX** |
| `continue_list_inner`, `lib.rs:10868` / `10892` | same backward scan | same | quadratic | **FIX** |
| `set_list_format`, `lib.rs:5050` / `5056` / `5063` | backward *and* forward scan with `paragraph_properties` per step | contiguous run of one list instance; unbounded when the scan finds no boundary | quadratic | **FIX** |
| `restart_list`, `lib.rs:4812` | `for (id, _) in ordered.into_iter().skip(index)` + `paragraph_properties` | every paragraph after the caret | quadratic | **FIX** |
| `suggest_range_edit`, `lib.rs:8098` / `8104` | per-node `paragraph_properties` | selection — whole document under Select All | quadratic on a document-wide suggestion | **FIX** |
| `create_style_from_selection`, `lib.rs:9153` | per-node `paragraph_properties` | selection — whole document under Select All | quadratic | **FIX** |
| `apply_paragraph_props_as`, `lib.rs:10750` | per-node `paragraph_properties` | selection — whole document under Select All | quadratic; Select All then change alignment | **FIX** |
| `apply_indent_props`, `lib.rs:10816` / `10819` | per-node `locate_table_row` **and** `paragraph_properties` | selection — whole document under Select All | quadratic, twice over | **FIX** |
| `paragraph_decision_ops`, `lib.rs:15819` | `find_paragraph_any` per merged revision pair | number of paragraph-merge revisions | linear in revisions, each walk O(n) — grows with review volume, not with the document | **FIX** (same index, no reason not to) |
| `ordered_paragraphs`, `lib.rs:10446` | builds a `String` for **every paragraph on every surface** to take its `len()`, then throws it away; also appends every non-body surface **twice** (`surface_block_lists` already includes them) | whole document, per call; called from ~15 places including caret navigation | 1.3M `String` allocations per call, on a per-keystroke path — and every header/footer/note paragraph appears twice in the result | **HANDOFF** (see below) |
| `toggle_list` `lib.rs:4636`, `toggle_checklist_item` `:4663`, `list_style_at` `:4903`, `paragraph_indent` `:6558`, `selection_style_paragraph` `:9187`, `continue_list_inner` `:10849`, `can_continue_list` `:4866`, `apply_cell_props` `:10778`, `unchecked_followup` `:9971`, `effective_run_properties_in_range` `:16330` | one lookup for the caret's own paragraph | one node | one walk per command | **FINE** — leave them |

`ordered_paragraphs` is a handoff rather than a fix for two reasons. Its cost is
*linear* main-thread work, which is another live agent's lane, and the duplicate
surface append is a **behaviour** defect (header paragraphs appear twice in the
ordering used by `paragraphs_in_selection`), which a performance change must not
quietly alter. Both halves are worth a PR of their own:

- `crates/casual-doc-wasm/src/lib.rs:10446` — replace the `Vec<(NodeId, String)>`
  walk with a length-only walk (the code's own comment already says the string is
  "thrown away immediately" and is "the bulk of the cost of ordering two
  endpoints"), and delete the duplicated header/footer/note loops at `:10458`–
  `:10469`, which double-count every non-body surface.
- The same duplication exists inside `collect_block_text_all_surfaces`
  (`lib.rs:12328`): `surface_block_lists` returns text-box block lists *as
  surfaces*, and `collect_block_text` also descends into text boxes, so every
  text-box paragraph is collected twice. This affects word count and text
  extraction, not just cost.

## Rust — `casual-doc-layout`

| location | pattern | what bounds it | cost at 1.3M paragraphs | verdict |
| --- | --- | --- | --- | --- |
| `collect_band_block` → `find_paragraph`, `src/anchor.rs:1024`/`:1040` | the lookup `collect()`s **every header (or footer) block into a fresh `Vec`** before searching it, once per band paragraph | header/footer content per placed band fragment, per page | header content is small, but the allocation is per paragraph per band per page — tens of thousands of throwaway `Vec`s on a long document | **FIX** (drop the `collect()`; iterate lazily) |

## JavaScript — `webapp/src/**`

`webapp/` is another agent's file domain in this session, so these are mapped and
handed over, not touched. The shape is the same family: a document-scale
operation on the hottest possible loop.

| location | pattern | what bounds it | cost at 1.3M paragraphs | verdict |
| --- | --- | --- | --- | --- |
| `paintReviewMarkers`, `webapp/src/main.js:4172` | full `listComments()`/`listRevisions()` JSON marshal + one `selectionRects()` engine call **per comment/revision regardless of visibility** + three fresh `addEventListener`s per marker chunk — reached from `drawSelection` on **every keystroke, every arrow key, every scroll-driven page change** | comment/revision count, which scales with a reviewed document | 10k tracked changes ⇒ 10k engine calls and ~30k listener attachments *per keystroke* | **HANDOFF — worst JS row** |
| `updateReviewControls`, `webapp/src/main.js:1560` | `JSON.parse(doc.listRevisions())` only to read `.length`, on the same per-keystroke path | revision count | full revision list serialised and parsed per keystroke to enable two buttons | **HANDOFF** |
| `paintChecklistMarkers`, `webapp/src/main.js:6017` | `doc.checklistMarkers()` returns **every** marker in the document (not windowed), scanned per keystroke/scroll | checklist item count | O(all checklist items) marshal + loop per keystroke, to attach a handful | **HANDOFF** |
| `renderReviewMarginItems`, `webapp/src/main.js:1823` | `comments.find(…)` inside the per-comment loop | comment count | genuinely quadratic: 10k comments ⇒ up to 10⁸ comparisons per render | **HANDOFF** |
| `reviewCardSignature`, `webapp/src/review_layout.mjs:128` | `comments.filter(…)` once per item in the caller's loop | comment count | quadratic in comment count per sidebar rebuild | **HANDOFF** |
| `renderAll`, `webapp/src/main.js:3877` | `for (let i = 0; i < count; i++) doc.pageSize(i)` — one synchronous wasm round-trip per page, no yielding | every page | 20–60k blocking wasm calls on open, zoom, or "Show changes" | **HANDOFF** (main-thread lane) |
| `printDocument`, `webapp/src/print.mjs:79` | 150 DPI raster + canvas per page, no yielding | every page — inherent to printing | freezes the tab on a very long document; a progress/chunking UX gap rather than a complexity defect | **HANDOFF** |
| `scanAllMatches`, `webapp/src/main.js:14786` | up to `FIND_SCAN_CAP` (5000) `findText` calls per keystroke in the Find box | explicitly capped, `Set` used for cycle detection | deliberate, bounded trade-off | **FINE** |
| `a11y_mirror.mjs:30`, `pages_panel.mjs:56`, `page_scroll.mjs:204` (binary search), `mountReviewWindow` `main.js:2520` | windowed / virtualised / O(log n) | viewport window | — | **FINE** — these are the pattern the rows above still need |
| `buildOutline` `main.js:11781`, `refreshBookmarkList` `:13338`, `populateStyles` `:9646`, `clipboard.mjs`, `drafts.mjs` | per-row listener, full list rebuild | headings / bookmarks / style definitions / clipboard payload / open editing slots — none scale with paragraph count | — | **FINE** |

## Rule this audit suggests is missing from the contract

The contract now says performance is a gate. This audit adds one sharper rule,
because every row above shares a single shape:

> **A helper that takes an id and returns the thing it names must say in its own
> doc comment whether it is a scan.** Every defect in this document is a call
> site where a linear walk read like a field access. The fix is not only faster
> code, it is naming: `paragraph_properties(&doc, node)` looks like `O(1)` and is
> `O(document)`.

The corollary, which the guards here implement: **a performance guard asserts a
ratio between two input sizes, never a wall clock.**
