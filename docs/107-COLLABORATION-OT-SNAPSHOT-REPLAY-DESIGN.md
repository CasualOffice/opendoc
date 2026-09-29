# 107 — Collaboration: OT over Transactions, with Snapshot/Replay Versioning

**Status:** Proposed — design for discussion, not yet accepted.
**Opened:** 2026-09-15.
**Owner decision taken (2026-09-15):** operational transformation, carried by transactions,
with snapshot-plus-replay providing versioning. This resolves the long-open
"collaboration operation model: OT vs CRDT" question in `08` §Pending Decisions, and
supersedes the recommendation in `106` §9 Q5, which argued for the cheaper ordered-log
model. The owner's call is recorded and this document designs to it.

**Constraint taken with it:** *editing must stay light.* Treated here as a binding,
measurable budget (§4), not an aspiration.

**Relates to:** ADR-004 (no DOM as source of truth), ADR-005 (mutation through commands and
transactions), **ADR-043 / `147`** (the §2.1 unification, as built — read it with §2 below),
**ADR-045 / `150`** (the transform itself, as built — read it with §3 below; it corrects §3.1,
§3.3, §8 Q1 and §8 Q4 in place),
**ADR-047 / `152`** (6.6's foundation — the protocol, the two session state machines, the
identity discipline and the rollback/replay driver; it corrects §8 Q6 and §7 6.6 in place),
ADR-006 (collaboration is adapter-based), ADR-030 / `45` (extensibility
invariants I1–I4), `24-TRANSACTION-SEMANTICS.md`, `25-NORMALIZED-SNAPSHOT-IO.md`,
`26-SELECTION-FOUNDATION.md`, `59-V1-EDITING-OP-SET.md`, `82-REVIEW-IDENTITY-AND-HISTORY-DESIGN.md`,
`106` Phase 6, `143` provider/embed architecture, and `144` phased execution
checklist.

---

## 1. Why OT is the right call here, despite the cheaper option

`106` §3 A3 established that ONLYOFFICE — the product this one is positioned against —
uses neither OT nor a CRDT, but a server-ordered change log plus pessimistic object locks.
Matching that is cheaper. The owner chose OT anyway, and the reasons hold up:

1. **The hard prerequisite is already paid.** OT needs a closed set of operations that can
   be transformed. `casual-doc-edit` is exactly that: 47 operations, closed by invariant I2,
   each returning its own **inverse** so undo is inverse-application. A design that already
   has invertible granular ops is most of the way to one that has transformable ops.
2. **Position mapping already exists and is already affinity-correct.**
   `casual-doc-transaction::PositionMap::map` maps a position through a committed
   transaction's steps and handles the insert-at-the-same-boundary tie with `Affinity`. That
   is the core of OT's positional transform, written and tested.
3. **Locks are a worse user experience and a worse fit.** Pessimistic object locks mean a
   second user is *refused*, and refusal needs a server to arbitrate. Locks are therefore
   structurally hostile to A1 (local-first), because the lock authority must be online.
4. **OT subsumes the versioning requirement.** Once a durable ordered operation log exists,
   snapshot-plus-replay gives version history, restore, and document comparison as
   consequences rather than as three separate features (§5). The ordered-log model gives the
   log but not the transform, so offline-then-merge stays impossible.
5. **Offline-then-merge is the whole point of local-first.** A local-first editor whose
   changes cannot be merged after a disconnection is local-first in name only. OT delivers
   this; locks cannot.

**What OT costs, stated plainly.** A transform function for every ordered pair of concurrent
operations that can touch the same state; a convergence property (TP1) that must be proven,
not assumed; and a discipline that every future operation added to the op set arrives with
its transform rules. §3 makes this tractable; §8 is honest about what is not yet designed.

---

## 2. The blocker: there are two op sets, and the live editor uses the wrong one

