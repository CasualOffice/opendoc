// The browser's end of a shared session (ADR-063, `152` §5.4/§5.5, `107` 6.6).
//
// Every guard here is a property the ENGINE cannot check for itself, because the
// engine has no socket: what happens when one closes, what is still owed when a
// write fails, how long the next attempt waits, and what the reader is told
// while that is going on. `server/src/serve_tests.rs` holds the other half —
// two real `ClientSession`s converging through a real relay — and the seam
// between the two halves is the five-member contract this file stands in for.
//
// The socket, the timers and the randomness are INJECTED, which is what lets a
// dropped connection, an exhausted retry budget and a replayed queue be driven
// in node with no browser and no wall clock. A backoff only a stopwatch can
// observe is a backoff nothing checks (`SKILL` §6: clock-bound tests degrade
// under load however sound the code is).
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

const {
  CONNECTION_STATE_KEYS,
  backoffDelay,
  collabTransport,
  openRoom,
  resumeKey,
  roomUrl,
  webSocketOpener,
} = await import("../src/collab_transport.mjs");
const { CONNECTION_LOST, refusalKey } = await import("../src/session_access.mjs");
const { EN_STRINGS } = await import("../src/en_strings.mjs");
const { hostConfig } = await import("../src/capabilities.mjs");

const GERMAN = JSON.parse(readFileSync(join(WEBAPP, "locales", "de.json"), "utf8"));

// ---- The stand-ins --------------------------------------------------------

/** A socket whose every event is raised by the test rather than by a network.
 *
 *  `failFrom` makes `send` throw from the nth call onwards, which is how a write
 *  failure — the thing the custody rule exists for — is produced without one. */
function fakeSocket() {
  const socket = {
    sent: [],
    closed: false,
    binaryType: "",
    failFrom: null,
    onopen: null,
    onmessage: null,
    onclose: null,
    onerror: null,
    send(bytes) {
      if (socket.failFrom !== null && socket.sent.length >= socket.failFrom) {
        throw new Error("InvalidStateError: the socket is closing");
      }
      socket.sent.push(bytes);
    },
    close() {
      socket.closed = true;
    },
    accept() {
      socket.onopen();
    },
    deliver(data) {
      socket.onmessage({ data });
    },
    drop() {
      socket.onclose();
    },
  };
  return socket;
}

function opener() {
  const sockets = [];
  return {
    sockets,
    open: (url) => {
      const socket = fakeSocket();
      socket.url = url;
      sockets.push(socket);
      return socket;
    },
    last: () => sockets[sockets.length - 1],
  };
}

/** A clock that records what it was asked to wait and fires only when told. */
function clock() {
  const scheduled = [];
  return {
    delays: () => scheduled.filter((t) => !t.cancelled).map((t) => t.ms),
    pending: () => scheduled.filter((t) => !t.cancelled && !t.fired).length,
    setTimer: (fn, ms) => {
      scheduled.push({ fn, ms, cancelled: false, fired: false });
      return scheduled.length - 1;
    },
    clearTimer: (handle) => {
      if (scheduled[handle]) scheduled[handle].cancelled = true;
    },
    tick() {
      for (const entry of scheduled.filter((t) => !t.cancelled && !t.fired)) {
        entry.fired = true;
        entry.fn();
      }
    },
  };
}

/** The five members of `casual-doc-wasm`'s session facade the transport uses,
 *  and nothing else — which is the contract being asserted by standing in for
 *  it. `collabNextChunk` returns `undefined` when there is nothing to send, as
 *  `Option<Vec<u8>>` does across `wasm-bindgen`. */
function engine({ pending = [], revision = 0, outcome = { kind: "ack", revision: 1 } } = {}) {
  const facade = {
    joins: [],
    received: [],
    leaves: 0,
    pending: [...pending],
    revision,
    outcome,
    throwOnReceive: null,
    throwOnJoin: null,
    get collabState() {
      return JSON.stringify({
        joined: facade.joins.length > 0,
        revision: facade.revision,
        unacknowledged: facade.pending.length > 0,
        desynced: false,
        capabilities: [],
      });
    },
    collabJoinFrame(identity, key, at) {
      if (facade.throwOnJoin) throw new Error(facade.throwOnJoin);
      facade.joins.push({ identity, key, at });
      return new Uint8Array([0x4a, at & 0xff]);
    },
    collabReceiveFrame(bytes) {
      if (facade.throwOnReceive) throw new Error(facade.throwOnReceive);
      facade.received.push(bytes);
      return JSON.stringify(facade.outcome);
    },
    collabNextChunk() {
      return facade.pending.shift();
    },
    collabLeaveFrame() {
      facade.leaves += 1;
      return new Uint8Array([0x4c]);
    },
  };
  return facade;
}

