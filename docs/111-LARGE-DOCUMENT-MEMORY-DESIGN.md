# 111 — Large-document memory: why a 1.3M-paragraph file is refused, and what it costs to admit it

**Status:** Design; stages 1a, 1b, 1c and 2 landed. The file in the title opens with all
of its pages reachable (§4, `docs/113` §8.6), and after 1c its model costs **142 bytes
per paragraph against 2,046** — a 346-403 MB peak against 2,674 MB (§4, stage 1c).
**Opened:** 2026-09-20.
**Owner:** unassigned.
**Supersedes nothing.** Extends the admission work landed in #552 (`docs/104` HF-158).

## 1. The problem, stated concretely

The owner opened `40mb.docx` — 41,705,760 bytes, **1,303,306 paragraphs** — and the
editor refused it:

> This document has 1,303,306 paragraphs, more than the 262,144 the browser editor can
> hold in memory. Split it into smaller documents, or open it in a desktop word
> processor.

The refusal is arithmetically correct and the message is honest. It is still a refusal,
against an owner requirement to **support files up to 200 MB**.

Two facts about that file are worth recording because they shaped the analysis:

- **It is not a DOCX.** `file(1)` reports ASCII text with CRLF line terminators; `unzip`
  rejects it ("End-of-central-directory signature not found"). It is 40 MB of the line
  `examplefile.com - Sample Files` repeated, with a `.docx` extension. It is admitted by
  the plain-text adapter, which is why the newline pre-check in `viewer_admission_error`
  fires on it at all.
- **Its shape is the worst case for us**, and not an unrealistic one: maximum block
  count for minimum bytes. A 200 MB DOCX of photographs has an ordinary block count and
  already opens. Block count, not file size, is what binds — which is what
  `MAX_VIEWER_BLOCKS` says, and it says it correctly.

## 2. Measurement, not assumption

Measured on this build (macOS arm64, release, `crates/casual-doc-layout` probes), on
synthetic prose of 130-character paragraphs at a US-Letter content width of 9,360 twips:

| paragraphs | pages | peak RSS |
| ---: | ---: | ---: |
| 50,000 | 2,273 | 1.41 GB |
| 100,000 | 4,546 | 1.38 GB (probe) / — |
| 200,000 | 9,091 | 4.97 GB |

Cost is linear in blocks. Broken down at 100,000 paragraphs:

| stage | resident | per paragraph |
| --- | ---: | ---: |
| document model | 149 MB | 1,565 B |
| + shaped galley | 452 MB | 4,739 B |
| + paginated layout | 774 MB | 8,116 B |
| **total** | **1,375 MB** | **14,420 B** |

**14.4 KB of resident memory per paragraph.** At 262,144 blocks that is ~3.8 GB, which
is the wasm32 4 GB linear-memory ceiling. `MAX_VIEWER_BLOCKS` is therefore not an
arbitrary constant — it is the memory cliff, measured. Raising it without reducing the
per-paragraph cost would reintroduce the `RuntimeError: unreachable` module abort that
HF-158 fixed.

For the owner's file: 1,303,306 × 14.4 KB ≈ **18.7 GB**. Four to five times the entire
address space.

## 3. Where the bytes are

### 3.1 The model — struct size, not content

`std::mem::size_of` on this build:

| type | size |
| --- | ---: |
| `BlockNode` / `Paragraph` | 816 B |
| `ParagraphProperties` | 768 B |
| `InlineNode` | 512 B |
| `Run` | 496 B |
| `RunProperties` | 448 B |

A paragraph carrying a single 130-character run costs ~1,565 B, of which **1,216 B is
two property structs that are entirely default**. The text itself is ~150 B. We are
paying eight bytes of empty options for every byte of document.

Inside `ParagraphProperties` (768 B), stored **by value**:

| field | size | typical |
| --- | ---: | --- |
| `borders: ParagraphBorders` | 288 B | default on essentially every paragraph |
| `prop_change: Option<PropChange<ParagraphProperties>>` | 112 B | only on a tracked format change |
| `mark_revision: Option<MarkRevision>` | 80 B | only on a tracked paragraph mark |

Inside `RunProperties` (448 B):

| field | size | typical |
| --- | ---: | --- |
| `prop_change: Option<PropChange<RunProperties>>` | 112 B | only on a tracked format change |

