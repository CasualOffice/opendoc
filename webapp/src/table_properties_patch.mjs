// SPDX-License-Identifier: Apache-2.0
// Table properties: validate the form, and send only what the reader CHANGED.
//
// Extracted from `main.js` for `table_commands.mjs`'s reason, which is the one
// this repository keeps writing down: `main.js` has zero exports and no mount
// seam, so a function living there is a function no test can reach. This one is
// worth reaching — it is a validator and a DIFF, and the diff is the part that is
// easy to get subtly wrong. A patch that carried every field would re-assert
// eleven properties the reader did not touch, which on a table somebody else is
// also editing is eleven operations to transform and eleven chances to clobber.
//
// It was also the payment for the access-rights surface's lines (`module_seams`'s
// ratchet is a ratchet and this file is what bought the room), so the extraction
// is real work rather than a line-count manoeuvre: the function arrives testable.
//
// PURE. It reads the live form elements through the `form` bag and reports a
// validation failure by calling the browser's own `reportValidity`, which is the
// behaviour it had in `main.js` and is deliberately unchanged — an extraction
// that also redesigns the failure path is two changes wearing one diff.

/** The fields whose value is a measurement, in the order the form validates
 *  them — which is the order they appear, so the first complaint is the topmost
 *  problem rather than whichever field the object literal happened to list
 *  first. */
const MEASURED = Object.freeze([
  "width",
  "indent",
  "columnWidth",
  "rowHeight",
  "cellMargin",
  "cellSpacing",
]);

/**
 * The properties that differ from what the table currently holds, or `null` when
 * the form is not valid.
 *
 * `null` and not an empty object: an empty patch is a real and different answer —
 * "nothing changed, do nothing" — and conflating it with "the reader typed
 * something impossible" is how a refused dialog looks like a successful one.
 *
 * # Why row height is special-cased twice
 *
 * Once because `auto` is expressed as `-1` rather than as an absent value, which
 * is the bridge's encoding and not a choice available here; and once because the
 * bridge requires the value and the rule TOGETHER whenever either moves. Sending
 * a rule with no value leaves the engine to guess at a height, which is the one
 * thing a geometry call must never do.
 *
 * Complexity: O(fields) — eleven of them, once per Apply.
 *
 * @param {Record<string, HTMLInputElement|HTMLSelectElement>} form the live fields
 * @param {object} context
 * @param {string} context.alignment the radio group's answer
 * @param {Record<string, unknown>} context.current what the table holds now
 * @param {(value: string) => number} context.inchesToTwips
 * @param {(value: string) => number} context.signedInchesToTwips
 * @param {string} context.rowHeightRequired the sentence for a missing row height
 * @returns {Record<string, unknown>|null}
 */
export function tablePropertiesPatch(form, context) {
  const measured = MEASURED.map((name) => form[name]).filter((input) => input && !input.disabled);
  for (const input of measured) {
    input.setCustomValidity("");
    if (!input.checkValidity()) {
      input.reportValidity();
      input.focus();
      return null;
    }
  }
  if (form.rowHeightRule.value !== "auto" && form.rowHeight.value.trim() === "") {
    form.rowHeight.setCustomValidity(context.rowHeightRequired);
    form.rowHeight.reportValidity();
    form.rowHeight.focus();
    return null;
  }

  const next = {
    alignment: context.alignment,
    tableWidthTwips: context.inchesToTwips(form.width.value),
    tableIndentTwips: context.signedInchesToTwips(form.indent.value),
    fixedLayout: form.fixedLayout.checked,
    headerRow: form.headerRow.checked,
    columnWidthTwips: context.inchesToTwips(form.columnWidth.value),
    rowHeightTwips:
      form.rowHeightRule.value === "auto" ? -1 : context.inchesToTwips(form.rowHeight.value),
    rowHeightRule: form.rowHeightRule.value,
    cellMarginTwips: context.inchesToTwips(form.cellMargin.value),
    cellSpacingTwips: context.inchesToTwips(form.cellSpacing.value),
    caption: form.caption.value,
    description: form.description.value,
  };
  const patch = {};
  for (const [key, value] of Object.entries(next)) {
    // A disabled column-width field is not "zero width": it is a field this table
    // shape has no answer for, and sending its value would set one.
    if (key === "columnWidthTwips" && form.columnWidth.disabled) continue;
    if (value !== context.current[key]) patch[key] = value;
  }
  if ("rowHeightTwips" in patch || "rowHeightRule" in patch) {
    patch.rowHeightTwips = next.rowHeightTwips;
    patch.rowHeightRule = next.rowHeightRule;
  }
  return patch;
}