function transport(overrides = {}) {
  const wire = overrides.wire ?? opener();
  const time = overrides.time ?? clock();
  const session = overrides.session ?? engine();
  const states = [];
  const outcomes = [];
  const pipe = collabTransport({
    session,
    url: "wss://relay.example/doc/1",
    identity: "ada",
    resumeKey: "tab-key",
    open: wire.open,
    setTimer: time.setTimer,
    clearTimer: time.clearTimer,
    random: () => 1,
    onState: (state) => states.push(state),
    onOutcome: (outcome) => outcomes.push(outcome),
    baseDelayMs: 100,
    maxDelayMs: 800,
    attemptCeiling: overrides.attemptCeiling ?? 8,
  });
  return { pipe, wire, time, session, states, outcomes };
}

// ---- The client half of eviction ------------------------------------------

/** **An evicted client learns it was dropped, and the reader is told in their
 *  own language.**
 *
 *  This is the guard ADR-063 names. The relay evicts a participant whose write
 *  FAILED, so by the time it has decided there is no socket left to send a
 *  refusal down: no `Refusal` variant was added, `PROTOCOL_VERSION` was not
 *  bumped, and `every_refusal_code_has_a_row_in_the_register` would fail the
 *  other way for a row no variant carries. From a client an eviction is
 *  therefore indistinguishable from a cable, and the honest answer is one
 *  chrome-only code covering both — routed exactly as `session.grant-unreadable`
 *  is, through the one table, to a translated sentence.
 *
 *  Both halves are asserted, because they answer different questions: the CODE
 *  explains what happened, and the STATE's key is what the status line reads
 *  while it keeps happening. A reader who gets one and not the other is either
 *  told why and not what to expect, or the reverse. */
test("an evicted client learns it was dropped, and says so to the reader", () => {
  const { pipe, wire, states } = transport();
  pipe.start();
  wire.last().accept();
  assert.equal(pipe.state().name, "connected", "a socket that opened is a connected session");

  // The eviction. Nothing arrives on the wire — that is the whole point.
  wire.last().drop();

  const after = pipe.state();
  assert.equal(after.name, "reconnecting");
  assert.equal(
    after.code,
    CONNECTION_LOST,
    "a dropped connection must be NAMED, or the reader is left with a status that " +
      "changed and no reason for it",
  );

  const key = refusalKey(after.code);
  assert.equal(key, "session.connectionLost", "the code must route through the chrome's one table");
  assert.ok(EN_STRINGS[key], `${after.code} routes to ${key}, which EN_STRINGS does not declare`);
  assert.ok(GERMAN[key], `${after.code} routes to ${key}, which a shipped locale cannot answer`);

  assert.equal(after.key, CONNECTION_STATE_KEYS.reconnecting);
  assert.ok(EN_STRINGS[after.key], "the status line's own sentence must exist too");
  assert.ok(GERMAN[after.key]);

  assert.deepEqual(
    states.map((state) => state.name),
    ["reconnecting", "connected", "reconnecting"],
    "the reader is notified on each change of state and not in between",
  );
});

/** **A connection that cannot be re-established stops claiming it is trying,
 *  and still offers a way back.**
 *
 *  A transport that retries for ever with a hopeful sentence is lying by the
 *  ninth attempt. One that stops for ever with no gesture is the dead control
 *  `SKILL` §10 forbids. So the budget runs out, the state says `stopped`, the
 *  code still names what happened, and `reconnect()` resets the count. */
test("retries run out, the reader is told, and reconnect is a way back", () => {
  const { pipe, wire, time } = transport({ attemptCeiling: 2 });
  pipe.start();
  wire.last().accept();

  wire.last().drop();
  assert.equal(pipe.state().name, "reconnecting");
  assert.equal(pipe.state().attempt, 1);
  time.tick();

  wire.last().drop();
  assert.equal(pipe.state().name, "reconnecting");
  assert.equal(pipe.state().attempt, 2);
  time.tick();

  wire.last().drop();
  const done = pipe.state();
  assert.equal(done.name, "stopped", "past the ceiling the transport must stop claiming to try");
  assert.equal(done.code, CONNECTION_LOST, "stopping does not make the reason disappear");
  assert.ok(EN_STRINGS[done.key]);
  assert.equal(time.pending(), 0, "a stopped transport must not leave a timer armed");

  const sockets = wire.sockets.length;
  pipe.reconnect();
  assert.equal(wire.sockets.length, sockets + 1, "reconnect must actually open a socket");
  assert.equal(pipe.state().name, "reconnecting");
  assert.equal(pipe.state().attempt, 0, "a reader who asks is not asking for attempt nine's wait");
});

