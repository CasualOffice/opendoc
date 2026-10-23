// The Table band, merge and split cells, the Table properties inspector, and
// the Insert table size grid. Moved out of `main.js` unchanged (docs/177); it
// reaches the rest of the editor only through the context `ed`.
import { registerModal } from "./modal.mjs";
import { bindRadioGroup } from "./radio_group.mjs";
import { onButton, closePopover, registerPopover } from "./popover_manager.mjs";
import { tablePropertiesPatch as tablePatch } from "./table_properties_patch.mjs";
import { optionalInchesToTwips, signedInchesToTwips, twipsToDialogInches } from "./units.mjs";
import { bindSplitCellDialog } from "./table_cell_chrome.mjs";
import { announceTableEdit, bindTableBand, tableContextLabel } from "./table_band.mjs";
import { createTableGridPicker } from "./table_grid_picker.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createTableProps(ed) {
  bindTableBand({
    root: ed.tableRibbon,
    onButton,
    getDoc: () => ed.doc,
    getSelection: () => ed.selection,
    runEdit: ed.runEdit,
    clearTableSelection: () => ed.tableRange.clear(),
    selectTableContext: ed.selectTableContext,
    caretTableColumn: ed.caretTableColumn,
    status: (text) => ed.setStatus(text),
  });

  onButton(ed.mergeCellsBtn, async () => {
    // The "select something first" branch that used to be here was UNREACHABLE —
    // the button is disabled whenever fewer than two cells are selected — and it
    // carried a second, divergent copy of the menu's own sentence. The band's
    // disabled reason is now that one sentence, from the catalogue (`docs/141`
    // TBL-03, TBL-05).
    //
    // `mergeTableCellRange` takes the RECTANGLE, not one of the three degenerate
    // shapes a mode string could name, so an arbitrary drag merges (TBL-17).
    const range = ed.tableRange.get();
    if (!ed.selection || !ed.doc || !range) return;
    await announceTableEdit(ed.runEdit(() => ed.doc.mergeTableCellRange(range.anchorNode, range.focusNode), { gate: true }), "merge", ed.setStatus);
    ed.tableRange.clear();
    ed.updateToolbar();
  });

  const splitCellDialogView = bindSplitCellDialog({
    dialog: ed.splitCellDialog,
    rows: ed.splitCellRows,
    columns: ed.splitCellColumns,
    open: ed.splitCellBtn,
    close: ed.splitCellClose,
    cancel: ed.splitCellCancel,
    confirm: ed.splitCellConfirm,
    fallbackFocus: () => ed.pagesEl,
    registerModal,
    onButton,
    runEdit: (fn, options) => ed.runEdit(fn, options),
    doc: () => ed.doc,
    caretNode: () => (ed.selection && ed.doc ? ed.selection.focus.node : ""),
    afterSplit: () => {
      ed.tableRange.clear();
      ed.updateToolbar();
    },
  });
  const toggleSplitCellDialog = (open) => splitCellDialogView.toggle(open);

  // -- Table properties inspector ----------------------------------------------
  let tablePropertiesCurrent = null;
  let tablePropertiesNode = null;

  /** An optional inches field's value → twips, or -1 for "leave it unset". */
  function updateTableRowHeightField() {
    const automatic = ed.tableRowHeightRule.value === "auto";
    ed.tableRowHeight.disabled = automatic;
    ed.tableRowHeight.required = !automatic;
    if (automatic) ed.tableRowHeight.setCustomValidity("");
  }

  function reflectTableProperties(node = ed.selection?.focus.node) {
    if (!ed.doc || node == null) return false;
    const info = ed.doc.tableInfo(node);
    if (!info.found) {
      info.free();
      return false;
    }

    tablePropertiesNode = node;
    ed.tablePropertiesContext.textContent = tableContextLabel(info);
    ed.tableCaption.value = info.caption || "";
    ed.tableDescription.value = info.description || "";
    ed.tableHeaderRow.checked = info.headerRow;
    ed.tableFixedLayout.checked = info.fixedLayout;
    ed.tableColumnWidth.disabled = false; // a merged table sizes its grid columns too (docs/171)
    ed.tableColumnWidth.value = twipsToDialogInches(info.columnWidthTwips);
    ed.tableColumnWidthNote.textContent = "Sets the width of the current column.";
    ed.tableWidth.value = twipsToDialogInches(info.tableWidthTwips);
    ed.tableIndent.value = twipsToDialogInches(info.tableIndentTwips);
    ed.tableRowHeight.value = twipsToDialogInches(info.rowHeightTwips);
    ed.tableRowHeightRule.value = info.rowHeightRule || "auto";
    ed.tableCellMargin.value = twipsToDialogInches(info.cellMarginTwips);
    ed.tableCellSpacing.value = twipsToDialogInches(info.cellSpacingTwips);
    tableAlignGroup.reflect(info.alignment);
    updateTableRowHeightField();

    tablePropertiesCurrent = {
      alignment: info.alignment,
      tableWidthTwips: optionalInchesToTwips(ed.tableWidth.value),
      tableIndentTwips: signedInchesToTwips(ed.tableIndent.value),
      fixedLayout: info.fixedLayout,
      headerRow: info.headerRow,
      columnWidthTwips: optionalInchesToTwips(ed.tableColumnWidth.value),
      rowHeightTwips:
        ed.tableRowHeightRule.value === "auto" ? -1 : optionalInchesToTwips(ed.tableRowHeight.value),
      rowHeightRule: info.rowHeightRule || "auto",
      cellMarginTwips: optionalInchesToTwips(ed.tableCellMargin.value),
      cellSpacingTwips: optionalInchesToTwips(ed.tableCellSpacing.value),
      caption: ed.tableCaption.value,
      description: ed.tableDescription.value,
    };
    info.free();
    return true;
  }

  function toggleTableProperties(open) {
    const show = open ?? ed.tablePropertiesPanel.hidden;
    if (show && !reflectTableProperties()) return;
    const returnFocus = !show && ed.tablePropertiesPanel.contains(document.activeElement);
    ed.tablePropertiesPanel.hidden = !show;
    ed.tablePropertiesBtn.setAttribute("aria-expanded", String(show));
    if (show) {
      ed.toggleParagraphProperties(false);
      closePopover(ed.tablePopover);
      queueMicrotask(() =>
        tableAlignGroup.focusSelected(),
      );
    } else if (returnFocus) {
      ed.tablePropertiesBtn.focus({ preventScroll: true });
    }
  }

  ed.tablePropertiesBtn.addEventListener("click", (event) => {
    event.stopPropagation();
    toggleTableProperties();
  });
  ed.tableFormulaApply.addEventListener("click", () => {
    if (!ed.selection || !ed.doc || !ed.tableFormula.value.trim()) return;
    ed.runNodeEdit((node) => ed.doc.calculateTableFormula(node, ed.tableFormula.value));
  });
  document.getElementById("tablePropertiesClose").addEventListener("click", () => toggleTableProperties(false));
  const tableAlignGroup = bindRadioGroup(ed.tableAlign, {
    attr: "data-talign",
    onSelect: () => commitTableProperties(),
  });
  ed.tablePropertiesPanel.addEventListener("change", (event) => {
    if (!(event.target instanceof HTMLInputElement || event.target instanceof HTMLSelectElement)) {
      return;
    }
    if (event.target === ed.tableRowHeightRule) updateTableRowHeightField();
    commitTableProperties();
  });

  /** The form's live fields, named once so the patch builder is a pure function of
   *  its bag rather than of this file's eighteen consts. */
  const TABLE_PROPERTIES_FORM = { width: ed.tableWidth, indent: ed.tableIndent, columnWidth: ed.tableColumnWidth, rowHeight: ed.tableRowHeight, rowHeightRule: ed.tableRowHeightRule, cellMargin: ed.tableCellMargin, cellSpacing: ed.tableCellSpacing, fixedLayout: ed.tableFixedLayout, headerRow: ed.tableHeaderRow, caption: ed.tableCaption, description: ed.tableDescription };
  /** Validation and the CHANGED-only diff live in `table_properties_patch.mjs`,
   *  where a test can reach them — `main.js` has no exports, so the diff that
   *  decides whether eleven untouched properties are re-asserted was unreachable
   *  from node. */
  function tablePropertiesPatch() {
    return tablePatch(TABLE_PROPERTIES_FORM, { alignment: tableAlignGroup.value() ?? "left", current: tablePropertiesCurrent, inchesToTwips: optionalInchesToTwips, signedInchesToTwips, rowHeightRequired: "Enter a row height or choose Auto." });
  }

  function commitTableProperties() {
    if (!ed.doc || !tablePropertiesCurrent || !tablePropertiesNode) return;
    const patch = tablePropertiesPatch();
    if (!patch) return;
    if (Object.keys(patch).length === 0) {
      return;
    }
    const applied = ed.runNodeEdit(() =>
      ed.doc.applyTableProperties(tablePropertiesNode, JSON.stringify(patch)),
    );
    if (applied) {
      const activeNode =
        ed.selection && ed.doc.inTable(ed.selection.focus.node) ? ed.selection.focus.node : null;
      if (activeNode == null) toggleTableProperties(false);
      else reflectTableProperties(activeNode);
      ed.setStatus("Table properties updated");
    }
  }
  document.addEventListener("keydown", (event) => {
    if (
      ed.tablePropertiesPanel.hidden ||
      (document.activeElement !== ed.tablePropertiesBtn &&
        !ed.tablePropertiesPanel.contains(document.activeElement))
    ) {
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      toggleTableProperties(false);
    }
  });

  // -- Insert table: the size grid (`table_grid_picker.mjs`) --------------------
  createTableGridPicker({
    picker: document.getElementById("gridPicker"),
    label: document.getElementById("gridLabel"),
    button: ed.insertTableBtn,
    menu: ed.insertTableMenu,
    registerPopover,
    closePopover,
    blocked: () => ed.blockMutationInViewing(),
    insert: async (rows, columns) => {
      if (!ed.selection || !ed.doc) return false;
      await ed.runEdit(() => ed.doc.insertTable(ed.selection.focus.node, rows, columns), { gate: true });
      return true;
    },
    focusEditor: ed.focusEditorSurface,
  });

  /** Rebuilds the off-screen accessibility mirror for the caret's part of the
   *  document. The projection lives in `a11y_mirror.mjs`; what stays here is the
   *  editor state it needs and the gated toggle a mirror checkbox runs (HF-178). */
  return { reflectTableProperties, toggleSplitCellDialog, toggleTableProperties };
}
