// The Arrange COMMANDS: position, stacking, grouping, rotation, and text in a
// shape.
//
// The rules are `object_arrange.mjs` (pure) and the DOM is
// `object_arrange_chrome.mjs`; this is the layer between them — the one that
// decides what is offered, says WHY when it is not, and runs the engine call
// through the host's single gated edit path.
//
// Every facade call reached from here shipped with no caller at all
// (`canGroupObjects`/`groupObjects`/`ungroupObject`, `setObjectZOrder`,
// `setObjectPosition`, `setObjectRotation`/`setObjectFlip`,
// `addTextToShape`) — SKILL §9.4, the pattern this repository pays for most
// often. Word's Shape Format ▸ Arrange group is the model for all of it.
//
// `io` is its whole contact with the application:
//
//   `doc()`          the engine handle
//   `selection()`    the live object selection
//   `state()`        `arrangeState()` — the ONE per-repaint gather
//   `runEdit(fn,o)`  the gated, undoable edit path (async, resolves to a bool)
//   `setStatus(s,k)`
//   `t(key, params)`
//   `select(entry)`  select the object an `objectOrder()` row describes
//   `modifier`       the localised Ctrl/⌘ label, derived not spelled
//
// Cost: O(1) per command. `state()` is gathered once by the caller; nothing
// here asks the document a second question, and nothing here runs on a
// pointermove path.
import {
  positionAvailability,
  positionPayload,
  readGroupability,
  rotationPlan,
} from "./object_arrange.mjs";

