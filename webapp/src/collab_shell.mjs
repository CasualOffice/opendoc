import { newRoster } from "../pkg/casual_doc_wasm.js";
import { openRoom, resumeKey } from "./collab_transport.mjs";
import { createPresence } from "./collab_presence.mjs";
import { createRecovery } from "./collab_recovery.mjs";
import { createShare, shareLink, sharedDocument } from "./share_flow.mjs";
import { createShareChrome } from "./share_chrome.mjs";
import { createRecoveryDownloader } from "./recovery_chrome.mjs";

/** Host-managed sharing, checkpoints, recovery and ephemeral presence scheduling. */
export function createCollaborationShell({ root, getDocument, getTransport, setTransport,
  getSelection, getSettings, getName, hostConfig, paintOverlay, receiveAccess, arrival,
  repaintRecovered, edited, setSelection, status, t }) {
  const remotePresence = createPresence({ newRoster, document: getDocument,
    transport: getTransport, selection: getSelection, paint: paintOverlay, t });

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
    setTransport(openRoom(getDocument(), room, globalThis.WebSocket, {
      identity: getSettings().authorName.trim() || "You",
      resumeKey: resumeKey(globalThis.sessionStorage, () => globalThis.crypto.randomUUID()),
      onFrame: (bytes) => remotePresence.frame(bytes),
      onOutcome: (o) => {
        recovery.receive(o);
        if (["welcome", "resumed", "ack", "applied"].includes(o.kind)) publishCheckpoint();
        if (["awareness", "departed", "welcome", "resumed"].includes(o.kind)) { paintOverlay(); remotePresence.changed(); }
        const said = receiveAccess(o); if (said) status(said);
        if (o.documentChanged) arrival(o);
      },
      onState: (state) => { remotePresence.state(state); status(t(state.key), state.name === "connected" ? "" : "warn"); },
    }));
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
  const shareChrome = createShareChrome({ root, parent: root.getElementById("docTitle").parentElement,
    share: () => shareDocument(), available: () => share.available(), t });
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
        enabled: Boolean(getTransport()) && recovery.needed() && !recovery.pending(), run: () => recovery.receive({ kind: "recoveryRequired", revision: Number(JSON.parse(getDocument().collabState).revision) }) },
      { id: "collab.downloadRetained", label: t("collab.downloadRetained"), group: "Review", kw: "recovery retained local edits backup download",
        enabled: Boolean(getDocument()?.collabRecoveryCount()), run: downloadRetainedEdits },

    ];
  }
  function beforeReplace() {
    if (!getDocument()) return;
    getTransport()?.stop(); setTransport(null);
    remotePresence.open(false); recovery.reset();
    if (checkpointTimer !== null) clearTimeout(checkpointTimer);
    checkpointTimer = null;
    const url = new URL(window.location.href);
    for (const key of ["room", "document", "revision"]) url.searchParams.delete(key);
    window.history.replaceState(null, "", url);
  }
  return { presence: remotePresence, recovery, share, chrome: shareChrome, start: startCollaboration, commands, loadStartup, beforeReplace };
}
