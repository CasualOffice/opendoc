# 144 — Co-Editing Phased Implementation Plan and Checklist

**Status:** Proposed execution plan; documentation only. No co-editing support is
implemented or implied by this checklist.

**Opened:** 2026-09-28.

**Architecture:** doc 143. **Algorithm:** proposed ADR-033 and doc 107.
**Embedding:** ADR-036, docs 125–126. **History:** docs 139–140.

This document converts the research and architecture into review-sized delivery
phases with tickable evidence. It is the execution companion to doc 143, not a
replacement for it.

---

## 1. How to use this checklist

A box may be checked only when its evidence is committed and named beside the
item. Design prose, a type with no product consumer, a disabled test, or a demo
that bypasses the public contract does not count.

Use these states in the dashboard:

- **Not started:** no accepted design or implementation evidence.
- **Designing:** decisions or contracts are under review; no support claim.
- **Implementing:** at least one reviewable slice has landed; phase exit gate is
  not met.
- **Blocked:** a named owner decision or external dependency prevents progress.
- **Done:** every required checkbox and exit gate is satisfied.

Rules:

1. Update `docs/14-EXECUTION-TRACKER.md` with each implementation slice.
2. Record durable architectural choices in `docs/08-ADR-REGISTER.md`.
3. Add behavior tests in the same change as behavior.
4. Keep `docs/15-CI-AND-RELEASE-GATES.md` and
   `docs/18-SUPPORT-MATRIX.md` accurate.
5. Do not check a parent item while a required child remains unchecked.
6. Do not publish participant counts, latency, offline, security, or provider
   compatibility claims without the corresponding retained test report.
7. Preserve local-only editing throughout; a collaboration regression must never
   make a relay necessary for opening, editing, history, or export.

### 1.1 Documentation map

| Document | Authority |
| --- | --- |
| Doc 45 / ADR-030 | Cross-cutting mutation, anchor, and sidecar invariants |
| Doc 107 / proposed ADR-033 | OT operation semantics, convergence, budgets, snapshot/replay |
| Docs 125–126 / ADR-036 | Existing embed, host contract, roles, capabilities, and white-label delivery |
| Docs 139–140 / ADRs 038 and 040 | History PRD, shipped H1/H2 baseline, restore, and future diff |
| Doc 143 | Competitive research and target provider/integration architecture |
| This document | Delivery order, implementation slices, evidence, and tickable gates |
| Doc 145 | Portfolio sequencing across PDF/OCR, assistance/MCP, history, and co-editing |
| Doc 15 | CI and release gates that must become blocking as phases land |
| Doc 18 | The only support claims that may be made at a given time |

If documents conflict on co-editing scope or sequence, doc 143 and this document
are the current proposed design; accepted ADRs remain authoritative for decisions.

## 2. Current readiness dashboard

| Phase | Outcome | Status | Blocking gate |
| --- | --- | --- | --- |
| C0 | Decisions, documentation, and baseline | Designing | Owner acceptance of ADR-033/provider boundary and open policy decisions |
| C1 | One revisioned mutation and commit substrate | Not started | C0 algorithm and persisted-schema direction |
| C2 | Complete external host document/identity boundary | Not started | C0 API ownership decisions; may proceed alongside C1 |
| C3 | Provider-neutral protocol and deterministic simulator | Not started | C1 + C2 exit gates |
| C4 | Proven OT transform and offline reconciliation | Not started | C1 operation vocabulary; C3 envelopes/simulator |
| C5 | Optional reference relay and browser provider | Not started | C3 + C4 correctness gates |
| C6 | Presence and collaboration UX | Not started | C3 simulator; product claim waits for C5 |
| C7 | Review, history, restore, and diff integration | Not started | C1, C4, C5; doc 140 H3/H4/H5 |
| C8 | External integrator preview and production hardening | Not started | C1–C7 complete |

### 2.1 Verified baseline — not collaboration

- [x] A framework-neutral `<opendoc-editor>` custom element exists.
  Evidence: `packages/opendoc-embed`.
- [x] One host command/event schema has same-origin and `postMessage`
  transports. Evidence: ADR-036 and `webapp/src/host_contract.mjs`.
- [x] Host roles/capabilities are enforced in the embed, chrome, and engine mode.
  Evidence: docs 125–126 and package tests.
- [x] Build-time white-labelling and chrome-region composition exist.
  Evidence: ADR-039 and doc 126 phase 3.
- [x] Local fidelity-complete history, preview, naming, pinning, deletion, and
  append-only restore exist. Evidence: doc 140 H1/H2 and ADR-038/040.
- [x] Collaboration algorithm and integration research exist. Evidence: docs
  107 and 143.
- [ ] A host can open caller-supplied bytes and receive exported bytes entirely
  through the public host contract.
- [ ] Every live mutation uses one revisioned transaction/commit path.
- [ ] A collaboration adapter, wire protocol, relay, or remote presence surface
  exists.
