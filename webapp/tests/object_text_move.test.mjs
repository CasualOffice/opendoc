// Moving an in-line object to a new place in the text (`docs/109` UX-OB-02):
// the gesture half, in node.
//
// The engine's half — that the object arrives as itself, that one Undo puts it
// back, that a refused drop says why — is `casual-doc-wasm`'s
// `inline_move_tests.rs`; the browser half, a real pointer dragging a real
// picture, is `e2e/object-text-move.spec.mjs`. What is asserted here is the
// part only this module decides: when a press becomes a drag, which point the
// drop is aimed at, which key makes it a copy and when that is read, and
// what the keyboard's "Move to where?" does with each key.
import assert from "node:assert/strict";
import test from "node:test";

const { DRAG_THRESHOLD_PX, copyKeyHeld, copyKeyLabel, createKeyboardObjectMove, createObjectTextDrag } =
  await import("../src/object_text_move.mjs");
const { EDGE_SCROLL_BAND_PX, EDGE_SCROLL_MAX_PX, edgeScrollStep } = await import("../src/edge_scroll.mjs");
const { APPLE_PLATFORM, STANDARD_PLATFORM } = await import("../src/keyboard.mjs");

/** A minimal element: enough of the DOM for the drop caret. */
function element() {
  const el = {
    style: {},
    dataset: {},
    children: [],
    parentNode: null,
    className: "",
    ownerDocument: null,
    appendChild(child) {
      child.parentNode = el;
      el.children.push(child);
      return child;
    },
    remove() {
      if (el.parentNode) el.parentNode.children = el.parentNode.children.filter((c) => c !== el);
      el.parentNode = null;
    },
  };
  el.ownerDocument = { createElement: () => element() };
  return el;
}

/** A host whose engine answers every hit test with `hitAt(x, y)` and records
 *  every move it is asked to make. */
function harness({ hitAt = (x) => ({ node: "p2", offset: Math.round(x / 10) }), blocked = false, platform = STANDARD_PLATFORM } = {}) {
  const overlay = element();
  const page = { pageNumber: 1, overlay };
  const calls = { hitTests: 0, moves: [], selected: [], status: [] };
  const io = {
    platform,
    // No frame loop in node: the edge-scroll step is asserted on its own below.
    frame: () => 0,
    cancelFrame: () => {},
    doc: () => ({
      hitTest(_page, x, y) {
        calls.hitTests += 1;
        const hit = hitAt(x, y);
        return hit ? { ...hit, free() {} } : null;
      },
      caretRect: (node, offset) => [1, offset * 10, 100, 0, 240],
      moveInlineObject(node, target, offset, copy) {
        calls.moves.push({ node, target, offset, copy });
        return { placedObject: copy ? "copy-id" : "", dirtyPages: [0] };
      },
    }),
    pageAt: () => page,
    pageByNumber: () => page,
    pointToTwip: (_page, event) => ({ x: event.clientX, y: event.clientY }),
    scaleOf: () => ({ sx: 1, sy: 1 }),
    blocked: () => blocked,
    runEdit: async (thunk) => {
      thunk();
      return true;
    },
    select: (node) => calls.selected.push(node),
    setStatus: (text) => calls.status.push(text),
    t: (key) => key,
    viewport: () => ({ getBoundingClientRect: () => ({ top: -1e6, bottom: 1e6, left: -1e6, right: 1e6 }), scrollTop: 0, scrollLeft: 0 }),
  };
  return { io, page, overlay, calls };
}

const picture = { node: "pic", kind: "image", canMoveInText: true, ref: { root: "pic", subject: "pic" } };
const press = (x, y, extra = {}) => ({ clientX: x, clientY: y, buttons: 1, pointerId: 1, ...extra });
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