// ---- Custody: the rule the whole module is shaped around -------------------

/** **A chunk the engine has already handed over survives a failed write.**
 *
 *  `ClientSession::flush` advances `self.flushed` and pushes to `self.sent` AS
 *  IT RETURNS the chunk, and `resumed` rewinds neither. So nothing re-offers a
 *  chunk this side dropped: it is lost, and lost silently, which is the one
 *  outcome `SKILL` §12 forbids outright. The queue is custody and not a cache,
 *  and this is the guard that says so. */
test("a chunk whose write failed is sent on the next connection, not lost", () => {
  const chunk = new Uint8Array([1, 2, 3]);
  const { pipe, wire, time, session } = transport({ session: engine({ pending: [chunk] }) });
  pipe.start();
  // The join lands; the chunk's write does not.
  wire.last().failFrom = 1;
  wire.last().accept();

  assert.equal(session.pending.length, 0, "the engine has handed the chunk over and forgotten it");
  assert.equal(wire.last().sent.length, 1, "only the join got out");
  assert.equal(pipe.state().queued, 1, "so the chunk must still be in this module's custody");

  wire.last().drop();
  time.tick();
  wire.last().accept();

  assert.deepEqual(
    wire.last().sent.slice(1),
    [chunk],
    "the chunk the first socket could not take must go down the second one, behind the " +
      "new join — nothing else will ever offer it again",
  );
  assert.equal(pipe.state().queued, 0);
});

/** **Nothing is taken from the engine while the socket is not open.**
 *
 *  The companion half of the rule above, and the cheaper one to get wrong: a
 *  poll IS a handover, so polling with nowhere to put the result manufactures
 *  exactly the loss the custody queue exists to prevent. Asserted by counting
 *  what the engine still holds, which is the only place the loss would be
 *  visible. */
test("the engine is not polled while the socket is closed", () => {
  const chunks = [new Uint8Array([1]), new Uint8Array([2]), new Uint8Array([3])];
  const { pipe, wire, session } = transport({ session: engine({ pending: chunks }) });
  pipe.start();
  assert.equal(pipe.state().name, "reconnecting", "the socket has not opened yet");

  pipe.flush();
  pipe.flush();
  assert.equal(
    session.pending.length,
    3,
    "a poll while the socket is shut takes work out of the engine with nowhere to put it",
  );
  assert.equal(pipe.state().queued, 0);

  wire.last().accept();
  assert.deepEqual(wire.last().sent.slice(1), chunks, "and once it is open, all three go, in order");
  assert.equal(session.pending.length, 0);
});

// ---- The handshake --------------------------------------------------------

/** **Every connection opens with exactly one `Join`, carrying the key and the
 *  position the engine has actually reached.**
 *
 *  Three failures in one guard, all of them silent:
 *
 *    * a `Join` behind a replayed chunk is refused by the relay;
 *    * a `Join` naming revision 0 on a reconnect asks to be caught up from the
 *      beginning, and ADR-063 records that the KEY travelling on every join is
 *      the difference between being recognised and being handed a fresh
 *      `ClientId` with unacknowledged work gone and no refusal naming the loss;
 *    * **a `Join` whose own write failed must not be kept.** It is the one frame
 *      rebuilt per connection, so a join left in the custody queue would be sent
 *      again BEHIND the next connection's own join — two joins on one socket,
 *      which the relay refuses. That is the case the first half below drives,
 *      because it is the one a queue-everything implementation passes every
 *      other assertion of.
 */
