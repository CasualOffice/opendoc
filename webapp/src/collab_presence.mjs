// Presence is ephemeral. The engine owns payloads, clock suppression and geometry;
// the host owns scheduling and paint. No wire vocabulary or document mutation here.
//
// What a participant LOOKS like — name, colour, the ink on it — is one seam,
// `participantDisplay` (`presence_strip.mjs`), read by the caret label here and by
// the header's avatar strip, so a caret and its chip cannot disagree (HF-330).
// Both are fed the same roster rows (`{ client, profile?, attested }`, `docs/175`
// §4.3 and §14) and the same mount avatar policy, and both run `displayRoster`
// over the same people — this tab included — so a duplicated name carries the
// same "(2)" on a caret as on its chip.
import { displayRoster, participantDisplay } from "./presence_strip.mjs";

/** Outcomes that change who is in the room or where they are, and so repaint the
 *  carets and refresh the people strip. A join or a leave is as much a change of
 *  the room as a caret moving (`docs/175` §4.3). */
export const PRESENCE_OUTCOMES = Object.freeze(["awareness", "departed", "welcome", "resumed",
  "participantJoined", "participantLeft"]);

/** One roster row as the seam takes it. `profile` is passed through by
 *  reference and never mutated: `displayRoster` builds its suffixed label on the
 *  VIEW, and the profile stays exactly what the relay said. */
function row(value) {
  const out = { client: value.client, attested: value.attested === true };
  if (value.profile && typeof value.profile === "object") out.profile = value.profile;
  return out;
}

export function createPresence({ newRoster, document, transport, selection, paint, t,
  setTimer = setTimeout, clearTimer = clearTimeout, roster: rosterChanged = () => {},
  display = participantDisplay, avatars = "never" }) {
  let roster = null;
  let timer = null;
  /** This tab's own participant number, or null before the room has welcomed it. */
  function mine() {
    try {
      const n = JSON.parse(document()?.collabState ?? "{}").participant;
      return Number.isSafeInteger(n) ? n : null;
    } catch { return null; }
  }
  /** This tab as a seam input: its number, and the profile the relay will attach
   *  to its work (`roster.me`, `docs/175` §4.3), when the room has one. */
  function self() {
    let me = null;
    try { me = roster ? JSON.parse(roster.me ?? "null") : null; } catch { me = null; }
    const client = Number.isSafeInteger(me?.client) ? me.client : mine();
    return me && typeof me === "object" ? row({ ...me, client }) : { client, attested: false };
  }
  /** Everyone the roster holds, this tab excluded, in participant-number order, as
   *  seam rows `{ client, profile?, attested }`. From the roster's membership, not
   *  from caret geometry: a participant whose caret is on a page that is not laid
   *  out yet — or who has not sent a caret at all — is still in the room.
   *  O(participants). */
  function participants() {
    if (!roster) return [];
    let held = [];
    try { held = JSON.parse(roster.participants); } catch { return []; }
    if (!Array.isArray(held)) return [];
    const me = self().client;
    return held.filter((p) => Number.isSafeInteger(p?.client) && p.client !== me)
      .map(row).sort((a, b) => a.client - b.client);
  }
  function told(reconnected = false) {
    rosterChanged(roster ? { self: self(), others: participants(), reconnected } : null);
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
  /** Every participant's view, keyed by participant number, from the same rows
   *  and the same roster-wide duplicate pass the strip draws. O(participants). */
  function views() {
    const me = self();
    const everyone = [{ ...me, isSelf: true }, ...participants().map((p) => ({ ...p, isSelf: false }))];
    const drawn = displayRoster(everyone, { t, display, avatars });
    return new Map(drawn.filter((view) => view.client !== null).map((view) => [view.client, view]));
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
    self,
    views,
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
      const me = self().client;
      const remotes = JSON.parse(roster.remoteSelections(doc)).filter((remote) => remote.client !== me);
      const known = views();
      for (const remote of remotes) {
        // A caret from somebody the membership list does not hold yet (a frame
        // ordering race) still gets the seam's numbered view, never a blank.
        const view = known.get(remote.client) ?? display({ client: remote.client }, { t, avatars });
        const color = view.color;
        for (let i = 0; i < remote.selection.length; i += 5) {
          const el = place(remote.selection.slice(i, i + 5), "remote-selection");
          if (el) el.style.backgroundColor = color;
        }
        const el = paintCaretFlag(place, remote.caret, view, t);
        if (el) el.dataset.participant = String(remote.client);
      }
    },
  };
}

/**
 * One participant's caret and name flag on a page overlay: the room's carets
 * and Casual AI's (`ai_coeditor.mjs`, a participant that is not a room member)
 * are drawn by this one function, so the AI's flag cannot drift from what a
 * person's looks like. `rect` is the engine's flat `[page, x, y, w, h]`; `view`
 * is `participantDisplay`'s. Returns the caret element, or null when its page is
 * not on screen. O(1).
 */
export function paintCaretFlag(place, rect, view, t) {
  const el = place(rect, "remote-caret");
  if (!el) return null;
  el.style.borderColor = view.color;
  el.dataset.verified = String(view.verified);
  const label = el.ownerDocument.createElement("span");
  label.textContent = view.label;
  // A CLAIMED name is marked unverified on the caret as on the chip
  // (`docs/175` §4.2): the tooltip says so in words, and the class lets
  // the label read as a claim.
  if (!view.numbered && !view.verified) {
    label.title = t("collab.presence.unverified", { name: view.label });
    label.classList.add("is-unverified");
  }
  label.style.backgroundColor = view.color;
  // White alone measured 2.6:1 on the palette's yellows; the seam picks the ink.
  label.style.color = view.ink;
  el.append(label);
  return el;
}