test("a press is a click until it travels past the threshold, and a click moves nothing", async () => {
  const { io, page, calls } = harness();
  const drag = createObjectTextDrag(io);
  assert.equal(drag.arm(press(100, 100), page, picture), true);
  drag.move(press(100 + DRAG_THRESHOLD_PX - 1, 100));
  assert.equal(calls.hitTests, 0, "no hit test, no drop caret, inside the threshold");
  assert.equal(drag.dragging(), false);
  assert.equal(drag.finish(press(103, 100, { buttons: 0 })), true, "the press was watched");
  await settle();
  assert.deepEqual(calls.moves, [], "the click that selected commits nothing");
});

test("a drag aims the drop at what the click hit test answers, and moves there on release", async () => {
  const { io, page, overlay, calls } = harness();
  const drag = createObjectTextDrag(io);
  drag.arm(press(100, 100), page, picture);
  drag.move(press(300, 100));
  assert.equal(drag.dragging(), true);
  const caret = overlay.children.find((child) => child.className === "object-drop-caret");
  assert.ok(caret, "a drop caret is drawn");
  assert.equal(caret.style.left, "300px", "at the caret position the engine gives for the hit");
  drag.move(press(420, 100));
  assert.equal(caret.style.left, "420px", "and it follows the pointer");
  drag.finish(press(420, 100, { buttons: 0 }));
  await settle();
  assert.deepEqual(calls.moves, [{ node: "pic", target: "p2", offset: 42, copy: false }]);
  assert.equal(overlay.children.length, 0, "the drop caret goes with the drag");
  assert.deepEqual(calls.status.at(-1), "object.moved");
});

test("the copy key is read at RELEASE: Ctrl on Windows and Linux, Option on a Mac", async () => {
  for (const [platform, key, other] of [
    [STANDARD_PLATFORM, { ctrlKey: true }, { altKey: true }],
    [APPLE_PLATFORM, { altKey: true }, { ctrlKey: true }],
  ]) {
    assert.equal(copyKeyHeld(key, platform), true);
    assert.equal(copyKeyHeld(other, platform), false, `${platform}: the other key is not the copy key`);
    const { io, page, calls } = harness({ platform });
    const drag = createObjectTextDrag(io);
    drag.arm(press(100, 100), page, picture);
    drag.move(press(300, 100)); // dragged without the key...
    drag.finish(press(300, 100, { buttons: 0, ...key })); // ...and released with it
    await settle();
    assert.equal(calls.moves[0].copy, true, `${platform}: the release decides`);
    assert.deepEqual(calls.selected, ["copy-id"], "and the dropped copy becomes the selection");
    assert.equal(calls.status.at(-1), "object.copied");
  }
  assert.equal(copyKeyLabel(STANDARD_PLATFORM), "Ctrl", "never a Mac glyph off a Mac (105 UX-009)");
});

test("Esc cancels a drag in flight, and nothing moves", async () => {
  const { io, page, overlay, calls } = harness();
  const drag = createObjectTextDrag(io);
  drag.arm(press(100, 100), page, picture);
  drag.move(press(300, 100));
  assert.equal(drag.cancel(), true);
  assert.equal(overlay.children.length, 0);
  assert.equal(drag.finish(press(300, 100, { buttons: 0 })), false, "the release finds nothing to finish");
  await settle();
  assert.deepEqual(calls.moves, []);
});

test("an object that cannot move in the text is not armed, so the caller explains why", () => {
  const { io, page } = harness();
  const drag = createObjectTextDrag(io);
  assert.equal(drag.arm(press(0, 0), page, { ...picture, canMoveInText: false }), false);
  assert.equal(drag.arm(press(0, 0), page, null), false);
  assert.equal(drag.active(), false);
});

test("in Viewing or Suggesting the drag is refused once, at the threshold, and never commits", async () => {
  const { io, page, calls } = harness({ blocked: true });
  const drag = createObjectTextDrag(io);
  drag.arm(press(100, 100), page, picture);
  drag.move(press(300, 100));
  assert.equal(drag.dragging(), false);
  assert.equal(calls.hitTests, 0, "no drop caret is offered for a drop that cannot land");
  drag.finish(press(300, 100, { buttons: 0 }));
  await settle();
  assert.deepEqual(calls.moves, []);
});

