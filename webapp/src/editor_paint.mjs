// Page virtualization and painting: the page window, which canvases exist,
// painting the pages in view and releasing the rest. renderAll stays in
// main.js, which hands it out at boot. Moved out of `main.js` unchanged
// (docs/177); it reaches the rest of the editor only through the context `ed`.
import { PAGE_WINDOW_OVERSCAN_PX, pageClientRect, pageRangeAt, scrollToDoc } from "./page_scroll.mjs";
import { pageBandLeft } from "./page_scroll.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createPaint(ed) {
  // ---- Page virtualization -----------------------------------------------------
  // The pages live inside ONE positioned element, `.page-band`, whose height is
  // the scroll container's height. A sheet element (`.page-wrap`) exists only for
  // the pages in or near the viewport; every other page is arithmetic in
  // `page_scroll.mjs` and nothing in the DOM at all. A live raster `<canvas>` is
  // mounted inside a sheet as it comes on screen and released as it leaves, so
  // tab memory AND node count are bounded by the viewport, not the page count.
  //
  // Why a band rather than one sheet per page in flow, and what the mapping from
  // scroll space to document space costs: `page_scroll.mjs`, `docs/113` §8.6.

  /** Keep a live canvas for pages within ~one viewport-height of the visible band
   *  above and below, so a normal scroll never reveals an unpainted page. */
  const PAGE_VIRTUALIZATION_ROOT_MARGIN = "100% 0px";
  let pageObserver = null;


  function ensurePageObserver() {
    if (pageObserver) return pageObserver;
    pageObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          const idx = pageIndexOfWrap(entry.target);
          if (idx < 0) continue;
          const page = ed.pages[idx];
          page.visible = entry.isIntersecting;
          if (entry.isIntersecting) paintPageCanvas(page, idx);
          else releasePageCanvas(page);
        }
      },
      { root: ed.viewportEl, rootMargin: PAGE_VIRTUALIZATION_ROOT_MARGIN },
    );
    return pageObserver;
  }

  /** Resolve an observed wrap back to its (still-current) page index, or -1. */
  function pageIndexOfWrap(wrap) {
    const idx = wrap.__pageIndex;
    return Number.isInteger(idx) && ed.pages[idx]?.wrap === wrap ? idx : -1;
  }

  /** The pages that currently have a sheet, in page order. Every loop that walks
   *  `pages` looking for DOM goes through this: bounded by the window, not by a
   *  page count that can be 25,556. */
  function materializedPages() {
    const out = [];
    for (let i = ed.pageWindow.first; i <= ed.pageWindow.last; i++) {
      if (ed.pages[i]?.wrap) out.push(ed.pages[i]);
    }
    return out;
  }

  /** How far outside the viewport a page stays materialized: at least a viewport
   *  height (so an ordinary scroll never reveals a missing sheet) and at least
   *  `PAGE_WINDOW_OVERSCAN_PX`, which covers the comment column's mounting band
   *  so a card never anchors to a page that does not exist. */
  function pageWindowOverscan() {
    return Math.max(ed.viewportEl.clientHeight, PAGE_WINDOW_OVERSCAN_PX);
  }

  /** The band-local y a page's sheet is positioned at, in CSS px. */
  function pageBandTop(index) {
    return ed.bandOffset + ed.pageBandModel.tops[index];
  }

  /** The client rect a page's sheet has, or WOULD have if it were materialized.
   *
   *  Every twip→pixel conversion goes through `scaleOf`, and the pages it is
   *  asked about are no longer all in the DOM: a comment eight pages down, a
   *  find match on page 20,000, the caret after a jump. The band's rect plus the
   *  page's position answers those exactly — and identically to
   *  `getBoundingClientRect()`, because that is where the sheet was put. */
  function virtualPageRect(index) {
    return pageClientRect(ed.bandEl.getBoundingClientRect(), ed.pageBandModel, index, ed.bandOffset);
  }

  /** Build the sheet (and overlay) for one page, in page order in the band. */
  function materializePage(index) {
    const page = ed.pages[index];
    if (!page || page.wrap) return;
    const wrap = document.createElement("div");
    wrap.className = "page-wrap";
    wrap.dataset.pageNumber = String(index + 1);
    wrap.style.width = `${ed.pageBandModel.widths[index]}px`;
    wrap.style.height = `${ed.pageBandModel.heights[index]}px`;
    wrap.style.left = `${pageBandLeft(ed.pageBandModel, index)}px`; // a grid column in Multiple pages
    wrap.style.top = `${pageBandTop(index)}px`;
    wrap.__pageIndex = index;

    // A transparent overlay above the canvas holds the caret/selection we draw
    // ourselves from engine geometry — so the highlight matches the raster
    // exactly (doc 58: custom engine-driven selection, no overlay-vs-glyph drift).
    const overlay = document.createElement("div");
    overlay.className = "overlay";
    wrap.appendChild(overlay);

    // Keep the DOM in page order: hit-testing and the browser suite both read
    // document order off element order.
    let before = null;
    for (const el of ed.bandEl.children) {
      if (Number(el.dataset.pageNumber) > index + 1) {
        before = el;
        break;
      }
    }
    ed.bandEl.insertBefore(wrap, before);
    page.wrap = wrap;
    page.overlay = overlay;
    ensurePageObserver().observe(wrap);
  }

  /** Drop one page's sheet, canvas and overlay. The page RECORD survives. */
  function dematerializePage(page) {
    if (!page.wrap) return;
    pageObserver?.unobserve(page.wrap);
    page.wrap.remove();
    page.wrap = null;
    page.overlay = null;
    page.canvas = null;
    page.visible = false;
  }

  /** Re-place the window of sheets for the current scroll position.
   *
   *  Called from the scroll handler, so it must be cheap: a binary search, a few
   *  node insertions and one style write per sheet. It repaints the overlay layer
   *  only when the window moved — the sheets that just appeared have empty
   *  overlays, and the markers on them have to be drawn again. */
  function updatePageWindow({ force = false } = {}) {
    if (!ed.doc || !ed.pageBandModel || !ed.bandEl) return;
    const viewportHeight = ed.viewportEl.clientHeight;
    const { docY, offset } = scrollToDoc(
      ed.pageBandModel,
      viewportHeight,
      ed.viewportEl.scrollTop - ed.bandTopInScroller,
    );
    const overscan = pageWindowOverscan();
    const range = pageRangeAt(ed.pageBandModel, docY - overscan, docY + viewportHeight + overscan);
    const moved = range.first !== ed.pageWindow.first || range.last !== ed.pageWindow.last;
    if (!force && !moved && offset === ed.bandOffset) return;

    ed.bandOffset = offset;
    const previous = ed.pageWindow;
    ed.pageWindow = range;
    // Only a page that HAD a sheet can lose one: bounded by the window's size.
    for (let i = previous.first; i <= previous.last; i++) {
      if ((i < range.first || i > range.last) && ed.pages[i]) dematerializePage(ed.pages[i]);
    }
    for (let i = range.first; i <= range.last; i++) {
      materializePage(i);
      // The offset moves on every scroll once the document is compressed, so the
      // sheets that stayed have to move with it.
      ed.pages[i].wrap.style.top = `${pageBandTop(i)}px`;
    }
    if (moved || force) {
      ed.spellChecker.noteWindowChanged();
      ed.paintOverlayLayer();
      if (ed.runningEditBand) ed.drawRunningBands(ed.runningEditBand);
      ed.pagesPanelView.sync();
    }
  }

  /** The scroll owner for page materialization. Synchronous on purpose: a sheet
   *  one frame late is a blank page under the reader's eyes, and every geometry
   *  answer (`scaleOf`) before that frame would use a stale offset. */
  ed.viewportEl.addEventListener("scroll", () => updatePageWindow(), { passive: true });

  /** Mount and paint a page's raster canvas if it has none. The RGBA buffer is
   *  freed back to WASM immediately after the blit so it never accumulates. */
  function paintPageCanvas(page, index) {
    if (!ed.doc || page.canvas) return;
    let bmp;
    try {
      bmp = ed.doc.renderPage(index, ed.currentDpi());
    } catch (err) {
      console.error(`render page ${index}`, err);
      return;
    }
    const canvas = document.createElement("canvas");
    canvas.className = "page";
    canvas.width = bmp.widthPx;
    canvas.height = bmp.heightPx;
    // The surface is fully opaque, so tiny-skia's premultiplied RGBA equals the
    // straight-alpha RGBA `ImageData` expects — a direct blit is correct.
    canvas.getContext("2d").putImageData(new ImageData(bmp.rgba, bmp.widthPx, bmp.heightPx), 0, 0);
    bmp.free(); // return the ~13 MB RGBA Vec to WASM deterministically, not at GC.
    // The canvas sits under the transparent caret/selection overlay.
    page.wrap.insertBefore(canvas, page.overlay);
    page.canvas = canvas;
  }

  /** Drop a page's raster canvas (releases its GPU/CPU pixels). The sheet-sized
   *  wrap remains, so layout and scroll height are unchanged. */
  function releasePageCanvas(page) {
    if (!page.canvas) return;
    page.canvas.remove();
    page.canvas = null;
  }

  /** Synchronously paint the pages currently on screen (plus one screenful of
   *  margin), so the viewport is never briefly blank before the async observer
   *  fires. Matches the observer's rootMargin. */
  function paintPagesInView() {
    if (!ed.pages.length) return;
    const vr = ed.viewportEl.getBoundingClientRect();
    const margin = vr.height;
    for (let i = ed.pageWindow.first; i <= ed.pageWindow.last; i++) {
      const page = ed.pages[i];
      if (!page?.wrap) continue;
      const r = page.wrap.getBoundingClientRect();
      const onscreen = r.bottom >= vr.top - margin && r.top <= vr.bottom + margin;
      page.visible = onscreen;
      if (onscreen) paintPageCanvas(page, i);
    }
  }



  return {
    materializedPages, pageIndexOfWrap, paintPageCanvas, paintPagesInView, releasePageCanvas,
    updatePageWindow, virtualPageRect,
  };
}
