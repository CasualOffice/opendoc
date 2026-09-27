// The contextual Table band's structural controls.
//
// It lives in a module rather than in `main.js` for the reason `view_zoom.mjs`
// gives: `main.js` is at its line ratchet with no slack (`module_seams`), and
// four near-identical hand-written delegated loops are exactly what the ratchet
// exists to refuse. Moving them here also removes the one place where the band
// had its own opinion: its Select handler re-implemented `selectTableContext`
// (the function the Table menu, the right-click menu and the palette all call)
// line for line, which is two implementations of one rule — the shape `109`
// UX-005 exists to end.
//
// The operations are declared in the MARKUP (`data-table-action`,
// `data-table-distribute`, `data-table-sort`, `data-table-select`) and resolved
// through the tables below, so a button added to the band with an operation this
// module does not know ships DISABLED rather than live-and-inert: "never a dead
// control" (SKILL.md §10). `table_band.test.mjs` fails if the markup and these
// tables disagree in either direction.

/** Insert and delete, by `data-table-action`. Each entry takes the engine and the
 *  caret's node, so the table is data: nothing here closes over editor state. */
export const TABLE_ROW_COLUMN_ACTIONS = Object.freeze({
  "insert-row-above": (doc, node) => doc.insertRow(node, false),
  "insert-row-below": (doc, node) => doc.insertRow(node, true),
  "insert-column-left": (doc, node) => doc.insertColumn(node, false),
  "insert-column-right": (doc, node) => doc.insertColumn(node, true),
  "delete-row": (doc, node) => doc.deleteRow(node),
  "delete-column": (doc, node) => doc.deleteColumn(node),
  "delete-table": (doc, node) => doc.deleteTable(node),
});

/** Even distribution, by `data-table-distribute`. */
export const TABLE_DISTRIBUTE_ACTIONS = Object.freeze({
  rows: (doc, node) => doc.distributeTableRows(node),
  columns: (doc, node) => doc.distributeTableColumns(node),
});

/** A Table-band control's precondition BEYOND "the caret is in a table", by the
 *  selector that names it, most specific first.
 *
 *  `docs/141` TBL-03: band buttons shipped disabled with no stated reason while
 *  the very same commands explain themselves in the Table menu, so a merged
 *  table presented a grey Sort button whose tooltip still read "Sort rows
 *  ascending" — a disabled control with no reason cannot be told from a broken
 *  one (`SKILL.md` §10). The rules live here, next to the operations they gate,
 *  rather than as five hand-written loops inside `updateToolbar`.
 *
 *  `[data-table-select="column"]` is in this table because the MENU already
 *  gates `table.select.column` on a regular grid and the band did not — one
 *  capability answering differently on two surfaces is the parity defect
 *  `109` UX-005 exists to end, not a separate feature.
 */
export const TABLE_BAND_PRECONDITIONS = Object.freeze([
  ['[data-table-action*="column"]', "regular"],
  ['[data-table-select="column"]', "regular"],
  ['[data-table-distribute="rows"]', "rowHeights"],
  ['[data-table-distribute="columns"]', "regular"],
  ["[data-table-sort]", "regular"],
  ["#mergeCellsBtn", "cellSelection"],
]);

/** Precondition -> the catalogue key for the sentence that explains it.
 *
 *  KEYS, not sentences: this module is imported by a unit test with no DOM and
 *  no catalogue, and the band's reasons have to be the SAME sentences the Table
 *  menu shows. `en_strings.mjs` holds each of them once. */
export const TABLE_BAND_REASON_KEYS = Object.freeze({
  inTable: "table.reason.caretOutsideTable",
  regular: "table.reason.merged",
  rowHeights: "table.reason.rowHeights",
  cellSelection: "table.reason.mergeSelection",
});