> **Corrected 2026-09-29 by `147` (ADR-043), which implements §2.1 P-1…P-3.** Three things
> below were wrong or missing when this section was written, and the third changed the design:
>
> 1. **`casual-doc-edit` has 55 operations, not 47.** The tier table in §3.1 is sized against
>    the stale count.
> 2. **The two stacks do not merely have different op sets — they edit different document
>    models.** `casual-doc-transaction`, `casual-doc-selection::TextSelection` and
>    `casual-doc-sdk` operate on the Phase-0 schema-**v0** `casual_doc_model::Document`
>    (paragraphs of marked text runs; no tables, sections or objects). Everything the product
>    imports, renders, edits and exports is `casual_doc_model::v1::Document`. That, not the
>    op-set size, is *why* the live path could not call the transaction engine, and it is why
>    P-1 cannot be executed as worded: deleting the 5-operation enum outright would delete
>    `casual-doc-sdk`'s entire public API. `147` §6 states the boundary actually taken — the
>    envelope moves to v1 and the Phase-0 engine is quarantined as
>    `casual_doc_transaction::v0`, unchanged, until `126`'s v1 SDK retires it.
> 3. **The two op sets use different position spaces**, which §2.1 did not notice it was
>    choosing between: v0 is extended-grapheme with `Affinity`, v1 is UTF-8 byte offsets — and
>    `24` had explicitly *rejected* byte offsets. ADR-043 takes that reversal deliberately and
>    corrects `24` in place.
>
> Also corrected: §4 B1 is incompatible with `24`'s atomic pipeline, which clones the document
> and revalidates it per transaction. The live envelope does neither; see `147` §3.2 and §4.

This is the most consequential finding in the 2026-09 audit round, and it must be fixed
before any OT work begins. Verified directly:

| | `casual-doc-transaction` | `casual-doc-edit` |
| --- | --- | --- |
| Operations | **5** — `InsertText`, `DeleteRange`, `SplitParagraph`, `JoinParagraph`, `InsertRuns` | **47** — the full authoring surface |
| Has revisions | `RevisionId`, `TransactionId`, `Commit`, `base_revision` | none |
| Has position mapping | `PositionMap` / `MappingStep` (4 kinds) | none |
| Has inverses | no | **yes, every operation** |
| Depends on the other | — | **no dependency at all** |
| Used by | `casual-doc-sdk`, `casual-doc-selection` | `casual-doc-wasm` (14 references) |
| Referenced by `casual-doc-wasm` | **0 times** | 14 times |

So the OT substrate — revisions, commits, position mapping — lives in a crate **the live
editing path never calls**, and the live editing path has the invertible op set that OT
actually needs. The editor applies `casual_doc_edit::Operation` values straight to the
model and pushes the inverse onto a flat `Vec<HistoryEntry>` capped at `MAX_HISTORY_ENTRIES`.
There is no transaction envelope, no document revision chain, and no position map on that
path. (`casual-doc-wasm`'s `RevisionIdAllocator` is unrelated — it allocates *tracked-change*
revision ids for `w:ins`/`w:del`, not document revisions.)

Two further consequences worth naming:

- **ADR-005 is currently not honoured in practice.** "Public mutation must go through
  commands and transactions" is the rule; the shipped path is commands applied directly.
  Unifying the two op sets is therefore not new scope — it is closing a known architectural
  debt that OT happens to force.
- **`99` §6's process-debt theme repeats.** A capability recorded as built (the transaction
  engine) is not reachable from the product. That is the same class as "modeled but never
  consumed" (`105` FID-P-04), one layer up.

### 2.1 Prerequisite work: unify on one op set

**Decision:** `casual-doc-edit`'s 47-operation set is the survivor. It is the one the editor
uses, the one with inverses, and the one `59` specifies. `casual-doc-transaction` keeps the
envelope — `RevisionId`, `TransactionId`, `Commit`, `base_revision`, `PositionMap` — and its
5-operation enum is deleted, its mapping-step vocabulary generalised (§3.2).