**Correction (2026-09-20).** An earlier revision of this section also listed
`revision: Option<Revision>` at 160 B on `RunProperties`. **That field does not exist and
never has** — `Revision` is an inline node (`InlineNode::Revision`), not a run property,
and the 160 B was `size_of::<Option<Revision>>()` measured as a free-standing type rather
than as a field of anything. The error is left visible rather than quietly deleted,
because it is the exact failure mode `docs/105` EV-rules exist to catch: a number that was
measured correctly and then attributed to the wrong thing.

Four fields, 592 B, present on every paragraph and run in the document and populated on
almost none of them. The codebase already has the fix as an established pattern in the
same struct — `mark_run: Option<Box<RunProperties>>` is 8 B.

### 3.2 The galley and the layout — real, but not needed all at once

The remaining 12.8 KB per paragraph is shaped glyphs (`GlyphRun`, 96 B each) and the
placed/painted result (`PlacedFragment` 176 B, `PaintItem` 104 B, `Page` 368 B). This is
genuinely proportional to rendered content and cannot be shrunk to nothing.

It can, however, be made **non-resident**. The webapp already virtualizes which pages it
paints; the engine does not virtualize which pages it *builds*. `paginate_document`
builds every page of the document before the first one is shown.

## 4. The two changes, in order

### Stage 1 — shrink the model (this is the cheap one)

Box the five rarely-populated fields above: `Option<Box<T>>` where the payload is large
and usually absent, matching `mark_run`'s existing shape.

Measured outcome on `perf/shrink-model-properties` (commit `556a6c3`):

| type | before | after |
| --- | ---: | ---: |
| `ParagraphProperties` | 768 B | **304 B** |
| `RunProperties` | 448 B | **352 B** |
| `Paragraph` | 816 B | **352 B** |
| `Run` | 496 B | **400 B** |
| `BlockNode` | 816 B | **800 B** |
| model per paragraph | 1,565 B | **1,492 B** |

**The predicted ~850 B/paragraph did not land, and the reason matters more than the
miss.** A `Vec<BlockNode>` pays for the enum's *largest variant* on every element, and
that variant is `Table` at 800 B — not `Paragraph`, which is now 352 B. The same is true
one level down: `InlineNode` is 416 B, so the run vector pays 416 B per run however small
`Run` becomes. Shrinking the property structs therefore bought **73 B per paragraph, 4.7%**,
because the enum slots absorbed the rest.

This is not a reason to undo it: the structs had to shrink *first*, or boxing the variants
would buy nothing either. It does mean stage 1 has a second half.

### Stage 1b — box the large enum variants (landed)

**Measured, on `perf/box-large-enum-variants`, with the committed
`casual-doc-layout` `model_footprint` example** (macOS arm64, release; run it as
`cargo run --release --example model_footprint [paragraphs]`). The same probe produced
both columns, one run per side of the change:

| type | after 1a | after 1b |
| --- | ---: | ---: |
| `BlockNode` | 800 B | **352 B** |
| `InlineNode` | 416 B | 416 B (unchanged — see below) |
| `Paragraph` | 352 B | 352 B |
| `Run` | 400 B | 400 B |
| `Table` | 800 B | 800 B (unchanged — it moved out of line, it did not shrink) |
| **model per paragraph** | **1,452 B** | **1,004 B** |

The per-paragraph figures are this probe's, measured as the RSS delta of building
100,000 single-run 130-character paragraphs; §3.1's 1,565/1,492 came from a different,
uncommitted probe and ran slightly higher. 1,004 B is stable across runs (±3 B) and
across sizes (1,006 B at 200,000 paragraphs). The saving is **448 B per paragraph,
30.9%** — exactly the 800 − 352 the enum slot gave back, which is the point: nothing
about a paragraph changed, only what the `Vec` slot holding it costs.

**What was boxed, and why those two.** `BlockNode::Table(Box<Table>)` and
`BlockNode::Sdt(Box<BlockSdt>)`. `Table` at 800 B was the variant §4 named; `BlockSdt`
at 384 B is the one it did not, and leaving it inline would have stopped the enum at
400 B instead of 352 B. Both are rare, and both already allocate a `Vec` of rows or
blocks when they are used, so the added pointer hop costs nothing measurable on the
paths that touch them. `Paragraph` stays inline deliberately: it is the common case,
and boxing it would add an allocation per paragraph while shrinking nothing.