test("each connection opens with one join, carrying the key and the engine's position", () => {
  const chunk = new Uint8Array([7]);
  const session = engine({ pending: [chunk], revision: 42 });
  const { pipe, wire, time } = transport({ session });

  // Connection one: the JOIN itself cannot be written.
  pipe.start();
  wire.last().failFrom = 0;
  wire.last().accept();
  assert.deepEqual(session.joins, [{ identity: "ada", key: "tab-key", at: 42 }]);
  assert.equal(wire.last().sent.length, 0, "nothing got out");
  assert.equal(
    pipe.state().queued,
    0,
    "a join is rebuilt per connection, so a failed one must NOT be held — holding it " +
      "sends two joins down the next socket",
  );
  assert.equal(session.pending.length, 1, "and the chunk was never polled, because the join failed");

  // Connection two: the join lands, the chunk does not.
  session.revision = 57;
  wire.last().drop();
  time.tick();
  wire.last().failFrom = 1;
  wire.last().accept();

  assert.deepEqual(session.joins, [
    { identity: "ada", key: "tab-key", at: 42 },
    { identity: "ada", key: "tab-key", at: 57 },
  ]);
  assert.equal(wire.last().sent.length, 1, "exactly one frame — the join — reached the relay");
  assert.deepEqual(wire.last().sent[0], new Uint8Array([0x4a, 57]), "and it is the join");

  // Connection three: everything lands, and the custody queue follows the join.
  wire.last().drop();
  time.tick();
  wire.last().accept();
  assert.equal(
    session.joins.length,
    wire.sockets.length,
    "one join per connection — no more, and never none",
  );
  assert.deepEqual(wire.last().sent[0], new Uint8Array([0x4a, 57]), "the join is still first");
  assert.deepEqual(wire.last().sent[1], chunk, "the custody queue comes after the join, not before");
  assert.equal(wire.last().sent.length, 2, "and nothing was sent twice");
});

/** **A socket error does not count as a second failure.**
 *
 *  `onerror` is always followed by `onclose`, so handling both would double the
 *  attempt count and halve every backoff at exactly the moment the relay can
 *  least afford it. */
test("an error followed by a close is one attempt, not two", () => {
  const { pipe, wire, time } = transport();
  pipe.start();
  wire.last().accept();
  wire.last().onerror(new Error("ECONNRESET"));
  wire.last().drop();
  assert.equal(pipe.state().attempt, 1);
  assert.deepEqual(time.delays(), [100], "one wait was scheduled, at the first attempt's ceiling");
});

// ---- Refusals -------------------------------------------------------------

/** **A refusal the connection survives is not the end of the session, and a
 *  terminal one is.**
 *
 *  `Refusal::is_terminal` and `is_retryable` are separate answers in the engine
 *  for the reason `docs/20` gives: collapsing them is how a client retries
 *  something it must never send again, or abandons a session over a `StaleBase`
 *  it was supposed to rebase and resubmit. The transport therefore reads the
 *  `terminal` flag and nothing else — it does not interpret the code. */
test("a non-terminal refusal keeps the session; a terminal one stops it with its reason", () => {
  const chunk = new Uint8Array([5]);
  const session = engine({
    pending: [chunk],
    outcome: { kind: "refused", code: "ODC-7009", retryable: true, terminal: false },
  });
  const { pipe, wire, time, outcomes } = transport({ session });
  pipe.start();
  wire.last().accept();
  // The chunk went out on open; the engine will re-offer a rebased one after the
  // refusal, which is what the pump on arrival is for.
  session.pending.push(new Uint8Array([6]));
  wire.last().deliver(new Uint8Array([0xf0]).buffer);

  assert.equal(pipe.state().name, "connected", "a StaleBase is a rebase, not a disconnection");
  assert.deepEqual(wire.last().sent.at(-1), new Uint8Array([6]), "and the rebased chunk is sent");
  assert.equal(outcomes.at(-1).code, "ODC-7009", "the host is handed the engine's own outcome");

  session.outcome = { kind: "stopped", code: "ODC-7002", terminal: true };
  wire.last().deliver(new Uint8Array([0xf1]).buffer);

  const after = pipe.state();
  assert.equal(after.name, "stopped");
  assert.equal(after.code, "ODC-7002");
  assert.equal(
    refusalKey(after.code),
    "collab.protocolVersion",
    "a terminal refusal keeps the ENGINE's code, which is more specific than a transport one",
  );
  assert.equal(time.pending(), 0, "a version mismatch must not be retried — it loops for ever");
  assert.ok(wire.last().closed, "and the socket is closed rather than left open and idle");
});

/** **A frame this build cannot read is reported and does not end the session.**
 *
 *  The engine answers `ODC-7007` by throwing, because an unreadable frame is the
 *  one case where there is no outcome to return. The transport must hand that
 *  on and keep the connection, which the engine's own refusal taxonomy requires:
 *  `Malformed` means *do not send that again*, not *the session is over*. */
