// Version history (docs/139, docs/140, ADR-038, ADR-040; HF-068, OO-004): the
// name prompt, the read-only preview that swaps the canvas onto a version or a
// redline and back, and the timeline's wiring. Moved out of `main.js`
// unchanged (docs/177); it reaches the rest of the editor only through `ed`.
import { open, engineVersion } from "../pkg/casual_doc_wasm.js";
import { createNamePrompt } from "./name_dialog.mjs";
import { createVersionHistory } from "./version_panel.mjs";
import { registerModal } from "./modal.mjs";
import { t } from "./i18n.mjs";
import { authorNameFor } from "./host_identity.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createVersions(ed) {
  // ---- Version history (docs/139, docs/140, ADR-038/ADR-040; HF-068 / OO-004) --
  //
  // Everything about the timeline — the store handle, the panel, the keyboard
  // contract, the retention disclosure and the restore state machine — is in
  // `version_panel.mjs`. What is left here is the four seams it cannot own:
  // parsing bytes, putting a document on the canvas, taking it off again, and
  // reporting. Placed AFTER `settings`, because the capture interval is read from
  // it at construction.
  //
  // The read-only enforcement is deliberately NOT a new mechanism. A preview sets
  // `readOnlyReason` and `viewing` mode, which is the editor's existing fail-closed
  // choke point: `blockMutationInViewing()` refuses every mutation route — typing,
  // paste, toolbar, tables, review decisions, the SDK and the host bridge — and
  // `editRefusalMessage` already prefers `readOnlyReason` over every other
  // sentence, so a preview refuses an edit by SAYING it is a preview.
  const versionNamePrompt = createNamePrompt({
    registerModal,
    fallbackFocus: () => ed.pagesEl,
    ids: {
      dialog: "versionNameDialog",
      input: "versionNameInput",
      confirm: "versionNameConfirm",
      cancel: "versionNameCancel",
      close: "versionNameClose",
    },
  });


  /**
   * Swaps the canvas onto a read-only preview — a version, or a redline (ADR-065)
   * with `markup` on — or (with `null`) back to the live document. O(preview) to
   * render, once; the live wrapper is KEPT: it holds the reader's unsaved work.
   */
  async function showVersionPreview(previewDoc, { markup = false, reason = t("versionHistory.preview.readOnly") } = {}) {
    if (previewDoc) {
      ed.versionPreviewHome ??= { doc: ed.doc, selection: ed.selection, reviewMode: ed.reviewMode, readOnlyReason: ed.readOnlyReason, showingChanges: ed.showingChanges, reviewSidebarPreference: ed.reviewSidebarPreference };
      ed.doc = previewDoc;
      ed.readOnlyReason = reason; if (markup) ed.reviewSidebarPreference = false; // a redline's index is its own panel, not decision cards
      // A redline IS its markup; one too large for the markup layout is shown plain, and says why.
      try { ed.showingChanges = markup && (previewDoc.setShowChanges(true) ?? true); } catch (error) { ed.showingChanges = false; ed.setStatus(String(error?.message ?? error), "error"); }
    } else {
      if (!ed.versionPreviewHome) return;
      ({ doc: ed.doc, selection: ed.selection, readOnlyReason: ed.readOnlyReason, showingChanges: ed.showingChanges, reviewSidebarPreference: ed.reviewSidebarPreference } = ed.versionPreviewHome);
      const home = ed.versionPreviewHome;
      ed.versionPreviewHome = null;
      // LEAVING keeps the default focus restore: "Back to current" goes away with the
      // banner, and declining to move focus would drop the keyboard onto `<body>`.
      ed.setReviewMode(home.reviewMode);
    }
    ed.reflectShowingChangesState();
    // A different document: every cached answer about the last one is wrong (object
    // presence, table selection, card geometry; the measure ticker captures `doc`).
    ed.objectPresence.forget();
    ed.tableRange.clear();
    ed.reviewLayout = [];
    ed.reviewCardCache.clear();
    ed.commentEdit.end(); ed.commentQuotes.clear(); ed.commentQuotes.prime(ed.doc, ed.readReviewData(ed.doc).comments);
    if (previewDoc) {
      const start = ed.doc.firstPosition();
      ed.selection = {
        anchor: { node: start.node, offset: start.offset },
        focus: { node: start.node, offset: start.offset },
      };
      start.free();
      ed.setReviewMode("viewing", { restoreFocus: false });
    }
    ed.armBackgroundMeasure();
    await ed.renderAll();
    // The chrome that DESCRIBES the document — the screen-reader mirror, the
    // outline and the footer counts — is rebuilt on content-changed triggers, and
    // a swap is one: without this a version preview, a Compare view and Review ▸
    // Original all painted the other document while the mirror still read the
    // live one, so Original's rejected text was invisible to a screen reader.
    ed.scheduleChromeRefresh({ stats: true, outline: true, a11y: true });
    ed.drawSelection();
    ed.updateToolbar();
  }


  /**
   * The same name dialog, worded for a version everyone in the room will see
   * (`docs/175` §11): its local wording ("kept until you delete it") is not true of a
   * version the host keeps. The four lines are put back when it closes, so the local
   * prompt is never left saying the shared one's words.
   */
  const SHARED_NAME_WORDS = [
    ["versionNameTitle", "versionHistory.shared.prompt.title"],
    ["versionNameDescription", "versionHistory.shared.prompt.description"],
    ["versionNameNote", "versionHistory.shared.prompt.note"],
    ["versionNameConfirm", "versionHistory.shared.prompt.confirm"],
  ];
  async function promptSharedName(current) {
    const before = SHARED_NAME_WORDS.map(([id, key]) => {
      const el = document.getElementById(id);
      const was = el?.textContent ?? "";
      if (el) el.textContent = t(key);
      return [el, was];
    });
    try {
      return await versionNamePrompt.prompt(current);
    } finally {
      for (const [el, was] of before) if (el) el.textContent = was;
    }
  }

  const versionHistory = createVersionHistory({
    // The room's shared history, when there is a room (`docs/175` §11, HF-362).
    shared: () => ed.sharedHistory(),
    promptSharedName,
    parse: (bytes) => open(bytes),
    showPreview: (previewDoc, _row, options) => showVersionPreview(previewDoc, options),
    // Through the ORDINARY open path, so a restored document is indistinguishable
    // from an opened one: same admission limits, dirty tracking and loss reporting.
    activateRestored: async (bytes, name) => {
      ed.activatingRestore = true;
      try {
        await ed.openBytes(bytes, name);
      } finally {
        ed.activatingRestore = false;
      }
      // Edited/Unsaved until the host saves it (`docs/139` §8.5 step 7):
      // `openBytes` re-baselines dirty tracking onto the bytes it opened, and for a
      // restore that baseline is a lie — the file on disk is still the old one.
      // Same flag as a recovered draft, because it is the same fact.
      ed.restoredFromDraft = true;
      ed.setDocumentState("edited");
    }, restoreRefusal: ed.sharedRestoreRefusal, // ADR-068: a shared room refuses, never forks
    snapshot: () => (ed.doc ? ed.takeDraftSnapshot() : null),
    documentInfo: () => ({
      name: ed.currentName,
      docKey: ed.draftDocKey,
      revision: ed.revisionUnreadable ? null : ed.currentRevision,
      // Only with a document, and that guard is load-bearing: `applySettings` asks
      // this for `unavailableReason` at module init, BEFORE `boot()` has awaited
      // `init()`, and `engineVersion()` on an uninstantiated wasm module throws
      // `__wbindgen_add_to_stack_pointer` of undefined and kills boot outright.
      engine: ed.doc ? engineVersion() : "",
      actor: authorNameFor(ed.settings),
      hasDocument: Boolean(ed.doc),
    }),
    settings: () => ed.settings,
    hostAllows: () => ed.AUTOSAVE_ALLOWED_HERE,
    capabilities: () => ed.HOST_CAPS,
    publish: (text, kind) => ed.statusChannel.publish(text, kind),
    confirm: (options) => ed.confirmModal(options),
    promptName: (current) => versionNamePrompt.prompt(current),
    onOpenChange: (isOpen) => {
      // The timeline and the comments sidebar are both on the right, so they are
      // mutually exclusive for the reason Outline and Pages are: the canvas is
      // never squeezed from both sides at once.
      if (isOpen && !ed.reviewSidebar.hidden) ed.toggleReview(false);
    },
    redline: ed.redlineOf,
    navigateTo: ed.showChange,
    beforePreview: () => ed.comparePanel.closeView(),
  });

  /**
   * Pushes the host's reviewer identity into the open document through the
   * explicit `setActiveAuthor` seam (see docs/68 "Host identity seam" and
   * docs/81 REVIEW-GAP-013) — this is the one place identity crosses from the
   * host UI into the engine. A blank name still resolves to "You" so
   * `suggestInsert`/`suggestDelete`/etc. (which require a non-empty author)
   * keep working out of the box; a blank initials field lets the engine derive
   * initials from the name instead of duplicating that logic here.
   */
  return { showVersionPreview, versionHistory };
}
