# 45 — Extensibility & Collaboration Seams

## Purpose

Record the architectural **seams** that must be preserved so that three future
layers can be added **as adapters, without a core rewrite**:

1. **Collaboration** — relay-ordered Operational Transformation (OT) is the
   proposed first algorithm; a CRDT may remain a later adapter.
2. **Agentic / AI** — MCP tool servers, agent-driven edits.
3. **RAG / vector** — retrieval, embeddings, semantic search over document content.

Implementation of these layers is not current-focus work. Doc 132 now applies these
seams to an experimental, user-scenario-first Document Assistance Layer, local
semantic retrieval, and an optional MCP adapter; it does not change their unshipped
status. This document exists so current model/rendering work actively *reinforces*
the seams instead of eroding them. The concrete decision is recorded as **ADR-030**;
the four invariants below are a **review checklist item** for every PR that touches
the model or mutation paths.

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
- **Document assistance:** a shared service above commands/transactions resolves
  scope, gathers bounded context, invokes a host-approved provider, and emits a
  read-only result or reviewable change proposal. Accepting a proposal commits through
  the same mutation choke point as a local edit.
- **MCP:** an optional thin protocol adapter at the **SDK/assistance boundary**. It
  exposes bounded resources and proposal-oriented tools; it does not expose an
  unrestricted raw mutation primitive or create a second command path. Embedded web
  assistants call the same application service directly and do not require MCP.
- **RAG / vector:** structure-aware chunks retain `NodeId` plus range anchors in the I4
  sidecar. Incremental hybrid retrieval maps hits back to live document ranges and
  must rebuild explicitly after an observation gap. It never changes serialized
  document state.

The detailed experimental architecture, user-scenario catalogue, browser execution
profiles, MCP topologies, and graduation gates are in doc 132 and proposed ADR-035.

Layout, rendering, import, and export are **read-only consumers** of the model
(LayoutNG discipline), so none of these layers touch them. The bounded incremental
re-pagination already shipped is exactly what makes a stream of remote/agent ops
cheap to reflow (op → dirty range → re-paginate the neighborhood).

## The one real retrofit risk

Integer-offset assumptions leaking out of `ModelPos` into many edit sites would make
a future CRDT position swap expensive. Mitigation is nearly free and is invariant
**I3**: keep offset arithmetic behind the anchor abstraction. Cost to hold now ≈ a
review habit; cost to fix later if violated ≈ a large mechanical refactor.

## OT vs CRDT — owner direction recorded, ADR still proposed

| | OT | CRDT |
|---|---|---|
| Model change | small — keep offset positions | larger — position identity under I3 |
| Topology | central server / sequencer | decentralized, offline / local-first / p2p |
| Hard part | `transform` for every op-pair + convergence | position-ID bookkeeping + memory |
| Fits | client-server (Google-Docs style) | local-first, offline, peer sync |

The owner selected OT over revisioned transactions on 2026-09-15; doc 107 and
proposed ADR-033 define it. This is not yet an accepted or implemented support
claim. The provider boundary remains neutral so a future CRDT can be evaluated
without forking the editor. Docs 143–144 contain the current integration
architecture and phased checklist.

## Review checklist (apply to every model/mutation PR from now)

- [ ] No new public API mutates document state except through the operation channel (I1).
- [ ] Any new operation is added to the closed op enum and is composable + invertible (I2).
- [ ] New anchors/positions use `NodeId` + `ModelPos`; no raw offset arithmetic outside the anchor abstraction (I3).
- [ ] No AI/derived data is added to the `v1` model; it belongs in the sidecar store (I4).
