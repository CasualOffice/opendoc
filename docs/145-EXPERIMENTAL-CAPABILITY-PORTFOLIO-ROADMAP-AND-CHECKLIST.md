# 145 — Experimental Capability Portfolio Roadmap and Checklist

**Status:** Proposed portfolio plan; documentation only.

**Opened:** 2026-09-28.

This document is the top-level delivery map for four related but separate
capability lanes:

1. PDF semantic reconstruction and selective OCR;
2. document assistance, semantic/vector retrieval, and optional MCP;
3. version history, restore, attribution, and structural diff;
4. embedded real-time co-editing.

It does not merge their runtime boundaries, dependencies, or support claims. It
provides one place to see phase order, shared prerequisites, current evidence,
and checkboxes. The detailed documents remain authoritative for each lane.

---

## 1. Documentation set and authority

| Lane | Requirements and architecture | Decision | Execution source |
| --- | --- | --- | --- |
| PDF/OCR | Doc 131 | Proposed ADR-034 | Doc 131 PDFR-000–006; checklist below |
| Assistance/search/MCP | Doc 132 | Proposed ADR-035 | Doc 132 DAI-0–6; checklist below |
| History/restore/diff | Docs 139–140 | Accepted ADRs 038/040 for local H1/H2; proposed ADR-033 for later shared commits | Docs 139–140 VH/H0–H5; checklist below |
| Co-editing | Docs 107 and 143 | Proposed ADR-033 | Doc 144 C0–C8 |
| Embed/host foundation | Docs 125–126 | Accepted ADRs 036/039 | Existing package and host-contract evidence |
| Support claims | Doc 18 | Accepted matrix | Must change only after the corresponding gates pass |
| CI/release gates | Doc 15 | Accepted policy | Must become blocking as implementation lands |

Read this document first for sequencing, then use the lane document for design
details and acceptance evidence. If a checkbox conflicts with a lane document,
the lane document and accepted ADRs win; fix this index in the same change.

## 2. Boundary decisions

### 2.1 PDF/OCR is import, not assistance

PDF/OCR accepts a fixed-layout file, gathers bounded native and optical evidence,
reconstructs the normalized document model, and then uses the ordinary writer.
It does not call MCP, own an assistant, create embeddings, or accept an OCR
provider's DOCX as truth.

### 2.2 Assistance is a product service; MCP is an adapter

Translation, rewrite, formatting, text/table transformation, review, summary,
and semantic search are user scenarios implemented through a shared Document
Assistance Layer. MCP may expose those same bounded resources and proposal tools
to an external agent. MCP does not define the product model and does not create a
second mutation path.

### 2.3 Embeddings are derived sidecars

Chunks, embeddings, summaries, retrieval metadata, and unaccepted proposals are
rebuildable data keyed by stable document anchors. They do not enter OOXML, ODT,
the normalized model, or the fidelity-preservation envelope. Browser-local
profiles use Workers and never silently fall back to a remote provider.

### 2.4 History is useful without collaboration

Local fidelity-complete H1/H2 history and restore already work without a server.
Structural diff and unified commit attribution are later local capabilities.
Co-editing may consume those foundations, but a collaboration outage must not
disable local history or restore.

### 2.5 Co-editing is local WASM plus an optional provider

The engine, layout, render, import, export, and local history remain browser
local. Remote collaboration requires coordination, so the host supplies an
optional provider/relay. The relay orders bounded operation envelopes; it is not
a document converter, renderer, or mandatory file server.

## 3. Shared architecture and dependencies

```text
Existing local engine + stable NodeId/ModelPos + preservation writers
                              |
             +----------------+----------------+
             |                                 |
             v                                 v
   F1 unified transaction path       F2 complete host I/O + identity
             |                                 |
             +---------------+-----------------+
                             |
             +---------------+-------------------+
             |               |                   |
             v               v                   v
       History H3/H4    Assistance DAI-1     Co-editing C3/C4
             |               |                   |
             |         +-----+-----+             |
             |         v           v             |
             |     DAI-2 index  DAI-3 proposals  |
             |                     |             |
             |                     v             v
             |                  MCP DAI-4/5   relay/presence C5/6
             |                                   |
             +-------------------+---------------+
                                 v
                  shared history/restore/diff C7

PDFR-000–006 is an independent staged-import lane. It shares model, report,
worker, resource-bound, and writer infrastructure, but not the collaboration or
assistance protocol.
```

Shared prerequisites are shared services, not permission to couple the lanes.

