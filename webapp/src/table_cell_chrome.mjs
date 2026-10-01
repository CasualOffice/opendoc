// The CELL-level chrome of the Table band: the cell-format popover (shading,
// vertical alignment, the four cell-border presets and the four table-border
// presets) and the split-cell dialog.
//
// Extracted from `main.js` for the reason `table_commands.mjs`, `table_band.mjs`
// and `view_zoom.mjs` already give: `main.js` is at its line ratchet with no
// slack (`module_seams`), and the gutter and cell-selection layers had to be
// paid for out of it. These two are the natural thing to take, because neither
// reads a single piece of editor state directly — every binding arrives in
// `host` — so they were living in the 16,000-line file for no reason but
// history.
//
// The subject they share is "one cell, or the cells you have selected": the
// popover writes cell properties and the dialog splits one cell. Table-scoped
// controls (the properties inspector, the formula, autofit) are a different
// subject and stay where they are.
import { n } from "./i18n.mjs";

/** The border widths the pen offers, in EIGHTH-POINTS — `w:sz`'s own unit, and
 *  the unit `setCellBorderRange` and `setTableBorder` take, so nothing converts
 *  between the control and the engine.
 *
 *  THE LIST IS THE ONE GOOGLE DOCS AND ONLYOFFICE AGREE ON, and it is a subset of
 *  Word's. Verified from source rather than memory:
 *  `reference/web-apps/apps/common/main/lib/component/ComboBorderSize.js:109-115`
 *  offers 0.5, 1, 1.5, 2.25, 3, 4.5 and 6 pt, and Google Docs' border-width menu
 *  is the same seven.
 *
 *  Word also offers ¼ pt and ¾ pt and this deliberately does not. The engine can
 *  represent them (it clamps `w:sz` to 2..=96, so ¼ pt is the floor rather than
 *  out of range) — they are left out because two of the three references do not
 *  offer them and because at 100% zoom on a 96-DPI display ¼ pt is a third of a
 *  pixel, so the control would promise a distinction the page cannot show. That
 *  is a judgement, recorded here so it can be argued with rather than discovered.
 *
 *  `none` is not in this list: clearing a border is the `none` PRESET beside the
 *  weight, not a zero width. ONLYOFFICE folds the two together by prepending a
 *  "No borders" row to the same combo; we already had the preset, and a second
 *  way to say the same thing is how two controls start disagreeing. */
export const BORDER_WEIGHTS_EIGHTH_POINTS = Object.freeze([4, 8, 12, 18, 24, 36, 48]);

/** Word's and ONLYOFFICE's default pen, and the engine's own.
 *
 *  Three values were in play before this: the cell-format popover wrote 8
 *  (1 pt), `insertTable` in the wasm facade writes 4, and Word's Table Design
 *  opens on ½ pt — so a table inserted by the editor had ½ pt gridlines and the
 *  first border a person applied silently doubled to 1 pt. One value now, and it
 *  is the one all three references already used.
 *
 *  ONLYOFFICE source: `ComboBorderSize` prepends "No borders" and
 *  `TableSettings.js:366` then selects `store.at(1)`, which is the 0.5 pt row. */
export const DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS = 4;

/** The six line styles `casual_doc_layout::flow` can draw apart, in the order the
 *  control offers them.
 *
 *  Exactly these six, because exactly these six render differently: the three
 *  setters (`setCellBorder`, `setCellBorderRange`, `setTableBorder`) refuse
 *  anything else with a sentence rather than painting it as a solid line, and
 *  `cellBorderStyle` canonicalises whatever the document carries onto this same
 *  vocabulary — a `dashSmallGap` in an imported file reflects as `dashed` — so
 *  there is one spelling per rendering on both sides of the boundary.
 *
 *  The model can hold any `w:val` token and the exporter writes it back verbatim;
 *  what this list is about is what can be AUTHORED and seen. Offering `wave` or
 *  `thickThinMediumGap` in a dropdown that paints a plain line would be a control
 *  that lies about what it did.
 *
 *  Word's Line Style list is longer (24 entries) and ONLYOFFICE's border combo
 *  offers no style at all (`ComboBorderSize` is widths only), so this is a
 *  deliberate midpoint: every style the renderer distinguishes, and nothing it
 *  does not. */
export const BORDER_STYLES = Object.freeze([
  "single",
  "double",
  "dotted",
  "dashed",
  "dotDash",
  "dotDotDash",
]);

/** The style the engine applies when none is given, and what the control opens
 *  on. `setCellBorder`'s trailing argument is optional and means this, so an
 *  omitted style and this value are the same request. */
export const DEFAULT_BORDER_STYLE = "single";

