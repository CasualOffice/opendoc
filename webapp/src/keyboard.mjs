export const APPLE_PLATFORM = "apple";
export const STANDARD_PLATFORM = "standard";

/** Normalizes browser platform hints into the two editor keymaps.
 *
 * `navigator.userAgentData.platform` is preferred when present; `platform` and
 * `userAgent` keep this deterministic in older browsers and in tests.
 */
export function keyboardPlatform(source = globalThis.navigator) {
  const hint =
    typeof source === "string"
      ? source
      : [source?.userAgentData?.platform, source?.platform, source?.userAgent]
          .filter(Boolean)
          .join(" ");
  return /\b(?:mac|iphone|ipad|ipod)/i.test(hint)
    ? APPLE_PLATFORM
    : STANDARD_PLATFORM;
}

function hasUnsupportedModifier(event, platform) {
  return platform === APPLE_PLATFORM
    ? event.ctrlKey
    : event.metaKey || event.altKey;
}

/** Maps a physical keyboard event to an engine navigation intent.
 *
 * Shift is deliberately ignored here: the caller applies it as selection
 * extension. Unsupported platform-modifier combinations return `null` so the
 * browser/OS keeps its native shortcut.
 */
export function navigationDirection(event, platform = keyboardPlatform()) {
  const apple = platform === APPLE_PLATFORM;
  switch (event.key) {
    case "ArrowLeft":
      if (hasUnsupportedModifier(event, platform)) return null;
      if (apple && event.metaKey) return "lineStart";
      if (apple && event.altKey) return "wordLeft";
      if (!apple && event.ctrlKey) return "wordLeft";
      return "left";
    case "ArrowRight":
      if (hasUnsupportedModifier(event, platform)) return null;
      if (apple && event.metaKey) return "lineEnd";
      if (apple && event.altKey) return "wordRight";
      if (!apple && event.ctrlKey) return "wordRight";
      return "right";
    case "ArrowUp":
      if (hasUnsupportedModifier(event, platform) || event.altKey) return null;
      if ((apple && event.metaKey) || (!apple && event.ctrlKey))
        return "paragraphUp";
      return "up";
    case "ArrowDown":
      if (hasUnsupportedModifier(event, platform) || event.altKey) return null;
      if ((apple && event.metaKey) || (!apple && event.ctrlKey))
        return "paragraphDown";
      return "down";
    case "Home":
      if (hasUnsupportedModifier(event, platform) || event.altKey) return null;
      if ((apple && event.metaKey) || (!apple && event.ctrlKey))
        return "docStart";
      return "lineStart";
    case "End":
      if (hasUnsupportedModifier(event, platform) || event.altKey) return null;
      if ((apple && event.metaKey) || (!apple && event.ctrlKey))
        return "docEnd";
      return "lineEnd";
    case "PageUp":
      return event.metaKey || event.ctrlKey || event.altKey ? null : "pageUp";
    case "PageDown":
      return event.metaKey || event.ctrlKey || event.altKey ? null : "pageDown";
    default:
      return null;
  }
}

/** Returns the semantic word-deletion direction for a collapsed caret. */
export function wordDeletionDirection(event, platform = keyboardPlatform()) {
  if (event.key !== "Backspace" && event.key !== "Delete") return null;
  const apple = platform === APPLE_PLATFORM;
  const wordModifier = apple
    ? event.altKey && !event.metaKey && !event.ctrlKey
    : event.ctrlKey && !event.metaKey && !event.altKey;
  if (!wordModifier) return null;
  return event.key === "Backspace" ? "backward" : "forward";
}

/** Returns the semantic line-deletion direction for a collapsed caret.
 *
 * This is a macOS-only convention: ⌘Backspace deletes from the caret to the
 * start of the line, and ⌘Delete (fn+Delete) to the end of the line. Windows
 * and Linux have no ⌘-key equivalent, so the standard keymap always returns
 * `null` and leaves ⌘/Ctrl combinations to the browser. Word deletion (Option
 * on macOS, Ctrl elsewhere) is handled separately by `wordDeletionDirection`.
 */
export function lineDeletionDirection(event, platform = keyboardPlatform()) {
  if (platform !== APPLE_PLATFORM) return null;
  if (event.key !== "Backspace" && event.key !== "Delete") return null;
  const lineModifier = event.metaKey && !event.altKey && !event.ctrlKey;
  if (!lineModifier) return null;
  return event.key === "Backspace" ? "backward" : "forward";
}

