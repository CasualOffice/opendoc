// HF-025 / `105` UX-009 — shortcut labels must name keys the user's keyboard
// has.
//
// Two halves, deliberately:
//
//  1. The conversion and the sweep, as behaviour.
//  2. A SOURCE guard, because patching the two labels that were still wrong is
//     worth almost nothing next to failing the build the next time one is
//     added. A raw ⌘ is allowed to exist — that is how a shortcut is declared —
//     but only somewhere the runtime provably renders it through
//     `formatShortcut`/`localizeShortcutText`. Anywhere else is a label that
//     will reach a Windows or Linux user verbatim.
//
// The live half (does the sweep actually reach every surface) is
// `tests/e2e/shortcut-labels.spec.mjs`, which loads the editor with a Windows
// navigator and sweeps the rendered DOM.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

import { APPLE_PLATFORM, STANDARD_PLATFORM } from "../src/keyboard.mjs";
import {
  DOCUMENT_CONTENT_SELECTOR,
  LOCALIZED_ATTRIBUTES,
  SHORTCUT_GLYPHS,
  isShortcutLike,
  localizeShortcutGlyphs,
  localizeShortcutText,
} from "../src/shortcut_labels.mjs";

const SRC = new URL("../src/", import.meta.url);
const GLYPH = new RegExp(`[${SHORTCUT_GLYPHS}]`, "u");

function read(url) {
  return readFileSync(url, "utf8");
}

// --- the conversion ---------------------------------------------------------

test("a declared chord renders for the keyboard in front of the user", () => {
  assert.equal(localizeShortcutText("Undo (⌘Z)", STANDARD_PLATFORM), "Undo (Ctrl+Z)");
  assert.equal(
    localizeShortcutText("Keep text only (⌘⇧V)", STANDARD_PLATFORM),
    "Keep text only (Ctrl+Shift+V)",
  );
  assert.equal(localizeShortcutText("⌘⇧P", STANDARD_PLATFORM), "Ctrl+Shift+P");
  assert.equal(
    localizeShortcutText("Decrease indent (⇧Tab)", STANDARD_PLATFORM),
    "Decrease indent (Shift+Tab)",
  );
  assert.equal(
    localizeShortcutText("Accept change and move to next (⌘⌥⏎)", STANDARD_PLATFORM),
    "Accept change and move to next (Ctrl+Alt+Enter)",
  );
});

test("Apple keeps its glyphs and a label with no chord is untouched", () => {
  assert.equal(localizeShortcutText("Undo (⌘Z)", APPLE_PLATFORM), "Undo (⌘Z)");
  assert.equal(
    localizeShortcutText("Insert table (3×3)", STANDARD_PLATFORM),
    "Insert table (3×3)",
  );
  assert.equal(localizeShortcutText("", STANDARD_PLATFORM), "");
});

test("a localized chord is still recognized as a chord", () => {
  // The ribbon tooltip decides whether a title's parenthetical is a shortcut or
  // part of the name. A glyph-only test says no to "Ctrl+B" — which is every
  // label on the platform this work exists for.
  assert.equal(isShortcutLike("⌘B"), true);
  assert.equal(isShortcutLike("Ctrl+B"), true);
  assert.equal(isShortcutLike("Shift+Tab"), true);
  assert.equal(isShortcutLike("3×3"), false);
  assert.equal(isShortcutLike("compact view"), false);
});

// --- the sweep --------------------------------------------------------------
//
// `localizeShortcutGlyphs` touches only `nodeType`, `data`, `childNodes`,
// `getAttribute`, `setAttribute` and `matches`, so a handful of plain objects
// is a complete stand-in for a DOM — and keeps this test runnable under plain
// `node --test`.

function textNode(data) {
  return { nodeType: 3, data };
}

function element(tag, attributes = {}, children = [], selectorNames = []) {
  const attrs = new Map(Object.entries(attributes));
  return {
    nodeType: 1,
    tag,
    attrs,
    childNodes: children,
    getAttribute: (name) => (attrs.has(name) ? attrs.get(name) : null),
    setAttribute: (name, value) => attrs.set(name, value),
    matches: (selector) =>
      selectorNames.some((name) => selector.split(",").some((part) => part.trim() === name)),
  };
}

test("the sweep rewrites chrome labels and reports how many it changed", () => {
  const button = element("button", { title: "Bold (⌘B)", "aria-label": "Bold" });
  const chip = element("span", {}, [textNode("⌘⇧P")]);
  const root = element("div", {}, [button, chip]);

  const changed = localizeShortcutGlyphs(root, STANDARD_PLATFORM);

  assert.equal(changed, 2);
  assert.equal(button.getAttribute("title"), "Bold (Ctrl+B)");
  assert.equal(chip.childNodes[0].data, "Ctrl+Shift+P");
});

