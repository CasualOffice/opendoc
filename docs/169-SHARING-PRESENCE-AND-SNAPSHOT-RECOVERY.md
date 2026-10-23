# 169 — Sharing, presence and snapshot recovery

Status: implemented; collaboration remains experimental. Design approved by the owner
2026-10-11. See ADR-067 and backlog COLLAB-001.

## Host boundary

The engine owns transactions, cursor payloads and geometry, snapshot validation and
replay. The browser owns sockets, scheduling and overlays. The embedding host owns
rooms, document storage, authentication, authorization and authoritative checkpoints.
No engine crate depends on the relay or on a mandatory provider.

The editor reads an optional `globalThis.OPENDOC_COLLABORATION` provider:

```js
{
  async createShare({ bytes, name }) {
    // Persist these exact collabSnapshot() package bytes and create the room before returning.
    return { room: "wss://host.example/rooms/id",
      documentUrl: "https://host.example/documents/id/seed.json", revision: 0 };
  },
  async snapshotFor({ room, minimumRevision }) {
    // Return a checkpoint at or after minimumRevision, of this same document.
    return { bytes: new Uint8Array(/* snapshot package */), revision: 42 };
  },
  async publishSnapshot({ room, revision, bytes }) {
    // Optional. Authenticate the author, verify authority, persist monotonically.
  }
}
```

`createShare` is required to enable Share for an unshared document. A participant
already following a valid shared link can copy that link again without this method.
The editor exports the seed, persists it through the host, verifies that editing did
not change the document in the meantime, then joins. The clipboard has a manual-copy
fallback in the editor’s registered modal dialog. The link carries `room`, `document` and the seed's exact
`revision`; it excludes grants, author names and unrelated URL parameters. Document
URLs must be HTTP(S), same-origin and without credentials; HTTPS pages require WSS.
The host must authorize document reads and room joins separately from possession of
the link. Links are invitations, not capability grants.

The host command contract declares `collab.share` with the `save` grant because
it persists document bytes, and `collab.downloadRetained` with the `download`
grant. `collab.recover`, like reconnecting, needs no new mutation grant: it
synchronizes authoritative state and previously authorized work, with the engine
checking protection and capabilities during replay. Both host transports apply
these same requirements. Unavailable recovery and backup commands explain why
they cannot run, including a recovery already in progress.

A participant opens the seed automatically. Editing is refused until Welcome. Protocol
6's Welcome carries `base_revision` and `missed`: the fresh replica applies the room
history between the seed and the current revision before editing or flushing. A seed
outside retained history requires `snapshotFor`. Hosts must keep document bytes paired
with their exact revision; serving a newer snapshot at an old seed URL is incorrect.
The relay continues to hold ordered operations, not the document.

## Snapshot contents and bounds

`collabSnapshot()` packages the current normalized document with its binary resources,
including images and embedded fonts. The JSON envelope starts with
`collabSnapshotVersion: 1`, followed by `document` (the normalized JSON string) and
`resources` (a sorted map of resource IDs to byte arrays). The engine opens this package
before font registration and layout; no binary resource is silently dropped. Plain
normalized JSON remains accepted for older host snapshots, but those snapshots must
already contain no external binary resources.

Packages are limited to 64 MiB, 2,048 resources, 32 MiB of decoded resource bytes, and
1,024 bytes per nonempty resource ID. Unknown envelope fields and versions are refused.
Adapter-private producer envelopes are not portable in this interchange: a shared
copy preserves the canonical model and resources, rather than promising byte-identical
DOCX source round trips. Hosts should retain original source files separately.

## Running the optional local host

```sh
cargo build -p opendoc-relay --locked
# Build webapp/pkg using the normal wasm-pack workflow first.
node examples/share-host.mjs
```

Open the editor on the host's printed loopback port (default 8100). Share creates a
room and saves its seed; opening the link in another browser profile opens that
same document. The host proxies WebSocket upgrades to independent relay processes.
It accepts at most 16 rooms, bounds request bodies to 64 MiB, binds only loopback,
rejects foreign origins and deletes its temporary storage on exit. It is a development
example with open editor access and client-supplied checkpoints, **not a production
storage/auth service**. Production hosts must verify authoritative checkpoints rather
than trusting a client upload. Existing relay deployments remain usable independently.

## Presence

