# 143 — Embedded Co-Editing Research and Integration Architecture

**Status:** Proposed architecture; research and documentation only. Real-time
collaboration is not implemented or supported.

**Reviewed:** 2026-09-28.

**Depends on:** ADR-005, ADR-006, ADR-030, proposed ADR-033, ADR-036,
ADR-038, ADR-040, and docs 05, 45, 68, 82, 83, 107, 125, 126, 139, and 140.

This document answers two different questions:

1. how other products let a host embed and operate co-editing; and
2. how OpenDoc should expose co-editing without turning its browser-local WASM
   document engine into a mandatory document server.

It refines the integration and deployment parts deliberately left open by doc
107. It does not accept ADR-033, select a wire transport, reserve a package name,
or authorize implementation.

---

## 1. Executive conclusion

OpenDoc can support an embeddable Google-Docs-like co-editing experience, but
remote real-time collaboration cannot truthfully be a zero-service feature. Two
browsers need a rendezvous point, an ordering authority, or a peer-discovery and
signalling system. The correct product boundary is therefore:

- **document processing stays local:** import, normalized model, layout,
  rendering, editing, diff projection, and export remain in Rust/WASM;
- **single-user editing needs no server:** the current local-first property is a
  release gate, not a fallback mode;
- **remote co-editing uses an optional host-selected collaboration provider:**
  it authenticates a session, orders and fans out bounded operation envelopes,
  provides catch-up, and distributes ephemeral presence;
- **the provider is not a DOCX server:** it does not download the source file,
  parse OOXML, render pages, convert formats, or become the authoritative live
  document model;
- **the provider is not automatically zero-knowledge:** operation envelopes
  contain the text and properties being edited unless a separately designed
  end-to-end encryption layer protects them;
- **the host owns identity, authorization, storage, retention, network policy,
  and deployment:** OpenDoc supplies contracts and a reference adapter, not a
  compulsory backend or hosted account.

This is a distinct integration shape from both office-suite document servers
(ONLYOFFICE/Collabora/Microsoft 365) and editor-specific collaborative data
frameworks (Yjs/Fluid/ShareDB). It preserves OpenDoc's differentiator: a host can
open, edit, and save a real DOCX entirely in the browser, and add only the small
network plane required when people actually collaborate.

## 2. Current OpenDoc state — audited, not inferred

### 2.1 What is already usable by an embedding host

| Capability | Current evidence | Honest status |
| --- | --- | --- |
| Framework-neutral mount | `@casualoffice/opendoc-embed` and `<opendoc-editor>` | Built; source/tarball installable, not published to npm |
| Isolation | Custom element owns a sandboxed iframe | Built |
| Same- and cross-origin control | One host schema exposed through direct frame session and correlated `postMessage` client | Built; ADR-036 |
| Host commands/events | `describe`, `query`, `execute`, `ping`; `ready`, `change`, `selection`, `save`, `export`, `error`, `refusal` | Built |
| Capability policy | Five role presets, per-capability narrowing, API gate, engine mode, chrome state, browser sandbox | Built |
| White-label build | Generated brand tokens, names, icons, strings, and selectable chrome regions | Built; build-time rather than per-instance runtime theme |
| Local version timeline | Fidelity-complete IndexedDB checkpoints, metadata list, preview, name/pin/delete, and append-only restore | H1/H2 built; structural diff and collaboration attribution are not |
| Co-editing | OT design, transaction/log prerequisites, budgets, and phasing | Design only; no provider, relay, remote cursor, or protocol |

The existing custom element is iframe-based. “Two transports” means a host may
call the same session object directly when it can access the frame, or use the
`postMessage` client across an origin boundary. It does not mean there are two
different editor implementations.

### 2.2 Gaps that block a third-party co-editing integration

1. **No host document-ingress contract.** The package mounts the editor, but the
   public host contract cannot yet open a caller-supplied byte stream or URL and
   cannot return exported bytes. An integration cannot safely bootstrap a host's
   document with the public API alone.
