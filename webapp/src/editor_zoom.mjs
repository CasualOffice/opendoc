// Zoom: the editable percentage, Fit width / Fit page, and Ctrl+scroll.
// setZoom, setZoomMode and stepZoom stay in main.js, which hands them to the
// view-zoom control at boot. Moved out of `main.js` unchanged (docs/177); it
// reaches the rest of the editor only through the context `ed`.
import { closePopover, registerPopover } from "./popover_manager.mjs";
import { fitPageIndex, fitZoomFactor, parseZoomInput, reflectZoomMenu } from "./view_zoom.mjs";
import { TWIPS_PER_INCH } from "./units.mjs";
import { anchorPoint, anchorScrollDelta, captureAnchor } from "./zoom_anchor.mjs";
import { createPageInView } from "./page_in_view.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createZoom(ed) {
  // ---- Zoom (Q4): editable %, Fit width / Fit page, Ctrl+scroll ---------------
  const zoomMenu = document.getElementById("zoomMenu");
  const zoomMenuBtn = document.getElementById("zoomMenuBtn");
  const clampZoom = (z) => Math.min(ed.ZOOM_MAX, Math.max(ed.ZOOM_MIN, z));

  /** Fit-to-viewport factor for the CARET's page, not page 0. Fitting page 0 on a
   *  document whose later section turns landscape sized the view to the portrait
   *  page and let every landscape page overflow sideways — a "Fit width" that does
   *  not fit. The page index comes from the ruler, so the two cannot disagree
   *  about which page the user is on. The arithmetic is `view_zoom.mjs`. */
  function computeFitZoom(mode) {
    if (!ed.doc) return ed.zoomFactor;
    const size = ed.doc.pageSize(fitPageIndex(ed.rulerView.builtForPage(), ed.doc.pageCount)); // the ENGINE bounds it, not the view (HF-400)
    const factor = fitZoomFactor(mode, size, ed.viewportEl.getBoundingClientRect(), { dpi: ed.BASE_DPI, twipsPerInch: TWIPS_PER_INCH });
    size.free();
    return clampZoom(factor);
  }

  // ---- Zoom anchoring (audit CANVAS-08, CANVAS-V-02; `zoom_anchor.mjs`) -----
  // `renderAll` calls `updateZoomDisplay` once the new factor is decided and
  // BEFORE it lays the band out again, and the relayout after it is synchronous.
  // So this is the one place that sees every zoom change, whatever asked for it
  // (the status bar, the View band, the palette, a chord, a fit recompute):
  // capture the anchor here against the OLD geometry, and restore it in a
  // microtask, which runs once the relayout has published the new pages.
  let laidOutAt = null;
  // The document that layout was for: a zoom that comes with a newly opened
  // document has no point to keep.
  let laidOutDoc = null;
  let wheelPointer = null;
  let restorePending = false;

  function captureZoomAnchor() {
    const pages = ed.pages;
    if (!ed.doc || !pages?.length) return null;
    const view = ed.viewportEl.getBoundingClientRect();
    let caret = null;
    const focus = ed.selection?.focus;
    if (focus) {
      const flat = ed.doc.caretRect(focus.node, focus.offset);
      const page = flat.length >= 5 ? pages[flat[0] - 1] : null;
      if (page) {
        const { rect, sx, sy } = ed.scaleOf(page);
        caret = { x: rect.left + flat[1] * sx, y: rect.top + (flat[2] + flat[4] / 2) * sy, page };
      }
    }
    const at = anchorPoint(wheelPointer, caret, view);
    wheelPointer = null;
    const page = at.source === "caret" ? caret.page : ed.pageFromClientPoint(at.x, at.y);
    if (!page) return null;
    const { rect, sx, sy } = ed.scaleOf(page);
    return captureAnchor(pages.indexOf(page), rect, sx, sy, at);
  }

  function restoreZoomAnchor(anchor) {
    const page = ed.pages?.[anchor.pageIndex];
    if (!page) return;
    const { rect, sx, sy } = ed.scaleOf(page);
    const { dx, dy } = anchorScrollDelta(anchor, rect, sx, sy);
    ed.viewportEl.scrollLeft += dx;
    ed.viewportEl.scrollTop += dy;
  }

  function noteZoomForAnchor() {
    const factor = ed.zoomFactor;
    if (laidOutAt !== null && laidOutDoc === ed.doc && Math.abs(factor - laidOutAt) > 1e-6 && !restorePending) {
      const anchor = captureZoomAnchor();
      if (anchor) {
        restorePending = true;
        queueMicrotask(() => {
          restorePending = false;
          restoreZoomAnchor(anchor);
        });
      }
    }
    if (ed.pages?.length) {
      laidOutAt = factor;
      laidOutDoc = ed.doc;
    }
    wheelPointer = null;
  }

  /** Repaints the zoom input (unless the user is mid-edit) and the preset checks. */
  function updateZoomDisplay() {
    noteZoomForAnchor();
    if (document.activeElement !== ed.zoomEl) {
      ed.zoomEl.value =
        ed.zoomMode === "fit-width" ? "Fit width"
          : ed.zoomMode === "fit-page" ? "Fit page"
            : `${Math.round(ed.zoomFactor * 100)}%`;
    }
    reflectZoomMenu(zoomMenu, { mode: ed.zoomMode, factor: ed.zoomFactor });
    ed.viewZoom.reflect();
  }


  /** Commit the typed zoom value. The parse is `view_zoom.mjs`'s. */
  function commitZoomInput({ restoreFocus = false } = {}) {
    const asked = parseZoomInput(ed.zoomEl.value);
    if (asked.mode) ed.setZoomMode(asked.mode, restoreFocus);
    else if (asked.factor) ed.setZoom(asked.factor, restoreFocus);
    else updateZoomDisplay(); // reject: restore the last valid display
  }
  ed.zoomEl.addEventListener("change", () => commitZoomInput());
  ed.zoomEl.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      commitZoomInput({ restoreFocus: true });
      ed.zoomEl.blur();
    } else if (e.key === "Escape") {
      updateZoomDisplay();
      ed.zoomEl.blur();
      ed.focusEditorSurface();
    }
  });
  ed.zoomEl.addEventListener("focus", () => ed.zoomEl.select());

  const zoomPopover = registerPopover(zoomMenuBtn, zoomMenu, updateZoomDisplay);
  zoomMenu.addEventListener("click", (e) => {
    const preset = e.target.closest(".zoom-preset");
    const fit = e.target.closest(".zoom-fit");
    if (preset) ed.setZoom(Number(preset.dataset.zoom));
    else if (fit) ed.setZoomMode(fit.dataset.zoomMode);
    else return;
    closePopover(zoomPopover);
  });
  for (const [id, dir] of [["zoomIn", 1], ["zoomOut", -1]]) document.getElementById(id).addEventListener("click", () => ed.stepZoom(dir));

  // Ctrl/⌘+scroll over the document zooms (a fixed % centered on the pointer's
  // intent), the desktop-editor convention. Passive:false so we can preventDefault
  // the page zoom the browser would otherwise do.
  ed.viewportEl.addEventListener(
    "wheel",
    (e) => {
      if (!(e.ctrlKey || e.metaKey) || !ed.doc) return;
      e.preventDefault();
      const base = ed.zoomMode === "custom" ? ed.zoomFactor : computeFitZoom(ed.zoomMode);
      // The point under the pointer stays where it is (CANVAS-08): Word's and
      // ONLYOFFICE's Ctrl+wheel, and a trackpad pinch, which arrives as this.
      wheelPointer = { x: e.clientX, y: e.clientY };
      ed.setZoom(clampZoom(base * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
    },
    { passive: false },
  );

  // Re-fit on viewport resize while a fit mode is active.
  let fitResizeRaf = 0;
  window.addEventListener("resize", () => {
    if (ed.zoomMode === "custom") {
      // The zoom is unchanged but the window is not: a different viewport height
      // needs different sheets and gives the scroll mapping a new travel.
      ed.updatePageWindow({ force: true });
      ed.paintPagesInView();
      return;
    }
    cancelAnimationFrame(fitResizeRaf);
    fitResizeRaf = requestAnimationFrame(() => ed.renderAll());
  });

  // A FILE dragged anywhere in the window is the editor's (audit FILE-14). The
  // handlers sat on the viewport only, so a file let go over the header, the
  // ribbon or the status bar fell through to the browser, which opened it in
  // place of the editor tab. Google Docs and Word catch it anywhere. On the
  // document: one overlay over the whole window, one open path. Only drags that
  // carry files are claimed — text dragged within a field keeps its own default.
  const carriesFiles = (event) => [...(event.dataTransfer?.types ?? [])].includes("Files");
  const setDragging = (on) => document.body.classList.toggle("file-dragging", on);
  document.addEventListener("dragover", (event) => {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    setDragging(true);
  });
  // Leaving the WINDOW (no element to go to) ends the hover; moving between
  // elements inside it fires `dragleave` too, and must not flicker the overlay.
  document.addEventListener("dragleave", (event) => {
    if (!event.relatedTarget) setDragging(false);
  });
  document.addEventListener("drop", (event) => {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    setDragging(false);
    // `handleFile` reports every failure it can name; this catch is for the ones
    // it cannot, so a dropped file can never fail into silence (docs/104 HF-065).
    ed.handleFile(event.dataTransfer?.files?.[0]).catch((err) => {
      console.error("open failed:", err);
      ed.setStatus("That file could not be opened", "error");
    });
  });

  // "Page X of Y" follows the page in view while scrolling (audit CANVAS-09).
  // Here because this module already owns the viewport's other listeners.
  createPageInView(ed);

  return { clampZoom, computeFitZoom, updateZoomDisplay, zoomMenu };
}