| Step | Work | Gate | State |
| --- | --- | --- | --- |
| P-1 | Move the operation enum out of `casual-doc-transaction`; make it depend on `casual-doc-edit` (or extract both from a shared `casual-doc-ops`) | One `Operation` type in the workspace; no duplicate vocabulary | **Done for the live path** (`147`). The envelope is re-founded on `v1::Document` + `casual_doc_edit::Operation`. The v0 enum survives as `casual_doc_transaction::v0`, reachable only from the Phase-0 SDK facade, because deleting it means rewriting that facade's public API against v1 (`126`). |
| P-2 | Route every `casual-doc-wasm` mutation through `Transaction` + `Commit`, so every edit allocates a `RevisionId` and emits a `PositionMap` | `casual_doc_transaction` reference count in the WASM facade > 0; ADR-005 provably honoured | **Done** (`147` §3.2, §5). Held by an exhaustive source guard, not by convention. |
| P-3 | Replace the flat `Vec<HistoryEntry>` undo stack with the commit log (§5.1); undo becomes "apply the inverse as a new commit" | Undo/redo behaviour unchanged under the existing browser suite; history survives a reload | **Done, except durability** (`147` §3.4). The log is in memory; surviving a reload is 6.1. |
| P-4 | Emit `MappingStep`s for the structural operations that currently produce none | Every operation either emits mapping steps or is classified Tier 3 (§3.1) with that recorded in its doc comment | **Open.** `147` emits steps for the four positional shapes the v0 map already covered and no more; §3.2's node-addressed steps and the per-operation tier classification are both still to write. |

P-1 through P-3 are worth doing **even if OT were cancelled**: they close ADR-005, they give
undo a durable basis, and they are the prerequisite for version history (`105` OO-004) and
crash recovery (`HF-011`) regardless of collaboration.

---

## 3. Making OT tractable over 47 operations

Naive OT needs a transform for every ordered pair of concurrent operations — 47 × 47 is not
a design, it is a wish. The tractability argument is that **most of the op set does not need
positional transform at all**, because most operations address *nodes*, not text offsets.
Invariant I3 already guarantees stable `NodeId`/`ModelPos` anchors; that is what makes this
work.

### 3.1 Three tiers

| Tier | Operations | What concurrency needs | Cost |
| --- | --- | --- | --- |
| **T1 — positional** | `InsertText`, `DeleteText`, `SplitParagraph`, `JoinParagraphs`, `FormatText`, `ClearFormatting`, `SetHyperlink`, `InsertInlineObject`, `RemoveInlineObject` | **Full pairwise transform.** Two users typing in one paragraph is the hot path and the only place offsets genuinely collide | The real OT work; ~9 ops, and `PositionMap` already covers 4 of the step kinds |
| **T2 — node-addressed** | Table row/column insert/delete, `InsertBlocks`/`DeleteBlocks`, object geometry/anchor/crop/extent/fill/stroke, `SetTableCellProperties`, `SetParagraphProperties`, bookmarks, notes, fields, running content | **Anchor validity, not transform.** The operation names a `NodeId`; concurrency either leaves that node alive (apply unchanged) or removes it (the op is *tombstoned* — dropped with a reported outcome) | Small: one liveness check plus a tombstone rule per carrier kind |
| **T3 — document-scope** | `SetStyleDefinition`, `SetSectionGeometry`, `SetCoreProperties`, `SetEvenAndOddHeaders`, `SetSectionTitlePage`, numbering-definition writes | **Serialise.** Last-writer-wins on a whole-document field, ordered by the relay; optionally an advisory lock for editor UX | Cheapest. These are rare, deliberate, and a lost race is visible and re-doable |

T1 is where correctness is hard and where nearly all traffic is. T2 is where nearly all
*operations* are, and it needs no offset math. That asymmetry is the design.

