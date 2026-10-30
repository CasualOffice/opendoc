// SPDX-License-Identifier: Apache-2.0
// Table text wrapping and positioning (`docs/153` `table.text-wrapping`,
// `table.position-dialog`).
//
// ## The competitive standard
//
// * **Word** — Table Properties ▸ Table ▸ Text wrapping: None | Around, and
//   beside it Positioning…: Horizontal position (Left, Center, Right, Inside,
//   Outside, or a measurement) relative to Column / Margin / Page; Vertical
//   position (Top, Center, Bottom, Inside, Outside, or a measurement) relative
//   to Paragraph / Margin / Page; Distance from surrounding text (top, bottom,
//   left, right); "Move with text"; "Allow overlap".
// * **ONLYOFFICE** — Table ▸ Advanced settings ▸ Text wrapping (inline /
//   flow) and Table position, the same fields
//   (`web-apps/apps/documenteditor/main/app/view/TableSettingsAdvanced.js`,
//   its `radioHAlign`/`radioHPosition` and "Move object with text" /
//   "Allow overlap" boxes).
// * **Google Docs** — no floating tables.
//
// **Followed: Word.** The two references agree on every field; Word's is the
// one this editor's Table properties panel already mirrors, so the wrap choice
// sits in the panel and Positioning… opens beside it, exactly as Word's does.
// "Move with text" is Word's own mapping, not a separate property: it is the
// vertical position measured from the paragraph (`w:vertAnchor="text"`), so the
// box and the "relative to" field are one setting shown twice and are kept in
// step here.
//
// ## What this module is
//
// The pure form model (`formFromPosition`, `positionPatch`) and the wiring. The
// engine reads (`tableFloatPosition`) and writes (`setTableFloatPosition`) the
// table's `w:tblpPr` and `w:tblOverlap`, one undo step per change; the patch
// carries ONLY what the reader changed, so a field the dialog cannot show (a
// `w:tblpYSpec="inline"` from a file) survives an edit of the others.
import { t } from "./i18n.mjs";

const TWIPS_PER_INCH = 1440;

/** The positions each axis offers, in Word's order; `offset` is "a measurement". */
export const H_ALIGNS = Object.freeze(["left", "center", "right", "inside", "outside", "offset"]);
export const V_ALIGNS = Object.freeze(["top", "center", "bottom", "inside", "outside", "offset"]);
export const H_RELATIVE = Object.freeze(["column", "margin", "page"]);
export const V_RELATIVE = Object.freeze(["paragraph", "margin", "page"]);

/** The bound the engine and the file format share (`ST_SignedTwipsMeasure`). */
const MAX_INCHES = 22;

/** Twips as the inches text a field shows: signed, two places, no trailing
 *  zeros, and `"0"` for zero. O(1). */
export function inchesText(twips) {
  const value = Number(twips) || 0;
  return (value / TWIPS_PER_INCH).toFixed(2).replace(/\.?0+$/, "") || "0";
}

/** An inches field's text as twips, or `null` when it is not a number in
 *  `min..=max` inches. Never clamped: a dialog says what it wants. O(1). */
export function inchesField(text, min, max) {
  const raw = String(text ?? "").trim();
  if (raw === "" || !Number.isFinite(Number(raw))) return null;
  const value = Number(raw);
  return value < min || value > max ? null : Math.round(value * TWIPS_PER_INCH);
}

/** The vertical position the form SHOWS for the engine's value: a file's
 *  `w:tblpYSpec="inline"` is drawn as Top (`table_float` treats them alike),
 *  and Word's dialog offers no separate choice for it. O(1). */
function shownVAlign(value) {
  return value === "inline" ? "top" : value;
}

/** The form's values for the engine's reading. O(1). */
export function formFromPosition(state) {
  return {
    hAlign: state.hAlign,
    hOffset: inchesText(state.hOffsetTwips),
    hRelative: state.hRelative,
    vAlign: shownVAlign(state.vAlign),
    vOffset: inchesText(state.vOffsetTwips),
    vRelative: state.vRelative,
    top: inchesText(state.topTwips),
    bottom: inchesText(state.bottomTwips),
    left: inchesText(state.leftTwips),
    right: inchesText(state.rightTwips),
    moveWithText: state.vRelative === "paragraph",
    allowOverlap: state.allowOverlap !== false,
  };
}

