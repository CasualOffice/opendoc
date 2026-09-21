// What the Styles control offers, asked directly.
//
// This rule has been reported wrong four times and was, until now, only
// observable by opening a browser and counting cards. The failures it has
// actually had are all here: offering every style the document defines, offering
// a fixed three regardless of the document, and dropping the style at the caret
// off the end of the capped list — which is the one that makes the control unable
// to tell you where you are.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  OFFERED_STYLE_COUNT,
  RECOMMENDED_STYLES,
  offeredStyleNames,
  previewPx,
  styleSlug,
} from "../src/style_picker.mjs";

const defined = (...names) => new Set(names);

test("it offers the recommended styles the document defines, and no others", () => {
  const offered = offeredStyleNames({
    recommended: ["Normal", "Heading 1"],
    inUse: [],
    defined: defined("Normal", "Heading 1", "Envelope Return", "Table Heading"),
    active: "Normal",
  });
  assert.deepEqual(offered, ["Normal", "Heading 1"]);
});

test("it never offers more than the cap, however many the document defines", () => {
  const many = Array.from({ length: 40 }, (_, i) => `Style ${i}`);
  const offered = offeredStyleNames({
    recommended: many,
    inUse: many,
    defined: new Set(many),
    active: "Style 0",
  });
  assert.equal(offered.length, OFFERED_STYLE_COUNT);
});

test("the style at the caret is always offered, even past the cap", () => {
  // The failure this exists for: a custom style outside the recommended set is
  // what the caret is in, the cap is already full, and the control shows six
  // styles none of which is the one you are in.
  const recommended = [
    "Normal",
    "Body Text",
    "Title",
    "Subtitle",
    "Heading 1",
    "Heading 2",
  ];
  const offered = offeredStyleNames({
    recommended,
    inUse: [],
    defined: defined(...recommended, "Quotation"),
    active: "Quotation",
  });
  assert.equal(offered.length, OFFERED_STYLE_COUNT);
  assert.ok(
    offered.includes("Quotation"),
    `caret's style was dropped: ${offered.join(", ")}`,
  );
});

test("a style the document is using is offered after the caret leaves it", () => {
  const offered = offeredStyleNames({
    recommended: ["Normal"],
    inUse: ["Quotation"],
    defined: defined("Normal", "Quotation"),
    active: "Normal",
  });
  assert.deepEqual(offered, ["Normal", "Quotation"]);
});

test("a style the document does not define is never offered", () => {
  // A style remembered from a previously open document, or a name from the
  // recommended list this document simply does not have.
  const offered = offeredStyleNames({
    recommended: ["Normal"],
    inUse: ["Ghost"],
    defined: defined("Normal"),
    active: "AlsoGhost",
  });
  assert.deepEqual(offered, ["Normal"]);
});

test("nothing is offered twice", () => {
  const offered = offeredStyleNames({
    recommended: ["Normal", "Heading 1"],
    inUse: ["Normal", "Heading 1"],
    defined: defined("Normal", "Heading 1"),
    active: "Normal",
  });
  assert.deepEqual(offered, ["Normal", "Heading 1"]);
});

test("the recommended list is a short curated set, not a stylesheet", () => {
  // If this list grows to the size of a document's stylesheet the cap is the
  // only thing left holding the line, and the ORDER stops meaning anything.
  assert.ok(
    RECOMMENDED_STYLES.length <= 20,
    `${RECOMMENDED_STYLES.length} recommended styles`,
  );
  assert.equal(
    RECOMMENDED_STYLES[0],
    "Normal",
    "the default style comes first",
  );
  assert.equal(
    new Set(RECOMMENDED_STYLES).size,
    RECOMMENDED_STYLES.length,
    "no duplicates",
  );
});

test("preview sizes stay inside the range that keeps a menu a menu", () => {
  // A 28pt Title drawn at 28pt is a heading with a menu around it; a 6pt Caption
  // drawn at 6pt cannot be read. Both still have to be visibly different.
  assert.equal(previewPx(0), null);
  assert.equal(previewPx(undefined), null);
  for (const pt of [6, 8, 11, 12, 14, 18, 28, 72]) {
    const px = previewPx(pt);
    assert.ok(px >= 12 && px <= 22, `${pt}pt rendered at ${px}px`);
  }
  assert.ok(
    previewPx(28) > previewPx(11),
    "the hierarchy has to survive the clamp",
  );
});

test("slugs are usable as CSS class suffixes", () => {
  assert.equal(styleSlug("Heading 1"), "heading-1");
  assert.equal(styleSlug("Intense Quote"), "intense-quote");
  assert.equal(styleSlug("  Odd — Name!  "), "odd-name");
});
