# Runtime Event Foundation

**Status:** Accepted for Phase 0
**Decision date:** 2026-07-24
**Tracker:** P0-005
**Implementation:** Complete on 2026-07-24

## Outcome

Add a bounded, ordered event journal to each document session. The first public
transport is synchronous queue polling. Native callbacks, async streams, and
JavaScript event bridges will adapt this journal later instead of defining
independent ordering semantics.

## Scope

Phase 0 emits:

- `TransactionCommitted` after forward edits, undo, and redo;
- `SelectionChanged` after an explicit selection change or an edit that maps
  selection to a different value.

Not included yet:

- ready, close, layout, command-state, resource, warning, or error events;
- blocking waits or async streams;
- host callbacks;
- event filtering;
- event coalescing;
- event persistence across process or session lifetime;
- cross-session or collaboration ordering.

## Ordering

Every emitted event receives a session-local `u64` sequence beginning at one.
Sequences are contiguous until the counter is exhausted and never represent
wall-clock time.

For one editing transaction:

1. emit `TransactionCommitted`;
2. emit `SelectionChanged` only when mapping changed anchor, focus, or affinity.

Events are therefore ordered by sequence and carry non-decreasing document
revisions. Selection-only updates retain the current document revision.

Concurrent session mutations are serialized by the existing session write lock.
State and all events caused by that mutation are recorded under the same lock,
so a subscriber cannot observe an event for partially published state.

## Payloads

Transaction events contain:

- committed revision;
- operation count;
- ordered position map;
- origin: forward, undo, or redo.

Selection events contain:

- current document revision;
- complete directed selection;
- reason: explicit, transaction, undo, or redo.

Payloads use SDK-owned values. Internal model, transaction, and selection types
remain private.

## Subscription

`DocumentSession::subscribe` creates a future-only cursor. It does not replay
events emitted before subscription.

`Subscription::drain(max_events)`:

- requires `max_events > 0`;
- returns at most that many ordered events;
- advances only that subscription's cursor;
- does not remove events needed by other subscribers;
- returns immediately when no event is available.

Subscriptions are independent. Dropping one subscription has no effect on the
session or other subscriptions. A subscription retains the session state needed
for polling until the subscription is dropped; explicit session close semantics
are deferred.

## Bounded Retention

The Phase 0 journal retains the latest 256 events per session. This fixed limit
prevents an inactive subscriber from causing unbounded memory growth.

When a cursor falls behind retained history, the next batch reports the exact
number of dropped events before returning the oldest retained event. Silent
event loss is forbidden. Consumers that observe a gap must refresh snapshots
before applying later incremental notifications.

The limit becomes host-configurable only with a backward-compatible engine
configuration builder and hard ceiling.

## Atomicity And Failure

Before publishing a mutation, the journal preflights sequence allocation for
all events caused by that mutation. Sequence exhaustion returns
`ODC-9001 internal` and leaves document, selection, revision, history, and
journal unchanged.

Polling a zero-sized batch returns `ODC-0001 invalid_argument` without advancing
the cursor.

The journal contains owned immutable event payloads. No host callback executes
while the session lock is held. Future callback/async adapters must first copy
or dequeue events and release internal locks before invoking host code.

## Rejected Alternatives

**Invoke callbacks directly from edit methods**

Rejected because re-entrancy, lock inversion, panic isolation, and host-thread
latency would become part of transaction correctness.

**Use an unbounded channel**

Rejected because abandoned or slow subscribers could grow memory without a
limit.

**Use document revision as the only event identity**

Rejected because multiple events can describe one transaction and explicit
selection changes do not increment document revision.

**Coalesce selection events in the foundation**

Rejected because coalescing policy depends on host frame scheduling. The base
journal preserves every semantic transition within its retention window.

## Acceptance Gates

- independent future-only subscriptions receive identical ordered events;
- one edit emits transaction first and changed selection second;
- unchanged explicit selection emits nothing;
- explicit selection emits without incrementing document revision;
- undo and redo origins and selection reasons are distinguishable;
- lag beyond 256 events reports the exact gap and remains bounded;
- zero-sized drains and sequence exhaustion are atomic;
- no callback or host code runs under the session lock;
- native, WASM, MSRV, docs, lint, and policy gates remain green.
