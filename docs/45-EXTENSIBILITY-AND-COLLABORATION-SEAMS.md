# 45 — Extensibility & Collaboration Seams

## Purpose

Record the architectural **seams** that must be preserved so that three future
layers can be added **as adapters, without a core rewrite**:

1. **Collaboration** — Operational Transformation (OT) or CRDT concurrent editing.
2. **Agentic / AI** — MCP tool servers, agent-driven edits.
3. **RAG / vector** — retrieval, embeddings, semantic search over document content.

None of these are current-focus work (they are Phase 2+). This document exists so
the Phase-1F model/rendering work landing **now** actively *reinforces* the seams
instead of eroding them, and so the requirement is not forgotten. The concrete
decision is recorded as **ADR-030**; the four invariants below are a **review
checklist item** for every PR that touches the model or mutation paths.

## The key insight

A remote collaborator, an OT/CRDT peer, and an AI agent all do the **same two
things**: *observe* the document and *apply operations* to it. So one clean
operation channel + one observation stream + stable anchors future-proofs **all
three at once**. This is one seam to protect, not three.

## The four invariants (the whole ballgame)

### I1 — Single mutation choke point
Every change to document state — local keystroke, remote op, agent action,
programmatic edit — MUST go through one `apply(operation)` path. No API mutates
model state by any other route. If this holds, a remote/agent op is literally the
same code path as a keystroke, and collaboration/agentic edits require **zero** new
mutation plumbing.
*Guard:* a test/review rule that no public API mutates the document except through
the transaction/operation channel.

### I2 — Closed, serializable, composable + invertible operation set
Operations are an explicit, closed enum; each is serializable, **composable**, and
**invertible**. This already exists in substance (Phase 0 / doc 24:
insert/delete/split/join + position **mapping** + **inverse** + history).
*Consequence:* adding **OT** = add one `transform(op_a, op_b)` (the concurrent
rebase) + a sync adapter — nothing else in the engine moves. Adding a new
construct-family op keeps the set closed (extend the enum), so `transform`/merge
grows predictably instead of being retrofitted.

### I3 — Stable identity for anchors; position math stays behind `ModelPos`
Anchors key on **`NodeId` (u128)**, not array indices — already the case, and the
single most important OT/CRDT-friendliness decision. All offset arithmetic stays
behind the `ModelPos` (node + UTF-8 byte offset) abstraction rather than being
scattered across edit sites.
*Consequence:* **CRDT** (which needs position *identity*, not offsets) is added by
swapping the per-node sequence representation **in one place**, not across a hundred
call sites. This is the one real retrofit risk (see below) and I3 neutralizes it.

### I4 — Derived / AI data lives in a sidecar, never in the OOXML model
RAG embeddings, summaries, agent annotations, and any AI-derived data attach to a
**separate store keyed by `NodeId`** — never inside the `v1` document model.
*Consequence:* derived data never affects DOCX round-trip fidelity, can be rebuilt
at any time, and RAG/agentic layers add **no** risk to the "no silent data loss"
guarantee.

## How each future layer lands (given I1–I4)

- **OT:** add `transform(op_a, op_b)` + convergence tests (TP1/TP2) + a
  server/sequencer sync adapter. Touches the operation module only.
- **CRDT:** swap the per-node text sequence for a sequence-CRDT (RGA/Yjs/Automerge
  style) under the existing ops; block structure stays keyed by `NodeId`. Localized
  by I3. A **hybrid** (block-level structure by `NodeId` + intra-node sequence CRDT)
  fits the current model most naturally.
- **MCP:** a thin protocol adapter at the **SDK boundary** exposing
  `read snapshot / subscribe events / apply ops` as tools. No core change — MCP is a
  transport over the same three primitives collaboration uses.
- **RAG / vector:** a traversal that yields `(text, NodeId anchor)` chunks + the I4
  sidecar for embeddings/metadata. Retrieval maps a hit back to a `NodeId` range
  (which the layout/hit-test layer can already resolve to on-screen geometry).

Layout, rendering, import, and export are **read-only consumers** of the model
(LayoutNG discipline), so none of these layers touch them. The bounded incremental
re-pagination already shipped is exactly what makes a stream of remote/agent ops
cheap to reflow (op → dirty range → re-paginate the neighborhood).

## The one real retrofit risk

Integer-offset assumptions leaking out of `ModelPos` into many edit sites would make
a future CRDT position swap expensive. Mitigation is nearly free and is invariant
**I3**: keep offset arithmetic behind the anchor abstraction. Cost to hold now ≈ a
review habit; cost to fix later if violated ≈ a large mechanical refactor.

## OT vs CRDT — deferred choice (future ADR)

| | OT | CRDT |
|---|---|---|
| Model change | small — keep offset positions | larger — position identity under I3 |
| Topology | central server / sequencer | decentralized, offline / local-first / p2p |
| Hard part | `transform` for every op-pair + convergence | position-ID bookkeeping + memory |
| Fits | client-server (Google-Docs style) | local-first, offline, peer sync |

The choice is a Phase-2 ADR (still listed under "Pending ADRs" in doc 08 as
"collaboration operation model"). **This document does not choose** — it guarantees
that either choice remains an additive adapter, not a rewrite.

## Review checklist (apply to every model/mutation PR from now)

- [ ] No new public API mutates document state except through the operation channel (I1).
- [ ] Any new operation is added to the closed op enum and is composable + invertible (I2).
- [ ] New anchors/positions use `NodeId` + `ModelPos`; no raw offset arithmetic outside the anchor abstraction (I3).
- [ ] No AI/derived data is added to the `v1` model; it belongs in the sidecar store (I4).
