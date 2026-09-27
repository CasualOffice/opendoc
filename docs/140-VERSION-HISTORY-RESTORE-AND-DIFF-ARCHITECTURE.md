# 140 — Version History, Restore, and Diff Architecture

**Status:** Accepted for the local storage layer, and **implemented** for H1's store and
H2's restore coordinator (`webapp/src/version_history.mjs`, schema v3 in
`webapp/src/drafts.mjs`). H3 (diff), H4 (unified commits) and H5 (collaboration) remain
proposed. Nothing here is a support claim: the layer is not reachable from the editor yet.
Decisions this document left open about identity, retention and restore atomicity are
settled in **ADR-038**.

**Opened:** 2026-09-27. Storage layer landed 2026-09-28.

**Product requirements:** doc 139.

**Architectural decision:** refines the snapshot/replay portion of proposed ADR-033
(doc 107). It does not accept ADR-033 or select a collaboration transport.

**Depends on:** docs 03, 05, 14, 15, 21, 24, 25, 27, 35, 45, 59, 71, 82,
94–98, 107, 112, 116, 125, and 139.

---

## 1. Decision summary

1. **One history domain, two time scales.** Immediate Undo/Redo and durable version
   history derive from the same committed transaction identities, but retain different
   data and have different UX.
2. **Append; never rewind.** Restore appends a new restore commit and new head. It never
   moves a pointer backward while discarding later history.
3. **Fidelity-complete checkpoints.** Restoration uses validated source-format artifact
   bytes produced through the ordinary preservation export ladder. Normalized JSON alone
   is forbidden as a restore artifact because doc 112 proved it loses binary resources
   and retained source data.
4. **Semantic projections are secondary.** A canonical normalized projection may be
   stored beside a checkpoint for diffing and indexing, but it is derived and never the
   sole recovery source.
5. **Diff is read-only derived data.** It is computed outside the authoritative model and
   rendered as an overlay/change list. It creates tracked changes only through a separate
   explicit Compare action.
6. **Host owns durable storage and policy.** Core owns schemas, validation, replay,
   restore atomicity, diff semantics, and bounds. Hosts own storage backend, retention,
   authorization, encryption, sync, and telemetry.
7. **No mandatory server.** The browser baseline uses the local persistence seam already
   established by doc 112. Collaboration enriches the same log later.
8. **Metadata and content are separate.** Listing history never loads document-sized
   checkpoint bytes.
9. **All public state changes use the transaction/session boundary.** History is not a
   hidden browser-only model mutation path.

## 2. Current repository fit

### Present foundations

- `casual-doc-edit` has a broad closed operation set with exact inverses.
- `casual-doc-transaction` has atomic application, revision checks, commit results, and
  position mapping for its narrow Phase-0 operation set.
- `casual-doc-sdk` exposes revisioned Undo/Redo for that narrow path.
- `casual-doc-wasm` has the broad live editor, semantic history labels, typing
  coalescing, and a 256-action in-memory Undo/Redo cap.
- docs 68, 82, 86, and 133 provide attributed comments, suggestions, review decisions,
  and review projection.
- doc 112 ships source-format IndexedDB drafts, recovery offers, quota handling, and a
  host-controlled autosave capability.
- format adapters already expose preservation-aware export and compatibility findings.
- normalized snapshots are deterministic and bounded.

### Blocking gaps

1. **Two operation/transaction paths.** Doc 107 proves that the live WASM editor bypasses
   `casual-doc-transaction`. Exact durable per-change history cannot claim one commit
   identity until those paths converge.
2. **Session-local revision identity.** The current revision is not a durable document
   commit ID and resets on reopen.
3. **No fidelity-complete session checkpoint contract.** Format bytes can reconstruct the
   document, but no stable SDK value binds bytes, format, engine/schema version,
   compatibility report, resources, and content hash into one history artifact.
4. **No document lineage identity.** Filename and byte hash are unsuitable: Save As,
   restored content, identical copies, and renamed files require explicit identities.
5. **No historical diff service.** Review markup describes authored suggestions, not
   differences between arbitrary states.
6. **No history capabilities/events.** The host contract covers current commands and
   autosave but not historical content, restore, or purge.
7. **No cross-session author contract.** `ActiveAuthor` is adequate for authored review
   data but not yet a durable, host-scoped principal identity.

These gaps define the delivery order. They do not justify direct mutation of WASM model
state or storing lossy JSON snapshots.