/** `18` -> `"2.25 pt"`, in the reader's locale — `"2,25 pt"` in French.
 *
 *  The number goes through `Intl` rather than being written out seven times in
 *  markup: that is one catalogue key for the unit instead of seven English
 *  literals, and it is also the only way a comma-decimal locale reads correctly.
 *  `pt` is not translated and is not routed — it is the unit symbol, the same in
 *  every language this ships in, and the spacing and paragraph dialogs already
 *  print it bare.
 *
 *  Complexity: O(1). */
export function borderWeightLabel(eighthPoints) {
  return `${n(eighthPoints / 8)} pt`;
}

/**
 * Wires the cell-format popover.
 *
 * Every write goes through the engine's `…Range` variants via `host.formatRange`,
 * so it applies to EVERY selected cell in ONE undo entry (`docs/141` TBL-08,
 * `docs/109` HF-219). This used to refuse with *"Shading, alignment and cell
 * borders apply to one cell — put the caret in the cell to format it"*, because
 * every write went through the caret's own paragraph and nothing in the product
 * could iterate a selection's cells. That refusal was honest at the time and it
 * is gone now because it became unnecessary, not because it was suppressed.
 *
 * Passing the caret's own node as both endpoints is a one-cell range, so the
 * no-selection case is the SAME path rather than a second one.
 *
 * Table-scoped presets (`setTableBorder`) stay on `host.runNodeEdit`: a cell
 * selection does not change what they mean.
 *
 * @param {object} host
 * @param {HTMLElement} host.menu           the popover root (`#tableFmtMenu`)
 * @param {HTMLInputElement} host.shade     the cell shading colour input
 * @param {HTMLElement} host.shadeNone      the "no fill" button
 * @param {HTMLElement} host.vAlign         the vertical-alignment radio group
 * @param {HTMLInputElement} host.cellBorderColor
 * @param {HTMLInputElement} host.tableBorderColor
 * @param {HTMLSelectElement} host.borderWeight  the pen's width, in eighth-points
 * @param {HTMLSelectElement} host.borderStyle   the pen's line style, one of `BORDER_STYLES`
 * @param {() => object|null} host.doc
 * @param {() => string} host.caretNode     the caret's paragraph node id, or ""
 * @param {(apply) => boolean} host.formatRange  the range write runner
 * @param {(thunk) => boolean} host.runNodeEdit
 * @param {(el, fn) => void} host.onButton
 * @param {(el, options) => object} host.bindRadioGroup
 * @param {(hex:string) => Array<number>} host.hexToRgb
 * @returns {{reflect: () => void}} the popover's own state reflector
 */
