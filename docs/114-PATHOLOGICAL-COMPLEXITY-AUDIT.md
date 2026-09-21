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

## Rust — `casual-doc-layout`, `casual-doc-render`, `casual-doc-pdf`

The headline here is not a quadratic, it is **four independent full-document
traversals on the path whose own doc comment promises `O(edit)`**. Each is one
linear pass; stacked and run per keystroke on a 1.3M-paragraph document the
symptom is the same. Every row marked VERIFIED below was re-read directly rather
than taken on a sweep's word.

| location | pattern | what bounds it | cost at 1.3M paragraphs | verdict |
| --- | --- | --- | --- | --- |
| `contains_drop_cap_pair`, `src/flow.rs:1572`, called from `build_galley_cached_labeled` `:820` | `blocks.windows(2).any(…)` over the **whole body**, and `effective_drop_cap_frame` resolves each pair through the style cascade — `resolve_paragraph_in_table`, with no cheap short-circuit (VERIFIED) | every adjacent paragraph pair in the body | ~1.3M cascade resolutions **per keystroke**, on the function documented as "what makes an edit `O(edit)` rather than `O(document)`" | **HANDOFF — worst layout row** |
| `build_section_runs_cached`, `src/document_layout.rs:1188` and `:1193` | `section_break_points(document.body(), …)` then `referenced_endnotes(document.body())`, both unconditional for the single-trailing-section case the code itself calls the common one (VERIFIED) | whole body, twice | two more full walks per keystroke | **HANDOFF** |
| `resolve_note_labels` → `document_order_note_refs`, `src/note_numbering.rs:237`/`:364` | a third full walk of the body *and every inline* for note references — duplicating the endnote walk above | whole body | a third full walk per keystroke; `cuts.iter().find(…)` inside its per-block loop adds O(sections) per block | **HANDOFF** |
| `place_floats`, `src/anchor.rs:57`, from `finish_pagination_pass` `src/document_layout.rs:882` | walks every paragraph's **inline list** for anchors, on every pass, including documents with no floats | whole body's inlines | a fourth full walk per keystroke, at the finest granularity of the four | **HANDOFF** |
| `finish_pagination_pass`, `src/document_layout.rs:850` | re-places running content, resolves `PAGE`/`NUMPAGES`, page borders and page-number labels for **every page of the document** on every cached call; `page_number_labels_for` (`src/paginate.rs:1636`) adds a linear section lookup per page | every page | 20–100k pages reprocessed per keystroke instead of the pages the edit touched | **HANDOFF** |
| `window_of` → `blocks_with_endnotes`, `src/windowed.rs:455` | the fixed `Cow::Borrowed` fast path is guarded on "no endnotes", not on "no scroll" — a document with **one** endnote falls back to `blocks.to_vec()`, the whole body, per window build (VERIFIED) | every scroll/viewport change | the original ~1.3 GB transient clone, back, on every scroll frame | **HANDOFF — highest-severity regression risk** |
| `collect_band_block` → `find_paragraph`, `src/anchor.rs:1024`/`:1040` | the lookup `collect()`ed every header (or footer) block into a fresh `Vec` before searching it, once per band paragraph | header/footer definitions — **small**, so this was never quadratic | tens of thousands of throwaway `Vec`s across a long document's pages | **FIXED** (allocation only — the search itself was correctly bounded) |
| `ImageTable::use_image`, `casual-doc-pdf/src/picture.rs:128` | `self.unresolved.iter().any(…)` over a growing `Vec<String>` | distinct *unresolvable* media keys | only bites a corrupt import with thousands of dangling media parts; should be a `HashSet` | **HANDOFF (low)** |
| `first_dirty_fragment` `src/incremental.rs:143`, `repaginate_with_stats` `src/paginate.rs:380` | one linear diff pass over the galley per edit | whole galley, single pass | the necessary kind of O(n), not the accidental kind | **FINE** |
| `blocks_with_endnotes` `src/document_layout.rs:469`, `flow_blocks_into`'s float floor `src/flow.rs:~1405` | both carry comments documenting a *previous* quadratic / 1.3 GB-clone bug at that exact site | — | already fixed; recorded because this family has recurred here before | **FINE (history)** |
| `MeasureSink::drain_to` `src/measure.rs:882` `pending.remove(0)`, `StyleCascade::new`/`style_chain` `src/cascade.rs:69`/`:88`, everything in `hittest.rs`, `compose.rs`, `numbering.rs`, `fonts.rs`, `shape.rs`, all of `casual-doc-render`, `casual-doc-pdf/src/subset.rs` | front-of-vector mutation capped at 2 by construction; style chain capped at 64; per-page / per-line / per-row / fixed-table scans | a page, a line, a table row, a font table | — | **FINE** — none of these touch the body |