- [ ] Any real-time collaboration support claim is valid.

## 3. Dependency graph and parallel work

```text
                         C0 decisions
                         /          \
                        v            v
             C1 transaction core   C2 host I/O + identity
                        \            /
                         v          v
                     C3 protocol + simulator
                              |
                              v
                         C4 OT proof
                         /         \
                        v           v
                  C5 relay       C6 presence UX
                        \           /
                         v         v
                 C7 review/history/diff
                              |
                              v
                    C8 integrator preview
```

C1 and C2 may run in parallel after C0 because one is an engine boundary and the
other is a host boundary. C6 may start against the C3 simulator while C4/C5 are
underway, but it cannot graduate independently. All paths join before C7/C8.

## 4. Invariants that every phase must preserve

- [ ] **I1 — one mutation choke point:** local, remote, restore, review, host,
  and approved agent changes all use the same transaction service.
- [ ] **I2 — closed operations:** every operation is versioned, serializable,
  bounded, invertible where applicable, tier-classified, and has transform rules
  before it can ship.
- [ ] **I3 — stable anchors:** document, selection, comment, diff, and presence
  anchors use `NodeId`/`ModelPos`, never transient array indexes or DOM nodes.
- [ ] **I4 — derived sidecars:** presence, telemetry, embeddings, and unaccepted
  proposals never become hidden OOXML/model content.
- [ ] Unsupported or tombstoned work is refused or reported; it is never dropped
  silently.
- [ ] Source-format bytes and recovery checkpoints remain fidelity-complete.
- [ ] The provider is optional and replaceable; core has no provider dependency.
- [ ] Single-user operation remains functional with all network access blocked.
- [ ] Per-keystroke and remote-apply work respect doc 107 B1–B7.
- [ ] Host applications remain authoritative for storage, network, auth,
  telemetry, plugins, resources, and retention policy.

---

## 5. C0 — Decisions, documentation, and baseline freeze

### Goal

Remove contradictory specifications and accept enough policy to freeze the
operation, identity, provider, and security boundaries before implementation.

### Deliverables

- Accepted or rejected ADR-033.
- Accepted collaboration provider/embed boundary from doc 143.
- Resolutions for the eight open decisions in doc 143 §16.
- A versioning policy for persisted operation and collaboration envelopes.
- A threat model and data-flow inventory distinguishing source files,
  checkpoints, operation content, presence, audit metadata, and telemetry.
- A testable baseline report for current operation counts, mutation entry points,
  transaction consumers, host-contract verbs, and browser coverage.

### Checklist

#### Research and documentation

- [x] Audit the current embed, host-contract, history, and collaboration docs.
  Evidence: doc 143.
- [x] Compare official integration documentation for ONLYOFFICE,
  Collabora/WOPI, Microsoft 365, CKEditor, Yjs/Hocuspocus, ShareDB, Fluid, and
  Liveblocks. Evidence: doc 143 §3.
- [x] Define the proposed local-WASM plus optional-provider architecture.
  Evidence: doc 143 §§5–12.
- [x] Produce a phase plan and tickable checklist. Evidence: this document.
- [x] Reconcile every older document that still selects Yjs, says OT/CRDT is
  undecided, claims the npm package is published, or understates shipped embed
  and history behavior. Evidence: docs 08, 14, 15, 18, 45, 83, 99, 107,
  125–126, 140, 143, the repository README, and the embed package README.
- [ ] Add a documentation test/search guard for those superseded claims.

#### Owner decisions

- [ ] Accept, revise, or reject relay-ordered OT in ADR-033.
- [ ] Accept, revise, or reject the provider-neutral boundary in doc 143.
- [ ] Choose relay log ownership: relay store versus required host
  `OperationStore` port.
- [ ] Choose shared-room offline default: allowed, denied, or host policy with a
  documented product default.
- [ ] Decide table-geometry conflict semantics.
- [ ] Decide whether collaboration transforms raw edits before or after tracked
  change wrapping.
- [ ] Choose grant claims and behavior; defer token encoding unless required for
  interoperability.
- [ ] Decide whether the reference relay may store checkpoints.
- [ ] Decide operation/protocol compatibility and migration support windows.
- [ ] Decide npm/public-package release policy and owner-held publishing process.

#### Baseline evidence

- [ ] Enumerate every public/UI/WASM mutation entry point mechanically.
- [ ] Record the exact operation variants in both current operation sets.
- [ ] Record every transaction/revision consumer and live bypass.
- [ ] Measure local typing, undo, export, checkpoint, and replay baselines on
  named fixtures before architecture changes.
- [ ] Capture current package contents, host verbs/events, and browser test matrix.
- [ ] Publish the collaboration threat model for security review.

### Exit gate

- [ ] ADR-033/provider direction and C0 policy decisions are accepted.
- [ ] Superseded documentation claims are removed or visibly marked historical.
- [ ] Baseline evidence is committed and reproducible.
- [ ] C1/C2 public boundaries are named without reserving unapproved packages.

