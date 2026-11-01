// Editing the text in a picture (`image_text_edit.mjs`, `docs/182` §20b):
// before a text edit, every unedited line box the selection touches is made
// editable, once; the box lookup is asked once per revision and paragraph;
// the commands are disabled with a reason outside a box.
import assert from "node:assert/strict";
import test from "node:test";

import {
  IMAGE_TEXT_COMMANDS,
  createImageTextEdit,
  imageTextMode,
  imageTextRefusal,
} from "../src/image_text_edit.mjs";

/** A fake engine: paragraphs `a` and `b` are unedited line boxes, `p` is body text. */
function fakeDoc() {
  const state = { a: "unedited", b: "unedited", p: "" };
  const calls = { state: 0, edits: [] };
  return {
    calls,
    caretTextUnpainted: (node) => state[node] === "unedited",
    imageTextState: (node) => {
      calls.state += 1;
      return state[node];
    },
    editImageText: (node, offset, mode) => {
      calls.edits.push([node, offset, mode]);
      state[node] = "edited";
      return { node, offset };
    },
  };
}

function editor(selection) {
  const doc = fakeDoc();
  const ed = { doc, selection, currentRevision: 1, setStatus: () => {} };
  const applied = [];
  const imageText = createImageTextEdit(ed, {
    applyEditResult: async (res) => {
      applied.push(res);
      ed.currentRevision += 1;
    },
  });
  return { ed, doc, imageText, applied };
}

const at = (node, offset) => ({ node, offset });

test("a selection from one line box into another makes both editable before the edit", async () => {
  const { doc, imageText } = editor({ anchor: at("a", 2), focus: at("b", 3) });
  assert.equal(await imageText.prepare(), true);
  assert.deepEqual(
    doc.calls.edits.map((e) => e[0]).sort(),
    ["a", "b"],
    "both ends' boxes are made editable",
  );
  // A second edit finds them edited and does nothing more.
  await imageText.prepare();
  assert.equal(doc.calls.edits.length, 2);
});

test("body text is never touched, and the box lookup is asked once per revision", () => {
  const { doc, imageText } = editor({ anchor: at("p", 0), focus: at("p", 0) });
  assert.equal(imageText.prepare(), true, "ordinary typing must reach the edit without an async pause");
  assert.equal(doc.calls.edits.length, 0);
  assert.equal(doc.calls.state, 0, "the O(1) unpainted gate keeps typing off the walk");
  imageText.commands();
  imageText.commands();
  assert.equal(doc.calls.state, 1, "the palette built twice at one caret asks once");
});

test("the commands are disabled with a reason outside a box, and Restore only after an edit", () => {
  assert.equal(imageTextRefusal(IMAGE_TEXT_COMMANDS.fill, ""), "imageText.reason.noBox");
  assert.equal(imageTextRefusal(IMAGE_TEXT_COMMANDS.restore, "unedited"), "imageText.reason.notEdited");
  assert.equal(imageTextRefusal(IMAGE_TEXT_COMMANDS.restore, "edited"), "");
  assert.equal(imageTextRefusal(IMAGE_TEXT_COMMANDS.blur, "unedited"), "");
  const { imageText } = editor({ anchor: at("p", 0), focus: at("p", 0) });
  const rows = imageText.commands();
  assert.deepEqual(
    rows.map((r) => r.id),
    [IMAGE_TEXT_COMMANDS.fill, IMAGE_TEXT_COMMANDS.blur, IMAGE_TEXT_COMMANDS.pixelate, IMAGE_TEXT_COMMANDS.restore],
  );
  assert.ok(rows.every((r) => !r.enabled && r.disabledReason), "never a dead control");
});

test("the stored fill is read safely", () => {
  assert.equal(imageTextMode({ localStorage: { getItem: () => "blur" } }), "blur");
  assert.equal(imageTextMode({ localStorage: { getItem: () => "smudge" } }), "fill");
  assert.equal(
    imageTextMode({
      get localStorage() {
        throw new Error("blocked");
      },
    }),
    "fill",
  );
});
