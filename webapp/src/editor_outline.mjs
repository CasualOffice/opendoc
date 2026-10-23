// The Outline panel: the heading tree, scroll-to, and the flanking-panel
// toggles it shares. Moved out of `main.js` unchanged (docs/177); it reaches
// the rest of the editor only through the context `ed`.
import { renderOutline, reflectOutlineActive } from "./outline_panel.mjs";
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
  }

  /** Keeps the outline's active row synchronized with the model-backed caret. */
  function reflectOutlineSelection() {
    reflectOutlineActive(ed.outlineBody, ed.selection?.focus?.node ?? "");
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
    // Outline (left) and the review sidebar (right) are mutually exclusive so the
    // canvas is never squeezed from both sides at once. Opening the outline closes
    // the review sidebar; the reverse is enforced in renderReviewMarginItems.
    if (!ed.outlinePanel.hidden && !ed.reviewSidebar.hidden) ed.toggleReview(false);
    if (!ed.outlinePanel.hidden) closeFlankingPanel(ed.pagesPanel, ed.railPages);
    ed.railOutline.setAttribute("aria-pressed", String(!ed.outlinePanel.hidden));
    buildOutline();
    reanchorPageOverlays();
  }
  ed.railOutline.addEventListener("click", toggleOutline);
  ed.outlineClose.addEventListener("click", toggleOutline);

  /** The Pages navigator. Its behaviour lives in `pages_panel.mjs` (HF-085): six
   *  closures in this file were the only description of what `view.pages` does. */
  return {
    buildOutline, closeFlankingPanel, reanchorPageOverlays, reflectOutlineSelection, toggleOutline,
  };
}
