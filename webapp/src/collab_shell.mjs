import { newRoster } from "../pkg/casual_doc_wasm.js";
import { openRoom, resumeKey } from "./collab_transport.mjs";
import { createRecovery } from "./collab_recovery.mjs";
import { createSyncIndicator } from "./collab_sync.mjs";
import { createShare, shareLink, sharedDocument } from "./share_flow.mjs";
import { createShareChrome } from "./share_chrome.mjs";
import { createPresenceStrip } from "./presence_strip.mjs";
import { createRecoveryDownloader } from "./recovery_chrome.mjs";
import { refusalAnnouncement } from "./session_access.mjs";
import { avatarPolicy, hostIdentity, joinIdentity } from "./host_identity.mjs";
import { PRESENCE_OUTCOMES, createPresence } from "./collab_presence.mjs";
import { mentionsOrdered } from "./mention_notices.mjs";

/** Host-managed sharing, checkpoints, recovery and ephemeral presence scheduling. */
export function createCollaborationShell({ root, getDocument, getTransport, setTransport,
  getSelection, getSettings, getName, hostConfig, paintOverlay, receiveAccess, arrival,
  repaintRecovered, edited, setSelection, status, t }) {
  // The right end of the header's document row: state chips, then the people in the
  // room, then Share (`docs/109` HF-330) — the order Google Docs, Word on the web and
  // ONLYOFFICE share. Built hidden; it names nothing until a room opens, so no label
  // is read before the catalogue has landed (SKILL §11).
  const headerEnd = root.getElementById("documentState").parentElement;
  // One avatar policy for the mount, read once and handed to BOTH painters, so a
  // chip and a caret label cannot disagree about whether a picture may be fetched
  // (`docs/175` §8.1). Default "never".
  const avatars = avatarPolicy(globalThis.location?.search);
  const strip = createPresenceStrip({ root, parent: headerEnd, t, status, goTo: locateParticipant, avatars });
  const remotePresence = createPresence({ newRoster, document: getDocument, avatars,
    transport: getTransport, selection: getSelection, paint: paintOverlay, t,
    roster: (room) => { strip.show(Boolean(room)); if (room) strip.update(room); } });
  /** Scrolls the document to a participant's painted caret and marks it briefly.
   *  False when there is none painted (not laid out yet, or no caret sent yet), which
   *  the strip says out loud rather than doing nothing. O(painted carets). */
  function locateParticipant(client) {
    const caret = root.querySelector(`.remote-caret[data-participant="${Number(client)}"]`);
    if (!caret) return false;
    caret.scrollIntoView({ block: "center", inline: "nearest" });
    caret.classList.remove("is-located");
    void caret.offsetWidth; // restart the mark if the same chip is pressed twice
    caret.classList.add("is-located");
    return true;
  }
  const sync = createSyncIndicator({ transport: getTransport, document: getDocument, status, t });

  function currentSharedLink() {
    const params = new URLSearchParams(window.location.search);
    return shareLink(window.location.href, { room: params.get("room"), documentUrl: params.get("document"), revision: Number(params.get("revision")) });
  }
  function startCollaboration() {
    getTransport()?.stop();
    const room = hostConfig().room;
    remotePresence.open(Boolean(room));
    if (room && !JSON.parse(getDocument().collabState).joined) {
      const revision = Number(new URLSearchParams(window.location.search).get("revision") || 0);
      getDocument().collabPrepare(revision);
    }
    // Who joins (`docs/175` §3, §4.2): the host identity's `id` and profile when
    // the host gave one, else the Settings name with a CLAIMED profile, else "You"
    // with none. From here the identity is fixed: a room is never re-joined as
    // somebody else, so a later `identify` is refused.
    if (room) hostIdentity.markJoined();
    const who = joinIdentity(getSettings());
    const provider = globalThis.OPENDOC_COLLABORATION;
    setTransport(withSync(openRoom(getDocument(), room, globalThis.WebSocket, {
      identity: who.identity,
      profile: who.profile,
      // Asked per join, reconnects included, and only when the host has a
      // provider that can answer; the grant is never kept here (§3.3).
      credentials: typeof provider?.credentialsFor === "function" ? () => provider.credentialsFor({ room }) : undefined,
      resumeKey: resumeKey(globalThis.sessionStorage, () => globalThis.crypto.randomUUID()),
      onFrame: (bytes) => remotePresence.frame(bytes),
      onOutcome: (o) => {
        recovery.receive(o);
        // A refusal is EXPLAINED, with what it did to the reader's work read off
        // the outcome (RC-13). Before this, nothing showed a refused submission at
        // all: a read-only participant's change stayed on their copy, unshared,
        // under a status line that still said it was reaching everyone. A thrown
        // refusal that stopped the session is left to `onState`, where the
        // transport's stop arrives with the same code.
        if (o.kind === "refused" || (o.kind === "error" && !engineStopped())) {
          const said = refusalAnnouncement(o, t, { stopped: false });
          if (said) status(said, "warn");
        }
        if (["welcome", "resumed", "ack", "applied"].includes(o.kind)) publishCheckpoint();
        // A mention is sent once its comment is ORDERED (`docs/175` §7.2): when an
        // acknowledgement leaves nothing of this replica's unacknowledged.
        if (o.kind === "ack" && !engineUnacknowledged()) void mentionsOrdered("room");
        if (PRESENCE_OUTCOMES.includes(o.kind)) { paintOverlay(); remotePresence.changed(); }
        const said = receiveAccess(o); if (said) status(said);
        if (o.documentChanged) { followArrival(); arrival(o); }
        sync.update();
      },
      onState: (state) => {
        remotePresence.state(state);
        // The chip and the generic connection sentences follow first (`collab_sync.mjs`);
        // then a stop that carries a collaboration code says why, and that the work stays
        // here — written last, so the specific sentence is the one left in the status line.
        sync.update();
        const said = state.name === "stopped"
          ? refusalAnnouncement({ kind: "stopped", code: state.code, terminal: true }, t)
          : "";
        if (said) status(said, "warn");
      },
    })));
    sync.update();
  }
  /** The pipe, with its `flush` — which every local edit calls — also refreshing the sync
   *  indicator, so "sending" appears on the keystroke rather than on the next frame. */
  function withSync(pipe) {
    return pipe && Object.freeze({ ...pipe, flush() { pipe.flush(); sync.update(); } });
  }
  /** Carries the reader's selection across what just merged (`107` P-4), synchronously —
   *  before the arrival paints and before the next keystroke reads the selection, which is
   *  the window in which a stale caret put the later typist's word in backwards. The engine
   *  holds the composed map of everything merged since the last call, so a frame carrying
   *  several arrivals is one call. O(steps merged), never O(document). */
  function followArrival() {
    const doc = getDocument(), range = getSelection();
    if (!doc || typeof doc.collabMapSelection !== "function") return;
    // Taken even with no selection, so a later call never maps a fresh click through
    // arrivals that landed before it.
    const first = () => { const p = doc.firstPosition(); const at = { node: p.node, offset: p.offset }; p.free?.(); return at; };
    const anchor = range?.anchor ?? first(), focus = range?.focus ?? anchor;
    let mapped;
    try { mapped = JSON.parse(doc.collabMapSelection(anchor.node, anchor.offset, focus.node, focus.offset)); } catch { return; }
    if (!range || (!mapped.moved && !mapped.removed)) return;
    // An end whose paragraph somebody deleted, somewhere the engine could not re-seat it:
    // the start of the document, which the editor's own placement check accepts.
    const fallback = mapped.removed ? first() : null;
    setSelection({ anchor: fallback ?? mapped.anchor, focus: fallback ?? mapped.focus });
  }
  function engineStopped() {
    try { return Boolean(JSON.parse(getDocument().collabState).stopped); } catch { return false; }
  }
  const recovery = createRecovery({ document: getDocument, transport: getTransport,
    provider: () => globalThis.OPENDOC_COLLABORATION, room: () => hostConfig().room, status, t,
    recovered: async () => {
      remotePresence.open(Boolean(hostConfig().room));
      const position = getDocument().firstPosition();
      setSelection({ anchor: { node: position.node, offset: position.offset }, focus: { node: position.node, offset: position.offset } });
      edited(); await repaintRecovered();
    },
  });
  const share = createShare({ document: getDocument, provider: () => globalThis.OPENDOC_COLLABORATION,
    base: () => window.location.href, name: getName, clipboard: () => navigator.clipboard,
    existingLink: currentSharedLink, showLink: (link) => shareChrome.showLink(link), status, t,
    joined: async (shared, link) => {
      getTransport()?.stop();
      window.history.replaceState(null, "", link);
      getDocument().collabPrepare(shared.revision);
      startCollaboration();
      shareChrome.reflect();
    },
  });
  const shareChrome = createShareChrome({ root, parent: headerEnd,
    share: () => shareDocument(), available: () => share.available(), t });
  // `main.js` calls `chrome.reflect()` whenever the toolbar reflects, which is also
  // when a language arrives; the strip's labels ride the same call.
  const shareReflect = shareChrome.reflect;
  shareChrome.reflect = () => { shareReflect(); strip.reflect(); };
  shareChrome.bind();
  shareChrome.reflect();
  function shareDocument() { return share.share().finally(() => shareChrome.reflect()); }
  let checkpointTimer = null;
  function publishCheckpoint() {
    const host = globalThis.OPENDOC_COLLABORATION;
    if (typeof host?.publishSnapshot !== "function" || !getDocument() || checkpointTimer !== null) return;
    const current = getDocument(), room = hostConfig().room;
    checkpointTimer = setTimeout(() => {
      checkpointTimer = null;
      if (getDocument() !== current || recovery.needed()) return;
      try {
        const revision = Number(JSON.parse(getDocument().collabState).revision);
        Promise.resolve(host.publishSnapshot({ room, revision, bytes: getDocument().collabCheckpoint() }))
          .catch(() => status(t("collab.checkpointFailed"), "warn"));
      } catch { status(t("collab.checkpointFailed"), "warn"); }
    }, 500);
  }
  const downloadRetainedEdits = createRecoveryDownloader({ root, document: getDocument, t });

  async function loadStartup(params, load) {
    if (!params.has("document")) return false;
    const source = sharedDocument(window.location.href, params.get("document"));
    const revision = Number(params.get("revision") || 0);
    if (!source || !hostConfig().room || !Number.isSafeInteger(revision) || revision < 0) { status(t("collab.shareInvalid"), "error"); return true; }
    await load(source, "shared-document.json"); return true;
  }
  function commands() {
    return [
      { id: "collab.share", label: t("collab.share"), group: "File", kw: "share link collaborate join room",
      enabled: share.available(), disabledReason: t("collab.shareHostRequired"), run: shareDocument },
      { id: "collab.recover", label: t("collab.recover"), group: "Review", kw: "recover pending local edits snapshot",
        enabled: Boolean(getTransport()) && recovery.needed() && !recovery.pending(),
        disabledReason: t(recovery.pending() ? "collab.recoveryPending" : "collab.recoveryNotNeeded"),
        run: () => recovery.receive({ kind: "recoveryRequired", revision: Number(JSON.parse(getDocument().collabState).revision) }) },
      { id: "collab.downloadRetained", label: t("collab.downloadRetained"), group: "Review", kw: "recovery retained local edits backup download",
        enabled: Boolean(getDocument()?.collabRecoveryCount()), disabledReason: t("collab.noRetainedDocuments"), run: downloadRetainedEdits },

    ];
  }
  function beforeReplace() {
    if (!getDocument()) return;
    const wasShared = Boolean(getTransport());
    getTransport()?.stop(); setTransport(null);
    sync.stop();
    // Leaving a room is said out loud. A document replaced while shared used to drop the
    // room without a word, which is how a version restore forked a shared document
    // silently (`docs/109` HF-312): the reader's copy went one way, everybody else's
    // stayed, and nothing said they were no longer together.
    if (wasShared) status(t("collab.leftShared"), "warn");
    remotePresence.open(false); recovery.reset();
    if (checkpointTimer !== null) clearTimeout(checkpointTimer);
    checkpointTimer = null;
    const url = new URL(window.location.href);
    for (const key of ["room", "document", "revision"]) url.searchParams.delete(key);
    window.history.replaceState(null, "", url);
  }
  function engineUnacknowledged() {
    try { return Boolean(JSON.parse(getDocument().collabState).unacknowledged); } catch { return true; }
  }
  return { presence: remotePresence, strip, recovery, share, chrome: shareChrome, start: startCollaboration, commands, loadStartup, beforeReplace,
    /** Why Restore must not run now, or "". A restore replaces this tab's document
     *  wholesale, so in a shared room it would fork it — this reader's copy goes back and
     *  everybody else's stays — and leave the room without a word (ADR-068, `docs/109`
     *  HF-312). Refused with the reason until a restore can travel as one transaction. */
    restoreRefusal: () => (getTransport() ? t("versionHistory.restore.sharedRefused") : "") };
}
