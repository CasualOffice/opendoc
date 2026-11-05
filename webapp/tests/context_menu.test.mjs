import test from "node:test";
import assert from "node:assert/strict";
import {
  CONTEXT_MENU_BUDGET,
  clampContextMenuPosition,
  countMenuRows,
  enabledMenuIndexes,
  foldContextMenu,
  foldsForRowBudget,
  moveMenuIndex,
  normalizeMenuEntries,
  pruneContextMenu,
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

// ---- The short-menu policy (docs/184 D-12, audit CANVAS-07) -------------------

test("a context menu never carries Undo/Redo or a refused top-level row", () => {
  const kept = pruneContextMenu([
    { id: "edit.undo", group: "history" },
    { id: "edit.redo", group: "history", enabled: false },
    { id: "edit.cut", group: "clipboard", enabled: false, disabledReason: "Select content to cut" },
    { id: "edit.paste", group: "clipboard" },
    // A submenu keeps its greyed rows: there they are one of a family.
    { id: "table.insert", group: "table", submenu: [{ id: "a", enabled: false }, { id: "b" }] },
    // …but a submenu with NOTHING it can run is a refused row itself.
    { id: "table.look", group: "table", submenu: [{ id: "c", enabled: false, disabledReason: "no" }] },
  ]);
  assert.deepEqual(kept.map((row) => row.id), ["edit.paste", "table.insert"]);
});

test("rows hidden for one shared reason (a mode) leave that reason as a note", () => {
  const mode = "Turn on Editing to change this object";
  const kept = pruneContextMenu([
    { id: "object.altText", group: "a", enabled: false, disabledReason: mode },
    { id: "object.crop", group: "a", enabled: false, disabledReason: mode },
    { id: "object.properties", group: "a" },
  ]);
  assert.deepEqual(kept.map((row) => row.id), ["context.refusedNote", "object.properties"]);
  assert.equal(kept[0].note, true);
  assert.equal(kept[0].label, mode);
  // Row-specific preconditions (one reason each) are not a mode: no note.
  const quiet = pruneContextMenu([
    { id: "edit.cut", group: "c", enabled: false, disabledReason: "Select content to cut" },
    { id: "edit.copy", group: "c", enabled: false, disabledReason: "Select content to copy" },
    { id: "edit.paste", group: "c" },
  ]);
  assert.deepEqual(quiet.map((row) => row.id), ["edit.paste"]);
});

test("a note is not a keyboard stop", () => {
  const entries = [{ id: "n", note: true }, { id: "a" }, { id: "b" }];
  assert.deepEqual(enabledMenuIndexes(entries), [1, 2]);
  assert.equal(moveMenuIndex(entries, -1, 1), 1);
});

test("over the row budget, the trailing groups fold into one More submenu", () => {
  const rows = (group, n) => Array.from({ length: n }, (_, i) => ({ id: `${group}.${i}`, group }));
  const commands = [...rows("spelling", 5), ...rows("clipboard", 4), ...rows("annotate", 4), ...rows("format", 5)];
  assert.equal(countMenuRows(commands), 18);
  const folds = foldsForRowBudget(commands, CONTEXT_MENU_BUDGET.items);
  const folded = foldContextMenu(commands, folds, "More");
  assert.ok(countMenuRows(folded) <= CONTEXT_MENU_BUDGET.items);
  assert.equal(folded.at(-1).id, "context.more");
  assert.deepEqual(folded.at(-1).submenu.map((row) => row.id), commands.slice(13).map((row) => row.id));
  // The first group never folds: it is why the reader right-clicked.
  assert.deepEqual(foldContextMenu(commands, 99).slice(0, 5).map((row) => row.id), commands.slice(0, 5).map((row) => row.id));
  // Under budget, nothing moves.
  assert.equal(foldsForRowBudget(rows("a", 3)), 0);
});
