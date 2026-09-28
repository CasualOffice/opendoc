// The table command tree: the nineteen invocable `table.*` commands, built once
// and rendered by four surfaces (the Table menu, the canvas context menu, the
// command palette, and — through `command_taxonomy.mjs` — the palette's flat
// rows).
//
// Extracted from `main.js` rather than grown inside it, for the reason
// `table_band.mjs` and `view_zoom.mjs` already give: `main.js` is at its line
// ratchet with no slack (`module_seams`), and this is the largest self-contained
// declaration in the file that answers one question. Moving it also puts the
// command tree beside the band's enablement rules, which are the other half of
// the same subject — `docs/141` TBL-03 is precisely the defect that came from
// those two living apart.
//
// NOTHING here reaches for editor state. Every binding the tree needs arrives in
// `host`, so the module is a pure function of its context and can be built in a
// unit test with a stand-in engine. That is what lets `table_commands.test.mjs`
// assert that unmerge calls `splitMergedCell` with ONE argument — the whole of
// TBL-02 — without a browser.
import { t } from "./i18n.mjs";

/**
 * The table command tree for `context`.
 *
 * @param {object} context the shape `contextAt` produces: `{anchor, table,
 *        suggesting}`, where `table` is `plainTableInfo`'s five fields.
 * @param {object} host the editor bindings, all read at BUILD time so the tree
 *        reflects the state the menu is opening over:
 *        `doc()`, `tableSelection()`, `clearTableSelection()`,
 *        `plainTableInfo(node)`, `runEdit(thunk, options)`,
 *        `selectTableContext(node, mode)`, `openSplitCellDialog()`,
 *        `openCellFormat()`, `openTableProperties()`.
 * @returns {Array} the tree, submenu containers included.
 *
 * Complexity: O(1) in document size except for the one `plainTableInfo` a live
 * table selection costs, which is the pre-existing `tableInfo` walk (TBL-15).
 */
