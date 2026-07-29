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
