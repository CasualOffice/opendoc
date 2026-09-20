// The Pages navigator: a column of real page thumbnails, and a window on them.
//
// Extracted from `main.js` (`109` HF-085) as part of the page-band work,
// because the same fact that forced the band forced this: the panel rendered
// ONE THUMBNAIL PER PAGE of the document. At 14 pages that is a navigator; at
// the 13,726 pages the ceiling already allowed it is 13,726 `renderPage` calls
// on the main thread, each of which moves a windowed document's page window —
// a tab that stops responding, from a button that looks like a panel toggle.
// So the panel shows a WINDOW of pages around the one being read, says so, and
// follows the reader as they scroll.
//
// The window is not a limitation of the navigator, it is the navigator: Word's
// own thumbnail pane renders lazily for the same reason.

/** How many thumbnail cards may exist at once.
 *
 *  Sized so an ordinary document is still shown whole — `sample.docx` is 14
 *  pages, a long report is tens — while a document of any length costs the same
 *  to navigate. A page's thumbnail is a `renderPage` at 24 dpi, so this is also
 *  the bound on how much rasterizing one panel build can ask for. */
export const PAGES_PANEL_WINDOW = 40;

/** A small live render per page, so each card shows the real page layout rather
 *  than a blank box: low enough to be a preview rather than readable text, and
 *  each transient bitmap is freed immediately. */
const THUMB_DPI = 24;

/** Which pages to show for a document of `total` pages centred on `current`.
 *
 *  Clamped rather than wrapped at both ends, so the first and last pages of a
 *  document are reachable from the panel instead of being half a window off the
 *  edge of it.
 *
 *  @returns {{start:number, end:number}} 1-based, inclusive.
 */
export function pagesPanelRange(total, current, size = PAGES_PANEL_WINDOW) {
  if (total <= size) return { start: 1, end: total };
  const half = Math.floor(size / 2);
  const start = Math.min(Math.max(1, current - half), total - size + 1);
  return { start, end: start + size - 1 };
}

/**
 * Rebuilds the thumbnail column.
 *
 * @param {object} options
 * @param {{renderPage:Function}} options.doc the open document.
 * @param {Array<{pageNumber:number,wTwip:number,hTwip:number}>} options.pages
 *        the page records — geometry only; a page needs no sheet to have a
 *        thumbnail, which is the point.
 * @param {number} options.current the page to centre the window on and mark.
 * @param {HTMLElement} options.body the panel's scrolling body.
 * @param {(pageNumber:number) => void} options.onJump what a card's click does.
 * @returns {{start:number, end:number}} the range actually shown.
 */
export function renderPagesPanel({ doc, pages, current, body, onJump }) {
  body.replaceChildren();
  if (!pages.length) {
    const empty = document.createElement("div");
    empty.className = "outline-empty";
    empty.textContent = "No pages yet.";
    body.appendChild(empty);
    return { start: 0, end: -1 };
  }
  const total = pages.length;
  const range = pagesPanelRange(total, Math.min(Math.max(1, current), total));
  if (range.start > 1 || range.end < total) {
    // Never silently show a slice of a document as if it were the document —
    // the same rule the accessibility mirror follows when it windows.
    const note = document.createElement("p");
    note.className = "panel-window-note";
    note.textContent =
      `Pages ${range.start.toLocaleString()}–${range.end.toLocaleString()} of ` +
      `${total.toLocaleString()}. Scroll the document to see other pages here.`;
    body.appendChild(note);
  }
  for (let n = range.start; n <= range.end; n++) {
    const page = pages[n - 1];
    const card = document.createElement("button");
    card.type = "button";
    card.className = "page-thumb";
    card.dataset.page = String(n);
    card.title = `Page ${n}`;
    card.setAttribute("aria-label", `Page ${n}`);
    const box = document.createElement("span");
    box.className = "page-thumb-box";
    box.style.aspectRatio = `${page.wTwip} / ${page.hTwip}`;
    try {
      const bmp = doc.renderPage(n - 1, THUMB_DPI);
      const canvas = document.createElement("canvas");
      canvas.className = "page-thumb-canvas";
      canvas.width = bmp.widthPx;
      canvas.height = bmp.heightPx;
      canvas
        .getContext("2d")
        .putImageData(new ImageData(bmp.rgba, bmp.widthPx, bmp.heightPx), 0, 0);
      bmp.free(); // return the RGBA buffer to WASM now, not at GC.
      box.appendChild(canvas);
    } catch (err) {
      // A page that fails to render still shows a (correctly proportioned) card.
      console.error(`thumbnail page ${n - 1}`, err);
      box.classList.add("is-empty");
    }
    const num = document.createElement("span");
    num.className = "page-thumb-num";
    num.textContent = String(n);
    card.append(box, num);
    card.addEventListener("click", () => onJump(n));
    body.appendChild(card);
  }
  return range;
}

/** Keeps the navigator's active card synchronized with the caret's page. */
export function reflectPagesPanelSelection(body, pageNumber) {
  for (const card of body.querySelectorAll(".page-thumb")) {
    const active = Number(card.dataset.page) === pageNumber;
    card.classList.toggle("is-active", active);
    if (active) card.setAttribute("aria-current", "page");
    else card.removeAttribute("aria-current");
  }
}
