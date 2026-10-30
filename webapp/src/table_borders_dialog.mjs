// SPDX-License-Identifier: Apache-2.0
// Borders and shading for a table (`docs/153` `table.borders-dialog`).
//
// ## The competitive standard
//
// * **Word** — Table Design ▸ Borders ▸ Borders and Shading…: a Setting column
//   (None, Box, All, Grid, Custom), Style / Color / Width, a PREVIEW whose edges
//   you click to switch on and off (top, bottom, left, right and the two inside
//   lines), "Apply to: Table / Cell", and a Shading tab with a fill.
// * **ONLYOFFICE** — Table ▸ Advanced settings ▸ Borders & Background: the same
//   clickable preview (`web-apps/apps/documenteditor/main/app/view/
//   TableSettingsAdvanced.js`, its `CellBorders`/`TableBorders` preview), border
//   presets, size, colour and a background fill, applied to the table or the
//   selected cells.
// * **Google Docs** — no dialog: a per-cell border width/colour/dash picker in
//   the table toolbar, no inside lines as a unit.
//
// **Followed: Word**, with ONLYOFFICE's single panel (one card, not two tabs):
// a 1280 x 720 viewport has to hold it, and the two references agree on every
// control in it. The pen offers the six line styles the renderer draws apart and
// the seven widths the cell-format popover offers (`table_cell_chrome.mjs`), so
// the two border surfaces cannot disagree about what a border can be.
//
// ## What this module is
//
// The pure edge-diagram MODEL (`toggleEdge`, `presetEdges`, `bordersPatch`) that
// a node test drives, and the dialog wiring around it. The dialog opens on the
// engine's own reading of the current borders — per line, for the selected cells
// or the whole table, inside lines included (`tableBorderState`) — and Apply is
// ONE engine call (`applyTableBorders`), so it is one undo step.
import { t } from "./i18n.mjs";
import {
  BORDER_STYLES,
  BORDER_WEIGHTS_EIGHTH_POINTS,
  DEFAULT_BORDER_STYLE,
  DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS,
  borderWeightLabel,
} from "./table_cell_chrome.mjs";

/** The six lines of the preview, Word's order. */
export const BORDER_LINES = Object.freeze(["top", "bottom", "left", "right", "insideH", "insideV"]);

/** The four outer lines. */
const OUTER = Object.freeze(["top", "bottom", "left", "right"]);

/** The preset buttons, Word's Setting column: None, Box, All, and Grid's
 *  half — the inside lines alone. */
export const BORDER_PRESETS = Object.freeze(["none", "box", "all", "inside"]);

const STATES = new Set(["on", "off", "mixed", "unavailable"]);

/** The diagram's state from the engine's `tableBorderState` reading: every
 *  line, `on` | `off` | `mixed` | `unavailable`. A line the engine did not name
 *  is unavailable rather than guessed. O(1). */
export function edgesFromState(state) {
  const edges = {};
  for (const line of BORDER_LINES) {
    const value = state?.edges?.[line];
    edges[line] = STATES.has(value) ? value : "unavailable";
  }
  return edges;
}

/** One click on a line of the preview: on goes off, off and mixed go on (Word
 *  draws a mixed line whole on the first click). An unavailable line — an
 *  inside line of a single row or column — does not move. O(1). */
export function toggleEdge(edges, line) {
  const current = edges[line];
  if (!current || current === "unavailable") return edges;
  return { ...edges, [line]: current === "on" ? "off" : "on" };
}

/** A preset applied to the diagram. `none` clears every line, `box` draws the
 *  outline and clears the inside, `all` draws every line, `inside` draws the
 *  inside lines and leaves the outline as it is. Unavailable lines stay
 *  unavailable. O(1). */
export function presetEdges(edges, preset) {
  const next = { ...edges };
  for (const line of BORDER_LINES) {
    if (next[line] === "unavailable") continue;
    const outer = OUTER.includes(line);
    if (preset === "none") next[line] = "off";
    else if (preset === "all") next[line] = "on";
    else if (preset === "box") next[line] = outer ? "on" : "off";
    else if (preset === "inside" && !outer) next[line] = "on";
  }
  return next;
}

/** Which preset the diagram currently IS, or "" for a custom one — so the
 *  matching preset button reads as pressed. O(1). */
export function presetOf(edges) {
  for (const preset of BORDER_PRESETS) {
    const shaped = presetEdges(edges, preset);
    if (preset === "inside") continue; // leaves the outline alone, so it matches too eagerly
    if (BORDER_LINES.every((line) => shaped[line] === edges[line])) return preset;
  }
  return "";
}