> **Corrected 2026-09-30 by `150` (ADR-045), which builds this section.** The asymmetry holds
> and was the right bet. Three details were wrong:
>
> 1. **T1 is 11 operations over 55, not 9 over 47.** `InsertField`, `InsertNote` and
>    `CreateBookmark` belong to it as well.
> 2. **`InsertInlineObject`, `InsertNote` and `CreateBookmark` insert ZERO bytes.** A drawing,
>    a note reference and a bookmark marker are all zero-width in a paragraph's projected
>    text, by the edit crate's own length rule. They are T1 for anchoring and inert for
>    offsets, which makes T1's *offset* work smaller still.
> 3. **The classification lives in one exhaustive `match` in `transform.rs`, not in 55 doc
>    comments** (§9 exit gate 2). A doc comment in another crate is not checkable; an
>    exhaustive match makes a 56th variant a compile error.

**Tombstoning is a document-safety decision, not a convenience.** When a T2 operation's
anchor has been deleted concurrently, the operation is dropped and the loss is **reported
through the disposition taxonomy** (`35`), not silently discarded. This reuses the reporting
substrate `105` FID-R-01…FID-R-04 build, and is why Phase 2 precedes Phase 6 in `106`.

### 3.2 Generalising `MappingStep`

Today: `Insert`, `Delete`, `Split`, `Join` — all paragraph-text steps. Required additions,
so T2 anchors can be rebased and tombstones detected:

- `NodeInserted { parent, index, node }` / `NodeRemoved { parent, index, node }` — block and
  table-structure changes, so sibling indices and liveness are derivable.
- `NodeReplaced { old, new }` — `ReplaceTable`, `SetInlines`, and any op that rebuilds a
  subtree under a new identity.
- `PropertiesChanged { node }` — no positional effect; carried so a concurrent T3 or
  formatting op can detect the write-write race.

`PositionMap::map` extends to these by leaving `grapheme_offset` untouched and resolving node
identity instead. Selection mapping (`26`) gets the same benefit for free: a remote edit that
deletes the node your caret sits in currently has no defined behaviour.

### 3.3 Convergence

- **Tie-break:** concurrent T1 inserts at the same position order by the **total order the
  relay settled on** — `Side::Later` moves off the contested boundary, `Side::Earlier` holds
  it. No `site_id` is needed: the settled order is already the shared fact, which is the whole
  reason a server-ordered design is cheaper than a peer-to-peer one.

  > **Corrected 2026-09-30 by `150` §5.1.** This paragraph said "`Affinity` already decides
  > caret behaviour at that boundary; the same rule must decide *content* order". They are
  > **two different questions**: `Affinity` decides where your *caret* lands, `Side` decides
  > where *content* lands. They agree in the case that matters — your own insertion uses
  > `Affinity::After`, so your caret ends up after your own text — but conflating them is what
  > `150` §5.4 had to unpick. Worse, the operation carries no affinity at all
  > (`casual_doc_edit::Pos` has only a node and an offset), which is refusal U5 and finding
  > `150` §9.2.
- **TP1 must be proven**, not asserted: for concurrent `a`, `b`,
  `apply(apply(s, a), transform(b, a)) == apply(apply(s, b), transform(a, b))`. This is a
  property test over generated concurrent operation pairs on generated documents, plus the
  existing fuzz harness. It is a required exit gate, not a nice-to-have.
- **TP2 is avoided by construction.** Transforms are applied against a **totally ordered**
  log supplied by a relay, so no operation is ever transformed against two different
  histories. This is the deliberate trade that keeps the transform set small.
- **The relay is not a document server, and not mandatory.** It orders and fans out
  operations. It does not parse documents, convert formats, hold the authoritative model, or
  gate single-user editing — which is precisely the A1 line (`106` §3) that must not be
  crossed. Offline single-user operation continues with a local log and reconciles on
  reconnect. This is invariant I1 (one choke point) plus I4 (sidecar) doing their job.

---

## 4. "Editing stays light" as a budget

The owner constraint, made measurable. These are exit gates for Phase 6, not guidance.

