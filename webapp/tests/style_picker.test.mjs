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
  caretContexts,
  offeredStyleNames,
  previewPx,
  styleMenuGroups,
  styleSlug,
} from "../src/style_picker.mjs";

const defined = (...names) => new Set(names);

test("recommended styles come first", () => {
  const offered = offeredStyleNames({
    recommended: ["Normal", "Heading 1"],
    inUse: [],
    defined: defined("Normal", "Heading 1", "Envelope Return"),
    active: "Normal",
  });
  assert.deepEqual(offered.slice(0, 2), ["Normal", "Heading 1"]);
});

test("the list FILLS to the cap from the document's own styles", () => {
  // The defect: the rule was "recommended ∩ defined", which assumes every
  // document uses Word's English style names. The owner's Medical Incident
  // Report Form defines Normal, p1, No Spacing, header and footer — of which
  // only `Normal` is recommended — so the control offered ONE style out of
  // five and read as broken.
  const names = ["Normal", "p1", "No Spacing", "header", "footer"];
  const offered = offeredStyleNames({
    recommended: ["Normal", "No Spacing"],
    inUse: [],
    defined: defined(...names),
    active: "",
  });
  assert.equal(offered.length, 5, `offered only: ${offered.join(", ")}`);
  for (const name of names) {
    assert.ok(offered.includes(name), `${name} is defined but not offered`);
  }
});

test("machinery styles are ranked last, not dropped", () => {
  // `header`/`footer`/`annotation text` are not what you apply to body text,
  // so they come after ordinary styles — but dropping them would leave a
  // document whose only other styles are those with an empty control, which
  // is the failure this rule exists to prevent.
  const offered = offeredStyleNames({
    recommended: ["Normal"],
    inUse: [],
    defined: defined(
      "Normal",
      "header",
      "footer",
      "Quotation",
      "annotation text",
    ),
    active: "",
  });
  assert.deepEqual(offered.slice(0, 2), ["Normal", "Quotation"]);
  assert.ok(offered.includes("header"), "still reachable");
  assert.ok(
    offered.indexOf("Quotation") < offered.indexOf("header"),
    `ordinary styles must outrank machinery ones: ${offered.join(", ")}`,
  );
});

test("the recommended set names PARAGRAPH styles only", () => {
  // `Strong`, `Emphasis` and the rest of Word's quick set are CHARACTER
  // styles. `listStyles()` returns paragraph styles alone, so naming them
  // here was dead weight that could never match — and it hid how short the
  // real list was.
  for (const characterStyle of [
    "Strong",
    "Emphasis",
    "Subtle Reference",
    "Book Title",
  ]) {
    assert.ok(
      !RECOMMENDED_STYLES.includes(characterStyle),
      `${characterStyle} is a character style and cannot be a paragraph style`,
    );
  }
  // And the one that was simply missing.
  assert.ok(
    RECOMMENDED_STYLES.includes("No Spacing"),
    "Word's quick set has No Spacing",
  );
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

test("a document with more styles than the cap still offers exactly the cap", () => {
  // Filling must not become "show everything" — the cap is the whole reason
  // this control was rebuilt (`docs/115`).
  const many = Array.from({ length: 40 }, (_, i) => `Style ${i}`);
  const offered = offeredStyleNames({
    recommended: [],
    inUse: [],
    defined: new Set(many),
    active: "",
  });
  assert.equal(offered.length, OFFERED_STYLE_COUNT);
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

// --- narrowing by where the caret is ----------------------------------------

test("a header offers the document's header style first", () => {
  // The narrowing the owner asked for: filter by WHERE THE CURSOR IS. In a
  // header, `header` is the style worth offering; a list that ignores the
  // caret makes the reader hunt for it.
  const defined = new Set(["Normal", "No Spacing", "header", "footer", "p1"]);
  const offered = offeredStyleNames({
    recommended: ["Normal", "No Spacing"],
    inUse: [],
    defined,
    active: "",
    contexts: caretContexts({ story: "header" }),
  });
  assert.equal(offered[0], "header", `offered: ${offered.join(", ")}`);
  // …and the general ones are still there, just after it.
  assert.ok(offered.includes("Normal"));
});

test("a table cell offers the document's table style first", () => {
  const offered = offeredStyleNames({
    recommended: ["Normal"],
    inUse: [],
    defined: defined("Normal", "Table Paragraph", "Body Text"),
    active: "",
    contexts: caretContexts({ inTable: true }),
  });
  assert.equal(offered[0], "Table Paragraph", `offered: ${offered.join(", ")}`);
});

test("a list offers the document's list style first", () => {
  const offered = offeredStyleNames({
    recommended: ["Normal"],
    inUse: [],
    defined: defined("Normal", "List Paragraph", "Body Text"),
    active: "",
    contexts: caretContexts({ listKind: "bullet" }),
  });
  assert.equal(offered[0], "List Paragraph", `offered: ${offered.join(", ")}`);
});

test("ordinary body text is not narrowed", () => {
  // Narrowing must not fire where there is nothing to narrow to, or the
  // general case gets a random style promoted over Normal.
  assert.deepEqual(caretContexts({ story: "body" }), []);
  const offered = offeredStyleNames({
    recommended: ["Normal", "Body Text"],
    inUse: [],
    defined: defined("Normal", "Body Text", "header"),
    active: "",
    contexts: caretContexts({ story: "body" }),
  });
  assert.deepEqual(offered.slice(0, 2), ["Normal", "Body Text"]);
});

// --- the full list, and the filter ------------------------------------------

test("every defined style is reachable, not just the suggested six", () => {
  // The owner asked for the list back. Capping REACHABILITY at six hid styles
  // the document really had; the cap now bounds only the promoted group.
  const many = Array.from({ length: 30 }, (_, i) => `Style ${i}`);
  const defined = new Set(many);
  const suggested = offeredStyleNames({
    recommended: [],
    inUse: [],
    defined,
    active: "",
  });
  const { suggested: top, rest } = styleMenuGroups({ suggested, defined });
  assert.equal(
    top.length,
    OFFERED_STYLE_COUNT,
    "the promoted group is still short",
  );
  assert.equal(
    top.length + rest.length,
    many.length,
    "but everything the document defines is in the menu",
  );
});

test("typing narrows both groups", () => {
  const defined = defined_("Normal", "Heading 1", "Heading 2", "Quote");
  const { suggested, rest } = styleMenuGroups({
    suggested: ["Normal", "Heading 1"],
    defined,
    query: "head",
  });
  assert.deepEqual(suggested, ["Heading 1"]);
  assert.deepEqual(rest, ["Heading 2"]);
});

test("a query that matches nothing returns nothing, so the caller can say so", () => {
  const { suggested, rest } = styleMenuGroups({
    suggested: ["Normal"],
    defined: defined_("Normal", "Quote"),
    query: "zzz",
  });
  assert.equal(suggested.length + rest.length, 0);
});

test("machinery styles sort last inside the full list too", () => {
  const { rest } = styleMenuGroups({
    suggested: [],
    defined: defined_("header", "Quotation", "footer", "Body Text"),
  });
  assert.deepEqual(rest, ["Body Text", "Quotation", "footer", "header"]);
});

/** `defined` shadowed by a test-local above; this is the same helper. */
function defined_(...names) {
  return new Set(names);
}