## 3. Logical architecture

```text
OpenDoc UI / Host API / Headless host / Collaboration adapter
                           |
                    capability boundary
                           |
                           v
                 History Application Service
       +-----------------------------------------------+
       | list/group/name/pin/preview/copy/download     |
       | restore coordinator and failure recovery      |
       | retention/compaction planner                  |
       | attribution and audit projection              |
       +----------------------+------------------------+
                              |
             +----------------+----------------+
             v                                 v
      Commit/Transaction Service            Version Diff Service
      +-------------------------+            +---------------------+
      | apply + inverse         |            | load projections    |
      | revision/commit IDs     |            | align structures    |
      | mapping + affected set  |            | classify changes    |
      | undo/redo + events      |            | anchors + findings  |
      +------------+------------+            +----------+----------+
                   |                                    |
                   v                                    v
        authoritative live session             derived diff sidecar
                   |
                   v
              Checkpoint Service
      +---------------------------------+
      | preservation export ladder     |
      | validate reopen + hash         |
      | optional semantic projection   |
      +----------------+----------------+
                       |
                       v
              Host HistoryStore port
      +---------------------------------+
      | metadata | commits | artifacts |
      | pins     | leases  | tombstones|
      +---------------------------------+
        browser IndexedDB      native/host-defined store
```

Layout and rendering consume the selected live or preview session. They do not own
history. DOCX/ODT import/export do not know whether their bytes came from a current file,
draft, version preview, or restore checkpoint.

## 4. Domain model

The names below are conceptual public contracts, not implementation code.

### 4.1 Identities

| Identity | Stability | Purpose |
| --- | --- | --- |
| `DocumentLineageId` | persists across saves/restores | one logical document history |
| `CommitId` | globally unique within host domain | one atomic state transition |
| `VersionId` | stable user-visible point | metadata/actions for a restorable state |
| `CheckpointId` | content-addressed or random + hash | stored reconstruction artifact |
| `ActorId` | host-scoped opaque identity | attribution/authorization join |
| `SessionId` | one editing session | grouping and diagnostics |
| `BranchId` | one ordered lineage branch | offline divergence/collaboration |

None is derived solely from filename, timestamp, display name, or engine-local numeric
revision.

### 4.2 Commit record

A durable commit record contains:

- schema version and `CommitId`;
- lineage, branch, and parent commit IDs;
- monotonically ordered branch sequence;
- engine revision before/after for the producing session;
- transaction ID and operation-envelope version;
- closed origin kind;
- initiating actor and optional automation/agent actor;
- host-supplied timestamp plus monotonic local ordering data;
- semantic action kind from doc 71;
- bounded affected anchors/construct families;
- operation payload or a checkpoint transition reference;
- inverse reference while retained for Undo;
- compatibility/disposition summary;
- integrity hash and engine/schema versions.

Display names, initials, and colors are presentation metadata. `ActorId` remains opaque to
the core and does not grant permissions.

### 4.3 Version record

A version is a projection over a commit/checkpoint:

- `VersionId`, lineage, branch, and commit ID;
- optional checkpoint ID;
- optional user name and pin state;
- created/updated timestamps and actor;
- kind: automatic, saved, named, import, restore, recovery, or policy checkpoint;
- grouped-commit range;
- parent visible version;
- restorable/previewable/diffable state plus explicit reason when false;
- content size, format, word/page hints, compatibility summary;
- retention class and expiry if any.

Renaming a version changes metadata, not historical content or commit identity.

### 4.4 Fidelity-complete checkpoint

A checkpoint manifest binds:

- immutable artifact bytes in a fidelity-capable format;
- format ID and export mode used (`exact_if_unchanged`, `preserve_when_safe`, or
  `semantic`);
- compatibility report and any loss accepted at creation;
- content hash, byte length, and checksum algorithm;
- engine, schema, adapter, and preservation-envelope versions;
- document lineage/commit/version identities;
- optional canonical semantic projection hash/artifact;
- optional preview thumbnails, never authoritative;
- required external-resource manifest, with embedded bytes or resolvable host handles;
- creation cancellation/completion state.

For DOCX/ODT/RTF, the primary artifact uses the document's source format and the same
export ladder as Save and doc 112. Plain text and normalized JSON histories must promote
to a fidelity-capable package when edits introduce structure/resources those formats
cannot carry. The UI shows the effective format.

