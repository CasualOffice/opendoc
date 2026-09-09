import assert from "node:assert/strict";
import test from "node:test";

import {
  APPLE_PLATFORM,
  STANDARD_PLATFORM,
  formatShortcut,
  NAVIGATION_SHORTCUTS,
  keyboardPlatform,
  lineDeletionDirection,
  navigationDirection,
  navigationShortcuts,
  wordDeletionDirection,
} from "../src/keyboard.mjs";

const key = (value, modifiers = {}) => ({
  key: value,
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  shiftKey: false,
  ...modifiers,
});

test("platform detection recognizes Apple browser hints without classifying Windows as Apple", () => {
  assert.equal(
    keyboardPlatform({ userAgentData: { platform: "macOS" } }),
    APPLE_PLATFORM,
  );
  assert.equal(keyboardPlatform({ platform: "MacIntel" }), APPLE_PLATFORM);
  assert.equal(keyboardPlatform("Mozilla/5.0 (iPad)"), APPLE_PLATFORM);
  assert.equal(keyboardPlatform({ platform: "Win32" }), STANDARD_PLATFORM);
  assert.equal(
    keyboardPlatform({ platform: "Linux x86_64" }),
    STANDARD_PLATFORM,
  );
});

test("macOS maps Option by word and Command by line, paragraph, and document", () => {
  assert.equal(
    navigationDirection(key("ArrowLeft", { altKey: true }), APPLE_PLATFORM),
    "wordLeft",
  );
  assert.equal(
    navigationDirection(key("ArrowRight", { metaKey: true }), APPLE_PLATFORM),
    "lineEnd",
  );
  assert.equal(
    navigationDirection(key("ArrowUp", { metaKey: true }), APPLE_PLATFORM),
    "paragraphUp",
  );
  assert.equal(
    navigationDirection(key("ArrowDown", { metaKey: true }), APPLE_PLATFORM),
    "paragraphDown",
  );
  assert.equal(
    navigationDirection(key("Home", { metaKey: true }), APPLE_PLATFORM),
    "docStart",
  );
  assert.equal(
    navigationDirection(key("End", { metaKey: true }), APPLE_PLATFORM),
    "docEnd",
  );
  assert.equal(
    navigationDirection(key("ArrowLeft", { ctrlKey: true }), APPLE_PLATFORM),
    null,
  );
});

test("Windows and Linux map Ctrl by word, paragraph, and document", () => {
  assert.equal(
    navigationDirection(key("ArrowLeft", { ctrlKey: true }), STANDARD_PLATFORM),
    "wordLeft",
  );
  assert.equal(
    navigationDirection(
      key("ArrowRight", { ctrlKey: true }),
      STANDARD_PLATFORM,
    ),
    "wordRight",
  );
  assert.equal(
    navigationDirection(key("ArrowUp", { ctrlKey: true }), STANDARD_PLATFORM),
    "paragraphUp",
  );
  assert.equal(
    navigationDirection(key("ArrowDown", { ctrlKey: true }), STANDARD_PLATFORM),
    "paragraphDown",
  );
  assert.equal(
    navigationDirection(key("Home", { ctrlKey: true }), STANDARD_PLATFORM),
    "docStart",
  );
  assert.equal(
    navigationDirection(key("End", { ctrlKey: true }), STANDARD_PLATFORM),
    "docEnd",
  );
  assert.equal(
    navigationDirection(key("ArrowLeft", { altKey: true }), STANDARD_PLATFORM),
    null,
  );
});

test("plain line/page movement and Shift extension share both platform maps", () => {
  for (const platform of [APPLE_PLATFORM, STANDARD_PLATFORM]) {
    assert.equal(navigationDirection(key("ArrowLeft"), platform), "left");
    assert.equal(navigationDirection(key("Home"), platform), "lineStart");
    assert.equal(
      navigationDirection(key("End", { shiftKey: true }), platform),
      "lineEnd",
    );
    assert.equal(navigationDirection(key("PageUp"), platform), "pageUp");
    assert.equal(
      navigationDirection(key("PageDown", { shiftKey: true }), platform),
      "pageDown",
    );
    assert.equal(
      navigationDirection(key("PageDown", { ctrlKey: true }), platform),
      null,
    );
  }
});

test("word deletion uses Option on macOS and Ctrl elsewhere", () => {
  assert.equal(
    wordDeletionDirection(key("Backspace", { altKey: true }), APPLE_PLATFORM),
    "backward",
  );
  assert.equal(
    wordDeletionDirection(key("Delete", { altKey: true }), APPLE_PLATFORM),
    "forward",
  );
  assert.equal(
    wordDeletionDirection(key("Backspace", { ctrlKey: true }), APPLE_PLATFORM),
    null,
  );
  assert.equal(
    wordDeletionDirection(
      key("Backspace", { ctrlKey: true }),
      STANDARD_PLATFORM,
    ),
    "backward",
  );
  assert.equal(
    wordDeletionDirection(key("Delete", { ctrlKey: true }), STANDARD_PLATFORM),
    "forward",
  );
  assert.equal(
    wordDeletionDirection(key("Delete", { altKey: true }), STANDARD_PLATFORM),
    null,
  );
});