| Foundation | Needed by | Current state |
| --- | --- | --- |
| Stable anchors and normalized model | all lanes | Implemented foundation |
| Fidelity-preserving format I/O and reports | PDF, history, assistance proposals | Implemented foundation; per-format breadth varies |
| One revisioned mutation/commit path | assistance mutation, H4/H5, co-editing | Not implemented in the live path |
| Complete host open/save/export contract | external embed, history host policy, co-editing | Not implemented |
| Worker/job/cancellation/resource contract | PDF/OCR, indexing, summarization, diff | Must be unified; no feature-specific unbounded worker API |
| Derived sidecar store | embeddings, summaries, diff, presence | Designed; semantic sidecar not implemented |
| Provider/capability policy | inference, MCP, collaboration | Designed per lane; not implemented |

## 4. Portfolio dashboard

| Lane | Current phase | Honest status | Next decision/gate |
| --- | --- | --- | --- |
| PDF/OCR | PDFR-000 | Experimental design only; PDF import remains disabled | Accept/reject ADR-034, provider/dependency/corpus/limit decisions |
| Assistance/search/MCP | DAI-0 | Experimental design only | Accept/reject ADR-035 and DAI-0 scenario/privacy/topology decisions |
| History/restore/diff | H1/H2 | Local checkpoint timeline and append-only restore implemented and reachable | Finish H2 gaps, then H3 diff and H4 unified commits |
| Co-editing | C0 | Experimental design only | Accept/revise ADR-033, provider boundary, and doc 143 §16 decisions |

No row above authorizes an implementation or a support claim by itself.

## 5. Portfolio implementation order

### P0 — Truth, ownership, and decision closure

- [x] Separate PDF/OCR from assistance/MCP. Evidence: docs 131–132.
- [x] Define local history independently of collaboration. Evidence: docs 139–140.
- [x] Define provider-neutral embedded co-editing. Evidence: docs 143–144.
- [x] Reconcile support, CI, tracker, ADR, package, and architecture documents.
  Evidence: docs 08, 14, 15, 18, 45, 83, 99, 107, 125–145.
- [ ] Accept, revise, or reject proposed ADR-034.
- [ ] Accept, revise, or reject proposed ADR-035.
- [ ] Accept, revise, or reject proposed ADR-033 and the provider boundary.
- [ ] Assign product/security/engine/SDK owners for each accepted lane.
- [ ] Record resource, privacy, compatibility, retention, and support defaults.

**Exit gate:** each lane is explicitly accepted, deferred, or rejected, with no
dependency or package name selected by implication.

### P1 — Shared engine and host foundations

- [ ] Unify the operation vocabulary and live transaction path.
- [ ] Emit revision, inverse, mapping, affected anchors, origin, and stable events.
- [ ] Complete host document open/save/export and artifact lifetime contracts.
- [ ] Define lineage, branch, commit, actor, session, and site identities.
- [ ] Define one bounded async-job contract for progress, cancellation, cleanup,
  memory, and worker failure.
- [ ] Define sidecar lifecycle, schema version, invalidation, rebuild, and quota.
- [ ] Add native/WASM deterministic replay and schema golden-vector gates.

**Exit gate:** accepted lanes can use public engine/host contracts without direct
model mutation, private WASM calls, DOM truth, or an unbounded background job.

### P2 — Independent minimum useful slices

These may proceed in parallel after their own decisions and prerequisites:

- [ ] PDFR-001 deterministic evidence reconstruction without an OCR dependency.
- [ ] DAI-1 bounded read-only structural/keyword assistance without an LLM/MCP.
- [ ] H3 structural diff over existing local checkpoints.
- [ ] C3 deterministic collaboration protocol simulator without a real network.

**Exit gate:** every slice provides standalone user value and has no hidden cloud
or provider dependency.

### P3 — Provider-backed experimental slices

- [ ] PDFR-002/003 native PDF evidence plus selective baseline OCR.
- [ ] DAI-2 browser-local hybrid retrieval with an honest keyword fallback.
- [ ] DAI-3 reviewable selected-scope proposals.
- [ ] C4 OT convergence/offline proof and C5 optional reference relay.
- [ ] H4 persisted unified commits and attribution.

**Exit gate:** security, privacy, cancellation, resource, offline, determinism,
preservation, and failure-recovery gates pass for every enabled profile.

### P4 — External adapters and integrated workflows

- [ ] DAI-4 read-only MCP profile passes conformance and authorization tests.
- [ ] DAI-5 MCP proposal flow cannot commit without current scoped approval.
- [ ] C6 presence UX is accessible and independent of durable content delivery.
- [ ] C7 shared comments, review, history, restore, diff, and per-user undo pass.
- [ ] PDFR-004 difficult layout/table families meet semantic and visual gates.

**Exit gate:** adapters reuse the same public services and capability policy; no
adapter becomes a second model, mutation, index, or storage authority.

### P5 — Production evaluation and graduation

- [ ] PDFR-005/006 optional advanced profile and production conformance.
- [ ] DAI-6 document-wide/collection workflows and measured index strategy.
- [ ] C8 external integrator preview and provider conformance.
- [ ] Cross-browser/device/accessibility/security/load reports are retained.
- [ ] Migration, rollback, backup/recovery, threat model, SBOM, and limitations
  are published for every graduated artifact.
