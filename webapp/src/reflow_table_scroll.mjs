// Wide tables scroll sideways in reflow — the shell half of `docs/151` §6.3d.
//
// Google Docs' pageless view, in Google's words: "images will adjust to your
// screen size, and you can create wide tables and view them by scrolling left
// and right" (support.google.com/docs/answer/11528737). The engine half keeps a
// top-level table at the width its document declares (`MeasureFit::Scroll`) and
// owns ONE horizontal offset per table that every geometry reader honours
// (`casual_doc_layout::reflow_scroll`). This module is the scroller.
//
// ---- THE PATTERN, and why the scroll is the compositor's ---------------------
//
// It is a scroll container: a viewport onto content wider than itself. The
// question is who moves the pixels. Re-rasterising the tile per scroll step is
// what a canvas editor does by default, and it was measured before choosing:
// `renderPage` costs 70-150 ms for one 11in tile here — six to twelve frames a
// step. So the established answer for a retained raster is taken instead: the
// table's band is rasterised ONCE at its full width (`renderReflowTableStrip`)
// into a canvas inside a native `overflow-x: auto` box laid exactly over the
// band, and the BROWSER scrolls it. A scroll step costs the compositor, plus one
// `setReflowTableScroll` — `O(rows of the table)`, no raster, no relayout — so
// that a click, the caret and a selection land where the pixels are. Only an
// edit, which repaints the tile anyway, re-rasterises the strip.
//
// ---- THE FIVE INTERACTION DECISIONS (`docs/151` §6.3c, A-1..A-5) -------------
//
//   A-1 the caret drags the scroller with it — `follow()` below;
//   A-2 a selection dragged past the edge auto-scrolls — NOT built; recorded as
//       an open row rather than half-built (`docs/109`);
//   A-3 the scrollbar is visible whenever the table overflows — an always-on
//       overlay bar, never a hover-only one;
//   A-4 Home returns the view to the left edge — a consequence of A-1: the
//       caret moved, so the scroller follows it;
//   A-5 the page does not scroll sideways over a table — it has no horizontal
//       scroll to give (`docs/151` §6.2), so a sideways gesture over the strip
//       is the strip's alone; a VERTICAL one chains to the page, because the
//       strip cannot scroll vertically.
//
// ---- COST, per `docs/107` §4 -------------------------------------------------
//
// A scroll event: one division, one engine call `O(rows of the table)`, and the
// other strips of the same table (the tiles in the DOM window). Mounting a tile's
// strips: one `reflowTableOverflows` (`O(fragments on the tile)`) and one strip
// raster per over-wide table on it. Nothing here walks the document.
//
// ---- WHAT IT DOES NOT OWN ----------------------------------------------------
//
// The overlay. Caret, selection and table chrome are `main.js`'s, drawn from
// engine geometry — which is exactly why the offset lives in the engine: they
// are right as soon as they are redrawn. The `redraw` dependency asks for that
// redraw after a scroll; a host that has not wired it gets a full render once
// the gesture settles instead (correct, and heavier), never a stale caret.

import { backingRatio } from "./page_scroll.mjs";
import { TWIPS_PER_INCH } from "./units.mjs";

/** How close to the strip's edge, in CSS px, the caret may sit before the
 *  scroller follows it (A-1). The reflow gutter (16px) plus room for the caret
 *  to be seen as inside the table rather than on its edge. */
export const CARET_FOLLOW_EDGE_PX = 24;

/** How long after the last scroll event a host WITHOUT a cheap overlay redraw
 *  gets its full render. Under the ~200ms at which a response stops feeling
 *  attached to the gesture, like the width feed's debounce. */
export const SCROLL_SETTLE_MS = 160;

/**
 * Where the scroller must be for a caret at `caretX` (CSS px from the strip's
 * left edge, as currently painted) to be inside `[edge, width - edge]`, or
 * `null` when it already is. Pure, so the follow rule is answerable in `node`.
 *
 * Complexity: O(1).
 *
 * @param {number} caretX the caret's painted x, CSS px, in the tile.
 * @param {number} scrollLeft the strip's current scroll offset, CSS px.
 * @param {number} width the strip's visible width, CSS px.
 * @param {number} [edge]
 * @returns {number|null} the new `scrollLeft`, or `null` to leave it.
 */
