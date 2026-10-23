// SPDX-License-Identifier: Apache-2.0
// The shared document's sync indicator: what has become of the reader's last change.
//
// ---- WHY THIS EXISTS (`docs/109` HF-313) ------------------------------------
//
// The status used to be the SOCKET's state. "Shared — your changes are reaching
// everyone" was shown whenever the connection was open, so with the relay frozen
// for five seconds it went on saying so about edits nobody had received. An open
// socket says nothing about the last keystroke. What does is the engine's own
// account of it, which `collabState` reports:
//
//   unsent    typed, not yet handed to the transport;
//   inFlight  handed over, not yet acknowledged by the relay;
//   neither   ordered — on its way to everyone, which is the only state in which
//             the old sentence is true.
//
// ---- THE PATTERN, NAMED (SKILL §8) -------------------------------------------
//
// An ACKNOWLEDGEMENT-DRIVEN SAVE INDICATOR: Google Docs' "Saving…" / "All changes
// saved" and ONLYOFFICE's "Saving…" both follow the server's confirmation, not the
// connection, and both escalate when the confirmation does not come. So does this:
// once a change has waited STUCK_AFTER_MS it says so, with how long, and that the
// change is kept on this device (it is: the engine holds every unacknowledged
// commit, and the transport keeps custody of every frame it took).
//
// ---- WHERE IT IS SHOWN, AND WHY NOT IN THE STATUS LINE -------------------------
//
// A chip beside the document-state chip, with its own polite live region. The
// status line is shared by every refusal in the editor; an indicator that rewrote
// it on every acknowledgement would erase "Undo cannot reverse…" a few
// milliseconds after it appeared. The status line is still told about the two
// things a reader must not miss: a lost connection (the transport's sentences, as
// before) and a change that is stuck.
//
// Pure apart from the one element it owns; the clock and timers are injected so
// `collab_sync.test.mjs` can drive "stuck for 5 s" without waiting five seconds.

import { CONNECTION_STATE_KEYS } from "./collab_transport.mjs";

/** How long a change may wait for its acknowledgement before the reader is told. */
export const STUCK_AFTER_MS = 3000;

/** The indicator's state for one reading of the transport and the engine.
 *
 *  O(1). `since` is when the oldest still-pending change was first seen.
 *
 * @param {{connection: string|null, unsent: number, inFlight: number,
 *          since: number|null, now: number, stuckAfterMs?: number}} reading
 * @returns {{name: string, key: string, kind: string, seconds?: number}|null}
 */
export function syncState({ connection, unsent, inFlight, since, now, stuckAfterMs = STUCK_AFTER_MS }) {
  if (!connection) return null;
  if (connection !== "connected") {
    return { name: connection, key: CONNECTION_STATE_KEYS[connection], kind: "warn" };
  }
  if (unsent + inFlight === 0) return { name: "acknowledged", key: "collab.connected", kind: "" };
  const waited = since === null ? 0 : Math.max(0, now - since);
  if (waited >= stuckAfterMs) {
    // Whole seconds, in steps of one: the chip is a polite live region, and it is
    // re-announced only when its words change.
    return { name: "stuck", key: "collab.syncStuck", kind: "warn", seconds: Math.floor(waited / 1000) };
  }
  return inFlight > 0
    ? { name: "sent", key: "collab.syncSent", kind: "" }
    : { name: "pending", key: "collab.syncPending", kind: "" };
}

/**
 * The indicator over a live transport and engine.
 *
 * @param {object} o
 * @param {() => ({state: () => {name: string, queued: number}})|null} o.transport
 * @param {() => ({collabState: string})|null} o.document
 * @param {(text: string, kind?: string) => void} o.status the editor's status line
 * @param {(key: string, values?: object) => string} o.t
 * @param {Document} [o.root]
 */
export function createSyncIndicator({ transport, document: getDocument, status, t, root = globalThis.document,
  now = () => Date.now(), setTimer = setTimeout, clearTimer = clearTimeout, stuckAfterMs = STUCK_AFTER_MS }) {
  let since = null;
  let timer = null;
  let shown = "";
  let warned = "";
  let chip = null;

  function element() {
    if (chip || !root?.getElementById) return chip;
    const anchor = root.getElementById("documentState");
    if (!anchor?.parentElement) return null;
    chip = root.createElement("span");
    chip.id = "collabSync";
    chip.className = "document-state";
    chip.setAttribute("role", "status");
    chip.setAttribute("aria-live", "polite");
    chip.hidden = true;
    anchor.after(chip);
    return chip;
  }

  /** Re-reads the transport and the engine and shows what they say. O(1). */
  function update() {
    const pipe = transport();
    const doc = getDocument();
    if (!pipe || !doc) return stop();
    let engine = {};
    try { engine = JSON.parse(doc.collabState); } catch { /* an unreadable state shows nothing new */ }
    const connection = pipe.state();
    // Frames the transport holds in custody were taken from the engine and not yet
    // written: still unsent from the reader's point of view.
    const unsent = Number(engine.unsent || 0) + Number(connection.queued || 0);
    const inFlight = Number(engine.inFlight || 0);
    if (unsent + inFlight > 0) since ??= now();
    else since = null;
    const state = syncState({ connection: connection.name, unsent, inFlight, since, now: now(), stuckAfterMs });
    const el = element();
    if (state && el) {
      const text = t(state.key, { seconds: state.seconds ?? 0 });
      el.hidden = false;
      el.dataset.state = state.name;
      if (text !== shown) { el.textContent = text; el.title = text; shown = text; }
      // The status line hears about the states a reader must not miss, once each,
      // and once more when the trouble is over — so "Connection lost" does not
      // outlive the reconnection that ended it.
      const loud = state.kind === "warn" ? state.key : "";
      if (loud !== warned) status(text, state.kind);
      warned = loud;
    }
    // Tick only while something is waiting, so "stuck" can arrive without an event —
    // the frozen relay sends none. Nothing ticks in a quiet room.
    if (timer !== null) clearTimer(timer);
    timer = since !== null && connection.name === "connected" ? setTimer(() => { timer = null; update(); }, 1000) : null;
  }

  function stop() {
    if (timer !== null) clearTimer(timer);
    timer = null;
    since = null;
    shown = "";
    warned = "";
    if (chip) { chip.hidden = true; chip.textContent = ""; delete chip.dataset.state; }
  }

  return { update, stop };
}
