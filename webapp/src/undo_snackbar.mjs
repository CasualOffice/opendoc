// SPDX-License-Identifier: Apache-2.0
// The undo snackbar (PROPOSAL D-11; audit REVIEW-12): an action that changes a
// lot at once — Accept all, Reject all, Resolve — says what it did ONCE, with
// Undo beside it, for a few seconds. Google Docs' snackbar is the model; Word
// relies on Ctrl+Z alone, which leaves a person who did not see what changed
// with nothing to act on.
//
// One element (`#undoSnackbar` in `editor.html`), one message at a time: a new
// action replaces the previous offer.
//
// AN OFFER NEVER UNDOES SOMETHING ELSE. Two kinds of Undo are offered:
//
// * BOUND (`survivesEdits: true`): the step back names its subject — Resolve's
//   is `setCommentResolved(id, false)` — so it is right whatever happened since.
// * BLIND (the default): the step back is the editor's Undo, which undoes the
//   LAST step on the stack. Accept all's offer is one of these. Once anything
//   else lands — the person types, a co-author's change arrives, another
//   document opens — the last step is no longer the decision, so the offer is
//   WITHDRAWN at that edit (`withdrawUndoSnackbarOnEdit`, called from the one
//   choke point every edit passes, `noteDocumentEdited`), and its button also
//   refuses if the document's version moved under it. Google Docs withdraws its
//   snackbar on the next edit for the same reason.
//
// FOCUS. When the offer goes away while its button holds focus (Undo pressed,
// the timeout, Escape), focus goes back to the document through `focusBack`,
// never to `<body>` (audit REVIEW-06, PROPOSAL D-10).
//
// The live region is the element's own `role="status"`, so the sentence is also
// heard.
//
// Complexity: O(1).

/** How long the offer stays, in ms: long enough to read a short sentence and
 *  reach the button (Material's 4-10s band), short enough not to linger. */
export const UNDO_SNACKBAR_MS = 6000;

let timer = 0;
/** The offer on screen: `{ undo, survivesEdits, version, shownAt, focusBack }`. */
let offer = null;

function elements(doc) {
  return {
    bar: doc.getElementById("undoSnackbar"),
    text: doc.getElementById("undoSnackbarText"),
    undo: doc.getElementById("undoSnackbarUndo"),
  };
}

/** Hides the offer. Focus inside it goes back to the document. */
export function hideUndoSnackbar(doc = document) {
  clearTimeout(timer);
  timer = 0;
  const focusBack = offer?.focusBack;
  offer = null;
  const { bar } = elements(doc);
  if (!bar) return;
  const heldFocus = bar.contains(doc.activeElement);
  bar.hidden = true;
  if (heldFocus) focusBack?.();
}

/** Whether a BLIND offer's moment has passed: the document's version is no
 *  longer the one it was shown at (or cannot be read). Pure over `offer`. */
function stale(current) {
  if (!current || current.survivesEdits || !current.version) return false;
  const now = current.version();
  return now === null || now === undefined || now !== current.shownAt;
}

/** Called by the edit choke point after every landed edit: withdraws a blind
 *  offer the edit has made wrong. `unreadable` = the edit's revision could not
 *  be read, which is treated as a change; `replaced` = another document was
 *  opened, which withdraws a bound offer too. O(1). */
export function withdrawUndoSnackbarOnEdit({ unreadable = false, replaced = false } = {}, doc = globalThis.document) {
  if (!offer) return;
  if (replaced) return hideUndoSnackbar(doc);
  if (offer.survivesEdits) return;
  if (unreadable || stale(offer)) hideUndoSnackbar(doc);
}

/** Whether an offer is on screen. For tests and the edit path. */
export function undoSnackbarShown(doc = document) {
  const { bar } = elements(doc);
  return !!bar && !bar.hidden && !!offer;
}

/**
 * Shows `message` with an Undo button that runs `undo` once.
 *
 * @param {string} message what was done, already localised
 * @param {() => void} undo the step back
 * @param {{ survivesEdits?: boolean, version?: () => unknown, focusBack?: () => void }} [options]
 *   `survivesEdits`: `undo` names its subject and stays right after other edits.
 *   Otherwise `version` (the document's version token) must be given; the offer
 *   is withdrawn once it changes. `focusBack` puts focus in the document when the
 *   offer goes away holding it.
 */
export function showUndoSnackbar(message, undo, options = {}, doc = document) {
  const { bar, text, undo: button } = elements(doc);
  if (!bar || !text || !button) return;
  if (!button.dataset.wired) {
    button.dataset.wired = "true";
    button.addEventListener("click", () => {
      const current = offer;
      const fresh = current && !stale(current);
      hideUndoSnackbar(doc);
      if (fresh) current.undo?.();
    });
    bar.addEventListener("keydown", (event) => {
      if (event.key === "Escape") hideUndoSnackbar(doc);
    });
  }
  clearTimeout(timer);
  const { survivesEdits = false, version = null, focusBack = null } = options;
  offer = { undo, survivesEdits, version, shownAt: version ? version() : undefined, focusBack };
  text.textContent = message;
  bar.hidden = false;
  timer = setTimeout(() => hideUndoSnackbar(doc), UNDO_SNACKBAR_MS);
}