2. **The live mutation path still bypasses the transaction substrate.** Doc 107
   found a broad `casual-doc-edit` operation set in the live editor and a narrow,
   separate `casual-doc-transaction` set carrying revisions and mappings.
   Collaboration cannot be added until there is one committed operation path.
3. **No collaboration adapter contract.** There is no stable session handshake,
   operation envelope, acknowledgement, catch-up, presence, or provider lifecycle.
4. **No durable cross-device identity contract.** Local history now has lineage,
   but actor, site, session, branch, and provider document identities are not yet
   joined across hosts and reconnects.
5. **No server-enforceable role contract.** Browser capabilities protect an
   honest embed. A collaboration service must also reject unauthorized operation
   classes; chrome hiding and client checks do not protect a shared room.
6. **No collaboration compatibility policy.** Persisted operations require
   operation-schema and engine-version negotiation plus a refusal path for an
   incompatible client.
7. **No sync-state UX or observability.** Connected, catching up, offline,
   reconnecting, rejected, conflicted, and desynchronized states need host events
   and accessible editor feedback.
8. **The public docs conflicted at audit start.** Doc 83 named a Yjs adapter
   although the later owner decision selected OT; doc 45 called the choice
   deferred; and the package README's limitations section predated the
   command/event and white-label work. Those documentation defects are corrected
   alongside this document; they were not implementation gaps.

The first two gaps are hard prerequisites. A collaboration API built before them
would freeze the wrong document-open and mutation boundaries.

## 3. What established integrations actually require

Official product documentation was reviewed on 2026-09-28. Marketing feature
counts are intentionally excluded; the comparison is about integration burden
and architectural ownership.

| Product/framework | Embed shape | Shared-state/service shape | Host owns | Important lesson for OpenDoc |
| --- | --- | --- | --- | --- |
| ONLYOFFICE Docs | Script creates a `DocsAPI.DocEditor` iframe | Mandatory Document Server; same document `key` joins a session; server fetches source and calls host `callbackUrl` | Document URL/key, JWT, permissions, callback/save endpoint | Easy iframe integration, but document processing and collaboration are coupled to a server |
| Collabora Online / Microsoft 365 for the web | WOPI discovery produces an iframe URL; `postMessage` integrates host UI | Mandatory office service talks to host WOPI REST endpoints; file IDs, access tokens, locks, get/put file | Storage, WOPI host, identity/access token, host page | WOPI is a valuable storage/host boundary, not a suitable client-side document-engine API |
| CKEditor 5 RTC | Editor plugins inside the host UI | CKEditor Cloud Services SaaS or on-prem service handles RTC, users, comments/suggestions/revisions | Token endpoint and document-content persistence unless Cloud document storage is used | Collaboration, comments, suggestions, and revisions must share identities and version semantics |
| Yjs + y-websocket | Library/provider attached to a `Y.Doc` | Replaceable WebSocket/WebRTC/database providers; `y-indexeddb` adds browser persistence | Schema/editor binding, provider deployment, auth, durable policy | Separate persistent shared state from ephemeral awareness and make providers swappable |
| Hocuspocus | Yjs provider in the client | Self-hosted WebSocket server with auth, load/store hooks, webhooks, persistence extensions | Token, document name, storage, hook policy | Provider lifecycle hooks and bounded pre-auth resources are first-class product requirements |
| ShareDB | Browser client subscribes to a document | Node backend orders OT operations, persists through database adapters, scales through pub/sub | OT type, server/auth middleware, storage/pub-sub | An OT framework supplies transport machinery, not OpenDoc's 47-operation transform rules |
| Fluid Framework | App loads a permission-bound container and shared objects | A Fluid service supplies ordering, storage, container lifecycle, and audience/presence | Container schema, service choice, token/auth, app UI | Document identity should be a capability/permission boundary; connection identity is not durable user identity |
| Liveblocks Sync/Yjs | SDK enters a room and binds editor/shared state | Hosted sync, presence, permissions, history, reconnect, server-side/agent editing | Room authorization and application integration | A polished provider bundles operational concerns; OpenDoc's adapter must expose enough lifecycle to offer this later |

