// SPDX-License-Identifier: Apache-2.0
// The browser's end of a shared session: a reconnecting byte pipe, and nothing else.
//
// ADR-063 draws the line and this file is one side of it. `casual-doc-wasm`'s
// `collab` module owns the SESSION — it builds the `Join`, decodes every frame,
// applies arrivals through `ClientSession::receive`, rebases after a refusal and
// decides what a refusal means. This owns the SOCKET: when to open one, what to
// do when it closes, how long to wait before trying again, and which of three
// sentences the reader is shown while that is going on. Nothing here parses a
// frame, reads a revision out of a payload, or touches a document.
//
// ---- THE ESTABLISHED PATTERNS, NAMED BEFORE ANY CODE (SKILL §8) ------------
//
// Three, and none of them is new:
//
//   1. A RECONNECTING CLIENT WITH AN OUTBOUND QUEUE. socket.io's `sendBuffer`,
//      MQTT's in-flight window, a Kafka producer's accumulator: frames the
//      application handed over are held in order until the wire has taken them,
//      and the queue is drained on connect rather than discarded. The subtlety
//      that decides the whole design is in the next section.
//   2. RESUME FROM THE LAST ORDERED POSITION, not from zero — `Last-Event-ID`
//      on an EventSource, a consumer offset, a WAL LSN. The position comes from
//      the engine's own `collabState.revision`, so there is one answer to "where
//      am I" rather than a count kept here that can disagree with it.
//   3. EXPONENTIAL BACKOFF WITH FULL JITTER, as in AWS's "Exponential Backoff
//      And Jitter": `wait = random(0, min(cap, base * 2 ** attempt))`. Full and
//      not equal jitter, because the failure that matters is a relay coming back
//      up and being hit by every client it was holding in the same millisecond,
//      and full jitter is what spreads them.
//
// ---- WHY THE QUEUE IS REPLAYED AND NOT DROPPED -----------------------------
//
// It would be reasonable to argue the opposite: the engine already holds the
// unacknowledged work, so a queue here is a second copy of it, and two sources
// of truth for "what still needs sending" is the one mechanism this design is
// careful not to duplicate. That argument is wrong, and `ClientSession::flush`
// says why in its own code: it advances `self.flushed` and pushes to
// `self.sent` AS IT HANDS THE CHUNK OVER. `resumed` does not rewind either one.
//
// So a chunk this module takes from `collabNextChunk()` and fails to write is
// not re-offered by anything — it is simply gone, and gone silently, which is
// the one outcome `SKILL` §12 forbids outright. The engine rewinds only on a
// `Refused` it actually received.
//
// That gives two rules, and they are the reason this file exists at all:
//
//   * NEVER POLL THE ENGINE WHILE THE SOCKET IS NOT OPEN. A poll is a handover;
//     a handover with nowhere to go is a loss.
//   * A FRAME TAKEN FROM THE ENGINE STAYS IN CUSTODY UNTIL `send` RETURNS. On a
//     reconnect the queue is replayed in order, ahead of anything new.
//
// Replaying a chunk the relay may already have seen is safe and is the designed
// recovery rather than a hazard: sequence numbers are per client and monotonic,
// a duplicate is either accepted (the relay never got it) or answered
// `ODC-7009 StaleBase`, and `ClientSession::refused` is what rewinds the flush
// mark so the engine re-offers a REBASED chunk. That loop is exactly what
// `two_clients_through_one_relay_converge_on_one_document` exercises.
//
// ---- EVICTION, AND WHY IT ARRIVES AS A CLOSED SOCKET -----------------------
//
// The relay evicts a participant whose write FAILED. There is therefore no
// socket left to send a refusal down, which is why no `Refusal` variant was
// added for it and `PROTOCOL_VERSION` was not bumped. From here it is
// indistinguishable from any other dropped connection, and saying so is the
// point: this module reports `session.connection-lost` — a code in the chrome's
// own table that the engine never produces, routed exactly as
// `session.grant-unreadable` is — rather than inventing a certainty it does not
// have. The reader is told the connection is gone and that what they type now is
// not being shared; they are not told a story about why.
//
// ---- PURITY -----------------------------------------------------------------
//
// No DOM, no engine import, no clock and no socket constructor: the socket
// opener, the timers and the randomness all arrive as arguments, for the reason
// `reflow_view.mjs` and `proof_packs.mjs` give about theirs. That is what lets
// `collab_transport.test.mjs` drive a dropped connection, eight failed retries
// and a replayed queue in node, against a stand-in socket, with no wall clock —
// and a backoff that can only be observed through a stopwatch is a backoff
// nothing checks. `module_seams.test.mjs` holds the purity.
//
// Every sentence is a catalogue key the caller resolves; this module adds no
// string of its own (`docs/124`).

