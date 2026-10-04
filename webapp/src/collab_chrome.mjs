/** The reader's control over a shared session.
 *
 *  `collab_transport.mjs` is pure and stays that way (`module_seams`'s
 *  `PURE_MODULES`): it has had a `reconnect()` since the transport landed, and
 *  nothing could reach it. A transport that stops with no way back is the dead
 *  control `SKILL.md` §10 forbids, so this module is the controller half — the
 *  same split as `fold_view.mjs` (decisions) and `fold_chrome.mjs` (the engine
 *  and the rows).
 *
 *  One row, generated here rather than written into `main.js`, for the reason
 *  the command registry's own comment gives: a module that generates its rows
 *  cannot offer the palette and the menu different sets.
 */

/** The shared-session command rows.
 *
 *  O(1). `transport` is read on every call rather than captured, because the
 *  room is opened per DOCUMENT — a captured handle would be the previous file's
 *  pipe after the next open.
 *
 *  @param {object} o
 *  @param {() => {state: () => {name: string}, reconnect: () => void} | null} o.transport
 *  @param {(key: string) => string} o.t
 */
export function collabCommands({ transport, t }) {
  const pipe = transport();
  const name = pipe ? pipe.state().name : "";
  // Enabled ONLY in `stopped`, and that is not a guess about intent: the pipe's
  // own `reconnect()` returns early in every other phase, so offering the row
  // while connected or mid-retry would be a control that does nothing — the dead
  // control §10 forbids, wearing an enabled label.
  //
  // Three states, three sentences. "There is no shared document", "you are
  // already sharing" and "it is already trying" are different facts, and a
  // reader who reached for Reconnect deserves the one that applies to them
  // rather than a shared "unavailable".
  const reason =
    !pipe
      ? t("collab.standalone")
      : name === "connected"
        ? t("collab.connected")
        : name === "stopped"
          ? ""
          : t("collab.reconnecting");
  return [
    {
      id: "collab.reconnect",
      label: t("collab.reconnect"),
      group: "Review",
      kw: "reconnect connect connection share shared session collaborate offline retry rejoin",
      enabled: reason === "",
      disabledReason: reason,
      run: () => transport()?.reconnect(),
    },
  ];
}