test("an unreadable frame is reported to the host and the session survives", () => {
  const session = engine({});
  session.throwOnReceive = "refused: A message from the shared session could not be read.";
  const { pipe, wire, outcomes } = transport({ session });
  pipe.start();
  wire.last().accept();
  wire.last().deliver(new Uint8Array([0, 1, 2]).buffer);

  assert.equal(outcomes.at(-1).kind, "error");
  assert.match(outcomes.at(-1).message, /could not be read/);
  assert.equal(pipe.state().name, "connected");
});

/** **A deliberate leave writes a `Leave`; a dropped socket does not.**
 *
 *  Worth the frame: a clean close is answered at once, and a dropped socket
 *  waits on the operating system. Worth NOT sending after an eviction: there is
 *  nothing there. */
test("stop leaves deliberately, and a drop does not invent a leave", () => {
  const first = transport();
  first.pipe.start();
  first.wire.last().accept();
  first.pipe.stop();
  assert.equal(first.session.leaves, 1);
  assert.equal(first.pipe.state().name, "stopped");
  assert.ok(first.wire.last().closed);

  const second = transport();
  second.pipe.start();
  second.wire.last().accept();
  second.wire.last().drop();
  assert.equal(second.session.leaves, 0, "there is no socket to say goodbye down");
});

/** **An identity the engine will not accept is terminal, and is not retried.**
 *
 *  Retrying cannot change the answer — it is the host's value, refused by the
 *  engine for a stated reason — so a backoff loop here would be an infinite one
 *  with a hopeful sentence on it. The engine's own sentence is kept verbatim
 *  because it names which input was wrong. */
test("a join the engine refuses stops the transport and keeps the engine's sentence", () => {
  const session = engine({});
  session.throwOnJoin = "refused: A shared session needs an identity for the person joining.";
  const { pipe, wire, time } = transport({ session });
  pipe.start();
  wire.last().accept();

  const after = pipe.state();
  assert.equal(after.name, "stopped");
  assert.match(after.problem, /needs an identity/);
  assert.equal(time.pending(), 0);
});

// ---- The backoff, as complexity rather than as milliseconds ----------------

/** **The wait doubles, then stops doubling, and a thousandth attempt still
 *  returns a number.**
 *
 *  Guarded as a RATIO at *n* and *2n* rather than against a stopwatch, which is
 *  `SKILL` §8's rule and the only form that can tell a bounded backoff from an
 *  unbounded one. `random: () => 1` reads the ceiling out of the full-jitter
 *  window; the jitter itself is asserted separately, because a backoff with no
 *  jitter is what puts every client a relay was holding on the same
 *  millisecond. */
test("the backoff ceiling doubles, caps, and stays finite", () => {
  const ceiling = (attempt) =>
    backoffDelay(attempt, { baseDelayMs: 100, maxDelayMs: 800, random: () => 1 });

  assert.deepEqual(
    [0, 1, 2, 3, 4, 5].map(ceiling),
    [100, 200, 400, 800, 800, 800],
    "the window must double until it reaches the cap and then stop",
  );
  for (const n of [0, 1, 2]) {
    assert.equal(ceiling(n + 1), ceiling(n) * 2, "doubling, asserted as a ratio and not a value");
  }
  assert.equal(
    ceiling(1000),
    800,
    "an attempt number no cap absorbs must still return the cap — `base * 2 ** 1000` is " +
      "Infinity, and a transport cannot wait for that",
  );

  // Full jitter: the wait is spread across the window rather than landing on it.
  assert.equal(backoffDelay(3, { baseDelayMs: 100, maxDelayMs: 800, random: () => 0 }), 0);
  assert.equal(backoffDelay(3, { baseDelayMs: 100, maxDelayMs: 800, random: () => 0.5 }), 400);
  assert.notEqual(
    backoffDelay(3, { baseDelayMs: 100, maxDelayMs: 800, random: () => 0.25 }),
    backoffDelay(3, { baseDelayMs: 100, maxDelayMs: 800, random: () => 0.75 }),
    "without jitter every client a relay was holding retries in the same millisecond",
  );
});

/** **The transport asks for the backoff it computed, attempt by attempt.**
 *
 *  The arithmetic above is only worth having if the transport uses it, and the
 *  attempt number it passes is the one thing it can get wrong without any
 *  visible symptom. */