`BlockNode` came out at 352 B, not the 360–368 a tag would suggest, because the
discriminant fits in spare bits of `Paragraph` — the enum is now exactly the size of
the thing a document is nearly entirely made of.

**`InlineNode` was measured and deliberately left alone.** §4 assumed some rare variant
set its 416 B. It does not. Sorted largest-first, its payloads are `Run` 400,
`Symbol` 400, `NoteNumberMark` 384, `InlineSdt` 384, then a long tail — and all three
of the large ones are large for the same reason, a `RunProperties` by value. So the
enum is already sized by its *common* case. Boxing `Symbol` or `NoteNumberMark` would
buy exactly zero bytes while `Run` is 400 B, and boxing `Run` would put a heap
allocation on the hottest path in the model. **The remaining win under `InlineNode` is
shrinking `RunProperties` (352 B), which shrinks all three at once** — that is stage 1c
if it is ever worth doing, not a boxing change.

**Blast radius, as actually landed.** 16 files in 8 crates, all mechanical: 341
references to the two variants, of which the pattern matches kept working through
deref and only the constructions needed `Box::new`. No golden, fixture or snapshot
moved.

**Serialization is unchanged, and it is pinned rather than asserted.**
`crates/casual-doc-model/src/v1/tests.rs` carries a canonical document holding a table,
a block content control and a `symbol` inline, and requires re-serialization to
reproduce those exact bytes and to be a fixed point on reopen; a second test walks the
payloads back out (grid, row, cell, nested run, alias, symbol font and code point) so a
silently dropped field inside a boxed variant fails rather than passes. The size guard
in the same file is now written as a *relationship* — `BlockNode <= Paragraph + align`,
`InlineNode <= Run + align` — so it cannot be "fixed" by raising a constant alongside
the variant it was meant to catch.

**The ~900 B projection did not hold: the real number is 1,004 B**, 11.6% higher. For
the owner's file that is 1,303,306 × 1,004 B ≈ **1.31 GB** of permanently resident
model, not the 1.17 GB §4 projected — still inside a 4 GB address space, still
dependent on stage 2 for the galley and display list, and with correspondingly less
headroom. Stage 1a alone would have been ≈1.89 GB.

### Stage 1c — store formatting once, not once per node (landed)

Stages 1a and 1b shrank the property structs and then boxed the enum variants
that were absorbing the saving. Both were the same idea — make a node smaller —
and both under-delivered against their projection, 1a by 4.7% of a paragraph and
1b by 11.6% of its own. Stage 1c is a different idea, and it is the one the
owner named: **normalization**.

A document stored its formatting **per node**. Every `Paragraph` owned a
`ParagraphProperties` (304 B) and every `Run` a `RunProperties` (352 B), by
value. The owner's file has 1,303,306 paragraphs and exactly **one distinct
value of each** — and stored 1.3 million copies of both. That is not a struct
that is too big; it is the same value written down 1.3 million times.

`v1::Shared<T>` (`crates/casual-doc-model/src/v1/intern.rs`) replaces the
by-value field with an `Arc` to a shared entry:

- **Reads are unchanged.** `Shared<T>` dereferences to `T`, so
  `paragraph.properties.alignment` still compiles and still reads a field, and
  `&run.properties` still coerces to `&RunProperties`. This is what kept the
  change reviewable: of ~700 workspace sites that touch these fields, **six**
  needed a decision, and the rest were the compiler asking for `.into()`.
- **Writes are copy-on-write.** `DerefMut` and the named `make_mut` seam both
  call `Arc::make_mut`, so a node sharing its formatting with a million others
  gets its own copy the moment it is edited and the million are untouched. There
  is no way to write through a shared entry: in safe Rust with an `Arc` the
  aliasing bug is not expressible, which is why this shape was chosen over a
  side table plus `u32` handles.
- **Nothing accumulates and there is no table to compact.** An entry is freed
  when the last node referencing it drops — the answer to "eviction policy" is
  refcounting.
- **Two interning mechanisms, both cheap.** One process-wide default entry per
  type (one comparison, and the hit for nearly every node of nearly every
  document), then a bounded eight-entry most-recently-interned cache per type
  per thread, which catches the locality real documents have — consecutive runs
  in a paragraph share formatting — without hashing a 352-byte struct per node.

**The property structs did not shrink and are not meant to.** `ParagraphProperties`
is still 304 B and `RunProperties` still 352 B; a document with a thousand
distinct formats holds a thousand of them. What changed is the multiplier.