| Budget | Rule | Why |
| --- | --- | --- |
| **B1** | Per-keystroke work is **O(1) in document size**. No whole-document validation, re-layout, or re-projection per keystroke | Directly violated today by `HF-111` (every suggested keystroke re-validates the whole document) — a **prerequisite fix**, not a follow-up |
| **B2** | Transform cost per incoming remote operation is **O(concurrent ops since its base revision)**, never O(log length) and never O(document) | Bounded by the relay's ordering plus snapshot compaction (§5.2) |
| **B3** | Typing **coalesces** into one transaction per run, split on caret discontinuity, ~500 ms idle, or a structural op | Partly built: `typing_history` already requires exact caret continuity to coalesce. One commit per character would make the log, undo, and the network all quadratic in felt cost |
| **B4** | No operation on the typing path rewrites a paragraph. `SetInlines` is a paragraph-rewrite vehicle and must stay off that path — it is an undo/inverse mechanism, not an edit primitive | A rewrite op defeats both OT granularity and B1 |
| **B5** | Snapshots are **periodic, never per-operation**; the steady-state write is one appended operation | Snapshot cost is amortised, not per-keystroke |
| **B6** | Layout invalidation stays incremental. `incremental.rs` and `dirty_pages` already exist; a remote operation must use them, not force a full repaginate | A remote keystroke must cost what a local one costs |
| **B7** | The log is **bounded**: compaction (§5.2) caps replay work and memory, and the bound is explicit like the `HARD_MAX_*` package limits | `21-PARSER-LIMITS.md` precedent; unbounded growth is a resource-exhaustion bug |

Benchmarks to add to the existing harness (`29`), since none of the four committed baseline
cases covers layout, render, or repaint (`105` EV-002): local keystroke latency, remote-op
apply latency at several concurrency depths, transform cost vs concurrent-op count, snapshot
write cost, and cold replay from snapshot + N operations.

---

## 5. Snapshot, replay, and versioning

Docs 139–140 refine this section into the product, restore, fidelity-checkpoint, storage,
and version-diff contracts. They preserve snapshot-plus-replay but correct one important
ambiguity below: a normalized snapshot is suitable for semantic replay/diff projection,
not as the sole user-restorable artifact.

The versioning half of the decision, and the part that pays for itself immediately.

### 5.1 The log is the primitive

```
Snapshot(r0) ─ op(r1) ─ op(r2) ─ … ─ op(rN)        →  document at rN
```

- **Checkpoint:** a fidelity-complete source-format artifact plus a validated semantic
  projection, as specified by doc 140. The deterministic normalized snapshot (`25`) is
  strict, bounded, and useful for replay/diff; canonical CBOR (`08`, designed and
  unimplemented) may become its compact form. It cannot be the only restore source: doc
  112 measured that normalized JSON drops binary resources and the retained source
  envelope.
- **Operation:** one committed `Transaction` — its operations, `base_revision`, allocated
  `RevisionId`, author identity (`82` already models authors), and timestamp.
- **Replay:** load a snapshot, apply operations in order. Deterministic because the engine is
  deterministic — the same property the render pipeline already relies on.

Undo/redo, version history, and crash recovery are then **one mechanism**, which is why P-3
is worth doing independently of collaboration.

### 5.2 Compaction

Periodically write a fresh snapshot and drop the operations before it, keeping a bounded tail
(B7). Named versions pin their snapshot so a restore point is never compacted away. Policy
(interval, tail length, retention) is **host policy**, per AGENTS.md — the runtime enforces
bounds and the host chooses values.

### 5.3 Versions and revisions

Mirror the distinction the product must compete with (`105` §4.4 OO-004): a **revision** is
every committed transaction; a **version** is a named, pinned point a user can name, restore,
and compare. Both derive from the log; neither is a separate subsystem.

| Feature | How it falls out | Row closed |
| --- | --- | --- |
| Version history list | Log walk grouped by session and author | OO-004 |
| Restore a version | Replay to that revision; record the restore as a new commit (never rewrite history) | OO-004 |
| Per-contributor change colouring | Author identity is already on each commit (`82`) | OO-004 |
| Crash recovery / autosave | Persist the log tail; on reload, replay | **HF-011**, HF-068 |
| **Compare documents** | Replay both branches; diff the models; emit the result as tracked changes into the existing revision model | OO-007 |
| **Combine documents** | Replay both; transform one branch's operations onto the other — *this is the OT transform already built*, applied offline | OO-007 |

