// The shape gallery offers exactly what the engine models — checked against the
// engine's OWN table, not against a list somebody kept in step by hand.
//
// This is the guard for the defect that motivated the work. Fifteen of the
// twenty-two presets were modeled in `ShapeGeometry`, painted by the layout
// renderer and written by the exporter, while `Insert ▸ Shapes` offered seven,
// hand-written, in a spelling the facade only still accepts for compatibility.
// Nothing could have caught that, because the gallery and the model had no
// relationship a test could read.
//
// So the test reads `ShapeGeometry::preset_token` — the ONE token table, the
// same one import, export and `insertShape` resolve through — and asserts the
// gallery's token set equals it. Add a preset to the model and this goes red
// until the gallery offers it; drop one and it goes red until the gallery stops.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const MODEL = join(WEBAPP, "..", "crates", "casual-doc-model", "src", "v1", "body.rs");

const { SHAPE_GROUPS, SHAPE_PRESETS, shapeNameKey, shapePreviewShape } = await import(
  "../src/shape_catalogue.mjs"
);

/** Every `a:prstGeom@prst` token `ShapeGeometry::preset_token` can answer. */
function enginePresetTokens() {
  const source = readFileSync(MODEL, "utf8");
  const at = source.indexOf("pub const fn preset_token(self)");
  assert.ok(at > 0, "ShapeGeometry::preset_token moved — this guard must follow it");
  const body = source.slice(at, source.indexOf("\n    }\n", at));
  return [...body.matchAll(/Self::\w+ => "([A-Za-z0-9]+)"/g)].map((match) => match[1]);
}

test("the gallery offers every preset the engine models, and no other", () => {
  const engine = enginePresetTokens();
  assert.equal(engine.length, 22, "the engine's typed preset set changed size");
  assert.deepEqual(
    [...SHAPE_PRESETS.map((shape) => shape.token)].sort(),
    [...engine].sort(),
    "Insert ▸ Shapes and ShapeGeometry::preset_token disagree. A preset the engine " +
      "models and the gallery does not offer is modeled, painted and unreachable — " +
      "the exact defect this catalogue exists to make impossible.",
  );
});

test("every preset appears exactly once, in exactly one group", () => {
  const tokens = SHAPE_PRESETS.map((shape) => shape.token);
  assert.equal(new Set(tokens).size, tokens.length, "a preset is listed twice");
  const grouped = SHAPE_GROUPS.flatMap((group) => group.shapes.map((shape) => shape.token));
  assert.deepEqual(grouped, tokens);
});

test("every preset and every group carries a localisation key, never a literal", () => {
  for (const group of SHAPE_GROUPS) {
    assert.match(group.key, /^shapes\.group\./, group.id);
    for (const shape of group.shapes) {
      assert.match(shape.key, /^shapes\./, shape.token);
    }
  }
});

test("every key the catalogue names exists in the English catalogue", async () => {
  // A `t()` whose key is absent renders the KEY, which is how a gallery ends up
  // reading "shapes.homePlate" in every language including English.
  const { EN_STRINGS } = await import("../src/en_strings.mjs");
  for (const group of SHAPE_GROUPS) {
    assert.ok(group.key in EN_STRINGS, `${group.key} is not in EN_STRINGS`);
    for (const shape of group.shapes) {
      assert.ok(shape.key in EN_STRINGS, `${shape.key} is not in EN_STRINGS`);
    }
  }
});

test("shapeNameKey answers for every offered token and for nothing else", () => {
  for (const shape of SHAPE_PRESETS) assert.equal(shapeNameKey(shape.token), shape.key);
  assert.equal(shapeNameKey("cloudCallout"), null);
});

test("every preview is drawable, and stays inside its box", () => {
  for (const shape of SHAPE_PRESETS) {
    const spec = shapePreviewShape(shape.outline, 20);
    assert.ok(["polygon", "ellipse", "line", "rect"].includes(spec.tag), shape.token);
    for (const value of Object.values(spec.attrs)) {
      for (const number of String(value).split(/[\s,]+/).map(Number)) {
        assert.ok(Number.isFinite(number), `${shape.token} has a non-finite preview coordinate`);
        assert.ok(number >= -0.001 && number <= 20.001, `${shape.token} draws outside its box`);
      }
    }
  }
});

test("a polygon preview has at least three vertices", () => {
  // A two-point "polygon" is a line that will render as nothing, which is the
  // silent way a gallery cell comes out blank.
  for (const shape of SHAPE_PRESETS) {
    if (shape.outline.kind !== "polygon") continue;
    assert.ok(shape.outline.points.length >= 3, shape.token);
  }
});
