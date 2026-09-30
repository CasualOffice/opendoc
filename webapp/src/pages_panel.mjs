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

// ---- The panel's controller (`docs/148` §9 item 3) --------------------------
//
// The RENDERING moved out of `main.js` with the page-band work; the CONTROLLER
// — open/close, what to centre on, following the reader, and jumping — stayed
// behind as six closures and a piece of module state. That was tolerable while
// the only way to reach this panel was a rail tile. It stopped being tolerable
// when `view.pages` gave it a command: a command whose implementation is six
// closures in a 16,000-line module is a command nobody can find the behaviour
// of, and the file named `pages_panel.mjs` is where a reader will look.
//
// Everything is injected, so this file still names no `main.js` symbol and the
// seam is readable in one place. `docToScroll` is imported here rather than
// passed in, because it is the exact inverse of the page band's own scroll
// compression and belongs to the band, not to the shell.
//
// Complexity: `build` is O(PAGES_PANEL_WINDOW) thumbnails — a fixed 40, never
// the document — and `sync`, `reflect` and `jump` are O(cards on screen).
// Nothing here walks the document, which is what `docs/107` §4 requires of
// anything on a scroll path; `sync` in particular runs on every page-window
// move.
import { docToScroll } from "./page_scroll.mjs";

/**
 * Wires the Pages navigator to its panel.
 *
 * @param {object} deps
 * @param {HTMLElement} deps.panel       `#pagesPanel`, whose `hidden` is the open state
 * @param {HTMLElement} deps.body        `#pagesBody`, the scrolling thumbnail column
 * @param {HTMLElement} deps.railButton  the rail tile, whose `aria-pressed` follows
 * @param {HTMLElement} deps.closeButton the panel's own close control
 * @param {HTMLElement} deps.viewport    the single scroll owner
 * @param {() => object} deps.getDoc
 * @param {() => ({focus:{node:string,offset:number}}|null)} deps.getSelection
 * @param {() => Array} deps.getPages    the page records
 * @param {() => object} deps.getBandModel
 * @param {() => ({pageNumber:number}|null)} deps.pageInView
 * @param {() => number} deps.bandTop    the band's top in scroller coordinates
 * @param {() => void} deps.onExclusive  called when the panel opens, to close the others
 * @param {() => void} deps.onJumped     called once a jump has moved the scroller
 * @returns {{build:Function, sync:Function, reflect:Function, toggle:Function,
 *            isOpen:() => boolean}}
 */
export function createPagesPanel({
  panel,
  body,
  railButton,
  closeButton,
  viewport,
  getDoc,
  getSelection,
  getPages,
  getBandModel,
  pageInView,
  bandTop,
  onExclusive,
  onJumped,
  // "" when the navigator may be opened, the reason when it may not. Reflow is
  // the one caller today: its thumbnails would be TILES, and a navigator whose
  // cards read "7" about a rasterisation unit is a lie the reader cannot see
  // through (`docs/151` §6.4). A reason rather than a boolean because the rail
  // tile and the `view.pages` command both have to be able to SAY it — a
  // control that silently does nothing is the failure this repo keeps making.
  withheldReason = () => "",
  onWithheld = () => {},
}) {
  /** The range of pages currently carded. Its own, because the navigator
   *  windows separately from the page band. */
  let shown = { start: 0, end: -1 };

  /** The page the navigator MARKS as current: the caret's, falling back to the
   *  one being read. Deliberately not what the panel centres on — a reader who
   *  has scrolled 6,000 pages from their caret wants to see where they now are,
   *  not where they last typed. */
  function focusPage() {
    const selection = getSelection();
    if (selection) {
      const flat = getDoc().caretRect(selection.focus.node, selection.focus.offset);
      if (flat.length) return flat[0];
    }
    return pageInView()?.pageNumber ?? 1;
  }

  function reflect(pageNumber) {
    if (panel.hidden) return;
    reflectPagesPanelSelection(body, pageNumber);
  }

  /** Scrolls page `n` into view through the single scroll owner, then marks it. */
  function jump(n) {
    const band = getBandModel();
    if (!getPages()[n - 1] || !band) return;
    // From the BAND's geometry, not from the sheet's rect: the page being jumped
    // to is usually the one page in the document that has no sheet yet.
    const viewportHeight = viewport.clientHeight;
    const docY = Math.max(0, band.tops[n - 1] - 16);
    const target = bandTop() + docToScroll(band, viewportHeight, docY);
    const max = Math.max(0, viewport.scrollHeight - viewport.clientHeight);
    viewport.scrollTo({ top: Math.max(0, Math.min(max, target)), behavior: "auto" });
    onJumped();
    reflect(n);
  }

  /** Rebuilds the navigator around one page — by default the one being read. */
  function build(centre = null) {
    if (!getDoc()) return;
    // Every render passes through here, so this is where a mode change is
    // noticed: entering reflow closes an open navigator and disables its tile
    // rather than leaving a panel of page thumbnails standing over a document
    // that no longer has pages.
    if (reflectWithheld() || panel.hidden) return;
    const focus = focusPage();
    shown = renderPagesPanel({
      doc: getDoc(),
      pages: getPages(),
      current: centre ?? pageInView()?.pageNumber ?? focus,
      body,
      onJump: jump,
    });
    reflect(focus);
  }

  /** Follow the reader: when the viewport leaves the range the panel is
   *  showing, rebuild around where they now are. Scrolling INSIDE the shown
   *  range costs nothing, which is what keeps this off the scroll budget. */
  function sync() {
    if (panel.hidden || !getDoc()) return;
    const visible = pageInView()?.pageNumber ?? 1;
    if (visible < shown.start || visible > shown.end) build(visible);
  }

  /** Closes the panel and disables the rail tile when the navigator is withheld,
   *  so a mode change cannot leave a panel of tile thumbnails standing open.
   *  Returns the reason, or "". */
  // The title to put back when the navigator stops being withheld is read at
  // the moment it is needed, NOT captured here. Capturing it at construction
  // snapshots the ENGLISH literal that is in the markup before the localisation
  // sweep runs, and the first `reflectWithheld()` then writes that English back
  // over the translated title — which is exactly what
  // `localisation.spec.mjs`'s "no routed string shows its English at FIRST
  // PAINT" caught. The element carries `data-i18n-title`, so the string table
  // owns this value; asking the element for it keeps one owner instead of two.
  const railTitle = () => railButton.dataset.railTitle ?? railButton.title;
  function reflectWithheld() {
    const reason = withheldReason();
    // Remember the localised title the first time we are about to replace it,
    // so a second withholding does not save the REASON as the title.
    if (reason && railButton.dataset.railTitle === undefined) {
      railButton.dataset.railTitle = railButton.title;
    }
    railButton.disabled = !!reason;
    railButton.title = reason || railTitle();
    if (reason && !panel.hidden) {
      panel.hidden = true;
      railButton.setAttribute("aria-pressed", "false");
    }
    return reason;
  }

  function toggle() {
    const reason = reflectWithheld();
    if (reason) return onWithheld(reason);
    panel.hidden = !panel.hidden;
    // Pages, Outline (left) and the review sidebar (right) are mutually
    // exclusive, so the canvas is never squeezed from both sides at once.
    if (!panel.hidden) onExclusive();
    railButton.setAttribute("aria-pressed", String(!panel.hidden));
    build();
  }

  railButton.addEventListener("click", toggle);
  closeButton.addEventListener("click", toggle);

  return { build, sync, reflect, toggle, isOpen: () => !panel.hidden };
}
