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
| `revision: Option<Revision>` | 160 B | only on a tracked run |
| `prop_change: Option<PropChange<RunProperties>>` | 112 B | only on a tracked format change |

Five fields, 752 B, present on every paragraph and run in the document and populated on
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

- `ParagraphProperties` 768 B → ~312 B
- `RunProperties` 448 B → ~192 B
- model per paragraph 1,565 B → ~850 B

Blast radius is contained: `prop_change` has 99 references, `mark_revision` 40, across
**10 files in 5 crates** (`casual-doc-model` 3, `casual-doc-import` 3, `casual-doc-export`
2, `casual-doc-wasm` 1, `casual-doc-edit` 1). It is mechanical and fully covered by the
existing round-trip suite.

This alone does **not** admit the owner's file. It roughly halves the model, which at
1.3M paragraphs is the difference between 2.0 GB and 1.1 GB of permanently resident
memory — the headroom stage 2 needs in order to be worth doing.

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

### Projected result

For the owner's file (short paragraphs, so per-paragraph glyph cost is well below the
130-character probe): model ~650 B × 1.3M ≈ 845 MB resident, plus a bounded window of a
few hundred MB. **~1.15 GB — it fits.**

## 5. What is deliberately NOT proposed

- **Raising `MAX_VIEWER_BLOCKS` without stage 2.** The constant is a measured cliff. A
  larger number buys a module abort instead of an honest refusal, which is strictly
  worse (HF-158 exists because that is what used to happen).
- **Removing the refusal path.** Even after stage 2 there is a ceiling; it must keep
  saying what was found, what the limit is, and what to do.
- **Streaming the model to disk / IndexedDB.** Out of scope; local-first and
  no-mandatory-server (SKILL.md §1) are not at risk from an in-memory model of this size.

## 6. Reproducing these numbers

The probes used here were temporary examples under `crates/casual-doc-layout/examples/`
and were not committed; the committed `edit_latency` example produces the pagination
timings in §4. Per `docs/105` EV-rules a published number must derive from a committed
artifact — **if any number in this document is quoted outside it, commit the probe
first.** The numbers above are recorded as design input, not as a public claim.
