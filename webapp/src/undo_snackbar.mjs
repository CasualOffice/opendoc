// SPDX-License-Identifier: Apache-2.0
// The undo snackbar (PROPOSAL D-11; audit REVIEW-12): an action that changes a
// lot at once — Accept all, Reject all, Resolve — says what it did ONCE, with
// Undo beside it, for a few seconds. Google Docs' snackbar is the model; Word
// relies on Ctrl+Z alone, which leaves a person who did not see what changed
// with nothing to act on.
//
// One element (`#undoSnackbar` in `editor.html`), one message at a time: a new
// action replaces the previous offer, because Undo always undoes the LAST step
// and an older offer would undo something else. The live region is the
// element's own `role="status"`, so the sentence is also heard.
//
// Complexity: O(1).
import { t } from "./i18n.mjs";

/** How long the offer stays, in ms: long enough to read a short sentence and
 *  reach the button (Material's 4-10s band), short enough not to linger. */
export const UNDO_SNACKBAR_MS = 6000;

let timer = 0;
let onUndo = null;

function elements(doc) {
  return {
    bar: doc.getElementById("undoSnackbar"),
    text: doc.getElementById("undoSnackbarText"),
    undo: doc.getElementById("undoSnackbarUndo"),
  };
}

/** Hides the offer. */
export function hideUndoSnackbar(doc = document) {
  clearTimeout(timer);
  timer = 0;
  onUndo = null;
  const { bar } = elements(doc);
  if (bar) bar.hidden = true;
}

/**
 * Shows `message` with an Undo button that runs `undo` once.
 *
 * @param {string} message what was done, already localised
 * @param {() => void} undo the step back
 */
export function showUndoSnackbar(message, undo, doc = document) {
  const { bar, text, undo: button } = elements(doc);
  if (!bar || !text || !button) return;
  if (!button.dataset.wired) {
    button.dataset.wired = "true";
    button.addEventListener("click", () => {
      const run = onUndo;
      hideUndoSnackbar(doc);
      run?.();
    });
    bar.addEventListener("keydown", (event) => {
      if (event.key === "Escape") hideUndoSnackbar(doc);
    });
  }
  clearTimeout(timer);
  onUndo = undo;
  text.textContent = message;
  button.textContent = t("undoSnackbar.undo");
  bar.hidden = false;
  timer = setTimeout(() => hideUndoSnackbar(doc), UNDO_SNACKBAR_MS);
}
