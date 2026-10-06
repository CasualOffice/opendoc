// The heading chords' commands: Ctrl+Alt+1/2/3 → Heading 1/2/3, Ctrl+Alt+0 →
// Normal (Word's Ctrl+Alt+1..3, Google Docs' Ctrl+Alt+0..6).
//
// The editor had no heading shortcut at all, so the one formatting act a long
// document needs most — marking its structure — was a trip to the Styles box
// every time. The chords live in `keymap.mjs`; this module answers the only
// question with any rule in it, which is WHICH STYLE "Heading 2" means in the
// document that is open.
//
// That is not a constant. A .docx stores Word's built-in under Word's internal
// name, `heading 2`; another producer writes `Heading 2`; a document may not
// define it at all. `setParagraphStyle` looks a style up by EXACT stored name, so
// the command resolves the document's own spelling through the same mapping the
// Styles box displays with (`styleDisplayName`), which is what makes "the style
// the box calls Heading 2" and "the style Ctrl+Alt+2 applies" one style.
//
// A document with no Heading 2 gets the command DISABLED WITH THE REASON rather
// than a silent no-op (`SKILL` §10). Word would instantiate the built-in from its
// latent-style table; this engine has no operation that creates a built-in with
// Word's default formatting, so that is recorded as the difference rather than
// faked with a style of our own invention. Normal is the exception: with no
// style named Normal, the empty name clears the paragraph to the document's
// default paragraph style, which IS what Normal means.
//
// Pure: the engine reads arrive through `io`, so `quick_styles.test.mjs` drives
// the resolution in node.
import { t } from "./i18n.mjs";
import { styleDisplayName } from "./style_names.mjs";

/** The four commands, as data: id, the UI name it resolves, and how it is labelled. */
export const QUICK_STYLES = Object.freeze([
  Object.freeze({ id: "paragraph.heading.1", uiName: "Heading 1", level: 1 }),
  Object.freeze({ id: "paragraph.heading.2", uiName: "Heading 2", level: 2 }),
  Object.freeze({ id: "paragraph.heading.3", uiName: "Heading 3", level: 3 }),
  Object.freeze({ id: "paragraph.normal", uiName: "Normal", level: 0 }),
]);

/**
 * The document's stored name for the style Word calls `uiName`, or `null`.
 *
 * Complexity: O(styles) — run when the registry is built (a palette open, a
 * chord), never per keystroke.
 */
export function storedStyleFor(uiName, styles) {
  for (const name of styles ?? []) if (styleDisplayName(name) === uiName) return name;
  return null;
}

/**
 * The commands, for `editorCommands()`.
 *
 * @param {object} io
 * @param {() => string[]} io.styles the document's paragraph styles (`listStyles`).
 * @param {() => boolean} io.hasCaret whether there is a paragraph to style.
 * @param {(storedName: string) => unknown} io.apply applies a stored style name
 *        ("" clears to the default paragraph style).
 */
export function quickStyleCommands(io) {
  const styles = io.styles();
  const caret = io.hasCaret();
  return QUICK_STYLES.map(({ id, uiName, level }) => {
    const stored = storedStyleFor(uiName, styles);
    const target = stored ?? (level === 0 ? "" : null);
    return {
      id,
      label: level ? t("style.apply.heading", { level }) : t("style.apply.normal"),
      group: "Paragraph",
      kw: `${uiName} heading paragraph style title level outline`.toLowerCase(),
      enabled: caret && target !== null,
      disabledReason: caret ? t("style.reason.missing", { name: uiName }) : t("paragraph.caretRequired"),
      // File ▸ Shortcuts lists the three headings as ONE row, as Google Docs'
      // reference does ("Apply heading style [1-6]"); Normal keeps its own.
      referenceRow: level ? { id: "headings", label: t("style.apply.headingRange") } : undefined,
      run: () => (target === null ? undefined : io.apply(target)),
    };
  });
}