import { CONNECTION_LOST } from "./session_access.mjs";

/** The reader-facing connection state → catalogue key.
 *
 *  THREE AND NOT FOUR. A first connection that has not landed yet reports
 *  `reconnecting` like any later one, because "trying to connect" and "trying to
 *  reconnect" are the same fact about the reader's document — their changes are
 *  not reaching anyone — and a fourth state would be a fourth sentence to
 *  translate for a distinction nobody acts on.
 *
 *  `stopped` is not an error state. It is reached by `stop()` on a deliberate
 *  leave, by a terminal refusal, and by exhausting the retries; which of those
 *  happened is in the state's `code`, so one label does not have to carry three
 *  meanings. */
export const CONNECTION_STATE_KEYS = Object.freeze({
  connected: "collab.connected",
  reconnecting: "collab.reconnecting",
  stopped: "collab.stopped",
});

/** How long to wait before retry number `attempt` (zero-based).
 *
 * Full jitter: `random(0, min(cap, base * 2 ** attempt))`. Exported separately
 * from the transport because a backoff is arithmetic and arithmetic should be
 * answerable without a connection — `collab_transport.test.mjs` asserts the
 * ceiling doubles and then stops doubling, which a stopwatch cannot.
 *
 * The CAP is what keeps a long outage finite: `base * 2 ** 40` is a number of
 * milliseconds no outage outlives, and `2 ** 1024` is `Infinity` outright, so a
 * formula without the cap hands `setTimeout` a value it cannot honour. The
 * exponent is additionally clamped at 30, which the cap already covers for every
 * cap in use — it is there so the arithmetic stays finite rather than relying on
 * `Math.min` to absorb an `Infinity`. The guard drives a thousandth attempt and
 * asserts the cap comes back.
 *
 * Complexity: O(1).
 *
 * @param {number} attempt zero-based retry number
 * @param {{baseDelayMs?: number, maxDelayMs?: number, random?: () => number}} [options]
 * @returns {number} milliseconds
 */
export function backoffDelay(attempt, options = {}) {
  const base = Number(options.baseDelayMs ?? 500);
  const cap = Number(options.maxDelayMs ?? 30_000);
  const random = options.random ?? Math.random;
  const n = Math.max(0, Math.min(30, Math.trunc(Number(attempt) || 0)));
  const ceiling = Math.max(0, Math.min(cap, base * 2 ** n));
  return Math.floor(random() * ceiling);
}

/** The room endpoint a host supplied, or `null` for anything that is not one.
 *
 * `null` and not a thrown refusal, because the absence of a room is the normal
 * case: `152` §2a mode 1 is a document opened from a file, which needs no server
 * and never will. A caller that gets `null` opens standalone, and no control is
 * disabled and no sentence is shown on that account.
 *
 * ONLY `ws:` AND `wss:`. A host hands this in on the URL, as it hands in the
 * grant, and the reason that is safe is written down in `session_access.mjs`:
 * what arrives from this side can only ever narrow, the relay keeps its own copy
 * of the verified grant, and this editor holds no secret to leak to whoever
 * answers. What it must still not do is treat an arbitrary scheme as a socket —
 * `javascript:` and `data:` are refused here rather than handed to a
 * constructor, and a bare `http:` URL is refused too because it is a mistake
 * rather than an attack and silently upgrading it would hide the host's bug.
 *
 * Complexity: O(1), once per open.
 *
 * @param {unknown} raw the `room` field of `hostConfig()`
 * @returns {string|null}
 */
export function roomUrl(raw) {
  if (typeof raw !== "string" || raw.trim() === "") return null;
  let parsed;
  try {
    parsed = new URL(raw.trim());
  } catch {
    return null;
  }
  return parsed.protocol === "ws:" || parsed.protocol === "wss:" ? parsed.href : null;
}

