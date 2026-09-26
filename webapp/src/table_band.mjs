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
