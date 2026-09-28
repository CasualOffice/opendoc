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
        host.doc().setCellBorderRange(a, f, b.dataset.cellborder, r, g, bl, 8),
      );
      reflect();
    });
  }
  for (const b of host.menu.querySelectorAll("[data-tableborder]")) {
    host.onButton(b, () => {
      const [r, g, bl] = host.hexToRgb(host.tableBorderColor.value);
      host.runNodeEdit((n) => host.doc().setTableBorder(n, b.dataset.tableborder, r, g, bl, 8));
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