- [ ] Doc 18 contains only claims proven by blocking release gates.

**Exit gate:** an external integrator can reproduce the supported profile using
released artifacts and public documentation alone.

## 6. PDF/OCR lane checklist

Detailed design and gates: doc 131.

### PDFR-000 — Decision and evidence baseline

- [x] Browser-local staged architecture and evidence boundary documented.
- [x] Candidate OCR/parser/runtime landscape recorded without accepting a model.
- [ ] ADR-034 accepted or revised.
- [ ] Rights-safe digital/scanned/hybrid/hostile corpus committed.
- [ ] Parser/OCR/runtime licence and provenance review accepted.
- [ ] Exact size/page/pixel/time/memory/download limits accepted.
- [ ] `can_import` remains false until the gate is satisfied.

### PDFR-001–003 — Reconstruct, parse, then OCR selectively

- [ ] `PdfEvidenceV1` schema, bounds, golden vectors, and fuzzing exist.
- [ ] Deterministic reconstruction works from committed evidence without a parser.
- [ ] Native digital-PDF extraction runs in a bounded cancellable Worker.
- [ ] Native text is preferred; OCR is invoked only for classified regions/pages.
- [ ] CPU-WASM baseline works offline on the declared browser matrix.
- [ ] Unsupported/encrypted/hostile input fails atomically with findings.

### PDFR-004–006 — Breadth and conformance

- [ ] Reading order, columns, repeated furniture, and table topology graduate by
  construct-specific semantic/visual/editability evidence.
- [ ] Formula, handwriting, or VLM providers remain separately capability-gated.
- [ ] Unsupported hardware falls back locally, never silently to cloud OCR.
- [ ] Import → edit → DOCX save → reopen fixed points pass.
- [ ] Cross-browser, accessibility, hostile corpus, cancellation, memory, and
  named-device performance reports pass.
- [ ] PDF semantic import is marked supported only after PDFR-006.

## 7. Assistance, semantic search, vector, and MCP checklist

Detailed scenarios and gates: doc 132.

### DAI-0/1 — Product scenarios and read-only foundation

- [x] User scenarios precede MCP/model/tool selection.
- [x] MCP is defined as an optional adapter, not a product model.
- [x] PDF/OCR is explicitly excluded from this lane.
- [ ] ADR-035 and first experimental scenario set accepted.
- [ ] Versioned structural scope/projection/provenance schemas exist.
- [ ] Exact and keyword search return bounded anchored results.
- [ ] Read-only findings/summary flows disclose scope and unsupported content.

### DAI-2 — Local semantic/vector retrieval

- [ ] Chunks and embeddings live only in a versioned rebuildable sidecar.
- [ ] Worker indexing is cancellable, bounded, incremental, and gap-aware.
- [ ] Hybrid lexical/vector ranking cites live anchors.
- [ ] Partial/stale/indexing/unavailable states are visible.
- [ ] Keyword-only fallback works without model download.
- [ ] Multilingual quality, memory, storage, cold-start, and update budgets pass.

### DAI-3 — Reviewable changes

- [ ] Translation, rewrite, formatting, text/list/table transformation, and review
  use typed recipes over explicit scopes.
- [ ] Provider output is validated into a proposal, never applied as raw OOXML.
- [ ] Preview and granular accept/reject preserve source formatting and objects.
- [ ] Stale/moved/deleted anchors refuse or rebase under explicit rules.
- [ ] One approval produces one ordinary undoable transaction.
- [ ] Prompt-injection tests prove document content cannot widen capabilities.

### DAI-4/5/6 — MCP and broader workflows

- [ ] Read-only MCP resources/tools pass protocol, bound, authorization, audit,
  cancellation, and confused-deputy tests.
- [ ] Mutable tools return proposals; they cannot invoke an unrestricted edit API.
- [ ] Approval is revision/scope/actor bound, expiring, and non-replayable.
- [ ] Any browser companion requires explicit pairing, origin/session binding,
  least privilege, revocation, and a separate accepted design.
- [ ] Collection indexing is host-authorized and never ambient.
- [ ] Summaries/retrieval publish measured coverage and citation quality, not a
  generic accuracy claim.

## 8. History, edit management, restore, and diff checklist

Detailed requirements and architecture: docs 139–140.

### H1/H2 — Current local baseline

- [x] Fidelity-complete content-addressed source-format checkpoints exist.
- [x] Metadata timeline, grouping, naming, pinning, deletion, and retention exist.
- [x] Historical preview is isolated and read-only.
- [x] Restore validates first, preserves current state, and appends a new head.
- [x] Store/restore failure injection and idempotency tests exist.
- [ ] Copy/download historical artifact is public and capability-gated.
- [ ] Restore is one normal reversible commit with one-step session Undo.
- [ ] Full per-format checkpoint fidelity corpus passes.

