// The Outline panel: the heading tree, scroll-to, and the flanking-panel
// toggles it shares. Moved out of `main.js` unchanged (docs/177); it reaches
// the rest of the editor only through the context `ed`.
import { renderOutline, reflectOutlineActive, outlineSectionAtBy } from "./outline_panel.mjs";
import { t } from "./i18n.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createOutline(ed) {
  // ---- Outline panel (heading tree → scroll-to) -------------------------------
  // The rows themselves are `outline_panel.mjs`; what is left here is the two
  // pieces of editor state it needs and the caret move a row performs.
  /** Rebuilds the outline list from the document's headings (no-op when hidden). */
  function buildOutline() {
    if (!ed.doc || ed.outlinePanel.hidden) return;
    ed.foldView.sync();
    renderOutline(ed.outlineBody, ed.doc.documentOutline(), {
      emptyText: t("outline.noHeadings"),
      onPick: navigateToNode,
      activeNode: ed.selection?.focus?.node ?? "",
      ...ed.foldView.outlineOptions(),
    });
    // The rows are new: read their nodes once, in document order.
    headings = [...ed.outlineBody.querySelectorAll(".outline-item")].map((row) => row.dataset.node);
    headingSet = new Set(headings);
    shown = undefined;
    reflectOutlineSelection();
  }

  /** The outline's heading nodes in document order, and the same as a set, read
   *  once per rebuild of the rows (never per caret move). */
  let headings = [];
  let headingSet = new Set();
  /** The node the current row was last set to; `undefined` = rows rebuilt. */
  let shown;

  /** Keeps the outline's current row on the heading whose SECTION holds the
   *  caret (audit CRITIC-08): Word's Navigation pane and the Docs outline both
   *  highlight the heading you are under, not only a heading you are on. The
   *  row carries `aria-current="location"` and is kept in view.
   *
   *  Runs on every toolbar pass while the outline is open, so it is kept off the
   *  document's size: one `caretRect` for the caret, then a binary search that
   *  reads O(log headings) heading positions on demand (`outlineSectionAtBy`),
   *  each one engine `caretRect` read LIVE, so no cache can go stale under an
   *  edit. The rows are touched, and scrolled, only when the current heading
   *  CHANGES — typing inside one section touches none of them. */
  function reflectOutlineSelection() {
    const focus = ed.selection?.focus;
    let node = focus?.node ?? "";
    if (focus && ed.doc && !headingSet.has(node)) {
      const caret = ed.doc.caretRect(focus.node, focus.offset ?? 0);
      node = "";
      if (caret.length >= 3) {
        node = outlineSectionAtBy(headings.length, (i) => {
          const box = ed.doc.caretRect(headings[i], 0);
          return box.length >= 3 ? { node: headings[i], page: box[0], y: box[2] } : null;
        }, caret[0], caret[2]) ?? "";
      }
    }
    if (node === shown) return;
    shown = node;
    reflectOutlineActive(ed.outlineBody, node);
    const current = ed.outlineBody.querySelector('.outline-item[aria-current="location"]');
    current?.scrollIntoView?.({ block: "nearest" });
  }

  /** Places the caret at the start of `node` and scrolls it into view. */
  function navigateToNode(node) {
    if (!ed.doc) return;
    // Picking an outline row IS placing the caret, so the caret must be painted
    // even when the target is the first heading — the one node that happens to
    // sit exactly where the load-time insertion point was seeded.
    ed.implicitCaretAt = null;
    ed.selection = { anchor: { node, offset: 0 }, focus: { node, offset: 0 } };
    ed.drawSelection();
    ed.scrollCaretIntoView("center");
  }

  /** Re-anchors everything positioned against PAGE geometry. Every flanking panel
   *  changes the canvas WIDTH, which re-centres the sheet, and the review markers
   *  and fold chevron are positioned against where the sheet WAS. `toggleReview`
   *  re-rendered the markers and `toggleOutline` did not, so opening and closing
   *  the outline left comment icons inside the page instead of beside it. */
  function reanchorPageOverlays() {
    ed.scheduleReviewMarginRender();
    if (ed.doc) ed.drawSelection();
  }

  /** Closes one flanking panel AND the rail tile that owns it — one control, so
   *  one statement of the rule. Three sites spelled this out (the review gutter,
   *  `toggleOutline`, the Pages navigator's `onExclusive`) and a panel closed
   *  without its tile leaves a rail button pressed over a panel that is gone. */
  function closeFlankingPanel(panel, rail) {
    if (!panel || panel.hidden) return;
    panel.hidden = true;
    rail?.setAttribute("aria-pressed", "false");
  }

  function toggleOutline() {
    ed.outlinePanel.hidden = !ed.outlinePanel.hidden;
    // One panel per side: the dock manager (`dock_manager.mjs`) closes Pages
    // when the outline takes the left slot, as it does for every panel.
    ed.railOutline.setAttribute("aria-pressed", String(!ed.outlinePanel.hidden));
    buildOutline();
    reanchorPageOverlays();
  }
  ed.railOutline.addEventListener("click", toggleOutline);

  // With a panel on each side the page can be wider than the space between
  // them; the viewport then scrolls sideways. Keep the page centred in view
  // when that space changes (a panel opened, closed or resized), as Docs does,
  // instead of showing the left gutter with the page cut off under the right
  // panel. Only when the width changed, so the reader's own sideways scroll is
  // left alone. O(1) per resize.
  let lastViewportWidth = 0;
  if (typeof ResizeObserver === "function" && ed.viewportEl) {
    new ResizeObserver(() => {
      const v = ed.viewportEl;
      const width = v.clientWidth;
      if (width === lastViewportWidth) return;
      lastViewportWidth = width;
      // Only while a side panel squeezes the page: a window that is simply
      // narrower than the page keeps the page's left edge in view, scrolled
      // from the start (responsive-shell.spec), as it always did.
      const panelOpen = !!document.querySelector(".workarea > .side-panel:not([hidden])");
      if (!panelOpen) return;
      const overflow = v.scrollWidth - width;
      v.scrollLeft = overflow > 0 ? Math.round(overflow / 2) : 0;
    }).observe(ed.viewportEl);
  }
  ed.outlineClose.addEventListener("click", toggleOutline);

  /** The Pages navigator. Its behaviour lives in `pages_panel.mjs` (HF-085): six
   *  closures in this file were the only description of what `view.pages` does. */
  return {
    buildOutline, closeFlankingPanel, reanchorPageOverlays, reflectOutlineSelection, toggleOutline,
  };
}