test("each retry waits for its own attempt's window", () => {
  const { pipe, wire, time } = transport({ attemptCeiling: 8 });
  pipe.start();
  wire.last().accept();
  for (let i = 0; i < 5; i += 1) {
    wire.last().drop();
    time.tick();
    wire.last().accept();
  }
  assert.deepEqual(
    time.delays(),
    [100, 100, 100, 100, 100],
    "a successful connection resets the attempt count, so every wait is the first one's",
  );

  const slow = transport({ attemptCeiling: 8 });
  slow.pipe.start();
  slow.wire.last().accept();
  for (let i = 0; i < 5; i += 1) {
    slow.wire.last().drop();
    slow.time.tick();
  }
  assert.deepEqual(
    slow.time.delays(),
    [100, 200, 400, 800, 800],
    "and a connection that keeps failing backs off, capped",
  );
});

// ---- The seam itself ------------------------------------------------------

/** **The transport touches the session through five members and nothing else.**
 *
 *  ADR-063's division is the whole design: the engine owns the session and this
 *  owns the socket. A sixth member would be this module learning something about
 *  the protocol, which is how a transport comes to decide what a refusal means.
 *  Scanned from the source, so the stand-in above cannot be what keeps it
 *  honest — a stub only proves the members it implements are enough, not that no
 *  others are used. */
test("the transport uses five members of the session facade and no more", () => {
  const source = readFileSync(join(WEBAPP, "src", "collab_transport.mjs"), "utf8")
    .split("\n")
    .filter((line) => !line.trim().startsWith("//"))
    .join("\n");
  const used = [...source.matchAll(/\bsession\.([A-Za-z]+)/g)].map((m) => m[1]);
  assert.deepEqual(
    [...new Set(used)].sort(),
    ["collabJoinFrame", "collabLeaveFrame", "collabNextChunk", "collabReceiveFrame", "collabState"],
    "a sixth member is this module learning the protocol it is meant to be ignorant of",
  );
  assert.ok(used.length >= 5, "the scan must actually find the calls");
});

/** **The socket opener takes its constructor as an argument.**
 *
 *  Which is what keeps the module in `PURE_MODULES` and testable, and what lets
 *  a host supply its own transport. `binaryType` is set here because the codec
 *  takes bytes and the browser's default is a `Blob`. */
test("the opener is given its WebSocket rather than reaching for one", () => {
  const made = [];
  class Stub {
    constructor(url) {
      this.url = url;
      made.push(this);
    }
  }
  const socket = webSocketOpener(Stub)("wss://relay.example/doc/1");
  assert.equal(made.length, 1);
  assert.equal(socket.url, "wss://relay.example/doc/1");
  assert.equal(socket.binaryType, "arraybuffer", "a Blob arrives asynchronously; a frame cannot");
});

/** **Starting twice does not open two sockets on one session, in any phase.**
 *
 *  A chrome that re-runs its boot path — a re-mount, a locale switch, a second
 *  `DOMContentLoaded` in a harness — would otherwise end up with two connections
 *  sharing one engine, each taking custody of chunks the other will never send.
 *
 *  All four phases are driven, because they are guarded in two different places
 *  and a guard that drives only the easy ones proves whichever happens to be
 *  checked first. `connecting`, `open` and `stopped` are refused by `connect`;
 *  **`waiting` is refused by `start`**, and that is the one that matters — a
 *  retry is already scheduled there, so starting opens a second socket AND
 *  leaves the timer armed to open a third. */
test("start opens at most one socket, from every phase", () => {
  const { pipe, wire, time } = transport();

  pipe.start();
  pipe.start();
  assert.equal(wire.sockets.length, 1, "connecting");

  wire.last().accept();
  pipe.start();
  assert.equal(wire.sockets.length, 1, "open");

  wire.last().drop();
  assert.equal(pipe.state().name, "reconnecting");
  assert.equal(time.pending(), 1, "a retry is scheduled");
  pipe.start();
  assert.equal(wire.sockets.length, 1, "waiting: the scheduled retry owns the next socket");
  assert.equal(time.pending(), 1, "and starting must not have disturbed it");
  time.tick();
  assert.equal(wire.sockets.length, 2, "which then opens exactly one more");

  pipe.stop();
  pipe.start();
  assert.equal(wire.sockets.length, 2, "stopped: reconnect is the way back, not start");
});

/** **No room is the normal case, and a scheme that is not a socket is refused.**
 *
 *  `152` §2a mode 1: a document opened from a file has no room, needs no server
 *  and never will. So the absent case must be `null` rather than a refusal — a
 *  standalone page that announced a problem would be reporting the local-first
 *  guarantee as a fault. The refused cases are refused rather than handed to a
 *  `WebSocket` constructor, and `http:` is refused as loudly as `javascript:`
 *  because a host that wrote it has a bug and a silent upgrade hides it. */
