// Drift + honesty guard for the public DOCX fidelity support matrix
// (webapp/src/fidelity.js, rendered by fidelity.html). This does NOT re-derive
// support state — it locks the shape and a handful of load-bearing "must stay
// honest" cells so a careless edit can't silently overstate public claims or
// drop a construct family.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

// fidelity.js is a classic browser <script> (this package is type=module, so it
// can't be `require`d, and it must stay import/export-free for the browser).
// Evaluate it in a sandbox with a fake `module` and no `document`, so its
// data-export block runs while its DOM render is skipped.
const source = readFileSync(new URL("../src/fidelity.js", import.meta.url), "utf8");
const sandbox = { exports: {} };
new Function("module", source)(sandbox);
const { FIDELITY, FIDELITY_STAGE } = sandbox.exports;

const STAGES = ["modeled", "rendered", "editable", "roundtrips"];
const VALUES = new Set(["full", "partial", "placeholder", "preserved", "none"]);

test("every construct family in the expected set is present exactly once", () => {
  const expected = [
    "Paragraphs & text",
    "Character / run formatting",
    "Paragraph & named styles",
    "Tables",
    "Lists & numbering",
    "Images & inline drawings",
    "Text boxes & shapes",
    "Headers & footers",
    "Footnotes & endnotes",
    "Sections, columns & page setup",
    "Fields",
    "Math (OMML)",
    "Charts",
    "SmartArt",
    "VML pictures & shapes",
    "Comments",
    "Tracked changes",
    "Bookmarks & hyperlinks",
    "Content controls (w:sdt)",
  ];
  const actual = FIDELITY.map((row) => row.family);
  assert.deepEqual(actual, expected, "construct list drifted — update the page and this guard together");
});

test("every row is well-formed with a note and valid stage values", () => {
  for (const row of FIDELITY) {
    assert.ok(row.note && row.note.trim().length > 0, `${row.family} has a note`);
    for (const stage of STAGES) {
      assert.ok(VALUES.has(row[stage]), `${row.family}.${stage} is a known stage value (got ${row[stage]})`);
    }
    // Every stage value the page can render must have a glyph/label.
    for (const stage of STAGES) {
      assert.ok(FIDELITY_STAGE[row[stage]], `${row.family}.${stage} maps to a legend entry`);
    }
  }
});

test("load-bearing honesty invariants hold (do not overstate public support)", () => {
  const by = Object.fromEntries(FIDELITY.map((row) => [row.family, row]));
  // Images cannot be inserted or edited yet.
  assert.equal(by["Images & inline drawings"].editable, "none");
  // Headers/footers, text boxes, footnotes render but are not editing surfaces.
  assert.equal(by["Headers & footers"].editable, "none");
  assert.equal(by["Text boxes & shapes"].editable, "none");
  assert.equal(by["Footnotes & endnotes"].editable, "none");
  assert.equal(by["Fields"].editable, "none");
  // Math has a bounded typed/rendered subset, but remains partial and read-only.
  assert.equal(by["Math (OMML)"].modeled, "partial");
  assert.equal(by["Math (OMML)"].rendered, "partial");
  assert.equal(by["Math (OMML)"].editable, "none");
  // Charts / SmartArt are preserved, not rendered as charts/diagrams.
  assert.equal(by["Charts"].rendered, "preserved");
  assert.equal(by["SmartArt"].rendered, "preserved");
  // Nothing claims "full" editable for charts/smartart/math/images/headers.
  for (const family of ["Charts", "SmartArt", "Math (OMML)", "Images & inline drawings", "Headers & footers"]) {
    assert.notEqual(by[family].editable, "full", `${family} must not claim full editability`);
  }
});
