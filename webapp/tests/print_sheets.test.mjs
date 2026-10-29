// HF-036: a mixed-orientation document prints every page on its own sheet size.
//
// The raster print path sized one `@page` from the first page, so a landscape
// section printed onto portrait sheets and lost its right-hand third. Each
// distinct size now gets a named `@page` rule and each page names its own.
import { test } from "node:test";
import assert from "node:assert/strict";

import { namedPageRules } from "../src/print.mjs";

const LETTER = { widthTwip: 12240, heightTwip: 15840 };
const LANDSCAPE = { widthTwip: 15840, heightTwip: 12240 };

test("a landscape page gets its own sheet, sized landscape, and same sizes share one", () => {
  const { rules, names } = namedPageRules([LETTER, LANDSCAPE, LANDSCAPE, LETTER]);
  assert.equal(names[0], names[3], "the two portrait pages share a sheet rule");
  assert.equal(names[1], names[2], "the two landscape pages share a sheet rule");
  assert.notEqual(names[0], names[1], "portrait and landscape are different sheets");
  assert.match(rules, new RegExp(`@page ${names[0]} \\{ size: 8\\.5in 11in; margin: 0; \\}`));
  assert.match(rules, new RegExp(`@page ${names[1]} \\{ size: 11in 8\\.5in; margin: 0; \\}`));
  assert.equal(rules.match(/@page /g).length, 2, "one rule per distinct size");
});

test("a single-size document has one rule, and no pages has none", () => {
  assert.equal(namedPageRules([LETTER, LETTER]).rules.match(/@page /g).length, 1);
  assert.deepEqual(namedPageRules([]), { rules: "", names: [] });
});
