// Arrange: how an object sits in the page — wrap, position, stacking, grouping,
// rotation. The POLICY; `main.js` owns the chrome and the engine calls.
//
// WHY THIS FILE EXISTS. `setObjectAnchorKind`, `setObjectPosition`,
// `setObjectZOrder`, `groupObjects`/`ungroupObject`/`canGroupObjects` and
// `setObjectRotation`/`setObjectFlip` all landed in the facade and nothing in
// the editor called any of them. The gap the owner actually feels is narrower
// and worse than that list: inserting a picture gives you an INLINE object, and
// an inline object had no wrap control at all — the chip only appeared once the
// object was already floating, and "In line" did not exist as a mode anywhere in
// the product. So the default case had no way out of itself.
//
// COMPETITIVE STANDARD, control by control:
//
//   Wrap      Google Docs puts a chip directly under a selected image with the
//             modes as one row, "In line" first, and Word's Layout Options
//             button offers "In Line with Text" above the six wrapping modes.
//             Both treat in-line as a MODE of the same control, not as a
//             different state of the product. That is what is built here: one
//             row, seven choices, in-line first.
//   Position  Word's Arrange ▸ Position gallery: nine cells, "with square text
//             wrapping", each an alignment pair against the margin. Docs has the
//             same nine under "Fix position on page". Not a numeric dialog —
//             the numeric route already exists in the object inspector for
//             people who want exact EMU, and a gallery is the gesture.
//   Z-order   Word's Bring Forward / Send Backward split buttons, each with its
//             "to Front" / "to Back" sibling. Four commands, one engine op.
//   Group     Word's Arrange ▸ Group / Ungroup. Enablement is the ENGINE's
//             answer (`canGroupObjects` returns `{can, reason}`) so the disabled
//             face says why rather than being dead (SKILL §10).
//   Rotate    Word's Rotate menu: right 90, left 90, flip vertical, flip
//             horizontal. Discrete, and the same four both competitors ship.
//
// Everything here is a pure function of state the caller already has, which is
// what lets `tests/object_arrange.test.mjs` assert the rules — "picking In line
// on a floating object rewrites the anchor", "Group is refused with the engine's
// own reason" — in node, with no browser and no engine.
//
// Cost: O(1) per call. Nothing here reads the document.

/** The wrap row, in the order Docs and Word both lead with. `value` is this
 *  host's wrap vocabulary (`objectWrap`/`setObjectWrap`, which folds
 *  `behindDoc` into two pseudo-modes) plus `inline`, which is not a wrap mode at
 *  all but a different KIND of anchor — the whole point of the control. */
export const WRAP_CHOICES = Object.freeze([
  { value: "inline", key: "object.wrap.inline", inline: true },
  { value: "square", key: "object.wrap.square" },
  { value: "tight", key: "object.wrap.tight" },
  { value: "through", key: "object.wrap.through" },
  { value: "topAndBottom", key: "object.wrap.topAndBottom" },
  { value: "behind", key: "object.wrap.behind" },
  { value: "front", key: "object.wrap.front" },
]);

/** Word's Position gallery, as alignment pairs against the margin. `wrap` is
 *  square because that is what every cell in Word's gallery commits to — the
 *  gallery is "put it here, with text flowing round it". */
export const POSITION_PRESETS = Object.freeze([
  { id: "topLeft", key: "object.position.topLeft", h: "left", v: "top" },
  { id: "topCenter", key: "object.position.topCenter", h: "center", v: "top" },
  { id: "topRight", key: "object.position.topRight", h: "right", v: "top" },
  { id: "middleLeft", key: "object.position.middleLeft", h: "left", v: "center" },
  { id: "middleCenter", key: "object.position.middleCenter", h: "center", v: "center" },
  { id: "middleRight", key: "object.position.middleRight", h: "right", v: "center" },
  { id: "bottomLeft", key: "object.position.bottomLeft", h: "left", v: "bottom" },
  { id: "bottomCenter", key: "object.position.bottomCenter", h: "center", v: "bottom" },
  { id: "bottomRight", key: "object.position.bottomRight", h: "right", v: "bottom" },
]);

/** The four stacking commands, in Word's own order. */
export const Z_ORDER_CHOICES = Object.freeze([
  { value: "front", key: "object.z.front" },
  { value: "forward", key: "object.z.forward" },
  { value: "backward", key: "object.z.backward" },
  { value: "back", key: "object.z.back" },
]);

/** Word's Rotate menu. Flips are absolute in the facade, so a flip choice is
 *  resolved against what the object currently carries — see `rotationPlan`. */
export const ROTATE_CHOICES = Object.freeze([
  { value: "right90", key: "object.rotate.right90" },
  { value: "left90", key: "object.rotate.left90" },
  { value: "flipV", key: "object.rotate.flipVertical" },
  { value: "flipH", key: "object.rotate.flipHorizontal" },
]);

/**
 * What `objectPosition` said, in the shape the chrome asks questions of.
 *
 * `""` means "not an object" and `{"floating":false,"groupChild":true}` means
 * "a group's child, whose parent decides its position" — two answers a host
 * must tell apart, which is exactly why the facade distinguishes them.
 *
 * O(1).
 *
 * @param {string} json the raw `objectPosition(root)` answer
 */
export function readPosition(json) {
  if (!json) return { object: false, floating: false, groupChild: false, position: null };
  let parsed;
  try {
    parsed = JSON.parse(json);
  } catch {
    return { object: false, floating: false, groupChild: false, position: null };
  }
  return {
    object: true,
    floating: parsed.floating === true,
    groupChild: parsed.groupChild === true,
    position: parsed,
  };
}

