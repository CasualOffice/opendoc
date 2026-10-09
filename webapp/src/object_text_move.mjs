// Moving an in-line object to a new place in the text (`docs/109` UX-OB-02).
//
// The competitive standard, which this follows rather than reinterprets. In
// Word and Google Docs a picture that sits in the line of text is picked up and
// dragged: a drop caret follows the pointer through the words, and on release
// the picture is cut from where it was and inserted at the caret as ONE undo
// step. Ctrl-drag (Option-drag on a Mac) leaves the original and drops a copy.
// Esc cancels, and dropping it back where it came from does nothing. Word's
// keyboard twin is F2: the status bar asks "Move to where?", you put the
// insertion point somewhere, and Enter moves it there — Shift+F2 copies.
// Before this, the drag here only said why it would not move (`docs/109`
// HF-259).
//
// What lives where. The EDIT is the engine's `moveInlineObject` — one
// transaction built from the RemoveInlineObject/InsertInlineObject pair that
// already exist as each other's inverse, so the object travels as itself with
// every property it has. This module is the GESTURE: the threshold that tells
// a drag from the click that selected the object, the drop caret, the edge
// scroll, the copy key, and the keyboard mode. It has no hit test of its own —
// the drop point is whatever `hitTest`, the click's own hit test, answers for
// the pointer — and no geometry of its own: the drop caret is the engine's
// `caretRect` for that point. A second hit test is how a drop would come to
// land somewhere a click would not.
//
// Complexity: per pointer move, one `hitTest` and one `caretRect`, each bounded
// by the lines on one page — what a drag-selection already pays — and nothing
// that walks the document. The commit is one engine call per gesture.

import { APPLE_PLATFORM } from "./keyboard.mjs";
import { edgeScrollStep } from "./edge_scroll.mjs";

/** How far the pointer must travel, in CSS pixels, before a press on a selected
 *  object counts as a DRAG rather than the click that selected it. The same
 *  allowance the refused drag gives (`object_refusal.mjs`). */
export const DRAG_THRESHOLD_PX = 6;

/**
 * Whether the copy key is held: Ctrl in Word for Windows and in Docs, Option in
 * Word for Mac (where Ctrl-click is a right-click). Read on every move, so the
 * drop caret can show which it will be, and again on release, which decides.
 *
 * O(1).
 *
 * @param {{ctrlKey?: boolean, altKey?: boolean}} event
 * @param {string} platform `keyboardPlatform()`'s answer
 */
export function copyKeyHeld(event, platform) {
  return platform === APPLE_PLATFORM ? event.altKey === true : event.ctrlKey === true;
}

/** The copy key's name for the status line: never spelled with a Mac glyph on
 *  another platform (`105` UX-009). */
export function copyKeyLabel(platform) {
  return platform === APPLE_PLATFORM ? "⌥" : "Ctrl";
}

/**
 * Runs the engine move and says what happened. Shared by the drag and the
 * keyboard mode, so the two cannot report one gesture two ways.
 *
 * The placed copy, when there is one, becomes the selection — Word selects the
 * copy it dropped. A move keeps its id, so the object the user was holding is
 * still the selection without anything being re-selected here.
 */
async function commitMove(io, { node, selection, target, copy, reselect }) {
  let placed = "";
  let changed = false;
  const landed = await io.runEdit(
    () => {
      const result = io.doc().moveInlineObject(node, target.node, target.offset, copy);
      placed = result.placedObject ?? "";
      // Dropped back where it came from: the engine reports the document
      // unchanged, and so does this.
      changed = (result.dirtyPages?.length ?? 0) > 0;
      return result;
    },
    { gate: true, keepView: true },
  );
  if (!landed) return false;
  if (placed) io.select(placed, selection);
  else if (reselect) io.select(node, selection);
  if (changed) io.setStatus(io.t(copy ? "object.copied" : "object.moved"));
  return changed;
}

/**
 * The pointer drag of a selected in-line object.
 *
 * `io` is its whole contact with the application:
 *
 *   `doc()`                  the engine document
 *   `pageAt(x, y)`           the page record under a client point, or null
 *   `pageByNumber(n)`        the page record for 1-based page `n`
 *   `pointToTwip(page, ev)`  a client point → that page's twips
 *   `scaleOf(page)`          `{sx, sy}` twip → CSS px
 *   `blocked()`              true (having said why) when the mode forbids edits
 *   `platform`               `keyboardPlatform()`'s answer
 *   `runEdit(thunk, opts)`   the gated edit path; resolves to whether it landed
 *   `select(node, sel)`      select `node` with `sel`'s kind and capabilities
 *   `setStatus(text, kind)`  the status line
 *   `t(key, params)`         the catalogue
 *   `viewport()`             the scrolling element
 *
 * The pointer's shape is not set here: the hover router owns it, and reads
 * `dragging()` as the move gesture it already draws a `move` cursor for. The
 * drop caret says which of move and copy the release will be.
 *
 * @param {object} io
 */