---

## 6. C1 — One revisioned mutation and commit substrate

### Goal

Make ADR-005 true in the live editor: every mutation becomes one atomic,
revisioned commit over one operation vocabulary, with inverses and position maps.

### Proposed review-sized slices

| Slice | Scope | Must not include |
| --- | --- | --- |
| C1.1 | Choose/extract the survivor operation vocabulary and remove the duplicate enum | Networking or provider types |
| C1.2 | Generalize transaction envelopes to the full operation set | OT transforms |
| C1.3 | Route WASM/UI/SDK mutation through the transaction service | Relay or presence |
| C1.4 | Emit structural mapping/liveness steps for all operations | UI collaboration chrome |
| C1.5 | Replace parallel undo history with commit/inverse history | Durable network log |
| C1.6 | Version and bound commit serialization with golden vectors | A public collaboration protocol |

### Checklist

#### Operation vocabulary

- [ ] There is exactly one public `Operation` vocabulary in the workspace.
- [ ] Every variant declares schema name/version, resource bounds, inverse
  behavior, affected anchors, capability class, and OT tier.
- [ ] Operations using UTF-8 offsets are isolated behind `ModelPos` helpers.
- [ ] Structural operations preserve or explicitly tombstone stable node identity.
- [ ] Adding a variant without metadata/tests fails CI.

#### Transaction path

- [ ] Every edit allocates a transaction ID and revision transition.
- [ ] UI commands, keyboard/IME, paste, table/object actions, review decisions,
  restore, host calls, and future remote calls share the same apply service.
- [ ] Atomic groups either fully commit or leave the previous state intact.
- [ ] A failed validation emits no revision, history item, or observation event.
- [ ] Commit results include inverse, mapping, affected set, origin, and bounded
  compatibility/disposition information.
- [ ] Transaction application does not clone the entire document per keystroke.

#### Undo/redo and observation

- [ ] Undo applies an inverse as a new commit rather than rewinding shared state.
- [ ] Redo semantics remain deterministic after intervening remote-compatible
  commits.
- [ ] Typing coalescing preserves the existing user-visible undo behavior.
- [ ] Events expose commit handles/metadata, not full document snapshots.
- [ ] Event backpressure and observation-gap recovery are explicit.

#### Verification

- [ ] Existing editor unit, WASM, browser, import/export, and history suites pass.
- [ ] A guard proves no mutation bypasses the transaction service.
- [ ] Native and WASM replay the same commit sequence to the same state hash.
- [ ] Serialization golden vectors cover every operation and unknown versions.
- [ ] Fuzz targets cover transaction decode, apply, inverse, and atomic failure.
- [ ] Typing and remote-ready apply benchmarks meet B1/B3/B4/B6.

### Exit gate

- [ ] One operation type and one live mutation path exist.
- [ ] Every successful mutation has a durable-compatible commit identity and map.
- [ ] Undo/redo behavior remains correct without a parallel history model.
- [ ] No collaboration/network code was required to obtain the value of C1.

---

## 7. C2 — Complete host document I/O and identity boundary

### Goal

Allow a third-party application to open, save, export, identify, and recover its
document entirely through the existing one-schema/two-transport host contract.

### Proposed review-sized slices

| Slice | Scope | Primary proof |
| --- | --- | --- |
| C2.1 | Versioned open/import request using transferable bytes or host resource handle | Same document opens through both transports |
| C2.2 | Export/save artifact handle or stream with explicit lifetime | Byte-identical result through both transports |
| C2.3 | Host `DocumentStore`/resource callbacks and cancellation | No implicit network or wildcard messaging |
| C2.4 | Lineage, branch, actor, session, and site identity contracts | Reopen/rejoin/copy cases are distinct |
| C2.5 | Host history capabilities/events | List/preview/restore policy enforced at every layer |

### Checklist

#### Document ingress

- [ ] Host can open `ArrayBuffer`/`Blob` or an approved host resource handle.
- [ ] URL loading, if supported, is an explicit host policy and never a hidden
  WASM network request.
- [ ] Format detection, size limits, cancellation, password/refusal, and
  compatibility findings match ordinary open behavior.
- [ ] Bytes cross an iframe with explicit origin and ownership/lifetime rules.
- [ ] Open is atomic: invalid input never replaces the active session.

#### Save/export

- [ ] Host can request source-format save and supported target exports.
- [ ] Large artifacts use transferable buffers, streams, or storage handles rather
  than ordinary event payloads.
- [ ] Artifact lifetime/free semantics are documented and tested.
- [ ] Compatibility findings and refusal codes accompany the artifact.
- [ ] Save and export capabilities remain independently withholdable.

#### Identity

- [ ] `DocumentLineageId` survives save and restore.
- [ ] “Make a copy” creates a new lineage.
- [ ] `BranchId`, `CommitId`, `ActorId`, `SessionId`, and `SiteId` have distinct,
  documented stability and collision rules.
