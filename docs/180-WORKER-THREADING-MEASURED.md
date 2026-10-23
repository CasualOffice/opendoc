# 180 — Worker threading: what blocks the main thread, and what a worker can take

**Status:** Measured and designed 2026-10-24. **Decides:** ADR-073 (proposed). **Row:**
`docs/109` RM-13. **Builds on:** `56` (editor shell and render architecture, its threading
section), `113` (windowed layout), `116` (the main-thread budget audit and its §7 open-path
design), `107` §4 (the editing budgets), ADR-033 (collaboration is OT). **Artifacts:**
`webapp/tests/e2e/main-thread-measurement.spec.mjs`,
`webapp/tests/e2e/large_docx_fixture.mjs`,
`crates/casual-doc-wasm/examples/open-handover-timing.rs`.

The lane was asked for two things: measure the main-thread blocking during open, paste and a
style change, and then build the first slice, which was to run the open (import plus first
layout) in a Web Worker with progress, Cancel and a main-thread fallback, guarded by "no
main-thread task over 200 ms while the large fixture opens".

The measurements below show that this slice cannot meet its guard on the current engine, and
that shipping it anyway would move the freeze rather than remove it. So this document records
the measurements, the design the worker should have, and the engine work that has to land
first. It does not record a worker that was built.

---

## 1. The question

`SKILL.md` §8 says anything O(document) runs off the main thread with progress and cancel.
Today every O(document) step runs on the main thread: import, pagination, and (as §2.2 shows)
the first edit after the open. `56` planned a worker "when responsiveness demands" it, assuming
`SharedArrayBuffer` and `OffscreenCanvas`. `116` §7 rejected a worker *for that change* because
the editor calls the engine synchronously. Neither measured what a worker could actually hand
back to the main thread. That measurement decides the design, so it comes first.

## 2. Main-thread blocking today, measured

Measured in Chromium (Playwright, `chromium` project) on an arm64 Mac that was also running
other builds, so treat each figure as ±20%. The ratios between rows are stable across runs.
To reproduce:

```sh
cd webapp && ./build.sh
MEASURE_MAIN_THREAD=1 MAIN_THREAD_PARAGRAPHS=2000,20000,100000 \
  npx playwright test main-thread-measurement --project=chromium --timeout=1500000
```

The large fixture is generated rather than committed (`large_docx_fixture.mjs`). It has a
Heading 1 every 25 paragraphs and two sentences of prose in each of the others, which wrap to
three or four lines on a Letter page. At 2,000 paragraphs it is 263 pages, at 20,000 it is
2,629 pages, and at 100,000 it is about 13,000 pages. All three sizes are below the engine's
whole-layout ceiling (`MAX_WHOLE_LAYOUT_BLOCKS` = 262,144), so all three open as *editable*
documents. That is the regime this lane is about.

"Longest task" is the longest `PerformanceObserver` `longtask` entry. "Time to interactive" runs
from handing the file to the picker (or dispatching the edit) until the main thread has gone
500 ms without a long task.

| operation | longest task | time to interactive | tasks > 50 ms |
| --- | ---: | ---: | --- |
| open `sample.docx` (148 blocks, 14 pages) | 118 ms | 145 ms | [118] |
| paste 2,000 lines into `sample.docx` | 100 ms | 107 ms | [100] |
| apply a style in `sample.docx` | 59 ms | 701 ms | [59, 55] |
| select all + bold in `sample.docx` | 120 ms | 183 ms | [120] |
| open large-2,000 | 279 ms | 642 ms | [279, 266] |
| first keystroke in large-2,000 | 227 ms | 235 ms | [227] |
| second keystroke in large-2,000 | < 50 ms | — | [] |
| paste 2,000 lines into large-2,000 | 129 ms | 138 ms | [129] |
| select all + bold in large-2,000 | 369 ms | 580 ms | [71, 369, 76] |
| **open large-20,000** | **3,264 ms** | **7,627 ms** | [3264, 425, 135, 3217, 413] |
| **first keystroke in large-20,000** | **3,100 ms** | 3,775 ms | [3100, 131, 353] |
| second keystroke in large-20,000 | 353 ms | 776 ms | [93, 121, 353] |
| paste 2,000 lines into large-20,000 | 1,152 ms | 2,115 ms | [818, 128, 1152] |
| apply a style in large-20,000 | 1,840 ms | 2,937 ms | [671, 1840] |
| select all + bold in large-20,000 | 6,576 ms | 11,109 ms | [2209, 117, 283, 6576, 119, 1764] |
| open large-100,000 | 30,885 ms | 73,189 ms | [29091, 494, 5663, 232, 627, 30885, 526, 5562] |
| first keystroke in large-100,000 | 24,972 ms | 33,011 ms | [24972, 942, 5527, 1137, 407] |
| second keystroke in large-100,000 | 5,085 ms | 8,641 ms | [1213, 944, 5085, 975, 399] |

