import assert from "node:assert/strict";
import test from "node:test";
import { STUCK_AFTER_MS, createSyncIndicator, syncState } from "../src/collab_sync.mjs";

// `docs/109` HF-313: the status said "your changes are reaching everyone" while the
// relay was frozen, because it reported the SOCKET. These guards hold the indicator
// to the engine's account of the last change instead: unsent, sent, acknowledged,
// and stuck once the acknowledgement does not come.

const at = (overrides) => syncState({ connection: "connected", unsent: 0, inFlight: 0, since: null, now: 0, ...overrides });

test("only an acknowledged change is described as reaching everyone", () => {
  assert.equal(at({}).name, "acknowledged");
  assert.equal(at({}).key, "collab.connected");
  assert.equal(at({ unsent: 1, since: 0 }).name, "pending");
  assert.equal(at({ inFlight: 1, since: 0 }).name, "sent");
  for (const busy of [{ unsent: 1 }, { inFlight: 2 }]) {
    assert.notEqual(at({ ...busy, since: 0 }).key, "collab.connected", "a change still in flight is not on its way to everyone");
  }
});

test("a change that waits past the threshold is called stuck, with how long", () => {
  const stuck = at({ inFlight: 1, since: 0, now: STUCK_AFTER_MS + 2500 });
  assert.equal(stuck.name, "stuck");
  assert.equal(stuck.kind, "warn");
  assert.equal(stuck.seconds, Math.floor((STUCK_AFTER_MS + 2500) / 1000));
  assert.equal(at({ inFlight: 1, since: 0, now: STUCK_AFTER_MS - 1 }).name, "sent");
});

test("a lost connection is the transport's sentence, whatever the engine holds", () => {
  assert.equal(at({ connection: "reconnecting", unsent: 3 }).key, "collab.reconnecting");
  assert.equal(at({ connection: "stopped" }).key, "collab.stopped");
  assert.equal(syncState({ connection: null, unsent: 0, inFlight: 0, since: null, now: 0 }), null);
});

/** A frozen relay: the socket stays open, the chunk is never acknowledged. */
function frozenRoom() {
  let clock = 0;
  let pending = { unsent: 0, inFlight: 0 };
  const timers = [];
  const said = [];
  const chip = { hidden: true, textContent: "", dataset: {}, setAttribute() {}, after() {} };
  const root = {
    getElementById: (id) => (id === "documentState" ? { parentElement: {}, after: () => {} } : null),
    createElement: () => chip,
  };
  const indicator = createSyncIndicator({
    transport: () => ({ state: () => ({ name: "connected", queued: 0 }) }),
    document: () => ({ get collabState() { return JSON.stringify(pending); } }),
    status: (text, kind) => said.push([text, kind]),
    t: (key, values) => `${key}:${values?.seconds ?? 0}`,
    root,
    now: () => clock,
    setTimer: (fn) => { timers.push(fn); return timers.length; },
    clearTimer: () => {},
  });
  return {
    indicator, chip, said,
    set: (value) => { pending = value; },
    advance(ms) { clock += ms; const fn = timers.pop(); fn?.(); },
  };
}

test("with the relay frozen the chip moves from sent to stuck without any frame arriving", () => {
  const room = frozenRoom();
  room.set({ unsent: 0, inFlight: 1 });
  room.indicator.update();
  assert.equal(room.chip.dataset.state, "sent");
  for (let i = 0; i < 5; i += 1) room.advance(1000);
  assert.equal(room.chip.dataset.state, "stuck", "five seconds unacknowledged must be said");
  assert.match(room.chip.textContent, /^collab\.syncStuck:5$/);
  assert.deepEqual(room.said.at(-1), ["collab.syncStuck:3", "warn"], "the status line is told once, when it first sticks");
  room.set({ unsent: 0, inFlight: 0 });
  room.indicator.update();
  assert.equal(room.chip.dataset.state, "acknowledged");
  assert.equal(room.said.at(-1)[1], "", "and told again when the trouble is over");
});

test("a quiet room schedules nothing", () => {
  const room = frozenRoom();
  room.indicator.update();
  room.advance(0);
  assert.equal(room.chip.dataset.state, "acknowledged");
  assert.equal(room.said.length, 0);
});