#### Interning alone would have bought almost nothing, and the reason is §4's again

Measured, not assumed: with formatting shared but the enums untouched,
`InlineNode` went from 416 B only to **384 B**, because `InlineSdt` (384 B) —
which carries no run properties — then set its size. A `Vec` pays for the
largest variant, so the saving landed in the slot and stayed there, exactly as
in 1a. So 1c also stores out of line every `InlineNode` payload larger than a
`Run` (`Sdt`, `Field`, `TextBox`, `AnchoredDrawing`, `Group`, `EmbeddedObject`,
`Revision`, `Drawing`, `Math`, `MoveRangeStart`, `Hyperlink`, `Symbol`) and the
last `BlockNode` one (`AltChunk`, 96 B). `Run` stays inline: it is the common
case and it is now 48 bytes.

| type | after 1b | after 1c |
| --- | ---: | ---: |
| `BlockNode` | 352 B | **48 B** |
| `InlineNode` | 416 B | **64 B** |
| `Paragraph` | 352 B | **48 B** |
| `Run` | 400 B | **48 B** |
| `ParagraphProperties` | 304 B | 304 B (one copy per distinct value) |
| `RunProperties` | 352 B | 352 B (one copy per distinct value) |

#### Stage 1a — the largest single item was not a struct at all

The probes were extended before anything was changed, and they found something
no `size_of` table can see and no value-inspecting test can reach.

`Vec::push` onto an empty vector does not allocate one element. It allocates
`RawVec::MIN_NON_ZERO_CAP`, which is **four** elements for any element of 1,024
bytes or less. Every importer builds a paragraph's `inlines` by pushing, and
almost every paragraph holds one inline — so almost every paragraph carried
**three empty `InlineNode` slots**. At 416 B a slot that is **1,248 bytes per
paragraph of capacity holding nothing: 56% of what a paragraph cost, and 1.6 GB
on the owner's file.** The body vector's own doubling added 109 B more.

This is why `docs/111` §4's own 1,004 B/paragraph was never the production
figure: the probe built its body with `vec![one_inline]` and a sized `collect`,
which pays neither cost. Measured through the real plain-text adapter the same
document shape cost **2,353 B/paragraph**, not 1,004.

Two fixes, because they answer different questions:

- `Document::new` releases spare capacity in the body it is handed
  (`BlockNode::shrink_to_fit`). One choke point rather than five importers, and
  it covers DOCX, ODT and RTF as well.
- the plain-text adapter counts first and sizes exactly, so on the owner's own
  path the allocation is never made and **peak** never reaches for it either.

#### Measured, on the owner's own file at its own size

`crates/casual-doc-io/examples/import_footprint.rs` (committed; macOS arm64,
release), 1,303,306 paragraphs of 30 characters fed through the registry the way
the browser feeds a picked file — detection, then import with `retain_source`.
Peak is a sampled RSS high-water mark of a child process, because peak is what
fails an allocation.

| 1,303,306 paragraphs | before 1c | after 1c |
| --- | ---: | ---: |
| `BlockNode` slots | 352 B/para | 48 B/para |
| `InlineNode` slots, used | 416 B/para | 64 B/para |
| `InlineNode` slots, **capacity holding nothing** | **1,248 B/para** | **0** |
| run text | 30 B/para | 30 B/para |
| **itemised total** | **2,046 B/para** | **142 B/para** |
| resident (RSS delta) | 1,870 MB | **187-361 MB** |
| **peak RSS (sampled)** | **2,674 MB** | **346-403 MB** |
| import time | 9.1 s | **1.7-2.0 s** |

**A paragraph holding 30 characters costs 142 bytes of model, against 2,046.**
The itemised figure is exact — it is read off `Vec::capacity` and
`String::capacity` on the live structure — and it is the number to quote; the
resident and peak ranges are the same measurement taken three times and carry
the OS's reclaim behaviour on freed 40 MB buffers, which is why they are given
as ranges rather than as a single figure. Nothing here is extrapolated: every
row was measured at 1,303,306 paragraphs.