test("the edge band scrolls towards the edge, faster the deeper, on both axes", () => {
  const rect = { top: 0, bottom: 800, left: 0, right: 1000 };
  assert.deepEqual(edgeScrollStep(rect, 500, 400), { dx: 0, dy: 0 }, "nothing in the middle");
  assert.equal(edgeScrollStep(rect, 500, 800).dy, EDGE_SCROLL_MAX_PX, "full speed at the bottom edge");
  assert.ok(edgeScrollStep(rect, 500, 800 - EDGE_SCROLL_BAND_PX / 2).dy > 0);
  assert.ok(edgeScrollStep(rect, 500, 0).dy < 0, "up at the top");
  assert.ok(edgeScrollStep(rect, 0, 400).dx < 0, "left at the left");
  assert.ok(edgeScrollStep(rect, 1000, 400).dx > 0, "right at the right");
});

// ---- Word's F2, "Move to where?" ------------------------------------------------

function keyboard({ selected = picture, caret = { node: "p2", offset: 7 } } = {}) {
  const { io, calls } = harness();
  let held = selected;
  calls.left = 0;
  const kio = {
    ...io,
    selection: () => held,
    caret: () => caret,
    leaveObject: () => {
      held = null;
      calls.left += 1;
    },
    refusal: () => "because",
  };
  return { move: createKeyboardObjectMove(kio), calls };
}
const key = (name, extra = {}) => ({ key: name, shiftKey: false, ctrlKey: false, metaKey: false, altKey: false, preventDefault() {}, ...extra });

test("F2 asks Move to where?, the arrows go there, and Enter moves the object", async () => {
  const { move, calls } = keyboard();
  assert.equal(move.onKey(key("F2")), true);
  assert.equal(calls.left, 1, "the object is left for a caret, so the arrows move the caret");
  assert.equal(calls.status.at(-1), "object.moveTo.prompt");
  assert.equal(move.onKey(key("ArrowDown")), false, "navigation is the caret's, and keeps the move waiting");
  assert.equal(move.pending(), true);
  assert.equal(move.onKey(key("Enter")), true, "Enter is the move's, not a paragraph break");
  await settle();
  assert.deepEqual(calls.moves, [{ node: "pic", target: "p2", offset: 7, copy: false }]);
  assert.deepEqual(calls.selected, ["pic"], "and the object is selected at its new place");
});

test("Shift+F2 copies, Esc cancels, and typing abandons the move to do what it does", async () => {
  const copy = keyboard();
  copy.move.onKey(key("F2", { shiftKey: true }));
  assert.equal(copy.calls.status.at(-1), "object.copyTo.prompt");
  copy.move.onKey(key("Enter"));
  await settle();
  assert.equal(copy.calls.moves[0].copy, true);

  const cancelled = keyboard();
  cancelled.move.onKey(key("F2"));
  assert.equal(cancelled.move.onKey(key("Escape")), true);
  assert.equal(cancelled.calls.status.at(-1), "object.moveTo.cancelled");
  assert.equal(cancelled.move.onKey(key("Enter")), false, "Enter is a paragraph break again");

  const typed = keyboard();
  typed.move.onKey(key("F2"));
  assert.equal(typed.move.onKey(key("x")), false, "the letter is typed");
  assert.equal(typed.move.pending(), false, "and the move is abandoned");
  await settle();
  assert.deepEqual([...copy.calls.moves.slice(1), ...cancelled.calls.moves, ...typed.calls.moves], []);
});

test("F2 on an object that cannot move in the text says why instead", () => {
  const { move, calls } = keyboard({ selected: { ...picture, canMoveInText: false } });
  assert.equal(move.onKey(key("F2")), true);
  assert.equal(move.pending(), false);
  assert.equal(calls.left, 0, "the selection is kept");
  assert.equal(calls.status.at(-1), "because");
});

test("F2 with nothing selected is not this module's key", () => {
  const { move } = keyboard({ selected: null });
  assert.equal(move.onKey(key("F2")), false);
});
