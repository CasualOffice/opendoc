# 152 — The collaboration protocol, the two session state machines, and the identity discipline

**Status:** Accepted for implementation (this design ships with the change it describes).
**Opened:** 2026-09-30.
**Decides:** ADR-047 — **the relay orders and fans out; it holds no document and runs no
transform.**
**Implements:** `107` 6.6's foundation, ADR-033.
**Relates to:** `147`/ADR-043 (the envelope), `150`/ADR-045 (the transform this drives),
`143` §5–§9 (the provider/host split this corrects in one place), `45` invariants I1–I4,
`20-ERROR-CODE-REGISTRY.md` (the `ODC-7xxx` family), `106` Phase 6.

**The sibling is the reference.** `opencalc` (`/services/opencalc`) shipped this layer:
`casual-calc-transaction::{protocol, session, wire}`, ADR-011/012/014/015/016/017, and a
relay under `server/` that nothing in `crates/` may depend on. Its shapes, its constants and
its recorded incidents are taken deliberately. §8 lists what a document forced differently
and why, and §3 records the one place its answer is wrong for us.

**Explicitly not in this document, and not in the change:** the byte codec, the relay
binary, presence and cursors, collaborative undo, and any change to `casual-doc-wasm`. §9
says why each is out and what it is waiting for.

**Two modes, not one mode with an optional extra.** §2a records the owner's decision of
2026-10-01: standalone editing needs no server and never will, and a **shared** document joins
a room from its first open even with one participant — "one doc, one room". Wherever
collaboration is called *optional* in these documents, it is optional in the standalone mode
and structural in the shared one.

---

## 1. The established pattern, named before any code

**Jupiter / Google Wave**: a server imposing a total order over operations, clients applying
optimistically and rebasing on arrival. Not a CRDT, not peer-to-peer. ADR-033 decided it,
`150` built the transform, and this document builds the two state machines around it.

The two sub-patterns, also named rather than invented:

| Pattern | Where it is used here |
| --- | --- |
| **Rollback / replay** (Jupiter's client, ONLYOFFICE's client, ProseMirror's `rebaseSteps`) | §5.3. A replica rolls its unacknowledged work back to the last ordered position, applies the arrival there, and replays its own work rebased. |
| **Cumulative in-order acknowledgement with a bounded window** (TCP, minus everything else) | §5.4. `Ack{through}` with `MAX_OUTSTANDING`, degrading to stop-and-wait. **No selective acknowledgement, deliberately** — operation 2 has no meaning without operation 1, because it was written in coordinates that assume it. |

And the borrowed distinction worth keeping in the doc, because it is the one people get
wrong: *TCP acknowledges bytes having arrived; this acknowledges operations having been
ordered against everyone else's. Pipelining therefore hides latency; it does not reduce
work.*

---

## 2. Why the protocol lives in the engine crate

`casual-doc-transaction::{protocol, session, wire}` — pure data and two pure state machines,
inside the crate that already holds the envelope and the transform. **No separate client
crate, no transport, no clock, no I/O, and both ends compile from it** (the browser is this
crate through WASM).

The previous version of this lane proposed a new `casual-doc-collab` holding protocol types
and the session, with the rebase driver left in `casual-doc-transaction`. That was wrong, and
reading the sibling is what showed it: the driver needs the log, the document and `transform`
in one place, and splitting the session from the driver means splitting one mechanism across
two crates so that neither is complete. The sibling has one crate and no tension. So do we.

The relay, when it exists, is a workspace member under `server/`, and **nothing under
`crates/` may depend on it**. That is the sibling's CI rule and it is the structural half of
"no mandatory server" — the other half being that `casual-doc-wasm` does not reach these
modules at all, which `the_live_editor_has_no_collaboration_dependency` now fails the build
over.

---

## 2a. Two modes, and why "optional" was the wrong word

Owner decision, 2026-10-01. This closes a hole in what the earlier wording *claimed*, not in
what is built — §4.4's identity partition already had the right shape, and §7's new lifecycle
guard is what proves it rather than asserting it.

### The hole

The design said collaboration is "additive and optional" and that no server is required. The
owner asked the question that breaks that phrasing: **if a replica is not connected, how does
it ever learn that a second person started editing?**

It cannot, and no mechanism could give it that. Presence requires a connection by construction.
If a room came into being when the *second* participant appeared, the transition would be
unobservable from the first participant's side: they hold no socket, so nothing can reach them.
"Optional collaboration" therefore described a lifecycle that cannot work — a shared document
sitting outside a room, discovering company somehow.

### The decision: two modes, not one mode with an extra

| | **1 — standalone / serverless** | **2 — deployed, embedded, or shared: one doc, one room** |
| --- | --- | --- |
| How it is reached | Open a file | A link, or a host embedding the document |
| Room | None | **From the first open, even with one participant** |
| Presence, sharing, co-editing | None | Available |
| Server | **Never required** — the local-first guarantee stays absolute | The relay is part of what a shared document *is* |
| Minting space | `IdSpace::local` — reserved, no participant number can be handed it | `IdSpace::participant(base, number)` |

The owner's framing: *"one user doesn't connect to server is the case of just editing and saving
one user's edit. And one creates a document and shares a link, creates a room — but for real
editing even a single user gets a room. It's one doc, one room."*

So "optional" is true of **mode 1 only**, and every place the documents say collaboration is
optional now has to say which mode it means. A room is not an upgrade a document acquires when
a second person arrives; it is where a shared document lives from the first open.

### What this does not change, verified rather than assumed

The identity partition already drew exactly this line, and it was checked in the code before
this section was written:

- `open_document` builds its generator as `IdGenerator::new(IdSpace::local(IdSpace::of_document(id)).get())` — **mode 1 mints in the offline space**;
- `adopt_participant_identity(number)` rebases it to `IdSpace::participant(base, number)`, keeping the counter — **mode 2, and that call is the mode-1-to-mode-2 transition**;
- `IdSpace::participant` can never return the offline space (the participant number that would alias it is refused), so a participant can never mint over an offline replica's ids.

So this is a **lifecycle and wording change, not a rework**.

### What it opens, and the answers

**Who creates the room.** The **host**, at embed or share time, and not the first client. A
client cannot: `protocol::Join` carries an opaque `Identity` and the relay assigns the
participant number, so a client has no way to name a room it has not been told about, and
letting it invent one would make the room id a client-controlled value — which §10 Q4's
host-signed grant exists to prevent. `143` §5 already puts access with the host; this makes
creation explicit rather than implied.

**What a room costs a lone writer.** Measured, in the terms `107` §4 uses:

| Cost | A room with one participant |
| --- | --- |
| Transforms per keystroke | **0.** `transform` is never called with nothing concurrent — §5.3's uncontended path goes straight down `RevisionLog::apply`, the same call a keystroke makes |
| Document clones per keystroke | **0.** The one clone is on the *contended* path — a remote edit arriving while this replica has unacknowledged work |
| Round trips before an edit is visible locally | **0.** Editing is optimistic; the relay orders, it does not admit |
| Extra bytes per operation | One `Mint` (ADR-048) and the ids the operation declares, both charged in `carried_bytes` |
| Startup | One `Join`, one `Welcome`, and `ClientSession::joined` settling the log — no document walk |

`a_replica_with_nothing_pending_does_not_roll_back` is the guard that keeps the first two at
zero, and it was driven red. **The common case does not pay for the rare one**, which is the
condition the owner attached to this decision.

**A standalone document that is later shared.** Its work travels as the **snapshot the room is
created from**, never as operations, and this is forced rather than chosen: an operation
introducing an offline-space id is refused by every receiver
(`an_identity_minted_in_the_offline_space_is_refused_from_a_session`), so if a join could flush
pre-join work the two rules would contradict each other and a shared document's first exchange
would fail.

They do not contradict, and the whole lifecycle answer rests on one line:
`ClientSession::joined` calls `log.settle(log.head())`, and `flush`'s floor is
`max(flushed, horizon)`. Every pre-join commit is therefore below the horizon and is never
offered. The pre-join commits **stay in the log**, so the reader does not lose undo history by
sharing, and `IdGenerator::rebase` keeps the counter, so nothing minted offline is handed out
again under a participant number.
`work_done_before_a_room_existed_travels_as_the_snapshot_and_never_as_operations` asserts all
four of those, and goes red on removing that one `settle` call — with three pre-room commits
left above the horizon, which is exactly the shape that would have sent them.

**What is still open.** How the host is *told* a room exists — the grant of §10 Q4 — and what a
relay does with a room nobody is in. Neither is decided here, and neither blocks the wording.

---

## 3. The decision: a dumb relay, not a document replica

This is the one central open question this document exists to settle, and it is a real
decision rather than a copy, because of what our `transform` needs.

### 3.1 What makes it a decision

`transform(subject, against, side)` takes `against` as a **`Change`** — the operation *and
the inverse `apply` returned for it* (`150` §2.3). Our deletes name their victims by
identity rather than by address, so the inverse is the side table that says what a concurrent
change destroyed, and **only `apply` produces one**.

So a relay that transforms must, for every operation it orders, possess that operation's
inverse. It cannot compute one from the operation alone. It must therefore **apply** it, to a
real `v1::Document`, which means the relay is a full document replica running the whole
engine.

The sibling's relay is not in that position: a spreadsheet operation is self-describing, so
its `ServerSession::commit` transforms with only a sheet-name table and a formula table, and
holding the model is a convenience ("durable enough to survive a hibernation, not durable
enough to be a system of record") rather than a requirement. **Copying its shape would
therefore cost us far more than it costs them**, and it would contradict `143` §6, which says
the provider owns the transform **never** and "is not a DOCX server… does not become the
authoritative live document model".

### 3.2 The two options, stated plainly

| | **A — transforming replica relay** (the sibling's shape) | **B — dumb relay, accept only at head** (recommended) |
| --- | --- | --- |
| What the relay holds | The document, the log, the snapshots; the whole engine linked in | The order, a dedupe table, a retained tail of operations it never reads into |
| A stale submission | Rebased server-side and accepted | Refused with `StaleBase{current}`; the client rebases and resubmits with the same `seq` |
| Round trips under contention | One, always | One when the client has already received what it collided with; two when it has not |
| Starvation | Impossible | Possible under sustained contention — §3.4 |
| `143` §6 | **Contradicted**: the provider owns transform and is the live model | Upheld |
| `143` P3 (managed third-party provider) | **Impossible**: a third party would have to run our Rust engine | Possible: a relay only needs to order opaque envelopes |
| Cost per open room | The document, in memory, per replica, plus the engine's whole attack surface | A bounded tail of operations |
| Snapshot verification by replay | Available server-side — but blocked anyway by `150` §9.3 until minted ids are on the wire | Not available server-side |
| Reversibility | **One-way.** The engine is baked into the provider contract | **Two-way.** A deployment may link this crate and transform server-side; clients cannot tell, except that they are refused less |

### 3.3 Recommendation: B, and why

1. **The client must do this rebase anyway.** Every replica already rolls back and replays
   for arrivals. A stale-base refusal is that same code path plus a resubmit — *one*
   mechanism, not two, which is the rule that has repeatedly been right here (`SKILL` §8).
   Option A adds a second implementation of rebasing, on a machine that does not hold the
   undo log, and two implementations of one rule diverge.
2. **Refusal is not loss.** The operations are still in the client's log. It resubmits with
   the same `(client, seq)`, so the relay's dedupe makes a duplicate harmless.
3. **The licence is the wedge and embeddability is the product** (`SKILL` §1). A relay that
   parses documents and holds the live model is *ONLYOFFICE's shape* — the thing this project
   exists to be an alternative to. Keeping the relay substitutable is the same argument as
   keeping the engine local.
4. **It is the reversible choice.** Nothing in the protocol forbids a provider that
   transforms; §3.2's last row is the decisive asymmetry.

### 3.4 What choosing B costs, written down because it is real

- **Ping-pong.** Two people typing in one paragraph can collide on most keystrokes. Each
  collision costs the refused client one extra round trip.
- **Starvation is possible, and bounded only by behaviour, not by construction.** A client is
  refused whenever the head moved between its flush and the relay reading it. Progress is
  guaranteed while each retry is against a strictly newer head — which holds, because the
  arrivals that caused the refusal reach the client on the same ordered connection, and
  `ClientSession::flush` **will not resubmit until this replica's own position has reached the
  one the refusal named**. That is a state guarantee, not an assumption about message
  ordering. What is *not* guaranteed is that the head ever stops moving. Under sustained
  many-writer contention a slow client can be refused indefinitely.
- **No server-side snapshot verification.** The sibling verifies a snapshot by replay and
  byte comparison, and calls it a strong integrity check on its own state. We give that up
  for now — though `150` §9.3 blocks it regardless until operations carry the identities they
  cause to be minted.

**What has to be measured before this is believed** (and it is measurement, not design):
commit latency and the *refusal rate* as concurrent writers on one document rise. Tens
should be uneventful. That is the same gate `143` §16 and the sibling's `docs/57` set, and
nothing here should be called proven until it is run.

### 3.5 What would supersede this

Exactly one thing: a measured refusal rate that makes ordinary co-authoring feel broken. The
migration is then additive — a provider links this crate and transforms — and needs no
protocol bump, because a client that is refused less is a client that behaves the same.

---

## 4. Identity: the highest-value finding, and it is in none of our other docs

The sibling's ADR-025 is *"an interned id is replica-local; the value crosses the wire, the
id never does."* They found it as a production defect **three separate times**. Reading it
sent us looking for the analogue here. What we found is not the same defect, and it is worse.

### 4.1 What was measured

1. **Our operations already carry their values.** `SetStyleDefinition` carries the whole
   `Style` beside the `StyleId`; `CreateBookmark` carries the name; `InsertNote` carries the
   note's blocks; `InsertFieldRange` carries the definition. Across all 55 operations there
   is none that names a definition it neither carries nor that the total order guarantees
   already exists. **So there is no side table to add to the wire op**, and the handover that
   asked for one was working from the sibling's shape rather than from ours.
2. **`v1::intern` is not an id table.** It is `Shared<T>`, an `Arc` flyweight whose own doc
   comment says it "serializes exactly as `T` does". It puts no id on the wire at all.
3. **`StyleId`, `BookmarkId`, `FieldRangeId`, `NoteId`, `SectionId`, `HeaderFooterId`,
   `CommentId`, `MediaId` and the two numbering ids are all newtypes over `NodeId`**, which
   is `(namespace: u64, counter: u64)`.
4. **The live editor's namespace is derived from the document.** `casual-doc-wasm` computes
   `edit_namespace = (document.id() >> 64) ^ 0xED17_ED17_ED17_ED17` and starts its counter at
   one. Two replicas of one document therefore mint **identical ids for different nodes, from
   the first edit.**

So the hazard is not "an id means something else on the receiver". It is **two nodes with one
name**, and for `SetStyleDefinition` — whose `Some(style)` is "insert *or replace*" by design
— the consequence is a **silent overwrite of a definition the receiver minted**. Silent data
loss, from ordinary typing, on the first edit.

### 4.2 The rule

> **An introduction needs a private space; a reference needs the order.**

- *Introductions* — the ids an operation mints — must come from a space nobody else mints in.
  `IdSpace::participant(document_space, number)` derives one per participant and is
  **injective in the participant number**: `space(c) = base ^ (K · (c + 2))` with `K` odd, so
  `c ↦ K·(c+2)` is a bijection on `u64` and xor with a constant is a bijection. Not "unlikely
  to collide" — cannot.
- Two values are **reserved** and no participant number can be handed either: `base` itself,
  where the importer mints, and `base ^ K`, where a replica with **no session** mints
  (`IdSpace::local`, §4.4). Reaching them would need `c + 2 == 0` and `c + 2 == 1`; both
  overflow, and `IdSpace::participant` returns `None` for those two numbers rather than
  leaving it as a remark.
- It is **derived, not carried**: a receiver computes the sender's space from the arrival's
  `client` field, so there is no wire field to forge and no table to keep in step.
- **It lives in `casual-doc-model`, not in this crate.** Identity is a property of the model,
  and the live editor has to mint in a partitioned space whether or not it is in a session —
  so hosting the partition here would have made single-user editing depend on the
  collaboration modules, which `the_live_editor_has_no_collaboration_dependency` forbids.
  `wire` re-exports `IdSpace` and adds `wire::space_of`, the one-line adapter from this
  crate's `ClientId` to the model's derivation, so there is one derivation and not two.
- *References* — ids an operation names but did not mint — are safe because the session is
  **totally ordered** and everyone starts from one snapshot: the operation that created a
  definition is ordered before any operation that names it.

`wire::WireOperation::localise` is the choke point. It recomputes what the operation
introduces rather than trusting the declaration (a sender that under-declares would otherwise
skip both checks for the id it left out), then refuses `ForeignSpace` and `AlreadyHeld` with
`ODC-7008`.

**Why not re-map on receipt**, which is what the sibling does for an interned value: the id
*is* the identity here, so re-mapping would leave the two replicas disagreeing about the name
of the same logical node, and a snapshot could never be compared byte for byte across
replicas. `150` §9.3 already names that as the blocker for persisted collaboration; re-mapping
would make it permanent.

### 4.3 The standing rule for the next interned table

Any new definition table added to `v1::Definitions` must, **in the same change**:

1. have the operation that creates an entry **carry the value**, not just the key;
2. mint its key through `IdSpace::participant` (or `IdSpace::local` with no session), never through a document-derived one;
3. add its variant to `wire::WireOperation::introduces` — an exhaustive match with no wildcard
   arm, so this one is a compile error rather than a review comment;
4. add its table to `wire::Table` and to `localise`'s collision check;
5. arrive with **a test in which the receiver already holds a *different* entry at that id.**
   An id that lines up by accident proves nothing, and a test that merely round-trips a value
   proves less.

### 4.4 The live editor mints in the partition — closed 2026-10-01

The previous revision of this section recorded the live editor as unfixed: it derived its
minting namespace from the document — `(document.id() >> 64) ^ 0xED17_ED17_ED17_ED17` — so two
replicas minted identical ids for different nodes from the first edit, and a session refused
every arrival that introduced one with `ODC-7008`. That refusal was honest and it meant
**collaboration could not run at all**. It is now closed.

**The established pattern, named before the code.** This is a namespace-partitioning problem
and the prior art is **participant-prefixed identifiers** — the *site id* of Jupiter/Wave OT,
and of every CRDT that mints identity (Yjs's `(client, clock)`, Automerge's
`(actor, counter)`). A `NodeId` is already `(namespace, counter)`, so the partition costs no
new field and no new type. The alternative prior art, a **minted-range allocator** where a
coordinator hands each replica a block of ids to spend, is rejected for one reason: a replica
with no block cannot mint, so the first keystroke of a local-first document would have to wait
for a server. Site-id partitioning needs no round trip.

**Three spaces, provably disjoint.** With `base` the document's own space and `K` odd:

| Space | Value | Who mints in it | When |
| --- | --- | --- | --- |
| document | `base` | the importer | once, at open |
| offline | `base ^ K` | a replica with **no session** | every keystroke until a room is joined |
| participant `c` | `base ^ (K · (c + 2))` | one session participant | after `adoptParticipantIdentity(c)` |

**Which id families this covers, enumerated from the code rather than from a list.** Every one
of `StyleId`, `AbstractNumberingId`, `NumberingInstanceId`, `MediaId`, `SectionId`, `NoteId`,
`HeaderFooterId`, `CommentId`, `BookmarkId`, `FieldRangeId` and the bare `NodeId`s of
paragraphs, runs, tables, rows, cells, drawings, groups, text boxes, fields and content
controls is minted in `casual-doc-wasm` through **one allocator**, `WasmDocument::edit_ids`
(109 call sites, no second source: the only other `NodeId::from_parts` calls in the editing
crates are inside `#[cfg(test)]` fixtures). `casual-doc-edit`'s `RunIds` trait is implemented
for `IdGenerator`, so the run ids `apply` mints come from the same allocator. **One choke
point, so the partition is one change at one place** — which is why this was a small change
and not a sweep.

**Where the participant number comes from, and what happens before a session exists.** The
relay assigns it in `Welcome`; the host passes it to the engine through
`WasmDocument::adoptParticipantIdentity(number)`. Before that call — and for a document that
never joins anything — the editor mints in the **offline** space, which no participant number
can be handed. So a single-user document works with no session, no server and no round trip,
exactly as before, and minting stays one increment of a `u64`: **O(1) per keystroke**
(`107` §4 B1). `a_document_with_no_session_still_mints_and_does_so_outside_every_participant_space`
is what proves the local-first half rather than assuming it.

Every offline replica of one document uses the *same* offline space, which is sound rather
than lax: the protocol's precondition is that participants start from one snapshot, and an
offline replica's own unshared edits never travel (joining settles the log at its head, §5.3).
A per-replica random offline space would have been the Yjs/Automerge answer and would have
traded a *provable* disjointness for a 2⁻⁶⁴ one; it is not needed here.

**Backward compatibility, and the one format where it bites.** DOCX, ODT and RTF re-mint every
id on import (`IdGenerator::new(config.id_namespace)`), so a document saved and reopened
through them carries no old ids at all. A **normalized JSON snapshot preserves node ids
verbatim** — so a document saved after editing comes back carrying the previous session's
mints, and an allocator restarting its counter at one would hand them out again. That was
already true before this change and is now fixed: `Document::highest_counter_in(space)` (one
O(document) walk, at open and at join, never per keystroke) seeds the allocator above whatever
the document already holds in the space it is about to mint in, and `IdGenerator::rebase`
carries the counter across a space change so a replica **rejoining under a number it held
before** cannot reissue. Ids already minted are never re-mapped: the id *is* the identity, so
rewriting one would leave two replicas disagreeing about the name of a node for ever.

That walk is the model's own — `validate_unique_ids` and `highest_counter_in` are two readings
of one `visit_node_ids`, so a node kind added to the model is enumerated for both or for
neither. Writing it exposed a gap: **`FieldRangeId` was in no walk at all**, so a duplicate
field-range id was invisible to `validate`. It is enumerated now.

**The wire consequence: none.** The sibling's hardest-won rule is *an interned id is
replica-local — the value crosses the wire, the id never does*, and this change was checked
against it. The space is **derived** from the `client` field a message already carries, so no
field was added, no field was removed, and no payload changed shape. What *did* change is the
derivation itself, and that is exactly why `PROTOCOL_VERSION` goes **1 → 2**: a version-1 peer
and a version-2 peer compute different spaces for the same participant number and would refuse
each other's every introduction with `ODC-7008` while both believed the message well formed. A
silent disagreement about a derived value is the case the equality check exists for.

---

## 5. The protocol and the two state machines

### 5.1 The version rule

`PROTOCOL_VERSION` is compared for **equality before anything else happens**, on both ends. A
mismatch is `Stopped`, never `Refused`: a client that treats a version mismatch as retryable
loops for ever. `Refusal::is_retryable` and `Refusal::is_terminal` say which is which, in code
rather than in prose.

Bump it only when an old and a new peer would read **the same message differently**. An added
optional field is not a bump; a **new enum variant is a hard break**, because a tagged enum
with an unknown tag does not deserialize at all. The sibling names the two failure shapes:
the *loud* break (new variant, refused) and the *quiet* break (a whole-vector-replace field
an old peer writes back without, silently reverting everyone's data). Their wire types carry
no `deny_unknown_fields` — the opposite of our `v1` model convention, and deliberately so; the
codec lane inherits that decision along with the rest.

### 5.2 Two revisions, kept apart

`protocol::Revision` is the relay's ordered position. `RevisionId` is the local log's
revision. **They are different types and count different things**: a remote chunk of five
operations becomes one local commit and one local revision while the relay has ordered five.
Conflating them is how a client concludes it is up to date while five operations are still
owed to it.

A client names an absolute `Revision` in exactly one place — `Base::Revision`, for the first
chunk after a join or a resume, which is the only moment it knows an absolute answer. Every
chunk written on top of its own previous one says `Base::Chained`, and **the relay resolves
that from the same table it uses to suppress duplicates**. A `Chained` chunk from a client
with nothing accepted is **refused rather than guessed at**: it cannot happen from a correct
client, and inventing a base is how divergence starts.

### 5.3 The horizon, and the rollback/replay driver

`147` gave the log one position, `head`, because a single-user document has one: every commit
is final the moment it is applied. A collaborative one has two, so `RevisionLog` grows a
**horizon** — the revision up to which commits have been *ordered*.

That is what makes rollback/replay compatible with "nothing rewrites a commit": the driver
rewrites only commits **above** the horizon, which are by construction this replica's own
unacknowledged work, seen by nobody. A rewritten commit keeps its `TransactionId`, `GroupId`,
`Label` and `Origin`, so **undo is untouched by a rebase** — the user's steps are the same
steps, expressed against a document that has moved. `a_rebase_keeps_the_user_s_own_undo_steps`
pins it.

**A replica with nothing pending does not roll back at all.** The arrival goes through
`RevisionLog::apply` — the same call a keystroke makes — with no rollback, no transform, no
placement and no working copy. That is `107` B6 for free, and it is the common case: a reader,
or anyone whose edits have been acknowledged.

**The probe, and why it is needed.** `transform` reads `against`'s inverse. A peer receives
operations, never inverses — a forged inverse would be a way to diverge the replicas quietly —
so the inverse is produced locally by `apply`. Rollback gives it for the *first* unordered
commit. For the second, the arrival has to be seen as it would be after the first, and its
inverse there is the composition `p₁'⁻¹ ∘ R⁻¹ ∘ p₁` — three operations, where `Change` carries
one. **This is the sharpest thing the handover got half right**: rollback/replay does not
remove the inverse problem for a *sequence*, it removes it for one step.

So the driver does not compose. It runs in three phases:

1. **Pure.** The arrival's image at each unordered commit's own base, computed *forwards* from
   the inverses the commits already recorded, touching the document not at all.
2. **Rollback, probing.** Rolling commit *i* back arrives at commit *i*'s base, which is
   exactly where image *i*'s inverse has to be taken — so the probe happens on the way past:
   apply the image, keep the inverses, apply them straight back. Two O(edit) applications and
   no document copy.
3. **Replay.** The arrival lands at the horizon through `RevisionLog::apply`; each unordered
   commit is rebased over the image at its own base and re-appended with its own identity.

The probe rests on **one** invariant, and it is one undo already rests on: applying an
operation's inverse restores the state it was applied to (ADR-030 I2). If that is false, undo
is broken too, and `a_probe_leaves_the_document_exactly_as_it_found_it` is what says so.

**`150` §10 Q1 is closed by this, not worked around.** That question was how a session
supplies a `BlockPlacement` resolved against the base state when "neither replica holds that
state at the moment it transforms". A rollback driver holds it: rolling back to the horizon
*is* arriving at the base. `BlockIndex` is built there and nowhere else, so the precondition is
met by construction rather than by a caller's promise.

**`150` §10 Q2 is not an open question either — it is a blocker.** A
`Coalesce::ContinueKeepingFirstInverse` commit deliberately records no inverse, to stop one
word of suggested typing retaining one whole-paragraph snapshot per character. Such a commit
can neither serve as a concurrent change **nor be rolled back**, so **suggesting mode cannot
take part in a session** until `147`'s envelope retains inverses and drops them at undo-read
time instead. The session refuses with `SessionError::NotRollbackable` → `ODC-7001` rather than
diverging. `150` §6 and §10 are corrected in place.

### 5.4 Pipelining, acknowledgement and the budget

- `MAX_OUTSTANDING = 32`; at the bound `flush` returns `None` and edits keep accumulating in
  the log. **It degrades to stop-and-wait, which is a good thing to degrade to**, being what
  an unpipelined client already was.
- `Ack{through, revision}` is **cumulative**, and `revision` is where *that chunk* landed,
  **never head**. Naming a later revision would have the client conclude it holds everything
  in between, which it would then never be sent. A lost or skipped acknowledgement is
  self-healing.
- **The log is the pending queue.** `ClientSession` stores no operations of its own — it holds
  marks into the log. A second store beside the log is the defect `147` existed to remove for
  undo, and it would be the same defect here.
- The byte budget spends `CHUNK_BUDGET_BYTES` in commit order and **always admits at least one
  commit**, even one that alone exceeds it, or `flush` would spin on a submission it can never
  make. The measure is `WireOperation::carried_bytes`, a deliberately shallow lower bound: the
  budget exists to pack a *backlog*, not to enforce a frame cap, so its precision changes how
  well several commits are packed and never whether an over-cap commit is sent. Enforcing a
  transport's frame cap is the codec lane's, with the cap **stated once** so both ends read the
  same number — an over-cap frame does not come back refused, it closes the connection.
- **A replay re-anchors every mark that names a local revision.** Leaving them behind would let
  the next flush offer an operation that is already in flight under a different sequence
  number, which the relay's `(client, seq)` dedupe cannot recognise — so the edit applies
  twice. Silent, and arrived at by arithmetic.
  `a_rebase_re_anchors_the_marks_that_say_what_is_already_in_flight` pins it.

### 5.5 Resume

One function decides replay versus snapshot: `ServerSession::history_since(revision)`, `None`
exactly when the revision is outside `[oldest_rebasable, head]`.

- `Some` → `Resumed{missed}`, with **no snapshot** (replacing the document would discard the
  unacknowledged work this message exists to preserve) and the **same** `ClientId`. `missed`
  travels **inside** the message so the client cannot resend before rebasing past it.
- `None` → `Refused{TooFarBehind{oldest, current}}` **first**, then `Welcome` and a snapshot.
  The loss is announced *before* the thing that discards it lands.

A resume is honoured only when the key is known **and** the identity it was issued to is the
one presenting it. That reduces a key from something that authorises to a **disambiguator**:
without the scoping, anyone with a valid session could adopt another participant's `ClientId`
and have that participant's submissions suppressed as duplicates. The key is remembered on
**every** join that offers one, resumed or not, because the point of a key is the *next*
reconnect. A resumed session **keeps** its chunk counter; restarting it would let a new chunk
collide with an old number and be discarded as a duplicate — silently.

`DEFAULT_RETAINED_REVISIONS = 400` (the sibling's `every × retain_intervals`). **That number
is the definition of "bounded offline"**, and a client away longer loses the work it had not
had acknowledged — announced, per `ODC-7006`, never silent.

### 5.6 Refusals are three different user actions

`Malformed` means *do not send that again*. `CannotMerge` means *that one action did not
take*. `NotSaving` means *the document is not being saved, copy your work out*. The sibling
answered an unparseable message with `CannotMerge` — naming the transform, the one part that
was working — and lost a live debugging session to it.

`docs/20` is extended with `ODC-7002`…`ODC-7009`; `ODC-7001 collaboration_conflict` already
existed and is exactly `CannotMerge`, so it is reused rather than duplicated.
`the_three_refusals_that_ask_for_opposite_things_are_three_codes` asserts the distinction
rather than leaving it to the reader.

---

## 6. Complexity

| Path | Cost |
| --- | --- |
| A single-user keystroke | **Unchanged, and provably so.** `the_keystroke_path_runs_no_transform` and `the_live_editor_has_no_collaboration_dependency` fail the build if that stops being true. |
| An arrival with nothing pending | One `RevisionLog::apply`. No transform, no placement, no copy. |
| An arrival with *n* unordered commits | *n* probes (2 × O(edit) each), *n* rollbacks, *n* replays, O(n × remote ops) transforms, **one document clone**, and **one O(document) `BlockIndex` build**. |
| `ServerSession::commit` | O(1) plus one clone of the submitted operations into the retained tail. |

The two O(document) terms are both on the **contended** path — a remote edit arriving while
this replica has unacknowledged work — and never on a keystroke or an uncontended arrival. The
document clone is what makes "on `Err` the document is exactly what it was" true here as it is
of `RevisionLog::apply`; a rollback that fails half way through has damaged the document and no
operation repairs it. Paying less is §10 Q1.

---

## 7. How it is verified

<!-- session-suite-count: 37 -->
**37 tests** over the state machines, driving **two replicas and a relay in one process**. The
number is **derived, not maintained**: `the_session_suite_count_in_the_design_doc_is_derived`
counts the suite and fails if this line disagrees, because a hand-kept count in a published
document has twice drifted into a false public claim here (`104` read 114/47 against an actual
146/54). The
sibling's recorded lesson about where its own collaboration bugs were is *"both sides were
individually correct and no test put them in a room together"* — a WASM binding that sent a
bare submission instead of a tagged message, and integer-keyed maps that were undeliverable,
both invisible to every test that *constructed* a message instead of parsing one.

**Every load-bearing guard was driven red before it was trusted** (`SKILL` §4):

| Mutation | What went red |
| --- | --- |
| Drop the already-held collision check in `wire::localise` | `an_arriving_definition_at_an_id_this_replica_already_holds_is_refused` — the arrival was **accepted**, silently overwriting the receiver's own style |
| Remove the uncontended fast path in `ClientSession::receive` | `a_replica_with_nothing_pending_does_not_roll_back` — and the arrival was **dropped**, which is what added the empty-arrival guard |
| Make `Coalesce::ContinueKeepingFirstInverse` keep its inverses | `an_unordered_commit_that_kept_no_inverse_cannot_be_rolled_back` — the rollback succeeded, so the guard is about the real cause |
| Stop the probe putting the document back | `a_probe_leaves_the_document_exactly_as_it_found_it` **and** `two_replicas_editing_one_paragraph_converge`, which diverged visibly: `abcdAGgefgh` against `abcdAGgAGefgh` — the arrival applied twice |
| Give `IdCollision` a code `docs/20` does not list | `every_refusal_code_has_a_row_in_the_register` — "*IdCollision sends ODC-7099, which docs/20 does not list*" |
| Add an `ODC-7010` row to `docs/20` that no variant carries | the same guard, the other way — "*docs/20 lists ODC-7010, which no refusal in this crate sends*" |

**The identity partition (§4.4), driven red the same way.** Four mutations, each of a
different production line, with the failure each produced:

| Mutation | What went red, and what it said |
| --- | --- |
| `adopt_participant_identity_internal` no longer calls `edit_ids.rebase` — the editor keeps minting where it was | `two_replicas_editing_one_document_never_mint_the_same_id`: "*two replicas minted **10** identical ids for different nodes*", listing all ten. Also `a_replica_does_not_hold_the_id_the_other_replica_introduced` ("*two replicas registered two different bookmarks under one id*"), and both rejoin guards |
| `IdSpace::participant` back to `checked_add(1)` — the offline space stops being reserved | `a_document_with_no_session_still_mints_and_does_so_outside_every_participant_space`: "*participant 0 would mint over an offline replica's ids*", and in the transaction crate `the_space_a_replica_mints_in_offline_is_no_participant_s_space` and `an_identity_minted_in_the_offline_space_is_refused_from_a_session` |
| `IdGenerator::rebase` resets the counter to one | `adopting_a_participant_number_never_reissues_an_id_already_minted`: "*b6f8da3d188d5c820000000000000001 was minted twice by one replica*" |
| The open path drops `reserve_through` | `a_document_reopened_from_a_snapshot_does_not_reissue_the_ids_it_already_holds`: "*the reopened allocator is at counter 1, at or below the highest (10) the document already holds in that space*" |

The third of those **passed on its first writing**, and the reason is worth recording because
it is the shape `SKILL` §4 warns about. The rejoin guard's replica still had its earlier ids
*in the document*, so `reserve_through`'s document scan — a second, weaker safety net — carried
the guard and the counter never had to. The guard was rewritten to **create the condition**:
each phase is undone, so the ids it minted are nowhere in the document, and the test asserts
`highest_counter_in(space) == 0` before relying on anything. Those ids are still live — an
undone commit's operations sit in the revision log holding them, and a redo or an
already-submitted chunk would put them back — so only the carried counter keeps them spent.

The convergence test is the one that matters most: two replicas typing into one paragraph at
the same offset, one of them twice, so the rebase is of a **sequence** and not of a single
operation — the case the probe exists for.

**What the register guard pairs, stated exactly, because it is narrower than it sounds.** It
pairs the `Refusal` *enum* with `docs/20` — every variant has a row, every `ODC-7xxx` row has
a variant — and it is written as an array holding one of each variant, so adding a variant
without a code fails to compile and adding one without a row fails the test. It does **not**
prove a variant is reachable. Two are not yet sent by anything in this crate:
`NotAuthorised` and `ReadOnlyAccess` are the authorisation family, and §9 says why no
host-signed grant is built here. They are wire surface a host fills in, and the register
describes them so a host can; that is a different claim from "this code is emitted", and
conflating the two is how a registry comes to document behaviour nothing performs.

**One defect was found by writing a guard rather than by mutating one, and it is worth
recording because it is a consequence of this design and not of a slip.** Because a remote
chunk becomes a commit in *this replica's own log* — which is the point of it going down
`RevisionLog::apply` — "everything in the log this client has not sent yet" is the wrong set
to flush: on the uncontended path it includes the arrival, which would be **echoed back under
this client's own sequence number and applied twice by everyone**. The floor for a flush is
therefore the **horizon as well as** the flush mark. `a_replica_never_sends_back_an_operation_it_received`
was written first and went red against exactly that:

> this replica offered somebody else's operation as its own work

The general lesson, for the next increment: the moment a remote edit shares one log with local
edits, *every* set the session computes from "what is in the log" has to say which side of the
horizon it means.

**What is not verified, and is not claimed.** There is no encoded round trip, because there is
no encoding. The class of defect the sibling found lives exactly there, which is why §10 makes
round-tripping through a real encoded string — with *populated* payloads, asserting literal
substrings — the codec lane's first obligation rather than an afterthought.

---

## 8. What was taken from the sibling, and what a document forced differently

**Taken directly:** the protocol in the engine crate with a CI rule that the core may not
depend on the relay; `PROTOCOL_VERSION` equality-checked first and a mismatch as a stop; the
bump rule and its two failure shapes; `Base::{Revision, Chained}` with the relay resolving
`Chained` and refusing it from a client with nothing accepted; `(client, seq)` idempotency and
`Duplicate` naming where a chunk landed the first time; cumulative `Ack{through, revision}`
with `revision` never being head; `MAX_OUTSTANDING` degrading to stop-and-wait; a byte budget
that always admits at least one item; `history_since` as the one function deciding replay
versus snapshot; `TooFarBehind` announced before the snapshot; a client-generated resume key
scoped to an identity; the one-way desync latch; and three words for three refusals.

**Forced differently by a document:**

1. **The relay holds no document and runs no transform** (§3), because our transform needs an
   inverse and theirs does not.
2. **Two revision types** (§5.2), because our envelope's unit of ordering is a commit and
   theirs is an operation.
3. **A horizon on the log** (§5.3), because our log is also the undo history and a rebase must
   not disturb it.
4. **The probe** (§5.3), because our `Change` carries one inverse and a sequence needs several.
5. **`IdSpace` instead of value tables on the wire** (§4), because our operations already carry
   their values and our hazard is at the mint.
6. **The log is the pending queue** (§5.4), because our envelope already stores the operations
   and a second store is the defect `147` removed.

---

## 9. Deliberately out of scope for this increment

Each is out for a reason, not for lack of time.

| Not built | Why, and what it waits for |
| --- | --- |
| **The byte codec** | `casual-doc-edit` has no `serde` at all. Of the three op-set findings that were about to move the shapes, **`150` §9.3 is now closed** — and it moved no `Operation` variant at all: the mint travels on the envelope (`Transaction`, and `WireOperation` on the wire), because the number of identities an operation mints is discovered at application time and cannot be enumerated at authoring time (`150` §9.4, ADR-048). What still moves the shapes is `150` §9.1 (node-addressed block operations) and §9.2 (`Pos` affinity). Freezing bytes over those is the one thing a compatibility surface must not do. `107` §8 Q5, `150` §10 Q5. |
| **The relay binary** | It is a workspace member under `server/`, not a crate, and it needs the codec and a transport first. The state machine it will drive is here and is testable without it, which is the point of a state machine over supplied bytes. |
| **Presence and cursors** | Yjs's awareness protocol, adopted not invented: one entry per client, overwritten wholesale, no merge therefore no transform, never persisted or replayed. It needs no transform and no order, so it is genuinely separable — and it needs the anchor mapping `107` P-4 owes before a remote cursor can survive a structural edit. |
| **Collaborative undo** | `150` §11 already records what the transform commits us to, and the sibling's `docs/69` is the reference. It is a **local** decision taken before submitting, needs no wire field and no protocol bump, and its primitive — `Rebase::Tombstoned` — already exists. |
| ~~**Any `casual-doc-wasm` change**~~ | **Done 2026-10-01** (§4.4). The editor mints through the model's `IdSpace`, not through the collaboration modules, so `the_live_editor_has_no_collaboration_dependency` still holds unchanged — which is the reason the partition was put in `casual-doc-model` rather than in `wire`. |

---

## 10. Open questions — recorded, not hidden

1. **The two O(document) terms on the contended path** (§6): the document clone and the
   `BlockIndex` build. A placement maintained incrementally alongside the log removes the
   second; a reversible application buffer would remove the first. Both are optimisations of a
   correct mechanism, and both should be measured before they are built.
2. **Identities inside a carried subtree.** `wire::WireOperation::introduces` enumerates the
   ids an operation names in its **own fields**. `InsertBlocks`, `InsertTable`, `SetInlines`,
   `ReplaceTable`, `InsertRow`, `InsertColumn` and the note/field/header payloads carry whole
   `BlockNode`/`InlineNode` trees whose nodes have ids of their own, and enumerating those means
   a recursive walk of 4 block and 28 inline variants — duplicating the model's structure in
   this crate. They are covered by the id-space rule at the mint and, if that discipline is
   broken, by `Document::validate`'s duplicate-node-id rule — which is O(document) and
   deliberately off the apply path (`147` §3.2), so such a collision **lands first** and is
   only caught at the next validation point. Closing this properly means the walk belonging to
   `casual-doc-model`, where the structure already lives.
3. ~~**The relay's refusal rate under contention**~~ — **run 2026-10-01.** §3.4 said nothing
   here should be called proven until this was measured, so it was.
   `the_dumb_relay_s_refusal_rate_is_the_ping_pong_and_nothing_worse` drives *W* replicas
   that all type before any of them sends — simultaneous offers against one head, which is
   the only arrangement that contends at all — and counts what the relay ordered against
   what it refused. Deterministic: a fixed round robin, no clock, no randomness. The table is
   printed by the test (`cargo test -p casual-doc-transaction -- --nocapture refusal`), not
   typed into this prose:

   | Writers | Ordered | Refused | Refusals per ordered chunk | Worst attempts for one chunk |
   | ---: | ---: | ---: | ---: | ---: |
   | 1 | 4 | 0 | 0.00 | 1 |
   | 2 | 8 | 4 | 0.50 | 2 |
   | 4 | 16 | 24 | 1.50 | 4 |
   | 8 | 32 | 112 | 3.50 | 8 |
   | 16 | 64 | 480 | 7.50 | 16 |

   **The answer, stated plainly.** The cost is exactly `(W - 1) / 2` wasted round trips per
   chunk that lands, and the worst single writer needs `W` attempts. It is **linear in the
   number of concurrent writers, not quadratic** — the ping-pong ADR-047 described and
   nothing worse — and a **lone writer is never refused at all**, which is the A1 line and is
   pinned separately by `a_single_writer_is_never_refused_by_the_ordering_rule`.

   **What this does and does not decide.** It decides that the mechanism is sound and its
   cost has the shape the ADR claimed: nobody starves, everybody's work lands, and the bound
   is the writer count. It does **not** say the shape is acceptable at a given latency: at 16
   simultaneous writers a chunk costs eight extra round trips, which on a 100 ms link is
   nearly a second of ping-pong. Two things move that number and neither needs the relay to
   hold a document: **coalescing** (`107` §4 B3 — one transaction per typing run, not per
   character, which is what makes 16 *simultaneous* writers a pathological rather than a
   typical arrangement) and **pipelining** (`MAX_OUTSTANDING`, §5.4). The measurement to run
   next is therefore the same table under realistic coalescing and a non-zero think time,
   and that one belongs with the `107` §4 benchmarks rather than here.

   The guard was driven red twice (`SKILL` §4). Dropping the chunk-rewind in
   `ClientSession::refused` — so a refused chunk is not put back — produced "*writer 1 still
   has unacknowledged work, so the run did not finish*", which is the assertion that stops a
   protocol from looking cheap by losing work. Dropping the relay's `base != revision` check
   produced "*no contention was measured at all, so nothing here is evidence*", which is what
   stops the harness from reporting a comfortable zero because it forgot to contend — the
   defect its own first draft had, and which is recorded in the harness's comments.
4. **The host-signed grant.** `protocol::Join` carries an opaque `Identity` and no token. The
   sibling's token is "the whole integration contract" — who, which document, what permission,
   where to fetch, where to POST back — and `143` §16 Q5 leaves the encoding open (JWT, PASETO,
   or an opaque provider token). Nothing here decides it; `Identity` is the seam it plugs into.
5. **Access enforcement at the operation.** The sibling enforces read-only *at the operation*
   rather than by hiding a toolbar, including inside a batch. `Refusal::ReadOnlyAccess` exists
   and nothing sends it yet, because there is no token to read an access level from.
   **Half answered, from the other direction** (ADR-049): the *document's own*
   `w:documentProtection` is now enforced at the operation, in `casual-doc-edit`, and a batch
   is judged whole — so the shape the relay needs exists and is reusable by it. What is still
   missing is the **session's** access level, which needs the token of Q4; a document that
   asks not to be edited and a participant who is not allowed to edit it are two different
   questions with one enforcement point.
6. **Durability.** The relay's retained tail is in memory and bounded by count. A durable
   ordered log, snapshots and compaction are `107` 6.1, and the sibling's warning transfers
   directly: retained/unmodelled bytes are inert, so store them **once** with the document and
   have periodic snapshots carry only the mutable model and refer to them — `serde_json` writes
   a `Vec<u8>` as an array of decimal numbers, which they measured at about **four times** the
   size. That lands squarely on our verbatim-OOXML retention advantage.
7. **`w:id`, the OOXML revision serial, is *not* partitioned — the one identity family this
   change deliberately left alone.** `RevisionIdAllocator` in `casual-doc-wasm` seeds from the
   values a document already carries and mints the lowest free integer, so two replicas
   editing in suggesting mode mint the **same `w:id` for two different revisions**. It is a
   real defect and it is a *different* family: a `w:id` is a document-format serial, not a
   model identity — no operation addresses one, nothing resolves one, and it exists for OOXML
   round-trip and for Word's "accept all by this author" grouping. Partitioning it the way
   `NodeId` is partitioned would need a high-half participant prefix, and the value is written
   into `w:ins`/`w:del` as a decimal string that Word reads as a 32-bit integer — so the fix
   is a **bounded** stride (participant `c` mints values ≡ `c` mod `S`), and choosing `S`
   trades a participant ceiling against a revision ceiling inside `i32`. That is a decision
   with a compatibility constraint, not a mechanical change, so it is recorded here rather
   than made silently. Until it is made, **two replicas in suggesting mode can produce
   colliding revision serials**, and §5.3 already blocks suggesting mode from a session for an
   unrelated reason (a commit that kept no inverse cannot be rolled back), so nothing ships
   on top of it meanwhile.
8. **Snapshot verification by replay across replicas is still not possible**, and this change
   narrowed the blocker rather than removing it. `150` §9.3 named two causes: colliding ids
   (fixed here — two replicas can no longer mint the same id) and **operations that do not
   carry the identities they cause to be minted**. The second stands: a `FormatText` that
   splits a run mints the tail run's id from the local `RunIds`, so two replicas replaying one
   ordered log still produce *different* ids for the same run and the snapshots still differ
   byte for byte. `WireOperation::introduces` already enumerates the ids an operation declares
   in its own fields; closing this means the undeclared mints inside `apply` becoming declared
   ones, the way `SplitParagraph` already carries `new_id`. See `150` §9.3, updated in place.
9. **Tracked changes and the order of wrapping.** `107` §8 Q3 and `150` §10 Q4 are untouched
   here, and §5.3's suggesting-mode blocker is now a second reason they have to be settled
   before collaboration ships.
10. **§5.3's blocker has a diagnosis, and it is not in the session** (ADR-051, proposed).
    Review typing is an `UpdateReviewState` — a whole-paragraph rewrite — so its inverse is a
    paragraph snapshot per character, which is the only reason the coalescing mode that
    *drops* inverses exists. `107` §4 **B4** already forbids a paragraph rewrite on the typing
    path, so the violation and the blocker are one thing rather than two. Expressed granularly,
    every commit can afford to keep its inverse and the rollback driver needs no change at all.
    The **second** reason suggesting mode cannot join — colliding revision `w:id` strings
    across replicas, Q7 above — is genuinely separate: ADR-048's partition covers `NodeId`
    and not a producer string.

---

## 11. Corrections to other documents, made in place

| Document | What it said | What is true |
| --- | --- | --- |
| `143` §6 / §1 | The provider owns OT transform **never** and must not become the live model — stated as settled | Still the decision, and now an *argued* one rather than an assumption: §3 records the option it excludes, what excluding it costs, and what would reverse it. `143` §16 Q1 is answered. |
| `150` §6 last row, §10 Q2 | A `Coalesce::ContinueKeepingFirstInverse` commit "cannot say what it destroyed" — recorded as an open question | Not a question: it also cannot be **rolled back**, which makes suggesting mode unable to take part in a session at all. A blocker with a stable refusal code. |
| `150` §10 Q1 | Base-state placement is "the sharpest edge in this design"; callers should pass `NoPlacement` and take the refusal | Closed. The rollback driver *is* at the base state when the placement is needed, so it builds one there and nowhere else. |
| `147` §3.3 | The log has one position, `head` | Two: `head` and `horizon`. A commit above the horizon is provisional and may be re-expressed; "nothing rewrites a commit" holds for everything below it, which is everything anybody else has seen. |
| `107` §8 Q6 | "`site_id` allocation without a mandatory server, and collision behaviour" — open | Answered by §4.2 and by a property rather than by a probability: the participant number the relay assigns *is* the site id, and `IdSpace::participant` is injective in it. **And answered for the no-server case too** (§4.4): with no relay there is no site id, so a replica mints in a reserved offline space no participant number can be handed. |
| `106` line 366 | Step 6.3's tie-break is `(revision, site_id)` | There is no `site_id` in `transform.rs` and never was: the tie-break is `Side::Earlier`/`Later`, taken from the relay's total order. Corrected in place. `104` HF-068 and `139` VH-007 carry the same stale phrase and belong to other owners, so they are reported rather than edited. |
| `150` §9.3 | Operations do not carry the identities they cause to be minted — reported, not taken | **Taken.** `150` §9.4 / ADR-048: `apply` holds no id generator, an operation travels with the *space* it mints in, and the two session convergence guards compare documents with node identities intact instead of normalising them away. |
| `152` §4.4 (this document, previous revision) | "The live editor still derives its minting namespace from the document… the fix is the next increment's first item" | Done, 2026-10-01. §4.4 records the mechanism, the id families, the backward-compatibility case, and the four mutation proofs (§7). |
| `152` §4.2 (this document, previous revision) | `space(c) = base ^ (K · (c + 1))`, one refused participant number | `space(c) = base ^ (K · (c + 2))`, two refused participant numbers, `base ^ K` reserved for an offline replica. `PROTOCOL_VERSION` 1 → 2, because the space is derived and two versions would disagree about it silently. |