**Peak, not resident, is what killed the tab, and it was steady-state rather
than transient.** Before 1c the sampled peak (2,674 MB) matched the *sum of
everything the model allocated* (2,666 MB itemised) to 0.3%: there was no
transient spike to blame, the model simply needed 2.7 GB. That figure also
matches, and explains, `docs/113` §8.4's browser reading of **2,476-2,504 MB of
wasm linear memory** for this file — the 2.5 GB the tab was holding was the
model, and the layout tiers windowing had already bounded were a rounding error
next to it.

**What is left in the gap between 187 MB resident and 403 MB peak** is no longer
per-paragraph: it is four 40 MB-sized buffers of the same file — the source
bytes, the normalized UTF-8 copy `normalize_text` builds, the second such copy
`probe` builds and throws away during detection, and the permanent copy
`retain_source: true` keeps in the source envelope. Named here rather than left
to be discovered (`docs/105` §9 rule 4); it is `casual-doc-io`/`casual-doc-wasm`
work, not the model's, and it is now the largest remaining item on this path.

#### What stage 1c means for the ceiling

`MAX_VIEWER_BLOCKS` is 1,800,000 and was set by a browser measurement of wasm
linear memory (`docs/113` §8.6), of which the model was ~2.5 GB at 1.3M blocks.
The model is now ~0.2 GB there. **That constant is therefore stale in the
conservative direction and must be re-measured through the committed browser
probe before it is moved** — the rule that it moves only when a measurement
moves has held twice and holds here.

#### The guards, and what drives them red

In `crates/casual-doc-model/src/v1/tests.rs`, extending the existing ratchet.
Each was mutation-proved; the two that matter most:

- `a_node_costs_less_than_the_formatting_it_carries` — written as a
  relationship (`Paragraph` must be **smaller than** `ParagraphProperties`),
  which no by-value arrangement of the same data can satisfy. Mutating
  `Shared<T>` to hold its value inline as well as by reference turns it red at
  `a paragraph's handle on its formatting must be one pointer, not 320 bytes`.
- `editing_one_node_leaves_every_node_that_shared_its_formatting_untouched` —
  the copy-on-write guard. Replacing `Arc::make_mut` with
  `Arc::get_mut(..).expect("unique")`, which is what a `properties_mut` that
  forgot to copy would look like, turns it red.

One mutation is worth recording because it **passed**, which is the failure mode
`SKILL.md` §4 exists for: deleting the default-entry fast path from
`Shared::new` left the sharing guard green, because the recent-entry cache
re-shared the default anyway. The guard now also requires the shared entry to be
the one `Shared::default()` hands out, so the two mechanisms are
distinguishable and losing either is red.

### Stage 2 — window the layout (this is the architecture)

Only a bounded set of pages may hold a galley and a display list at once. The model stays
fully resident; the *rendered* form of it does not.

This is how Word behaves and it is the only way the ceiling goes away rather than moving.
It changes an invariant the engine currently relies on — `PaginatedLayout.pages` is a
complete `Vec<Page>`, and `page_count()` is exact the moment `paginate_document` returns —
so it needs its own design before implementation. Open questions to settle there:

- **Page count before full layout.** Exact page count requires laying out everything.
  Word shows an estimate and refines it in the background. What does the status bar,
  the page-number field (`Page X of Y`), and the scrollbar do in the meantime?
- **Resumable pagination.** Laying out a window starting at an arbitrary block requires
  a serializable "state at block N" (section, column, running-content and note-numbering
  state). `build_section_runs` does not expose one today.
- **Eviction policy and thrash.** Which window, how large, and what happens when a
  user drags the scrollbar across a 60,000-page document.
- **Interaction with the incremental path.** `GalleyCache`/`DirtySet` already exist for
  the edit path (`paginate_document_cached`: 477 ms at 200k paragraphs against 4,471 ms
  for a full re-pagination). Windowing and caching must be one mechanism, not two.

#### Stage 2 as landed — the design is `docs/113`, and it is built in the engine

That design is `113-WINDOWED-LAYOUT-DESIGN.md`, which answers all four questions above
and carries the measurements. In short, and only so this section is not read as still
open:

- **Page count** is exact from the first frame, not estimated: the measure tier is built
  in one shaping pass through a sink that keeps two shaped paragraphs at a time, and
  paginated by the *same* paginator the paint tier uses.
- **Resumable pagination** is a checkpoint every 64 pages plus, for the flow half, a
  classification of whether the document carries any state across a block boundary at
  all — rather than a snapshot of that state, which would be a second place for the two
  paths to disagree.