### 2.1 The open is two whole-document passes, not one

At 20,000 paragraphs the open has two tasks of about 3.2 s each. The first is `open()`, which
does admission, import and `paginate_document`. The second is the web-font upgrade:
`openBytes` paints from the bundled metric-compatible faces first, then fetches the named
families and calls `registerFonts`, and `repaginate()` lays the whole document out again (`116`
§3.1 found the same thing on the windowed path). The ~420 ms tasks after each pass are the host
rebuilding page records, the outline and the accessibility tree.

### 2.2 The first keystroke pays for the open a second time

The first keystroke in large-20,000 blocks for 3.1 s and the second for 353 ms. The cause is in
`crates/casual-doc-wasm/src/lib.rs`. Every whole layout outside the edit path is built without
the galley cache:

- the open (`open_document_bounded` calls `paginate_document`, with the comment "populated
  lazily on the first edit's incremental re-pagination");
- the font upgrade (`repaginate` starts with `self.galley_cache = GalleyCache::new()`);
- fold and view changes (`relayout_for_fold`, `set_layout_view`).

So the first edit after any of these runs `paginate_document_after_edit_folded` against an
empty cache and shapes every paragraph again. The shaping the open did is thrown away.

**This is the fact that decides the slice.** If the open moved into a worker and nothing else
changed, the main thread would still shape the whole document on the first keystroke. The
3.2 s task would move from "open" to "first keystroke", and the guard would pass while the
user's experience got no better.

### 2.3 Every keystroke is O(document) as well

The second keystroke in large-20,000 still costs 353 ms, and in large-100,000 it costs 5.1 s.
The same spec with `MAIN_THREAD_PROFILE=1` takes a sampled CPU profile of one more keystroke.
At 20,000 paragraphs it attributes 467 ms, inclusive, to the spell checker's post-edit scan:

- `spell_check.mjs` `paragraphsInWindow`, at 381 ms, calls `pageOf` → `doc.caretRect(node, 0)`
  for each paragraph it walks;
- `caretRect`, at 466 ms inclusive, is where the time goes. Its cost grows with the document,
  so the scan its doc comment calls "nothing proportional to the document's size" makes
  O(window) calls that each cost O(document).

`updateStats` → `doc.documentStats()` adds 93 ms per keystroke. That is a whole-document text
walk. Both break `107` §4 (per-keystroke work is O(1) in document size) and `SKILL.md` §2.
Neither is a threading problem: they belong to the main-thread budget programme (`116`,
`MTB-001`), and §6 lists them as prerequisites.

## 3. Where the open's time goes, and what can cross a thread

### 3.1 The split

Native release build, `cargo run --release --example open-handover-timing -p casual-doc-wasm
-- <file>`, on the same fixtures:

| fixture | import | paginate | model JSON | decode model | pages JSON | decode pages | adopt / compute |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `sample.docx` | 0.021 s | 0.042 s | 0.6 MB | 0.008 s | 0.9 MB | 0.004 s | 5.2× faster |
| large-2,000 | 0.029 s | 0.402 s | 1.2 MB | 0.008 s | 40.5 MB | 0.183 s | 2.3× faster |
| large-20,000 | 0.297 s | 3.904 s | 12.2 MB | 0.044 s | **406 MB** | **1.784 s** | 2.3× faster |

Pagination (shaping plus placement) is 93% of the open at 20,000 paragraphs. That agrees with
`116` §3, which measured 88% on the owner's 1.3M-paragraph file.

### 3.2 What a worker could hand back

Without `SharedArrayBuffer`, a worker and the main thread have separate wasm heaps, and only
bytes cross between them. A worker open helps the editor only if the main thread can adopt the
result for less than it costs to compute. The probe measures this directly:

- **The model crosses cheaply.** The normalized model of large-20,000 is 12 MB of JSON and
  decodes in 44 ms, about 7× cheaper than importing the DOCX.
- **The laid-out pages do not.** The same document's `PaginatedLayout` is 406 MB of JSON and
  decodes in 1.78 s, only 2.3× cheaper than computing it, and it is held as one buffer. Almost
  all of it is glyph runs. A hand-written binary codec might cut the bytes 3 to 4×, but it would
  still be O(document) on the main thread. It would also be a second encoding of every layout
  type, and every field added to the layout would be a place for that encoding to drift.
- **The import's preservation state does not cross at all.** `SourceEnvelope`
  (`casual-doc-io/src/artifact.rs`) is format-specific preservation state with no
  serialization. Without it the main thread's copy would lose verbatim OOXML retention on
  save. That would be a silent data-loss regression (`SKILL.md` §1, §12).

The editor's synchronous use of the engine sets the rest of the shape. A rough count of the
members the editor reads as `doc.X` or `ed.doc.X` finds 291 distinct engine members, which
undercounts because modules also alias the document locally:

```sh
grep -oh '\b\(ed\.\)\?doc\.[a-zA-Z_]\+' webapp/src/*.mjs webapp/src/main.js | sed 's/^ed\.//' | sort -u | wc -l
```

The caret, hit testing, selection rectangles and painting all read the laid-out pages on every
input event. The editable document therefore has to live where its layout lives, and today
that is the main thread.

### 3.3 Constraints that hold regardless

- **No `SharedArrayBuffer`.** GitHub Pages does not send COOP/COEP. A service worker can add
  them (the `coi-serviceworker` technique), but shared wasm memory also needs the engine built
  with `+atomics` and a rebuilt `std`. That is a nightly-only build, and the toolchain is pinned
  to stable 1.96.0. `56`'s plan to share linear memory is therefore unavailable, not merely
  deferred, and it stays unavailable to embedders who cannot set those headers.
- **The module is 24 MB** (`build.sh`'s size table). A worker must not download or compile it
  again. It is instantiated from the `WebAssembly.Module` the page already compiled, which
  `postMessage` can clone. A second instance still allocates its own copy of the data segment
  (font bytes included) and its own document, so a worker roughly doubles resident memory while
  it holds a document.
- **There is already one module worker**, the proofing worker (`spell_check.mjs` →
  `proof_worker.js`), with a transport that degrades to an in-process implementation when
  `Worker` is missing, construction throws, or no ready message arrives before a deadline. An
  engine worker uses the same transport shape, and not a second mechanism.

## 4. The design (ADR-073)

**Pattern: an actor with a command queue.** The worker is a single-threaded mailbox that owns a
second engine instance. The main thread posts commands, each with a request id and a document
generation. The worker answers with `progress`, `result` or `error` for that id. It runs one
command at a time, and a command for a generation older than the newest is answered with
`cancelled` without being run. wasm cannot be interrupted part-way through a call, so the only
way to pre-empt a running command is `worker.terminate()`. Cancel is therefore a generation
bump plus terminate-and-respawn, which is cheap because the compiled `Module` is reused. The
worker is disposable by design.

**What stays on the main thread.** Input, IME, the caret, selection, hit testing, the editing
choke point (`WasmDocument` and, once ADR-005 is honoured, `casual_doc_transaction`), and the
layout of the pages the user can see. Nothing on the keystroke path makes a message round trip,
so per-keystroke latency cannot get worse because of the worker.

**What crosses, and what never does.** Bytes go in, transferred and not copied. What comes out
is summaries whose size does not grow with the glyph count:

- the normalized model, which §3.2 measured as cheap;
- the measure tier (`DocumentMeasures`: page outlines and resumable checkpoints), which is what
  `113` already uses to make the page count exact without a resident galley;
- page rasters, as transferred `ImageBitmap`s, for read-only surfaces: thumbnails, print,
  Compare and version previews.

The paint tier (laid-out glyph runs) never crosses (§3.2).

**How the canvas gets pages.** The editable pages are painted on the main thread from the main
thread's engine, exactly as today. The caret and selection overlay have to agree with the
painted text in the same frame, and an `OffscreenCanvas` owned by another thread cannot give
that guarantee. Worker-painted surfaces arrive as `ImageBitmap`s and are drawn with
`drawImage`. `OffscreenCanvas` transfer is rejected for the editing surface, and it is not
needed for the read-only ones.

**Progress and Cancel for open.** The phases are admitted, imported, measuring (*n* of *N*
blocks, from the measure tier's own committed-block counter), and ready. Cancel restores the
previous document. `openBytes` already follows that rule for a failed parse: parse before swap,
and swap only on success.

**Fallback.** If there is no `Worker`, module-worker construction throws, a CSP forbids it, or
the worker sends no ready message before the deadline, the editor opens on the main thread as
it does today. The bytes and the result are the same, and so is the same proofing-transport
code path.

**What the main thread must become first.** The decisive constraint from §2.2 and §3.2 is that
the main thread holds the editable document, and so it holds the editable document's layout.
A worker can only shorten the open if that layout is *lazy*: built for the visible window and
extended in bounded idle ticks, as `113` and `116` §7 already do for the read-only windowed
body. Until whole documents can be laid out that way and stay editable, any worker open hands
the main thread an O(document) layout to build, either at open or on the first keystroke.

## 5. Rejected

- **Hand the laid-out pages from the worker to the main thread.** 406 MB and 1.78 s at 20,000
  paragraphs (§3.2). That is O(document) on the main thread, and only 2.3× cheaper than the
  work it replaces.
- **Move the whole engine into the worker and make `ed.doc` an asynchronous proxy.** That
  rewrites the 291 members (§3.2) to be asynchronous and puts a round trip on every caret
  move. This is `116` §7's rejection, and the measurements here confirm it.
- **Shared memory (`SharedArrayBuffer`, wasm threads).** No COOP/COEP on Pages, and it needs a
  nightly `std` (§3.3).
- **A "worker open" slice that ships now.** It would either pass its guard while moving the
  freeze to the first keystroke (§2.2), or fail its guard. Either way it would be a feature
  that looks done and is not (`SKILL.md` §9.4).
- **Time-slicing the existing `open()` on the main thread with yields.** `paginate_document`
  is one uninterruptible call. Yielding needs the same resumable layout that §4 requires, and
  once that exists the yielding is free.

## 6. Order of work

Each step is useful on its own and is a prerequisite for the step after it. Lanes are named by
file domain (`SKILL.md` §7).

1. **E1 — one shaping pass per full layout** (wasm lane, `lib.rs`). Build the open's layout,
   the font upgrade's `repaginate` and the fold/view re-layouts through the galley cache
   (`paginate_document_after_edit_folded` with `DirtySet::everything()` and no previous
   layout), so the first edit resumes instead of reshaping. Expected effect at 20,000
   paragraphs: the first keystroke drops from 3.1 s to the second keystroke's 0.35 s. The open
   itself does not change. Not done in this lane, because the lane was limited to additive
   `lib.rs` changes and this one modifies existing functions. Guard: a wasm test asserting
   `shaped_last_build() == 0` for a one-character edit immediately after the open and after
   `registerFonts`.
2. **E2 — per-keystroke reads that scan the document** (webapp and wasm). Give `caretRect` an
   indexed page lookup, or give the spell checker a page-range paragraph enumeration that does
   not go through `caretRect`, and make `documentStats` incremental or deferred to idle time.
   This addresses the 353 ms / 5.1 s rows. Guard: extend `main-thread-budget.spec.mjs`'s
   `OPERATIONS` with "a keystroke", plus a Rust n-versus-2n doubling test for `caretRect`.
3. **E3 — a lazy, editable whole layout** (layout lane). Apply `116` §7's prefix-and-extend to
   whole documents, and keep the document editable while the tail is being measured. An edit
   above the measured frontier re-measures from its checkpoint, and an edit below it is
   measured when the frontier reaches it. This is what makes the main thread's open
   O(window). It needs the `FlowResume::FromStart` classes (multi-section, floats, footnotes,
   line numbering) handled, or refused with a reason, rather than quietly opened whole.
4. **W1 — the engine actor and the open in a worker** (webapp; additive `lib.rs`). The worker
   imports and runs the measure pass, then hands back the model and the measure tier. The main
   thread adopts the model, materializes the first window, and takes the measure tier instead
   of measuring. Progress, Cancel and fallback as in §4. This needs `SourceEnvelope` to cross,
   or a decision that the main thread re-derives it from the original bytes it already keeps
   for the draft key. That decision belongs to the io lane, and it is recorded as Q2. Guards:
   the three this lane was asked for. No main-thread task over 200 ms while large-20,000 opens
   (`PerformanceObserver` long tasks). Cancel restores the previous document and leaves it
   editable. With `Worker` removed, the open succeeds on the main thread and produces the same
   page count.
5. **W2 — read-only surfaces from the worker.** Thumbnails, print and PDF raster, and Compare
   and version previews, rendered as `ImageBitmap`s from a worker replica. The replica is kept
   current by forwarding committed transactions, using the same op stream collaboration ships
   (ADR-033), so this waits for the op-set unification in `SKILL.md` §2.

## 7. Open questions

- **Q1.** E3 lets the user edit while the page count is still an estimate. On the windowed
  path the status bar already reads `pageCountIsExact` and shows `~`; `116` §7 item 2 says
  `NUMPAGES` and print must read the same flag, which E3 has to deliver for whole documents.
  Is an estimated `Page X of ~Y` acceptable while editing, as Word shows while it repaginates
  in the background? The owner should confirm.
- **Q2.** `SourceEnvelope`: serialize it, or have the main thread re-derive it from the
  original bytes? The second avoids a new encoding, but it puts an import-sized pass back on
  the main thread at the first save.
- **Q3.** The font-upgrade pass (§2.1) doubles the open whenever the named web fonts are not
  already registered. The font lane should decide whether faces found in the HTTP cache are
  registered before the first layout instead of after it.