- [ ] No identity is derived solely from filename, URL, timestamp, display name,
  email address, or content hash.
- [ ] Identity values are opaque to the engine and bounded at admission.

#### Contract parity and policy

- [ ] In-process and `postMessage` surfaces are generated from or checked against
  one schema.
- [ ] Unknown contract majors are refused before document bytes move.
- [ ] Capability changes never widen access through malformed input or defaults.
- [ ] Host events carry stable codes and handles; localized prose is not an API.
- [ ] Same-origin and cross-origin examples run in CI.

### Exit gate

- [ ] An external test host can open, edit, save/export, close, and reopen its own
  file through both transports without private editor APIs.
- [ ] The host can assign durable identity and history policy.
- [ ] Network interception proves the local-only profile sends nothing externally.

---

## 8. C3 — Provider-neutral protocol and deterministic simulator

### Goal

Freeze collaboration behavior against an in-memory deterministic transport before
introducing sockets, deployment, or distributed failure.

### Proposed review-sized slices

| Slice | Scope |
| --- | --- |
| C3.1 | Session descriptor, capability grant claims, negotiation, and stable errors |
| C3.2 | Durable operation envelope, acknowledgement, deduplication, and sequence |
| C3.3 | Ephemeral presence envelope and expiry |
| C3.4 | `CollaborationAdapter` lifecycle and cancellation |
| C3.5 | Deterministic simulator for delay, reorder, duplicate, drop, partition, and reconnect |
| C3.6 | Checkpoint-plus-tail bootstrap and state-hash verification |

### Checklist

#### Protocol schema

- [ ] Session descriptor includes contract/operation/engine versions, lineage,
  branch, actor, session, site, capabilities, expiry, watermark, and state hash.
- [ ] Operation envelope includes idempotency ID, base revision, bounded
  transaction, origin, capability class, affected anchors, and integrity data.
- [ ] Presence has its own lossy envelope, rate limit, expiry, and size bound.
- [ ] Every field has a bound and unknown enum/version behavior.
- [ ] Golden vectors and migration fixtures are language/runtime independent.

#### Adapter lifecycle

- [ ] States cover idle, authenticating, bootstrapping, catching up, synced,
  offline, reconnecting, read-only, rejected, incompatible, and desynchronized.
- [ ] Join, leave, cancellation, teardown, and token refresh are idempotent.
- [ ] Disconnect removes local presence without a durable content commit.
- [ ] Destroying an embed leaves no connection, timer, callback, or pending promise.
- [ ] State transitions emit machine-readable host events and accessible UI facts.

#### Simulator

- [ ] Seeded scheduler reproduces every delivery order.
- [ ] It can delay, duplicate, reorder, drop, partition, reconnect, and expire
  sessions without real time or a real network.
- [ ] It models stale checkpoints, compacted tails, permission loss, and token expiry.
- [ ] It asserts content commits are never treated like lossy presence.
- [ ] It produces a minimal replay artifact for every failure.

#### Bootstrap and catch-up

- [ ] Room creation is compare-and-set and idempotent.
- [ ] Late join validates checkpoint hash/version before activation.
- [ ] Ordered tail replay is bounded by explicit compaction policy.
- [ ] State-hash disagreement enters `desynchronized` and preserves recovery data.
- [ ] Stale bases outside the transform window request checkpoint rebase; no guess.

### Exit gate

- [ ] Two and five simulated clients deterministically join, edit, disconnect,
  catch up, and leave with identical state.
- [ ] Protocol behavior is transport independent.
- [ ] Every failure state has a stable code, recovery rule, and test.

---

## 9. C4 — OT transforms and offline reconciliation

### Goal

Prove convergence for the full supported operation set and define visible,
recoverable outcomes for operations that cannot survive a concurrent change.

### Proposed review-sized slices

| Slice | Scope |
| --- | --- |
| C4.1 | T1 insert/delete/split/join transforms and same-position tie-break |
| C4.2 | T1 format, hyperlink, and inline-object transforms |
| C4.3 | T2 node-liveness, rebase, and tombstone dispositions |
| C4.4 | Table-geometry concurrency rules |
| C4.5 | T3 document-scope serialization/conflict outcomes |
| C4.6 | Tracked-change/comment/review interaction |
| C4.7 | Bounded offline queue, rebase, and recovery artifact |

### Checklist

#### Transform correctness

- [ ] Every operation is classified T1, T2, or T3.
- [ ] T1 ordered pairs have explicit transform behavior or a justified symmetric
  rule generated from one authority.
- [ ] Same-position insert ordering and selection affinity agree.
- [ ] Delete/format overlap rules are documented and tested.
- [ ] Node removal/replacement maps anchors or emits a tombstone disposition.
- [ ] T3 conflicts are deterministic, visible, and re-doable.
- [ ] Table row/column/grid races have accepted semantics.

#### Review semantics

