// Object alt text, moving and nudging an object with snap guides, and entering
// an object's text. Moved out of `main.js` unchanged (docs/177); it reaches the
// rest of the editor only through the context `ed`.
import { OBJECT_LABELS, traversalRoot } from "./object_traversal.mjs";
import { registerModal } from "./modal.mjs";
import { clearGuides, paintGuides } from "./object_guides.mjs";
import { pageSnapTargets, snapBox } from "./object_snap.mjs";
import { EMU_PER_TWIP } from "./units.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createObjectMoves(ed) {
  // ---- Object alt text + crop dialogs -----------------------------------------
  // Reuse the shared `.dialog-overlay`/`.dialog-card` system (as the bookmark
  // manager does). The node is captured on open so the dialog stays bound to one
  // object even though `objectSelection` remains live behind the modal overlay.
  const altTextDialog = document.getElementById("altTextDialog");
  const altTextInput = document.getElementById("altTextInput");
  const altTextForm = document.getElementById("altTextForm");
  const altTextNote = document.getElementById("altTextNote");
  const altTextClose = document.getElementById("altTextClose");
  const altTextCancel = document.getElementById("altTextCancel");
  const ALT_TEXT_HINT = "Leave empty to remove the alt text. Enter applies; Shift+Enter adds a line.";
  /** The object a currently-open object dialog is editing. The dialog stays bound
   *  to it even though `objectSelection` keeps moving behind the modal. */
  let objectDialogNode = null;

  /** True (after surfacing the standard read-only/untracked message) if an object
   *  edit must not apply in the current review mode. Object edits are fail-closed
   *  in Suggesting (untracked) and read-only in Viewing — the same gate `runEdit`
   *  applies for delete/wrap; the dialogs pre-check it so they can close cleanly. */
  function objectEditBlocked() {
    return ed.blockMutationInViewing() || ed.blockUntrackedInSuggesting();
  }

  function setAltTextNote(message, isError) {
    altTextNote.textContent = message || ALT_TEXT_HINT;
    altTextNote.classList.toggle("error", !!isError && !!message);
  }

  const altTextModal = altTextDialog
    ? registerModal(altTextDialog, {
        initialFocus: () => altTextInput,
        fallbackFocus: () => ed.pagesEl,
        onOpen: () => altTextInput.select(),
        onClose: () => {
          objectDialogNode = null;
        },
      })
    : null;

  function openAltTextDialog() {
    if (!ed.doc || !ed.objectSelection?.canAltText || !altTextModal) return;
    objectDialogNode = ed.objectSelection.node;
    // Prefill the current alt text so the user refines it rather than blind-
    // overwriting (Word/Docs both show the existing description).
    altTextInput.value = ed.doc.objectDescr(ed.objectSelection.node) ?? "";
    setAltTextNote("", false);
    altTextModal.open();
  }

  function closeAltTextDialog() {
    altTextModal?.close();
  }

  /** Applies the alt text as one undoable action. The engine rejects an
   *  over-length description; that is shown inline (its raw error name is internal
   *  vocabulary) rather than as the toolbar's generic status. Empty input sends
   *  `null`, which clears the alt text. */
  function applyAltText() {
    if (!ed.doc || !objectDialogNode) return;
    const text = altTextInput.value.trim();
    if (objectEditBlocked()) {
      closeAltTextDialog();
      return;
    }
    let res;
    try {
      res = ed.doc.setObjectDescr(objectDialogNode, text || null);
    } catch (err) {
      console.warn("setObjectDescr ignored:", err?.message ?? err);
      setAltTextNote("That description is too long. Try a shorter one.", true);
      altTextInput.focus();
      return;
    }
    ed.applyEditResult(res).then(() => {
      closeAltTextDialog();
      ed.setStatus(text ? "Alt text updated" : "Alt text removed");
    });
  }

  if (altTextDialog) {
    altTextForm.addEventListener("submit", (event) => {
      event.preventDefault();
      applyAltText();
    });
    altTextInput.addEventListener("input", () => {
      if (altTextNote.classList.contains("error")) setAltTextNote("", false);
    });
    altTextInput.addEventListener("keydown", (event) => {
      // Enter applies; Shift+Enter inserts a newline (multi-line descriptions).
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        applyAltText();
      }
    });
    altTextClose.addEventListener("click", () => closeAltTextDialog());
    altTextCancel.addEventListener("click", () => closeAltTextDialog());
  }

  // Threshold (twips) beyond which a float body drag counts as a move, not a click.
  const MOVE_THRESHOLD_TWIP = 40;

  /** Begins a floating-object move drag (docs/85 §5.3): previews an outline that
   *  follows the pointer; the model is untouched until release. */
  // How close a dragged edge comes before the page pulls it onto a line, in CSS
  // pixels — converted to twips at the CURRENT zoom, so the pull is the same
  // distance under the finger at 50% as at 200%.
  const SNAP_TOLERANCE_PX = 7;

  /** The alignment lines `page` offers, and the snap radius in that page's twips.
   *  Called ONCE per gesture (at pointer-down), never per pointer move: it asks
   *  the engine for the page's ruler geometry, and a drag that re-asked would be
   *  paying an engine call per sample for an answer that cannot change. */
  function snapContextFor(page) {
    let targets;
    try {
      const g = ed.doc.pageRulerGeometry(page.pageNumber - 1);
      targets = pageSnapTargets({
        widthTwip: g.widthTwip,
        heightTwip: page.hTwip,
        marginStartTwip: g.marginStartTwip,
        marginEndTwip: g.marginEndTwip,
      });
      g.free?.();
    } catch {
      return null; // no geometry, no guides — the drag is simply free
    }
    const { sx } = ed.scaleOf(page);
    return { targets, tolerance: sx > 0 ? SNAP_TOLERANCE_PX / sx : 0 };
  }

  /** Draws (or hides) the alignment guides for a live drag, on the drag's page. */
  function paintSnapGuides(drag, guides) {
    drag.guideEls = paintGuides(drag.page.overlay, drag.guideEls, guides, ed.scaleOf(drag.page));
  }

  /** Removes a gesture's guides. */
  function clearSnapGuides(drag) {
    clearGuides(drag?.guideEls);
    if (drag) drag.guideEls = null;
  }

  function startObjectMove(event, page, node) {
    if (!ed.doc || !ed.objectSelection?.canMove || ed.objectSelection.node !== node) return;
    if (ed.reviewMode === "viewing" || ed.reviewMode === "suggesting") {
      // Selection still happened; a move is simply not offered in these modes.
      return;
    }
    const rect = ed.doc.objectRect(node); // [page, x, y, w, h] twips
    if (rect.length < 5) return;
    const [, x, y, w, h] = rect;
    const preview = document.createElement("div");
    preview.className = "object-resize-preview";
    const { sx, sy } = ed.scaleOf(page);
    preview.style.left = `${x * sx}px`;
    preview.style.top = `${y * sy}px`;
    preview.style.width = `${w * sx}px`;
    preview.style.height = `${h * sy}px`;
    page.overlay.appendChild(preview);
    ed.objectMoveDrag = {
      node,
      root: ed.objectSelection.ref.root,
      // Set only for a shape INSIDE a group, which moves within its group
      // instead of moving the group. `commitObjectMove` branches on it.
      child: insideGroupSelection() ? node : null,
      page,
      startClientX: event.clientX,
      startClientY: event.clientY,
      startX: x,
      startY: y,
      startW: w,
      startH: h,
      lastX: x,
      lastY: y,
      moved: false,
      preview,
      snap: snapContextFor(page),
      guideEls: null,
    };
    // The move gesture takes the pointer the way the resize gesture already does:
    // a touch drag otherwise never became a move at all (the browser had already
    // committed to scrolling by the time `updateObjectMove` ran), and a mouse drag
    // died the moment the pointer left the sheet.
    event.preventDefault();
    page.overlay?.setPointerCapture?.(event.pointerId);
  }

  /** Updates the move preview from the pointer delta (page-local twips), pulled
   *  onto the page's alignment lines when an edge or the centre comes close. */
  function updateObjectMove(event) {
    if (!ed.objectMoveDrag) return;
    const drag = ed.objectMoveDrag;
    const { sx, sy } = ed.scaleOf(drag.page);
    const dxTwip = Math.round((event.clientX - drag.startClientX) / sx);
    const dyTwip = Math.round((event.clientY - drag.startClientY) / sy);
    let x = Math.max(0, drag.startX + dxTwip);
    let y = Math.max(0, drag.startY + dyTwip);
    // Alt is the Word/Docs escape hatch: hold it and the page stops pulling, so a
    // deliberate 2mm-off-centre placement is still reachable.
    if (drag.snap && !event.altKey) {
      const snapped = snapBox({ x, y, w: drag.startW, h: drag.startH }, drag.snap.targets, drag.snap.tolerance);
      x = snapped.x;
      y = snapped.y;
      paintSnapGuides(drag, snapped.guides);
    } else if (drag.guideEls) {
      paintSnapGuides(drag, []);
    }
    drag.lastX = x;
    drag.lastY = y;
    if (Math.abs(dxTwip) + Math.abs(dyTwip) > MOVE_THRESHOLD_TWIP) drag.moved = true;
    drag.preview.style.left = `${x * sx}px`;
    drag.preview.style.top = `${y * sy}px`;
    event.preventDefault();
  }

  /** Commits (or cancels) a float move on release: one `SetAnchor` to the new
   *  absolute page position (page-local twips → EMU). Returns whether a drag was
   *  active. A bare click (no movement) commits nothing — it was a select. */
  function finishObjectMove(event) {
    if (!ed.objectMoveDrag) return false;
    const drag = ed.objectMoveDrag;
    ed.objectMoveDrag = null;
    drag.preview.remove();
    clearSnapGuides(drag);
    event.preventDefault();
    if (drag.moved) {
      ed.runEdit(() => commitObjectMove(drag), { gate: true });
    } else {
      ed.drawSelection();
    }
    return true;
  }

  /** Nudges the selected floating object one `objectNudge` step (`object_keys.mjs`
   *  owns which key means which step), as a single `SetAnchor` op (gated in
   *  Viewing/Suggesting like a drag-move). */
  function nudgeSelectedObject({ dx, dy, step }) {
    if (!ed.doc || !ed.objectSelection?.canMove) return;
    const subject = ed.objectSelection.node;
    const root = ed.objectSelection.ref.root;
    const rect = ed.doc.objectRect(subject); // [page, x, y, w, h] twips
    if (rect.length < 5) return;
    if (insideGroupSelection()) {
      ed.runEdit(
        () => ed.doc.moveGroupChildBy(subject, dx * step * EMU_PER_TWIP, dy * step * EMU_PER_TWIP),
        { gate: true },
      );
      return;
    }
    const nx = Math.max(0, rect[1] + dx * step);
    const ny = Math.max(0, rect[2] + dy * step);
    ed.runEdit(() => ed.doc.setObjectAnchorPosition(root, nx * EMU_PER_TWIP, ny * EMU_PER_TWIP), {
      gate: true,
    });
  }

  /** Whether the selection is a shape INSIDE a group, which moves within the
   *  group rather than moving the group. */
  function insideGroupSelection() {
    return traversalRoot(ed.objectSelection?.ref) !== null;
  }

  /** Commits a finished drag.
   *
   *  A shape inside a group moves by a DELTA in its own parent's space;
   *  everything else sets an absolute page position. Using the absolute call for
   *  both is what made dragging a grouped shape move the whole group
   *  (`docs/109` HF-173) — the object being dragged was the child and the object
   *  being moved was its root. */
  function commitObjectMove(drag) {
    if (drag.child) {
      return ed.doc.moveGroupChildBy(
        drag.child,
        (drag.lastX - drag.startX) * EMU_PER_TWIP,
        (drag.lastY - drag.startY) * EMU_PER_TWIP,
      );
    }
    return ed.doc.setObjectAnchorPosition(
      drag.root,
      drag.lastX * EMU_PER_TWIP,
      drag.lastY * EMU_PER_TWIP,
    );
  }

  /** Aborts an in-progress float move, discarding the preview (Escape / cancel). */
  function cancelObjectMove() {
    if (!ed.objectMoveDrag) return;
    ed.objectMoveDrag.preview.remove();
    clearSnapGuides(ed.objectMoveDrag);
    ed.objectMoveDrag = null;
    ed.drawSelection();
  }

  /** Enters a container object's edit mode (docs/85 §4.3). A leaf object (image)
   *  has no edit mode — its primary context action is a later image slice. Placing
   *  a caret *inside* a text box's flowed body is the P1G-OBJ-TEXTBOX slice; here
   *  the grammar transitions state and the surrounding-text caret is shown. */
  function enterObjectEditMode(at = null) {
    if (!ed.objectSelection) return;
    if (ed.objectSelection.kind === "group" && ed.descendSelectedGroup()) return;
    if (!ed.objectSelection.canEditText) {
      const label = OBJECT_LABELS[ed.objectSelection.kind] ?? "Object";
      const hint = ed.objectSelection.canResize ? " — drag its handles to resize" : "";
      ed.setStatus(`${label} selected${hint}`, "", { timeout: 3000 });
      return;
    }
    ed.objectSelection = { ...ed.objectSelection, mode: "editing" };
    // Put the caret IN the box. Edit mode used to be a state flag and nothing
    // else: the border changed, and there was no caret and nowhere for a keystroke
    // to go, so "entering" a text box did not let you type in it. The box's
    // content is ordinary block content in the same id space, so this is an
    // ordinary caret position once the geometry can be resolved.
    // Prefer the point the user actually clicked; fall back to probing the box
    // when entry came from the keyboard (Enter on a selected box), which has no
    // point of its own.
    const caret = at ?? caretInsideObject(ed.objectSelection.node);
    if (caret) {
      ed.selection = { anchor: caret, focus: caret };
      ed.setStatus("Editing the text box — press Esc to select it, Esc again to leave");
    } else {
      ed.setStatus("This text box has no editable content yet", "error");
    }
    ed.drawSelection();
    ed.updateToolbar();
  }

  /** A caret position inside object `node`'s text, found by probing its own
   *  rectangle — the engine resolves a point to text-box content, and the object's
   *  placed rect is the one region guaranteed to be inside it. */
  function caretInsideObject(node) {
    const modelCaret = ed.doc?.textBoxCaret?.(node);
    if (modelCaret) {
      const at = { node: modelCaret.node, offset: modelCaret.offset };
      modelCaret.free?.();
      return at;
    }
    const flat = ed.doc?.objectRect(node);
    if (!flat || flat.length < 5) return null;
    const [pageNumber, x, y, w, h] = flat;
    // Probe down the box's own left-to-middle band; the first line of content is
    // near the top, but vertical anchoring can push it down.
    for (let dy = 4; dy < h; dy += Math.max(20, Math.round(h / 12))) {
      const hit = ed.doc.textBoxHitTest(pageNumber, Math.round(x + Math.min(w / 2, 200)), Math.round(y + dy));
      if (!hit) continue;
      const at = { node: hit.node, offset: hit.offset };
      hit.free?.();
      return at;
    }
    return null;
  }

  /** Paints a clickable target over each checklist item's checkbox marker (docs/67
   *  — checklist authoring). The checkbox glyph itself is baked into the page raster
   *  by the layout engine; this overlay is the *model-as-truth* click target
   *  (`doc.checklistMarkers()` gives each marker's engine rect + node + state) that
   *  toggles the item's checked state via one edit op, then re-renders. Like the
   *  review markers it never mutates the canvas; unlike them it carries its own
   *  handler (the checkbox is a control, not passive text). Gated through
   *  `runNodeEdit` so it is blocked in Viewing and Suggesting, consistent with the
   *  other list edits. */
  return {
    cancelObjectMove, clearSnapGuides, enterObjectEditMode, finishObjectMove, insideGroupSelection,
    nudgeSelectedObject, objectEditBlocked, openAltTextDialog, paintSnapGuides, snapContextFor,
    startObjectMove, updateObjectMove,
  };
}
