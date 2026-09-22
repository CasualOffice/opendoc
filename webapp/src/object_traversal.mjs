// Which objects Tab walks, and where it lands.
//
// The rule has two halves and the second one was missing. At the top level Tab
// cycles the document's floating objects, which is what Word does once one is
// selected. INSIDE a group it has to cycle that group's own children — and it
// did not: both used one flat top-level list, a group child is not in that
// list, and `findIndex` returning -1 was read as "nothing is selected" and
// restarted from the top. On the owner's Medical form that meant a group of
// nine shapes could be descended into exactly once, to its first child, after
// which Tab jumped back out to the group forever. Every other shape in it was
// unreachable by any gesture, keyboard or pointer.
//
// So the choice of SET is the decision worth naming and testing, separately
// from the DOM that carries it out.

/** Display names for the object kinds the engine reports. */
export const OBJECT_LABELS = {
  image: "Image",
  textbox: "Text box",
  shape: "Shape",
  group: "Group",
};

/**
 * The group whose children Tab should walk, or `null` at the top level.
 *
 * A group child's reference has `root !== subject`: the root is the anchored
 * group, the subject the shape within it. That distinction is the engine's and
 * the host does not reconstruct it from geometry (`docs/101` UXOBJ-001).
 */
export function traversalRoot(ref) {
  return ref && ref.root && ref.subject !== ref.root ? ref.root : null;
}

/**
 * Where a step of `step` lands in a list of `count`, from `current`.
 *
 * `current < 0` means the selection is not in this list — nothing selected, or
 * an object deleted out from under the traversal — and the step starts from
 * whichever end it is moving toward rather than refusing to move.
 */
export function nextObjectIndex(count, current, step) {
  if (count <= 0) return -1;
  if (current < 0) return step > 0 ? 0 : count - 1;
  return (current + step + count) % count;
}

/**
 * What a screen reader is told after a step.
 *
 * It names the SET, because "3 of 9" means a different thing inside a group and
 * there is no other cue that Escape is what leaves it. Handles are visual only.
 */
export function traversalAnnouncement(kind, index, count, inGroup) {
  return `${OBJECT_LABELS[kind] ?? "Object"} ${index + 1} of ${count}${inGroup ? " in group" : ""}`;
}

/** What a click on a group should do, given what is already held.
 *
 * Three outcomes, not two — the missing third is what made a grouped shape
 * selectable but not draggable (`docs/109` HF-173):
 *
 *   "select"  — take the group as a unit. The first click on any group, and a
 *               click on the group's own background once inside it.
 *   "descend" — take the shape under the pointer. Word's second click.
 *   "keep"    — the pointer is on the shape already held: change nothing and
 *               let the gesture continue, because this click is the start of
 *               a DRAG of that shape.
 *
 * "keep" used to be folded into "select", so pressing down on a selected
 * grouped shape re-selected its group and then dragged the group. The shape
 * could be picked and never moved.
 *
 * @param {{root?: string, subject?: string}|null|undefined} held
 * @param {{kind?: string, root?: string}|null|undefined} hit
 * @param {boolean} pointerOnHeldChild
 * @returns {"select"|"descend"|"keep"}
 */
export function groupClickAction(held, hit, pointerOnHeldChild) {
  if (!held?.root || hit?.kind !== "group" || hit.root !== held.root)
    return "select";
  if (held.subject !== held.root && pointerOnHeldChild) return "keep";
  return "descend";
}

/**
 * Whether a click should reach INTO the group it landed on.
 *
 * Retained as the yes/no form of `groupClickAction` for callers that only
 * need to know whether to resolve a descendant.
 */
export function clickDescendsIntoGroup(held, hit, pointerOnHeldChild) {
  return groupClickAction(held, hit, pointerOnHeldChild) === "descend";
}

/**
 * Whether Escape should climb to the parent group rather than drop to the text
 * caret — true exactly when the selection is something inside a group.
 *
 * The way out is the way in, one level at a time (`docs/117` §5 rule 4).
 * Without it, descending was a one-way door: Escape from a child went all the
 * way back to the document, so the group just picked up was gone and reaching
 * it again meant clicking again.
 */
export function escapeClimbsToGroup(ref) {
  return traversalRoot(ref) !== null;
}
