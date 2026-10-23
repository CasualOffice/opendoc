// Autosave drafts and crash recovery (HF-011, OO-004, docs/112): the per-tab
// draft slot, the quiesce-driven snapshot in the document's own format, and
// the boot-time recovery offer. Moved out of `main.js` unchanged (docs/177);
// it reaches the rest of the editor only through the context `ed`.
import { engineVersion } from "../pkg/casual_doc_wasm.js";
import { findingTotals } from "./compat_findings.mjs";
import { downloadNameForFormat, formatInfo } from "./format_io.mjs";
import { DRAFT_EXPORT_MODES, DRAFT_WRITE_REASONS, DraftPresence, DraftScheduler, HEARTBEAT_MS, bindDraftFlushOnExit, describeDraftAge, describeDraftSize, documentKey, draftFormatFor, draftSlotId, evictableSlots, offerableDrafts, openDraftStore, slotIsLive } from "./drafts.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createDrafts(ed) {
  // ---- Autosave drafts and crash recovery (HF-011 / OO-004, docs/112) --------
  //
  // The hole this closes: the open document lives in the wasm heap and nowhere
  // else, so an OOM kill, a wasm trap or a power cut — none of which run a single
  // line of shutdown code — took the whole session with it. The `beforeunload`
  // guard above catches a deliberate close and nothing else.
  //
  // The shape is the sibling's (opencalc `editor.drafts.js`), as owner decision
  // D-1 requires: an IndexedDB store with a meta row and a bytes row per tab
  // slot, written on quiesce with a ceiling, and a boot-time bar that OFFERS the
  // work back and never applies it. The policy lives in `drafts.mjs` where it can
  // be unit-tested; what is here is wiring, the surfaces, and the export ladder.
  //
  // Two invariants, both load-bearing:
  //   * nothing on the keystroke path is O(document) — `noteDraftDirty` stores an
  //     integer and re-arms a timer, and the snapshot runs from that timer;
  //   * the draft is written in the document's OWN format, because the
  //     normalized-JSON snapshot measurably drops embedded images (docs/112 §3.2)
  //     and handing back a document with its pictures gone is silent data loss.
  const draftRecoveryBar = document.getElementById("draftRecoveryBar");
  const draftRecoveryList = document.getElementById("draftRecoveryList");
  const draftRecoveryTitle = document.getElementById("draftRecoveryTitle");
  const draftStatusEl = document.getElementById("draftStatus");
  const autosaveToggle = document.getElementById("autosaveToggle");
  const spellCheckToggle = document.getElementById("spellCheckToggle");
  const grammarCheckToggle = document.getElementById("grammarCheckToggle");
  const draftsClearBtn = document.getElementById("draftsClearBtn");

  /** This tab's slot. `let`, because opening another document hands the current
   *  slot's draft over as an orphan and takes a fresh one (see
   *  `handOverDraftBeforeOpen`). */
  let draftSlot = draftSlotId(safeSessionStorage());
  let draftStore = null;
  let draftStoreOpening = null;
  /** Non-empty once autosave has given up, and the reason it gave. */
  let draftUnavailableReason = "";

  let draftBarDismissed = false;
  let ownSlotHasDraft = false;
  let draftHeartbeatTimer = 0;
  /** Serializes store work: two overlapping writes would race on one slot. */
  let draftQueue = Promise.resolve();

  /** `sessionStorage`, or null where touching it throws (cross-origin embed,
   *  site data blocked) — the same posture `readPref` takes for preferences. */
  function safeSessionStorage() {
    try {
      return window.sessionStorage;
    } catch {
      return null;
    }
  }

  /** Whether this page may keep drafts at all.
   *
   *  Off inside a frame: `home-embed.js` boots `editor.html?demo=1` in an iframe
   *  on the marketing home page, and without this every visitor who typed in the
   *  hero demo would leave a draft on the origin — which a later real session
   *  would then be offered. `?autosave=1` lets a host opt back in, and
   *  `?autosave=0` lets one opt out of the top-level case (docs/112 O-2). */
  // One authority. This used to re-derive framing here, which is exactly why
  // `capabilities.mjs` was not the single answer to "what may this page do".
  const AUTOSAVE_ALLOWED_HERE = ed.HOST_CAPS.has("autosave");

  /** The user's switch. Defaults on; `settings` is read at call time. */
  function autosaveEnabled() {
    return AUTOSAVE_ALLOWED_HERE && ed.settings.autosave !== false && !draftUnavailableReason;
  }

  /** Autosave has failed in a way the user should know about. It is never silent:
   *  a draft that is not being written is a promise the editor is not keeping. */
  function reportAutosaveUnavailable(reason) {
    if (draftUnavailableReason) return;
    draftUnavailableReason = reason;
    stopDraftHeartbeat();
    setDraftStatus("Autosave unavailable", "unavailable", `Autosave is not running: ${reason}. Save the document to keep your work.`);
    // `publish`, not `announce`: the draft pill it pairs with is `priority 3` in
    // the footer's informational half, so it is the FIRST thing shed as the window
    // narrows — a broken promise about the user's work must not narrow away with it.
    ed.statusChannel.publish(`Autosave unavailable: ${reason}`, "error");
  }

  function setDraftStatus(text, state = "", title = "") {
    if (!draftStatusEl) return;
    draftStatusEl.textContent = text;
    draftStatusEl.hidden = !text;
    if (state) draftStatusEl.dataset.draftState = state;
    else delete draftStatusEl.dataset.draftState;
    draftStatusEl.title = title || text;
  }

  /** hh:mm in the user's locale, for "Draft saved 14:32". */
  function clockTime(at) {
    return new Date(at).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  }

  /** Opens the store once, lazily. Returns null when it cannot be opened, having
   *  already said so. */
  async function ensureDraftStore() {
    if (draftStore) return draftStore;
    if (!AUTOSAVE_ALLOWED_HERE || draftUnavailableReason) return null;
    if (!draftStoreOpening) {
      draftStoreOpening = openDraftStore({}).then(
        (store) => {
          draftStore = store;
          return store;
        },
        (err) => {
          reportAutosaveUnavailable(`this browser refused local storage (${err?.message ?? err})`);
          return null;
        },
      );
    }
    return draftStoreOpening;
  }

  /** Runs store work one job at a time, so two writes never race on one slot. */
  function queueDraftWork(job) {
    draftQueue = draftQueue.then(job).catch((err) => {
      console.error("draft store", err);
    });
    return draftQueue;
  }

  // The cadence. Injected `write` is the only thing that ever costs document-sized
  // work, and the scheduler only ever calls it from a timer.
  const draftScheduler = new DraftScheduler({ write: (reason) => queueDraftWork(() => writeDraft(reason)) });

  // Cross-tab presence. Answering "is that tab still open?" with a heartbeat
  // timestamp is wrong in exactly the case that matters: a crashed tab's last
  // heartbeat is seconds old, so a heartbeat-only lease withholds the draft from
  // the person reopening the editor to get it back. Asking the other tabs
  // directly distinguishes "still open" from "died" immediately.
  const draftPresence = new DraftPresence({ slotId: draftSlot });

  /** O(1). The edit path's entire contribution to autosave. */
  function noteDraftDirty() {
    if (!autosaveEnabled() || !ed.doc) return;
    draftScheduler.noteDirty();
  }

  /** Re-keys this tab onto a freshly opened document. */
  function adoptDraftDocument(name, bytes) {
    ed.draftDocKey = documentKey(name, bytes);
    draftScheduler.reset();
  }

  /**
   * Takes the snapshot, in the document's own format.
   *
   * The mode ladder is `exportDocumentAs`'s plus `semantic` as the last resort, so
   * a draft and a save can never come from different ladders; there is deliberately
   * no cross-format fallback, because normalized JSON would succeed where DOCX
   * failed and drop every image. `contentId` rides along — see `version_panel.mjs`.
   */
  function takeDraftSnapshot() {
    const formatId = draftFormatFor(ed.currentSourceFormat);
    let lastError = null;
    for (const mode of DRAFT_EXPORT_MODES) {
      try {
        const artifact = ed.doc.exportAs(formatId, mode);
        const bytes = artifact.bytes;
        let findings = 0;
        try {
          findings = findingTotals(artifact.reportJson).headline; // the chip's count (FID-FW-01)
        } catch {
          findings = 0; // a report we cannot parse must not lose us the draft
        }
        artifact.free();
        return { bytes, formatId, mode, findings, contentId: ed.doc.contentDigest(), words: ((s) => { const words = s.words; s.free(); return words; })(ed.doc.documentStats()) };
      } catch (err) {
        lastError = err;
      }
    }
    throw lastError ?? new Error("no export mode produced a snapshot");
  }

  /** Writes this tab's draft. Only ever called from the scheduler or a flush. */
  async function writeDraft(reason) {
    if (!ed.doc || !autosaveEnabled() || !ed.documentIsDirty()) return;
    const store = await ensureDraftStore();
    if (!store) return;

    let snapshot;
    try {
      snapshot = takeDraftSnapshot();
    } catch (err) {
      reportAutosaveUnavailable(`this document could not be exported (${err?.message ?? err})`);
      return;
    }

    const now = Date.now();
    const meta = {
      slotId: draftSlot,
      docKey: ed.draftDocKey,
      name: ed.currentName,
      formatId: snapshot.formatId,
      exportMode: snapshot.mode,
      findings: snapshot.findings,
      revision: ed.currentRevision,
      bytes: snapshot.bytes.length,
      savedAt: now,
      heartbeatAt: now,
      engine: engineVersion(),
      reason,
    };

    try {
      await pruneDraftSlots(store, now);
      await store.putDraft(meta, snapshot.bytes);
    } catch (err) {
      // Quota is the one failure worth a second attempt: drop the oldest draft
      // that is not ours and try again before giving up on autosave entirely.
      if (err?.name === "QuotaExceededError") {
        const freed = await freeOldestOtherSlot(store);
        if (freed) {
          try {
            await store.putDraft(meta, snapshot.bytes);
          } catch (retryErr) {
            reportAutosaveUnavailable(`this browser is out of storage (${retryErr?.name ?? "quota"})`);
            return;
          }
        } else {
          reportAutosaveUnavailable("this browser is out of storage for local drafts");
          return;
        }
      } else {
        reportAutosaveUnavailable(`the draft could not be written (${err?.message ?? err})`);
        return;
      }
    }

    ownSlotHasDraft = true;
    // The version capture rides HERE and nowhere else, with the SAME bytes: the
    // artifact is already exported and already verified at this point, so a
    // version costs one hash and one IndexedDB transaction rather than a second
    // export of the document (`docs/140` §6.2, §7.5). The decision itself —
    // `VersionCapturePolicy.shouldCapture` — is O(1) in both document size and
    // stored-version count and is all the editing path ever pays.
    ed.versionHistory.capture(reason, snapshot);
    setDraftStatus(
      `Draft saved ${clockTime(now)}`,
      "saved",
      `${describeDraftSize(meta.bytes)} kept in this browser only, until you save. Written ${clockTime(now)}.`,
    );
    startDraftHeartbeat();
  }

  /** Deletes expired rows and anything past the slot cap, never our own. */
  async function pruneDraftSlots(store, now) {
    const metas = await store.listMeta();
    for (const slotId of evictableSlots(metas, { now, keepSlotId: draftSlot })) {
      await store.deleteSlot(slotId);
    }
  }

  /** Frees the oldest slot that is not ours. Returns whether anything went. */
  async function freeOldestOtherSlot(store) {
    const metas = (await store.listMeta())
      .filter((meta) => meta.slotId !== draftSlot)
      .sort((a, b) => (a.savedAt ?? 0) - (b.savedAt ?? 0));
    if (metas.length === 0) return false;
    await store.deleteSlot(metas[0].slotId);
    return true;
  }

  /** Tells other tabs this slot is still owned. Meta only — a few hundred bytes,
   *  never the snapshot — and only while a draft actually exists. */
  function startDraftHeartbeat() {
    if (draftHeartbeatTimer || !ownSlotHasDraft) return;
    draftHeartbeatTimer = setInterval(() => {
      if (!ownSlotHasDraft) return stopDraftHeartbeat();
      queueDraftWork(async () => {
        const store = await ensureDraftStore();
        if (store) await store.touch(draftSlot, Date.now());
      });
    }, HEARTBEAT_MS);
  }

  function stopDraftHeartbeat() {
    if (!draftHeartbeatTimer) return;
    clearInterval(draftHeartbeatTimer);
    draftHeartbeatTimer = 0;
  }

  /** The work is on disk now, so this tab's draft has nothing left to protect. */
  function discardOwnDraft() {
    draftScheduler.reset();
    if (!ownSlotHasDraft) return;
    ownSlotHasDraft = false;
    stopDraftHeartbeat();
    setDraftStatus("");
    queueDraftWork(async () => {
      const store = await ensureDraftStore();
      if (store) await store.deleteSlot(draftSlot);
    });
  }

  /**
   * Called just before another document replaces the open one.
   *
   * If there is unsaved work, it is written one last time and the slot is left
   * behind as an ORPHAN — heartbeat cleared, so the next scan offers it — while
   * this tab takes a fresh slot for the incoming document. The user may have
   * pressed "Discard and open" at the confirm gate, but that discards the
   * document from the screen; it is not a request to shred the only copy.
   */
  function handOverDraftBeforeOpen() {
    if (!ed.doc) return;
    const hadWork = ed.documentIsDirty() && autosaveEnabled();
    const previousSlot = draftSlot;
    if (hadWork) {
      // Synchronous snapshot, queued write: the wrapper is freed by the caller
      // moments from now, so the bytes must be taken before this returns.
      let snapshot = null;
      try {
        snapshot = takeDraftSnapshot();
      } catch {
        snapshot = null; // nothing to hand over; the open still proceeds
      }
      if (snapshot) {
        const meta = {
          slotId: previousSlot,
          docKey: ed.draftDocKey,
          name: ed.currentName,
          formatId: snapshot.formatId,
          exportMode: snapshot.mode,
          findings: snapshot.findings,
          revision: ed.currentRevision,
          bytes: snapshot.bytes.length,
          savedAt: Date.now(),
          // Orphaned on purpose: this tab no longer owns the slot, so the next
          // scan must not mistake it for a live tab's live draft.
          heartbeatAt: 0,
          engine: engineVersion(),
          reason: "handover",
        };
        queueDraftWork(async () => {
          const store = await ensureDraftStore();
          if (store) await store.putDraft(meta, snapshot.bytes);
        });
        draftSlot = rotateDraftSlot();
        // Presence must follow the rotation, or this tab keeps answering pings
        // for the slot it just handed over — which would mark that orphan draft
        // as belonging to a live tab and hide it from every recovery scan.
        draftPresence.slotId = draftSlot;
      }
    } else if (ownSlotHasDraft) {
      discardOwnDraft();
    }
    ownSlotHasDraft = false;
    stopDraftHeartbeat();
    draftScheduler.reset();
    setDraftStatus("");
  }

  /** A fresh slot id for this tab, persisted like the first one. */
  function rotateDraftSlot() {
    const storage = safeSessionStorage();
    try {
      storage?.removeItem("opendoc.draftSlot");
    } catch {
      /* storage is optional; a session-only slot still works */
    }
    return draftSlotId(storage);
  }

  // ---- The recovery offer -----------------------------------------------------

  /** Reads the meta store, prunes what has expired, and renders the bar. */
  async function refreshDraftOffers({ announce = false } = {}) {
    const store = await ensureDraftStore();
    if (!store) return;
    const now = Date.now();
    let metas = [];
    try {
      metas = await store.listMeta();
    } catch (err) {
      reportAutosaveUnavailable(`the draft store could not be read (${err?.message ?? err})`);
      return;
    }
    for (const slotId of evictableSlots(metas, { now, keepSlotId: draftSlot })) {
      try {
        await store.deleteSlot(slotId);
      } catch {
        /* a row we cannot delete is not a reason to withhold the offer */
      }
    }
    // Ask the other tabs which of these slots they still own. Only rows whose
    // heartbeat is recent enough to be ambiguous are worth asking about, so a
    // boot with nothing else running waits for nothing.
    const ambiguous = metas
      .filter((meta) => meta.slotId !== draftSlot && slotIsLive(meta, now, draftSlot))
      .map((meta) => meta.slotId);
    const liveSlotIds = await draftPresence.liveSlots(ambiguous);

    ed.draftOffers = offerableDrafts(metas, {
      now,
      ownSlotId: draftSlot,
      liveSlotIds,
      // Silence is only proof when there was a way to ask.
      heartbeatFallback: draftPresence.unavailable,
    }).filter(
      // Once this tab has written its own draft, that draft is the document on
      // screen. Offering it back would be offering the user their own live work.
      (meta) => !(ownSlotHasDraft && meta.slotId === draftSlot),
    );
    renderDraftRecovery({ announce });
  }

  function renderDraftRecovery({ announce = false } = {}) {
    if (!draftRecoveryBar) return;
    if (draftBarDismissed || ed.draftOffers.length === 0) {
      draftRecoveryBar.hidden = true;
      draftRecoveryList.replaceChildren();
      return;
    }
    const now = Date.now();
    draftRecoveryTitle.textContent =
      ed.draftOffers.length === 1
        ? "Unsaved work recovered"
        : `Unsaved work recovered from ${ed.draftOffers.length} sessions`;

    draftRecoveryList.replaceChildren(
      ...ed.draftOffers.map((meta) => {
        const row = document.createElement("li");
        row.className = "draft-recovery-row";
        row.dataset.slot = meta.slotId;

        const name = document.createElement("span");
        name.className = "draft-recovery-name";
        name.textContent = meta.name || "Untitled document";
        name.title = meta.name || "Untitled document";

        const detail = document.createElement("span");
        detail.className = "draft-recovery-meta";
        const sameDocument = meta.docKey && meta.docKey === ed.draftDocKey ? " · this document" : "";
        detail.textContent = `autosaved ${describeDraftAge(now - (meta.savedAt ?? now))} · ${describeDraftSize(meta.bytes)}${sameDocument}`;

        const restore = document.createElement("button");
        restore.type = "button";
        restore.dataset.draftRestore = meta.slotId;
        restore.textContent = "Restore";
        restore.setAttribute("aria-label", `Restore ${meta.name || "the recovered document"}`);

        const remove = document.createElement("button");
        remove.type = "button";
        remove.dataset.draftDelete = meta.slotId;
        remove.textContent = "Delete";
        remove.setAttribute("aria-label", `Delete the recovered draft of ${meta.name || "this document"}`);

        row.append(name, detail, restore, remove);
        return row;
      }),
    );
    draftRecoveryBar.hidden = false;
    if (announce) {
      const first = ed.draftOffers[0];
      // `announce`, not `publish`: the recovery bar is its own on-screen surface at
      // every width, so a toast would be a second copy of what is already in view.
      ed.statusChannel.announce(
        `Unsaved work recovered: ${first.name}, autosaved ${describeDraftAge(now - (first.savedAt ?? now))}. Restore or delete it from the recovery bar.`,
      );
    }
  }

  /** Brings the offer back after a dismissal — the File menu / palette route. */
  function showDraftRecovery() {
    draftBarDismissed = false;
    renderDraftRecovery();
    queueDraftWork(() => refreshDraftOffers());
    draftRecoveryBar?.querySelector("button[data-draft-restore]")?.focus({ preventScroll: true });
  }

  /** Restores one draft. Offers first, applies only on this call. */
  async function restoreDraft(slotId) {
    const meta = ed.draftOffers.find((row) => row.slotId === slotId);
    const store = await ensureDraftStore();
    if (!meta || !store) return;
    // The same gate File ▸ Open uses: restoring replaces what is on screen.
    if (!(await ed.confirmDiscardIfEdited())) return;
    let bytes;
    try {
      bytes = await store.readBytes(slotId);
    } catch (err) {
      ed.setStatus(`The recovered draft could not be read: ${err?.message ?? err}`, "error");
      return;
    }
    if (!bytes) {
      ed.setStatus("That draft is no longer in this browser's storage", "error");
      await forgetDraft(slotId);
      return;
    }
    // A promoted draft (a .txt document kept as DOCX so its formatting survived)
    // comes back under the extension it will actually save as.
    const name = downloadNameForFormat(meta.name, formatInfo(meta.formatId).extension);
    // `openBytes` swallows a failed parse by design — it leaves the previous
    // document on screen rather than destroying it — so success has to be
    // observed, not assumed. Its opened-document hook runs only after the parse
    // succeeded. Treating a failed restore as a success and then deleting the
    // row would delete the only copy of the work the restore was trying to save.
    let opened = false;
    await ed.openBytes(bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes), name, () => {
      opened = true;
    });
    if (!opened) {
      ed.setStatus(`“${meta.name}” could not be restored — the draft has been kept`, "error");
      return;
    }
    // Restored work has never been written out. Saying "Opened" here would tell
    // the user their document is safe on disk when it is not.
    ed.restoredFromDraft = true;
    ed.setDocumentState("edited");
    // Take our own copy BEFORE dropping the row it came from, so the work is
    // never momentarily in neither place — a crash in the gap would otherwise
    // lose exactly the document the user had just recovered.
    await queueDraftWork(() => writeDraft(DRAFT_WRITE_REASONS.RESTORED));
    if (slotId === draftSlot) {
      // This tab reclaimed its own slot, so the write above IS that row, now
      // holding the restored document. Deleting it would delete the work.
      ed.draftOffers = ed.draftOffers.filter((row) => row.slotId !== slotId);
      renderDraftRecovery();
    } else if (ownSlotHasDraft) {
      await forgetDraft(slotId);
    } else {
      // Autosave could not take a copy (off, or unavailable), so the row we
      // restored from is still the only one. Keep it; just stop offering it.
      ed.draftOffers = ed.draftOffers.filter((row) => row.slotId !== slotId);
      renderDraftRecovery();
    }
    ed.setStatus(
      `Restored “${name}” from the draft autosaved ${describeDraftAge(Date.now() - (meta.savedAt ?? Date.now()))} — save it to keep it`,
    );
  }

  /** Deletes one draft, after asking: it is the only copy of that work. */
  async function deleteDraftWithConfirmation(slotId) {
    const meta = ed.draftOffers.find((row) => row.slotId === slotId);
    if (!meta) return;
    const ok = await ed.confirmModal({
      title: "Delete this recovered draft?",
      message: `“${meta.name}” was autosaved ${describeDraftAge(Date.now() - (meta.savedAt ?? Date.now()))}. This is the only copy of that work — deleting it cannot be undone.`,
      confirmLabel: "Delete draft",
      cancelLabel: "Keep it",
      note: "Restore it first if you are not sure.",
      icon: "warning",
    });
    if (!ok) return;
    await forgetDraft(slotId);
    ed.setStatus(`Deleted the recovered draft of “${meta.name}”`);
  }

  async function forgetDraft(slotId) {
    const store = await ensureDraftStore();
    if (store) {
      try {
        await store.deleteSlot(slotId);
      } catch (err) {
        console.error("draft delete", err);
      }
    }
    ed.draftOffers = ed.draftOffers.filter((row) => row.slotId !== slotId);
    renderDraftRecovery();
  }

  /** Settings ▸ Autosave ▸ Delete saved drafts, and the switch's off path. */
  async function clearAllDrafts({ confirm = true } = {}) {
    if (confirm) {
      const ok = await ed.confirmModal({
        title: "Delete every saved draft?",
        message:
          "Drafts are the only copy of work that was never saved to a file. Deleting them cannot be undone.",
        confirmLabel: "Delete drafts",
        cancelLabel: "Keep them",
        icon: "warning",
      });
      if (!ok) return;
    }
    const store = await ensureDraftStore();
    if (store) {
      try {
        await store.clear();
      } catch (err) {
        console.error("draft clear", err);
      }
    }
    ownSlotHasDraft = false;
    stopDraftHeartbeat();
    ed.draftOffers = [];
    renderDraftRecovery();
    setDraftStatus("");
    ed.setStatus("Saved drafts deleted");
  }

  draftRecoveryBar?.addEventListener("click", (event) => {
    const restore = event.target.closest?.("button[data-draft-restore]");
    if (restore) {
      void restoreDraft(restore.dataset.draftRestore);
      return;
    }
    const remove = event.target.closest?.("button[data-draft-delete]");
    if (remove) void deleteDraftWithConfirmation(remove.dataset.draftDelete);
  });
  document.getElementById("draftRecoveryDismiss")?.addEventListener("click", () => {
    // Dismiss keeps the draft. Nothing in this bar may lose work by being closed.
    draftBarDismissed = true;
    renderDraftRecovery();
    ed.setStatus("Recovered work is still available from File ▸ Recover unsaved work");
  });

  // Hidden, then pagehide: see `bindDraftFlushOnExit` for why those two.
  bindDraftFlushOnExit(draftScheduler);

  /** Boot: scan the store and offer whatever a previous session left behind. */
  async function startDrafts() {
    if (!AUTOSAVE_ALLOWED_HERE) return;
    if (ed.settings.autosave === false) {
      setDraftStatus("Autosave off", "off", "Autosave is off. Turn it back on in Settings ▸ Autosave.");
      return;
    }
    await queueDraftWork(() => refreshDraftOffers({ announce: true }));
  }

  return {
    AUTOSAVE_ALLOWED_HERE, adoptDraftDocument, autosaveToggle, clearAllDrafts, discardOwnDraft,
    draftsClearBtn, grammarCheckToggle, handOverDraftBeforeOpen, noteDraftDirty, queueDraftWork,
    refreshDraftOffers, setDraftStatus, showDraftRecovery, spellCheckToggle, startDrafts,
    takeDraftSnapshot,
  };
}