// Every shortcut in this editor is DECLARED in Apple glyphs — "⌘⇧P" — because
// that is what the ribbon tooltips and the palette were first written against.
// On Windows and Linux those glyphs name keys that are not on the keyboard, so
// the shortcut column was telling a large share of users to press a key they do
// not have.
const SHORTCUT_GLYPHS = new Map([
  ["⌘", "Ctrl"],
  ["⌃", "Ctrl"],
  ["⌥", "Alt"],
  ["⇧", "Shift"],
  ["⏎", "Enter"],
  ["⌫", "Backspace"],
  ["⌦", "Delete"],
  ["⎋", "Esc"],
  ["⇥", "Tab"],
]);

/** Renders a declared shortcut for the keyboard in front of the user.
 *
 * Apple keeps the glyphs, which is the platform convention and is what the
 * strings already are. Everywhere else each glyph becomes its key name joined by
 * "+", so "⌘⇧P" reads "Ctrl+Shift+P". A string with no glyphs at all is
 * returned untouched, so a plain "F5" survives both platforms.
 */
export function formatShortcut(shortcut, platform = keyboardPlatform()) {
  if (!shortcut) return "";
  if (platform === APPLE_PLATFORM) return shortcut;
  const parts = [];
  let literal = "";
  for (const character of shortcut) {
    const name = SHORTCUT_GLYPHS.get(character);
    if (name) {
      if (literal) {
        parts.push(literal);
        literal = "";
      }
      parts.push(name);
    } else {
      literal += character;
    }
  }
  if (literal) parts.push(literal);
  return parts.join("+");
}

// Caret movement is the one part of the keymap with no command behind it — it
// is handled straight out of `navigationDirection` — so it appears in no menu,
// no palette and, before this table existed, in no reference either. Declared
// here beside the function it describes, with the modifier genuinely differing
// per platform (Apple moves by word on Option, everyone else on Control), and
// `keyboard.test.mjs` drives each row through `navigationDirection` so a row
// that stops being true turns red instead of quietly lying in the dialog.
export const NAVIGATION_SHORTCUTS = [
  { apple: "←  →", standard: "←  →", label: "Move by character", direction: ["left", "right"], event: { key: "ArrowLeft" }, second: { key: "ArrowRight" } },
  { apple: "⌥←  ⌥→", standard: "Ctrl+←  Ctrl+→", label: "Move by word", direction: ["wordLeft", "wordRight"], event: { key: "ArrowLeft", altKey: true }, second: { key: "ArrowRight", altKey: true }, standardEvent: { key: "ArrowLeft", ctrlKey: true }, standardSecond: { key: "ArrowRight", ctrlKey: true } },
  { apple: "↑  ↓", standard: "↑  ↓", label: "Move by line", direction: ["up", "down"], event: { key: "ArrowUp" }, second: { key: "ArrowDown" } },
  { apple: "⌘↑  ⌘↓", standard: "Ctrl+↑  Ctrl+↓", label: "Move by paragraph", direction: ["paragraphUp", "paragraphDown"], event: { key: "ArrowUp", metaKey: true }, second: { key: "ArrowDown", metaKey: true }, standardEvent: { key: "ArrowUp", ctrlKey: true }, standardSecond: { key: "ArrowDown", ctrlKey: true } },
  { apple: "⌘←  ⌘→", standard: "Home  End", label: "Start or end of line", direction: ["lineStart", "lineEnd"], event: { key: "ArrowLeft", metaKey: true }, second: { key: "ArrowRight", metaKey: true }, standardEvent: { key: "Home" }, standardSecond: { key: "End" } },
  { apple: "⌘↖  ⌘↘", standard: "Ctrl+Home  Ctrl+End", label: "Start or end of document", direction: ["docStart", "docEnd"], event: { key: "Home", metaKey: true }, second: { key: "End", metaKey: true }, standardEvent: { key: "Home", ctrlKey: true }, standardSecond: { key: "End", ctrlKey: true } },
  { apple: "⇞  ⇟", standard: "PgUp  PgDn", label: "Move by screen", direction: ["pageUp", "pageDown"], event: { key: "PageUp" }, second: { key: "PageDown" } },
];

/** The caret-movement rows as the platform in front of the user sees them. */
export function navigationShortcuts(platform = keyboardPlatform()) {
  return NAVIGATION_SHORTCUTS.map((row) => ({
    keys: platform === APPLE_PLATFORM ? row.apple : row.standard,
    label: row.label,
  }));
}