Compare and combine being consequences of the transform rather than separate features is the
strongest practical argument for the owner's choice over the ordered-log model.

---

## 6. What this does not change

- **Apache-2.0, local-first, no mandatory server.** The relay is additive and optional
  (invariants I1/I4). Single-user editing with the network off is unchanged — the
  ONLYOFFICE advantage A1 is preserved.
- **ADR-006 stays true**: core depends on no specific collaboration vendor. OT lives behind
  the adapter seam; a CRDT adapter remains possible later for peer-to-peer or true
  partition-tolerant merge, which the relay-ordered design deliberately does not attempt.
- **The closed op set (I2) is now load-bearing twice over** — for undo and for transform.
  Adding an operation without transform rules and a tier classification becomes a CI failure,
  not a review comment.

---

## 7. Phasing

Sequenced so each step is independently valuable and none is a big-bang merge.

| Step | Work | Independently worth it? |
| --- | --- | --- |
| **6.0** | §2.1 P-1…P-4: unify the op set, route mutation through transactions, commit-log undo, mapping steps for structural ops. Plus the B1 prerequisite fix (`HF-111`) | **Yes** — closes ADR-005 and the `HF-111` perf defect. **P-1…P-3 landed 2026-09-29** (`147`, ADR-043); P-4 and the v0-stack retirement remain |
| **6.1** | Durable log + snapshot + compaction; crash recovery and autosave | **Yes** — closes `HF-011`, the oldest P1 data-safety row |
| **6.2** | Version history: list, restore, per-author colouring | **Yes** — closes OO-004 |
| **6.3** | T1 transform + tie-break + TP1 property tests + the §4 benchmarks. **No network yet** | Yes — offline compare/combine becomes possible. **Transform and TP1 landed 2026-09-30** (`150`, ADR-045); the §4 benchmarks remain |
| **6.4** | T2 anchor rebase and tombstoning with taxonomy reporting; T3 serialisation | Yes — completes the transform set. **The rebase and the `Tombstoned` outcome landed with 6.3**; what remains is the *reporting* — routing a tombstone into the disposition taxonomy (`35`), which has no caller until 6.6 |
| **6.5** | Compare and combine documents | **Yes** — closes OO-007 |
| **6.6** | Relay adapter, presence, per-user cursors, author identity on the wire | Collaboration ships. **The foundation landed 2026-09-30** (`152`, ADR-047): the wire vocabulary, the two session state machines, the identity discipline and the rollback/replay rebase driver, all in `casual-doc-transaction` and all with no transport. What remains is the byte codec, the relay binary, presence, the host-signed grant, and durability — plus the one prerequisite `152` §4.4 names, which is a live editor that mints in a session-supplied identity space instead of a document-derived one |
| **6.7** | Roles and permission enforcement, against the Phase 4 permissions object | Closes OO-018 |

Note 6.0–6.5 deliver four tracker rows and **no** networking. If collaboration were cancelled
tomorrow, everything through 6.5 would still be the right work.

**Revised effort:** `106` Phase 6 estimated 5–7 months for the ordered-log model. OT plus the
6.0 unification is **7–10 months**, of which 6.0–6.2 (~3 months) is debt repayment that was
owed anyway.

---

## 8. Open questions — design is not finished

Recorded rather than hidden, per AGENTS.md.

1. ~~**Formatting-vs-text transform.**~~ **Answered by `150` §5.5.** The format applies to the
   **survivor**: the range is rebased through the delete, and when nothing survives the result
   is `Satisfied` rather than a tombstone, because the text the user meant to format was
   removed by the other operation and nothing was lost by this one. The converse needs no
   rule — formatting changes no byte offsets. `150` §5.8 adds the case this question did not
   reach: two concurrent *formatting* writes over overlapping text resolve **per field over
   the overlap**, not per operation.