export function createObjectArrangeCommands(io) {
  /** Ctrl on Windows/Linux, ⌘ on a Mac — derived, never spelled, because a spec
   *  that asserts a Mac glyph fails on the Linux runner (`105` UX-009). */
  const OBJECT_MULTI_SELECT_MODIFIER = io.modifier;

  /** The objects held ALONGSIDE the primary selection, for Group.
   *
   *  Word, PowerPoint and Docs all build a multi-object selection with
   *  Ctrl/Cmd+click, and grouping is the only command here that needs more than
   *  one object — which is why this is a plain list rather than a second
   *  selection model. It is dropped whenever the primary selection changes by
   *  any other route, so it can never name an object that is no longer there.
   *
   *  Each entry carries BOTH ids, because the engine answers two different
   *  questions from two different ones: `groupObjects` takes the top-level
   *  ROOTS, and `objectRect` — which is what draws the held outline — is
   *  answered for the SUBJECT. Holding only the root drew nothing at all. */
  let objectMultiSelect = [];

  /** The nodes a Group command acts on: the primary selection first, then the
   *  others in the order they were picked — which the engine keeps as the new
   *  group's child order, and therefore as its paint order. */
  function groupCandidateNodes() {
    if (!io.selection() || io.selection().mode !== "selected") return [];
    return [io.selection().ref.root, ...objectMultiSelect.map((held) => held.root)];
  }

  /** Whether Group is offered, and why not — the ENGINE's verdict and the
   *  ENGINE's sentence, except for the one case it was never asked about. */
  function groupability() {
    const nodes = groupCandidateNodes();
    if (!io.doc() || nodes.length === 0) return { can: false, reason: io.t("object.wrap.notAnObject") };
    if (nodes.length < 2) {
      // The engine would answer "grouping needs at least two objects", which is
      // true and useless. This says what to DO, and it is the only reason here
      // this host writes for itself.
      return {
        can: false,
        reason: io.t("object.group.selectMore", { modifier: OBJECT_MULTI_SELECT_MODIFIER }),
      };
    }
    const verdict = readGroupability(io.doc().canGroupObjects?.(JSON.stringify(nodes)) ?? "");
    return {
      can: verdict.can,
      reason:
        verdict.reason ??
        io.t("object.group.selectMore", { modifier: OBJECT_MULTI_SELECT_MODIFIER }),
    };
  }

  /** Every placed top-level object right now — the "before" side of a diff.
   *  O(objects), on a command, never on a repaint. */
  function placedIds() {
    try {
      return new Set(JSON.parse(io.doc().objectOrder()).map((entry) => entry.node));
    } catch {
      return new Set();
    }
  }

  /** Selects whatever object appeared since `before`. */
  function selectAppeared(before) {
    let entry = null;
    try {
      entry = JSON.parse(io.doc().objectOrder()).find((row) => !before.has(row.node)) ?? null;
    } catch {
      entry = null;
    }
    if (entry) io.select(entry);
  }

  /** Whether restacking applies. A top-level float is ordered by
   *  `@relativeHeight`; a group child by its index among its siblings. An inline
   *  object is ordered by the text flow and has no stacking of its own. */
  function zOrderAvailability(state) {
    if (!state) return { enabled: false, reason: io.t("object.wrap.notAnObject") };
    if (!state.read.floating && !state.read.groupChild) {
      return { enabled: false, reason: io.t("object.z.notStackable") };
    }
    return { enabled: true, reason: null };
  }

  /** Applies one cell of Word's Position gallery. An in-line object is floated
   *  first, because a position is a claim only an anchor can carry. */
  function applyObjectPosition(preset) {
    const state = io.state();
    if (!state) return;
    const availability = positionAvailability(state.read);
    if (!availability.available) {
      io.setStatus(io.t(availability.reasonKey), "error");
      return;
    }
    void io.runEdit(() => {
      if (availability.floatFirst) io.doc().setObjectAnchorKind(state.root, "floating");
      return io.doc().setObjectPosition(state.root, JSON.stringify(positionPayload(preset)));
    }, { gate: true });
  }

  /** Word's Bring to Front / Bring Forward / Send Backward / Send to Back.
   *
   *  WHICH node is restacked matters, because OOXML stacks the two cases
   *  differently. A top-level float is ordered by `wp:anchor@relativeHeight`
   *  against the other floats in its band; a shape inside a group is ordered by
   *  its index among its siblings. Every shape this host inserts is wrapped in a
   *  group-of-one — the only shape a shape takes in this model — so its SUBJECT
   *  is technically a group child, and restacking it reorders it among its one
   *  sibling: a silent no-op, which is exactly what it did. The subject is
   *  restacked only when it really has siblings; otherwise the ROOT is, which is
   *  the object the reader sees.
   *
   *  The sibling count costs ONE engine call, on the command, never on a
   *  repaint. */
  function setObjectZOrderCommand(order) {
    const state = io.state();
    const availability = zOrderAvailability(state);
    if (!availability.enabled) {
      io.setStatus(availability.reason, "error");
      return;
    }
    let siblings = 0;
    if (state.node !== state.root) {
      try {
        siblings = JSON.parse(io.doc().objectDescendants(state.root)).length;
      } catch {
        siblings = 0;
      }
    }
    const target = siblings > 1 ? state.node : state.root;
    void io.runEdit(() => io.doc().setObjectZOrder(target, order), { gate: true });
  }

  /** Word's Arrange ▸ Group. */
  async function groupSelectedObjects() {
    const verdict = groupability();
    if (!verdict.can) {
      io.setStatus(verdict.reason, "error");
      return;
    }
    const nodes = groupCandidateNodes();
    // The object that appeared is the new group. The engine reports a caret, not
    // the node it created, so the host diffs the placed order — the same way
    // insert does. Selecting it is what Word does, and without it the chip goes
    // on describing a shape that is now somebody's child.
    const before = placedIds();
    const applied = await io.runEdit(() => io.doc().groupObjects(JSON.stringify(nodes)), { gate: true });
    if (!applied) return;
    objectMultiSelect = [];
    selectAppeared(before);
    io.setStatus(io.t("object.grouped"));
  }

  /** Word's Arrange ▸ Ungroup, peeling one layer at a time as Word does. */
  async function ungroupSelectedObject() {
    if (!io.doc() || io.selection()?.kind !== "group") {
      io.setStatus(io.t("object.ungroup.notAGroup"), "error");
      return;
    }
    const before = placedIds();
    const applied = await io.runEdit(() => io.doc().ungroupObject(io.selection().ref.root), { gate: true });
    if (!applied) return;
    selectAppeared(before);
    io.setStatus(io.t("object.ungrouped"));
  }

  /** Word's Rotate menu. Flips are ABSOLUTE in the facade, so a menu row reads
   *  what the object carries and writes the opposite flag back (`rotationPlan`). */
  function rotateSelectedObject(value) {
    const state = io.state();
    if (!state?.transform) {
      io.setStatus(io.t("object.rotate.unsupported"), "error");
      return;
    }
    const plan = rotationPlan(value, state.transform);
    if (!plan) return;
    void io.runEdit(
      () =>
        plan.op === "rotation"
          ? io.doc().setObjectRotation(state.node, plan.degrees)
          : io.doc().setObjectFlip(state.node, plan.flipH, plan.flipV),
      { gate: true },
    );
  }

  /** Word's and Docs' "Add text": a `wps:wsp` with a `wps:txbx` IS a shape with
   *  text, so this is a node rewrite rather than an overlay — a star stays a star
   *  and gets words inside it. The caret the engine returns is the new body's,
   *  which is what lets a double-click drop you straight into typing. */
  async function addTextToSelectedShape() {
    if (!io.doc() || io.selection()?.kind !== "shape") {
      io.setStatus(io.t("object.addText.notAShape"), "error");
      return false;
    }
    return io.runEdit(() => io.doc().addTextToShape(io.selection().node), { gate: true });
  }

  return {
    multiSelect: () => objectMultiSelect,
    /** Adds an object to the multi-selection (Ctrl/⌘+click), or removes it when
     *  it is already held — which is how both competitors' modifier behaves. */
    toggleMember(ref) {
      const at = objectMultiSelect.findIndex((held) => held.root === ref.root);
      if (at >= 0) objectMultiSelect.splice(at, 1);
      else objectMultiSelect.push({ root: ref.root, node: ref.subject ?? ref.root });
    },
    /** Drops the extra members. Called whenever the primary selection moves by
     *  any route but the modifier click, so the list can never name an object
     *  that is no longer on screen. */
    clearMembers() {
      objectMultiSelect = [];
    },
    candidates: groupCandidateNodes,
    groupability,
    zOrderAvailability,
    applyPosition: applyObjectPosition,
    setZOrder: setObjectZOrderCommand,
    group: groupSelectedObjects,
    ungroup: ungroupSelectedObject,
    rotate: rotateSelectedObject,
    addText: addTextToSelectedShape,
  };
}