An artifact is restorable only after it passes bounded reopen, model validation, resource
resolution, and hash verification. Merely writing bytes is not enough.

## 5. Undo/Redo and durable history

### 5.1 Same commits, different retention

Undo/Redo needs exact inverses, selection restoration, and a short action-oriented stack.
Version history needs durable checkpoints, metadata, grouping, and bounded long-term
retention.

They share `CommitId` and action kind:

```text
user gesture -> transaction -> commit
                         |-> inverse retained for Undo window
                         |-> durable commit metadata
                         |-> checkpoint when policy requests one
```

History must not store one full checkpoint per keystroke. Undo must not reconstruct a
document by diffing snapshots. Both would violate the main-thread and storage budgets.

### 5.2 Undo as a new commit

Undo applies the retained inverse as a new commit whose origin is `undo` and whose
`undoes_commit` points to the original. Redo similarly references the undo. The original
commit remains in durable history. UI grouping may hide these mechanical details unless
expanded.

Remote commits create an Undo boundary. A later collaboration design may provide
per-author undo, but it may not erase other authors' commits.

### 5.3 Selection state

An Undo entry carries bounded pre/post selection and edit-context anchors. Mapping uses
the transaction position map. If an anchor is no longer valid, history restores the
nearest safe engine-resolved position and reports that degradation; the browser does not
guess from coordinates or text.

## 6. Checkpoint policy

### 6.1 Creation triggers

Candidate triggers are:

- initial import/open baseline;
- explicit Save/export;
- explicit **Name this version**;
- before and after Restore;
- bounded periodic history checkpoint while dirty;
- collaboration compaction boundary;
- host policy or SDK request.

Crash drafts keep doc 112's faster cadence and single-slot overwrite. They are not added
to the visible timeline until recovery is accepted or policy explicitly promotes them.

### 6.2 Main-thread policy

The edit path records constant-size commit metadata and schedules work. Serialization,
hashing, compression, semantic projection, diff, and storage pruning never execute inside
the keystroke transaction.

Where available, a Worker performs compression, hashing, and storage preparation. **As
built**, hashing is `crypto.subtle.digest("SHA-256", bytes)` rather than a Worker: the
browser computes it off the JavaScript thread already, and it is awaited *before* the
IndexedDB transaction opens, because awaiting a non-IDB promise inside a live transaction
lets it auto-commit underneath the caller. WASM model export remains on the thread/profile
the runtime supports, scheduled after quiescence and bounded by doc 116 — and version capture
adds none of its own: it reuses the artifact the doc 112 autosave path has already exported,
so a captured version costs one hash and one transaction, not a second export. Future worker-owned runtime execution can move it without changing
the contract.

### 6.3 Compression

Doc 112 measured compression as unnecessary for eight draft slots but explicitly deferred
the decision for multi-version history. History adopts per-artifact compression only after
measuring:

- compression ratio by source format;
- CPU and wall time;
- peak memory;
- browser support and Worker behavior;
- restore latency;
- whether DOCX/ODT ZIP bytes gain enough to justify recompression.

Already-compressed container formats may receive little benefit. The manifest records the
codec and uncompressed hash. There is always a bounded no-compression profile.

## 7. Browser storage architecture

### 7.1 Reuse the existing persistence seam

Doc 112's owner decision requires one browser store for drafts, versions, and recent
documents. Do not create an unrelated version database.

The existing `opendoc-drafts` IndexedDB can migrate transactionally to a later schema with
logical stores such as:

- existing `meta` and `bytes` draft stores;
- `documents` for lineage metadata and policy;
- `version_meta` for metadata-only panel reads;
- `checkpoint_blobs` for artifact bytes keyed by checkpoint/content identity;
- `semantic_projections` for optional diff projections;
- `commits` for durable commit envelopes when transaction unification lands;
- `pins`/`leases` or equivalent metadata required for compaction and active previews.

**As built (schema v3):** `documents`, `version_meta` (indexed by lineage and by checkpoint),
`checkpoint_blobs` keyed by the SHA-256 of the artifact, and `history_ops` for prepared
restores. Two corrections to the list above. A **pin is a boolean field on the version row**,
not a `pins` store: it protects exactly that row, and a separate store would be a second
thing to keep consistent with it. `semantic_projections` and `commits` are **not created** —
an empty store is a claim that something is being written to it, and neither the diff
projection (H3) nor the durable commit envelope (H4) exists yet.