test("a room is a ws or wss URL, and its absence is the standalone mode", () => {
  for (const absent of [null, undefined, "", "   ", 7, {}]) {
    assert.equal(roomUrl(absent), null, `for ${JSON.stringify(absent)}`);
  }
  for (const refused of [
    "http://relay.example/doc/1",
    "https://relay.example/doc/1",
    "javascript:alert(1)",
    "data:text/plain,hello",
    "relay.example/doc/1",
    "file:///etc/passwd",
  ]) {
    assert.equal(roomUrl(refused), null, `${refused} must not reach a socket constructor`);
  }
  assert.equal(roomUrl("wss://relay.example/doc/1"), "wss://relay.example/doc/1");
  assert.equal(roomUrl("  ws://localhost:8090/doc/1  "), "ws://localhost:8090/doc/1");
});

/** **The room arrives with the rest of the host's configuration, not beside it.**
 *
 *  `capabilities.mjs`'s own comment gives the reason and names the defect it is
 *  guarding against: two places that know how a host configures this editor is
 *  how `autosave` came to be known by both that file and `main.js`. So the
 *  endpoint is a `hostConfig()` field, and this asserts the two halves meet —
 *  `hostConfig` reads the string, `roomUrl` decides. */
test("the room endpoint is a hostConfig field that roomUrl accepts", () => {
  const config = hostConfig({
    location: { search: "?room=wss%3A%2F%2Frelay.example%2Fdoc%2F1" },
    self: 1,
    top: 1,
  });
  assert.equal(config.room, "wss://relay.example/doc/1", "read verbatim, not decided");
  assert.equal(roomUrl(config.room), "wss://relay.example/doc/1");
  assert.equal(roomUrl(hostConfig({ location: { search: "" } }).room), null);
});

// ---- The composition `main.js` would otherwise write inline ---------------

/** **No room means no transport, and that is the standalone mode rather than a
 *  failure.**
 *
 *  `null` is what makes the caller's next open correct without a branch
 *  (`collab?.stop()`), and it is what keeps a document opened from a file free of
 *  any sentence about a connection it never wanted (`152` §2a mode 1). The three
 *  ways there can be no transport are driven separately, because two of them are
 *  the host's mistake and one is the normal case. */
test("openRoom returns null for a document with no room", () => {
  class Stub {}
  const session = engine();
  assert.equal(openRoom(session, null, Stub), null, "no room at all");
  assert.equal(openRoom(session, "http://relay.example/x", Stub), null, "not a socket URL");
  assert.equal(openRoom(session, "wss://relay.example/x", undefined), null, "no WebSocket here");
  assert.equal(openRoom(null, "wss://relay.example/x", Stub), null, "no document yet");
  assert.equal(session.joins.length, 0, "and nothing was asked of the engine on any of them");
});

/** **A configured room is STARTED, not merely built.**
 *
 *  A transport that was assembled and never started is `SKILL` §9.4 one layer
 *  smaller: everything present, nothing reachable. So this asserts the socket was
 *  opened and the handshake went out, not that an object came back. */
test("openRoom opens the socket and hands the host its outcomes", () => {
  const made = [];
  class Stub {
    constructor(url) {
      this.url = url;
      this.sent = [];
      this.binaryType = "";
      made.push(this);
    }
    send(bytes) {
      this.sent.push(bytes);
    }
    close() {}
  }
  const session = engine({ revision: 11 });
  const states = [];
  const outcomes = [];
  const pipe = openRoom(session, "wss://relay.example/doc/1", Stub, {
    identity: "ada",
    resumeKey: "tab-key",
    onState: (state) => states.push(state),
    onOutcome: (outcome) => outcomes.push(outcome),
  });
  assert.ok(pipe, "a configured room must produce a transport");
  assert.equal(made.length, 1, "and it must have opened a socket");
  assert.equal(made[0].url, "wss://relay.example/doc/1");
  assert.equal(made[0].binaryType, "arraybuffer");

  made[0].onopen();
  assert.deepEqual(session.joins, [{ identity: "ada", key: "tab-key", at: 11 }]);
  assert.equal(pipe.state().name, "connected");
  assert.ok(EN_STRINGS[pipe.state().key]);

  made[0].onmessage({ data: new Uint8Array([1, 2]).buffer });
  assert.equal(outcomes.at(-1).kind, "ack", "the host is handed what the engine decided");
  assert.ok(states.some((state) => state.name === "connected"));
});

