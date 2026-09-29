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
  `wire::IdSpace::of(document_space, client)` derives one per participant and is **injective
  in the participant number**: `space(c) = base ^ (K · (c + 1))` with `K` odd, so `c ↦ K·(c+1)`
  is a bijection on `u64` and xor with a constant is a bijection. Not "unlikely to collide" —
  cannot. It is also never the document's own space, which would need `c = u64::MAX`, and that
  participant number is refused rather than left as a remark.
- It is **derived, not carried**: a receiver computes the sender's space from the arrival's
  `client` field, so there is no wire field to forge and no table to keep in step.
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
2. mint its key through the session's `IdSpace`, never through a document-derived one;
3. add its variant to `wire::WireOperation::introduces` — an exhaustive match with no wildcard
   arm, so this one is a compile error rather than a review comment;
4. add its table to `wire::Table` and to `localise`'s collision check;
5. arrive with **a test in which the receiver already holds a *different* entry at that id.**
   An id that lines up by accident proves nothing, and a test that merely round-trips a value
   proves less.

### 4.4 What this does not fix, and who owns it

The live editor still derives its minting namespace from the document. **This increment does
not change `casual-doc-wasm`** (§9), so the fix — an editor that mints in a session-supplied
space — is the next increment's first item. Until then a session refuses every arrival that
introduces an id, loudly and with a code, which is the right failure: the alternative is the
silent overwrite. `an_identity_minted_in_the_document_s_own_space_is_refused` pins exactly
that, against exactly today's derivation.

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

23 tests over the state machines, driving **two replicas and a relay in one process**. The
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

The convergence test is the one that matters most: two replicas typing into one paragraph at
the same offset, one of them twice, so the rebase is of a **sequence** and not of a single
operation — the case the probe exists for.

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
| **The byte codec** | `casual-doc-edit` has no `serde` at all, and the op-set lane is about to move the operation shapes (node-addressed blocks, `Pos` affinity, minted run ids — `150` §9.1–§9.3). Freezing bytes over shapes that are about to change is the one thing a compatibility surface must not do. `107` §8 Q5, `150` §10 Q5. |
| **The relay binary** | It is a workspace member under `server/`, not a crate, and it needs the codec and a transport first. The state machine it will drive is here and is testable without it, which is the point of a state machine over supplied bytes. |
| **Presence and cursors** | Yjs's awareness protocol, adopted not invented: one entry per client, overwritten wholesale, no merge therefore no transform, never persisted or replayed. It needs no transform and no order, so it is genuinely separable — and it needs the anchor mapping `107` P-4 owes before a remote cursor can survive a structural edit. |
| **Collaborative undo** | `150` §11 already records what the transform commits us to, and the sibling's `docs/69` is the reference. It is a **local** decision taken before submitting, needs no wire field and no protocol bump, and its primitive — `Rebase::Tombstoned` — already exists. |
| **Any `casual-doc-wasm` change** | Another lane owns that crate, and §4.4's fix belongs there. `the_live_editor_has_no_collaboration_dependency` is what stops the next increment reaching into it by accident rather than on purpose. |

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
3. **The relay's refusal rate under contention** (§3.4). This is the measurement that decides
   whether §3.3 was right, and nothing here should be called proven until it is run.
4. **The host-signed grant.** `protocol::Join` carries an opaque `Identity` and no token. The
   sibling's token is "the whole integration contract" — who, which document, what permission,
   where to fetch, where to POST back — and `143` §16 Q5 leaves the encoding open (JWT, PASETO,
   or an opaque provider token). Nothing here decides it; `Identity` is the seam it plugs into.
5. **Access enforcement at the operation.** The sibling enforces read-only *at the operation*
   rather than by hiding a toolbar, including inside a batch. `Refusal::ReadOnlyAccess` exists
   and nothing sends it yet, because there is no token to read an access level from.
6. **Durability.** The relay's retained tail is in memory and bounded by count. A durable
   ordered log, snapshots and compaction are `107` 6.1, and the sibling's warning transfers
   directly: retained/unmodelled bytes are inert, so store them **once** with the document and
   have periodic snapshots carry only the mutable model and refer to them — `serde_json` writes
   a `Vec<u8>` as an array of decimal numbers, which they measured at about **four times** the
   size. That lands squarely on our verbatim-OOXML retention advantage.
7. **Tracked changes and the order of wrapping.** `107` §8 Q3 and `150` §10 Q4 are untouched
   here, and §5.3's suggesting-mode blocker is now a second reason they have to be settled
   before collaboration ships.

---

## 11. Corrections to other documents, made in place

| Document | What it said | What is true |
| --- | --- | --- |
| `143` §6 / §1 | The provider owns OT transform **never** and must not become the live model — stated as settled | Still the decision, and now an *argued* one rather than an assumption: §3 records the option it excludes, what excluding it costs, and what would reverse it. `143` §16 Q1 is answered. |
| `150` §6 last row, §10 Q2 | A `Coalesce::ContinueKeepingFirstInverse` commit "cannot say what it destroyed" — recorded as an open question | Not a question: it also cannot be **rolled back**, which makes suggesting mode unable to take part in a session at all. A blocker with a stable refusal code. |
| `150` §10 Q1 | Base-state placement is "the sharpest edge in this design"; callers should pass `NoPlacement` and take the refusal | Closed. The rollback driver *is* at the base state when the placement is needed, so it builds one there and nowhere else. |
| `147` §3.3 | The log has one position, `head` | Two: `head` and `horizon`. A commit above the horizon is provisional and may be re-expressed; "nothing rewrites a commit" holds for everything below it, which is everything anybody else has seen. |
| `107` §8 Q6 | "`site_id` allocation without a mandatory server, and collision behaviour" — open | Answered by §4.2 and by a property rather than by a probability: the participant number the relay assigns *is* the site id, and `IdSpace::of` is injective in it. |
