// The swatch vocabularies, checked against the engine rather than against a comment.
//
// `palettes.mjs` claims HIGHLIGHT_COLORS is "the complete set of OOXML
// `w:highlight` named colors the engine accepts". That kind of claim is exactly
// what SKILL.md §9 exists for: it was true when written and nothing made it stay
// true. A highlight the engine can write and the picker cannot offer is a
// capability no user can reach (§9 rule 4), and the reverse — a name in the
// picker the engine will reject — is a swatch that silently does nothing.
//
// So the expected set is DERIVED from `highlight_token` in
// `casual-doc-export/src/semantic.rs`, which is the exhaustive `match` over
// `HighlightColor` and therefore the engine's own enumeration.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { HIGHLIGHT_COLORS, HIGHLIGHT_LABEL, TEXT_STANDARD_COLORS, highlightHex } from "../src/palettes.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

/** The engine's own highlight names, from the exhaustive match that writes them.
 *  `none` is the absence of a highlight, not a swatch, so it is not expected in
 *  the picker. */
function engineHighlightNames() {
  const source = read("../../crates/casual-doc-export/src/semantic.rs");
  const fn = source.match(/fn highlight_token\([\s\S]*?\n}/);
  assert.ok(fn, "highlight_token must still exist in casual-doc-export/src/semantic.rs");
  const names = [...fn[0].matchAll(/=>\s*"([A-Za-z]+)"/g)].map((m) => m[1]);
  assert.ok(names.length > 5, "the match arms must have been read, not missed");
  return names.filter((n) => n !== "none");
}

test("the highlight picker offers exactly the highlights the engine can write", () => {
  const engine = engineHighlightNames().sort();
  const picker = HIGHLIGHT_COLORS.map((c) => c.name).sort();
  assert.deepEqual(
    picker,
    engine,
    "HIGHLIGHT_COLORS must match HighlightColor: a colour only the engine knows is unreachable, and one only the picker knows is a dead swatch",
  );
});

test("every highlight carries a swatch and a label a user can read", () => {
  for (const { name, hex, label } of HIGHLIGHT_COLORS) {
    assert.match(hex, /^#[0-9a-f]{6}$/, `${name} needs a 6-digit lowercase hex swatch`);
    assert.equal(highlightHex(name), hex, `highlightHex(${name}) must answer its own swatch`);
    assert.equal(HIGHLIGHT_LABEL.get(name), label, `${name} must be labelled`);
    assert.ok(label.trim().length > 0 && label !== name, `${name} needs a human label, not its OOXML token`);
  }
  const names = HIGHLIGHT_COLORS.map((c) => c.name);
  assert.equal(new Set(names).size, names.length, "a duplicated name would shadow a swatch");
  const labels = HIGHLIGHT_COLORS.map((c) => c.label);
  assert.equal(new Set(labels).size, labels.length, "two swatches with one label are indistinguishable to a reader");
});

test("the absence of a highlight is not an unknown highlight", () => {
  // `setHighlight` takes "none" to clear, and the caret reports "" before a
  // document is open. Both mean "paint no swatch", and so does a name the table
  // does not carry — the picker must not throw or paint black for any of them.
  assert.equal(highlightHex("none"), null);
  assert.equal(highlightHex(""), null);
  assert.equal(highlightHex(undefined), null);
  assert.equal(highlightHex("chartreuse"), null);
});

test("the standard text colors are a well-formed two-row palette", () => {
  // The picker lays these out as a grayscale row then a hue row (10 + 10). A
  // count that is not a multiple of the row width leaves a ragged grid.
  assert.equal(TEXT_STANDARD_COLORS.length, 20, "the palette is two rows of ten");
  for (const hex of TEXT_STANDARD_COLORS) {
    assert.match(hex, /^#[0-9a-f]{6}$/, `${hex} must be a 6-digit lowercase hex`);
  }
  assert.equal(
    new Set(TEXT_STANDARD_COLORS).size,
    TEXT_STANDARD_COLORS.length,
    "a repeated swatch wastes a cell and reads as a rendering bug",
  );
  const grayscale = TEXT_STANDARD_COLORS.slice(0, 10);
  for (const hex of grayscale) {
    const [r, g, b] = [1, 3, 5].map((i) => hex.slice(i, i + 2));
    assert.ok(r === g && g === b, `${hex} is in the grayscale row but is not gray`);
  }
});
