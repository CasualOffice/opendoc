// Resolve and Delete, acting on the comment the caret is inside.
//
// Both operations existed only on the comment CARD, which exists only while the
// review sidebar is open — so a reviewer reading a commented paragraph had to
// open a panel to resolve the comment in front of them. ONLYOFFICE puts both in
// its Collaboration tab's Comments group and Word does the same, targeting the
// comment containing the cursor rather than a sidebar selection.
//
// The engine ops are the same ones the card already calls
// (`setCommentResolved`, `deleteComment`), reached through the same `runEdit`
// gate with `keepView: true` — an edit whose target is a comment must not
// scroll the reader to the text caret, which is the defect fixed across all
// thirteen object call sites earlier and applies identically here.
//
// Lives in a module rather than in `main.js` because that file is on a hard line
// ratchet; the host ties are passed in, so this is also testable without a DOM.

/** Wires the two actions to the editor's own apply path.
 *
 *  `commentId()` is read at CALL time, never captured: the active comment
 *  changes with every caret move, and a captured id would resolve whichever
 *  comment happened to be active when the editor booted.
 *
 *  Complexity: O(1) — one engine call, no document walk.
 */
export function createReviewCommentActions({ runEdit, getDoc, commentId, announce, afterChange }) {
  /** Resolve it. Not a toggle: the ribbon button reads "Resolve", and a control
   *  that silently reopens what you just resolved because you pressed it twice
   *  is a different command wearing the same label. Reopening stays on the card,
   *  where the state is visible. */
  async function resolve() {
    const id = commentId();
    if (!id || !getDoc()) return false;
    const ok = await runEdit(() => getDoc().setCommentResolved(id, true), { keepView: true });
    if (ok) {
      announce?.("Comment resolved");
      afterChange?.();
    }
    return ok;
  }

  /** Delete it. No confirmation, deliberately: this is one undoable action and
   *  Word deletes immediately too. A modal in front of an undoable edit trains
   *  people to dismiss modals. The card keeps its confirm step because a card
   *  click is far easier to make by accident than a ribbon press. */
  async function remove() {
    const id = commentId();
    if (!id || !getDoc()) return false;
    const ok = await runEdit(() => getDoc().deleteComment(id), { keepView: true });
    if (ok) {
      announce?.("Comment deleted");
      afterChange?.();
    }
    return ok;
  }

  return { resolve, remove };
}