/** A `connect(url)` over a `WebSocket` constructor, with binary frames.
 *
 *  Takes the constructor as an ARGUMENT rather than reaching for the global one,
 *  which is the only reason this module stays unit-testable (and the only reason
 *  it can live in `PURE_MODULES`). `binaryType` is set here and not left to the
 *  caller because the engine's codec takes bytes: a `Blob`, the browser's
 *  default, would arrive as an object this module would have to read
 *  asynchronously, and a frame read out of order is a frame the length-prefixed
 *  framing cannot resynchronise on.
 *
 *  Complexity: O(1).
 *
 * @param {new (url: string) => object} WebSocketImpl
 * @returns {(url: string) => object}
 */
export function webSocketOpener(WebSocketImpl) {
  return (url) => {
    const socket = new WebSocketImpl(url);
    socket.binaryType = "arraybuffer";
    return socket;
  };
}

/** Whatever a socket delivered, as bytes — or `null` for something that cannot
 *  be a frame.
 *
 *  `null` rather than a coerced value: a text frame on a binary protocol is not
 *  a frame with a problem, it is evidence the other end is not the relay, and
 *  feeding the codec a transcoded string would answer with `ODC-7007` and blame
 *  the message. */
function asBytes(data) {
  if (data instanceof Uint8Array) return data;
  if (data && typeof data === "object" && typeof data.byteLength === "number") {
    return data.buffer
      ? new Uint8Array(data.buffer, data.byteOffset ?? 0, data.byteLength)
      : new Uint8Array(data);
  }
  return null;
}

/** The key this tab is recognised by on its next connection, remembered for the
 * tab's life.
 *
 * `152` §5.5: a resume is honoured only when the key is known AND the identity it
 * was issued to is the one presenting it, which reduces a key from something that
 * authorises to a **disambiguator** — and ADR-063 records what happens to a client
 * that does not keep one. It is handed a fresh `Welcome` and a new `ClientId`, and
 * whatever it had not had acknowledged is gone **with no refusal naming the
 * loss**. That is the only reason this exists: the key is not a convenience.
 *
 * `sessionStorage` and not `localStorage`, and that is the whole decision. The key
 * must survive a RELOAD of this tab — the case a resume is for — and must NOT be
 * shared with a second tab on the same document, because two connections
 * presenting one key are two clients claiming to be one, and the relay would hand
 * the second one the first one's position. `sessionStorage` is per tab and
 * survives a reload; `localStorage` is neither.
 *
 * Both the storage and the mint are INJECTED, so the fallback path — a private
 * window, blocked site data, a storage accessor that throws — is drivable in node.
 * A tab that cannot remember a key still gets one: it simply cannot resume, which
 * is worse than resuming and far better than refusing to open.
 *
 * Complexity: O(1), once per tab.
 *
 * @param {{getItem: (k: string) => string|null, setItem: (k: string, v: string) => void}|null} storage
 * @param {() => string} mint a fresh opaque id
 * @returns {string}
 */
export function resumeKey(storage, mint) {
  const NAME = "opendoc.collab.resumeKey";
  try {
    const held = storage?.getItem(NAME);
    if (typeof held === "string" && held !== "") return held;
  } catch {
    // Blocked site data. Fall through and mint one that lives for this load.
  }
  const fresh = String(mint());
  try {
    storage?.setItem(NAME, fresh);
  } catch {
    // Nothing to do: the key works for this connection and cannot be resumed from.
  }
  return fresh;
}

/**
 * Opens the shared session a host configured, or returns `null` when there is no
 * room.
 *
 * The composition `main.js` would otherwise have to write inline, kept here for
 * the reason every other decision in this module is: `main.js` has no seam a test
 * can reach (`109` HF-085), so a policy written there is a policy nothing checks.
 * What stays in `main.js` is only what it alone can do — paint.
 *
 * `null` for the standalone mode is the important half of the signature. `152`
 * §2a mode 1 is a document opened from a file; it needs no server and never will,
 * so the caller's `collab` handle is simply absent and no control is disabled and
 * no sentence is shown. `collab?.stop()` on the next open is then correct without
 * a branch.
 *
 * It `start()`s before returning, because a transport that was configured and not
 * started is the "built and unreachable" failure one layer smaller.
 *
 * Complexity: O(1).
 *
 * @param {object} session the engine handle (a `WasmDocument`)
 * @param {unknown} room the `room` field of `hostConfig()`
 * @param {(new (url: string) => object)|undefined} WebSocketImpl the host's constructor
 * @param {{identity?: string, resumeKey?: string, onOutcome?: (o: object) => void,
 *          onState?: (s: object) => void}} hooks
 * @returns {object|null}
 */