- [ ] Direct edits and suggesting-mode edits transform in the accepted order.
- [ ] Authorship is preserved across transform and replay.
- [ ] Concurrent accept/reject decisions are deterministic and idempotent.
- [ ] Comment anchors map or become explicitly unlinked; threads are not lost.
- [ ] Per-user undo never undoes another actor's accepted commit.

#### Formal/property evidence

- [ ] TP1 property tests cover generated valid concurrent pairs.
- [ ] Multi-operation scenarios cover three or more clients and pending queues.
- [ ] Fuzzing covers transform, compose, inverse, mapping, and replay.
- [ ] Native and WASM results are byte/state equivalent where specified.
- [ ] A new operation cannot compile/pass CI without tier and transform coverage.

#### Offline reconciliation

- [ ] Offline edit permission is explicit host policy with a product default.
- [ ] Pending queue is durable, bounded, inspectable, and never silently evicted.
- [ ] Reconnect fetches acknowledged head/tail before resubmitting pending IDs.
- [ ] Duplicate resubmission returns the first acknowledgement.
- [ ] Unmergeable work is preserved in an exportable recovery artifact.
- [ ] Every rejection/tombstone is visible in the disposition channel.

#### Performance

- [ ] Local typing remains O(1) in document size.
- [ ] Transform cost is O(concurrent operations since base), not total log length.
- [ ] Remote apply uses incremental layout invalidation.
- [ ] Snapshot/compaction work is never performed per keystroke.
- [ ] Benchmarks cover increasing concurrent depth and pathological documents.

### Exit gate

- [ ] TP1, fuzz, replay, offline, review, table, and performance gates pass.
- [ ] Simulator clients converge without locks for supported concurrent edits.
- [ ] Unsupported races yield explicit recovery/disposition, never silent loss.

---

## 10. C5 — Optional reference relay and browser provider

### Goal

Implement the smallest separately deployable coordination service that proves the
provider port without becoming a document parser, converter, renderer, or required
single-user dependency.

### Proposed review-sized slices

| Slice | Scope |
| --- | --- |
| C5.1 | Transport framing, connection multiplexing, heartbeat, and backpressure |
| C5.2 | Authentication, grant verification, tenant/document routing, and revocation |
| C5.3 | Ordered sequence, idempotency, acknowledgement, and bounded catch-up |
| C5.4 | Operation-store port, compaction leases, and checkpoint references |
| C5.5 | Presence fan-out, coalescing, expiry, and rate limits |
| C5.6 | Horizontal scale, pub/sub or sharding profile, health, and metrics |
| C5.7 | Deployment image/configuration and operator runbook |

### Checklist

#### Service boundary

- [ ] Relay is a separate optional artifact with no core/runtime dependency.
- [ ] It never parses DOCX/ODT, lays out pages, renders, or converts formats.
- [ ] It admits only versioned bounded operation/presence envelopes.
- [ ] It can delegate durable log/checkpoint storage through the accepted port.
- [ ] Single-user and local history work when the relay is absent or unreachable.

#### Authentication and authorization

- [ ] Grants are short-lived, audience-bound, tenant/document/branch scoped, and
  revocable.
- [ ] No signing secret ships to a browser.
- [ ] Provider verifies capability class on every operation.
- [ ] Connection authentication has absolute time/byte/message ceilings.
- [ ] Tenant, document, and branch isolation have adversarial tests.
- [ ] Permission loss has an explicit client transition and local-recovery path.

#### Ordering, storage, and recovery

- [ ] Sequence assignment is atomic and monotonic per branch.
- [ ] Client operation IDs make submission idempotent.
- [ ] Catch-up and replay are bounded by checkpoint/tail policy.
- [ ] Compaction cannot remove a required base, named version, active lease, or
  recovery point.
- [ ] Corrupt/missing storage is isolated and reported.
- [ ] Restart, failover, and duplicate-delivery tests preserve convergence.

#### Resource and abuse controls

- [ ] Limits exist for connections, rooms, participants, envelope bytes,
  operations/transaction, pending submissions, presence rate, catch-up size, and
  retention.
- [ ] Backpressure never creates an unbounded browser or server queue.
- [ ] Presence may coalesce/drop; content commits may not.
- [ ] Slow-consumer, reconnect-storm, oversized, malformed, and unauthorized
  workloads are tested.
- [ ] Logs/metrics exclude content by default.

#### Privacy

- [ ] Documentation states that operation payloads reveal changed content to a
  normal terminating provider.
- [ ] Payload inspection proves source-format files/rasters are not sent in the
  default profile.
- [ ] Checkpoint storage, if allowed, is explicit host policy with documented
  encryption/key ownership.
- [ ] No zero-knowledge or E2EE claim is made without a separate accepted design.

#### Operations

- [ ] Health/readiness, graceful drain, metrics, structured audit events, and
  alert thresholds are documented.
- [ ] Deployment examples use non-development secrets and TLS termination.
- [ ] Upgrade/rollback and protocol compatibility procedures are tested.
- [ ] Backup/restore and regional/disaster policy state what the relay does and
  does not own.

