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
const source = readFileSync(
  new URL("../src/fidelity.js", import.meta.url),
  "utf8",
);
const sandbox = { exports: {} };
new Function("module", source)(sandbox);
const { FIDELITY, FIDELITY_STAGE, FORMAT_SUPPORT } = sandbox.exports;

const STAGES = ["modeled", "rendered", "editable", "roundtrips"];
const VALUES = new Set(["full", "partial", "placeholder", "preserved", "none"]);

test("format pipeline rows are complete and do not overstate ODT", () => {
  assert.deepEqual(
    FORMAT_SUPPORT.map((row) => row.format),
    ["DOCX", "Normalized JSON", "Plain text", "ODT"],
  );
  for (const row of FORMAT_SUPPORT) {
    assert.ok(
      row.note && row.note.trim().length > 0,
      `${row.format} has a note`,
    );
    for (const stage of ["validation", "import", "export", "host"]) {
      assert.ok(
        VALUES.has(row[stage]),
        `${row.format}.${stage} is a known stage value`,
      );
    }
  }
  const odt = FORMAT_SUPPORT.find((row) => row.format === "ODT");
  assert.equal(odt.validation, "full");
  assert.equal(odt.import, "partial");
  assert.equal(odt.export, "partial");
  assert.equal(odt.host, "partial");
  assert.match(odt.note, /nested bullet\/number lists/);
  assert.match(odt.note, /recursive tables/);
  assert.match(odt.note, /typed footnotes\/endnotes/);
  assert.match(odt.note, /authored citation labels/);
  assert.match(odt.note, /matching writer/);
  assert.match(odt.note, /unsafe merge geometry is visibly projected and reported/i);
  assert.match(odt.note, /advanced list continuation\/item overrides/);
  assert.equal(
    FORMAT_SUPPORT.find((row) => row.format === "Normalized JSON").host,
    "full",
  );
  assert.equal(
    FORMAT_SUPPORT.find((row) => row.format === "Plain text").host,
    "full",
  );
});

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
  assert.deepEqual(
    actual,
    expected,
    "construct list drifted — update the page and this guard together",
  );
});

test("every row is well-formed with a note and valid stage values", () => {
  for (const row of FIDELITY) {
    assert.ok(
      row.note && row.note.trim().length > 0,
      `${row.family} has a note`,
    );
    for (const stage of STAGES) {
      assert.ok(
        VALUES.has(row[stage]),
        `${row.family}.${stage} is a known stage value (got ${row[stage]})`,
      );
    }
    // Every stage value the page can render must have a glyph/label.
    for (const stage of STAGES) {
      assert.ok(
        FIDELITY_STAGE[row[stage]],
        `${row.family}.${stage} maps to a legend entry`,
      );
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
  // Common shape model (fill/gradient, outline/dash/arrows, rotation/flip, wrap
  // contour, preset geometry) is fully typed as of Layer 1; custGeom stays
  // retained-not-typed, so semantic-mode round-trip remains partial.
  assert.equal(by["Text boxes & shapes"].modeled, "full");
  assert.equal(by["Text boxes & shapes"].roundtrips, "partial");
  assert.equal(by["Footnotes & endnotes"].editable, "none");
  assert.equal(by["Fields"].editable, "none");
  // Math is fully typed as of Layer 1 (all 20 OMML math elements mapped or
  // raw-retained), but rendering is a bounded subset and it stays read-only.
  assert.equal(by["Math (OMML)"].modeled, "full");
  assert.equal(by["Math (OMML)"].rendered, "partial");
  assert.equal(by["Math (OMML)"].editable, "none");
  // Character rendering stays partial only until the wavy-underline squiggle
  // merges (it is currently drawn as a flat line); emphasis/outline/shadow
  // effects also stay unpainted. Paragraphs and Lists now render their common
  // surface in full — the remainders (docGrid/autospace on paragraphs, unknown
  // numFmt on lists) are niche/bounded, not common gaps.
  assert.equal(by["Character / run formatting"].rendered, "partial");
  // Numbering is fully modeled as of Layer 1 (overrides/indirection/restart/
  // numFmt vocabulary all typed + round-trip) and now renders in full.
  assert.equal(by["Lists & numbering"].modeled, "full");
  // Charts / SmartArt are preserved, not rendered as charts/diagrams.
  assert.equal(by["Charts"].rendered, "preserved");
  assert.equal(by["SmartArt"].rendered, "preserved");
  // Nothing claims "full" editable for charts/smartart/math/images/headers.
  for (const family of [
    "Charts",
    "SmartArt",
    "Math (OMML)",
    "Images & inline drawings",
    "Headers & footers",
  ]) {
    assert.notEqual(
      by[family].editable,
      "full",
      `${family} must not claim full editability`,
    );
  }
});