export function openRoom(session, room, WebSocketImpl, hooks = {}) {
  const url = roomUrl(room);
  if (url === null || !session || typeof WebSocketImpl !== "function") return null;
  const pipe = collabTransport({
    session,
    url,
    // A missing host name must not become an EMPTY one: the engine refuses
    // `Identity::new("")`, so passing it through would turn a forgotten host
    // input into a terminal stop at the handshake, refusing about the wrong
    // thing in a place the reader cannot connect to its cause.
    identity: hooks.identity || "anonymous",
    // The key is NOT defaulted the same way, and the asymmetry is the point. A
    // key invented per call would connect and then fail to resume — silently,
    // which is the loss ADR-063 records: a fresh `Welcome`, a new `ClientId`, and
    // unacknowledged work gone with no refusal naming it. So a caller that forgot
    // it gets the engine's own `session.resume-key-unusable` on `state().problem`,
    // which is a sentence pointing at the real mistake.
    resumeKey: typeof hooks.resumeKey === "string" ? hooks.resumeKey : "",
    open: webSocketOpener(WebSocketImpl),
    onOutcome: hooks.onOutcome,
    onState: hooks.onState,
  });
  pipe.start();
  return pipe;
}

/**
 * The browser's end of one shared session.
 *
 * `session` is the engine handle, used through five members and nothing else:
 * `collabJoinFrame`, `collabReceiveFrame`, `collabNextChunk`, `collabLeaveFrame`
 * and the `collabState` getter. That list is the whole contract, and it is why
 * this module can be driven by a stand-in in node.
 *
 * Nothing happens until `start()`. Opening a socket as a side effect of
 * construction is how a page ends up connected before its chrome can show that
 * it is.
 *
 * Complexity: O(1) per frame in each direction, and O(queued) to drain — never
 * O(document). The per-keystroke wire cost is the engine's (`107` §4 B1), and
 * `a_keystrokes_wire_cost_grows_with_neither_the_document_nor_the_session`
 * guards it there.
 *
 * @param {{
 *   session: object,
 *   url: string,
 *   identity: string,
 *   resumeKey: string,
 *   open: (url: string) => object,
 *   setTimer?: (fn: () => void, ms: number) => unknown,
 *   clearTimer?: (handle: unknown) => void,
 *   random?: () => number,
 *   onOutcome?: (outcome: object) => void,
 *   onState?: (state: object) => void,
 *   baseDelayMs?: number,
 *   maxDelayMs?: number,
 *   attemptCeiling?: number,
 * }} options
 */