- **Eviction** is LRU over counted paint-tier bytes, on `GalleyCache` and on the window;
  thrash is handled by a clock-free scroll coalescer whose guard drags 60,000 pages and
  asserts one window is built.
- **One mechanism**, as required: `GalleyCache` gained the budget, no second cache exists.

**Measured on the owner's own file shape at its own size** (1,303,306 paragraphs), with
the committed `casual-doc-layout` `layout_footprint` example; peak is a sampled RSS
high-water mark of the child process, because peak is what fails an allocation:

| 1,303,306 paragraphs | `paginate_document` (today) | `measure_document` + one window |
| --- | ---: | ---: |
| pages | 29,621 | 29,621 |
| per paragraph | 3,378 B | **1,021 B** |
| resident | 4.10 GiB | **1.24 GiB** |
| peak RSS | 4.14 GiB | **1.23 GiB** |
| time | 8.6-34.6 s | 8.3-33.7 s |

Timing is a range because it is the noisy half: six runs on a shared 16 GiB laptop gave
8.3-38 s for the same work while the memory figures reproduced to within 0.4%. Both
columns move together — the windowed open pays the same single shaping pass — so
windowing does not make opening faster, it makes it fit. The 3,378 B/paragraph figure is
the same at 300,000, 600,000 and 1,303,306 paragraphs, so the total is measured rather
than extrapolated.

Note how far §3.1's 14.4 KB/paragraph has moved, and why the comparison is not with it:
that figure was measured on 130-character prose with a pre-stage-1 model. On the owner's
own 30-character paragraphs the production path costs 3,378 B, which is the number that
matters for that file. The 4.14 GiB peak is the measured reason it is refused: it does
not fit a wasm32 address space.

#### The ceiling has moved: 262,144 → 700,000 → 1,800,000

`casual-doc-wasm` now holds a `BodyLayout` that is either the whole document's pages or
one window of them (`docs/113` §8.2), so the browser no longer pays the left-hand column
above. Re-measured **through the browser**, with the committed probe
`webapp/tests/e2e/viewer-ceiling-measurement.spec.mjs` — `wasm` is
`WebAssembly.Memory.buffer.byteLength` after the open, which is the high-water mark
because linear memory never shrinks:

| blocks | path | open | wasm | pages | last page reachable |
| --- | --- | ---: | ---: | ---: | --- |
| 262,144 | whole | 26.3-42.2 s | 1,222 MB | 5,141 | yes |
| 262,146 | windowed | 17.3-31.2 s | **592 MB** | 5,141 | yes |
| 700,000 | windowed | 85.9-95.0 s → **17.7 s** | 1,314-1,334 MB | 13,726 | yes |
| 800,000 | windowed | 118.7 s | 1,442 MB | 15,687 | no → **yes** |
| **1,303,306 — the owner's file** | windowed | 110.5 s → **32.4-51.9 s** | **2,504 MB** | **25,556** | no → **yes** |
| **1,800,000 — the ceiling** | windowed | **47.2-53.1 s** | **3,090 MB** | 35,295 | **yes** |

The second figure in each cell is after the host's page band (`docs/113` §8.6). The
"reachable" column moved because the scroll container is no longer as tall as the
document; the open times moved because two thirds of the old figure was the host
building one sheet element per page.

At the same block count the windowed path holds **592 MB against 1,222 MB**, which is
what pays for a 2.7× higher ceiling.

**The owner's file now opens in the browser, and all of it is reachable** — 2,504 MB
inside a wasm32 address space where the whole-layout path needed 4.14 GiB and could not
be attempted, with all 25,556 pages reported, rasterizable, exportable, findable and
scrollable to. The second half of that sentence is `docs/113` §8.6: the viewer used to
build one sheet per page, so its scroll container was 27,549,376 px against a browser
limit between 2^24 and 2^25, and the last third could not be reached. It now positions a
window of sheets inside one 8,000,090 px band, so the container is the same height at
every document size and the constant is again what it says it is — the largest size
measured to open **and be wholly reachable**.

### Result after stages 1a and 1b, measured — superseded by 1c

For the owner's file (short paragraphs, so per-paragraph glyph cost is well below the
130-character probe): model **1,004 B measured** × 1.3M ≈ **1.31 GB** resident, plus the
bounded window stage 2 owes. That fits a 4 GB address space, but with less headroom than
either earlier draft of this document claimed — the ~900 B this section projected for
stage 1b was 11.6% low, which is the third projection here to miss and the reason the
number above is quoted from a committed probe rather than reasoned about. Stage 1a alone
would have left the model at ≈1.89 GB, which does not fit once a window is added.

