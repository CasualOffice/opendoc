// Zoom: the editable percentage, Fit width / Fit page, and Ctrl+scroll.
// setZoom, setZoomMode and stepZoom stay in main.js, which hands them to the
// view-zoom control at boot. Moved out of `main.js` unchanged (docs/177); it
// reaches the rest of the editor only through the context `ed`.
import { closePopover, registerPopover } from "./popover_manager.mjs";
import { fitPageIndex, fitZoomFactor, parseZoomInput, reflectZoomMenu } from "./view_zoom.mjs";
import { TWIPS_PER_INCH } from "./units.mjs";

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

  /** Repaints the zoom input (unless the user is mid-edit) and the preset checks. */
  function updateZoomDisplay() {
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

  // Drag-and-drop anywhere over the viewport.
  for (const type of ["dragover", "drop"]) {
    ed.viewportEl.addEventListener(type, (e) => e.preventDefault());
  }
  ed.viewportEl.addEventListener("dragover", () => ed.viewportEl.classList.add("dragging"));
  ed.viewportEl.addEventListener("dragleave", () => ed.viewportEl.classList.remove("dragging"));
  ed.viewportEl.addEventListener("drop", (e) => {
    ed.viewportEl.classList.remove("dragging");
    // `handleFile` reports every failure it can name; this catch is for the ones
    // it cannot, so a dropped file can never fail into silence (docs/104 HF-065).
    ed.handleFile(e.dataTransfer?.files?.[0]).catch((err) => {
      console.error("open failed:", err);
      ed.setStatus("That file could not be opened", "error");
    });
  });

  return { clampZoom, computeFitZoom, updateZoomDisplay, zoomMenu };
}