export function collabTransport(options) {
  const {
    session,
    url,
    identity,
    resumeKey,
    open,
    setTimer = setTimeout,
    clearTimer = clearTimeout,
    random = Math.random,
    onOutcome = () => {},
    onState = () => {},
    baseDelayMs = 500,
    maxDelayMs = 30_000,
    attemptCeiling = 8,
  } = options ?? {};

  /** `idle` → `connecting` → `open`, and `waiting` between attempts. `stopped`
   *  is the only one nothing leaves on its own. */
  let phase = "idle";
  let socket = null;
  let timer = null;
  let attempt = 0;
  /** The code the reader's sentence is keyed by, or `null` when there is nothing
   *  to explain. Carried on the STATE and not thrown, because a lost connection
   *  is a condition the chrome displays, not an exception it catches. */
  let code = null;
  /** The engine's own refusal sentence, when the session could not even be
   *  opened. Kept verbatim: it is more specific than anything routed. */
  let problem = "";
  /** Frames the engine has handed over and the wire has not taken. See the
   *  header: this is custody, not a cache. */
  let queue = [];
  let reported = null;

  const name = () =>
    phase === "open" ? "connected" : phase === "stopped" ? "stopped" : "reconnecting";

  /** The state as the chrome reads it. Always every field, so a caller
   *  destructuring it cannot end up with `undefined` where a key belongs. */
  function state() {
    const which = name();
    return Object.freeze({
      name: which,
      key: CONNECTION_STATE_KEYS[which],
      code,
      problem,
      attempt,
      queued: queue.length,
      revision: revisionNow(),
    });
  }

  /** Announces a state the reader would describe differently.
   *
   *  On a change of NAME or CODE only. The queue length and the attempt number
   *  move constantly and change no sentence, so notifying on them would turn one
   *  status line into a flicker; a caller that wants them pulls `state()`. */
  function report() {
    const next = state();
    if (reported && reported.name === next.name && reported.code === next.code) return;
    reported = next;
    onState(next);
  }

  /** The ordered position to resume from, asked of the engine rather than
   *  counted here, so there is one answer. `0` when the session has not joined
   *  — which is a first join, and the relay reads it as one. */
  function revisionNow() {
    try {
      const parsed = JSON.parse(session.collabState);
      const value = Number(parsed?.revision);
      return Number.isSafeInteger(value) && value >= 0 ? value : 0;
    } catch {
      return 0;
    }
  }

  /** One write, with custody kept on failure. `false` means the socket is going
   *  away; `onclose` follows and does the retrying, so this never schedules. */
  function sendNow(bytes) {
    try {
      socket.send(bytes);
      return true;
    } catch {
      return false;
    }
  }

  /** Hands the queue to the wire in order, stopping at the first refusal. The
   *  order is the protocol: sequence numbers are monotonic per client. */
  function drain() {
    while (phase === "open" && queue.length > 0) {
      if (!sendNow(queue[0])) return;
      queue.shift();
    }
  }

  /** Takes everything the engine is willing to offer, then writes it.
   *
   *  Guarded on `open` because a poll is a handover (see the header). Terminates:
   *  `flush` returns `None` once `MAX_OUTSTANDING` chunks are outstanding, and
   *  every chunk it does return consumes at least one commit. */
  function pump() {
    if (phase !== "open") return;
    for (;;) {
      let chunk;
      try {
        chunk = session.collabNextChunk();
      } catch {
        return;
      }
      if (!chunk || chunk.length === 0) break;
      queue.push(chunk);
    }
    drain();
  }

  /** One frame from the relay, fed to the session, and whatever it decided
   *  handed to the host.
   *
   *  A REFUSAL IS NOT AN ERROR here either: the connection survives one, and the
   *  engine has already rebased, so the right answer to a non-terminal refusal is
   *  to pump again and let it resubmit. Only `terminal` stops the session. */
  function receive(data) {
    const bytes = asBytes(data);
    if (bytes === null) {
      onOutcome(Object.freeze({ kind: "error", code: null, message: "" }));
      return;
    }
    let text;
    try {
      text = session.collabReceiveFrame(bytes);
    } catch (error) {
      // The engine's own sentence, kept verbatim — it names the frame's problem,
      // which nothing here could.
      onOutcome(
        Object.freeze({ kind: "error", code: null, message: String(error?.message ?? error) }),
      );
      return;
    }
    let outcome;
    try {
      outcome = JSON.parse(text);
    } catch {
      return;
    }
    onOutcome(Object.freeze(outcome));
    if (outcome?.terminal === true) {
      halt(outcome.code ?? null, false);
      return;
    }
    pump();
  }

  /** The socket went away: schedule the next attempt, or give up and say so.
   *
   *  This is the path an EVICTION arrives on, and the path a cable arrives on,
   *  and they are the same path. See the header. */
  function dropped() {
    socket = null;
    if (phase === "stopped" || phase === "idle") return;
    attempt += 1;
    code = CONNECTION_LOST;
    if (attempt > attemptCeiling) {
      // Not a dead end: `reconnect()` resets the count, and the chrome offers it
      // beside the sentence. A transport that stops forever with no way back is
      // the dead control `SKILL` §10 forbids, one layer down.
      phase = "stopped";
      report();
      return;
    }
    phase = "waiting";
    report();
    timer = setTimer(() => {
      timer = null;
      connect();
    }, backoffDelay(attempt - 1, { baseDelayMs, maxDelayMs, random }));
  }

  /** Opens a socket, unless one is already live or already scheduled.
   *
   *  ONE GATE, HERE, and not a condition at each call site. Three callers reach
   *  this — `start`, `reconnect` and the retry timer — so a check at the call
   *  sites would be three copies of one rule, and the timer's copy would
   *  necessarily differ from the other two. A gate split in two is also a gate
   *  whose halves cannot both be driven red: the first draft checked `idle` in
   *  `start` *and* the live phases here, and the mutation that removed the
   *  second copy left every guard green, because nothing could reach it.
   *
   *  The retry timer clears its own handle before calling, so a scheduled retry
   *  arrives here as `waiting` with no timer and proceeds, while a `start()`
   *  during the same wait is refused — the scheduled attempt already owns the
   *  next socket. */
  function connect() {
    if (phase === "stopped" || phase === "open" || phase === "connecting") return;
    if (phase === "waiting" && timer !== null) return;
    phase = "connecting";
    report();
    try {
      socket = open(url);
    } catch (error) {
      socket = null;
      problem = String(error?.message ?? error);
      dropped();
      return;
    }
    socket.onopen = opened;
    socket.onmessage = (event) => receive(event?.data);
    socket.onclose = dropped;
    // A socket error is always followed by a close, in every browser and in the
    // specification, so there is nothing to do here that `dropped` does not
    // already do — and doing it twice would double-count the attempt and halve
    // every backoff.
    socket.onerror = () => {};
  }

  /** The handshake: a `Join` first and nothing before it, then the custody
   *  queue, then anything new.
   *
   *  The join is written DIRECTLY rather than queued, because the queue may hold
   *  chunks from the previous connection and a `Join` behind them is a `Join` the
   *  relay refuses; and because a join that failed to write must not be left at
   *  the head of the queue, where the next connection would send two. */
  function opened() {
    phase = "open";
    attempt = 0;
    code = null;
    let join;
    try {
      join = session.collabJoinFrame(identity, resumeKey, revisionNow());
    } catch (error) {
      // An identity or a key the engine will not take is terminal and is the
      // host's to fix: retrying cannot change the answer, so this does not.
      problem = String(error?.message ?? error);
      halt(null, false);
      return;
    }
    if (!sendNow(join)) return;
    drain();
    pump();
    report();
  }

  /** Ends the session. `leave` writes a `Leave` first, which is worth sending on
   *  a deliberate departure and pointless after a terminal refusal — the relay
   *  has already stopped this client. */
  function halt(reason, leave) {
    if (timer !== null) {
      clearTimer(timer);
      timer = null;
    }
    if (leave && phase === "open" && socket) {
      try {
        sendNow(session.collabLeaveFrame());
      } catch {
        // A leave that cannot be written changes nothing: the relay notices the
        // close. It is an optimisation over the operating system's timeout.
      }
    }
    phase = "stopped";
    code = reason;
    if (socket) {
      try {
        socket.close();
      } catch {
        // Already closing.
      }
      socket = null;
    }
    report();
  }

  return Object.freeze({
    /** Opens the first connection. Idempotent in every phase, because
     *  [`connect`] is the one gate: a chrome that re-runs its boot path does not
     *  end up with two sockets on one session, and a `start()` during a retry
     *  wait does not pre-empt the attempt that is already scheduled. */
    start() {
      connect();
    },
    /** Leaves deliberately. Terminal — `reconnect()` is how a reader comes back,
     *  and it is a gesture rather than a timer. */
    stop() {
      halt(null, true);
    },
    /** Tries again after the retries were exhausted, or after a `stop()`.
     *
     *  Resets the attempt count, which is the whole point: a reader who asks is
     *  not asking for the ninth attempt's backoff. The custody queue is kept —
     *  those frames are still the only copy. */
    reconnect() {
      if (phase !== "stopped") return;
      phase = "idle";
      attempt = 0;
      code = null;
      problem = "";
      connect();
    },
    /** Offers whatever the engine has produced since the last frame. The chrome
     *  calls this after a local edit; it is a poll and not an event, for the
     *  reason `collabNextChunk` gives — the common answer is "nothing". */
    flush() {
      pump();
    },
    /** The reader-facing state, pulled. O(1). */
    state,
  });
}
