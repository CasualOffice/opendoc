// Selection commands: moving the caret to a position, by a viewport (Page Up /
// Page Down), and Select all. Moved out of `main.js` unchanged (docs/177); it
// reaches the rest of the editor only through the context `ed`.
import { selectionMatchesRange } from "./caret_navigation.mjs";

/** The start or end of the story the caret is in — the one rule Ctrl+Home,
 *  Ctrl+End and Select All share (`docs/109` UX-057, audit CRITIC-01).
 *
 *  The document's edges are the BODY's (`firstPosition` / `lastPosition`, which
 *  the engine answers from the body alone). A header or footer has edges of its
 *  own only while it is OPEN for editing (`runningOpen`); a note body or a text
 *  box the caret is already inside has its own edges too, as in Word. Anything
 *  else — including a caret that has somehow ended up in a header or footer
 *  outside header/footer mode — goes to the body, so navigation can never be the
 *  thing that enters a running band. Returns `{node, offset}`. O(document): one
 *  engine walk. */
export function storyBoundary(doc, focusNode, { runningOpen = false } = {}, toEnd = false) {
  const kind = focusNode ? String(doc.storyKind?.(focusNode) ?? "body") : "body";
  const ownEdges = kind === "header" || kind === "footer" ? runningOpen : kind !== "body" && kind !== "";
  const caret = ownEdges ? doc.storyEdge(focusNode, toEnd) : toEnd ? doc.lastPosition() : doc.firstPosition();
  const at = { node: caret.node, offset: caret.offset };
  caret.free?.();
  return at;
}

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
    // A jump (Ctrl+Home/End, Go to) brings the caret into view; only the
    // viewport step below keeps the caret's screen height.
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

    const [pageNumber, x, y, w, h] = flat;
    const caretPage = ed.pages[pageNumber - 1];
    if (!caretPage) return;
    const { rect: pageRect, sx, sy } = ed.scaleOf(caretPage);
    const viewportRect = ed.viewportEl.getBoundingClientRect();
    // One screen less one line, Word's and Docs' page step (audit CANVAS-10:
    // the first PageDown moved the view 146px of a 612px viewport).
    const line = Math.max(16, (h || 0) * sy);
    const distance = Math.max(48, ed.viewportEl.clientHeight - line);
    const caretClientY = pageRect.top + y * sy;
    const column = ed.verticalGoal.columnFor(dir, ed.selection.focus, () => x) ?? x;
    const targetX = pageRect.left + column * sx + Math.max(1, (w * sx) / 2);
    const targetY = pageRect.top + y * sy + (dir === "pageUp" ? -distance : distance);
    const page = ed.pageFromClientPoint(targetX, targetY);
    if (!page) return;
    const to = ed.anchorAt(page, ed.clientPointEvent(targetX, targetY));
    if (!to) return;
    ed.verticalGoal.keep(column, to);

    ed.selection = extend ? { anchor: ed.selection.anchor, focus: to } : { anchor: to, focus: to };
    // The VIEW moves by the same step, so the caret keeps its place on screen
    // (Word and Docs). Scrolling only until the caret showed moved the view by
    // whatever was left below the caret, a fraction of a screen.
    const moved = ed.doc.caretRect(to.node, to.offset);
    const movedPage = moved.length >= 5 ? ed.pages[moved[0] - 1] : null;
    if (movedPage) {
      const now = ed.scaleOf(movedPage);
      ed.viewportEl.scrollTop += now.rect.top + moved[2] * now.sy - caretClientY;
    }
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

    // The story the caret is in: the body, or the header/footer/note being edited.
    // It used to run from the body's start to the LAST paragraph of every story, a
    // footer, so Select All + Delete emptied the headers and footers too (UX-057).
    const focusNode = ed.selection?.focus?.node ?? null;
    const scope = { runningOpen: !!ed.runningEditBand };
    const a = storyBoundary(ed.doc, focusNode, scope, false);
    const b = storyBoundary(ed.doc, focusNode, scope, true);
    ed.tableRange.clear();
    ed.selection = { anchor: a, focus: b };
    ed.drawSelection();
    ed.focusEditorSurface();
    ed.setStatus("Document selected");
  }

  return { navByViewport, navToPosition, selectAll };
}
