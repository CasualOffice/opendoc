// The edit choke point: `runEdit` and `applyEditResult`, the typing session,
// and the gates that refuse an edit in Viewing or an untracked one in
// Suggesting. Moved out of `main.js` unchanged (docs/177); it reaches the
// rest of the editor only through the context `ed`.
import { readRevision } from "./edit_errors.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createEditPipeline(ed) {
  function adoptEditPosition(node, offset) {
    const at = { anchor: { node, offset }, focus: { node, offset } };
    try {
      if (ed.doc.caretRect(node, offset).length >= 5) return at;
    } catch {
      // fall through — an unplaceable position is exactly what we are guarding
    }
    try {
      const fallback = ed.doc.firstPosition();
      const recovered = { node: fallback.node, offset: fallback.offset };
      fallback.free?.();
      if (ed.doc.caretRect(recovered.node, recovered.offset).length >= 5) {
        return { anchor: recovered, focus: recovered };
      }
    } catch {
      // no recoverable position: keep the reported one rather than crashing, and
      // let the caller repaint. Better a missing caret than a dead editor.
    }
    return at;
  }

  /** Repaints what a remote arrival changed; the engine applied it, and the shell already
   *  carried `selection` across it synchronously (`collabMapSelection`, `107` P-4). */
  async function paintArrival({ dirty = [], pageCount, viewRevision }) {
    ed.noteDocumentEdited(viewRevision);
    if (pageCount !== ed.pages.length) await ed.renderAll();
    else { for (const i of dirty) ed.repaintPage(i); ed.drawSelection(); }
    ed.scheduleChromeRefresh({ stats: true, outline: true });
  }

  async function applyEditResult(res, { keepView = false, keepSelection = false } = {}) {
    const node = res.node;
    const offset = res.offset;
    const dirty = res.dirtyPages;
    const newCount = res.pageCount;
    const revision = readRevision(res);
    res.free();
    // The engine reports an edit that changed nothing — a picture dropped where it
    // came from, an empty comparison — as the SAME revision and no dirty pages.
    if (revision === null || revision !== ed.currentRevision || dirty.length > 0) ed.noteDocumentEdited(revision);
    // An edit has landed, so the caret the editor now shows is the result of the
    // user's own action — never the untouched load-time seed, even if the two
    // positions coincide.
    ed.implicitCaretAt = null;
    ed.verticalGoal.clear(); // an edit ends a run of vertical moves (HF-164)
    if (!keepSelection || !ed.selection) ed.selection = adoptEditPosition(node, offset); // a setting keeps the reader's selection (HF-283)
    // A content mutation invalidates a row/column/table selection the same way it
    // invalidates an object selection. Without this the accent fill survives
    // typing, deleting, undo and arrow keys, so the editor claims a whole table is
    // selected while the real selection is a collapsed caret in one cell — Copy
    // returns "" and Backspace removes a single character.
    ed.tableRange.clear();
    // The paste-options chip only ever means "redo the paste you just made,
    // differently" — it acts by undoing it. Once any other edit has landed, an
    // undo removes that edit instead, so the offer has expired. Retiring it here,
    // where every edit passes, is what stops a ⌘Z before the click from silently
    // deleting the sentence the user typed before the paste.
    if (ed.pasteOptionsShowing()) ed.hidePasteOptions();
    dropVanishedObjectSelection();
    if (newCount !== ed.pages.length) {
      await ed.renderAll(); // structural change (page added/removed): rebuild the list
    } else {
      for (const i of dirty) ed.repaintPage(i);
      ed.drawSelection();
    }
    ed.scheduleChromeRefresh({ stats: true, outline: true });
    // A review edit is not a text edit: the caret never moved, and it is usually
    // nowhere near the comment being worked on — so scrolling it into view sent
    // the reader back to wherever the caret happened to be (the top, normally).
    // ...and neither is an OBJECT edit: the user is manipulating the object, not
    // the caret, so following the caret throws them off it (`object-edit-keeps-view`).
    if (!keepView && !ed.objectSelection) ed.scrollCaretIntoView();
    // Any edit can introduce a scalar no provisioned face covers — an emoji from
    // the picker, a paste from another app, an IME commit. Coverage used to be
    // checked only on open and for the two edits known to add symbols (checklist
    // and list markers), so everything else rendered as notdef boxes until the
    // document was reopened. `missingCoverage()` is an engine-side scan that
    // returns nothing in the overwhelmingly common case, and the fetch only
    // happens when a bucket is genuinely absent, so this stays off the hot path.
    void ed.ensureGlyphCoverage("this edit");
  }

  /** Drops an object selection whose object is no longer in the document.
   *
   *  Undoing an insert (or any edit that removes the selected object) left the
   *  selection, its handles and its context bar pointing at a node the model no
   *  longer holds — chrome hovering over nothing, and a Fill button that would
   *  fail. `deleteSelectedObject` clears the selection itself; every other path
   *  that can remove an object needed this. */
  function dropVanishedObjectSelection() {
    if (!ed.doc || !ed.objectSelection) return;
    let rect = [];
    try {
      rect = ed.doc.objectRect(ed.objectSelection.node);
    } catch {
      rect = [];
    }
    if (rect.length >= 5) {
      refreshObjectCapabilities();
      return;
    }
    ed.objectSelection = null;
    ed.objectCropSession = null;
    ed.updateObjectSelectionState();
    ed.updateObjectContextBar();
  }

  /** Re-reads the ENGINE's capability bits for the object that is still selected.
   *
   *  They were snapshotted when the object was selected and never read again, so
   *  an edit that CHANGES what the object can do left the chip offering the old
   *  answer. `setObjectAnchorKind` is exactly that edit: an object taken in line
   *  can no longer be moved or wrapped — the flow decides where it sits — and the
   *  chip went on offering Move and a wrap mode for a node that has no anchor.
   *  The same is true of grouping, ungrouping and anything else that rewrites the
   *  node under a live selection.
   *
   *  Cost: O(objects) once per EDIT, never per keystroke and never per pointermove
   *  — `objectOrder()` is the same walk `objectPresence` already memoises, and a
   *  document with no selected object pays nothing. */
  function refreshObjectCapabilities() {
    const held = ed.objectSelection;
    if (!held || held.mode === "editing") return;
    let entries;
    try {
      entries =
        held.ref.subject === held.ref.root
          ? JSON.parse(ed.doc.objectOrder())
          : JSON.parse(ed.doc.objectDescendants(held.ref.root));
    } catch {
      return;
    }
    const fresh = Array.isArray(entries)
      ? entries.find((entry) => entry.node === held.node)
      : null;
    // Not in the list any more (it became a group's child, say): leave what is
    // held rather than guessing. `objectRect` already proved it is still placed.
    if (!fresh) return;
    ed.objectSelection = {
      ...held,
      kind: fresh.kind ?? held.kind,
      anchored: fresh.anchored ?? held.anchored,
      ...ed.objectCapabilities(fresh),
    };
  }

  /** Ends the current typing gesture. The next printable key receives a fresh
   * session id and therefore cannot merge with earlier history. */
  function breakTypingSession() {
    ed.typingSessionActive = false;
    ed.lastTypingAt = 0;
  }

  /** Returns the gesture id for an adjacent typing tick. A pause is a semantic
   * boundary even if the caret has not moved. */
  function typingSessionForKey() {
    const now = performance.now();
    if (!ed.typingSessionActive || now - ed.lastTypingAt > ed.TYPING_PAUSE_MS) {
      ed.typingSession = (ed.typingSession + 1) >>> 0;
      if (ed.typingSession === 0) ed.typingSession = 1;
    }
    ed.typingSessionActive = true;
    ed.lastTypingAt = now;
    return ed.typingSession;
  }

  // Pointer interaction always establishes a new caret/selection/command gesture.
  document.addEventListener("pointerdown", breakTypingSession, { capture: true });

  /** True (after showing the standard status message and returning focus to the
   *  canvas) if Suggesting mode should block a command whose mutation cannot -
   *  yet or ever - be represented as a tracked revision. This is the single
   *  fail-closed gate shared by every mutation path (`runEdit`, `runNodeEdit`,
   *  `runToolbarEdit`): a command that bypasses tracking must never silently
   *  apply while the mode still reads Suggesting (REVIEW-GAP-004). */
  function blockUntrackedInSuggesting({ paragraphLevel = false } = {}) {
    if (ed.reviewMode !== "suggesting") return false;
    // Paragraph formatting IS trackable now: the engine records a `w:pPrChange`
    // holding the prior while Suggesting is on (docs/108 phase 2, HF-131).
    if (paragraphLevel) return false;
    ed.setStatus("This command cannot be tracked yet; switch to Editing to apply it", "error");
    ed.focusEditorSurface();
    return true;
  }



  /** The column a run of vertical caret moves is aiming at (HF-164). */
  return {
    applyEditResult, blockUntrackedInSuggesting, breakTypingSession, paintArrival,
    typingSessionForKey,
  };
}
