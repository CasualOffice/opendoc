// SPDX-License-Identifier: Apache-2.0
// The seam between the editor's own repaint and Casual AI's live co-editor
// (`docs/183` §23, `ai_coeditor.mjs`).
//
// The co-editor is created on demand by the plugin platform, which loads only
// when an AI provider exists; the overlay repaint and the review-mode switch run
// from boot. They meet here instead of through the editor context, so `main.js`
// carries no line for a feature that, with no provider, does not exist (owner
// rule, 2026-10-31: the editor with no provider is identical to today). With
// nothing registered both calls are a null check.
//
// It is also how a shared room hears where Casual AI is writing (`docs/178`
// §10): the co-editor says when its anchor moved, the room's presence reads it
// and puts it in this participant's own caret frame, so the other people see
// the AI's caret. With no provider nothing moves and the room sends exactly
// what it sent before.
//
// Complexity: O(1).

/** Casual AI's colour: its pointer, caret and flag, here and on other people's
 *  pages in a shared room. */
export const AI_COLOR = "#7c3aed";

let coeditor = null;
let accessListener = null;
const movedListeners = new Set();

/** Registers the page's co-editor and the access-change listener. */
export function registerAiLive({ editor = null, accessChanged = null } = {}) {
  coeditor = editor;
  accessListener = accessChanged;
}

/** Paints the AI's caret, flag and pointer on the overlay being repainted. */
export function paintAiLive(place) {
  coeditor?.paint(place);
}

/** Tells the AI layer that what the AI may do may have changed (a mode switch). */
export function aiAccessMayHaveChanged() {
  accessListener?.();
}

/** Where Casual AI is writing now, `{node, offset}`, or null when no turn is
 *  writing. Read by the room's presence for this participant's caret frame. */
export function aiLiveAnchor() {
  return coeditor?.writingAnchor?.() ?? null;
}

/** Subscribes to Casual AI's anchor moving (a batch landed, a turn began or
 *  ended). Returns the unsubscribe. */
export function onAiLiveMoved(listener) {
  movedListeners.add(listener);
  return () => movedListeners.delete(listener);
}

/** Called by the co-editor whenever its anchor moved or its turn stopped. */
export function aiLiveMoved() {
  for (const listener of movedListeners) {
    try {
      listener();
    } catch {
      /* a listener's failure is not the co-editor's */
    }
  }
}
