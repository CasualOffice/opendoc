// Page setup's presets (`docs/184` D2): a paper-size select and the margin
// chips. A form adapter only: a preset fills the existing distance fields, and
// Apply stays the single transactional writer. Values are twips, independent of
// the reader's units.
import { bindRadioGroup } from "./radio_group.mjs";

export const PAPER_PRESETS = Object.freeze({ letter: [12240, 15840], legal: [12240, 20160], a4: [11906, 16838], a5: [8391, 11906] });
export const MARGIN_PRESETS = Object.freeze({ normal: [1440, 1440, 1440, 1440], narrow: [720, 720, 720, 720], moderate: [1440, 1440, 1080, 1080], wide: [1440, 1440, 2880, 2880] });

/** How far a value may sit from a preset and still BE that preset: one
 *  millimetre (56.7 twips). The fields keep two decimals of the reader's unit,
 *  so an A4 page (11906 twips wide) reads back as 8.27 in = 11909 twips; with
 *  the old one-twip tolerance the dialog called an A4 page "Custom" (audit
 *  DIALOGS-10). Word recognises a paper size the same way, within a tolerance. */
export const PRESET_TOLERANCE_TWIPS = 57;

/** The preset whose every value is within the tolerance of `values`, or
 *  "custom". Pure, so a unit test holds the A4 case. */
export function matchPreset(presets, values, tolerance = PRESET_TOLERANCE_TWIPS) {
  return (
    Object.entries(presets).find(([, candidate]) =>
      candidate.every((value, index) => Number.isFinite(values[index]) && Math.abs(value - values[index]) <= tolerance),
    )?.[0] ?? "custom"
  );
}

export function mountPagePresets({ el, measure, orientation, updatePreview }) {
  const paper = el("pagePaperPreset");
  const customSize = el("pageCustomSize");
  const dimensions = ["pageWidth", "pageHeight"].map(el);
  const edges = ["pageMarginTop", "pageMarginBottom", "pageMarginLeft", "pageMarginRight"].map(el);
  const read = (fields) => fields.map((field) => measure.read(field.value));
  const oriented = (values) => (orientation() === "landscape" ? [...values].reverse() : values);
  // Custom is a chip like the others; choosing it changes nothing and puts the
  // caret in the first edge, which is what "I will type my own" means.
  const margins = bindRadioGroup(el("pageMarginsPreset"), {
    attr: "data-margins",
    onSelect: (value) => {
      const preset = MARGIN_PRESETS[value];
      if (preset) fill(edges, preset);
      else edges[0]?.focus();
    },
  });
  /** Width and height show only for a custom paper size: for a named size they
   *  would repeat the select in two more controls. */
  function reflectCustom() {
    if (customSize) customSize.hidden = paper.value !== "custom";
  }
  function reflect() {
    paper.value = matchPreset(Object.fromEntries(Object.entries(PAPER_PRESETS).map(([key, value]) => [key, oriented(value)])), read(dimensions));
    margins.reflect(matchPreset(MARGIN_PRESETS, read(edges)));
    reflectCustom();
  }
  function fill(fields, values) {
    fields.forEach((field, index) => {
      field.value = measure.format(values[index]);
    });
    updatePreview();
  }
  paper.addEventListener("change", () => {
    const preset = PAPER_PRESETS[paper.value];
    if (preset) fill(dimensions, oriented(preset));
    reflectCustom();
  });
  for (const field of edges) field.addEventListener("input", () => margins.reflect(matchPreset(MARGIN_PRESETS, read(edges))));
  for (const field of dimensions) field.addEventListener("input", () => {
    paper.value = matchPreset(Object.fromEntries(Object.entries(PAPER_PRESETS).map(([key, value]) => [key, oriented(value)])), read(dimensions));
    // Typing a size is choosing Custom: the fields stay, whatever it matches.
    if (customSize) customSize.hidden = false;
  });
  return { reflect };
}
