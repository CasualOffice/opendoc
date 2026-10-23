// Selection commands: moving the caret to a position, by a viewport (Page Up /
// Page Down), and Select all. Moved out of `main.js` unchanged (docs/177); it
// reaches the rest of the editor only through the context `ed`.
import { selectionMatchesRange } from "./caret_navigation.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createSelectionCommands(ed) {
  function navToPosition(caret, extend) {
    ed.breakTypingSession();
    ed.pendingFormat = null; // caret moved → disarm typing format
    const to = { node: caret.node, offset: caret.offset };
    ed.verticalGoal.clear();
    if (typeof caret.free === "function") caret.free();
    ed.selection = extend ? { anchor: ed.selection.anchor, focus: to } : { anchor: to, focus: to };
    ed.drawSelection();
    ed.focusEditorSurface();
    ed.scrollCaretIntoView();
  }

  /** Page Up/Down moves the model caret by one visible editor viewport. Browser
   * geometry chooses only the page-local probe; `hitTest` returns the model anchor
   * that remains the source of truth. */
  function navByViewport(dir, extend) {
    if (!ed.selection) return;
    ed.breakTypingSession();
    ed.pendingFormat = null;
    const flat = ed.doc.caretRect(ed.selection.focus.node, ed.selection.focus.offset);
    if (flat.length < 5) return;

    const [pageNumber, x, y, w] = flat;
    const caretPage = ed.pages[pageNumber - 1];
    if (!caretPage) return;
    const { rect: pageRect, sx, sy } = ed.scaleOf(caretPage);
    const viewportRect = ed.viewportEl.getBoundingClientRect();
    const distance = Math.max(48, viewportRect.height - 48);
    const column = ed.verticalGoal.columnFor(dir, ed.selection.focus, () => x) ?? x;
    const targetX = pageRect.left + column * sx + Math.max(1, (w * sx) / 2);
    const targetY = pageRect.top + y * sy + (dir === "pageUp" ? -distance : distance);
    const page = ed.pageFromClientPoint(targetX, targetY);
    if (!page) return;
    const to = ed.anchorAt(page, ed.clientPointEvent(targetX, targetY));
    if (!to) return;
    ed.verticalGoal.keep(column, to);

    ed.selection = extend ? { anchor: ed.selection.anchor, focus: to } : { anchor: to, focus: to };
    ed.drawSelection();
    ed.focusEditorSurface();
    ed.scrollCaretIntoView();
  }

  /** Selects the active cell first, then the whole document on a repeated ⌘A. */
  function selectAll() {
    if (!ed.doc) return;
    ed.breakTypingSession();

    if (ed.selection && ed.doc.inTable(ed.selection.focus.node)) {
      const range = ed.doc.cellTextRange(ed.selection.focus.node);
      try {
        if (!range.found) {
          ed.setStatus(
            "Select All is unavailable because this cell crosses another editing surface",
            "error",
          );
          return;
        }
        const start = { node: range.startNode, offset: range.startOffset };
        const end = { node: range.endNode, offset: range.endOffset };
        if (!selectionMatchesRange(ed.selection, start, end)) {
          ed.tableRange.clear();
          ed.selection = { anchor: start, focus: end };
          ed.drawSelection();
          ed.focusEditorSurface();
          ed.setStatus("Cell contents selected — choose Select All again to select the document");
          return;
        }
      } finally {
        range.free();
      }
    }

    const a = ed.doc.firstPosition();
    const b = ed.doc.lastPosition();
    ed.tableRange.clear();
    ed.selection = {
      anchor: { node: a.node, offset: a.offset },
      focus: { node: b.node, offset: b.offset },
    };
    a.free();
    b.free();
    ed.drawSelection();
    ed.focusEditorSurface();
    ed.setStatus("Document selected");
  }

  return { navByViewport, navToPosition, selectAll };
}
