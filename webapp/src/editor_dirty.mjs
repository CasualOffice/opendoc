// Unsaved-work tracking: the saved and current revisions, the document epoch,
// and the one answer to "is this document dirty" that Save, the unload guard
// and drafts all read. Moved out of `main.js` unchanged (docs/177); it reaches
// the rest of the editor only through the context `ed`.
import { CAPTURE_REASON } from "./version_history.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createDirtyTracking(ed) {
  // ---- Unsaved-work tracking ---------------------------------------------------
  //
  // Engine-authoritative and fail-dirty. Two independent axes make a document
  // unsaved, and BOTH must be tracked or the guard is silently wrong for one of
  // them:
  //
  //   * the MODEL — every applied edit carries the engine's own monotonic
  //     revision, so the question "has this changed since it was last written
  //     out?" is answered by the engine rather than inferred by the UI;
  //   * the NAME — renaming changes what Save produces without touching the model
  //     at all, so no revision can ever observe it.
  //
  // A watermark, not a boolean: a boolean cannot be cleared correctly by a save
  // that lands between two edits. And when the revision cannot be read for any
  // reason, `revisionUnreadable` latches and the document reports dirty — a
  // needless warning costs one click, the opposite mistake costs the document.
  //
  // Undo back to the original content still reports dirty, because revisions are
  // monotonic. That is the intended direction of the error.
  let savedRevision = 0;

  let savedName = "";


  /** Everything true of EVERY landed edit, whichever path applied it.
   *
   *  Four apply paths grew independently and diverged: `applyEditResult` cleared
   *  the find cache but never marked the document edited, `runToolbarEdit` marked
   *  it but never cleared the cache, `applyBookmarkEdit` did both, and
   *  `runNodeEdit` — the path every table and list structural edit takes — did
   *  neither. Shading a cell or converting a list left the header reading
   *  "Opened" and left Find matching against pre-edit text.
   *
   *  A rule that enumerates its subjects is one omission from being wrong, and the
   *  omission is the write path somebody adds last. What is true of every edit
   *  belongs here and nowhere else; what genuinely differs between the paths
   *  (caret movement, stats, sync vs async repaint) stays with them.
   *
   *  Call with the revision read BEFORE `res.free()`. */
  function noteDocumentEdited(revision) {
    ed.clearFindParagraphCache();
    // Every landed edit can add or remove a floating object, so the memoized
    // yes/no answer is dropped here — the one choke point every edit passes
    // (`109` HF-183). Costs one engine call on the next question, which is the
    // price of not asking on every keystroke.
    ed.objectPresence.forget();
    if (revision === null || revision === undefined) ed.revisionUnreadable = true;
    else ed.currentRevision = revision;
    ed.setDocumentState("edited");
    // Autosave's only contact with the edit path, and it must stay O(1) in
    // document size (docs/107 §4): this stores an integer and re-arms a timer.
    // The snapshot itself is taken from that timer, never from a keystroke.
    ed.noteDraftDirty();
    // Spelling's only contact with it, under the same constraint and for the
    // same reason: a keystroke must never touch the dictionary (docs/114 §3).
    ed.spellChecker.noteEdited();
    // And the host's, under the same constraint: one revision integer, one boolean.
    ed.hostSession?.noteChange();
    ed.remotePresence.sync(); // Map each view change before a later arrival can replace its map.
    ed.collab?.flush(); // Send local edits even when no remote presence changes.
    // An edit that changed the document's Track Changes setting — an Undo, a co-author — moves the mode with it (HF-283).
    ed.reviewTracking.follow(ed.doc?.trackRevisions, ed.reviewMode, (mode) => ed.SESSION.modeAuthority?.allows(mode).allowed !== false && ed.setReviewMode(mode, { restoreFocus: false }));
  }



  /** Re-baseline onto a freshly opened document: nothing is unsaved yet. */
  function resetDirtyTracking(name) {
    ed.documentEpoch += 1;
    savedRevision = 0;
    ed.currentRevision = 0;
    savedName = name;
    ed.revisionUnreadable = false;
    ed.restoredFromDraft = false;
  }

  /** The bytes have left the editor, so the current state is now the saved one. */
  function markDocumentSaved() {
    savedRevision = ed.currentRevision;
    savedName = ed.currentName;
    ed.revisionUnreadable = false;
    ed.restoredFromDraft = false;
    ed.setDocumentState("downloaded");
    // An explicit Save is ALWAYS a version (ADR-038 / `docs/139` §18 question 3):
    // it is the point a user recognises in a timeline, and because checkpoints are
    // content-addressed a Save whose bytes are identical to the last version costs
    // one ~300-byte row rather than a second copy of the document. This takes its
    // own snapshot in the document's own format — the download may have been a PDF,
    // and a checkpoint has to be fidelity-capable whatever was handed over.
    ed.versionHistory.capture(CAPTURE_REASON.SAVE);
    // The user has the bytes, so the draft has nothing left to protect. Keeping
    // it would mean offering back, after the next crash, work that is already on
    // disk — and the offer would look like the save had not happened.
    ed.discardOwnDraft();
  }

  /** Whether closing right now would lose work. */
  function documentIsDirty() {
    if (!ed.doc) return false;
    if (ed.revisionUnreadable || ed.restoredFromDraft) return true;
    return ed.currentRevision !== savedRevision || ed.currentName !== savedName;
  }

  /** Apply an EditResult: place the caret, repaint only the dirty pages (or rebuild
   *  on a page-count change), redraw the caret, and keep it in view. */
  /** Adopts the position an edit reports, or the nearest one that exists.
   *
   *  The engine's `EditResult` position was trusted unconditionally, and for undo,
   *  redo and accept/reject-all it can name a node that the very same operation
   *  removed. Nothing then painted a caret — `caretRect` returns an empty rect, so
   *  `place()` bailed — and every following keystroke threw inside `runEdit`, which
   *  turned it into a console warning and the generic status "That edit isn't
   *  supported for this selection yet". The document was still intact and the
   *  editor still looked alive; it simply ignored the keyboard until the user
   *  happened to click. That is the worst failure shape available to a text
   *  editor, and it is why this validates rather than trusts.
   *
   *  `caretRect` is the oracle because it is the same call the painter makes: if
   *  it cannot place the caret, neither can the user. The fallback is the body's
   *  first position, which the open path already relies on.
   *
   *  The engine should also not report a position it has just deleted; this is the
   *  host-side belt, not a reason to leave that unfixed. */
  return { documentIsDirty, markDocumentSaved, noteDocumentEdited, resetDirtyTracking };
}