/** "Move with text" and the vertical "relative to" are one setting: ticking
 *  the box measures from the paragraph; clearing it while the field says
 *  paragraph measures from the margin, which is where Word's goes. Returns the
 *  values with the two in step, given which of them the reader just changed.
 *  O(1). */
export function linkMoveWithText(values, changed) {
  if (changed === "moveWithText") {
    if (values.moveWithText) return { ...values, vRelative: "paragraph" };
    return values.vRelative === "paragraph" ? { ...values, vRelative: "margin" } : values;
  }
  return { ...values, moveWithText: values.vRelative === "paragraph" };
}

const MEASURES = Object.freeze([
  ["hOffset", "hOffsetTwips", -MAX_INCHES],
  ["vOffset", "vOffsetTwips", -MAX_INCHES],
  ["top", "topTwips", 0],
  ["bottom", "bottomTwips", 0],
  ["left", "leftTwips", 0],
  ["right", "rightTwips", 0],
]);

/**
 * The `setTableFloatPosition` patch for what changed between the engine's
 * reading `state` and the form `values`: `{patch}` (`null` when nothing did),
 * or `{invalid}` naming the first field that is not a measurement in range.
 *
 * An offset field is only read when its axis is set to a measurement, so a
 * stale number in a disabled field is never sent. O(fields).
 */
export function positionPatch(state, values) {
  const patch = {};
  for (const [field, key, min] of MEASURES) {
    if (field === "hOffset" && values.hAlign !== "offset") continue;
    if (field === "vOffset" && values.vAlign !== "offset") continue;
    // A field still showing what it opened on is untouched, even though its
    // two-place inches do not round-trip the twips (180 shows as 0.13 = 187).
    if (String(values[field] ?? "").trim() === inchesText(state[key])) continue;
    const twips = inchesField(values[field], min, MAX_INCHES);
    if (twips === null) return { invalid: field };
    if (twips !== (Number(state[key]) || 0)) patch[key] = twips;
  }
  if (values.hAlign !== state.hAlign) patch.hAlign = values.hAlign;
  // Against what the form showed, so an untouched `inline` is not rewritten.
  if (values.vAlign !== shownVAlign(state.vAlign)) patch.vAlign = values.vAlign;
  if (values.hRelative !== state.hRelative) patch.hRelative = values.hRelative;
  if (values.vRelative !== state.vRelative) patch.vRelative = values.vRelative;
  if (values.allowOverlap !== (state.allowOverlap !== false)) patch.allowOverlap = values.allowOverlap;
  return { patch: Object.keys(patch).length ? patch : null };
}

let active = null;

/** The Positioning dialog and the wrap choice as the rest of the editor reaches
 *  them. No-ops before the editor boots. */
export const tablePosition = Object.freeze({
  open: () => active?.open() ?? false,
  setWrap: (wrap) => active?.setWrap(wrap) ?? Promise.resolve(false),
  reflect: () => active?.reflect(),
});

/**
 * Builds the dialog and the panel's wrap control.
 *
 * `io`: `doc()`, `caretNode()`, `runEdit(fn, options)`, `status(text, kind)`,
 * `registerModal`, `onButton`, `fallbackFocus()`, `afterEdit()`.
 */