The database name is legacy implementation detail; public APIs refer to `HistoryStore`.

### 7.2 Atomic writes

Metadata that references a blob becomes visible only in the same committed IndexedDB
transaction that writes or verifies the blob. A large-artifact backend such as OPFS would
require a two-phase protocol:

1. write temporary content and verify hash;
2. commit metadata referencing the finalized content;
3. garbage-collect unreachable temporaries.

OpenDoc must not list a restorable version whose bytes are incomplete.

### 7.3 Multi-tab coordination

Reuse doc 112's explicit cross-tab presence mechanism. Add a per-lineage writer lease or
equivalent optimistic generation check so two tabs cannot independently prune or advance
the same local head without detection.

Conflicting active tabs create explicit branches/version groups. Last-writer-wins is not an
acceptable silent merge policy.

### 7.4 Storage degradation

Quota or unavailable IndexedDB transitions history to a visible degraded state. Current
editing continues, autosave behavior follows doc 112, and the UI says that new versions
cannot be retained. It does not delete pinned versions automatically to make space.

## 8. Native and embedded storage port

The host-facing `HistoryStore` responsibilities are conceptual:

- list bounded version metadata;
- atomically put/get/delete immutable artifacts;
- append/read commit envelopes by lineage and branch;
- compare-and-set the current head;
- pin/unpin and apply retention plans;
- report quota/capabilities;
- cancel long reads/writes;
- purge a lineage under policy.

The port does not accept arbitrary SQL or filesystem paths. The runtime validates every
record returned by a host as untrusted data.

An in-memory provider keeps the SDK usable without persistence. A host that advertises
durable history must pass the same crash, atomicity, corruption, and retention conformance
suite as the browser adapter.

## 9. Restore architecture

### 9.1 Restore state machine

```text
Idle
  -> LoadingTarget
  -> ValidatingTarget
  -> CheckpointingCurrent
  -> Prepared
  -> CommittingRestore
  -> ActivatingNewHead
  -> Complete

Any pre-commit failure -> Idle with no visible change
Any post-commit interruption -> recover from prepared record to old or complete new head
```

### 9.2 Prepare

The coordinator:

1. checks `history.restore` and target lineage;
2. reads and hashes target artifact bytes;
3. opens them through the bounded normal format path in an isolated session;
4. resolves resources and validates the model;
5. calculates compatibility findings and target/current summary;
6. creates/verifies a fidelity-complete checkpoint for the current head;
7. records a prepared restore with expected current head (compare-and-set token).

No live state changes during prepare.

### 9.3 Commit and activation

The history store atomically appends a restore record and advances the lineage head only
if the expected head still matches. The active host then switches to the already-validated
target session and initializes its engine revision as the next state of the logical
lineage.

The restore record references:

- restored-from `VersionId`/checkpoint;
- pre-restore head/checkpoint;
- initiating actor and confirmation;
- compatibility findings;
- new commit/version identity;
- idempotency key.

Retrying the same idempotency key returns the existing result. It cannot create repeated
restores.

### 9.4 Restore Undo

In-session Undo invokes the same coordinator targeting the pre-restore checkpoint and
records origin `undo`. It does not swap undocumented browser variables. After the Undo
window expires, the pre-restore version remains visible and can be restored normally.

### 9.5 Selection and UI state

Restoration resets selection to an engine-defined safe start or a persisted checkpoint
selection only if that contract is later accepted. Scroll position, open panels, and hover
state are not document history. The UI exits preview, shows a restore result banner, marks
the document dirty, and emits a host change/history event.

### 9.6 Failure and recovery

A durable prepared record is recoverable after a crash. Startup resolves it by validating
which head and artifact committed; it never guesses from timestamps. Orphan artifacts are
garbage-collected only after proving that no version, draft, prepared restore, or preview
lease references them.

## 10. Version grouping

Grouping is a deterministic projection over immutable commits. Boundary signals include:

- actor/session change;
- explicit Save, Name, Restore, Import, Recovery, or automation plan;
- editing mode/review-decision boundary;
- idle interval exceeding host presentation policy;
- branch or collaboration synchronization boundary;
- maximum commits/elapsed duration per group.

The host may choose display cadence within bounded policy, but the same inputs and policy
version produce the same groups. Group IDs are derived metadata and may be rebuilt. Named
versions point to exact commits, not group positions.

## 11. Version diff service

### 11.1 Inputs and output

Inputs:

- two version/checkpoint identities;
- comparison profile and requested construct families;
- cancellation token and resource limits.

Output is a versioned `VersionDiff` sidecar:

- left/right identities and hashes;
- completeness and compatibility summary;
- ordered change records;
- per-author/per-kind counts;
- unresolved/not-compared findings;
- anchors for both sides where available;
- elapsed/resource diagnostics safe for hosts.

### 11.2 Alignment pipeline

1. Load bounded semantic projections or open isolated checkpoint sessions.
2. Match stable `NodeId` identities when the states share the same retained lineage.
3. Match unmatched structure by story, section, heading path, block kind, table topology,
   and bounded content hashes.
4. Diff paragraph inline trees with grapheme-aware text and mark/style runs.
5. Diff structural/property objects by typed fields, not serialized JSON text.
6. Detect moves only above a confidence threshold; otherwise report delete + insert.
7. Produce explicit `not_compared`, `ambiguous_match`, or `missing_resource` findings.
8. Sort changes by document order with deterministic tie-breaking.

Stable IDs improve alignment but are never assumed across independently imported files.

### 11.3 Change record

A change record contains:

- stable diff-change ID;
- family and kind (`insert`, `delete`, `replace`, `move`, `format`, `property`,
  `review`, `resource`);
- left/right story and structural anchors;
- bounded before/after summaries, with full content lazy;
- author/origin attribution when derivable from intervening commits;
- contributing commit range;
- confidence/completeness state;
- accessibility label;
- optional geometry resolved only for mounted preview pages.

### 11.4 Rendering

The diff overlay is a read-only render decoration keyed by sidecar anchors. It does not
modify normalized nodes, review revisions, source envelopes, or export. Virtualized pages
request only visible change geometry. The side panel owns the complete ordered list.

### 11.5 Attribution

When a durable commit log is present, attribution walks commits between left and right and
intersects their affected anchors with diff changes. When only snapshots exist, the service
may identify version-level actors but must mark per-change authorship `unknown` rather than
guess from colors or text.

### 11.6 Compare document generation

The later **Create comparison document** command compiles a supported `VersionDiff` into a
new document with tracked revision markup. Neither source changes. Unsupported diff records
remain findings attached to the new document/session; they are not dropped. This is separate
from ordinary preview and does not block VH-3.

## 12. Events and SDK surface

Target read APIs:

- `history_capabilities()`;
- `list_versions(cursor, limit, filter)`;
- `get_version_metadata(version_id)`;
- `open_version_preview(version_id)`;
- `diff_versions(left, right, profile)`;
- `list_diff_changes(diff_id, cursor, limit)`;
- `history_storage_status()`.

Target commands:

- `create_version(expected_head, kind)`;
- `name_version(version_id, name)`;
- `pin_version(version_id, pinned)`;
- `restore_version(expected_head, version_id, idempotency_key)`;
- `copy_version(version_id, options)`;
- `export_version(version_id, format, options)`;
- `delete_history(scope, confirmation)`.

Target events:

- `HistoryChanged` with metadata invalidation only;
- `HistoryPersistenceDegraded`;
- `VersionPreviewOpened/Closed`;
- `VersionDiffReady/Failed`;
- `RestorePrepared/Committed/Failed`;
- `HistoryCompacted` with counts/bytes, never content.

All lists are paginated and bounded. Events never carry whole document snapshots.

## 13. Retention and compaction

Retention works from a byte budget, a version count and an age window — **three bounds,
two kinds of rule**. This sentence previously read "a byte budget plus policy, *not* a raw
version count"; the owner's ruling on 2026-09-27 ("for version around 20-30 or retain for
7 days") overrides that, and ADR-038 records how the three are reconciled: the **count and
the byte budget are ceilings** that always apply and release the oldest eligible version
first, while the **age window is a floor on retention** rather than a deletion deadline — the
newest `keepFloor` versions survive it whatever their age, because a document nobody touched
for a week must not lose its entire past.

Never automatically prune:

- current head and its reconstructable base;
- named/pinned versions;
- pre/post checkpoints of an active or recent restore safety window;
- artifacts referenced by active previews, drafts, branches, or prepared operations;
- minimum recent recovery window required by host policy.

Eligible automatic versions may be thinned by age while preserving daily/weekly
checkpoints according to an explicit, versioned host policy. Commit tails compact into a
new validated checkpoint. A named version pins the checkpoint needed to reconstruct its
exact state.