`newRoster()` is separate from `WasmDocument` and its undo log. The collaboration
facade encodes/decodes frames; the roster owns monotonic clocks, unchanged-position
suppression and mapping. `caretFrame(document, anchor, offset, head, offset)` emits an
optional bounded presence frame. `remoteSelections(document)` returns engine geometry.
The editor sends at most one scheduled update per 100 ms, never queues presence on a
disconnected socket, and clears departed/disconnected participants. Colored remote
carets have participant-number labels and remote selections are overlays.

Local edits map existing carets through their position maps. A contended arrival maps
them through the complete rollback, ordered apply and local replay, rather than through
a replay log that assumes an earlier view. Presence changes neither document revisions
nor Undo. Identity comes from the relay, not from the opaque sender payload. Labels
currently show participant numbers, not host display names. Presence is ephemeral and
may briefly be behind concurrent document frames; it is never durable history.

The people in a room are also shown at the right end of the header, immediately before
Share (`109` HF-330): one chip per participant in their caret colour, this tab's first,
four chips and then a count, and at the phone rung one button with a count. The list
comes from the roster's membership (`participants`), not from caret geometry, so a
participant whose caret is not laid out yet is still listed. A chip's accessible name and
tooltip say the number stands in for a name this host does not share; pressing it scrolls
to that caret. Joins and leaves go to the strip's own polite live region, batched to at
most one sentence per 2 s, and a reconnect re-settles quietly. What a participant looks
like is one function, `participantDisplay` (`presence_strip.mjs`), the seam `175` §14
fixes, read by both the strip and the caret label.

## The reader's own caret, Undo, and the sync indicator

Added 2026-10-22 (ADR-069, `109` HF-310, HF-311, HF-313), after two real browsers through
this host reproduced three defects:

- **The reader's caret follows other people's edits.** Every frame that changed the
  document is followed, synchronously, by `collabMapSelection`, which carries the selection
  through the session's composed caret map (`107` §2.2). Before, two people typing in one
  paragraph produced the later typist's word reversed on both screens, and a remote
  insertion earlier in the paragraph sent the next keystrokes to the wrong place.
- **Undo is selective.** Remote commits are never undo steps; the reader's own step is
  undone by its inverse rebased over everything after it, or refused with the reason when
  that cannot be done cleanly (`107` §5.4). Before, ⌘Z undid the other person's typing, for
  them too.
- **The sync chip reports acknowledgements, not the socket.** `collabState` now carries
  `unsent` (typed, not yet handed to the transport) and `inFlight` (sent, not yet
  acknowledged), and `collab_sync.mjs` shows *sending*, *waiting for the server to confirm*,
  the acknowledged "your changes are reaching everyone", and — after 3 s without an
  acknowledgement — *not confirmed for N s, kept on this device*. It is its own polite live
  region beside the document-state chip, so it never overwrites a refusal in the status
  line; the status line hears only about trouble starting and ending.

## Version restore in a shared room

Refused, with the reason, on every surface (ADR-068, `109` HF-312). A restore activates the
version through the open path, which replaced this tab's document and dropped the room:
the reader's copy went back, everybody else's stayed. Make a copy and Download of a version
remain available. Leaving a room for any reason is now announced (`collab.leftShared`).

The preferred design — a restore that is one transaction against the current state, so it
travels in the settled order — needs three pieces this document's snapshot design does not
supply: re-minting every restored node in the restorer's id space with its references
intact; carrying what the op set cannot (binary resources live only in the snapshot
package); and proving on a clone that the program reproduces the version exactly. A room
re-seed through the snapshot recovery above was considered and rejected for now: the relay
has no epoch, so it needs a protocol message and a version bump, and every participant's
pending work would be judged against a document none of them wrote it on.

## Refusal recovery and preserved work

Unsupported transforms and TooFarBehind produce `recoveryRequired`. The transport
pauses chunk draining/polling. The existing optimistic document and log remain intact
while the host supplies a snapshot, or when the host is unavailable. Review offers a
retry. Invalid bytes, a different document identity, an old/fractional/unsafe revision,
protection refusal and replay failure cannot silently replace pending work.

The engine reconstructs the acknowledged base by undoing pending commits on a clone.
It replays a pending text transaction only when **every complete source paragraph is
identical** in that base and the recovery target, and current capabilities/protection
allow the operation. This conservatively supports insertion and same-paragraph deletion;
it does not guess stale offsets in a paragraph someone else changed. Structural edits,
formatting and changed-paragraph text are retained in a full copy of the original local
document. Later dependent transactions are checked against their own authored bases.