### Exit gate

- [ ] Real browsers pass the C3/C4 suite through the reference relay.
- [ ] Kill/restart, network partition, token expiry, role loss, late join,
  compaction, and backpressure recover without silent loss.
- [ ] A custom fake provider passes the same conformance suite, proving replacement.

---

## 11. C6 — Presence and collaboration UX

### Goal

Make shared editing understandable and accessible without mixing ephemeral
presence into durable document state.

### Proposed review-sized slices

| Slice | Scope |
| --- | --- |
| C6.1 | Sync-state model, status UI, and host events |
| C6.2 | Participant list and actor/session grouping |
| C6.3 | Remote carets and selections in body and every editing surface |
| C6.4 | Offline, reconnect, permission-loss, conflict, and recovery UX |
| C6.5 | Accessibility, responsive/touch, motion, contrast, and scale hardening |

### Checklist

#### Presence model

- [ ] Actor and session are distinct; multiple tabs show one person with multiple
  sessions where appropriate.
- [ ] Presence fields are bounded, sanitized, rate-limited, and non-authoritative.
- [ ] Presence expires on disconnect and is never replayed from history.
- [ ] Colors are deterministic, distinguishable, theme-safe, and not the sole cue.
- [ ] Unknown/malformed presence cannot affect document state or HTML.

#### Rendering

- [ ] Remote carets/selections map through every incoming/local pending commit.
- [ ] Body, table cells, headers/footers, notes, and text boxes are covered.
- [ ] Virtualization/zoom preserve correct remote geometry.
- [ ] Offscreen collaborators do not force page mounting or full-document scans.
- [ ] Dense collaborator overlays degrade legibly under an explicit policy.

#### State and recovery UX

- [ ] Connected, catching up, synced, offline, reconnecting, read-only,
  incompatible, rejected, and desynchronized have distinct states.
- [ ] Unsynced local work count/status is visible without modal spam.
- [ ] Permission loss preserves work and offers an authorized recovery action.
- [ ] Tombstones/conflicts use non-blocking summaries plus inspectable detail.
- [ ] Desynchronization stops mutation and explains recovery; no silent refresh.

#### Accessibility and devices

- [ ] Participant and sync changes reach an appropriate live region without
  announcing every keystroke.
- [ ] Remote cursors are exposed semantically only where useful, without focus theft.
- [ ] Keyboard-only users can inspect participants and recovery actions.
- [ ] High contrast, reduced motion, zoom, narrow viewport, touch, and coarse
  pointer journeys pass.
- [ ] Screen-reader testing covers at least the declared browser/AT matrix.

#### Performance

- [ ] Presence updates coalesce and remain off the durable commit path.
- [ ] Remote overlay work is proportional to visible collaborators/pages.
- [ ] Tests cover 2, 5, 20, and the eventual claimed participant count without
  publishing a scale claim prematurely.

### Exit gate

- [ ] Every collaboration state is understandable through UI and host events.
- [ ] Remote presence works across all editable surfaces and viewport changes.
- [ ] Accessibility and main-thread budgets pass on the supported matrix.

---

## 12. C7 — Review, history, restore, and diff integration

### Goal

Join real-time commits with comments, suggestions, durable versions, append-only
restore, attribution, and structural diff without creating parallel histories.

### Proposed review-sized slices

| Slice | Scope |
| --- | --- |
| C7.1 | Durable actor/origin attribution on unified commits |
| C7.2 | Collaborative comments/suggestions and concurrent review decisions |
| C7.3 | Version grouping/naming over shared commits and checkpoints |
| C7.4 | Shared-room append-only restore and collaborator notification |
| C7.5 | Structural version diff with anchors and incomplete findings |
| C7.6 | Per-user undo/redo across remote commits |

### Checklist

#### Attribution and audit

- [ ] Durable commits carry opaque actor, session, origin, and optional automation
  identity without treating display metadata as authority.
- [ ] Grouping preserves every commit and discloses incomplete attribution after
  compaction/snapshot-only history.
- [ ] Audit projection is bounded and host-authorized.
- [ ] AI/automation commits are distinguishable from the approving human.

#### Comments, suggestions, and decisions

- [ ] Comment/reply/resolve/edit/delete operations synchronize through the same log.
- [ ] Suggesting mode preserves author and revision identity across transform.
- [ ] Concurrent accept/reject is deterministic and idempotent.
- [ ] Anchors map or become visibly unlinked; content/thread data is retained.
- [ ] Review permissions are enforced provider-to-engine.

#### Versions and restore

- [ ] Visible versions bind to shared commit/checkpoint identities.
- [ ] Naming/pinning/deletion conflicts have deterministic compare-and-set behavior.
- [ ] Restore first preserves current work, validates the target, and appends one
  new shared restore commit.
- [ ] Restore never rewrites the branch head backward or deletes later history.
- [ ] Collaborators receive pre-activation state and post-commit result.
- [ ] Failed/stale restore leaves all clients on the prior complete state.

