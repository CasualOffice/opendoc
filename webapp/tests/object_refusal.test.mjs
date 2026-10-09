// Why a selected object refuses (`object_refusal.mjs`, `docs/109` HF-259).
//
// The engine publishes a reason beside every capability it withholds; this is
// the host half that carries it with the selection and says it.
import assert from "node:assert/strict";
import test from "node:test";

const { capabilityRefusal, createRefusedDrag, readCapabilityReasons, REFUSED_DRAG_THRESHOLD_PX } = await import(
  "../src/object_refusal.mjs"
);

/** The engine's own marked-refusal shape (`casual_doc_edit::refusal`). */
const MOVE =
  "refused: Only a floating object can be moved freely. Give this one a text wrap first.\u001fobject.not-floating-move";

test("reasons are read from a hit payload's JSON string and from an order entry's object", () => {
  assert.deepEqual(readCapabilityReasons({ capabilityReasons: JSON.stringify({ canMove: MOVE }) }), { canMove: MOVE });
  assert.deepEqual(readCapabilityReasons({ capabilityReasons: { canMove: MOVE } }), { canMove: MOVE });
});

test("a stale bridge or garbage yields no reasons rather than throwing", () => {
  assert.deepEqual(readCapabilityReasons(null), {});
  assert.deepEqual(readCapabilityReasons({}), {});
  assert.deepEqual(readCapabilityReasons({ capabilityReasons: "{not json" }), {});
  assert.deepEqual(readCapabilityReasons({ capabilityReasons: { canMove: 3, canWrap: "" } }), {});
  const freed = {
    get capabilityReasons() {
      throw new Error("null pointer passed to rust");
    },
  };
  assert.deepEqual(readCapabilityReasons(freed), {});
});

test("the reason is the engine's sentence, without its marker or its code", () => {
  const selection = { capabilityReasons: { canMove: MOVE } };
  assert.equal(
    capabilityRefusal(selection, "canMove"),
    "Only a floating object can be moved freely. Give this one a text wrap first.",
  );
});

test("a host that can route the code says it in the reader's language", () => {
  const selection = { capabilityReasons: { canMove: MOVE } };
  const route = (code) => (code === "object.not-floating-move" ? "Seul un objet flottant se déplace librement." : "");
  assert.equal(capabilityRefusal(selection, "canMove", route), "Seul un objet flottant se déplace librement.");
  // An unrouted code keeps the engine's English, which is more specific than any
  // generic replacement.
  assert.match(capabilityRefusal(selection, "canMove", () => ""), /^Only a floating object/);
});

test("an available capability has nothing to explain", () => {
  assert.equal(capabilityRefusal({ capabilityReasons: {} }, "canMove"), "");
  assert.equal(capabilityRefusal(null, "canMove"), "");
});

test("a CLICK on an object that cannot move stays silent; a DRAG says why, once", () => {
  const said = [];
  const drag = createRefusedDrag({ setStatus: (text, kind) => said.push([text, kind]) });
  const at = (x, y, buttons = 1) => ({ clientX: x, clientY: y, buttons });

  drag.arm(at(100, 100), "because");
  drag.move(at(101, 101));
  assert.deepEqual(said, [], "a jitter inside the threshold is still a click");
  drag.end();

  drag.arm(at(100, 100), "because");
  drag.move(at(100 + REFUSED_DRAG_THRESHOLD_PX, 100));
  drag.move(at(160, 140));
  drag.move(at(200, 180));
  assert.deepEqual(said, [["because", "error"]], "said once, however far the drag goes");
  drag.end();
  assert.equal(drag.move(at(300, 300)), false, "an ended press is not watched");
});

test("a press with no reason is not watched, and a released button ends the watch", () => {
  const said = [];
  const drag = createRefusedDrag({ setStatus: (text) => said.push(text) });
  drag.arm({ clientX: 0, clientY: 0 }, "");
  assert.equal(drag.move({ clientX: 50, clientY: 50, buttons: 1 }), false);
  drag.arm({ clientX: 0, clientY: 0 }, "because");
  assert.equal(drag.move({ clientX: 50, clientY: 50, buttons: 0 }), false, "the button came up off-window");
  assert.deepEqual(said, []);
});
