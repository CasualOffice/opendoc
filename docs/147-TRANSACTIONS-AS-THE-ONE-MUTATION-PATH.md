# 147 — Transactions as the one mutation path: revision chain, derived undo, and the choke-point guard

**Status:** Accepted for implementation (this design ships with the change it describes).
**Opened:** 2026-09-29.
**Implements:** `107` §2.1 P-1…P-3 (the prerequisite unification), ADR-005, ADR-033, ADR-043.
**Relates to:** `24-TRANSACTION-SEMANTICS.md`, `45-EXTENSIBILITY-AND-COLLABORATION-SEAMS.md`
(invariants I1–I4), `59-V1-EDITING-OP-SET.md`, `26-SELECTION-FOUNDATION.md`, ADR-030.

**Explicitly not in this document:** `transform`, any OT algorithm, any network, presence or
relay, and any new engine operation. This lane makes transform *possible*; it does not write
it. Adding an operation stays forbidden by ADR-030 I2.

---

## 1. The established pattern, named before any code

This is not a new problem. The shape is **command pattern over an append-only log** — event
sourcing with a single mutation choke point, where the *only* durable primitive is the ordered
sequence of applied commands and every other history feature is a projection of it.

The specific prior art, in the order it is closest to this engine:

| Prior art | What it contributes here |
| --- | --- |
| **ProseMirror** (`Transaction` → `Step[]` → `StepMap` → `Mapping`, with `prosemirror-history` as a *plugin over the transaction stream*) | The exact factoring adopted below: an envelope carrying a closed set of invertible steps, a position map per commit, and undo **derived** from the stream rather than maintained beside it. It is also in-house prior art — the sibling `docs` repo is a ProseMirror editor. |
| **CodeMirror 6** (`Transaction`, `ChangeSet`, `@codemirror/commands` history) | Confirms the same factoring independently, and the "one transaction per user gesture, coalesced by time and caret continuity" rule (`107` B3). |
| **Event sourcing / command sourcing** | The log is the system of record; snapshots are a *derived, compactable* optimisation, never the primitive. This is `107` §5 already. |
| **Memento vs. Command undo** | Why undo here is inverse-command replay and not snapshot-per-step: `24` already rejected snapshot-per-history-entry because memory scales with document size. |

What this repository already had, and what it was missing:

- It **had** the choke point (`45` I1): `WasmDocument::apply_group` is the one place that
  mutates the model, held there by an exhaustive source guard.
- It **had** the invertible closed op set (`45` I2): `casual_doc_edit::apply` returns the
  inverse of every operation.
- It **had** an envelope with revisions and a position map (`casual-doc-transaction`).
- It was missing **the join**: the envelope and the choke point were built on different
  document models, so the envelope was unreachable from the product, and history was a flat
  `Vec<HistoryEntry>` beside the model with no revision chain at all.

So the work is not invention. It is connecting two halves that were designed to fit and never
were, and then deleting the second mechanism the gap had grown.

---

## 2. What was actually measured, and where `107` is wrong

`107` §2 describes the blocker as *two operation sets*. That understates it by one layer, and
the difference changes the design. Measured on `main` at `91b1bd6`:

| Claim in `107` §2 | Measured | Consequence |
| --- | --- | --- |
| Two op sets: 5 vs 47 | Two op sets: **5 vs 55** (`casual_doc_edit::Operation`) | `107`'s tier table in §3.1 is sized against a stale count; it is corrected there, not here. |
| `casual-doc-wasm` references `casual_doc_transaction` 0 times | Confirmed — it was not even a dependency | ADR-005 was not honoured in practice. |
| — (not stated) | **The two stacks edit different document models.** `casual-doc-transaction`, `casual-doc-selection::TextSelection` and `casual-doc-sdk` operate on the Phase-0 `casual_doc_model::Document` (schema v0: paragraphs of marked text runs, 254 lines, no tables, no sections, no objects). `casual-doc-edit`, `casual-doc-wasm`, layout, import and export operate on `casual_doc_model::v1::Document` (~10k lines). | This, not the op-set size, is why the live path *could not* call the transaction engine. `docs/14` P1G-006e recorded it ("different types, no v1→v0 map"); `107` did not carry it forward. |
| — (not stated) | **The two op sets use different position spaces.** v0 positions are *extended grapheme* offsets with `Affinity`; v1/`casual-doc-edit` positions are *UTF-8 byte* offsets with no affinity (`59`, `58` §3 — the same space hit-testing uses). `24`'s "Rejected Alternatives" explicitly rejected UTF-8 operation offsets. | `107` §2.1's "make `casual-doc-transaction` depend on `casual-doc-edit`" silently reverses a recorded Phase-0 decision. ADR-043 takes that decision explicitly instead of by accident. |