#### Diff

- [ ] Diff is a read-only derived sidecar, never a mutation during inspection.
- [ ] Stable-ID and independently imported alignment paths are both tested.
- [ ] Text, formatting, list, table, section, object, comment, and review changes
  have typed outcomes or explicit `not_compared` findings.
- [ ] Navigation anchors remain valid against the previewed versions.
- [ ] Compare-to-tracked-changes is a separate explicit transaction.

#### Undo/redo

- [ ] Local undo applies the actor's inverse as a new commit.
- [ ] Remote commits remain intact.
- [ ] Undo after restore/review actions follows accepted product semantics.
- [ ] Reload/reconnect does not invent unavailable undo depth.

### Exit gate

- [ ] Shared comments, suggestions, versions, restore, diff, and undo pass
  multi-client race and crash tests.
- [ ] One commit/history identity model serves local, remote, review, restore, and
  approved automation changes.

---

## 13. C8 — External integrator preview and production hardening

### Goal

Make collaboration consumable by another product through stable artifacts,
complete examples, conformance tests, deployment guidance, and honest support
limits.

### 13.1 Package and compatibility checklist

- [ ] Publish or deliberately defer the embed package under an accepted version
  and provenance policy.
- [ ] Publish collaboration contracts/adapters only after names and support policy
  are accepted; do not reserve aspirational packages.
- [ ] Declare semantic versioning for host contract, operation schema, wire
  protocol, checkpoint schema, and provider conformance independently.
- [ ] Provide compatibility negotiation and a tested N/N-1 upgrade path or state a
  narrower policy explicitly.
- [ ] Sign/checksum release artifacts and publish SBOM/licence information.
- [ ] Consumer type-check, bundler, SSR import, and tree-shaking tests pass.
- [ ] React/Vue/Svelte examples use the same custom element/contract rather than
  separate behavior.

### 13.2 Required integrator documentation

- [ ] Five-minute local-only embed quickstart.
- [ ] Host document open/save/export guide.
- [ ] Same-origin direct-session guide.
- [ ] Cross-origin iframe and origin-policy guide.
- [ ] Identity, roles, capabilities, and token/grant guide.
- [ ] Two-user collaboration quickstart using the reference relay.
- [ ] Custom `CollaborationAdapter` implementation guide.
- [ ] Operation/checkpoint storage adapter guide.
- [ ] Offline/reconnect and local-recovery guide.
- [ ] Comments, suggestions, history, restore, and diff guide.
- [ ] White-label/chrome composition guide.
- [ ] Security/privacy/threat-model guide, including operation-content visibility.
- [ ] Relay deployment, scaling, backup, upgrade, and rollback runbook.
- [ ] Troubleshooting guide for refusal, incompatible, desynchronized, corrupt
  checkpoint, quota, slow consumer, and provider outage states.
- [ ] Protocol/schema migration guide.
- [ ] Support matrix with exact browser, format, role, offline, participant, and
  provider limits.

Every snippet/example above must execute in CI against public artifacts.

### 13.3 Conformance and release checklist

- [ ] Provider conformance kit runs against reference and independent fake provider.
- [ ] Chromium, Firefox, and WebKit multi-context suites pass.
- [ ] Declared screen-reader/device/accessibility matrix passes.
- [ ] Security review covers auth, tenant isolation, origin, replay, injection,
  limits, dependencies, secrets, audit, and content privacy.
- [ ] Load/soak/chaos reports use named hardware, topology, document fixtures, and
  participant counts.
- [ ] B1–B7 and browser main-thread/peak-memory budgets pass.
- [ ] Local-only network interception and source-file non-transmission tests pass.
- [ ] Data migration, backup/restore, rolling upgrade, and rollback drills pass.
- [ ] Known limitations and recovery guarantees are published.
- [ ] No critical/high unresolved document-loss or authorization defect exists.

### Exit gate

- [ ] A new external integrator can embed a host-owned document, add collaboration,
  enforce roles, save/recover, and operate the relay using only published artifacts
  and documentation.
- [ ] The same integration works without collaboration configuration as a fully
  local editor.
- [ ] Support matrix and release evidence state only what the gates prove.

---

## 14. End-to-end user-scenario acceptance matrix