export function createObjectTextDrag(io) {
  let drag = null;
  // Injectable, so a unit test drives the gesture without a frame loop that
  // outlives a failed assertion; the browser's own scheduler otherwise.
  const frame = io.frame ?? ((fn) => requestAnimationFrame(fn));
  const cancelFrame = io.cancelFrame ?? ((id) => cancelAnimationFrame(id));

  /** Removes the drop caret and stops the scroll loop. */
  function clear(held) {
    held.caret?.remove();
    held.caret = null;
    if (held.scrollFrame) cancelFrame(held.scrollFrame);
    held.scrollFrame = 0;
  }

  /** Hit-tests the last pointer position and moves the drop caret there. One
   *  `hitTest` and one `caretRect`, nothing else. */
  function retarget(held, clientX, clientY) {
    const page = io.pageAt(clientX, clientY);
    if (!page) return;
    const { x, y } = io.pointToTwip(page, { clientX, clientY });
    const hit = io.doc().hitTest(page.pageNumber, x, y);
    if (!hit) return;
    const target = { node: hit.node, offset: hit.offset };
    hit.free?.();
    held.target = target;
    const rect = io.doc().caretRect(target.node, target.offset); // [page, x, y, w, h]
    const caretPage = rect.length >= 5 ? io.pageByNumber(rect[0]) : null;
    if (!caretPage?.overlay) return;
    const { sx, sy } = io.scaleOf(caretPage);
    // Re-placed rather than moved: an overlay repaint (an edge scroll that
    // materialises a page) clears its children, and a caret that silently
    // disappeared mid-drag would leave the user dropping blind.
    if (!held.caret || held.caret.parentNode !== caretPage.overlay) {
      held.caret?.remove();
      held.caret = caretPage.overlay.ownerDocument.createElement("div");
      held.caret.className = "object-drop-caret";
      caretPage.overlay.appendChild(held.caret);
    }
    held.caret.style.left = `${rect[1] * sx}px`;
    held.caret.style.top = `${rect[2] * sy}px`;
    held.caret.style.height = `${Math.max(1, rect[4] * sy)}px`;
    held.caret.dataset.copy = String(held.copy);
  }

  /** While the drag is held near a viewport edge, scrolls towards it and
   *  re-targets, once per frame (`edge_scroll.mjs` — the drag-selection's own
   *  rule). Stops itself the moment the drag ends. */
  function scrollLoop(held) {
    held.scrollFrame = frame(() => {
      held.scrollFrame = 0;
      if (drag !== held) return;
      const viewport = io.viewport();
      const { dx, dy } = edgeScrollStep(viewport.getBoundingClientRect(), held.lastX, held.lastY);
      if (dx || dy) {
        const before = [viewport.scrollLeft, viewport.scrollTop];
        viewport.scrollLeft = Math.max(0, before[0] + dx);
        viewport.scrollTop = Math.max(0, before[1] + dy);
        if (viewport.scrollLeft !== before[0] || viewport.scrollTop !== before[1]) {
          retarget(held, held.lastX, held.lastY);
        }
      }
      scrollLoop(held);
    });
  }

  return {
    /**
     * Watches a press on the selected object, which becomes a drag once it
     * travels past the threshold. Returns whether it is watching — false for an
     * object that cannot move in the text, so the caller explains why instead.
     */
    arm(event, page, selection) {
      if (!selection?.canMoveInText) return false;
      drag = {
        node: selection.node,
        selection,
        startX: event.clientX,
        startY: event.clientY,
        lastX: event.clientX,
        lastY: event.clientY,
        started: false,
        target: null,
        copy: false,
        caret: null,
        scrollFrame: 0,
        overlay: page.overlay ?? null,
        pointerId: event.pointerId,
      };
      return true;
    },

    /** Follows the pointer. Returns whether a press is being watched, so the
     *  caller's other pointer handlers stand down. */
    move(event) {
      const held = drag;
      if (!held) return false;
      if (event.buttons === 0) {
        clear(held);
        drag = null;
        return false;
      }
      held.lastX = event.clientX;
      held.lastY = event.clientY;
      if (!held.started) {
        const far =
          Math.abs(event.clientX - held.startX) + Math.abs(event.clientY - held.startY) >=
          DRAG_THRESHOLD_PX;
        if (!far) return true;
        // Viewing or Suggesting: the reason is said once, by the mode's own
        // sentence, and the press goes back to being the click that selected.
        if (io.blocked()) {
          drag = null;
          return true;
        }
        held.started = true;
        // The drag takes the pointer, the way the float move does, so it is not
        // lost the moment it leaves the sheet it started on. Only once it IS a
        // drag: the click that merely selected must not hold the pointer, or
        // the capture's release lands on whatever gesture the next press began.
        try {
          held.overlay?.setPointerCapture?.(held.pointerId);
        } catch {
          // The pointer is already gone; the release will find nothing to do.
        }
        io.setStatus(io.t("object.dragInText.hint", { key: copyKeyLabel(io.platform) }));
        scrollLoop(held);
      }
      held.copy = copyKeyHeld(event, io.platform);
      retarget(held, event.clientX, event.clientY);
      event.preventDefault?.();
      return true;
    },

    /** Drops on release. Returns whether a press was being watched. A press
     *  that never became a drag was the click that selected, and commits
     *  nothing. */
    finish(event) {
      const held = drag;
      if (!held) return false;
      drag = null;
      clear(held);
      if (!held.started || !held.target) return true;
      event.preventDefault?.();
      const copy = copyKeyHeld(event, io.platform);
      io.setStatus("");
      void commitMove(io, { node: held.node, selection: held.selection, target: held.target, copy });
      return true;
    },

    /** Esc, a lost pointer, a blurred window: nothing moves. Returns whether a
     *  drag was cancelled. */
    cancel() {
      const held = drag;
      if (!held) return false;
      drag = null;
      clear(held);
      if (held.started) io.setStatus(io.t("object.moveTo.cancelled"));
      return held.started;
    },

    /** Whether a press on an in-line object is being watched. */
    active: () => drag !== null,

    /** Whether that press has become a drag — what the hover router reads as
     *  the object-move gesture. */
    dragging: () => drag?.started === true,
  };
}

