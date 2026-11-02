// SPDX-License-Identifier: Apache-2.0
// Enter on an EMPTY list item leaves the list, as in Word and Google Docs:
// one level out when the item is nested, out of the list at the outermost
// level. Pressing Enter twice is the universal way out of a list (`docs/109`
// UX-058, audit CANVAS-02).
//
// One rule for every way a paragraph break arrives — the Enter key AND the
// `beforeinput` `insertParagraph` an on-screen keyboard or IME sends — so a
// phone cannot get a third empty bullet where a desktop gets out of the list.
// The engine decides what "a list item" is from the numbering the reader SEES,
// style-supplied included (`list_levels.rs`), which is why this used to do
// nothing on Word's List Bullet items.

/** Ends or outdents the empty list item at a collapsed caret. Returns true when
 *  it handled the break (whether or not the edit landed), false when the caret
 *  is not on an empty list item and the break should split the paragraph. */
export async function exitEmptyListItem(ed) {
  if (!ed.selection || ed.hasRange()) return false;
  const node = ed.selection.focus.node;
  const listKind = ed.doc.listStyleAt(node);
  if (!listKind || ed.doc.paragraphLength(node) !== 0) return false;
  const level = ed.doc.listLevelAt?.(node) ?? 0;
  await ed.runToolbarEdit(
    (a, b, c, d) => (level > 0 ? ed.doc.adjustListLevel(a, b, c, d, -1) : ed.doc.toggleList(a, b, c, d, listKind)),
    { paragraphLevel: true },
  );
  return true;
}
