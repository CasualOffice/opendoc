import assert from "node:assert/strict";
import test from "node:test";
import { DEFAULT_GRADIENT_BASE, GRADIENT_CHOICES, gradientCommands, tint } from "../src/shape_gradient.mjs";

// RM-30: Shape Fill ▸ Gradient. Mutations that drive these red: tinting toward
// black fails the tint test; ignoring the shape's own colour (always the
// accent) fails the write test; passing `null` instead of `undefined` for the
// radial row (which wasm-bindgen does not read as "omitted") fails the last.

test("a tint mixes toward white", () => {
  assert.equal(tint("#000000", 0.5), "#808080");
  assert.equal(tint("#3355c4", 0), "#3355c4");
  assert.equal(tint("#3355c4", 1), "#ffffff");
  assert.equal(tint("not a colour", 0), DEFAULT_GRADIENT_BASE);
});

function menu(over = {}) {
  const applied = [];
  const entry = gradientCommands({
    fill: "#CC0000",
    mutationEnabled: true,
    mutationReason: "",
    t: (key) => key,
    apply: (...args) => applied.push(args),
    ...over,
  });
  return { entry, applied };
}

test("each direction writes the shape's own colour fading to its tint, at Word's angle", () => {
  const { entry, applied } = menu();
  assert.deepEqual(entry.submenu.map((row) => row.id), GRADIENT_CHOICES.map((c) => `object.fill.gradient.${c.id}`));
  entry.submenu.find((row) => row.id.endsWith(".linearDown")).run();
  assert.deepEqual(applied[0], ["#cc0000", tint("#cc0000", 0.75), 90]);
});

test("a shape with no fill starts from the accent; the radial row omits the angle; the mode closes all", () => {
  const { entry, applied } = menu({ fill: null });
  entry.submenu.find((row) => row.id.endsWith(".fromCenter")).run();
  assert.equal(applied[0][0], DEFAULT_GRADIENT_BASE);
  assert.equal(applied[0][2], undefined);
  const closed = menu({ mutationEnabled: false, mutationReason: "closed" }).entry.submenu;
  assert.ok(closed.every((row) => row.enabled === false && row.disabledReason === "closed"));
});
