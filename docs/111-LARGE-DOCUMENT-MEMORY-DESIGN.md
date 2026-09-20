# 111 — Large-document memory: why a 1.3M-paragraph file is refused, and what it costs to admit it

**Status:** Design. **Opened:** 2026-09-20. **Owner:** unassigned.
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

### Result after stages 1a and 1b, measured

For the owner's file (short paragraphs, so per-paragraph glyph cost is well below the
130-character probe): model **1,004 B measured** × 1.3M ≈ **1.31 GB** resident, plus the
bounded window stage 2 owes. That fits a 4 GB address space, but with less headroom than
either earlier draft of this document claimed — the ~900 B this section projected for
stage 1b was 11.6% low, which is the third projection here to miss and the reason the
number above is quoted from a committed probe rather than reasoned about. Stage 1a alone
would have left the model at ≈1.89 GB, which does not fit once a window is added.

## 5. What is deliberately NOT proposed

- **Raising `MAX_VIEWER_BLOCKS` without stage 2.** The constant is a measured cliff. A
  larger number buys a module abort instead of an honest refusal, which is strictly
  worse (HF-158 exists because that is what used to happen).
- **Removing the refusal path.** Even after stage 2 there is a ceiling; it must keep
  saying what was found, what the limit is, and what to do.
- **Streaming the model to disk / IndexedDB.** Out of scope; local-first and
  no-mandatory-server (SKILL.md §1) are not at risk from an in-memory model of this size.

## 6. Reproducing these numbers

**§4 stage 1b's numbers are reproducible: `cargo run --release --example
model_footprint [paragraphs]`** in `crates/casual-doc-layout`. That example is committed
and prints both halves of the measurement — the `size_of` table, every `BlockNode` and
`InlineNode` payload sorted largest-first (so the variant that sets an enum's size is
named rather than guessed at), and the resident bytes per paragraph as an RSS delta over
a synthetic body. The committed `edit_latency` example produces the pagination timings.

The §2 and §3.1 probes predate it and were temporary examples that were not committed,
so **those numbers remain design input, not a public claim.** Per `docs/105` EV-rules a
published number must derive from a committed artifact — which stage 1b's now do, and
§2/§3.1's do not.
