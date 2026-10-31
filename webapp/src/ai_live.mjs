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
// Complexity: O(1).

let coeditor = null;
let accessListener = null;

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
