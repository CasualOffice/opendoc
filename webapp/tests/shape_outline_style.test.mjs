import assert from "node:assert/strict";
import test from "node:test";
import {
  ARROW_CHOICES,
  DASH_CHOICES,
  outlineStyleCommands,
  readOutlineStyle,
} from "../src/shape_outline_style.mjs";

// RM-30: Shape Outline ▸ Dashes and ▸ Arrows. Mutations that drive these red:
// passing the dash with the arrows (so an arrow choice re-dashes the line)
// fails the write test; checking the arrow row by the end alone fails the check
// test; offering Arrows on a picture border fails the picture test.

function rows(over = {}) {
  const applied = [];
  const list = outlineStyleCommands({
    style: { dash: "dash", head: "none", tail: "triangle" },
    picture: false,
    mutationEnabled: true,
    mutationReason: "",
    t: (key) => key,
    apply: (...args) => applied.push(args),
    ...over,
  });
  return { list, applied, sub: (id) => list.find((row) => row.id === id)?.submenu ?? [] };
}

test("Word's eight dashes and eight arrow pairs, each a model token", () => {
  const { sub } = rows();
  assert.deepEqual(sub("object.outline.dashes").map((row) => row.id.split(".").at(-1)), DASH_CHOICES.map((c) => c.id));
  assert.deepEqual(sub("object.outline.arrows").map((row) => row.id.split(".").at(-1)), ARROW_CHOICES.map((c) => c.id));
  assert.equal(DASH_CHOICES[0].token, "solid", "Solid first, and it is the token that clears the pattern");
});

test("a dash row writes only the dash; an arrow row writes only the two ends", () => {
  const { sub, applied } = rows();
  sub("object.outline.dashes").find((row) => row.id.endsWith(".longDashDot")).run();
  sub("object.outline.arrows").find((row) => row.id.endsWith(".both")).run();
  assert.deepEqual(applied, [
    ["largeDashDot", undefined, undefined],
    [undefined, "triangle", "triangle"],
  ]);
});

test("the current dash and the current arrow pair are the checked rows", () => {
  const { sub } = rows();
  const checked = (id) => sub(id).filter((row) => row.shortcut === "✓").map((row) => row.id);
  assert.deepEqual(checked("object.outline.dashes"), ["object.outline.dash.dash"]);
  assert.deepEqual(checked("object.outline.arrows"), ["object.outline.arrows.end"], "end only, not start or both");
});

test("a picture border has Dashes and no Arrows, as in Word; the review mode closes every row", () => {
  const picture = rows({ picture: true });
  assert.deepEqual(picture.list.map((row) => row.id), ["object.outline.dashes"]);
  const viewing = rows({ mutationEnabled: false, mutationReason: "closed" });
  for (const row of [...viewing.sub("object.outline.dashes"), ...viewing.sub("object.outline.arrows")]) {
    assert.equal(row.enabled, false, row.id);
    assert.equal(row.disabledReason, "closed", row.id);
  }
});

test("reading the outline frees the engine value, and no outline reads as null", () => {
  let freed = 0;
  const format = (fields) => ({ ...fields, free: () => (freed += 1) });
  assert.deepEqual(readOutlineStyle(format({ outline: "#000000", outlineDash: "dot", headEnd: "none", tailEnd: "oval" })), {
    dash: "dot",
    head: "none",
    tail: "oval",
  });
  assert.equal(readOutlineStyle(format({ outline: undefined })), null);
  assert.equal(readOutlineStyle(undefined), null);
  assert.equal(freed, 2);
});
