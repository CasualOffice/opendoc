// The floating selection toolbar that sits above a text selection. Moved out of
// `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
import { onButton } from "./popover_manager.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createSelectionToolbar(ed) {
  // ---- Floating selection toolbar (appears above a text selection) ------------
  const selToolbar = document.getElementById("selToolbar");

  /** Shows the floating toolbar centred just above the current range selection (or
   *  below it when there's no room), or hides it when the selection is collapsed. */
  function positionSelToolbar() {
    if (ed.activeLink || !ed.selection || !ed.hasRange()) {
      selToolbar.hidden = true;
      return;
    }
    const rects = ed.pagesEl.querySelectorAll(".overlay .highlight");
    if (!rects.length) {
      selToolbar.hidden = true;
      return;
    }
    let top = Infinity;
    let bottom = -Infinity;
    let left = Infinity;
    let right = -Infinity;
    for (const el of rects) {
      const b = el.getBoundingClientRect();
      top = Math.min(top, b.top);
      bottom = Math.max(bottom, b.bottom);
      left = Math.min(left, b.left);
      right = Math.max(right, b.right);
    }
    selToolbar.hidden = false; // must be visible to measure
    const tw = selToolbar.offsetWidth;
    const th = selToolbar.offsetHeight;
    const viewport = ed.viewportEl.getBoundingClientRect();
    const topBound = Math.max(8, viewport.top + 8);
    const bottomBound = Math.min(window.innerHeight - 8, viewport.bottom - 8);
    let x = (left + right) / 2 - tw / 2;
    // …plus whatever the touch handles need. They hang above the selection's top
    // and below its bottom, and this bar is a FIXED element over the page: eight
    // pixels of gap put it through the middle of the start handle's dot and took
    // every touch aimed at it (`touch_selection.mjs`'s `clearance`).
    const gap = 8 + ed.touchSelection.clearance();
    let y = top - th - gap;
    if (y < topBound) y = bottom + gap; // no room above → drop below the selection
    if (y + th > bottomBound) {
      // A selection at the viewport bottom may have no full-height slot below it;
      // keep the bar visible and out of the browser chrome rather than allowing a
      // fixed-position toolbar to disappear below the window.
      y = Math.min(y, bottomBound - th);
    }
    y = Math.max(topBound, y);
    x = Math.max(8, Math.min(x, window.innerWidth - tw - 8));
    selToolbar.style.left = `${Math.round(x)}px`;
    selToolbar.style.top = `${Math.round(y)}px`;
  }

  for (const b of selToolbar.querySelectorAll("[data-fmt]")) {
    onButton(b, () => ed.toggleFormat(b.dataset.fmt));
  }
  // Text-color and highlight now use the ribbon's swatch-picker popovers, wired up
  // above (registerPopover on #selTextColorBtn / #selHighlightBtn). Keep clicks
  // inside the bar from collapsing the selection; hide on viewport scroll.
  selToolbar.addEventListener("mousedown", (e) => {
    if (e.target.tagName !== "INPUT" && e.target.tagName !== "SELECT") e.preventDefault();
  });
  ed.viewportEl.addEventListener("scroll", () => (selToolbar.hidden = true), { passive: true });
  /** Apply a font family to the range or arm it at the caret. */
  return { positionSelToolbar };
}