export function followScrollLeft(caretX, scrollLeft, width, edge = CARET_FOLLOW_EDGE_PX) {
  if (!(width > 2 * edge)) return null;
  if (caretX < edge) return Math.max(0, scrollLeft - (edge - caretX));
  if (caretX > width - edge) return scrollLeft + (caretX - (width - edge));
  return null;
}

/**
 * The overlay bar's thumb: its width and its left edge in the track, CSS px.
 * Pure for the same reason. Never thinner than a finger-findable 24px.
 *
 * @param {number} track the track's width.
 * @param {number} visible the strip's visible width.
 * @param {number} total the strip's scroll width.
 * @param {number} scrollLeft
 * @returns {{width:number, left:number}}
 */
export function thumbGeometry(track, visible, total, scrollLeft) {
  if (!(total > visible) || !(track > 0)) return { width: track, left: 0 };
  const width = Math.min(track, Math.max(24, (track * visible) / total));
  const range = total - visible;
  const left = range > 0 ? ((track - width) * Math.max(0, Math.min(range, scrollLeft))) / range : 0;
  return { width, left };
}

/**
 * Wires per-table horizontal scrolling over a reflowed page band.
 *
 * @param {{
 *   viewport: HTMLElement,
 *   getDoc: () => any,
 *   redraw?: (() => void)|null,
 *   settle?: () => void,
 *   view?: Window,
 * }} deps
 *
 * `redraw` is the host's overlay repaint (`main.js`'s `drawSelection`): cheap,
 * and what keeps the caret and the selection on the columns they belong to
 * while the table moves under them. `settle` is the fallback a host without it
 * gets once the gesture settles — a full render, which also redraws them.
 */