### 3.1 Primary-source findings

- ONLYOFFICE co-editing groups clients by an integrator-generated document key;
  the editor service sends changes between clients, and its callback handler
  notifies host storage about session/save states. JWT is shared between the
  integrator's server and ONLYOFFICE Docs. See the official
  [co-editing flow](https://api.onlyoffice.com/docs/docs-api/get-started/how-it-works/co-editing/),
  [callback contract](https://api.onlyoffice.com/docs/docs-api/usage-api/callback-handler/),
  and [security model](https://api.onlyoffice.com/docs/docs-api/get-started/how-it-works/security/).
- Collabora documents the WOPI host as the storage-facing server and the editor as
  an iframe discovered from the Collabora service. Microsoft describes the same
  separation for Microsoft 365 for the web: WOPI endpoints, discovery, a host
  page/iframe, optional `postMessage`, file IDs and locks. See the
  [Collabora SDK manual](https://sdk.collaboraonline.com/CO-SDK-manual.pdf),
  [Microsoft WOPI integration overview](https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/online/),
  and [co-authoring requirements](https://learn.microsoft.com/en-us/microsoft-365/cloud-storage-partner-program/online/scenarios/coauth).
- Yjs treats network synchronization, local database persistence, and awareness
  as separate providers/protocols. Awareness is intentionally not persisted and
  disappears when a client goes offline. See
  [y-websocket](https://docs.yjs.dev/ecosystem/connection-provider/y-websocket),
  [offline persistence](https://docs.yjs.dev/getting-started/allowing-offline-editing),
  and [awareness](https://docs.yjs.dev/getting-started/adding-awareness).
- ShareDB is explicitly an OT client/server framework: its backend coordinates
  and commits edits, while the application must register the actual OT type on
  client and server. It provides middleware, database/pub-sub adapters, offline
  reconnection, historical snapshots, and document presence. See its
  [overview](https://share.github.io/sharedb/),
  [OT type contract](https://share.github.io/sharedb/types/), and
  [presence API](https://share.github.io/sharedb/api/presence).
- Fluid containers are shared-state and permission boundaries; the service also
  supplies an audience, while newer Presence state is session-scoped. See
  [containers](https://fluidframework.com/docs/build/containers),
  [audience](https://fluidframework.com/docs/build/audience), and
  [presence](https://fluidframework.com/docs/build/presence).
- Hocuspocus exposes provider tokens and awareness on the client and
  authentication, connection, load/store, change, disconnect, and webhook hooks
  on the server. See the
  [provider configuration](https://tiptap.dev/docs/hocuspocus/provider/configuration),
  [server hooks](https://tiptap.dev/docs/hocuspocus/server/hooks), and
  [server limits/configuration](https://tiptap.dev/docs/hocuspocus/server/configuration).
- CKEditor distinguishes sequential/asynchronous collaboration, where the host
  supplies persistence adapters, from real-time collaboration backed by its SaaS
  or on-prem service. Its RTC integration joins content, comments, suggestions,
  users/presence, and revision history. See the
  [collaboration overview](https://ckeditor.com/docs/ckeditor5/latest/features/collaboration/collaboration.html)
  and [RTC integration guide](https://ckeditor.com/docs/ckeditor5/latest/features/collaboration/real-time-collaboration/real-time-collaboration-integration.html).
- Liveblocks illustrates the operational bundle a future managed OpenDoc provider
  would need: sync, presence, optimistic updates, reconnect catch-up, multiplayer
  undo, history/restore, and room permissions. See the
  [Sync overview](https://liveblocks.io/docs/products/sync) and
  [Yjs provider API](https://liveblocks.io/docs/api-reference/liveblocks-yjs).

### 3.2 What not to copy

- Do not require a server merely to open, render, edit, or export a document.
- Do not make the collaboration provider the only holder of recoverable document
  state.
- Do not expose a second model or mutable JSON shadow beside OpenDoc's normalized
  model.
- Do not equate a room name, connection ID, display name, or filename with a
  durable document/user identity.
- Do not persist cursors and typing indicators in the document log.
- Do not accept a provider-specific data type as the core model. A Yjs, Fluid,
  ShareDB, or hosted adapter may exist later, but it must implement the OpenDoc
  collaboration port.

## 4. User and integrator scenarios

The API is derived from these jobs rather than from a chosen transport.

| Scenario | User outcome | Host obligation | OpenDoc obligation |
| --- | --- | --- | --- |
| Local-only editing | Open/edit/save with no network | Supply bytes or a storage adapter | No collaboration dependency or network call |
| Same-browser multi-view | Two embeds show one document consistently | Choose a shared local session | Reuse committed operations; presence may use `BroadcastChannel` |
| Remote live editing | Editors see committed changes and cursors promptly | Supply provider endpoint and scoped grant | Transform/apply remote commits deterministically |
| Review session | Commenters suggest/comment but cannot make direct edits | Issue role/capability grant | Enforce operation classes in UI, engine, host contract, and provider |
| Disconnect/reconnect | User continues locally and later reconciles | Permit or forbid offline edits by policy | Queue bounded commits, rebase, disclose conflicts/tombstones |
| Late join | New participant reaches the same state without full-log replay | Retain checkpoint/log tail or provide storage hooks | Validate checkpoint, replay ordered tail, verify state hash |
| Host save | Host persists a fidelity-complete artifact | Implement storage callback/port | Export from committed local state and report compatibility findings |
| History/restore | Users preview and restore without erasing later history | Authorize history capabilities and persist policy | Reuse doc 140 append-only restore; broadcast it as a new commit |
| Version diff | Users compare named/current states | Make both checkpoints available | Produce a derived structural diff; never mutate during inspection |
| Multiple tabs/devices | One user may have several sessions | Supply stable actor ID and unique session/site IDs | Attribute by actor, order/tie-break by site/session |
| Provider replacement | Host moves from reference relay to managed/self-built provider | Implement the same adapter and migration policy | No engine or editor rewrite |

## 5. Proposed logical architecture

```text
Host application
  identity/auth · storage · retention · network policy · telemetry
       |                         |
       | existing host contract | short-lived collaboration grant
       v                         v
<opendoc-editor> / iframe ---- CollaborationAdapter port
       |                         |
       v                         v
HostSession              optional collaboration provider
commands/events          auth · order · ack · catch-up · presence
       |                         |
       +------------+------------+
                    v
          Transaction / Commit service
       one op vocabulary · inverse · maps · OT
                    |
                    v
      normalized model -> layout -> canvas -> export
                    |
                    v
       HistoryStore / CheckpointStore ports
       browser IndexedDB or host-selected storage
```

The provider is a port at the transaction boundary, not inside DOCX import,
layout, rendering, or UI components. The existing host contract remains the
outer control surface; collaboration adds lifecycle methods and events to that
same versioned schema rather than inventing a third iframe protocol.

## 6. Ownership boundaries

| Concern | Core/runtime | Editor/embed | Host | Collaboration provider |
| --- | --- | --- | --- | --- |
| Parse/save DOCX/ODT | Own | Expose | Supply/persist bytes | Never |
| Model and operation validity | Own | Report | — | Validate envelope/schema/bounds only |
| OT transform and deterministic apply | Own | Drive/display | — | Never reinterpret operations |
| Document/branch IDs | Validate/use | Carry | Mint/map | Route by opaque IDs |
| Actor identity and roles | Use opaque IDs/capabilities | Reflect/refuse | Authoritative | Verify scoped grant |
| Sequence/order/ack/catch-up | Protocol contract | Client state | Select policy/provider | Own |
| Durable document bytes | Checkpoint schema | Request/display | Own storage policy | Optional opaque storage only if host chooses |
| Durable operation tail | Define/version | Queue locally | Own retention policy | Store or delegate through port |
| Presence/cursors | Anchor/map | Render | Supply display metadata policy | Fan out ephemerally |
| History/restore/diff | Semantics/validation | UI and events | Authorization/storage | Order restore commit |
| Telemetry/audit | Emit bounded facts | Surface | Own collection/consent | Emit provider metrics/audit events |

## 7. Deployment profiles

### P0 — Local-only baseline

No provider configuration and no network behavior. This must remain the default
and must pass with network interception enabled. Local history uses IndexedDB or
the host storage port.

### P1 — Host-managed relay (recommended first release)

The host deploys or selects a provider and mints short-lived grants. Browsers
still parse and render locally. The relay receives only versioned operation and
presence envelopes. It may persist an ordered tail through a host storage port.

### P2 — OpenDoc reference relay

A separately deployable, replaceable reference implementation proves the port
and gives small integrators a working path. It is not linked into the core and
is not required for P0. Production use needs a durable adapter, rate limits,
tenant isolation, observability, and deployment guidance.

### P3 — Managed provider adapter

A host or vendor maps OpenDoc envelopes onto a hosted collaboration service.
This is valuable only if it preserves OpenDoc commit identity, permissions,
history semantics, and deterministic replay; translating the document into a
provider-owned rich-text model is not acceptable.

Peer-to-peer synchronization is explicitly not a v1 profile. It changes threat,
identity, discovery, and partition semantics and fits a future CRDT adapter more
naturally than the proposed relay-ordered OT design.

## 8. Public collaboration contract

Names below are conceptual. Acceptance requires a separate API/ADR review.

### 8.1 Session descriptor

A join request needs:

- contract and operation-schema versions;
- engine/schema compatibility range;
- opaque `DocumentLineageId`, `BranchId`, and provider room locator;
- durable `ActorId`, unique `SessionId`, and collision-resistant `SiteId`;
- role plus explicit capabilities, with an expiry;
- current checkpoint/commit/revision watermark and optional state hash;
- locale/display metadata separated from authoritative identity;
- provider URL and transport options supplied by the host;
- an opaque short-lived grant, never a long-lived shared secret in the bundle.

The room locator is routing data. It is not proof of access. A filename, URL,
email address, or display name must not be used as any durable identifier.

### 8.2 Durable operation envelope

Every submitted commit needs:

- protocol and operation-schema version;
- lineage, branch, actor, session, and site identities;
- client operation ID for idempotency;
- base ordered revision and locally allocated transaction ID;
- one bounded committed transaction from the closed operation set;
- origin kind: human edit, review action, restore, automation, or approved agent;
- affected-anchor summary and required capability class;
- optional compatibility/disposition summary;
- integrity data over the envelope.

The provider assigns the authoritative branch sequence and acknowledges the
client operation ID. Duplicate submissions return the first acknowledgement.
An unknown version, over-limit payload, expired grant, wrong branch, stale base
outside the retained transform window, or unauthorized operation is refused with
a stable code; none is silently coerced.

### 8.3 Ephemeral presence envelope

Presence is a separate, lossy, rate-limited channel containing only what the UX
needs: actor/session/site, display label/color, activity state, and mapped
selection/caret anchors. It is not checkpointed, replayed, exported into DOCX,
or used as authorization evidence. Stale presence expires without a durable
leave commit.

### 8.4 Provider lifecycle

The adapter must expose observable states:

`idle -> authenticating -> bootstrapping -> catching-up -> synced`

and recoverable branches for `offline`, `reconnecting`, `read-only`, `rejected`,
`incompatible`, and `desynchronized`. Hosts receive stable state/error codes;
the editor supplies localized human messages. Cancellation and teardown are
required, including removal of presence and zero pending callbacks after the
element is disconnected.

### 8.5 Existing host-contract extensions

The one-schema/two-transport rule from ADR-036 continues. A future additive
surface should cover:

- configure/join/leave collaboration;
- query connection, sync, pending-commit, and collaborator state;
- collaboration-state, collaborator, remote-change, conflict/disposition, and
  permission-change events;
- open host-supplied bytes and request an export artifact through bounded handles;
- history list/preview/restore/diff capabilities from doc 140.

Large bytes and snapshots must use transferable handles/streams or host storage
references. They must not be copied into ordinary per-change events or broadcast
through wildcard `postMessage` targets.

## 9. Bootstrap, editing, reconnect, and restore flows

### 9.1 Join and late-join

1. Host loads source bytes locally and opens an isolated session.
2. Host supplies the collaboration descriptor and short-lived grant.
3. Provider authenticates and returns the accepted protocol/schema range,
   branch head, and a checkpoint/tail plan.
4. Client verifies the checkpoint hash and compatibility before activation.
5. Client replays the ordered tail through the normal transaction path.
6. Client compares the resulting state hash/watermark, then becomes `synced`.
7. Presence starts only after content bootstrap succeeds.

The host may instead declare its opened bytes to be the initial checkpoint for a
new room. Two clients may never both initialize the same empty room; provider
creation is compare-and-set and idempotent.

### 9.2 Local edit

1. UI command passes capability policy.
2. Runtime creates and validates one transaction against the current revision.
3. It applies optimistically, emits inverse/mapping/affected anchors, and appends
   to the bounded local pending queue.
4. Adapter submits the envelope.
5. Provider rejects it or assigns the next branch sequence, persists/delegates
   the accepted envelope under host policy, and fans it out.
6. Acknowledgement binds the local operation ID to ordered commit identity.

The UI never waits for network round-trip before painting an authorized local
keystroke. A rejection is visible and recoverable; it must not quietly delete the
user's text.

### 9.3 Incoming remote commit

1. Verify envelope version, identity, order, bounds, and deduplication.
2. Transform it against unacknowledged local commits.
3. Apply through the same transaction service as a local command.
4. Map selection, comments, review anchors, presence, and viewport decorations.
5. Incrementally invalidate layout; never repaginate the whole document solely
   because the source was remote.
6. Emit one host event with handles and metadata, not document bytes.

### 9.4 Offline and reconnect

Offline edits are a host policy, not an accidental behavior. If permitted, the
client stores a bounded pending queue beside a fidelity-complete checkpoint. On
reconnect it requests the ordered tail since its last acknowledgement, rebases
pending commits, reports every tombstone/conflict disposition, and resubmits with
the same operation IDs. If the base is older than the retained transform window,
the provider requests a checkpoint rebase; it must not guess.

### 9.5 Restore

Doc 140 remains authoritative: restore validates a fidelity-complete checkpoint,
preserves the current state first, and appends a new restore commit. In a shared
room restore requires a distinct capability, is ordered like any document-scope
operation, and notifies collaborators before activation. It never rewrites the
provider's log head backward or erases later versions.

## 10. Roles and authorization

Role presets are convenience. The wire carries explicit capabilities so a host
can narrow a role without inventing a new role vocabulary.

| Capability class | Typical operations |
| --- | --- |
| `view` | Join, catch up, receive content and presence |
| `comment` | Add/reply/resolve permitted comment threads |
| `suggest` | Submit tracked insertion/deletion/format proposals |
| `edit` | Direct content and structure operations |
| `review` | Accept/reject suggestions and moderate comments |
| `history.read` | List/preview/diff versions |
| `history.restore` | Append a restore commit |
| `share.admin` | Change room grants/provider policy; host-side only |

Enforcement is layered:

1. host issues the least-privilege scoped grant;
2. provider verifies tenant/document/branch/capability/expiry per envelope;
3. host-contract session refuses before command dispatch;
4. engine enforces edit/suggest/view semantics;
5. chrome reflects the result with a reason.

The provider does not trust a client-supplied role label. Grant changes take
effect through a sequenced policy event; losing edit permission leaves the
document readable and preserves unsent local work for an explicit recovery path.

## 11. Security, privacy, and resource bounds

- Grants are short-lived, audience-bound, tenant/document/branch scoped, and
  revocable. Browser bundles contain no provider signing secret.
- Cross-origin embeds retain ADR-036's explicit origin allowlist and correlated
  envelopes. Collaboration configuration does not create a wildcard side channel.
- The provider admits a closed operation schema under per-message, per-transaction,
  pending-queue, participant, presence-rate, log-tail, and catch-up limits.
- Authentication has an absolute deadline and strict pre-auth byte/message caps;
  Hocuspocus's documented pre-auth limits are a useful precedent.
- Tenant routing and storage keys include opaque tenant and lineage IDs; no global
  room-name namespace is assumed safe.
- Presence display values are untrusted, bounded presentation data and are never
  inserted as HTML.
- External links, images, fonts, plugins, and AI providers remain host policy.
- The relay receives no source document or raster by default. If a host elects to
  store encrypted checkpoints with it, that is an explicit P1/P2 storage adapter
  choice and must not change the local-only claim.
- Operation payloads necessarily reveal changed text and formatting to a provider
  that terminates the normal protocol. “No document-server processing” means the
  provider does not receive or parse the source-format file; it does not mean the
  collaboration service cannot observe edits. A zero-knowledge/E2EE profile would
  need a separate key-distribution, search/audit, abuse-control, and recovery design.
- Audit records contain identities, outcomes, refusal codes, and commit metadata;
  content logging is off by default and host-controlled.
- A state-hash mismatch transitions to `desynchronized`, stops mutation, retains
  local recovery material, and requests a validated checkpoint. Silent repair is
  forbidden.

## 12. Performance and operational budgets

Doc 107's B1–B7 remain normative. Integration adds:

- one browser connection may multiplex several document sessions, but isolation,
  backpressure, and teardown remain per document;
- presence is coalesced and may be dropped; content commits are acknowledged and
  never dropped silently;
- a typing run is batched at the transaction layer, not by rebuilding a paragraph;
- late join uses checkpoint plus bounded tail, never an unbounded replay;
- provider backpressure cannot block input or allow an unbounded local queue;
- remote apply and selection mapping have percentile latency gates at several
  participant and pending-operation depths;
- the provider exports connection count, auth failures, ack latency, catch-up
  depth, transform depth, rejection/tombstone counts, checkpoint age, and
  desynchronization events without document content.

No participant-count claim should be published until a named environment and
document workload establish it.

## 13. Recommended integration journey

The eventual developer path should be five explicit steps:

1. install or build the embed artifact and self-host its static editor/WASM files;
2. mount `<opendoc-editor>` with a role, chrome selection, accessible frame title,
   and explicit editor/host origins;
3. open host-supplied bytes through the versioned host contract and subscribe to
   save/error/refusal state;
4. optionally obtain a short-lived collaboration grant from the host and attach
   a provider adapter with stable lineage/actor IDs;
5. persist exported checkpoints and operation tails using host-owned storage and
   surface provider health/retention policy to administrators.

The documentation set needs, before a collaboration preview:

- a runnable plain-JavaScript two-user example;
- React/Vue/Svelte usage of the same custom element without separate behavior;
- same-origin and cross-origin iframe examples;
- a complete auth/token and role-narrowing example;
- self-hosted relay and custom-provider guides;
- storage/checkpoint and restore guides;
- reconnect, expired-token, permission-loss, incompatible-client, and provider
  outage examples—not only the happy path;
- a protocol/version migration guide and an operator runbook.

Examples must execute in CI. A snippet is not evidence of support merely because
it appears in a design document.

## 14. Delivery sequence and gates

| Stage | Deliverable | Exit gate |
| --- | --- | --- |
| C0 | Reconcile docs; accept/reject this integration architecture | No current page claims a published package, Yjs choice, or shipped collaboration |
| C1 | One operation vocabulary and committed transaction path | Every public/UI/WASM mutation emits one revisioned commit and mapping; ADR-005 holds |
| C2 | Host document I/O and durable identity contract | External host opens bytes, saves/exports, reopens, and retains lineage through both host transports |
| C3 | Versioned collaboration port and deterministic in-memory simulator | Lifecycle, dedupe, ordering, catch-up, permission, disconnect, and incompatible-version tests pass without a network |
| C4 | OT transform tiers | TP1 property/fuzz tests, tombstone reporting, table-race decision, tracked-change ordering, B1–B7 budgets |
| C5 | Reference relay plus browser adapter | Two and five clients converge; late join, kill/reload, offline rebase, token expiry, role loss, and backpressure pass |
| C6 | Presence and collaboration UX | Remote cursors/selections, participant list, accessible sync state, reconnect and conflict recovery pass |
| C7 | History/restore/diff integration | Shared restore appends one commit; versions preserve attribution; diff remains read-only derived data |
| C8 | External integrator preview | Tarball/registry policy resolved, examples and operator docs pass CI, cross-browser/security/load matrix published |

Current status is **C0 design**. C1 and C2 are blocking architecture work, not
tasks to hide inside a networking PR.

The review-sized slices, dependencies, evidence rules, end-to-end scenarios,
and tickable C0–C8 acceptance checklists live in doc 144. A checkbox there is
execution evidence, not a support claim; doc 18 remains authoritative for support.

## 15. Required CI additions before a support claim

- one-operation-vocabulary and every-mutation-through-transaction guards;
- operation schema golden vectors and backward/forward compatibility fixtures;
- TP1 property tests and fuzzing across valid concurrent operation pairs;
- deterministic replay and state-hash equality across native/WASM and reconnects;
- duplicate, reordered, delayed, dropped, and partitioned transport simulation;
- checkpoint plus tail late-join, compaction, stale-base, and corrupt-checkpoint tests;
- permission matrix at provider, host contract, engine, and chrome boundaries;
- tenant/document isolation, expired/revoked token, origin, replay, rate, size,
  pre-auth resource, and malicious-presence tests;
- comments, suggestions, decision, undo, restore, and version-attribution races;
- browser kill/reload and offline edit/reconnect without silent loss;
- Chromium, Firefox, and WebKit multi-context runs before a cross-browser claim;
- screen-reader, keyboard, touch, high-contrast, reduced-motion, and narrow-view
  collaboration journeys;
- provider load and chaos runs in a named environment, with retained reports;
- a network-interception test proving local-only P0 performs no collaboration call
  and a content-inspection test proving P1 sends no source file bytes.

## 16. Open decisions

1. Accept ADR-033 and this provider/host split, or revise the OT choice before API
   names or persisted operation schemas are frozen.
2. Decide whether the first reference relay stores a bounded log itself or calls a
   required host `OperationStore` port. The latter better preserves host ownership;
   the former is easier to operate.
3. Define the offline-edit default for shared rooms: permitted, denied, or host
   policy. The architecture supports policy; product UX needs one default.
4. Resolve table-geometry concurrency and tracked-change wrapping order from doc
   107 before C4.
5. Decide the minimum browser cryptographic/token profile and whether grants are
   JWT, PASETO, or an opaque provider token. The contract should specify claims
   and behavior, not force one encoding unless interoperability requires it.
6. Decide whether checkpoints may ever be stored by the reference relay. If yes,
   encryption/key ownership and the public privacy claim need a separate ADR.
7. Decide registry publication/versioning for `@casualoffice/opendoc-embed` before
   publishing install commands. The repository currently proves tarball contents;
   it does not prove registry availability.
8. Define provider compatibility and migration guarantees before a durable log is
   released. “Best effort replay” is not a document-safety policy.

## 17. Recommendation

Proceed with the architecture, but do not start with WebSockets or a hosted
service. First close C0–C2: reconcile the public documentation, unify mutation
and commit identity, and give an external host a complete open/save boundary.
Then build the collaboration port against a deterministic in-memory simulator.
Only after convergence, authorization, replay, and recovery pass without a real
network should a reference relay be added.

This ordering makes every early step useful for embedding, history, restore,
diff, crash recovery, and agent proposals even if the eventual transport or
provider changes.
