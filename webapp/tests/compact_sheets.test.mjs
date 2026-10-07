// The compact chrome's sheets, held to their declarations without a browser.
//
// `phone-command-surface.spec.mjs` and `chrome-raw-keys.spec.mjs` hold the
// behaviour; this holds the DATA those behaviours stand on, so a renamed ribbon
// id, a command moved out of the Format menu or a group named by an undeclared
// key fails here in milliseconds instead of as a silent plain row or a dotted
// key on a phone.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { COMPACT_TOOLBAR, PHONE_TOOLBAR, SHEET_STATE, checkedFrom } from "../src/compact_toolbar.mjs";
import { APP_MENU_SECTIONS, sectionCommandIds } from "../src/command_taxonomy.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { keysFromMarkup } from "../tools/build-locale.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const editor = readFileSync(join(WEBAPP, "editor.html"), "utf8");

test("every state-bearing row is a command the Format sheet actually renders", () => {
  // A SHEET_STATE entry for a command no sheet shows is dead data that reads
  // as coverage. The Aa sheet renders `APP_MENU_SECTIONS.format`.
  const format = new Set(sectionCommandIds(APP_MENU_SECTIONS.format));
  const stray = Object.keys(SHEET_STATE).filter((id) => !format.has(id));
  assert.deepEqual(stray, []);
});

test("every state source is a real element in the markup", () => {
  // The rows MIRROR the ribbon's own pressed state rather than recomputing it.
  // A source id that stopped existing would not throw — the row would simply
  // render plain, which is exactly the defect this map exists to fix.
  const missing = Object.entries(SHEET_STATE)
    .filter(([, state]) => !editor.includes(`id="${state.from}"`))
    .map(([id, state]) => `${id} -> #${state.from}`);
  assert.deepEqual(missing, []);
});

test("the toggle strip is Docs' row: the font band's character toggles, and nothing else", () => {
  const strip = Object.entries(SHEET_STATE)
    .filter(([, state]) => state.strip)
    .map(([id]) => id);
  assert.deepEqual(strip, [
    "format.bold",
    "format.italic",
    "format.underline",
    "format.strike",
    "format.superscript",
    "format.subscript",
  ]);
  // One of four is a radio; the strip is checkboxes only.
  for (const id of strip) assert.notEqual(SHEET_STATE[id].radio, true, id);
});

test("aria-pressed maps onto aria-checked, and 'mixed' only where the role allows it", () => {
  assert.equal(checkedFrom("true"), "true");
  assert.equal(checkedFrom("false"), "false");
  assert.equal(checkedFrom(null), "false");
  assert.equal(checkedFrom(undefined), "false");
  // A half-bold selection is neither on nor off, and saying either would lie.
  assert.equal(checkedFrom("mixed"), "mixed");
  // `menuitemradio` has no mixed state in WAI-ARIA.
  assert.equal(checkedFrom("mixed", { radio: true }), "false");
  assert.equal(checkedFrom("true", { radio: true }), "true");
});

test("every key the bar names a control with is declared where a fallback can find it", () => {
  // The class behind the phone's "appMenuBar.format": a script-built control
  // named by a key that only the FETCHED catalogue carries. `nameFromKey` falls
  // back to the English the markup authored beside the key, so a key must be
  // either compiled in (`EN_STRINGS`) or declared by the markup (`data-i18n*`)
  // — a key that is neither has no English anywhere before its catalogue lands,
  // and would be read aloud as itself.
  const markup = keysFromMarkup(editor);
  const keys = new Set();
  for (const group of [...COMPACT_TOOLBAR, ...PHONE_TOOLBAR]) {
    if (group.labelKey) keys.add(group.labelKey);
    for (const item of group.items) {
      if (item.labelKey) keys.add(item.labelKey);
      if (item.adoptLabelKey) keys.add(item.adoptLabelKey);
      for (const section of item.sections ?? []) keys.add(section.nameKey);
    }
  }
  assert.ok(keys.size > 5, "the sweep found the bar's keys");
  const undeclared = [...keys].filter((key) => !(key in EN_STRINGS) && !markup.has(key));
  assert.deepEqual(undeclared, []);
  // And the markup-only ones are exactly the ones that need the fallback: if
  // this set were empty, the fallback would be untested by construction.
  const markupOnly = [...keys].filter((key) => !(key in EN_STRINGS)).sort();
  assert.deepEqual(markupOnly, ["appMenuBar.format", "appMenuBar.insert", "appMenuBar.table"]);
});