/**
 * The Layout ▸ Arrange rows, as the ribbon/palette surface table takes them.
 *
 * Declared here rather than in `main.js` for the reason every row in that table
 * gives: one declaration is what stops the ribbon button, the enablement rule
 * and the palette entry drifting into three opinions. The English `label` and
 * `kw` are read at import, before a catalogue exists, exactly as the sibling
 * Layout and References rows are — `layout-references-surface.spec.mjs` is the
 * guard that every one of them reaches the palette too.
 *
 * `layout.arrange.bringForward` shipped permanently DISABLED carrying "Bring
 * forward needs a z-order operation the engine does not expose yet".
 * `setObjectZOrder` exists now, so the control is live and the reason is gone: a
 * reason left standing after its gap closed is the same lie as an overstatement
 * (`109` EV-007).
 *
 * O(rows), a fixed twelve.
 *
 * @param {object} commands what `createObjectArrangeCommands` returned
 * @param {object} buttons  the ribbon faces, as thunks (they are looked up at
 *                          import and may not exist on an embedded chrome)
 */
export function arrangeSurfaceRows(commands, buttons) {
  const one = (button) => (button ? [button].filter(Boolean) : []);
  return [
    { command: "layout.arrange.bringForward", label: "Bring object forward", kw: "bring forward z order layer front arrange stack", buttons: () => one(buttons.bringForward()), requires: "object", run: () => commands.setZOrder("forward") },
    { command: "layout.arrange.sendBackward", label: "Send object backward", kw: "send backward z order layer back arrange stack behind", buttons: () => one(buttons.sendBackward()), requires: "object", run: () => commands.setZOrder("backward") },
    { command: "layout.arrange.bringToFront", label: "Bring object to front", kw: "bring front z order layer top arrange stack", buttons: () => [], requires: "object", run: () => commands.setZOrder("front") },
    { command: "layout.arrange.sendToBack", label: "Send object to back", kw: "send back z order layer bottom arrange stack", buttons: () => [], requires: "object", run: () => commands.setZOrder("back") },
    { command: "layout.arrange.group", label: "Group objects", kw: "group combine objects shapes together arrange", buttons: () => one(buttons.group()), requires: "object", run: () => void commands.group() },
    { command: "layout.arrange.ungroup", label: "Ungroup objects", kw: "ungroup split apart objects shapes arrange", buttons: () => one(buttons.ungroup()), requires: "object", run: () => void commands.ungroup() },
    { command: "layout.arrange.rotateRight", label: "Rotate object right 90 degrees", kw: "rotate right clockwise 90 turn object shape arrange", buttons: () => one(buttons.rotate()), requires: "object", run: () => commands.rotate("right90") },
    { command: "layout.arrange.rotateLeft", label: "Rotate object left 90 degrees", kw: "rotate left anticlockwise counterclockwise 90 turn object shape arrange", buttons: () => [], requires: "object", run: () => commands.rotate("left90") },
    { command: "layout.arrange.flipHorizontal", label: "Flip object horizontally", kw: "flip horizontal mirror reverse object shape arrange", buttons: () => [], requires: "object", run: () => commands.rotate("flipH") },
    { command: "layout.arrange.flipVertical", label: "Flip object vertically", kw: "flip vertical mirror reverse object shape arrange", buttons: () => [], requires: "object", run: () => commands.rotate("flipV") },
    { command: "layout.arrange.inLine", label: "Put object in line with text", kw: "in line inline with text wrap flow anchor object image shape", buttons: () => [], requires: "object", run: () => buttons.inLine() },
    { command: "layout.arrange.addText", label: "Add text to shape", kw: "add edit text shape label caption inside words", buttons: () => [], requires: "object", run: () => void commands.addText() },
  ];
}
