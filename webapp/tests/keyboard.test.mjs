import assert from "node:assert/strict";
import test from "node:test";

import {
  APPLE_PLATFORM,
  STANDARD_PLATFORM,
  keyboardPlatform,
  navigationDirection,
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