## Rust — `casual-doc-edit`, `-export`, `-import`, `-model`, `-transaction`, `-selection`, `-odf`, `-io`

| location | pattern | what bounds it | cost at 1.3M paragraphs | verdict |
| --- | --- | --- | --- | --- |
| `Operation::UpdateReviewState`, `casual-doc-edit/src/lib.rs:1569` | `paragraphs[..index].iter().any(…)` — a rescan of the entries already seen, per entry — **plus** `find_paragraph_any` per entry | the op's own paragraph list; "accept all changes" passes one entry per affected paragraph, so k can be the document (VERIFIED) | O(k²) + O(k·n) ≈ 1.7 × 10¹² | **FIXED** (set + one index) |
| the same arm's second loop, `casual-doc-edit/src/lib.rs:~1586` and its rollback | `find_paragraph_mut` per entry, and `blocks_owning_mut` → `surface_of` per entry (two walks each) | same | still O(k·n) — the **mutable** side needs a path index, not a reference index | **HANDOFF — named below** |
| every `Operation::*` arm of `apply`, `casual-doc-edit/src/lib.rs:887`–`2200` | each edit locates its one target with 1–4 linear walks (`blocks_owning_mut`, `find_paragraph_mut`, `find_table_mut`, `find_cell_mut`, `find_shape_mut`) | one node per edit — but each *lookup* is O(document) | plain typing is O(document) per keystroke; a session touching every paragraph is O(n²) | **HANDOFF** (per-keystroke O(document) is the other lane's subject; the fix is a path index, one mechanism for both) |
| ~14 arms calling `doc.validate()` after mutating — `SetObjectDescr`, `DeleteObject`, `SetCoreProperties`, `UpdateReviewState`, `SetSectionGeometry`, `SetStyleDefinition`, `CreateBookmark`, `DeleteBookmark`, `InsertField`, `InsertInlineObject`, `RemoveInlineObject`, `RemoveField`, `InsertNote`, `RemoveNote` | `Document::validate` (`casual-doc-model/src/v1/document.rs:160` → `:423` → `:816`) re-derives every node id into a `BTreeSet` and re-checks every paragraph's style/numbering refs | whole body | a **second** full pass on top of the lookup, per such edit | **HANDOFF** |
| `casual-doc-transaction/src/lib.rs:297` | `let mut working = document.clone();` — a deep clone of the entire document per transaction (VERIFIED) | whole document | O(n) copy per transaction, with a keystroke-shaped op vocabulary | **HANDOFF** — and it bears on ADR-033: the OT path must not be built on a per-transaction full clone |
| `casual-doc-model/src/document.rs:66`/`:75`/`:127`/`:131` (schema **v0** model) | `body.iter().find_map(…)` / `.position(…)` per lookup | whole body | the same defect on a second model, used by `-transaction` and `-selection` | **HANDOFF** |
| `TextSelection::validate`, `casual-doc-selection/src/lib.rs:71` | two O(n) `document.paragraph(…)` scans per selection change (anchor + focus) | whole body, per caret move | caret movement alone would be O(n) per keypress wherever this model is live | **HANDOFF** |
| `notes_xml` / `comments_xml`, `casual-doc-export/src/semantic.rs:1493`/`:1538` | `own_media.iter().any(…)` dedup instead of a set, restarted per note/comment | distinct images referenced from notes/comments | O(k²) for a note- and image-heavy document; not the reported hang | **HANDOFF (low)** |
| `vml_textbox_segment`, `casual-doc-import/src/body.rs:5219` | `self.pending_vml_textboxes.remove(0)` | pending VML text boxes | O(k²) shifting; matters for legacy-VML-heavy imports | **HANDOFF (low)** |
| `write_paragraph`'s section lookup, `casual-doc-export/src/semantic.rs:3920` | `sections.iter().find(…)`, but only for a paragraph that actually carries a section break | section count | negligible | **FINE** |
| `Shared::new`'s recent-entry scan, `casual-doc-model/src/v1/intern.rs:96` (capped at 8), style/numbering resolution via `BTreeMap` in `-import`/`-odf`, `resolve_style` memoized in `casual-doc-odf/src/content.rs:1560`, `resolve_numbering_level` (≤9 levels), `related_part` (8 fixed calls), `casual-doc-io/src/pdf.rs:212` | constant-bounded scans, map lookups, or memoized resolution | fixed small sets | — | **FINE** |

No quadratic string work exists in these crates: every writer uses `push_str` /
`Writer::write_event`, and there is no `String::insert`/`remove` or `replace`
chain over document-sized text anywhere in production code.

## What this change actually fixed

Twelve document-bounded call sites, all in `casual-doc-wasm` except the last two:

1. `document_outline` — the confirmed hang.
2. `collect_a11y_blocks` / `accessibility_tree` — lookup removed entirely.
3. `page_setup_sections`.
4. `can_continue_list` — and it drives a toolbar enabled state, so it ran on selection change.
5. `continue_list_inner`.
6. `set_list_format`.
7. `restart_list`.
8. `copy_rich_runs_inner` — Select All, Copy.
9. `suggest_range_edit`.
10. `create_style_from_selection`, `apply_paragraph_props_as`, `apply_indent_props` — the last with *two* per-node walks, the table-row test and the property read.
11. `paragraph_decision_ops` and `collect_changed_review_paragraphs` — Accept All / Reject All / delete a comment.
12. `casual-doc-edit`'s `UpdateReviewState` prevalidation (set + index), and `casual-doc-layout`'s band-anchor allocation.

Each is either walk-once-and-carry or one `ParagraphIndex`. Behaviour is
unchanged: the index is pinned against both walking helpers by
`the_paragraph_index_answers_exactly_what_the_linear_walks_answer`, and the whole
workspace suite passes.

## The named handoff list

Nothing below is fixed here. Each is a location, not a theme.

**Per-keystroke O(document) work — the non-blocking-open lane**

1. `crates/casual-doc-layout/src/flow.rs:820`/`:1572` — `contains_drop_cap_pair` full-body scan with a cascade resolution per pair, on the incremental galley builder. Worst of these.
2. `crates/casual-doc-layout/src/document_layout.rs:1188` and `:1193` — two unconditional full-body scans in `build_section_runs_cached`.
3. `crates/casual-doc-layout/src/note_numbering.rs:237`/`:364` — a third full walk, duplicating (2)'s endnote work.
4. `crates/casual-doc-layout/src/anchor.rs:57` via `document_layout.rs:882` — `place_floats` walks every paragraph's inlines every pass.
5. `crates/casual-doc-layout/src/document_layout.rs:850` — every page reprocessed per cached call.
6. `crates/casual-doc-layout/src/windowed.rs:455` — one endnote reinstates the whole-body clone, per scroll frame.
7. `crates/casual-doc-wasm/src/lib.rs:10446` — `ordered_paragraphs` allocates a `String` per paragraph to read its length, per call, on caret paths; and appends every non-body surface twice.
8. `crates/casual-doc-wasm/src/lib.rs:12328` — `collect_block_text_all_surfaces` collects text-box paragraphs twice (they are both a listed surface and descended into), which affects word count and text extraction, not only cost.
9. `crates/casual-doc-wasm/src/lib.rs:13319` — `review_surfaces` copies every surface (`to_vec`) on each review command.

**The mutable half of the lookup family — needs a path index**

10. `crates/casual-doc-edit/src/lib.rs` — `find_paragraph_mut`, `find_paragraph_any_mut`, `blocks_owning_mut`/`surface_of`, `find_table_mut`, `find_cell_mut`, `find_shape_mut`. A `ParagraphIndex` cannot serve these: it hands out shared references. The one mechanism that would is a **path index** (`NodeId → (Surface, Vec<usize>)`, built by the same walk, descended with the existing `vec_at_path_mut`), which would also retire the `surface_of`-then-walk double pass. That is a design change, not a rename, so it is not smuggled into a performance PR. It is what `UpdateReviewState`'s second loop and every `apply` arm need.
11. `crates/casual-doc-model/src/v1/document.rs:160` — `validate()` after ~14 op arms: a second full pass per edit. Needs either incremental validation or validation of the touched subtree.
12. `crates/casual-doc-transaction/src/lib.rs:297` — the per-transaction full document clone; bears directly on ADR-033.
13. `crates/casual-doc-selection/src/lib.rs:71` and `crates/casual-doc-model/src/document.rs:66` — the same linear-lookup defect on the schema-v0 model, on the caret path.

**JavaScript — the webapp lane** (rows 1–7 of the JavaScript table above)

14. `webapp/src/main.js:4172` `paintReviewMarkers`, `:1560` `updateReviewControls`, `:6017` `paintChecklistMarkers`, `:1823` `renderReviewMarginItems`, `webapp/src/review_layout.mjs:128` `reviewCardSignature`, `webapp/src/main.js:3877` `renderAll`, `webapp/src/print.mjs:79` `printDocument`.

**Small and cheap**

15. `crates/casual-doc-pdf/src/picture.rs:128` (`Vec` → `HashSet`), `crates/casual-doc-export/src/semantic.rs:1493`/`:1538` (same), `crates/casual-doc-import/src/body.rs:5219` (`remove(0)` → an index cursor), `crates/casual-doc-layout/src/note_numbering.rs:384` and `crates/casual-doc-layout/src/paginate.rs:1636` (linear section lookups → binary search).

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
