// Word's UI names for its built-in styles, shown instead of the stored `w:name`
// — and ONLY shown: the stored name stays the style's identity.
//
// The Styles box, its search and the palette printed "heading 1", "caption" and
// "macro" beside "Title" and "Normal", because a .docx stores Word's built-ins
// under Word's internal spelling and the editor printed what was stored. Word
// shows "Heading 1", "Caption" and "Macro Text" (`desk-08c`, `desk-08f`).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { styleDisplayName } from "../src/style_names.mjs";
import { styleMenuGroups } from "../src/style_picker.mjs";

test("Word's lower-case built-ins are shown by their UI names", () => {
  assert.equal(styleDisplayName("heading 1"), "Heading 1");
  assert.equal(styleDisplayName("heading 9"), "Heading 9");
  assert.equal(styleDisplayName("toc 2"), "TOC 2");
  assert.equal(styleDisplayName("index 3"), "Index 3");
  assert.equal(styleDisplayName("caption"), "Caption");
  assert.equal(styleDisplayName("header"), "Header");
  assert.equal(styleDisplayName("footer"), "Footer");
  assert.equal(styleDisplayName("title"), "Title");
  assert.equal(styleDisplayName("normal"), "Normal");
  // The renames that are not just capitalisation.
  assert.equal(styleDisplayName("annotation text"), "Comment Text");
  assert.equal(styleDisplayName("macro"), "Macro Text");
});

test("a style that already reads as Word's name, or is the document's own, is left alone", () => {
  for (const name of ["Heading 1", "Title", "Normal", "Body Text", "List Bullet 2", "OpenDoc Bullet", "p1"]) {
    assert.equal(styleDisplayName(name), name);
  }
  // Word's built-ins stop at 9; a tenth is somebody's own style.
  assert.equal(styleDisplayName("heading 10"), "heading 10");
  assert.equal(styleDisplayName(""), "");
});

test("the style menu's search finds a built-in by the name a reader sees", () => {
  const defined = new Set(["Normal", "heading 1", "annotation text", "p1"]);
  // By what is shown…
  assert.deepEqual(styleMenuGroups({ suggested: [], defined, query: "Comment" }).rest, ["annotation text"]);
  // …and still by what is stored, so nothing that matched before stops matching.
  assert.deepEqual(styleMenuGroups({ suggested: [], defined, query: "annotation" }).rest, ["annotation text"]);
  // And the VALUE handed back is the stored name, never the label.
  assert.deepEqual(styleMenuGroups({ suggested: [], defined, query: "heading" }).rest, ["heading 1"]);
});

test("every surface that prints a paragraph style name prints it through styleDisplayName", () => {
  // The four places a reader meets a style name in `main.js`: a Styles-menu
  // option, the trigger's own label, the palette row and Paragraph properties'
  // Style list. Each used to print the stored name directly.
  const main = readFileSync(new URL("../src/main.js", import.meta.url), "utf8");
  for (const site of [
    "label.textContent = styleDisplayName(name);",
    'stylesTriggerLabel.textContent = styleDisplayName(active || "Normal");',
    "label: `Style: ${styleDisplayName(name)}`,",
    "...styles.map((s) => [s, styleDisplayName(s)])",
  ]) {
    assert.ok(main.includes(site), `a style name is printed raw again; expected: ${site}`);
  }
});
