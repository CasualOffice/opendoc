import { test } from "node:test";
import assert from "node:assert/strict";

import {
  BAD_FIELD_KEYS,
  PRESET_COLOR,
  SHADOW_PRESETS,
  activePreset,
  optionsRequest,
  presetRequest,
  readShadow,
  shadowCommands,
} from "../src/shape_shadow.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";

const t = (key) => `<${key}>`;

test("Word's gallery: No shadow, nine outer presets in its order, then Shadow options", () => {
  const menu = shadowCommands({ shadow: null, mutationEnabled: true, mutationReason: "", t, apply() {}, openOptions() {} });
  assert.equal(menu.id, "object.shadow");
  const ids = menu.submenu.map((row) => row.id);
  assert.deepEqual(ids, [
    "object.shadow.none",
    ...SHADOW_PRESETS.map((preset) => `object.shadow.${preset.id}`),
    "object.shadow.options",
  ]);
  assert.equal(SHADOW_PRESETS.length, 9);
  assert.equal(menu.submenu[0].shortcut, "✓", "a shape with no shadow checks No shadow");
});

test("every preset is Word's black, 60% transparent, and differs only where Word's do", () => {
  for (const preset of SHADOW_PRESETS) {
    const request = presetRequest(preset);
    assert.equal(request.color, PRESET_COLOR);
    assert.equal(request.transparency, 0.6);
    if (preset.id === "center") {
      assert.deepEqual([request.distancePt, request.blurPt], [0, 5]);
    } else {
      assert.deepEqual([request.distancePt, request.blurPt], [3, 4]);
    }
  }
  // The angles point where the names say: bottom right is 45 degrees clockwise
  // from +x with y down, top left the opposite.
  const angle = (id) => SHADOW_PRESETS.find((preset) => preset.id === id).angle;
  assert.equal(angle("bottomRight"), 45);
  assert.equal(angle("topLeft"), 225);
  assert.equal(angle("bottom"), 90);
});

test("the current preset is checked and a custom shadow checks none", () => {
  const preset = SHADOW_PRESETS[2];
  const shadow = { color: "#000000", transparency: 0.6, angle: preset.angle, distancePt: 3, blurPt: 4 };
  assert.equal(activePreset(shadow), preset);
  const menu = shadowCommands({ shadow, mutationEnabled: true, mutationReason: "", t, apply() {}, openOptions() {} });
  assert.deepEqual(
    menu.submenu.filter((row) => row.shortcut === "✓").map((row) => row.id),
    [`object.shadow.${preset.id}`],
  );
  assert.equal(activePreset({ ...shadow, color: "#ff0000" }), null);
  assert.equal(activePreset(null), null);
});

test("each row applies through one call, and every row is refused with the review-mode reason", () => {
  const applied = [];
  let opened = 0;
  const menu = shadowCommands({
    shadow: null,
    mutationEnabled: true,
    mutationReason: "",
    t,
    apply: (request) => applied.push(request),
    openOptions: () => opened++,
  });
  menu.submenu[0].run();
  menu.submenu[1].run();
  menu.submenu.at(-1).run();
  assert.deepEqual(applied, [null, presetRequest(SHADOW_PRESETS[0])]);
  assert.equal(opened, 1);

  const refused = shadowCommands({ shadow: null, mutationEnabled: false, mutationReason: "why", t, apply() {}, openOptions() {} });
  assert.ok(refused.submenu.every((row) => row.enabled === false && row.disabledReason === "why"));
});

test("the engine's answer reads as cannot-have, has-none or has-one", () => {
  assert.equal(readShadow(""), undefined, "a picture cannot carry one");
  assert.equal(readShadow("not json"), undefined);
  assert.equal(readShadow('{"shadow":null}'), null);
  assert.deepEqual(readShadow('{"shadow":{"color":"#000000","transparency":0.6,"angle":45,"distancePt":3,"blurPt":4}}'), {
    color: "#000000",
    transparency: 0.6,
    angle: 45,
    distancePt: 3,
    blurPt: 4,
  });
});

test("the options form becomes a request in the engine's units, and a bad field is named", () => {
  assert.deepEqual(optionsRequest({ color: "#FF0000", transparency: "25", angle: "-90", distance: "10", blur: "2.5" }), {
    color: "#ff0000",
    transparency: 0.25,
    angle: 270,
    distancePt: 10,
    blurPt: 2.5,
  });
  assert.deepEqual(optionsRequest({ color: "red", transparency: 0, angle: 0, distance: 0, blur: 0 }), { error: "color" });
  assert.deepEqual(optionsRequest({ color: "#000000", transparency: 101, angle: 0, distance: 0, blur: 0 }), { error: "transparency" });
  assert.deepEqual(optionsRequest({ color: "#000000", transparency: 0, angle: "", distance: 0, blur: 0 }), { error: "angle" });
  assert.deepEqual(optionsRequest({ color: "#000000", transparency: 0, angle: 0, distance: 201, blur: 0 }), { error: "distance" });
  assert.deepEqual(optionsRequest({ color: "#000000", transparency: 0, angle: 0, distance: 0, blur: -1 }), { error: "blur" });
});

test("every key the module names has English", () => {
  const keys = [
    "shape.shadow",
    "shape.shadow.none",
    "shape.shadow.options",
    ...SHADOW_PRESETS.map((preset) => preset.key),
    ...Object.values(BAD_FIELD_KEYS),
  ];
  assert.deepEqual(keys.filter((key) => !(key in EN_STRINGS)), []);
});
