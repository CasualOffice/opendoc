// Platform-correct shortcut LABELS, everywhere a label can reach the user.
//
// `keyboard.mjs` already knows how to render one chord for one platform
// (`formatShortcut`), and the surfaces that build their hints in JS — the
// palette, the app menus, the shortcuts dialog, the ribbon tooltip — call it.
// That left the labels nobody builds: a `title=` written in `editor.html`, the
// palette's own `⌘⇧P` chip, a `.title =` assigned in JS. Those are still raw
// Apple glyphs on a Windows or Linux keyboard, which is the whole of HF-025 /
// `105` UX-009 — the shortcut works on Ctrl, only the label lies.
//
// So the rule this module encodes is: a shortcut is DECLARED in Apple notation
// (that is what the strings already are, and it is unambiguous), and it is
// RENDERED through here. `localizeShortcutGlyphs` sweeps a chrome subtree once
// at boot so a statically authored label needs no per-site change, and
// `localizeShortcutText` is the same conversion for a string being assigned.
//
// It deliberately does NOT observe the DOM. Document content can legitimately
// contain a ⌘ — a comment saying "press ⌘Z", a paragraph about Mac shortcuts —
// and rewriting that would be corrupting the user's text, not localising the
// chrome. `DOCUMENT_CONTENT_SELECTOR` names the subtrees that are content.
import { APPLE_PLATFORM, formatShortcut, keyboardPlatform } from "./keyboard.mjs";

/** The Apple modifier/key glyphs `formatShortcut` knows how to name. */
export const SHORTCUT_GLYPHS = "⌘⌃⌥⇧⏎⌫⌦⎋⇥";

const GLYPH = new RegExp(`[${SHORTCUT_GLYPHS}]`, "u");

// A chord inside a longer string: one or more glyphs plus the plain key that
// follows them ("⌘⇧V", "⇧Tab"), stopping at whitespace, a bracket, a slash or a
// "+" so "Undo (⌘Z)" localises the chord and leaves the sentence alone.
const CHORD = new RegExp(`[${SHORTCUT_GLYPHS}]+[^\\s()+/]*`, "gu");

// What a chord looks like AFTER localisation. Needed because a reader that
// recognises a shortcut only by its glyphs (the ribbon tooltip did) stops
// recognising it on the platform this module exists for.
const NAMED_CHORD = /(?:^|[^A-Za-z])(?:Ctrl|Alt|Shift|Cmd|Meta)\+/u;

/** Attributes whose value is shown to, or spoken to, the user.
 *
 *  `data-tip-title` is in the set because the ribbon tooltip parks the native
 *  `title` there while a control is hovered; a label that were localised in one
 *  and not the other would flip between renderings mid-hover. */
export const LOCALIZED_ATTRIBUTES = [
  "title",
  "aria-label",
  "aria-keyshortcuts",
  "placeholder",
  "alt",
  "data-tip-title",
];

/** Subtrees that hold DOCUMENT content rather than application chrome, and so
 *  must never be rewritten. */
export const DOCUMENT_CONTENT_SELECTOR =
  "#pages, #a11yDocument, #reviewSidebarBody, script, style, template";

/** True when `text` reads as a keyboard chord in either notation. */
export function isShortcutLike(text) {
  return !!text && (GLYPH.test(text) || NAMED_CHORD.test(text));
}

/** Renders every Apple chord inside `text` for the keyboard in front of the
 *  user. Identity on Apple, and identity on any string with no chord in it. */
export function localizeShortcutText(text, platform = keyboardPlatform()) {
  if (!text || platform === APPLE_PLATFORM || !GLYPH.test(text)) return text;
  return String(text).replace(CHORD, (chord) => formatShortcut(chord, platform));
}

/** Rewrites every statically authored shortcut label under `root`.
 *
 *  Returns the number of labels changed, which is what lets a caller (and a
 *  test) tell "nothing needed changing" from "the sweep never ran".
 */
export function localizeShortcutGlyphs(
  root,
  platform = keyboardPlatform(),
  { skip = DOCUMENT_CONTENT_SELECTOR } = {},
) {
  if (!root || platform === APPLE_PLATFORM) return 0;
  let changed = 0;
  const visit = (node) => {
    if (node.nodeType === 3) {
      const next = localizeShortcutText(node.data, platform);
      if (next !== node.data) {
        node.data = next;
        changed += 1;
      }
      return;
    }
    if (node.nodeType !== 1) return;
    if (skip && node.matches?.(skip)) return;
    for (const name of LOCALIZED_ATTRIBUTES) {
      const value = node.getAttribute?.(name);
      if (value == null) continue;
      const next = localizeShortcutText(value, platform);
      if (next !== value) {
        node.setAttribute(name, next);
        changed += 1;
      }
    }
    for (const child of [...node.childNodes]) visit(child);
  };
  visit(root);
  return changed;
}
