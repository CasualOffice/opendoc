import test from "node:test";
import assert from "node:assert/strict";
import {
  clampContextMenuPosition,
  enabledMenuIndexes,
  moveMenuIndex,
  normalizeMenuEntries,
} from "../src/context_menu.mjs";

test("context menu groups commands without leading or duplicate separators", () => {
  const entries = normalizeMenuEntries([
    { id: "copy", group: "clipboard" },
    { id: "cut", group: "clipboard", enabled: false },
    { id: "hidden", group: "insert", visible: false },
    { id: "link", group: "insert" },
    { id: "comment", group: "review" },
  ]);
  assert.deepEqual(
    entries.map((entry) => entry.separator ? "|" : entry.id),
    ["copy", "cut", "|", "link", "|", "comment"],
  );
  assert.deepEqual(enabledMenuIndexes(entries), [0, 3, 5]);
});

test("menu keyboard movement wraps and skips separators and disabled items", () => {
  const entries = [
    { id: "copy" },
    { id: "cut", enabled: false },
    { separator: true },
    { id: "paste" },
  ];
  assert.equal(moveMenuIndex(entries, -1, 1), 0);
  assert.equal(moveMenuIndex(entries, 0, 1), 3);
  assert.equal(moveMenuIndex(entries, 3, 1), 0);
  assert.equal(moveMenuIndex(entries, 0, -1), 3);
  assert.equal(moveMenuIndex(entries, 0, "last"), 3);
});

test("context menu placement stays inside every viewport edge", () => {
  assert.deepEqual(
    clampContextMenuPosition(790, 590, 220, 260, 800, 600),
    { left: 572, top: 332 },
  );
  assert.deepEqual(
    clampContextMenuPosition(-20, -10, 220, 260, 800, 600),
    { left: 8, top: 8 },
  );
});
