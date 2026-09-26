// The colour vocabularies the swatch pickers offer, as data.
//
// Literal tables with no behaviour beyond one lookup, so — like `glyph_sets.mjs`
// — nothing in here has any business reaching a global, and the engine-parity
// question "does the picker offer every highlight the engine can write?" is
// answerable in node instead of a browser (`palettes.test.mjs`).
//
// Extracted from `main.js` when two independently green PRs merged into a red
// `main`: the file was AT its line ratchet, so #613 and #614 each paid for one
// new line by deleting the SAME trailing blank line, and git can only apply
// that deletion once. Cosmetic whitespace is not currency — the ratchet wants
// code moved out, and this is the move that buys the slack back.

/** Standard-colors palette (Google-Docs-style: a grayscale row + a hue row).
 *  The document theme palette is not exposed to the webapp, so the theme-colors
 *  row is omitted gracefully rather than faked. */
export const TEXT_STANDARD_COLORS = Object.freeze([
  "#000000", "#434343", "#666666", "#999999", "#b7b7b7", "#cccccc", "#d9d9d9", "#efefef", "#f3f3f3", "#ffffff",
  "#980000", "#ff0000", "#ff9900", "#ffff00", "#00ff00", "#00ffff", "#4a86e8", "#0000ff", "#9900ff", "#ff00ff",
]);

/** The complete set of OOXML `w:highlight` named colors the engine accepts, with
 *  their display swatch and a human label. `setHighlight` takes the name, not a
 *  hex. "Complete" is asserted, not asserted-by-comment: `palettes.test.mjs`
 *  derives the engine's own set from `highlight_token` in
 *  `casual-doc-export/src/semantic.rs` and fails if this table drifts from it,
 *  because a highlight the engine can write and the picker cannot offer is a
 *  capability no user can reach. */
export const HIGHLIGHT_COLORS = Object.freeze([
  { name: "yellow", hex: "#ffff00", label: "Yellow" },
  { name: "green", hex: "#00ff00", label: "Bright green" },
  { name: "cyan", hex: "#00ffff", label: "Turquoise" },
  { name: "magenta", hex: "#ff00ff", label: "Pink" },
  { name: "blue", hex: "#0000ff", label: "Blue" },
  { name: "red", hex: "#ff0000", label: "Red" },
  { name: "darkYellow", hex: "#808000", label: "Dark yellow" },
  { name: "darkGreen", hex: "#008000", label: "Green" },
  { name: "darkCyan", hex: "#008080", label: "Teal" },
  { name: "darkMagenta", hex: "#800080", label: "Violet" },
  { name: "darkRed", hex: "#800000", label: "Dark red" },
  { name: "darkBlue", hex: "#000080", label: "Dark blue" },
  { name: "darkGray", hex: "#808080", label: "Gray 50%" },
  { name: "lightGray", hex: "#c0c0c0", label: "Gray 25%" },
  { name: "black", hex: "#000000", label: "Black" },
  { name: "white", hex: "#ffffff", label: "White" },
]);

const HIGHLIGHT_HEX = new Map(HIGHLIGHT_COLORS.map((c) => [c.name, c.hex]));

/** Name to human label, for the picker's swatch titles. */
export const HIGHLIGHT_LABEL = new Map(HIGHLIGHT_COLORS.map((c) => [c.name, c.label]));

/** The swatch to paint for a highlight name. `"none"` and the empty name are
 *  the absence of a highlight, not an unknown one, so they answer `null` the
 *  same way an unrecognised name does. */
export function highlightHex(name) {
  return name && name !== "none" ? HIGHLIGHT_HEX.get(name) ?? null : null;
}
