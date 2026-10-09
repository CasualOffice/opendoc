// A text box's body properties between the engine's JSON and the inspector
// (`text_box_body.mjs`, `docs/109` HF-253).
//
// The shapes below are the engine's own: `TextBoxBodyProperties` serialises
// camelCase, omits every field at its default (so a freshly inserted text box is
// `{}`), and parses with `deny_unknown_fields`. The inspector used to read and
// write snake_case keys and to give up when `insets` was absent, which made its
// whole "Text box body" section a dead control.
import assert from "node:assert/strict";
import test from "node:test";

const { DEFAULT_INSETS_EMU, readTextBoxBody, writeTextBoxBody } = await import("../src/text_box_body.mjs");

const FIELDS = {
  insets: { left: "0.5", top: "0.05", right: "0.1", bottom: "0.05" },
  verticalAnchor: "bottom",
  horizontalOverflow: "overflow",
  verticalOverflow: "clip",
  autoFit: "shape",
};

test("an all-default body reads as Word's defaults, not as zero insets", () => {
  assert.deepEqual(readTextBoxBody("{}"), {
    insets: { left: 0.1, top: 0.05, right: 0.1, bottom: 0.05 },
    verticalAnchor: "top",
    horizontalOverflow: "overflow",
    verticalOverflow: "overflow",
    autoFit: "none",
  });
});

test("the engine's camelCase keys are the ones read", () => {
  const body = readTextBoxBody(
    JSON.stringify({
      insets: { leftEmu: 457_200, topEmu: 0, rightEmu: 91_440, bottomEmu: 45_720 },
      verticalAnchor: "center",
      horizontalOverflow: "clip",
      verticalOverflow: "ellipsis",
      autoFit: { mode: "normal", font_scale: 62_500 },
    }),
  );
  assert.equal(body.insets.left, 0.5);
  assert.equal(body.insets.top, 0);
  assert.equal(body.verticalAnchor, "center");
  assert.equal(body.horizontalOverflow, "clip");
  assert.equal(body.verticalOverflow, "ellipsis");
  assert.equal(body.autoFit, "normal");
});

test("Apply on a DEFAULT text box writes a record — it used to return silently", () => {
  const written = JSON.parse(writeTextBoxBody("{}", FIELDS));
  assert.deepEqual(written, {
    insets: { leftEmu: 457_200, topEmu: 45_720, rightEmu: 91_440, bottomEmu: 45_720 },
    verticalAnchor: "bottom",
    horizontalOverflow: "overflow",
    verticalOverflow: "clip",
    autoFit: { mode: "shape" },
  });
  // Only keys the model's `deny_unknown_fields` accepts.
  for (const key of Object.keys(written)) {
    assert.ok(
      ["insets", "verticalAnchor", "horizontalOverflow", "verticalOverflow", "autoFit"].includes(key),
      `${key} is not a TextBoxBodyProperties field`,
    );
  }
});

test("a kept Scale-text autofit keeps the scale Word authored", () => {
  const authored = JSON.stringify({ autoFit: { mode: "normal", font_scale: 62_500, line_spacing_reduction: 20_000 } });
  const written = JSON.parse(writeTextBoxBody(authored, { ...FIELDS, autoFit: "normal" }));
  assert.deepEqual(written.autoFit, { mode: "normal", font_scale: 62_500, line_spacing_reduction: 20_000 });
});

test("a bad inset is refused here rather than sent to the engine", () => {
  assert.equal(writeTextBoxBody("{}", { ...FIELDS, insets: { ...FIELDS.insets, left: "-1" } }), null);
  assert.equal(writeTextBoxBody("{}", { ...FIELDS, insets: { ...FIELDS.insets, top: "wide" } }), null);
});

test("something that is not a text box has no body to read or write", () => {
  assert.equal(readTextBoxBody(""), null);
  assert.equal(readTextBoxBody("not json"), null);
  assert.equal(writeTextBoxBody("", FIELDS), null);
  assert.equal(DEFAULT_INSETS_EMU.leftEmu, 91_440);
});