2. **Tombstone visibility.** When a T2 operation is dropped because its anchor died, does the
   author see a notice, a silent no-op, or an undo-able marker? A silent drop violates the
   no-silent-loss rule; a modal per race is unusable. Proposal: a non-blocking toast plus a
   taxonomy entry — which needs `105` UX-017's toast channel first.
3. **Interaction with tracked changes.** Suggesting mode already wraps edits in revision
   markup. Are collaborative operations transformed *before* or *after* revision wrapping?
   Wrong order corrupts authorship. `83`/`86` must be reconciled with this.
4. ~~**Table-geometry races** … "the most likely place TP1 fails".~~ **Half right, corrected by
   `150` §5.9.** The *index arithmetic* converges: `InsertColumn`/`DeleteColumn` are refused on
   irregular tables by `apply` itself, so the grid is regular wherever they apply, and
   regular-grid arithmetic converges under the tie-break. What does not converge is the
   **carried payload** — `InsertRow` carries one cell per column and `InsertColumn` one cell
   per row, so a concurrent change to the other axis leaves it the wrong shape. A concurrent
   removal drops the matching cell exactly; a concurrent insertion would need a cell with fresh
   identities, which a pure transform may not mint, and is refused (U9). No fourth tier and no
   advisory lock. The place TP1 actually failed most often while this was being built was the
   paragraph split dividing a range, and the run a concurrent insertion attaches to.
5. **Log format stability.** The operation log becomes persisted, versioned data — so the
   operation enum becomes a compatibility surface. Needs a schema-version policy like
   `22-NORMALIZED-SCHEMA-V0.md`'s, and a decision on whether old logs must replay on new
   builds.
6. ~~**`site_id` allocation** without a mandatory server, and collision behaviour.~~
   **Answered 2026-09-30 by `152` §4 (ADR-047), and by a property rather than by a
   probability.** There is no separate `site_id`: the participant number the relay assigns *is*
   it, and `wire::IdSpace::of(document_space, client)` derives a 64-bit minting namespace from
   it that is **injective in the participant number**, so two replicas cannot mint the same
   `NodeId`. Collision behaviour is therefore not a probability to bound — it is refused at
   the receiver with `ODC-7008` whenever the discipline was not followed, which is what today's
   live editor does: it derives its namespace from the *document*, so two replicas mint
   identical ids for different nodes from the first edit. `152` §4.4 owns the fix.
7. **Relay protocol and transport.** The *protocol* is now specified — `152` / ADR-047,
   `casual-doc-transaction::{protocol, session, wire}`. The **transport and the byte codec are
   still open**, deliberately: `casual-doc-edit` has no `serde` and the op-set lane is about to
   move the operation shapes, so freezing an encoding now would freeze a compatibility surface
   over shapes that are about to change. ONLYOFFICE uses socket.io; that remains an
   implementation detail, not a constraint on us.

---

## 9. Exit gates

Phase 6 is not `Done` until all hold:

1. One operation vocabulary in the workspace; every WASM mutation goes through a
   `Transaction` and allocates a `RevisionId` (ADR-005 provably honoured).
2. Every operation is tier-classified, and the build fails on an operation added without a
   classification and transform rules. **Met** (`150` §3.2) — but *not* as worded: the
   classification is an exhaustive `match` in `transform.rs`, not a doc comment per operation,
   because a doc comment in another crate is not checkable and an exhaustive match is a
   compile error.
3. TP1 property tests pass over generated concurrent pairs; the fuzz harness covers the
   transform. **Half met:** TP1 holds over 3,153 generated pairs (`150` §8) and every guard
   is proven red. The fuzz harness does not yet reach `transform`.
4. Budgets B1–B7 are benchmarked, with baselines committed — including the layout/render/
   repaint cases the current baseline lacks.
5. A dropped (tombstoned) operation always produces a disposition entry; no silent loss.
6. Crash recovery, version restore, and compare/combine each have a regression test **proven
   red** against the reintroduced bug.
7. Single-user editing works with the network disabled, verified by a test that asserts it —
   A1 is protected by a gate, not by intent.