/**
 * Every button in the band, with whether it is available and — when it is not —
 * the catalogue key for the reason.
 *
 * "The caret is in a table" is checked first and reported first: with the caret
 * in a paragraph, "Unavailable for merged or spanned tables" would be true of
 * nothing and would send the reader looking for a merge.
 *
 * @param {{querySelectorAll: Function}} root the band element.
 * @param {{inTable?: boolean, regular?: boolean, rowHeightRule?: string,
 *          hasCellSelection?: boolean}} context what the caret's table is,
 *        taken from the ONE `tableInfo` the caller already holds.
 * @returns {Array<{control: object, enabled: boolean, reasonKey: string}>}
 *
 * Complexity: O(controls in the band) — nineteen — and it reads no document.
 * The caller's single `tableInfo` stays the only engine call on this path.
 */
export function tableBandStates(root, context) {
  const inTable = context.inTable === true;
  const met = {
    inTable,
    regular: context.regular === true,
    rowHeights: ["exact", "atLeast"].includes(context.rowHeightRule),
    cellSelection: context.hasCellSelection === true,
  };
  return [...root.querySelectorAll("button")].map((control) => {
    const requires = TABLE_BAND_PRECONDITIONS.find(([selector]) =>
      control.matches(selector),
    )?.[1];
    const unmet = !inTable ? "inTable" : requires && !met[requires] ? requires : "";
    return {
      control,
      enabled: !unmet,
      reasonKey: unmet ? TABLE_BAND_REASON_KEYS[unmet] : "",
    };
  });
}

/** Wires the band.
 *
 *  Everything the handlers need is injected, so the module never reaches for a
 *  module-scope binding in `main.js` and can be driven from a test with a
 *  stand-in engine.
 *
 *  Complexity: O(controls in the band) once at boot; each handler reads the caret
 *  and nothing else, so activation is O(1) in document size.
 */
export function bindTableBand({
  root,
  onButton,
  getDoc,
  getSelection,
  runEdit,
  clearTableSelection,
  selectTableContext,
  caretTableColumn,
}) {
  /** The caret's node, or null when there is nothing to act on. */
  const caretNode = () => (getDoc() && getSelection() ? getSelection().focus.node : null);

  /** Binds `control` to an operation resolved from `table`, or disables it. */
  const bind = (control, table, key, handler) => {
    const operation = table[key];
    if (!operation) {
      control.disabled = true;
      return;
    }
    onButton(control, () => {
      const node = caretNode();
      if (!node) return;
      handler(operation, node);
    });
  };

  for (const control of root.querySelectorAll("[data-table-action]")) {
    bind(control, TABLE_ROW_COLUMN_ACTIONS, control.dataset.tableAction, (operation, node) => {
      // A structural rewrite invalidates a row/column/table selection, so the
      // accent fill is dropped before the edit rather than left claiming a
      // selection the new grid no longer has.
      clearTableSelection();
      runEdit(() => operation(getDoc(), node), { gate: true });
    });
  }

  for (const control of root.querySelectorAll("[data-table-distribute]")) {
    bind(control, TABLE_DISTRIBUTE_ACTIONS, control.dataset.tableDistribute, (operation, node) => {
      clearTableSelection();
      runEdit(() => operation(getDoc(), node), { gate: true });
    });
  }

  for (const control of root.querySelectorAll("[data-table-sort]")) {
    const direction = control.dataset.tableSort;
    // Sort takes a direction rather than an operation, and the engine validates
    // it, so there is no table to look it up in — but an empty attribute is still
    // a markup bug and still ships disabled.
    if (!direction) {
      control.disabled = true;
      continue;
    }
    onButton(control, () => {
      const node = caretNode();
      if (!node) return;
      const column = caretTableColumn(node);
      runEdit(() => getDoc().sortTable(node, direction, column), { gate: true });
    });
  }

  for (const control of root.querySelectorAll("[data-table-select]")) {
    const mode = control.dataset.tableSelect;
    if (!mode) {
      control.disabled = true;
      continue;
    }
    // The SAME function the Table menu, the right-click menu and the palette's
    // `table.select.*` rows call. The band used to inline its own copy.
    onButton(control, () => {
      const node = caretNode();
      if (node) selectTableContext(node, mode);
    });
  }
}