const samePen = (a, b) => !!a && !!b && a.style === b.style && Number(a.size) === Number(b.size) && String(a.color).toLowerCase() === String(b.color).toLowerCase();

/**
 * The `applyTableBorders` patch for what the reader CHANGED, or `null` when
 * nothing did.
 *
 * A line is sent when its on/off state moved. When the PEN moved, every line
 * that is on is sent too, because that is what choosing a new style and
 * pressing OK means in Word: the lines shown in the preview are drawn with it.
 * A mixed line the reader did not touch is never sent, so it keeps whatever
 * each cell had. Shading is sent when it moved. O(lines).
 *
 * @param {object} args
 * @param {string} args.scope           "cells" | "table"
 * @param {Record<string,string>} args.initial  the diagram as opened
 * @param {Record<string,string>} args.edges    the diagram now
 * @param {{style,size,color}|null} args.initialPen
 * @param {{style,size,color}} args.pen
 * @param {string} args.initialShading  "#rrggbb", "" or "mixed"
 * @param {string} args.shading         "#rrggbb" or "" (no fill)
 * @returns {object|null}
 */
export function bordersPatch({ scope, initial, edges, initialPen, pen, initialShading, shading }) {
  const penMoved = !samePen(initialPen, pen);
  const changed = {};
  for (const line of BORDER_LINES) {
    const now = edges[line];
    if (now !== "on" && now !== "off") continue;
    if (now !== initial[line] || (penMoved && now === "on")) changed[line] = now;
  }
  const patch = { scope };
  if (Object.keys(changed).length > 0) {
    patch.edges = changed;
    patch.style = BORDER_STYLES.includes(pen.style) ? pen.style : DEFAULT_BORDER_STYLE;
    patch.size = Number(pen.size) || DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS;
    patch.color = pen.color;
  }
  if (shading !== initialShading) patch.shading = shading ? shading : "none";
  return Object.keys(patch).length > 1 ? patch : null;
}

/** A CSS border for a pen, so the preview draws what the page will. O(1). */
export function penCss(pen) {
  const style = { single: "solid", double: "double", dotted: "dotted", dashed: "dashed", dotDash: "dashed", dotDotDash: "dotted" }[pen?.style] ?? "solid";
  // Eighth-points to CSS px at 96 dpi, and never less than one device line;
  // a double needs three px to show two.
  const px = Math.max(style === "double" ? 3 : 1, Math.round(((Number(pen?.size) || 4) / 8) * (96 / 72)));
  return `${px}px ${style} ${pen?.color ?? "#000000"}`;
}

let active = null;

/** The dialog as the command tree, the palette and the ribbon reach it. Before
 *  the editor boots it is a no-op. */
export const tableBordersDialog = Object.freeze({
  open: () => active?.open() ?? false,
});

/**
 * Builds the dialog and installs it behind `tableBordersDialog`.
 *
 * `io`: `doc()`, `caretNode()`, `cellRange()` (`[anchor, focus]` or null),
 * `runEdit(fn, options)`, `status(text, kind)`, `registerModal`, `onButton`,
 * `fallbackFocus()`, `afterEdit()`.
 */
