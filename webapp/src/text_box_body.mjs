// A text box's body properties — Word's Format Shape ▸ Text Box: the four
// insets, the vertical alignment, the overflow and the autofit — between the
// engine's JSON and the inspector's fields.
//
// Why this is its own module (`docs/109` HF-253): the engine has had
// `textBoxBodyProperties` / `setTextBoxBodyProperties` and the inspector had a
// "Text box body" section, and the section did NOTHING. The model serialises
// camelCase (`verticalAnchor`, `autoFit`, …) and leaves out every field that is
// at its default, insets included; the inspector read snake_case keys and gave
// up when `insets` was missing. So:
//
//   * the fields always showed Top / Overflow / Fixed and 0 in insets, whatever
//     the text box really had — Word's own default insets are 0.1in and 0.05in;
//   * Apply returned silently on every text box that still had default insets,
//     which is every text box anyone inserts — a dead control (`SKILL` §10);
//   * and had it got past that, it would have written snake_case keys into a
//     record the model parses with `deny_unknown_fields`, and been refused.
//
// Pure: no DOM, no engine. `tests/text_box_body.test.mjs` drives it against the
// engine's own serialised shapes.

/** EMU per inch. */
const EMU_PER_INCH = 914_400;

/** `TextBoxInsets::default()` — what an omitted `insets` means. */
export const DEFAULT_INSETS_EMU = Object.freeze({
  leftEmu: 91_440,
  topEmu: 45_720,
  rightEmu: 91_440,
  bottomEmu: 45_720,
});

const SIDES = ["left", "top", "right", "bottom"];

/**
 * The fields a body-properties record shows, defaults filled in as the model
 * fills them. `null` for a record that does not parse (not a text box).
 *
 * O(1).
 *
 * @param {string} raw the engine's `textBoxBodyProperties` JSON
 * @returns {{insets: Record<string, number>, verticalAnchor: string,
 *            horizontalOverflow: string, verticalOverflow: string, autoFit: string} | null}
 *          insets in inches, rounded to hundredths as the fields show them
 */
export function readTextBoxBody(raw) {
  const record = parse(raw);
  if (!record) return null;
  const emu = { ...DEFAULT_INSETS_EMU, ...(record.insets ?? {}) };
  return {
    insets: Object.fromEntries(
      SIDES.map((side) => [side, Math.round((Number(emu[`${side}Emu`]) / EMU_PER_INCH) * 100) / 100]),
    ),
    verticalAnchor: record.verticalAnchor ?? "top",
    horizontalOverflow: record.horizontalOverflow ?? "overflow",
    verticalOverflow: record.verticalOverflow ?? "overflow",
    autoFit: record.autoFit?.mode ?? "none",
  };
}

/**
 * The record to hand back to `setTextBoxBodyProperties`: the authored one, with
 * what the fields say written over it — and nothing else touched, so a scale
 * Word wrote for "shrink text on overflow" survives a change of inset.
 *
 * `null` when a field is not a usable value (a negative or non-numeric inset),
 * so the caller can refuse rather than send a record the engine would refuse.
 *
 * O(1).
 *
 * @param {string} raw the engine's current JSON for this text box
 * @param {{insets: Record<string, number>, verticalAnchor: string,
 *          horizontalOverflow: string, verticalOverflow: string, autoFit: string}} fields
 *        insets in inches
 * @returns {string | null}
 */
export function writeTextBoxBody(raw, fields) {
  const record = parse(raw);
  if (!record) return null;
  const insets = {};
  for (const side of SIDES) {
    const inches = Number(fields.insets?.[side]);
    if (!Number.isFinite(inches) || inches < 0) return null;
    insets[`${side}Emu`] = Math.round(inches * EMU_PER_INCH);
  }
  record.insets = insets;
  record.verticalAnchor = fields.verticalAnchor;
  record.horizontalOverflow = fields.horizontalOverflow;
  record.verticalOverflow = fields.verticalOverflow;
  // Keep the authored scale and spacing reduction of a "normal" autofit; the
  // inspector changes the MODE only, and must never erase detail it cannot show.
  record.autoFit =
    fields.autoFit === "normal" && record.autoFit?.mode === "normal"
      ? record.autoFit
      : { mode: fields.autoFit };
  return JSON.stringify(record);
}

/** The record as an object, or `null`. The engine answers `""` for a node that
 *  is not a text box and `"{}"` for one whose body is entirely default. */
function parse(raw) {
  if (typeof raw !== "string" || raw === "") return null;
  try {
    const value = JSON.parse(raw);
    return value && typeof value === "object" && !Array.isArray(value) ? value : null;
  } catch {
    return null;
  }
}