export function bindCellFormatMenu(host) {
  // The pen's options, filled once. Deliberately NOT refilled on reflect: the
  // list does not depend on the document, and rebuilding a `<select>` under a
  // keyboard user who has it open loses their place.
  if (host.borderWeight && host.borderWeight.options.length === 0) {
    for (const eighths of BORDER_WEIGHTS_EIGHTH_POINTS) {
      const option = document.createElement("option");
      option.value = String(eighths);
      option.textContent = borderWeightLabel(eighths);
      host.borderWeight.append(option);
    }
    host.borderWeight.value = String(DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS);
  }

  /** The width the next border stroke draws with, in eighth-points.
   *
   *  Falls back to the default rather than to `NaN` when the control is absent or
   *  somehow empty: a border is what the person asked for, and refusing to draw
   *  one because a dropdown could not be read would be the worse answer. */
  function penWeight() {
    const chosen = Number(host.borderWeight?.value);
    return Number.isFinite(chosen) && chosen > 0
      ? chosen
      : DEFAULT_BORDER_WEIGHT_EIGHTH_POINTS;
  }

  /** The line style the next border stroke draws with.
   *
   *  Falls back to `single` rather than to `undefined` for the same reason
   *  `penWeight` falls back to the default weight: the engine would accept
   *  `undefined` and mean `single` anyway, and sending the value explicitly is
   *  what makes the control's state and the engine's argument the same thing. An
   *  unrecognised value — markup and this table disagreeing — is refused here
   *  rather than being sent for the engine to refuse, so the stroke still lands. */
  function penStyle() {
    const chosen = host.borderStyle?.value;
    return BORDER_STYLES.includes(chosen) ? chosen : DEFAULT_BORDER_STYLE;
  }

  const vAlignGroup = host.bindRadioGroup(host.vAlign, {
    attr: "data-valign",
    onSelect: (valign) => {
      host.formatRange((a, f) => host.doc().setCellVerticalAlignRange(a, f, valign));
      reflect();
    },
  });

  /** Reads the caret cell's current properties back onto the controls. */
  function reflect() {
    const doc = host.doc();
    const node = host.caretNode();
    if (!doc || !node) return;
    const rgb = doc.cellShadingAt(node);
    if (rgb >= 0 && document.activeElement !== host.shade) {
      host.shade.value = `#${rgb.toString(16).padStart(6, "0")}`;
    }
    vAlignGroup.reflect(doc.cellVerticalAlignAt(node) || "top");
    // The style ALREADY IN FORCE, which is the half that makes the dropdown a
    // control rather than a write-only switch. `cellBorderStyle` returns "" when
    // the cell has no border at all, and that is NOT a reason to move the control:
    // the pen keeps whatever the person last chose, so clearing a border and
    // drawing a new one does not silently revert to solid. A keyboard user with the
    // list open is left alone for the same reason the weight list is never refilled.
    const inForce = doc.cellBorderStyle(node);
    if (
      host.borderStyle &&
      BORDER_STYLES.includes(inForce) &&
      document.activeElement !== host.borderStyle
    ) {
      host.borderStyle.value = inForce;
    }
    const edges = doc.cellBorderEdges(node);
    const bit = { top: 1, bottom: 2, left: 4, right: 8 };
    for (const b of host.menu.querySelectorAll(".border-btn")) {
      const k = b.dataset.cellborder;
      const on = k === "box" ? edges === 0b1111 : k === "none" ? edges === 0 : (edges & bit[k]) !== 0;
      b.setAttribute("aria-pressed", String(on));
    }
  }

  host.shade.addEventListener("change", () => {
    const [r, g, b] = host.hexToRgb(host.shade.value);
    host.formatRange((a, f) => host.doc().setCellShadingRange(a, f, r, g, b, false));
  });
  host.onButton(host.shadeNone, () =>
    host.formatRange((a, f) => host.doc().setCellShadingRange(a, f, 0, 0, 0, true)),
  );
  for (const b of host.menu.querySelectorAll(".border-btn")) {
    host.onButton(b, () => {
      const [r, g, bl] = host.hexToRgb(host.cellBorderColor.value);
      host.formatRange((a, f) =>
        host.doc().setCellBorderRange(a, f, b.dataset.cellborder, r, g, bl, penWeight(), penStyle()),
      );
      reflect();
    });
  }
  for (const b of host.menu.querySelectorAll("[data-tableborder]")) {
    host.onButton(b, () => {
      const [r, g, bl] = host.hexToRgb(host.tableBorderColor.value);
      host.runNodeEdit((node) =>
        host.doc().setTableBorder(node, b.dataset.tableborder, r, g, bl, penWeight(), penStyle()),
      );
    });
  }

  return { reflect };
}

/**
 * Wires the split-cell dialog.
 *
 * Split cell had no keydown listener, no backdrop handler, no focus trap and no
 * Enter-to-confirm, and it closed onto `splitCellBtn` — a button inside a hidden
 * contextual ribbon panel, so `.focus()` was a no-op and the keyboard fell to
 * `<body>`, which is why the editor appeared frozen afterwards (HF-062). The
 * modal primitive supplies all of it; `fallbackFocus` is what catches the hidden
 * button, since the controller only restores focus to a control still on screen.
 *
 * @param {object} host the dialog's elements plus `doc()`, `caretNode()`,
 *        `runEdit`, `afterSplit()`, `onButton`, `registerModal`, `fallbackFocus`
 * @returns {{toggle: (open:boolean) => void}}
 */
export function bindSplitCellDialog(host) {
  const modal = host.registerModal(host.dialog, {
    initialFocus: () => host.columns,
    fallbackFocus: host.fallbackFocus,
    defaultAction: () => void apply(),
  });

  function toggle(open) {
    if (!open) {
      modal.close();
      return;
    }
    host.rows.value = "1";
    host.columns.value = "2";
    host.columns.setCustomValidity("");
    modal.open();
  }

  /** Splits the caret's merged cell into rows x columns. Refuses out-of-range
   *  input inline and keeps the dialog open with the typed values intact, rather
   *  than closing on a number the engine will not accept. */
  async function apply() {
    const node = host.caretNode();
    const doc = host.doc();
    if (!node || !doc) return;
    const rows = Number.parseInt(host.rows.value, 10);
    const columns = Number.parseInt(host.columns.value, 10);
    if (
      !Number.isInteger(rows) ||
      !Number.isInteger(columns) ||
      rows < 1 ||
      columns < 1 ||
      rows > 20 ||
      columns > 20
    ) {
      host.columns.setCustomValidity("Enter whole numbers from 1 to 20.");
      host.columns.reportValidity();
      return;
    }
    host.columns.setCustomValidity("");
    await host.runEdit(() => doc.splitMergedCell(node, rows, columns), { gate: true });
    toggle(false);
    host.afterSplit();
  }

  host.onButton(host.open, () => {
    if (host.caretNode()) toggle(true);
  });
  host.onButton(host.close, () => toggle(false));
  host.onButton(host.cancel, () => toggle(false));
  host.onButton(host.confirm, () => void apply());

  return { toggle };
}