Content-addressed deduplication may share identical immutable blobs, but authorization and
purge operate by lineage references. A blob is deleted only at reference count zero after a
transactional reachability scan.

## 14. Security and privacy model

- Historical data is a separate capability from current-document read access.
- Every command rechecks authorization in the service, not only in UI visibility.
- Version names, actor names, metadata, and imported manifests are untrusted strings.
- Checkpoint bytes pass the same hostile-format admission limits as ordinary open.
- Decompression has declared ratio/output/time bounds.
- No historical external hyperlink or macro executes during preview/diff/restore.
- Preview uses a capability-restricted read-only session.
- Local history never syncs or uploads without host/user policy.
- History deletion and restoration receive audit events without logging document content.
- Encryption at rest and key lifecycle are host policy; OpenDoc reports the provider's
  declared protection and does not imply IndexedDB is encrypted by OpenDoc.
- Checksums detect accidental corruption, not malicious host tampering. A future signed
  audit profile requires a separate ADR.

## 15. Determinism and compatibility

For fixed artifacts, engine version, diff profile, and policy:

- version grouping is deterministic;
- checkpoint hashes and manifests are deterministic except explicitly random IDs;
- replay produces the same validated model;
- diff records, ordering, and completeness findings are deterministic;
- restore produces the same content and compatibility report;
- time/localization affects labels only, never identity or ordering.

Persisted operation envelopes and semantic projections are versioned compatibility
surfaces. A new engine either migrates/replays an older supported version or refuses it
with an actionable reason. It never partially replays unknown operations.

Source-format checkpoint bytes remain the compatibility fallback when an old operation
log cannot replay, provided the current importer safely admits them.

## 16. Resource bounds

Doc 21's approach applies to history:

- maximum lineages, branches, versions, named versions, commits per replay, and pins;
- metadata page size and maximum name length;
- artifact byte size and total byte budget;
- decompressed output and compression ratio;
- diff nodes, changes, text bytes, table cells, recursion depth, and work time;
- concurrent previews/diffs/checkpoints;
- maximum replay tail after compaction;
- cancellation deadline and cleanup bound.

Far-over-limit input is rejected before allocation where possible. Hosts may choose lower
limits, never values above engine hard ceilings.

## 17. Delivery architecture

### H0 — Contract and truth correction

- accept docs 139/140 and reconcile ADR-033;
- define lineage/version/checkpoint/capability schemas;
- correct the old HF-068 normalized-JSON restore proposal;
- add CI placeholders and support-matrix truth.

### H1 — Snapshot history without false attribution

- extend the doc 112 store through a schema migration; **landed** (schema v3, additive);
- source-format checkpoint manifest and validation; **landed** — content-addressed artifact
  plus format/mode/findings/engine/revision metadata, verified on read;
- metadata timeline, naming/pinning, preview, copy/download, retention; **timeline, naming,
  pinning and retention landed. Preview, copy and download are NOT built** — they need the
  editor session, which this lane deliberately did not touch;
- version-level actor only; no per-change author claim; **held to**;
- no mutation required except metadata; **held to**.

### H2 — Atomic restore

- restore coordinator and prepared record; **landed**;
- compare-and-set head, pre-restore checkpoint, idempotency; **landed**;
- active-session atomic switch through the host/session command boundary; **NOT built** —
  `commitRestore` hands the validated bytes back and the session swap is the wiring lane's;
- one-step session Undo and after-reload reversal; **NOT built** (the pre-restore version is
  stored, which is what makes both possible later);
- crash/failure injection and fidelity corpus; **failure injection landed** in
  `webapp/tests/version_history.test.mjs` and
  `webapp/tests/e2e/version-history-store.spec.mjs`; the format-fidelity corpus is not run
  against checkpoints yet.

### H3 — Structural diff

- semantic projection, alignment, typed diff families, findings;
- Worker/background execution where possible;
- panel/overlay navigation and accessibility;
- current/previous/two-version comparison.

### H4 — Unified durable commits

- complete doc 107 P-1 through P-4;
- persist versioned transaction envelopes and affected anchors;
- exact per-change authorship, Show editors, durable Undo policy;
- commit-log compaction into the existing checkpoint format.

### H5 — Collaboration and comparison documents

- explicit branches and remote attribution;
- OT replay/merge after ADR-033 acceptance;
- separate tracked comparison document;
- host relay/storage adapter conformance.

