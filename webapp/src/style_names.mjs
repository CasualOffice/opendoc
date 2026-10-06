// What Word CALLS its built-in styles, as opposed to what it STORES.
//
// A .docx names Word's built-in paragraph styles in `w:name` with Word's
// internal spelling, and several of those are lower case: `heading 1` …
// `heading 9`, `toc 1`, `caption`, `header`, `footer`, `annotation text`. Word
// never shows those strings. Its gallery, Styles pane and every dialog show the
// UI name — "Heading 1", "TOC 1", "Comment Text" — and keep the stored name for
// the file. The editor printed the stored name everywhere, so the Styles box, its
// search and the palette offered "heading 1" beside "Title" and "Normal", which
// reads as a broken import rather than as Word's own vocabulary
// (`desk-08c-styles-dropdown.png`, `desk-08f-styles-search-heading.png`).
//
// THIS IS DISPLAY ONLY. The stored name is the style's identity:
// `setParagraphStyle` looks a style up by exact name, a command id is
// `style.<stored name>`, and export writes `w:name` back untouched. Nothing here
// is ever passed back to the engine; every caller keeps the stored name as the
// value and shows this as the label.
//
// English only, and that is a recorded gap rather than an oversight: Word
// localises these names too ("Überschrift 1"), and doing the same is a catalogue
// family of its own (`docs/109`). A custom style is never renamed — "p1" stays
// "p1" — because only Word's own built-ins have a UI name to show.
//
// Pure: no DOM, no engine. `style_names.test.mjs` drives it in node.

/** Built-in styles whose UI name differs from the stored one, keyed by the
 *  stored name LOWER-CASED. Matched case-insensitively because Word recognises
 *  its built-ins that way and producers disagree about case (LibreOffice and
 *  python-docx both write `heading 1`; others write `Heading 1`). The Comment
 *  rows are the renames that are not merely capitalisation: Word stores
 *  `annotation text` and shows "Comment Text". */
const UI_NAMES = new Map([
  ["normal", "Normal"],
  ["title", "Title"],
  ["subtitle", "Subtitle"],
  ["header", "Header"],
  ["footer", "Footer"],
  ["caption", "Caption"],
  ["quote", "Quote"],
  ["intense quote", "Intense Quote"],
  ["no spacing", "No Spacing"],
  ["body text", "Body Text"],
  ["list paragraph", "List Paragraph"],
  ["normal indent", "Normal Indent"],
  ["footnote text", "Footnote Text"],
  ["endnote text", "Endnote Text"],
  ["annotation text", "Comment Text"],
  ["annotation subject", "Comment Subject"],
  ["index heading", "Index Heading"],
  ["table of figures", "Table of Figures"],
  ["table of authorities", "Table of Authorities"],
  ["toa heading", "TOA Heading"],
  ["toc heading", "TOC Heading"],
  ["envelope address", "Envelope Address"],
  ["envelope return", "Envelope Return"],
  ["macro", "Macro Text"],
  ["balloon text", "Balloon Text"],
  ["plain text", "Plain Text"],
]);

/** The numbered families: `heading 1` → "Heading 1", `toc 3` → "TOC 3",
 *  `index 2` → "Index 2". Word's built-ins stop at 9. */
const NUMBERED = [
  [/^heading ([1-9])$/i, "Heading"],
  [/^toc ([1-9])$/i, "TOC"],
  [/^index ([1-9])$/i, "Index"],
];

/**
 * The name to SHOW for paragraph style `name`.
 *
 * Word's UI name for one of its built-ins, and `name` itself otherwise. Never
 * the value to apply — see the header.
 *
 * Complexity: O(1).
 */
export function styleDisplayName(name) {
  if (typeof name !== "string" || !name) return name;
  const fixed = UI_NAMES.get(name.toLowerCase());
  if (fixed) return fixed;
  for (const [pattern, family] of NUMBERED) {
    const match = pattern.exec(name);
    if (match) return `${family} ${match[1]}`;
  }
  return name;
}