test("line deletion is a macOS ⌘ chord and never fires elsewhere", () => {
  // macOS: ⌘Backspace clears to line start, ⌘Delete to line end.
  assert.equal(
    lineDeletionDirection(key("Backspace", { metaKey: true }), APPLE_PLATFORM),
    "backward",
  );
  assert.equal(
    lineDeletionDirection(key("Delete", { metaKey: true }), APPLE_PLATFORM),
    "forward",
  );
  // Option (word delete) and plain Backspace must not be treated as line delete.
  assert.equal(
    lineDeletionDirection(key("Backspace", { altKey: true }), APPLE_PLATFORM),
    null,
  );
  assert.equal(
    lineDeletionDirection(key("Backspace"), APPLE_PLATFORM),
    null,
  );
  // ⌘ combined with another deletion modifier is not a plain line delete.
  assert.equal(
    lineDeletionDirection(
      key("Backspace", { metaKey: true, altKey: true }),
      APPLE_PLATFORM,
    ),
    null,
  );
  // Windows/Linux have no ⌘-key line delete.
  assert.equal(
    lineDeletionDirection(key("Backspace", { metaKey: true }), STANDARD_PLATFORM),
    null,
  );
  assert.equal(
    lineDeletionDirection(key("Delete", { metaKey: true }), STANDARD_PLATFORM),
    null,
  );
});

test("Apple keyboards keep the glyphs the shortcuts are declared in", () => {
  assert.equal(formatShortcut("⌘⇧P", APPLE_PLATFORM), "⌘⇧P");
  assert.equal(formatShortcut("⌘⌥⏎", APPLE_PLATFORM), "⌘⌥⏎");
});

test("every other keyboard gets key names it actually has", () => {
  assert.equal(formatShortcut("⌘S", STANDARD_PLATFORM), "Ctrl+S");
  assert.equal(formatShortcut("⌘⇧P", STANDARD_PLATFORM), "Ctrl+Shift+P");
  assert.equal(formatShortcut("⌘⌥⏎", STANDARD_PLATFORM), "Ctrl+Alt+Enter");
  assert.equal(formatShortcut("⌘⌥M", STANDARD_PLATFORM), "Ctrl+Alt+M");
});

test("a shortcut with no glyphs survives both platforms unchanged", () => {
  assert.equal(formatShortcut("F5", STANDARD_PLATFORM), "F5");
  assert.equal(formatShortcut("F5", APPLE_PLATFORM), "F5");
  assert.equal(formatShortcut("", STANDARD_PLATFORM), "");
  assert.equal(formatShortcut(undefined, STANDARD_PLATFORM), "");
});

test("every shortcut the editor declares renders without a leftover glyph", () => {
  const declared = [
    "⌘A", "⌘B", "⌘C", "⌘F", "⌘I", "⌘K", "⌘P", "⌘S", "⌘U", "⌘V", "⌘X", "⌘Z",
    "⌘⇧C", "⌘⇧P", "⌘⇧V", "⌘⇧Z", "⌘⌥M", "⌘⌥⏎",
  ];
  for (const shortcut of declared) {
    const rendered = formatShortcut(shortcut, STANDARD_PLATFORM);
    assert.ok(
      !/[⌘⌃⌥⇧⏎⌫⌦⎋⇥]/u.test(rendered),
      `${shortcut} still shows an Apple glyph as ${rendered}`,
    );
  }
});

test("every caret-movement row in the reference is a chord the editor really handles", () => {
  const blank = { altKey: false, ctrlKey: false, metaKey: false, shiftKey: false };
  for (const row of NAVIGATION_SHORTCUTS) {
    const [first, last] = row.direction;
    assert.equal(
      navigationDirection({ ...blank, ...row.event }, APPLE_PLATFORM),
      first,
      `${row.label} (Apple, first key)`,
    );
    assert.equal(
      navigationDirection({ ...blank, ...row.second }, APPLE_PLATFORM),
      last,
      `${row.label} (Apple, second key)`,
    );
    assert.equal(
      navigationDirection({ ...blank, ...(row.standardEvent ?? row.event) }, STANDARD_PLATFORM),
      first,
      `${row.label} (standard, first key)`,
    );
    assert.equal(
      navigationDirection({ ...blank, ...(row.standardSecond ?? row.second) }, STANDARD_PLATFORM),
      last,
      `${row.label} (standard, second key)`,
    );
  }
});

test("the reference renders the platform's own modifier, not the other one's", () => {
  const apple = navigationShortcuts(APPLE_PLATFORM);
  const standard = navigationShortcuts(STANDARD_PLATFORM);
  assert.equal(apple.length, NAVIGATION_SHORTCUTS.length);
  assert.equal(standard.length, NAVIGATION_SHORTCUTS.length);
  const word = (rows) => rows.find((r) => r.label === "Move by word").keys;
  assert.match(word(apple), /⌥/);
  assert.match(word(standard), /Ctrl/);
  for (const row of standard) assert.ok(!/[⌘⌥⇧⌃]/u.test(row.keys), row.keys);
});
