// SPDX-License-Identifier: Apache-2.0
// Focus on open (`docs/109` UX-060, audit CANVAS-05).
//
// Word, Google Docs and ONLYOFFICE open a document with the caret blinking at
// the start of the body, ready for typing, and the toolbar reflecting the
// formatting there. Ours opened with focus on <body>: the caret was seeded but
// implicit (deliberately unpainted, see `implicitCaretAt` in main.js), so the
// first keystrokes went nowhere until the reader clicked, while the ribbon
// already showed the start-of-body formatting for a caret nobody could see.
//
// The editor now takes focus after the first paint — but only when doing so
// cannot take anything from anyone:
//
//   * not inside a frame: an embedded editor must not pull focus (and the
//     page's scroll position) away from the host page it sits in — the Home
//     page's live demo is exactly this case;
//   * not when something else already has focus — a preset that opened Find,
//     a dialog, or a reader who clicked into the chrome while the file loaded;
//   * not when the document cannot be typed into (read-only, Viewing, or a
//     container that offers no caret);
//   * not on a touch-first device, where focusing an editable element can
//     raise the soft keyboard over a document the reader has only opened to
//     read. Docs and Word on a phone wait for a tap too.
//
// Pure: the caller passes what it knows, so the rule is tested in Node.

/** Whether opening a document should focus the editing surface. */
export function shouldFocusOnOpen({ framed, activeElement, body, editable, coarsePointer }) {
  if (framed || !editable || coarsePointer) return false;
  return !activeElement || activeElement === body || activeElement === body?.parentElement;
}
