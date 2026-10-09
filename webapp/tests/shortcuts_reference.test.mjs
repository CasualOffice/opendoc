// The keyboard-shortcut reference's grouping, including the one shape it gained
// with the heading chords: a FAMILY of commands listed as one row.
//
// Google Docs lists "Apply heading style [1-6] · Ctrl+Alt+[1-6]" as one row, and
// Word lists F6 / Shift+F6 as one "pane" row. Listed one per command, the seven
// chords the heading and region work added pushed the reference 19px past the
// 1280x720 laptop fold `dialog-fit.spec.mjs` holds it to; collapsed, it fits.
import { test } from "node:test";
import assert from "node:assert/strict";

import { joinChords, shortcutGroups } from "../src/shortcuts_reference.mjs";
import { formatShortcut, STANDARD_PLATFORM } from "../src/keyboard.mjs";

const format = (chord) => formatShortcut(chord, STANDARD_PLATFORM);

test("a family of commands is one row, and its chords read as a range or a list", () => {
  const headings = { id: "headings", label: "Apply Heading 1–3" };
  const regions = { id: "regions", label: "Move between regions" };
  const groups = shortcutGroups(
    [
      { id: "h1", label: "Apply Heading 1", group: "Paragraph", shortcut: "⌘⌥1", referenceRow: headings },
      { id: "h2", label: "Apply Heading 2", group: "Paragraph", shortcut: "⌘⌥2", referenceRow: headings },
      { id: "h3", label: "Apply Heading 3", group: "Paragraph", shortcut: "⌘⌥3", referenceRow: headings },
      { id: "n", label: "Apply Normal style", group: "Paragraph", shortcut: "⌘⌥0" },
      { id: "next", label: "Next", group: "View", shortcut: "F6", referenceRow: regions },
      { id: "prev", label: "Previous", group: "View", shortcut: "⇧F6", referenceRow: regions },
    ],
    [],
    format,
  );
  assert.deepEqual(groups, [
    [
      "Paragraph",
      [
        { keys: "Ctrl+Alt+1–3", label: "Apply Heading 1–3" },
        { keys: "Ctrl+Alt+0", label: "Apply Normal style" },
      ],
    ],
    ["View", [{ keys: "F6 / Shift+F6", label: "Move between regions" }]],
  ]);
});

test("only a counting run of final digits becomes a range", () => {
  assert.equal(joinChords(["Ctrl+Alt+1", "Ctrl+Alt+2", "Ctrl+Alt+3"]), "Ctrl+Alt+1–3");
  assert.equal(joinChords(["⌘⌥1", "⌘⌥2"]), "⌘⌥1–2");
  // A gap is not a range: "1–3" would claim a chord for 2.
  assert.equal(joinChords(["Ctrl+Alt+1", "Ctrl+Alt+3"]), "Ctrl+Alt+1 / Ctrl+Alt+3");
  assert.equal(joinChords(["F6", "Shift+F6"]), "F6 / Shift+F6");
  assert.equal(joinChords(["Ctrl+H"]), "Ctrl+H");
});
