// Presence is ephemeral. The engine owns payloads, clock suppression and geometry;
// the host owns scheduling and paint. No wire vocabulary or document mutation here.
export function createPresence({ newRoster, document, transport, selection, paint, t,
  setTimer = setTimeout, clearTimer = clearTimeout }) {
  let roster = null;
  let timer = null;
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
    },
    frame(bytes) { roster?.receiveFrame(bytes); sync(); },
    sync,
    state(state) {
      if (state.name !== "connected") { roster?.clear(); paint(); }
      else changed();
    },
    changed,
    paint(place) {
      const doc = document();
      if (!roster || !doc) return;
      const mine = JSON.parse(doc.collabState).participant;
      for (const remote of JSON.parse(roster.remoteSelections(doc))) {
        if (remote.client === mine) continue;
        // A bounded stable palette, assigned by the relay's identity.
        const color = `hsl(${(remote.client * 137.508) % 360} 70% 38%)`;
        for (let i = 0; i < remote.selection.length; i += 5) {
          const el = place(remote.selection.slice(i, i + 5), "remote-selection");
          if (el) el.style.backgroundColor = color;
        }
        const el = place(remote.caret, "remote-caret");
        if (el) {
          el.style.borderColor = color;
          el.dataset.participant = String(remote.client);
          const label = el.ownerDocument.createElement("span");
          label.textContent = t("collab.participant", { number: remote.client + 1 });
          label.style.backgroundColor = color;
          el.append(label);
        }
      }
    },
  };
}