`collabRecover(bytes, revision)` installs the checked document and new pending log
atomically. Safely replayed edits remain unacknowledged and are flushed after reconnect.
The existing participant's sequence counter survives. `collabRecoveryCount` and
`collabRecoveryDocument(index)` expose directly reopenable snapshot packages; Review
can download them for manual reconciliation, choosing each document separately when
there is more than one so browser bulk-download restrictions cannot hide a backup. Up to four retained documents are kept in
memory. Download them before leaving the tab; closing/replacing the document releases
its in-memory backups. At the limit the next recovery refuses and preserves the current
copy. Retained uncertain transactions are **not automatically merged**.

`collabCheckpoint()` exports the acknowledged state, rolling back pending work on a
clone; it cannot publish optimistic edits as ordered data. Resource-bearing structural
edits must be acknowledged before a checkpoint can be published. Optional checkpoint uploads
are throttled to a scheduled 500 ms update, carry the exact room revision and must be
validated by the host. Hosts without `snapshotFor` keep the local document intact and
require their own recovery integration.

### What the reader is told

Every `ODC-7xxx` refusal is explained, and the explanation is built from what the
engine did rather than from the code (RC-13). A cause sentence (`collab.<code>`) says
only why; a consequence clause is chosen from the outcome's `kind`, `terminal` and
`retryable` and from `collabState.stopped` (`session_access.mjs`'s
`refusalConsequence`):

| Outcome | Clause | What it asserts |
|---|---|---|
| `recoveryRequired`, host has `snapshotFor` | `collab.outcome.recovering` | the edit is kept here and not yet ordered |
| `recoveryRequired`, no `snapshotFor` | `collab.outcome.recoveryUnavailable` | the same, and nothing is loading |
| `stopped`, or a thrown refusal that stopped the session | `collab.outcome.stopped` | the edit is kept here; the session has ended |
| `refused`, not retryable | `collab.outcome.notShared` | the edit is kept here, unordered; the session continues |
| thrown, session still running | `collab.outcome.notApplied` | the session continues |
| `refused`, retryable (`StaleBase`) | none | resubmitted automatically |
| `ODC-7011` | cause only | an access change touched no document |

`webapp/tests/collab_refusal_outcomes.json` records every code on every route it can
arrive by, and `every_collaboration_refusal_does_what_the_fixture_says` drives each
row through the real facade, so the table above is held to the engine. The earlier
`ODC-7006` sentence ("unsent changes were lost — reload to continue") claimed the
opposite of what this section specifies, and reloading was the one action that would
have lost the work. A terminal refusal the relay sends as a `Refused` (`NotAuthorised`,
`NotSaving`) is thrown by the engine after it records the stop; the transport now
reads `collabState.stopped` and stops too, instead of reporting "connected".

Known limit, for the engine lane: an arriving chunk refused locally with
`IdCollision` (or any other thrown merge error that is not `CannotMerge`) leaves the
document untouched but the replica one ordered revision short, and nothing asks for
recovery. The reader is told "this copy may no longer match"; converting those to
`recoveryRequired` as `CannotMerge` already is would close it.

## Convergence verification

`randomized_relay_convergence` runs three real WebSocket clients through the production
relay. Each round authors concurrent random insertions/deletions in three paragraphs,
including multibyte text, then randomizes submission order and checks complete model
and revision equality after quiescence. Each client has one to three pending transactions.
The default 24 seeds run in normal workspace CI; a larger deterministic soak is:

```sh
bash fuzz/convergence.sh 1000
```

Seed 3 exposed sequential remote chunk coordinates being treated as independent. The
driver now feeds contended remote chunk steps through rollback/replay individually,
retaining atomicity for the entire chunk. The seed remains in the default corpus.
This target covers the supported text lane. It does not prove convergence for every
structural/formatting pair or simulate arbitrary packet corruption. Existing typed
transform, rollback, refusal, reconnect and journal tests cover those separate contracts.
The browser test `collaboration.spec.mjs` verifies Share, automatic opening, late-join
catch-up, guest editing, checkpoint publication, selections and departure cleanup.

New collaboration strings have English fallback entries in all shipped catalogues;
translation review remains open.

## Browser artifact budget

With the existing release `z` profile and Binaryen 117 `-Oz`, the complete build is
25,711,106 bytes, including an 11,820,216-byte code section. Compared with the prior
chart-feature measurement recorded in the artifact guard, this adds 249,476 bytes
(235,115 code bytes). The resource package itself adds only 6,454 total / 5,865 code
bytes to the collaboration build. This is measured engine growth, not an optimization
setting change. Ceilings are 25,850,000 total / 11,950,000 code bytes, retaining tight
headroom and the existing non-stub floor and optimization-regression checks.