export function createReflowTableScroll({
  viewport,
  getDoc,
  redraw = null,
  settle = () => {},
  view = window,
}) {
  let on = false;
  let cssPerTwip = 0;
  let redrawFrame = 0;
  let settleTimer = null;
  /** The last caret position the scroller followed, in UNSCROLLED terms, so a
   *  caret that moved only because its table was scrolled is not chased back
   *  to where it was — that would make a table with the caret in it impossible
   *  to scroll away from. */
  let lastFollowed = "";
  const pending = new Set();
  let flushQueued = false;

  const doc = () => getDoc();
  const ratio = () => backingRatio(view);

  function stripsOf(wrap) {
    return [...wrap.querySelectorAll(":scope > .reflow-table-scroll")];
  }

  function allStrips() {
    return [...viewport.querySelectorAll(".page-wrap > .reflow-table-scroll")];
  }

  function updateBar(strip) {
    const port = strip.__port;
    const bar = strip.__bar;
    if (!port || !bar) return;
    const track = bar.clientWidth;
    const { width, left } = thumbGeometry(track, port.clientWidth, port.scrollWidth, port.scrollLeft);
    const thumb = bar.firstElementChild;
    thumb.style.width = `${width}px`;
    thumb.style.transform = `translateX(${left}px)`;
  }

  function scheduleRedraw() {
    if (redraw) {
      if (redrawFrame) return;
      redrawFrame = view.requestAnimationFrame(() => {
        redrawFrame = 0;
        redraw();
      });
      return;
    }
    if (settleTimer !== null) view.clearTimeout(settleTimer);
    settleTimer = view.setTimeout(() => {
      settleTimer = null;
      settle();
    }, SCROLL_SETTLE_MS);
  }

  /** A strip was scrolled — by a finger, a wheel, the bar, or `follow`. */
  function onScroll(strip) {
    const d = doc();
    if (!d || !on) return;
    const twip = Math.round(strip.__port.scrollLeft / strip.__sx);
    updateBar(strip);
    if (twip === strip.__offset) return;
    const applied = d.setReflowTableScroll(strip.__index, strip.dataset.table, twip);
    if (applied < 0) return;
    // Every strip of this table moves together: a table cut across two tiles
    // is one table, and the engine has already moved its rows on both.
    for (const other of allStrips()) {
      if (other.dataset.table !== strip.dataset.table) continue;
      other.__offset = applied;
      if (other !== strip) other.__port.scrollLeft = applied * other.__sx;
    }
    scheduleRedraw();
  }

  function buildStrip(row) {
    const strip = view.document.createElement("div");
    strip.className = "reflow-table-scroll";
    strip.dataset.table = row.table;
    // A visual duplicate of rows the accessibility mirror already carries as
    // text, so hidden from assistive technology — and therefore never in the
    // tab order (axe `aria-hidden-focus`): Chromium makes a scroller with no
    // focusable child keyboard-focusable on its own, so it is opted out.
    strip.setAttribute("aria-hidden", "true");
    const port = view.document.createElement("div");
    port.className = "reflow-table-scroll__port";
    port.tabIndex = -1;
    const canvas = view.document.createElement("canvas");
    canvas.className = "reflow-table-strip";
    port.append(canvas);
    const bar = view.document.createElement("div");
    bar.className = "reflow-table-scroll__bar";
    const thumb = view.document.createElement("div");
    thumb.className = "reflow-table-scroll__thumb";
    bar.append(thumb);
    strip.append(port, bar);
    strip.__port = port;
    strip.__bar = bar;
    port.addEventListener("scroll", () => onScroll(strip), { passive: true });
    wireBar(strip);
    return strip;
  }

  /** The overlay bar: drag the thumb, or press the track to page. The press is
   *  the bar's alone — the editor's `pointerdown` on `#pages` must not also
   *  move the caret to wherever the bar happens to sit over the table. */
  function wireBar(strip) {
    const bar = strip.__bar;
    const thumb = bar.firstElementChild;
    bar.addEventListener("pointerdown", (event) => {
      event.stopPropagation();
      event.preventDefault();
      const port = strip.__port;
      if (event.target === thumb) {
        const startX = event.clientX;
        const startScroll = port.scrollLeft;
        const track = bar.clientWidth;
        const range = port.scrollWidth - port.clientWidth;
        const thumbWidth = thumb.getBoundingClientRect().width;
        const perPx = track > thumbWidth ? range / (track - thumbWidth) : 0;
        thumb.setPointerCapture?.(event.pointerId);
        const move = (e) => {
          port.scrollLeft = startScroll + (e.clientX - startX) * perPx;
        };
        const up = () => {
          thumb.removeEventListener("pointermove", move);
          thumb.removeEventListener("pointerup", up);
          thumb.removeEventListener("pointercancel", up);
        };
        thumb.addEventListener("pointermove", move);
        thumb.addEventListener("pointerup", up);
        thumb.addEventListener("pointercancel", up);
        return;
      }
      const thumbRect = thumb.getBoundingClientRect();
      const page = port.clientWidth * 0.9;
      port.scrollLeft += event.clientX < thumbRect.left ? -page : page;
    });
  }

  /** Paints a strip's raster: the table's band at its full width, offset zero. */
  function paintStrip(strip, index, row, dpi) {
    const d = doc();
    let bmp;
    try {
      bmp = d.renderReflowTableStrip(index, row.table, dpi);
    } catch {
      return false;
    }
    const canvas = strip.__port.firstElementChild;
    const r = ratio();
    // Read everything off the wasm-owned bitmap BEFORE it is freed: a getter on
    // a freed handle throws "null pointer passed to rust".
    const width = bmp.widthPx;
    const height = bmp.heightPx;
    canvas.width = width;
    canvas.height = height;
    canvas.style.width = `${width / r}px`;
    canvas.style.height = `${height / r}px`;
    canvas.getContext("2d").putImageData(new ImageData(bmp.rgba, width, height), 0, 0);
    bmp.free();
    strip.style.height = `${height / r}px`;
    return true;
  }

  /** Puts a strip over every over-wide table on one tile, and takes away the
   *  strips of tables that no longer overflow there. Called whenever the tile
   *  is (re)painted, which is the one moment its tables can have changed.
   *
   *  Complexity: O(fragments on the tile) for the engine's list, plus one
   *  table-band raster per over-wide table on it. */
  function mount(wrap) {
    const d = doc();
    if (!d || !wrap.isConnected) return;
    if (!on || !(cssPerTwip > 0)) {
      for (const strip of stripsOf(wrap)) strip.remove();
      return;
    }
    const index = Number(wrap.dataset.pageNumber) - 1;
    let rows = [];
    try {
      rows = JSON.parse(d.reflowTableOverflows(index));
    } catch {
      rows = [];
    }
    const keep = new Set(rows.map((row) => row.table));
    for (const strip of stripsOf(wrap)) if (!keep.has(strip.dataset.table)) strip.remove();
    if (!rows.length) return;
    const dpi = cssPerTwip * TWIPS_PER_INCH * ratio();
    for (const row of rows) {
      let strip = stripsOf(wrap).find((s) => s.dataset.table === row.table);
      if (!strip) {
        strip = buildStrip(row);
        // Before the overlay, so the caret and the selection — drawn in the
        // overlay — stay above the strip: positioned siblings paint in DOM order.
        wrap.insertBefore(strip, wrap.querySelector(":scope > .overlay"));
      }
      // The tile's raster is drawn 1:1 from its box's top-left (`page_scroll`'s
      // SEAMLESS TILES), so a twip on it is exactly `cssPerTwip` CSS px — the
      // strip lines up with the raster it covers to the pixel.
      const sx = cssPerTwip;
      strip.__index = index;
      strip.__sx = sx;
      strip.__offset = row.offsetTwip;
      strip.style.top = `${row.topTwip * sx}px`;
      if (!paintStrip(strip, index, row, dpi)) {
        strip.remove();
        continue;
      }
      strip.__port.scrollLeft = row.offsetTwip * sx;
      updateBar(strip);
    }
  }

  /** A1: the caret drags the scroller with it. Reads the caret the overlay
   *  drew — the engine's own position, at the current offset — and moves the
   *  strip only if the caret MOVED (in unscrolled terms) and is out of view. */
  function follow(caret) {
    const wrap = caret.parentElement?.parentElement;
    if (!on || !wrap?.classList.contains("page-wrap")) return;
    const top = parseFloat(caret.style.top) || 0;
    const x = parseFloat(caret.style.left) || 0;
    for (const strip of stripsOf(wrap)) {
      const stripTop = parseFloat(strip.style.top) || 0;
      if (top < stripTop || top >= stripTop + strip.offsetHeight) continue;
      const port = strip.__port;
      const key = `${wrap.dataset.pageNumber}:${top}:${Math.round(x + port.scrollLeft)}`;
      if (key === lastFollowed) return;
      lastFollowed = key;
      const next = followScrollLeft(x, port.scrollLeft, port.clientWidth);
      if (next !== null) port.scrollLeft = next;
      return;
    }
  }

  function flush() {
    flushQueued = false;
    const wraps = [...pending];
    pending.clear();
    for (const wrap of wraps) mount(wrap);
  }

  function queue(wrap) {
    if (!wrap) return;
    pending.add(wrap);
    if (flushQueued) return;
    flushQueued = true;
    queueMicrotask(flush);
  }

  // One observer for the whole band. A tile's canvas is (re)inserted whenever it
  // is painted — a render, an edit's dirty repaint, scrolling it back into view
  // — and that is exactly when its tables may have changed, so that is when its
  // strips are rebuilt. Overlay churn (the caret blink, a selection) is skipped
  // after one class check per record.
  const observer = new view.MutationObserver((records) => {
    if (!on) return;
    for (const record of records) {
      const target = record.target;
      if (record.type === "attributes") {
        // The hover router (`pointer_hover.mjs`) writes its answer — a link's
        // pointer, a column boundary's resize arrow — onto the TILE's canvas,
        // which the strip covers. Carried across, so the table band shows the
        // same cursor the rest of the tile would.
        if (target.matches?.("canvas.page")) {
          for (const strip of stripsOf(target.parentElement)) {
            strip.style.cursor = target.style.cursor;
          }
        }
        continue;
      }
      if (target.classList?.contains("overlay")) {
        for (const node of record.addedNodes) {
          if (node.classList?.contains("caret")) follow(node);
        }
        continue;
      }
      for (const node of record.addedNodes) {
        if (node.nodeType !== 1) continue;
        if (node.matches("canvas.page")) queue(node.parentElement);
        else if (node.classList.contains("page-wrap")) queue(node);
      }
    }
  });
  observer.observe(viewport, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["style"],
  });

  return {
    /**
     * Called from the reflow render pass with the view it settled on. Turning
     * reflow off removes every strip; turning it on (or changing the zoom)
     * re-mounts the tiles in the DOM window.
     *
     * Complexity: O(tiles in the DOM window).
     */
    sync({ on: nextOn, cssPerTwip: nextScale }) {
      const changed = nextOn !== on || nextScale !== cssPerTwip;
      on = !!nextOn;
      cssPerTwip = nextScale > 0 ? nextScale : cssPerTwip;
      if (!changed) return;
      if (!on) {
        for (const strip of allStrips()) strip.remove();
        lastFollowed = "";
        return;
      }
      for (const wrap of viewport.querySelectorAll(".page-band > .page-wrap")) queue(wrap);
    },
    /** For a test or a host: the strips currently in the DOM. */
    strips: allStrips,
  };
}