`107` §2.1 P-1 says the 5-operation enum "is deleted". That is only safe for the **live** path.
The v0 enum also backs `casual-doc-sdk`'s entire public surface — `DocumentSnapshot`,
`Position`, `InsertTextRequest`, `PositionMap` — which is a Phase-0 facade with no product
consumer (`126`: *"`crates/casual-doc-sdk` exists and has no product consumer"*) and which
cannot represent a real document at all. Deleting it means rewriting that public API against
v1, which is `126`'s work, not this lane's, and is exactly the kind of setback this change is
forbidden to cause. §6 states the boundary that results.

---

## 3. The design

### 3.1 Crate layering

`casual-doc-transaction` becomes the envelope over the **one live op set**, and is
re-founded on the model the product actually edits:

```
casual-doc-model  (v0 legacy  |  v1 real)
        |                         |
        |                 casual-doc-edit      Operation — the ONE live op vocabulary (I2),
        |                         |            apply() -> inverse, mutates v1 in place
        |                         |
        +------> casual-doc-transaction        root:    envelope over v1 + edit::Operation
                       |                       ::v0:    the Phase-0 engine, unchanged, quarantined
                       |
         +-------------+--------------+
         |                            |
  casual-doc-selection          casual-doc-sdk      (both still on ::v0 — see §6)
                                       |
                                casual-doc-wasm     routes EVERY mutation through the root
```

The dependency `casual-doc-transaction → casual-doc-edit` is new and is the inversion that
makes the join possible: the envelope is defined *over* the op set rather than owning a rival
one. There is no new crate and no third vocabulary.

### 3.2 The envelope

```rust
pub struct Transaction {                 // an intent, not yet applied
    id: TransactionId,
    base_revision: RevisionId,
    label: Label,                        // the engine's user-facing step vocabulary
    coalesce: Coalesce,                  // New | Continue | ContinueKeepingFirstInverse
    operations: Vec<Operation>,          // casual_doc_edit::Operation — the one set
}

pub struct Commit {                      // an applied transaction: one link in the chain
    id: TransactionId,
    base_revision: RevisionId,
    revision: RevisionId,                // strictly base_revision + 1
    group: GroupId,                      // undo granularity (§3.4)
    label: Label,
    origin: Origin,                      // Edit | Undo { group } | Redo { group }
    operations: Vec<Operation>,          // forward — the OT substrate
    inverse_operations: Vec<Operation>,  // in the order that undoes them
    position_map: PositionMap,
}
```

`RevisionLog::apply(document: &mut v1::Document, ids: &mut dyn RunIds, tx) -> &Commit` is the
only way to produce a `Commit`. It:

1. refuses a stale `base_revision` (`ODC-2001` semantics are preserved by the SDK's own path);
2. refuses an empty operation list;
3. takes a working snapshot **only when `operations.len() > 1`** — a single operation needs
   none, because `casual_doc_edit::apply` validates before it mutates and its
   `a_refused_operation_leaves_the_document_unchanged` asserts it. This is the rule
   `apply_group` already had; it moves, it does not change;
4. applies each operation in order, collecting the inverse and any mapping step;
5. reverses the inverses, allocates the next revision, and appends the `Commit`.

**It does not clone the document, and it does not validate the whole model.** `24`'s atomic
pipeline steps 2 and 5 ("clone the current normalized document", "validate complete model
invariants") are both O(document) and would violate `107` B1 on every keystroke. They remain
where they were affordable — on the v0 SDK path, which already clones — and are named as the
reason the SDK keeps its own pipeline in §6. `24` is corrected in place.

### 3.3 The revision chain

```rust
pub struct RevisionLog {
    commits: VecDeque<Commit>,   // append-only, ordered, bounded by undo groups
    head: RevisionId,
    next_group: u64,
}
```

The document knows its own history as an **ordered sequence**, not a stack. `head()` is the
current revision; every applied transaction — forward, undo or redo — appends exactly one
commit and advances `head` by one. Nothing rewrites or pops a commit; the only removal is
compaction from the **front** (§3.5), behind the undo horizon.

This is the artifact `107` §5 needs: `Snapshot(r0) ─ op(r1) ─ … ─ op(rN)`. Durability,
compaction and replay are `107` 6.1 and are not built here; the in-memory chain is.

### 3.4 Undo, derived

Undo is no longer a second store. It is a **read of the log**.

*Granularity.* One user action is one `GroupId`. A coalesced typing run is several commits
(one per keystroke — the OT substrate must keep each) sharing one group. `Coalesce::Continue`
puts the new commit in the previous commit's group; `Coalesce::New` opens a fresh one.

*Which group undo targets.* A backward scan of the commits, cancelling as it goes:

```
undo target  = scan back; each Undo commit cancels the next Edit-or-Redo group found
redo target  = scan back; each Redo commit cancels the next Undo group found;
               an Edit commit terminates the scan
```

The second rule is where "a fresh edit clears redo" comes from: it is a *consequence* of the
log's order, not a `redo.clear()` call anywhere. This is the standard linear-undo reading of
an event log and is what `prosemirror-history` does over its plugin state.

*How undo applies.* The target group's commits are read newest-first and their
`inverse_operations` concatenated; that list is applied as a **new transaction**, which
appends a commit with `origin = Undo { group }`. History is never rewritten — `107` §5.3's
restore rule, one layer down.

*Labels.* `Commit::label` carries the engine's existing step vocabulary ("Typing", "Table
structure", …). A group's label is its newest commit's label, which reproduces today's
behaviour exactly: the old code re-pushed the merged entry with a fresh `kind` each keystroke.
An `Undo`/`Redo` commit carries the label of the group it acts on, as the old code carried
`entry.kind` across the pop/push.

*Inverse compaction.* `Coalesce::ContinueKeepingFirstInverse` records a commit's forward
operations but contributes **no** inverse to the group. This is the review-typing rule the old
code spelled as "keep only the first paragraph inverse": a `SetInlines` inverse is a whole
paragraph snapshot, and one word must not retain one snapshot per character. The forward
operations are still logged in full, so nothing OT needs is lost — strictly more is kept than
before.

### 3.5 Bounds

`MAX_HISTORY_ENTRIES = 256` was a bound on *undo steps*. The log preserves that bound as a
bound on **groups**: when a 257th group is opened, the oldest group's commits are dropped
whole. Bounding by commits instead would have silently shortened undo depth for anyone who
types (a 60-character word is 60 commits and one step), which is a behaviour change disguised
as a constant.

Memory is roughly 2–2.5× the old history for the same depth, because a commit keeps its
forward operations as well as its inverse. That is the price of the OT substrate and it is
bounded, not unbounded. `107` B7's compaction is where it gets cheaper.

### 3.6 Position mapping

`MappingStep`/`PositionMap` at the crate root move to the **byte** space of the live op set,
and keep `Affinity` — affinity is orthogonal to the unit and is load-bearing for OT's
insert-at-the-same-boundary tie-break (`107` §3.3). Steps are emitted for the four positional
shapes the v0 map already covered (`InsertText`, `DeleteText`, `SplitParagraph`,
`JoinParagraphs`); every other operation emits none, which is exactly the coverage that
existed before. Extending this to the node-addressed operations is `107` §3.2 / P-4 and is
deliberately left open (§7).

Emitting a step is O(1) — the offsets are already on the operation — so this costs nothing per
keystroke.

---

## 4. Performance: what makes this O(1) in document size

`107` §4 B1 is the binding constraint. Per keystroke, the new path does:

| Step | Cost |
| --- | --- |
| Build a one-operation `Transaction` | O(1) |
| Base-revision check | O(1) |
| Snapshot | **none** — single-operation transactions take no working copy |
| `casual_doc_edit::apply` | O(edit) — unchanged, this is what ran before |
| Mapping step | O(1) |
| Append the commit, resolve the coalesce group | O(1) amortised |
| Model validation | **none** — was never on this path and is not added |
| Re-layout | unchanged: incremental, driven by the same `DirtySet` |

The two things that would have broken B1 — `24`'s per-transaction document clone and its
per-transaction `validate()` — are the two things deliberately left on the v0 path. A design
that moved them onto the keystroke path would have passed every correctness test and failed
this task.

**Guarded, not asserted.** `the_transaction_log_keeps_typing_flat_in_document_size` builds
documents of *n* and *2n* paragraphs, types the same run into each, and asserts the block
visits the edit path makes do not scale with the document (SKILL §8: guard complexity by
doubling, never by a clock). A timing threshold cannot tell a slow constant from a quadratic
and is flaky under load; this counts work.

---

## 5. The exhaustiveness guard

Without a guard the unification decays silently: the next edit path added is one direct
`casual_doc_edit::apply` call away from bypassing the chain, and nothing user-visible breaks
when it does — which is the worst possible failure mode, because the log then *looks* complete.

`every_document_mutation_is_a_transaction` extends the existing
`every_model_mutation_goes_through_the_atomic_choke_point` (same file, same source-scanning
shape, same CRLF normalisation via `engine_source` — two Windows-only CI failures came from
literals containing `\n` not matching a CRLF checkout). It reads the crate's own source and
fails the build unless:

1. `casual_doc_edit::apply` has **zero** call sites in `casual-doc-wasm` — the engine may no
   longer reach the op set directly at all; it reaches it only through the envelope;
2. the envelope's `apply` has **exactly one** call site, and that call site is inside
   `apply_group`;
3. every history read (`can_undo`, `can_redo`, `undo_label`, `redo_label`, `undo_inner`,
   `redo_inner`) resolves against `self.log`, and no parallel history stack field survives on
   the editor state;
4. `apply_group` is the only place a `Transaction` is constructed, so an operation cannot be
   applied outside a transaction envelope.

**What it cannot see.** Stated plainly, because a guard's blind spots are part of its
contract:

- It is a **source-level** guard on one crate. Another crate taking `&mut v1::Document` and
  calling `casual_doc_edit::apply` is invisible to it. Today only `casual-doc-wasm` and
  `casual-doc-edit`'s own tests do; `casual-doc-import` builds documents before any session
  exists, which is construction, not mutation.
- It cannot see mutation that does not go through `casual_doc_edit::apply` at all — a direct
  `document.body_mut()` write. `casual-doc-edit`'s own container guard covers the shape of
  that risk inside the edit crate; nothing covers it workspace-wide. Recorded in §7.
- It checks *shape*, not semantics: a transaction with the wrong operations still passes.
  Behaviour is what the rest of the suite is for.
- A macro or a re-export could alias the call and defeat the text match. That is true of the
  guard it extends, and is the cost of source scanning; it is a real bound, not a theoretical
  one.

Both guards are proven red by mutation before they are trusted (SKILL §4).

---

## 6. The boundary of this change, stated honestly

**On the new path (the live editing path, and the one OT needs):** `casual-doc-wasm` — every
mutation, every undo, every redo. One operation vocabulary, one envelope, one log.

**Still on the old path:** `casual-doc-sdk` and `casual-doc-selection::TextSelection`, through
`casual_doc_transaction::v0`. They edit the schema-v0 model, which no product surface renders,
opens or saves. Their code is moved into a `v0` module and is otherwise **unchanged** — same
operations, same grapheme positions, same clone-and-validate pipeline, same `ODC-*` codes, same
tests.

That boundary is deliberate and it is the whole of what is left:

- Moving them would mean re-expressing `DocumentSnapshot`, `Position`, `SelectionSnapshot` and
  four command requests against a model 40× larger — the SDK's public API, rewritten. That is
  `126`'s Phase work.
- It is **not** a half-migrated choke point. The two paths do not share a document: nothing can
  edit a v1 document except through the envelope, and nothing can edit a v0 document except
  through the v0 pipeline. There is no document reachable by both, so there is no state where
  "some edits are in the chain and some are not".
- The `v0` namespace is the marker. `casual_doc_transaction::v0::Operation` names itself as the
  Phase-0 vocabulary at every call site, which is what makes the remaining work greppable
  instead of forgotten.

---

## 7. Open questions

Recorded rather than hidden, per `AGENTS.md`.

1. **The v0 path's retirement.** `126` owns the v1 SDK. Until then the workspace holds two
   operation vocabularies over two document models; only one is reachable from the product.
   Closing this is what finally lets `107` §2.1 P-1 be written as it is worded.
2. **`Label` is `&'static str`.** Fine in memory; a persisted log (`107` 6.1) needs a stable
   serialisable code with a schema-version policy, exactly as `107` §8 Q5 says of the operation
   enum itself. Choosing that vocabulary is that lane's work.
3. **Mapping steps for node-addressed operations.** `107` §3.2's `NodeInserted`/`NodeRemoved`/
   `NodeReplaced`/`PropertiesChanged` are not emitted. Until they are, a selection cannot be
   rebased across a concurrent structural edit — which is `107` P-4, and which the tier
   classification in `107` §3.1 also still lacks per-operation.
4. **Workspace-wide mutation guard.** §5 guards one crate's source. A workspace-wide rule —
   "only `casual-doc-transaction` may call `casual_doc_edit::apply`" — needs either a lint or a
   visibility change (`pub(crate)` plus a re-export), and the second would break
   `casual-doc-edit`'s own test surface. Not attempted here.
5. **Group memory.** §3.5 bounds groups, not commits within a group. A pathological single
   gesture (a 100,000-character run typed without a caret discontinuity) is one group and is not
   bounded. The old code had the identical exposure; naming it is new.
6. **Two `PositionMap` types** exist while §6's boundary stands — byte-space at the root,
   grapheme-space in `v0`. They must not be allowed to drift into "the general one"; the v0 one
   is frozen and is deleted with the v0 path.