### H3 — Structural diff

- [ ] Stable-ID and independently imported alignment paths exist.
- [ ] Text, formatting, paragraph, list, table, section, object, comment, and
  review families return typed changes or explicit `not_compared` findings.
- [ ] Diff runs as cancellable read-only derived work.
- [ ] Navigation anchors and accessible list/overlay journeys pass.
- [ ] Compare-to-tracked-changes is a separate explicit transaction.

### H4/H5 — Durable commits and collaboration

- [ ] Unified versioned commit envelopes replace snapshot-only attribution.
- [ ] Actor/origin attribution discloses incompleteness after compaction.
- [ ] Per-user undo applies an inverse commit without removing remote work.
- [ ] Shared restore is append-only, atomic, and visible to all clients.
- [ ] Offline branches/rebase preserve unmergeable work as a recovery artifact.
- [ ] Ordinary DOCX/ODT files contain no hidden OpenDoc history.

## 9. Co-editing checklist

Doc 144 is the complete tickable C0–C8 plan and is not duplicated here.
Portfolio-level gates are:

- [x] Research, provider boundary, architecture, phases, and evidence rules exist.
- [ ] C0 decisions and threat model are accepted.
- [ ] C1 one revisioned mutation/commit substrate passes.
- [ ] C2 complete host I/O and durable identity pass.
- [ ] C3 provider-neutral protocol and deterministic simulator pass.
- [ ] C4 OT, offline reconciliation, review, table, and performance gates pass.
- [ ] C5 optional relay and independent-provider conformance pass.
- [ ] C6 presence and recovery UX pass accessibility/device/scale gates.
- [ ] C7 shared review/history/restore/diff integration passes.
- [ ] C8 external integrator preview and production hardening pass.

## 10. Cross-lane architecture-change register

| Area | Required change | Forbidden shortcut |
| --- | --- | --- |
| Mutation | One revisioned transaction service for UI, host, restore, remote, and approved assistance changes | Direct model/WASM mutation per feature |
| Async work | Bounded jobs with progress, cancellation, teardown, and resource accounting | Unbounded work on the main thread |
| Identity | Stable node/range plus lineage/commit/actor/session identities | DOM nodes, array indexes, filename, or display name as identity |
| Derived data | Versioned rebuildable sidecars | Embeddings/diff/presence hidden in OOXML |
| Providers | Host-selected capability ports | Mandatory cloud/vendor dependency in core |
| Network | Host policy and explicit profile | Silent remote fallback or document-derived fetch |
| Storage | Host-owned ports with browser reference stores where useful | Provider as the only recoverable copy |
| Errors | Stable bounded codes/findings/dispositions | Localized prose or silent drop as API behavior |
| Security | Least privilege at provider, contract, engine, and chrome | UI-only permission enforcement |
| Compatibility | Independently version host, operation, protocol, evidence, proposal, checkpoint, and sidecar schemas | One package version assumed to make persisted data compatible |

## 11. Pull-request checklist for any lane

- [ ] Tracker slice names one lane, phase, owner, dependency, and exit gate.
- [ ] Accepted ADR or explicitly experimental spike authorizes the work.
- [ ] Public behavior, support impact, and non-goals are stated.
- [ ] Resource bounds, cancellation, security/privacy, and offline behavior are stated.
- [ ] Mutation uses the shared transaction path or is read-only/derived.
- [ ] Unsupported input is preserved, refused, or reported explicitly.
- [ ] Native/WASM and public/private boundary effects are tested.
- [ ] Relevant fuzz/property/corpus/browser/accessibility/performance gates are added.
- [ ] Docs 08, 14, 15, 18 and the lane document are updated together.
- [ ] Generated documentation is rebuilt and checked.
- [ ] No checkbox is marked complete without a committed evidence reference.

## 12. Portfolio definition of done

The portfolio is not one release train. Each lane graduates independently. A lane
is Done only when:

- [ ] its decision is accepted and its phase exit gates pass;
- [ ] an external user scenario works through public artifacts only;
- [ ] security, privacy, determinism, preservation, accessibility, performance,
  compatibility, recovery, and operations evidence matches the support claim;
- [ ] fallback and unsupported behavior are visible and safe;
- [ ] local-only document editing remains functional with network access blocked;
- [ ] doc 18, examples, package metadata, and release notes state no more than the
  retained evidence proves.

Until then, the accurate summary is:

- PDF is export-only; semantic PDF/OCR import is experimental design;
- assistance, semantic/vector retrieval, and MCP are experimental design;
- local history/preview/restore H1/H2 are partly implemented; diff/shared history are not;
- real-time co-editing is experimental design and is not implemented or supported.
