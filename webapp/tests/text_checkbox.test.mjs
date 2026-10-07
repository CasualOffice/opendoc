import assert from "node:assert/strict";
import test from "node:test";
import { textCheckboxReplacement } from "../src/text_checkbox.mjs";

test("a leading text checkbox toggles from either side of its UTF-8 glyph", () => {
  for (const offset of [0, 3]) {
    assert.deepEqual(textCheckboxReplacement("☐ Layout matches reference", offset), { start: 0, end: 3, text: "☒" });
    for (const checked of ["☑", "☒"]) {
      assert.equal(textCheckboxReplacement(`${checked} Text parses`, offset).text, "☐");
    }
  }
});

test("ordinary symbols, inline boxes and the label remain editable text", () => {
  for (const text of ["© ® ✓", "a ☐ box", "□", "☐symbol"]) {
    assert.equal(textCheckboxReplacement(text, 0), null);
  }
  for (const offset of [-1, 4, 8, NaN]) {
    assert.equal(textCheckboxReplacement("☐ label", offset), null);
  }
});