export function createTablePosition(io) {
  const el = (id) => document.getElementById(id);
  const root = el("tablePositionDialog");
  const wrapGroup = el("tableWrap");
  const positionButton = el("tablePositionBtn");
  const wrapNote = el("tableWrapNote");
  const fields = {
    hAlign: el("tablePositionHAlign"),
    hOffset: el("tablePositionHOffset"),
    hRelative: el("tablePositionHRelative"),
    vAlign: el("tablePositionVAlign"),
    vOffset: el("tablePositionVOffset"),
    vRelative: el("tablePositionVRelative"),
    top: el("tablePositionTop"),
    bottom: el("tablePositionBottom"),
    left: el("tablePositionLeft"),
    right: el("tablePositionRight"),
    moveWithText: el("tablePositionMoveWithText"),
    allowOverlap: el("tablePositionOverlap"),
  };

  /** The engine's reading for the caret's table, or null. */
  function readState() {
    const doc = io.doc();
    const node = io.caretNode();
    if (!doc || !node) return null;
    try {
      return JSON.parse(doc.tableFloatPosition(node));
    } catch {
      return null;
    }
  }

  /** The panel's wrap radios and the Positioning button, from the table. */
  function reflect() {
    const state = readState();
    if (!wrapGroup) return;
    for (const input of wrapGroup.querySelectorAll('input[name="tableWrap"]')) {
      input.checked = input.value === (state?.wrap ?? "none");
      input.disabled = !state || (input.value === "around" && !state.floatable);
    }
    if (positionButton) positionButton.disabled = !state || state.wrap !== "around";
    if (wrapNote) wrapNote.hidden = !state || state.floatable;
  }

  async function setWrap(wrap) {
    const doc = io.doc();
    const node = io.caretNode();
    if (!doc || !node || (wrap !== "none" && wrap !== "around")) return false;
    const landed = await io.runEdit(() => doc.setTableFloatPosition(node, JSON.stringify({ wrap })), { gate: true });
    if (landed) {
      io.status(t(wrap === "around" ? "tablePosition.wrapAround" : "tablePosition.wrapNone"));
      io.afterEdit();
    }
    reflect();
    return landed;
  }

  wrapGroup?.addEventListener("change", (event) => {
    // The panel commits its OWN fields on change; this group is not among
    // them, and this listener is the only thing that writes it.
    event.stopPropagation();
    if (event.target instanceof HTMLInputElement && event.target.name === "tableWrap") {
      void setWrap(event.target.value);
    }
  });

  let opened = null;

  function values() {
    return {
      hAlign: fields.hAlign.value,
      hOffset: fields.hOffset.value,
      hRelative: fields.hRelative.value,
      vAlign: fields.vAlign.value,
      vOffset: fields.vOffset.value,
      vRelative: fields.vRelative.value,
      top: fields.top.value,
      bottom: fields.bottom.value,
      left: fields.left.value,
      right: fields.right.value,
      moveWithText: fields.moveWithText.checked,
      allowOverlap: fields.allowOverlap.checked,
    };
  }

  function show(form) {
    for (const [name, input] of Object.entries(fields)) {
      if (!input) continue;
      if (input.type === "checkbox") input.checked = !!form[name];
      else input.value = form[name];
      input.setCustomValidity?.("");
    }
    fields.hOffset.disabled = form.hAlign !== "offset";
    fields.vOffset.disabled = form.vAlign !== "offset";
  }

  const modal = root
    ? io.registerModal(root, {
        initialFocus: () => fields.hAlign,
        fallbackFocus: io.fallbackFocus,
        defaultAction: () => void apply(),
      })
    : null;

  root?.addEventListener("change", (event) => {
    const target = event.target;
    if (target === fields.moveWithText || target === fields.vRelative) {
      show(linkMoveWithText(values(), target === fields.moveWithText ? "moveWithText" : "vRelative"));
    } else if (target === fields.hAlign || target === fields.vAlign) {
      show(values());
    }
  });

  async function apply() {
    const doc = io.doc();
    const node = io.caretNode();
    if (!doc || !node || !opened) return false;
    const result = positionPatch(opened, values());
    if (result.invalid) {
      const input = fields[result.invalid];
      input.setCustomValidity(t("tablePosition.invalid"));
      input.reportValidity();
      input.focus();
      return false;
    }
    if (!result.patch) {
      modal.close();
      return true;
    }
    const landed = await io.runEdit(() => doc.setTableFloatPosition(node, JSON.stringify(result.patch)), { gate: true });
    if (landed) {
      modal.close();
      io.status(t("tablePosition.applied"));
      io.afterEdit();
    }
    return landed;
  }

  if (root) {
    for (const button of root.querySelectorAll("[data-dialog-cancel]")) io.onButton(button, () => modal.close());
    for (const button of root.querySelectorAll("[data-dialog-apply]")) io.onButton(button, () => void apply());
  }

  const instance = {
    open() {
      const state = readState();
      if (!state || !modal) return false;
      if (state.wrap !== "around") {
        io.status(t("tablePosition.needsAround"), "warn");
        return false;
      }
      opened = state;
      show(formFromPosition(state));
      modal.open();
      return true;
    },
    setWrap,
    reflect,
  };
  if (positionButton) io.onButton(positionButton, () => instance.open());
  active = instance;
  return instance;
}