/**
 * Whether the wrap row should be offered for this object at all, and why not.
 *
 * The refusal is a KEY, not a sentence: this module is pure and has no
 * catalogue, and a field literally called `reason` holding English is what the
 * unrouted-string scanner counts (and rightly).
 *
 * Deliberately NOT keyed on the `canWrap` capability bit: the engine sets that
 * only for objects that ALREADY float, so keying the control on it is what made
 * the control invisible in the one case the user meets first. What the control
 * needs is "is this a top-level object whose anchor kind I may rewrite", which
 * is what `objectPosition` answers.
 *
 * O(1).
 */
export function wrapAvailability(read) {
  if (!read.object) return { available: false, reasonKey: "object.wrap.notAnObject" };
  if (read.groupChild) return { available: false, reasonKey: "object.wrap.groupChild" };
  return { available: true, reasonKey: null };
}

/**
 * Which wrap chip reads as pressed: `inline` whenever the object is in the run
 * flow, and otherwise whatever `objectWrap` says.
 *
 * O(1).
 */
export function activeWrapChoice(read, wrapToken) {
  if (!read.object) return null;
  if (!read.floating) return "inline";
  return WRAP_CHOICES.some((choice) => choice.value === wrapToken) ? wrapToken : "square";
}

/**
 * The engine calls that take an object from where it is to the wrap mode
 * `value`, as an ordered list the caller executes.
 *
 * This is the one place the in-line conversion is expressed, and it is a real
 * node rewrite in the document rather than a flag: going from in-line to any
 * wrapping mode is `setObjectAnchorKind(root, "floating")` FOLLOWED BY the wrap
 * itself, because a newly floated object gets Word's conversion default (square)
 * and the user asked for something specific. Going the other way is the anchor
 * rewrite alone — an in-line object has no wrap to set, and `setObjectWrap`
 * would refuse it.
 *
 * An empty plan means the object is already in the requested mode. The caller
 * runs nothing, so a chip that re-asserts the current mode does not fill the
 * undo stack.
 *
 * O(1).
 */
export function wrapPlan(value, read, wrapToken) {
  const active = activeWrapChoice(read, wrapToken);
  if (value === active) return [];
  if (value === "inline") return [{ op: "anchorKind", kind: "inline" }];
  const plan = [];
  if (!read.floating) plan.push({ op: "anchorKind", kind: "floating" });
  plan.push({ op: "wrap", mode: value });
  return plan;
}

/**
 * The `setObjectPosition` payload for one Position-gallery cell.
 *
 * Only the fields the cell decides are sent. The facade treats an omitted field
 * as "keep what the object has", so a cell that restated the wrap distances
 * would be overwriting an author's `wp:wrapSquare` insets with this host's idea
 * of them.
 *
 * O(1).
 */
export function positionPayload(preset) {
  return {
    horizontal: { relativeFrom: "margin", align: preset.h },
    vertical: { relativeFrom: "margin", align: preset.v },
    wrap: "square",
  };
}

/**
 * Whether the Position gallery applies, and why not.
 *
 * An in-line object HAS no anchor, so a position cell has to float it first —
 * which is a legitimate thing for the gallery to do (Word's gallery does
 * exactly that, and its "In Line with Text" cell is the same control's first
 * row) but it must be said, not silently done. `floatFirst` is what tells the
 * caller to prepend the anchor rewrite.
 *
 * O(1).
 */
export function positionAvailability(read) {
  if (!read.object) return { available: false, reasonKey: "object.wrap.notAnObject", floatFirst: false };
  if (read.groupChild) return { available: false, reasonKey: "object.position.groupChild", floatFirst: false };
  return { available: true, reasonKey: null, floatFirst: !read.floating };
}

/**
 * The engine's own answer to "can these objects be grouped", unwrapped.
 *
 * The reason is the ENGINE's sentence, not one invented here: it knows the four
 * rules (floating, top-level, same paragraph, same page) and which one this
 * selection breaks. A host that wrote its own reason would be guessing, and
 * would drift the day a rule changed.
 *
 * O(1).
 *
 * @param {string} json the raw `canGroupObjects(nodes)` answer
 */
export function readGroupability(json) {
  if (!json) return { can: false, reason: null };
  try {
    const parsed = JSON.parse(json);
    return { can: parsed.can === true, reason: parsed.reason ?? null };
  } catch {
    return { can: false, reason: null };
  }
}

/**
 * The rotation/flip call one Rotate-menu choice makes, given what the object
 * currently carries (`objectTransform`'s answer, parsed).
 *
 * Flips are ABSOLUTE in the facade — two checkboxes, not two toggles — so a
 * "Flip horizontal" menu row has to read the current flags and write the
 * opposite one back. Doing that here rather than at the call site is what keeps
 * the menu row from having to know the facade's shape.
 *
 * O(1).
 */
export function rotationPlan(value, transform) {
  const current = Number(transform?.rotationDegrees ?? 0) || 0;
  const flipH = transform?.flipH === true;
  const flipV = transform?.flipV === true;
  switch (value) {
    case "right90":
      return { op: "rotation", degrees: (current + 90) % 360 };
    case "left90":
      return { op: "rotation", degrees: (current + 270) % 360 };
    case "flipH":
      return { op: "flip", flipH: !flipH, flipV };
    case "flipV":
      return { op: "flip", flipH, flipV: !flipV };
    default:
      return null;
  }
}

/** `objectTransform`'s answer, parsed, or `null` when the object models none.
 *  O(1). */
export function readTransform(json) {
  if (!json) return null;
  try {
    return JSON.parse(json);
  } catch {
    return null;
  }
}
