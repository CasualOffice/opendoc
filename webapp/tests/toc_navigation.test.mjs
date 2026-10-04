// The contents-entry resolver, which had no unit tests at all — which is how a
// table of contents over NUMBERED headings came to be completely inert while
// three end-to-end specs passed on fixtures that happened not to be numbered.
//
// The defect: Word writes a generated entry as tab FIELDS —
//
//     title                TAB page      an unnumbered heading
//     number TAB title     TAB page      a NUMBERED heading
//
// and `tocEntryLabel` cut the text at the FIRST tab. For the unnumbered shape
// that is the page tab and the label is right; for the numbered shape it is the
// tab after the list number, so every label collapsed to its bare number ("1.")
// and matched no heading in the index. Nothing was armed on screen: no cursor
// change, no hover affordance, and a click landed a caret instead of navigating.
//
// The owner reported it on a 16-page NDA whose entries read exactly
// `"1.\tDefinitions & Interpretations\t2"` — the shape of every numbered
// contract, report and thesis.
import { strict as assert } from "node:assert";
import { test } from "node:test";

import {
  buildHeadingIndex,
  buildHeadingLabels,
  isTocEntryStyle,
  resolveTocTarget,
  tocEntryLabel,
} from "../src/toc_navigation.mjs";

/** The entry text the owner's document actually produces, verbatim. */
const NDA_ENTRY = "1.\tDefinitions & Interpretations\t2";

/** A heading index whose heading text carries NO numbering, because the number
 *  is painted from the list and is not in the run text. */
const NDA_INDEX = buildHeadingIndex([
  "1\tn7\t0\tDefinitions & Interpretations",
  "1\tn9\t0\tConfidentiality & Privacy",
]);

test("a numbered Word entry keeps its title, not just its number", () => {
  assert.equal(tocEntryLabel(NDA_ENTRY), "1. Definitions & Interpretations");
});

test("a numbered entry resolves to the heading whose text omits the number", () => {
  assert.equal(
    resolveTocTarget(NDA_INDEX, NDA_ENTRY),
    "n7",
    "the heading's own text is the bare title; the number lives in the list",
  );
});

test("every entry of a numbered table resolves, not merely the first", () => {
  const entries = [NDA_ENTRY, "2.\tConfidentiality & Privacy\t5"];
  assert.deepEqual(
    entries.map((e) => resolveTocTarget(NDA_INDEX, e)),
    ["n7", "n9"],
  );
});

test("the unnumbered shape that already worked still works", () => {
  const index = buildHeadingIndex(["1\tn2\t0\tAlpha chapter"]);
  assert.equal(tocEntryLabel("Alpha chapter\t2"), "Alpha chapter");
  assert.equal(resolveTocTarget(index, "Alpha chapter\t2"), "n2");
});

test("a hand-built table with literal dot leaders still works", () => {
  const index = buildHeadingIndex(["1\tn3\t0\tAlpha chapter"]);
  assert.equal(tocEntryLabel("Alpha chapter......... 2"), "Alpha chapter");
  assert.equal(resolveTocTarget(index, "Alpha chapter......... 2"), "n3");
});

test("a leader written into the page field is still not part of the title", () => {
  assert.equal(tocEntryLabel("Alpha\t...... 2"), "Alpha");
});

test("numbering carried inside the title field resolves both directions", () => {
  // The entry is numbered and the heading is too: matching must not require the
  // numbering to be absent from the heading.
  const numberedHeading = buildHeadingIndex(["1\tn4\t0\t1.2 Beta chapter"]);
  assert.equal(resolveTocTarget(numberedHeading, "1.2 Beta chapter\t7"), "n4");
  // And the same entry against a heading that omits it.
  const bareHeading = buildHeadingIndex(["1\tn5\t0\tBeta chapter"]);
  assert.equal(resolveTocTarget(bareHeading, "1.2 Beta chapter\t7"), "n5");
});

test("a roman page number is dropped like an arabic one", () => {
  assert.equal(tocEntryLabel("2.1.3\tDeep heading\tiv"), "2.1.3 Deep heading");
});

test("an entry with no page number keeps its whole title", () => {
  assert.equal(tocEntryLabel("1.\tAlpha"), "1. Alpha");
});

test("a title that merely looks like a page number is not thrown away", () => {
  // Dropping the last field here would leave "1." — numbering and nothing else,
  // which can never be a title, so the field is kept instead.
  assert.equal(tocEntryLabel("1.\t2020"), "1. 2020");
});

test("the style id and the style name are both recognised", () => {
  // The owner's file writes the ID (`TOC1`, no space) while `styles.xml` names
  // the style `TOC 1`; ODF says `Contents 1`.
  for (const name of ["TOC1", "TOC 1", "toc 1", "Contents 1", "Index 2"]) {
    assert.equal(isTocEntryStyle(name), true, name);
  }
  for (const name of ["Normal", "Heading 1", "TOC", "TOCHeading", ""]) {
    assert.equal(isTocEntryStyle(name), false, name);
  }
});

test("a non-entry paragraph resolves to nothing", () => {
  assert.equal(resolveTocTarget(NDA_INDEX, "Some ordinary sentence.\t4"), null);
});

// A jump has to name a destination the reader recognises. An authored `\h` entry
// is a hyperlink to a `_TocNNNNNNNNN` bookmark, and the link path used to report
// that id verbatim — "Jumped to _Toc130812265" — while the SAME gesture on an
// entry that resolved through the contents path named the heading. One document,
// one gesture, two answers depending on which mechanism happened to win.
test("a heading's node resolves back to its text, for naming a jump", () => {
  const labels = buildHeadingLabels([
    "1\tn7\t0\tDefinitions & Interpretations",
    "1\tn9\t0\tConfidentiality & Privacy",
  ]);
  assert.equal(labels.get("n9"), "Confidentiality & Privacy");
  assert.equal(labels.get("n7"), "Definitions & Interpretations");
});

test("a node that is not a heading names nothing, so the caller can fall back", () => {
  const labels = buildHeadingLabels(["1\tn7\t0\tAlpha"]);
  assert.equal(labels.get("n99"), undefined);
});

test("the first heading with a node id wins, matching buildHeadingIndex", () => {
  const labels = buildHeadingLabels(["1\tn1\t0\tFirst", "2\tn1\t0\tSecond"]);
  assert.equal(labels.get("n1"), "First");
});

test("a malformed outline row is skipped rather than throwing", () => {
  const labels = buildHeadingLabels(["nonsense", "1\tn1", "1\tn2\t0\t   ", "1\tn3\t0\tReal"]);
  assert.equal(labels.size, 1);
  assert.equal(labels.get("n3"), "Real");
});