export function createTableBordersDialog(io) {
  const root = document.getElementById("tableBordersDialog");
  if (!root) return null;
  const el = (id) => document.getElementById(id);
  const styleSelect = el("tableBordersStyle");
  const widthSelect = el("tableBordersWidth");
  const colorInput = el("tableBordersColor");
  const scopeSelect = el("tableBordersScope");
  const fillInput = el("tableBordersFill");
  const noFill = el("tableBordersNoFill");
  const preview = el("tableBordersPreview");
  const edgeButtons = [...root.querySelectorAll("[data-border-line]")];
  const presetButtons = [...root.querySelectorAll("[data-border-preset]")];

  if (widthSelect && widthSelect.options.length === 0) {
    for (const eighths of BORDER_WEIGHTS_EIGHTH_POINTS) {
      const option = document.createElement("option");
      option.value = String(eighths);
      option.textContent = borderWeightLabel(eighths);
      widthSelect.append(option);
    }
  }

  /** What the dialog opened on, and the diagram now. */
  let opened = null;
  let edges = edgesFromState(null);

  const pen = () => ({
    style: BORDER_STYLES.includes(styleSelect.value) ? styleSelect.value : DEFAULT_BORDER_STYLE,
    size: Number(widthSelect.value) || DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS,
    color: colorInput.value || "#000000",
  });
  const shading = () => (noFill.checked ? "" : fillInput.value);

  /** The corners the dialog acts on: the drawn cell range, else the caret's cell. */
  function corners() {
    const range = io.cellRange();
    if (range?.[0]) return [range[0], range[1] ?? ""];
    const caret = io.caretNode();
    return caret ? [caret, ""] : null;
  }

  function paint() {
    const css = penCss(pen());
    for (const button of edgeButtons) {
      const line = button.dataset.borderLine;
      const state = edges[line];
      button.disabled = state === "unavailable";
      button.hidden = state === "unavailable";
      button.setAttribute("aria-pressed", state === "mixed" ? "mixed" : String(state === "on"));
      const mark = button.firstElementChild;
      if (mark) {
        const vertical = line === "left" || line === "right" || line === "insideV";
        mark.style.borderTop = vertical ? "" : state === "off" ? "" : css;
        mark.style.borderLeft = vertical ? (state === "off" ? "" : css) : "";
        mark.dataset.state = state;
      }
    }
    const current = presetOf(edges);
    for (const button of presetButtons) button.setAttribute("aria-pressed", String(button.dataset.borderPreset === current));
    preview?.style.setProperty("--table-borders-fill", noFill.checked ? "transparent" : fillInput.value);
    fillInput.disabled = noFill.checked;
  }

  /** Reads the engine's state for the current scope. False when there is no table. */
  function read() {
    const doc = io.doc();
    const ends = corners();
    if (!doc || !ends) return false;
    let state;
    try {
      state = JSON.parse(doc.tableBorderState(ends[0], ends[1], scopeSelect.value));
    } catch (error) {
      io.status(error?.message ?? String(error), "warn");
      return false;
    }
    edges = edgesFromState(state);
    const initialPen = state.pen ?? null;
    if (initialPen) {
      styleSelect.value = BORDER_STYLES.includes(initialPen.style) ? initialPen.style : DEFAULT_BORDER_STYLE;
      widthSelect.value = BORDER_WEIGHTS_EIGHTH_POINTS.includes(initialPen.size)
        ? String(initialPen.size)
        : String(DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS);
      colorInput.value = initialPen.color;
    }
    const fill = state.shading;
    noFill.checked = !fill || fill === "mixed";
    if (fill && fill !== "mixed") fillInput.value = fill;
    // The pen as the CONTROLS now show it, so an untouched control is not a
    // change; and `null` when nothing is drawn, so the first line the reader
    // adds is drawn with whatever pen they see.
    opened = { edges: { ...edges }, pen: initialPen ? pen() : null, shading: fill === "mixed" ? "mixed" : shading() };
    paint();
    return true;
  }

  const modal = io.registerModal(root, {
    initialFocus: () => edgeButtons.find((button) => !button.disabled) ?? scopeSelect,
    fallbackFocus: io.fallbackFocus,
    defaultAction: () => void apply(),
  });

  async function apply() {
    const doc = io.doc();
    const ends = corners();
    if (!doc || !ends || !opened) return false;
    const patch = bordersPatch({
      scope: scopeSelect.value,
      initial: opened.edges,
      edges,
      initialPen: opened.pen,
      pen: pen(),
      initialShading: opened.shading,
      shading: shading(),
    });
    if (!patch) {
      modal.close();
      return true;
    }
    const landed = await io.runEdit(() => doc.applyTableBorders(ends[0], ends[1], JSON.stringify(patch)), { gate: true });
    if (landed) {
      modal.close();
      io.status(t("tableBorders.applied"));
      io.afterEdit();
    }
    return landed;
  }

  for (const button of edgeButtons) {
    io.onButton(button, () => {
      edges = toggleEdge(edges, button.dataset.borderLine);
      paint();
    });
  }
  for (const button of presetButtons) {
    io.onButton(button, () => {
      edges = presetEdges(edges, button.dataset.borderPreset);
      paint();
    });
  }
  for (const control of [styleSelect, widthSelect, colorInput, fillInput, noFill]) {
    control?.addEventListener("input", paint);
    control?.addEventListener("change", paint);
  }
  // A different "Apply to" is a different set of lines, so the diagram re-reads.
  scopeSelect.addEventListener("change", () => void read());
  for (const button of root.querySelectorAll("[data-dialog-cancel]")) io.onButton(button, () => modal.close());
  for (const button of root.querySelectorAll("[data-dialog-apply]")) io.onButton(button, () => void apply());

  const instance = {
    open() {
      const range = io.cellRange();
      // A drawn cell range opens on Cells; a bare caret opens on Table, which
      // is where ONLYOFFICE's table settings start. Word opens a caret on Cell;
      // we deliberately do not, because borders a reader meant for the table
      // landing on one cell is the more surprising of the two mistakes.
      scopeSelect.value = range?.[0] ? "cells" : "table";
      if (!read()) return false;
      modal.open();
      return true;
    },
    apply,
  };
  active = instance;
  return instance;
}