test("the sweep never rewrites document content", () => {
  // A ⌘ in a comment or a paragraph is the USER's text. Rewriting it would be
  // corrupting the document to fix our chrome.
  const comment = element("p", {}, [textNode("Remember: ⌘Z undoes this")], [
    "#reviewSidebarBody",
  ]);
  const root = element("div", {}, [comment]);

  const changed = localizeShortcutGlyphs(root, STANDARD_PLATFORM);

  assert.equal(changed, 0);
  assert.equal(comment.childNodes[0].data, "Remember: ⌘Z undoes this");
  assert.ok(DOCUMENT_CONTENT_SELECTOR.includes("#reviewSidebarBody"));
});

test("the sweep is a no-op on Apple", () => {
  const button = element("button", { title: "Bold (⌘B)" });
  assert.equal(localizeShortcutGlyphs(element("div", {}, [button]), APPLE_PLATFORM), 0);
  assert.equal(button.getAttribute("title"), "Bold (⌘B)");
});

// --- the class guard --------------------------------------------------------

/** Ranges of `source` that are HTML comments — prose, never rendered. */
function htmlCommentRanges(source) {
  const ranges = [];
  const pattern = /<!--[\s\S]*?-->/g;
  let match;
  while ((match = pattern.exec(source))) ranges.push([match.index, pattern.lastIndex]);
  return ranges;
}

/** The attribute a character offset sits inside, or null when it is text. */
function attributeAt(source, index) {
  const before = source.slice(0, index);
  const open = before.lastIndexOf("<");
  const close = before.lastIndexOf(">");
  if (open < close) return null; // between tags: element text
  const tag = source.slice(open, index);
  const quotes = (tag.match(/"/g) || []).length;
  if (quotes % 2 === 0) return null; // not inside a quoted value
  const attr = tag.match(/([\w:-]+)\s*=\s*"[^"]*$/);
  return attr ? attr[1] : "(unnamed attribute)";
}

test("every shortcut glyph in editor.html sits where the sweep will reach it", () => {
  const source = read(new URL("../editor.html", import.meta.url));
  const comments = htmlCommentRanges(source);
  const offenders = [];
  const pattern = new RegExp(`[${SHORTCUT_GLYPHS}]`, "gu");
  let match;
  while ((match = pattern.exec(source))) {
    const at = match.index;
    if (comments.some(([start, end]) => at >= start && at < end)) continue;
    const attribute = attributeAt(source, at);
    if (attribute === null) continue; // element text — swept
    if (LOCALIZED_ATTRIBUTES.includes(attribute)) continue;
    const line = source.slice(0, at).split("\n").length;
    offenders.push(`editor.html:${line} — "${match[0]}" in ${attribute}=`);
  }
  assert.deepEqual(
    offenders,
    [],
    "a shortcut glyph in an attribute the boot sweep does not localize reaches a " +
      "Windows/Linux user verbatim (HF-025). Either use one of " +
      `${LOCALIZED_ATTRIBUTES.join(", ")}, or add the attribute to LOCALIZED_ATTRIBUTES.`,
  );
});

/** Source with comment text removed, so prose about ⌘Z is not read as a label.
 *  Line breaks inside a block comment are kept, so a reported line number is
 *  the line the reader will find in the file. */
function code(text) {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, (block) => block.replace(/[^\n]/g, " "))
    .replace(/(^|[^:])\/\/.*$/gm, "$1");
}

// The sinks that put a string in front of the user. A line that writes a glyph
// into one of these is a label, and must be localized where it is assigned —
// the boot sweep runs once and cannot see an assignment made later.
const LABEL_SINK =
  /\.(?:title|textContent|innerText|innerHTML|placeholder|ariaLabel)\s*=|setAttribute\(\s*["'](?:title|aria-label|aria-keyshortcuts|placeholder|alt)["']|insertAdjacentHTML\(/;
const LOCALIZER = /localizeShortcutText\(|formatShortcut\(/;

test("no script writes a raw Apple glyph into a user-visible label", () => {
  // `keyboard.mjs` owns the glyph→name table and `shortcut_labels.mjs` the
  // conversion; both necessarily contain glyphs.
  const exempt = new Set(["keyboard.mjs", "shortcut_labels.mjs"]);
  const offenders = [];
  for (const name of readdirSync(SRC).filter((f) => /\.(mjs|js)$/.test(f))) {
    if (exempt.has(name)) continue;
    const lines = code(read(new URL(name, SRC))).split("\n");
    lines.forEach((line, i) => {
      if (!GLYPH.test(line)) return;
      if (!LABEL_SINK.test(line)) return; // a declaration, rendered on read
      if (LOCALIZER.test(line)) return;
      offenders.push(`${name}:${i + 1} — ${line.trim()}`);
    });
  }
  assert.deepEqual(
    offenders,
    [],
    "this assigns an Apple glyph straight to a label. Wrap it in " +
      "localizeShortcutText(...) so it renders as Ctrl/Alt/Shift off Apple (HF-025).",
  );
});

test("the boot sweep is actually wired up", () => {
  // Evidence rule: a guard that only checks the source of a helper proves the
  // helper exists, not that anything calls it. The e2e spec proves the effect;
  // this catches the call being deleted with a one-line diff.
  const main = read(new URL("main.js", SRC));
  assert.match(main, /localizeShortcutGlyphs\(document\.body, EDITOR_KEYBOARD_PLATFORM\)/);
});
