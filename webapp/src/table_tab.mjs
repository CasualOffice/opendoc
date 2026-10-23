// What Tab and Shift+Tab land on inside a table: the destination cell's whole
// contents, selected (`docs/171` §5, `docs/174` OL-03).
//
// Word, Google Docs and ONLYOFFICE all select the next cell's contents on Tab,
// so typing REPLACES what was there — that is how a form or a filled-in table is
// edited cell by cell. This editor used to put a bare caret at offset 0, so
// typing inserted in front of the old text: Shift+Tab then "Y" turned "h" into
// "Yh" where the others give "Y".
//
// An EMPTY cell gets a caret, not an empty selection: there is nothing to
// replace, and a collapsed "selection" would paint nothing anyway.
//
// The bounds come from the engine (`cellTextRange`), never from canvas
// geometry: cell ownership is model state. A cell whose range the engine
// refuses (a text-box story inside it) also gets a caret, at the target the
// engine named.

/**
 * The selection a Tab move should leave: the destination cell's whole contents,
 * or `null` when the caret should simply be placed at `target` (an empty cell,
 * or one whose range the engine does not report).
 *
 * @param {{ cellTextRange(node: string): any }} doc the open document
 * @param {{ node: string, offset: number }} target the caret `moveTableCell` named
 * @returns {{ anchor: { node: string, offset: number }, focus: { node: string, offset: number } } | null}
 */
export function cellContentsSelection(doc, target) {
  const range = doc.cellTextRange(target.node);
  try {
    if (!range?.found) return null;
    const anchor = { node: range.startNode, offset: range.startOffset };
    const focus = { node: range.endNode, offset: range.endOffset };
    if (anchor.node === focus.node && anchor.offset === focus.offset) return null;
    return { anchor, focus };
  } finally {
    range?.free?.();
  }
}
