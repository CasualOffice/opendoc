// Presence is ephemeral. The engine owns payloads, clock suppression and geometry;
// the host owns scheduling and paint. No wire vocabulary or document mutation here.
//
// What a participant LOOKS like — name, colour, the ink on it — is one seam,
// `participantDisplay` (`presence_strip.mjs`), read by the caret label here and by
// the header's avatar strip, so a caret and its chip cannot disagree (HF-330).
import { displayRoster, participantDisplay } from "./presence_strip.mjs";

export function createPresence({ newRoster, document, transport, selection, paint, t,
  setTimer = setTimeout, clearTimer = clearTimeout, roster: rosterChanged = () => {},
  display = participantDisplay }) {
  let roster = null;
  let timer = null;
  /** This tab's own participant number, or null before the room has welcomed it. */
  function mine() {
    try {
      const n = JSON.parse(document()?.collabState ?? "{}").participant;
      return Number.isSafeInteger(n) ? n : null;
    } catch { return null; }
  }
  /** Everyone the roster holds, this tab excluded, in participant-number order.
   *  From the roster's membership, not from caret geometry: a participant whose
   *  caret is on a page that is not laid out yet is still in the room. O(participants). */
  function participants() {
    if (!roster) return [];
    let held = [];
    try { held = JSON.parse(roster.participants); } catch { return []; }
    const self = mine();
    return held.map((row) => row.client).filter((c) => Number.isSafeInteger(c) && c !== self);
  }
  function told(reconnected = false) {
    rosterChanged(roster ? { self: mine(), others: participants(), reconnected } : null);
  }
  function sync() {
    const doc = document();
    if (roster && doc) roster.remoteSelections(doc);
  }
  function changed() {
    if (!roster || timer !== null) return;
    timer = setTimer(() => {
      timer = null;
      const doc = document(), pipe = transport(), range = selection();
      if (!doc || !range || pipe?.state().name !== "connected") return;
      try {
        const frame = roster.caretFrame(doc, range.anchor.node, range.anchor.offset,
          range.focus.node, range.focus.offset);
        if (frame && !pipe.awareness(frame)) roster.clear();
      } catch { /* A missing caret is not a document edit or a session failure. */ }
    }, 100);
  }
  return {
    open(shared) {
      if (timer !== null) clearTimer(timer);
      timer = null;
      roster?.free();
      roster = shared ? newRoster() : null;
      told();
    },
    frame(bytes) { roster?.receiveFrame(bytes); sync(); told(); },
    sync,
    participants,
    mine,
    state(state) {
      // A dropped connection empties the room quietly: the sync chip already says
      // the connection went, and reading every name out as "left" would be noise.
      if (state.name !== "connected") { roster?.clear(); paint(); told(true); }
      else { told(true); changed(); }
    },
    changed,
    paint(place) {
      const doc = document();
      if (!roster || !doc) return;
      const self = mine();
      const remotes = JSON.parse(roster.remoteSelections(doc)).filter((remote) => remote.client !== self);
      // Through the roster view, so a duplicated name carries the same "(2)" here
      // as on its chip. A bounded stable palette, assigned by the relay's identity.
      const views = displayRoster(remotes.map((remote) => ({ client: remote.client })), { t, display });
      for (const [n, remote] of remotes.entries()) {
        const view = views[n];
        const color = view.color;
        for (let i = 0; i < remote.selection.length; i += 5) {
          const el = place(remote.selection.slice(i, i + 5), "remote-selection");
          if (el) el.style.backgroundColor = color;
        }
        const el = place(remote.caret, "remote-caret");
        if (el) {
          el.style.borderColor = color;
          el.dataset.participant = String(remote.client);
          const label = el.ownerDocument.createElement("span");
          label.textContent = view.label;
          label.style.backgroundColor = color;
          // White alone measured 2.6:1 on the palette's yellows; the seam picks the ink.
          label.style.color = view.ink;
          el.append(label);
        }
      }
    },
  };
}
