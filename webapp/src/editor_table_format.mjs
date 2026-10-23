// Table and cell formatting from the ribbon: the cell format menu, the table
// popover and the breaks menu. runNodeEdit stays in main.js. Moved out of
// `main.js` unchanged (docs/177); it reaches the rest of the editor only
// through the context `ed`.
import { bindBreaksMenu } from "./break_commands.mjs";
import { bindRadioGroup } from "./radio_group.mjs";
import { onButton, registerPopover } from "./popover_manager.mjs";
import { bindCellFormatMenu } from "./table_cell_chrome.mjs";

/** Installs this domain on the editor context `ed` (docs/177) and returns what the rest
 *  of the editor calls. Runs once, from `main.js`, at the point this code used to sit. */
export function createTableFormat(ed) {
  // -- Table & cell formatting (a single-node edit: applies to the caret's cell) --


  // The cell-format popover and the split-cell dialog are `table_cell_chrome.mjs`;
  // what stays here is the editor state they need. `formatRange` is the one line
  // that removed the old *"put the caret in the cell to format it"* refusal: the
  // caret's own node passed twice is a one-cell range, so one path serves both.
  const cellFormatMenu = bindCellFormatMenu({
    menu: ed.tableFmtMenu,
    shade: document.getElementById("cellShade"),
    shadeNone: document.getElementById("cellShadeNone"),
    vAlign: document.getElementById("cellVAlign"),
    cellBorderColor: document.getElementById("cellBorderColor"),
    tableBorderColor: document.getElementById("tableBorderColor"),
    borderWeight: document.getElementById("borderWeight"),
    borderStyle: document.getElementById("borderStyle"),
    doc: () => ed.doc,
    caretNode: () => (ed.selection && ed.doc ? ed.selection.focus.node : ""),
    formatRange: (apply) =>
      ed.selection && ed.doc ? ed.tableRange.formatRange(apply, ed.selection.focus.node) : false,
    runNodeEdit: ed.runNodeEdit,
    onButton,
    bindRadioGroup,
    hexToRgb: ed.hexToRgb,
  });
  const tablePopover = registerPopover(ed.tableBtn, ed.tableFmtMenu, () => cellFormatMenu.reflect());
  bindBreaksMenu(ed.BREAK_IO);

  return { tablePopover };
}