/** **A missing identity is substituted; a missing resume KEY is not.**
 *
 *  The asymmetry is the decision. `Identity::new("")` is refused by the engine, so
 *  passing an absent host name through would turn a forgotten input into a
 *  terminal stop at the handshake — a refusal about the wrong thing, in a place
 *  the reader cannot connect to its cause. A key invented per call would do
 *  something worse: connect, work, and then fail to resume **silently**, which is
 *  exactly the loss ADR-063 records (a fresh `Welcome`, a new `ClientId`, and
 *  unacknowledged work gone with no refusal naming it). So the key is left for the
 *  engine to refuse by name. */
test("openRoom substitutes a missing identity and refuses a missing resume key", () => {
  const made = [];
  class Stub {
    constructor() {
      this.binaryType = "";
      this.sent = [];
      made.push(this);
    }
    send(bytes) {
      this.sent.push(bytes);
    }
    close() {}
  }

  const named = engine();
  const pipe = openRoom(named, "wss://relay.example/doc/1", Stub, { resumeKey: "k" });
  assert.equal(named.joins.length, 0, "nothing is sent before the socket opens");
  made.at(-1).onopen();
  assert.deepEqual(
    named.joins,
    [{ identity: "anonymous", key: "k", at: 0 }],
    "an absent host name must not reach the engine as the empty string it refuses",
  );
  assert.equal(pipe.state().name, "connected");

  // No key: the engine's own refusal, by name, on the state the chrome reads.
  const keyless = engine();
  keyless.throwOnJoin = "refused: The key this tab would be recognised by is empty or too long.";
  const second = openRoom(keyless, "wss://relay.example/doc/1", Stub, { identity: "ada" });
  made.at(-1).onopen();
  assert.equal(second.state().name, "stopped", "and is not retried, because retrying cannot help");
  assert.match(
    second.state().problem,
    /recognised by is empty/,
    "the engine's own sentence names which input was wrong; nothing here could",
  );
});

// ---- The resume key ------------------------------------------------------

/** **The key survives a reload of this tab and is never shared with another.**
 *
 *  `152` §5.5 and ADR-063 between them make this load-bearing rather than tidy: a
 *  client with no key is handed a fresh `Welcome` and a new `ClientId`, and
 *  whatever it had not had acknowledged is gone **with no refusal naming the
 *  loss**. And a key shared between two tabs is two clients claiming to be one,
 *  which would hand the second one the first one's position.
 *
 *  So: remembered (a reload resumes), minted once (a second call in the same tab
 *  returns the same value), and per-storage (a second tab's storage is a second
 *  key). The last of those is what `sessionStorage` buys over `localStorage`, and
 *  it is asserted rather than left to the choice of accessor. */
test("the resume key is remembered for the tab and not shared with another", () => {
  const store = () => {
    const held = new Map();
    return {
      held,
      getItem: (k) => (held.has(k) ? held.get(k) : null),
      setItem: (k, v) => held.set(k, v),
    };
  };
  let n = 0;
  const mint = () => `key-${(n += 1)}`;

  const tab = store();
  const first = resumeKey(tab, mint);
  assert.equal(first, "key-1");
  assert.equal(resumeKey(tab, mint), "key-1", "a reload of this tab must present the same key");
  assert.equal(n, 1, "and must not mint a second one");

  const other = store();
  assert.notEqual(
    resumeKey(other, mint),
    first,
    "a second tab sharing the key would be two clients claiming to be one",
  );
});

/** **A tab that cannot remember a key still gets one.**
 *
 *  A private window, blocked site data, or an accessor that throws on read or on
 *  write. Such a tab cannot resume, which is a real loss and worse than
 *  remembering — and refusing to open the document over it would be very much
 *  worse. Both throwing directions are driven, because a store that reads and
 *  refuses to write is the shape that gets missed. */
test("a storage that is absent or throws still yields a usable key", () => {
  const mint = () => "fresh";
  assert.equal(resumeKey(null, mint), "fresh", "no storage at all");
  assert.equal(resumeKey(undefined, mint), "fresh");
  const throwsOnRead = {
    getItem() {
      throw new Error("The operation is insecure.");
    },
    setItem() {},
  };
  assert.equal(resumeKey(throwsOnRead, mint), "fresh");
  const throwsOnWrite = {
    getItem: () => null,
    setItem() {
      throw new Error("QuotaExceededError");
    },
  };
  assert.equal(resumeKey(throwsOnWrite, mint), "fresh");
});
