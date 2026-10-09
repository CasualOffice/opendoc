// The deck viewer's share of the editor's shell (`src/slides_chrome.mjs`).
//
// What is guarded here is that the slide page and the document editor stay ONE
// application to a reader: the same tab names, the same toolbar-mode preference,
// the same findings dialog reading the same report shape, and every ribbon
// control resolving to a command the page's registry actually has. The DOM
// behaviour — the switch, the ribbon, the fit, the zoom control — is driven in a
// real browser by `e2e/slides-viewer.spec.mjs`.

import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  CHROME_MODE_PREF,
  SLIDE_RIBBON,
  deckReportJson,
  groupCommandIds,
  ribbonTitle,
} from "../src/slides_chrome.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";
import { createSlideCommands } from "../src/slide_commands.mjs";
import { findingKind, findingTotals, groupFindings } from "../src/compat_findings.mjs";
import { keysFromMarkup } from "../tools/build-locale.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

/** The registry as the page builds it, over a viewer with a deck open. */
function registry() {
  const viewer = {
    slideCount: () => 3,
    currentIndex: () => 1,
    currentFit: () => "slide",
    canZoom: () => true,
    zoomFactor: () => 1,
  };
  const actions = {
    openPicker() {},
    save() {},
    repaint() {},
    theme: () => "system",
    setTheme() {},
    panelShown: () => false,
    togglePanel() {},
  };
  return createSlideCommands({ viewer, actions, t: (key) => key })();
}

test("every ribbon control names a command the page's registry has", () => {
  // The ribbon is the third surface over one registry (the compact bar and the
  // menus are the others). A row naming an id the registry lacks would render a
  // control that runs nothing — the dead chrome `docs/63` forbids.
  const ids = new Set(registry().map((command) => command.id));
  const missing = SLIDE_RIBBON.flatMap((tab) =>
    tab.groups.flatMap((group) => groupCommandIds(group).filter((id) => !ids.has(id))),
  );
  assert.deepEqual(missing, []);
});

test("the ribbon is laid out in the editor's band shapes, not one row of icons", () => {
  // The editor's ribbon is MULTI-ROW: a large captioned button for a group's
  // primary verb (Paste), small buttons stacked beside it (Cut / Copy), and
  // two-line bands (Font, Paragraph). A deck ribbon that is a single strip of
  // icons is a different application, which is what the owner rejected.
  const parts = SLIDE_RIBBON.flatMap((tab) => tab.groups.flatMap((group) => group.parts));
  const kinds = new Set(parts.map((part) => (part.big ? "big" : part.stack ? "stack" : "rows")));
  assert.deepEqual([...kinds].sort(), ["big", "rows", "stack"]);
  // Every tab leads with a captioned primary action, as the editor's do.
  for (const tab of SLIDE_RIBBON) {
    assert.ok(
      tab.groups[0].parts.some((part) => part.big),
      `the ${tab.tab} tab's first group has a large captioned button`,
    );
  }
  // And every caption is a routed, script-side string.
  for (const part of parts.filter((part) => part.big)) {
    assert.ok(part.labelKey in EN_STRINGS, `${part.labelKey} is a script-side key`);
  }
});

test("the ribbon's tabs are the editor's own tabs, under the editor's keys", () => {
  // Same word in every language on both pages: the captions are the editor's
  // catalogue keys, declared in the editor's markup, not new ones.
  const editor = new Map(keysFromMarkup(readFileSync(join(WEBAPP, "editor.html"), "utf8")));
  for (const tab of SLIDE_RIBBON) {
    assert.ok(editor.has(tab.labelKey), `${tab.labelKey} is an editor tab key`);
  }
  assert.deepEqual(
    SLIDE_RIBBON.map((tab) => tab.tab),
    ["file", "home", "view"],
    "File first, as in the editor",
  );
});

test("every chrome string the deck viewer borrows reads exactly as the editor's", () => {
  // `build-locale.mjs` merges the two pages' markup keys into ONE catalogue, last
  // page wins. A borrowed key with different English would silently rewrite the
  // editor's own string.
  const editor = new Map(keysFromMarkup(readFileSync(join(WEBAPP, "editor.html"), "utf8")));
  const slides = new Map(keysFromMarkup(readFileSync(join(WEBAPP, "slides.html"), "utf8")));
  const shared = [...slides.keys()].filter((key) => editor.has(key));
  assert.ok(shared.length >= 15, `the viewer borrows the editor's chrome strings: ${shared}`);
  const differing = shared.filter((key) => editor.get(key) !== slides.get(key));
  assert.deepEqual(differing, []);
});

test("the toolbar-mode switch is the editor's preference, not a second one", () => {
  // A reader who chose Compact in a document gets Compact in a deck. Read from
  // the editor's source rather than restated, so the two cannot drift.
  const main = readFileSync(join(WEBAPP, "src/main.js"), "utf8");
  const match = main.match(/const CHROME_MODE_PREF = "([^"]+)";/);
  assert.ok(match, "main.js declares its toolbar-mode preference");
  assert.equal(CHROME_MODE_PREF, match[1]);
});

test("a deck's findings open in the editor's dialog, grouped as a document's are", () => {
  // The deck engine's own spellings, exactly as `casual-pres-wasm` writes them:
  // hyphenated, flat part/element/attribute.
  const report = deckReportJson([
    {
      feature: "a:sym",
      occurrences: 3,
      part: "ppt/slides/slide2.xml",
      element: "sym",
      attribute: null,
      disposition: "omitted-not-retained",
      modelOutcome: "omitted",
      retentionOutcome: "not-retained",
    },
    {
      feature: "a:scene3d",
      occurrences: 1,
      part: "ppt/slides/slide4.xml",
      element: "scene3d",
      attribute: null,
      disposition: "omitted-preserved",
      modelOutcome: "omitted",
      retentionOutcome: "preserved",
    },
  ]);
  const parsed = JSON.parse(report);
  assert.equal(parsed.entries.length, 2);
  // The editor's location shape, with the names its findings panel reads.
  assert.deepEqual(parsed.entries[0].location, {
    partName: "ppt/slides/slide2.xml",
    namespace: null,
    localName: "sym",
    attributeName: null,
  });
  // The dialog's own classifier, on the restated entries: a not-retained loss is
  // LOST, a preserved omission is KEPT. Before the spellings were restated the
  // first fell through to "other".
  assert.equal(findingKind(parsed.entries[0]), "lost");
  assert.equal(findingKind(parsed.entries[1]), "preserved");
  // And the chip's count and the dialog's grouping both accept the report.
  assert.equal(findingTotals(report).entries, 2);
  assert.ok(groupFindings(report).length > 0);
});

test("a disabled ribbon control says why, as the compact bar's does", () => {
  assert.equal(
    ribbonTitle({ label: "Save a copy", enabled: false, disabledReason: "Open a presentation first" }),
    "Open a presentation first",
  );
  assert.equal(
    ribbonTitle({ label: "Next slide", enabled: true, shortcut: "→" }, (text) => `[${text}]`),
    "Next slide ([→])",
  );
});