## 18. Verification architecture

### Model and transaction

- every operation round-trips through forward/inverse and replay;
- failed apply/Undo/Redo/restore consumes no history;
- restore increments logical history and preserves the previous head;
- action selection/edit-context restoration tests;
- operation-envelope compatibility and unknown-version refusal.

### Checkpoint and fidelity

- DOCX/ODT/RTF/TXT/JSON checkpoint -> restore -> save -> reopen fixed points;
- media, source-envelope, unknown-part, relationship, macro-policy, comments, review,
  fields, drawings, notes, headers/footers, and metadata preservation;
- malformed, encrypted, oversized, truncated, missing-resource, and zip-bomb artifacts;
- target artifact validated before head mutation;
- checksums and corrupt-version isolation.

### Restore failure matrix

Inject failure before/after every state-machine transition, including quota, transaction
abort, tab kill, stale head, permission revocation, parser failure, resource failure, and
activation failure. Reopen must find either the complete old or complete restored head.

### Diff

- golden semantic diffs for every declared family;
- stable-ID and no-stable-ID alignment corpora;
- grapheme, bidi, CJK/Indic, combining-mark, and emoji text;
- table merges, moved blocks, style inheritance, review markup, and resources;
- deterministic ordering; cancellation; explicit unsupported findings;
- mutation tests proving no family can disappear silently.

### Browser/storage

- schema upgrade from doc 112 v1 without draft loss;
- metadata-only panel reads;
- cross-tab writer collision and branch behavior;
- quota and eviction disclosure;
- offline/network-interception proof;
- corruption of one row does not hide others;
- byte-budget compaction preserves pins and restore safety points.

### UX/accessibility

- keyboard/screen-reader complete journey;
- focus return and preview read-only enforcement;
- narrow viewport and touch;
- locale/time-zone/DST ordering and exact timestamp;
- capability matrix below UI and through the host bridge.

### Performance

- keystroke overhead remains O(1) and below noise threshold;
- panel metadata load, checkpoint, preview open, restore, replay, and diff benchmarks;
- cold and warm browser profiles;
- large/media-heavy/pathological documents;
- peak JS/WASM/Worker/storage memory and cancellation latency.

## 19. Rejected alternatives

### Normalized JSON as the restore source

Rejected by measured evidence in doc 112: binary resources and retained source envelope
are omitted. It remains useful as an optional semantic diff projection.

### Snapshot on every edit

Rejected because CPU, memory, and storage scale with document size on the interaction path.

### Restore by deleting newer history or moving the head backward

Rejected because it destroys auditability and makes a mistaken restore irreversible.

### Diff by serialized JSON or rendered pixels

Rejected because JSON noise does not express document semantics and pixel diff cannot
distinguish content, formatting, layout, or unsupported structures.

### Store history inside DOCX by default

Rejected because it changes interoperability, privacy, file size, and OOXML fidelity. A
future explicit OpenDoc archive is a separate decision.

### Browser-only history implementation

Rejected because it would create a second mutation/identity model unavailable to native,
headless, embedded, or collaborative hosts.

### Use tracked changes as version history

Rejected because suggestions are document content/review state. Accepting a suggestion
must not delete the historical evidence that the transaction occurred, and ordinary edits
may have no review markup.

## 20. Open architecture questions

1. Exact persisted operation-envelope schema and compatibility lifetime.
2. Whether `HistoryStore` belongs in the stable SDK before or after transaction unification.
3. How the active session adopts a restored logical revision without exposing a snapshot
   replacement backdoor.
4. Whether H2's session switch is sufficient or Restore must be a Tier-3 operation from its
   first implementation.
5. ~~Browser byte budget and retention defaults per device class.~~ **Settled by ADR-038**:
   25 versions / 7 days / floor 3 / 120 MB / 15 named, configurable and clamped. No device
   class split; the byte budget is the device-sensitive bound and a host can lower it.
6. Whether DOCX/ODT ZIP checkpoints should ever receive outer compression.
7. Canonical semantic projection format before CBOR is implemented.
8. Structural matching algorithm and thresholds for moves/renames without stable IDs.
9. Which changes are attributable after commit compaction and how incompleteness is shown.
10. Cross-device lineage and actor identity without a mandatory account/server.
11. Whether history archives need encryption/signing independent of host storage.
12. How macro-bearing `.docm` checkpoints interact with the eventual macro policy.