/** The keys that move the insertion point and so leave a "Move to where?"
 *  waiting: Word lets you navigate there, by keyboard or by clicking. */
const NAVIGATION_KEYS = new Set([
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "Shift",
  "Control",
  "Alt",
  "Meta",
]);

/**
 * Word's F2 ("Move to where?") and Shift+F2 ("Copy to where?"): the keyboard
 * surface for the same move, and the only one a screen-reader user has.
 *
 * F2 on a selected in-line object leaves the object for a caret beside it and
 * asks where; the arrows, Home/End, Page Up/Down — or a click — put the
 * insertion point; Enter moves it there and selects it again; Esc cancels.
 * Typing anything else abandons the move and lets the key do what it does, as
 * in Word.
 *
 * `io` is the drag's, plus:
 *
 *   `selection()`            the object selection, or null
 *   `caret()`                the current insertion point `{node, offset}`
 *   `leaveObject()`          drop the object selection for a caret beside it
 *   `refusal(sel)`           the sentence for a selected object that cannot move
 *
 * O(1) per key.
 *
 * @param {object} io
 */
export function createKeyboardObjectMove(io) {
  let pending = null;

  /** Starts the mode on the selected object; says why when it cannot. */
  function begin(copy) {
    const selection = io.selection();
    if (!selection) return false;
    if (!selection.canMoveInText) {
      io.setStatus(io.refusal(selection), "error");
      return true;
    }
    if (io.blocked()) return true;
    pending = { node: selection.node, selection, copy };
    io.leaveObject();
    io.setStatus(io.t(copy ? "object.copyTo.prompt" : "object.moveTo.prompt"));
    return true;
  }

  return {
    begin,

    /** Handles a key while the mode is open, or F2 on a selected object.
     *  Returns whether the key was consumed. */
    onKey(event) {
      const bare = !event.ctrlKey && !event.metaKey && !event.altKey;
      if (!pending) {
        if (event.key !== "F2" || !bare || !io.selection()) return false;
        event.preventDefault();
        return begin(event.shiftKey);
      }
      if (event.key === "Escape") {
        event.preventDefault();
        pending = null;
        io.setStatus(io.t("object.moveTo.cancelled"));
        return true;
      }
      if (event.key === "Enter" && bare && !event.shiftKey) {
        event.preventDefault();
        const held = pending;
        pending = null;
        const target = io.caret();
        if (target) {
          void commitMove(io, { ...held, target, reselect: true });
        }
        return true;
      }
      if (NAVIGATION_KEYS.has(event.key)) return false;
      // Anything else abandons the move and does what it does.
      pending = null;
      io.setStatus("");
      return false;
    },

    /** Whether a "Move to where?" is waiting. */
    pending: () => pending !== null,

    /** Abandons a waiting move without a word — for paths (a new document) that
     *  make it meaningless. */
    reset() {
      pending = null;
    },
  };
}
