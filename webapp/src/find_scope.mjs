/** Is a match inside the "selection only" find scope?
 *
 *  Extracted from `main.js`, where the rule sat behind two reads it does not
 *  depend on — a checkbox's `checked` and a module-level `findScope` — so a
 *  range-containment rule with four branches could only be exercised through
 *  the Find chrome. The caller keeps those reads; this is the rule itself.
 *
 *  The comparison is delegated because ordering two positions in different
 *  blocks is the ENGINE's answer (`selectionEdge`), not arithmetic: offsets in
 *  different nodes are not comparable numbers.
 */

/** True when `match` lies within `scope`.
 *
 *  O(1) plus one `posLE` call per boundary, and `posLE` is O(1) for the common
 *  single-node case.
 *
 *  @param {{startNode: string, startOffset: number, endNode: string, endOffset: number}} match
 *  @param {{startNode: string, startOffset: number, endNode: string, endOffset: number}} scope
 *  @param {(aNode: string, aOff: number, bNode: string, bOff: number) => boolean} posLE
 */
export function matchWithinScope(match, scope, posLE) {
  const { startNode, startOffset, endNode, endOffset } = scope;
  const node = match.startNode;
  if (node === startNode) {
    if (match.startOffset < startOffset) return false;
    // A single-node scope (startNode === endNode) caps the upper bound too.
    return node === endNode ? match.endOffset <= endOffset : true;
  }
  if (node === endNode) return match.endOffset <= endOffset;
  // Interior node: in scope iff scopeStart <= match and match <= scopeEnd.
  return (
    posLE(startNode, startOffset, node, match.startOffset) &&
    posLE(node, match.endOffset, endNode, endOffset)
  );
}

/** The canonical comparator for `matchWithinScope`, over an engine handle.
 *
 *  It lives here rather than at the call site because of the `free()`: the edge
 *  is an engine-owned handle and leaking one leaks WASM memory on every match
 *  tested, which is per-keystroke work during a find. One place to get right.
 *
 *  O(1) for two offsets in the same node; otherwise one `selectionEdge` call.
 *
 *  @param {{selectionEdge: Function}} doc
 */
export function positionComparator(doc) {
  return (aNode, aOff, bNode, bOff) => {
    if (aNode === bNode) return aOff <= bOff;
    const edge = doc.selectionEdge(aNode, aOff, bNode, bOff, false);
    const aIsEarlier = edge.node === aNode;
    edge.free();
    return aIsEarlier;
  };
}
