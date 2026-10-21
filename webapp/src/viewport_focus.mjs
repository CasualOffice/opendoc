// A press on the editor's scroll surface that misses every page keeps the
// editor's focus, and below the last page it puts the caret at the end.
//
// The page sheets sit inside `#pages`, which sits inside the scrolling
// `#viewport`. A press on a sheet is the page's own business (`onPointerDown`).
// A press on the grey surface around the sheets landed on `#pages` (focusable,
// so focus at least stayed in the editor) or, below the last page, on
// `#viewport`, which is NOT focusable: the browser moved focus to <body>, the
// next typing went nowhere, nothing said so, and Cmd+Z stopped working because
// the keystrokes never reached the editor (`docs/109` HF-301, `docs/174` OL-01).
//
// Word, Google Docs and ONLYOFFICE put the caret at the end of the document for
// a click below the last page, and none of them lets a click on the canvas take
// focus away from the text. So:
//
// - the press's default (moving focus) is prevented on the surface itself, and
//   the editor is focused instead — one rule for every part of the surface that
//   is not a sheet, so a new gap cannot reopen the hole;
// - below the last sheet, the caret goes to the document's end (Shift extends
//   the selection there, as a Shift+click does on a page).
//
// A press between two pages, or beside one, deliberately does NOT move the
// caret: the nearest-page resolution there would jump into whatever fragment
// happened to be closest (the reason `#pages`' own handler only acts on a
// sheet). It keeps focus, so typing continues where the caret already is.

/**
 * Installs the guard on the editor's scroll surface.
 *
 * @param {object} options
 * @param {HTMLElement} options.viewport the scrolling container
 * @param {HTMLElement} options.pages the container holding the page sheets
 * @param {() => (Element | null | undefined)} options.lastSheet the last page's
 *   sheet element when it is materialised, else nothing
 * @param {() => void} options.focus focuses the editing surface
 * @param {(extend: boolean) => void} options.caretToEnd moves (or with `extend`,
 *   extends) the selection to the document's end
 * @returns {(event: MouseEvent) => boolean} the handler, returned so a test can
 *   drive it; true when it handled the press
 */
export function installViewportFocusGuard({ viewport, pages, lastSheet, focus, caretToEnd }) {
  const handle = (event) => {
    if (event.button !== 0) return false;
    if (event.target !== viewport && event.target !== pages) return false;
    event.preventDefault();
    const sheet = lastSheet();
    if (sheet && event.clientY > sheet.getBoundingClientRect().bottom) caretToEnd(event.shiftKey);
    focus();
    return true;
  };
  viewport.addEventListener("mousedown", handle);
  return handle;
}