**And 1,004 B was still not the production figure.** It was measured on a body built
with `vec![one_inline]`; through the real import path the same document shape cost
**2,353 B/paragraph**, because of `Vec` capacity no `size_of` table can see. Stage 1c
measured the import path, fixed both halves, and brought a 30-character paragraph to
**142 B** itemised and the whole file to a **346-403 MB** peak. That is the fourth
projection in this document to be wrong, and the first to be wrong in the useful
direction: the problem was bigger than the struct table said, and so was the fix.

## 5. What is deliberately NOT proposed

- **Raising `MAX_VIEWER_BLOCKS` without stage 2.** The constant is a measured cliff. A
  larger number buys a module abort instead of an honest refusal, which is strictly
  worse (HF-158 exists because that is what used to happen). *(Stage 2 landed, the
  constant was re-measured through the browser, and it moved to 700,000, then to
  1,800,000 once the host stopped building one sheet element per page — see §4 and
  `docs/113` §8.6. The rule stands: it moved because a measurement moved, both times.)*
- **Removing the refusal path.** Even after stage 2 there is a ceiling; it must keep
  saying what was found, what the limit is, and what to do.
- **Streaming the model to disk / IndexedDB.** Out of scope; local-first and
  no-mandatory-server (SKILL.md §1) are not at risk from an in-memory model of this size.

## 6. Reproducing these numbers

**§4 stage 1c's import-path numbers come from the committed `casual-doc-io`
`import_footprint` example**: `cargo run --release --example import_footprint
[paragraphs] [chars]` in `crates/casual-doc-io`. It feeds a synthetic file of the
owner's own shape through the registry exactly as the browser feeds a picked file
(detection, then import with `retain_source`), runs the import in a child process whose
resident set the parent samples so **peak** and resident come from the same run, and
itemises the result off `Vec::capacity` on the live structure. `1303306 30` is the
owner's file.

**§4 stage 1b's and 1c's `size_of` numbers are reproducible: `cargo run --release
--example model_footprint [paragraphs] [chars] [shape]`** in `crates/casual-doc-layout`.
`shape` is `push` (how an importer builds a body) or `collect` (sized exactly — a floor,
and what the pre-1c 1,004 B figure was measured on); the default run does both and
reports a sampled peak for each. That example is committed
and prints both halves of the measurement — the `size_of` table, every `BlockNode` and
`InlineNode` payload sorted largest-first (so the variant that sets an enum's size is
named rather than guessed at), and the resident bytes per paragraph as an RSS delta over
a synthetic body. The committed `edit_latency` example produces the pagination timings.

**The stage-2 layout figures come from the committed `layout_footprint` example** in
the same crate (`cargo run --release --example layout_footprint [paragraphs]`), which
runs each phase in its own child process and samples the child's peak RSS, because peak
is what fails an allocation.

**The browser figures come from the committed probe**
`webapp/tests/e2e/viewer-ceiling-measurement.spec.mjs`. It is skipped in a normal run —
the largest row takes minutes — and produced deliberately:

```sh
cd webapp && ./build.sh
MEASURE_VIEWER_CEILING=1 npx playwright test viewer-ceiling-measurement
# one size, or a sweep:
MEASURE_VIEWER_CEILING=1 VIEWER_CEILING_BLOCKS=262146,700000 npx playwright test viewer-ceiling-measurement
# or a real file, which never enters the repository:
MEASURE_VIEWER_CEILING=1 VIEWER_CEILING_FILE=~/Downloads/40mb.docx npx playwright test viewer-ceiling-measurement
```

It prints one row per size: whether the document opened or was refused, the wall time,
`WebAssembly.Memory.buffer.byteLength` (the high-water mark — linear memory never
shrinks), the JS heap, the page count, and whether the host can scroll to the last page
and find ink on it. That last column is the one that set the ceiling.

The §2 and §3.1 probes predate all of this and were temporary examples that were not
committed, so **those numbers remain design input, not a public claim.** Per `docs/105`
EV-rules a published number must derive from a committed artifact — which stage 1b's,
stage 2's and the browser figures now do, and §2/§3.1's do not.