| Scenario | Required phases | Acceptance checkbox |
| --- | --- | --- |
| Local open/edit/save with network blocked | C1, C2 | [ ] Passes through public host contract with no request |
| Two live editors type in the same paragraph | C3–C6 | [ ] Converges with correct carets and per-user undo |
| Simultaneous table row/column changes | C4–C6 | [ ] Converges or reports accepted explicit conflict outcome |
| Commenter suggests but cannot direct-edit | C2–C7 | [ ] Refused at provider, contract, engine, and chrome layers |
| User edits offline and reconnects | C3–C6 | [ ] Rebase succeeds or preserves recovery artifact; no loss |
| User loses edit permission with pending work | C3, C5, C6 | [ ] Becomes readable and offers explicit work recovery |
| Late join after compaction | C3–C5 | [ ] Checkpoint + bounded tail reaches identical state hash |
| Restore an old version while others are online | C5–C7 | [ ] Current state preserved; one restore commit reaches all clients |
| Compare two versions | C7 | [ ] Read-only typed diff; ambiguous/uncompared areas disclosed |
| Same actor in two tabs/devices | C2, C3, C6, C7 | [ ] One actor, distinct sessions/sites, correct attribution |
| Provider restarts during editing | C5 | [ ] Idempotent reconnect/catch-up; no duplicate commit |
| Incompatible client joins | C3, C5 | [ ] Refused before mutation with upgrade guidance |
| Corrupt checkpoint or state-hash mismatch | C3, C5, C6 | [ ] Mutation stops; recovery data retained; no silent repair |
| Malicious/unauthorized tenant client | C5, C8 | [ ] Cannot observe or mutate another tenant/document/branch |
| Replace reference relay with another provider | C3, C5, C8 | [ ] Conformance kit passes without engine/editor fork |

## 15. Cross-cutting release checklists

### Document safety

- [ ] No silent operation, anchor, comment, suggestion, resource, or history loss.
- [ ] Recovery artifact exists when pending work cannot be merged.
- [ ] Restorable checkpoints remain source-format fidelity-complete.
- [ ] Export/reopen fixed points cover collaboration-created documents.
- [ ] Ordinary DOCX/ODT exports contain no hidden OpenDoc collaboration history.

### Security and privacy

- [ ] Least-privilege grants and revocation are enforced on every operation.
- [ ] Origin and tenant isolation are tested adversarially.
- [ ] Resource bounds cover unauthenticated and authenticated paths.
- [ ] Presence/display metadata is sanitized and bounded.
- [ ] Documentation distinguishes local-only, source-file-local, provider-visible
  edit content, checkpoint storage, telemetry, and any future E2EE profile.

### Determinism and compatibility

- [ ] Same checkpoint + ordered commits yields the same state across supported
  native/WASM builds.
- [ ] Schema/protocol golden vectors are immutable and versioned.
- [ ] Unknown/newer majors fail closed without damaging local work.
- [ ] Rolling upgrades and mixed supported versions pass the declared matrix.

### Performance and scale

- [ ] Typing/remote apply do not scan, clone, validate, or re-layout the whole
  document per operation.
- [ ] Pending queues, logs, checkpoints, presence, overlays, and events are bounded.
- [ ] Late join and reconnect work is bounded by checkpoint/tail policy.
- [ ] Participant/latency claims cite retained named-environment reports.

### Accessibility and UX

- [ ] Collaboration is understandable without color, animation, hover, or vision.
- [ ] Sync/recovery states never steal focus or trap keyboard users.
- [ ] Live announcements are useful and rate-limited.
- [ ] Touch, narrow viewport, zoom, high contrast, and reduced motion pass.

### Operations

- [ ] Provider health, saturation, rejection, transform depth, catch-up, compaction,
  and desynchronization metrics exist without content logging by default.
- [ ] Runbooks cover outage, overload, compromised credential, corrupt storage,
  failed upgrade, rollback, and disaster recovery.
- [ ] Retention, deletion, export, and audit responsibilities are explicit between
  OpenDoc, provider, and host.

## 16. Definition of done

Real-time collaboration is **Done** only when all of the following are true:

- [ ] C0–C8 exit gates are checked with committed evidence.
- [ ] Proposed ADR-033 and all implementation-shaping decisions are accepted or
  superseded by accepted ADRs.
- [ ] The public host API opens/saves host documents without private seams.
- [ ] Every mutation uses one revisioned commit path.
- [ ] OT convergence and offline recovery are property/fuzz tested.
- [ ] Provider replacement is proven by conformance tests.
- [ ] Presence, comments, suggestions, history, restore, diff, and per-user undo
  behave coherently across clients.
- [ ] Security, privacy, accessibility, cross-browser, performance, scale, and
  operations gates pass.
- [ ] Local-only editing still works with the network blocked.
- [ ] Public documentation, package status, support matrix, migration guide, and
  limitations match the released artifacts.

Until then, the accurate support statement remains: **OpenDoc has an embeddable
single-user browser editor and local history/restore foundations; real-time
co-editing is proposed, not implemented or supported.**

## 17. Checklist maintenance template

Use this block when a slice is opened in the execution tracker:

```text
Slice:
Owner:
Status: Designing | Implementing | Blocked | Done
Depends on:
Public behavior changed:
Schema/API version impact:
Security/privacy impact:
Performance budget:
Tests added:
Docs/ADR updated:
Evidence/PR:
Unchecked follow-ups:
```

When a checkbox is checked, append a short evidence reference rather than merely
changing `[ ]` to `[x]`. That makes this document auditable and prevents a future
reader from treating intention as implementation.