export function tableToolCommands(context, host) {
  const doc = host.doc();
  const tableSelection = host.tableSelection();
  const runEdit = host.runEdit;
  // Structural table edits rewrite the grid, which the engine cannot represent as
  // a tracked change, so Suggesting disables them with that as the stated reason.
  const structuralEnabled = !context.suggesting;
  const structuralReason = structuralEnabled
    ? ""
    : "This structural change cannot be tracked in Suggesting mode";
  const regular = context.table.regular;
  const selectedTable = tableSelection
    ? host.plainTableInfo(tableSelection.node)?.table
    : "";
  const hasTableSelection =
    !!selectedTable && selectedTable === context.table.table;
  // The same sentence the BAND now shows for the same precondition, from the one
  // catalogue entry (`table_band.mjs` `TABLE_BAND_REASON_KEYS`, `docs/141` TBL-03).
  const columnsReason = regular ? structuralReason : t("table.reason.merged");
  const tableMutation = (id, label, run, options = {}) => ({
    id,
    label,
    group: options.group ?? "op",
    enabled:
      structuralEnabled &&
      (options.regular !== true || regular) &&
      (options.enabled ?? true),
    disabledReason:
      !structuralEnabled
        ? structuralReason
        : options.regular === true && !regular
          ? columnsReason
          : options.enabled === false
            ? options.disabledReason
            : "",
    danger: options.danger,
    run,
  });

  const insertSubmenu = [
    tableMutation("table.insert.rowAbove", "Row above",
      () => runEdit(() => doc.insertRow(context.anchor.node, false), { gate: true }),
      { group: "row" }),
    tableMutation("table.insert.rowBelow", "Row below",
      () => runEdit(() => doc.insertRow(context.anchor.node, true), { gate: true }),
      { group: "row" }),
    tableMutation("table.insert.columnLeft", "Column left",
      () => runEdit(() => doc.insertColumn(context.anchor.node, false), { gate: true }),
      { regular: true, group: "col" }),
    tableMutation("table.insert.columnRight", "Column right",
      () => runEdit(() => doc.insertColumn(context.anchor.node, true), { gate: true }),
      { regular: true, group: "col" }),
  ];
  const deleteSubmenu = [
    tableMutation("table.delete.row", "Delete row",
      () => runEdit(() => doc.deleteRow(context.anchor.node), { gate: true }),
      { danger: true, group: "cell" }),
    tableMutation("table.delete.column", "Delete column",
      () => runEdit(() => doc.deleteColumn(context.anchor.node), { gate: true }),
      { danger: true, regular: true, group: "cell" }),
    tableMutation("table.delete.table", "Delete table",
      () => runEdit(() => doc.deleteTable(context.anchor.node), { gate: true }),
      { danger: true, group: "table" }),
  ];
  const selectSubmenu = [
    {
      id: "table.select.row",
      label: "Select row",
      group: "sel",
      run: () => host.selectTableContext(context.anchor.node, "row"),
    },
    {
      id: "table.select.column",
      label: "Select column",
      group: "sel",
      enabled: regular,
      disabledReason: regular ? "" : columnsReason,
      run: () => host.selectTableContext(context.anchor.node, "column"),
    },
    {
      id: "table.select.table",
      label: "Select table",
      group: "sel",
      run: () => host.selectTableContext(context.anchor.node, "table"),
    },
  ];
  const layoutSubmenu = [
    tableMutation("table.distribute.rows", "Distribute rows",
      () => runEdit(() => doc.distributeTableRows(context.anchor.node), { gate: true }),
      {
        regular: true,
        group: "distribute",
        enabled: ["exact", "atLeast"].includes(context.table.rowHeightRule),
        disabledReason: t("table.reason.rowHeights"),
      }),
    tableMutation("table.distribute.columns", "Distribute columns",
      () => runEdit(() => doc.distributeTableColumns(context.anchor.node), { gate: true }),
      { regular: true, group: "distribute" }),
    tableMutation("table.sort.ascending", "Sort ascending",
      () => runEdit(() => doc.sortTable(context.anchor.node, "ascending", context.table?.column ?? -1), { gate: true }),
      { regular: true, group: "sort" }),
    tableMutation("table.sort.descending", "Sort descending",
      () => runEdit(() => doc.sortTable(context.anchor.node, "descending", context.table?.column ?? -1), { gate: true }),
      { regular: true, group: "sort" }),
    // The KEYBOARD half of the table chrome layer's resize gestures (`docs/141`
    // D-1 §4.1.6). They are commands rather than a keydown branch because
    // `keymap.mjs` binds command IDS and nothing else: a chord that does not name
    // a real command is a build failure there, which is what stops an advertised
    // chord from being dead. Being commands also gets them onto the menu and the
    // palette for free, so the pointer gesture is not mouse-only capability.
    //
    // One step is 36 twips (0.025 in), the grid step the properties inspector's
    // own width and height fields move in — so the keyboard and the panel agree
    // about what "a bit wider" means.
    tableMutation("table.column.grow", "Widen column",
      () => host.stepTableBand("column", 1),
      { regular: true, group: "size" }),
    tableMutation("table.column.shrink", "Narrow column",
      () => host.stepTableBand("column", -1),
      { regular: true, group: "size" }),
    tableMutation("table.row.grow", "Taller row",
      () => host.stepTableBand("row", 1),
      { group: "size" }),
    tableMutation("table.row.shrink", "Shorter row",
      () => host.stepTableBand("row", -1),
      { group: "size" }),
  ];

  return [
    {
      id: "table.insert",
      label: "Insert",
      group: "table",
      icon: "tableInsert",
      submenu: insertSubmenu,
    },
    {
      id: "table.delete",
      label: "Delete",
      group: "table",
      icon: "tableDelete",
      submenu: deleteSubmenu,
    },
    tableMutation("table.merge", "Merge cells",
      async () => {
        await runEdit(() =>
          doc.mergeTableSelection(tableSelection.node, tableSelection.mode), { gate: true });
        host.clearTableSelection();
      },
      {
        group: "table",
        enabled: hasTableSelection,
        disabledReason: t("table.reason.mergeSelection"),
      }),
    // UNMERGE (`docs/141` TBL-02). `splitMergedCell(node)` with NO counts is what
    // reaches `split_table_cell`, the engine's real unmerge: it clears
    // `grid_span`/`vertical_merge` across the span and back-fills the vacated
    // cells. Every existing call site passed two integers, and the engine refuses
    // 1x1 ("choose more than one row or column to split the cell"), so no value a
    // person could type into the split dialog unmerged anything.
    //
    // THE ARITY IS THE FEATURE. The counts are `Option<u32>`, so omitting them is
    // how JavaScript says `None`, and `None`/`None` is the branch that unmerges.
    // Passing anything at all — a number, or `undefined` written out for
    // symmetry — lands in the split path instead. `table_commands.test.mjs`
    // asserts the argument list for that reason and not the return value.
    //
    // Offered whenever the table is IRREGULAR, i.e. holds a merge somewhere.
    // `TableInfo` reports no per-cell merge state (TBL-20), so "is THIS cell
    // merged" is not knowable here without a facade addition; a regular table can
    // still be refused up front rather than by the engine.
    tableMutation("table.unmerge", "Unmerge cells",
      () => runEdit(() => doc.splitMergedCell(context.anchor.node), { gate: true }),
      {
        group: "table",
        enabled: !regular,
        disabledReason: t("table.reason.noMergedCells"),
      }),
    tableMutation("table.split", "Split cell…",
      () => host.openSplitCellDialog(),
      { group: "table" }),
    {
      id: "table.select",
      label: "Select",
      group: "table-select",
      icon: "tableSelect",
      submenu: selectSubmenu,
    },
    {
      id: "table.layout",
      label: "Autofit & sort",
      group: "table-select",
      icon: "tableLayout",
      submenu: layoutSubmenu,
    },
    {
      id: "table.cellFormat",
      label: "Cell formatting…",
      group: "table-properties",
      icon: "paragraph",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => host.openCellFormat(),
    },
    {
      id: "table.properties",
      label: "Table properties…",
      group: "table-properties",
      icon: "settings",
      enabled: structuralEnabled,
      disabledReason: structuralReason,
      run: () => host.openTableProperties(),
    },
  ];
}
