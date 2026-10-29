// The table tools beyond rows and columns: Insert Table by size, Convert Text to
// Table and Table to Text, Split Table, Insert and Delete Cells with a shift, the
// six table style options, and cell text direction (`docs/153` table rows;
// `109` RM-26, RM-27, RM-37).
//
// ## The competitive standard, and which one this follows
//
// * **Word** — Insert ▸ Table ▸ Insert Table… (typed rows and columns), Convert
//   Text to Table… (separator, column count, autofit), Layout ▸ Convert to Text
//   (separator), Layout ▸ Split Table (Ctrl+Shift+Enter), the Insert Cells and
//   Delete Cells dialogs (shift right/down, shift left/up, entire row, entire
//   column), Table Design ▸ Table Style Options (six checkboxes), and Layout ▸
//   Text Direction.
// * **ONLYOFFICE** — the same set: `Common.Views.InsertTableDialog`,
//   `TextToTableDialog.js:392-406` (paragraphs, tabs, semicolons, other; fixed,
//   contents, window), `TableToTextDialog.js:164-203` (plus "convert nested
//   tables"), the table break chord (`sdkjs/word/Editor/Shortcuts.js:188`),
//   `DE.Views.CellsRemoveDialog` (`locale/en.json:2269-2272`), the six banding
//   checkboxes in Table settings, and horizontal / rotate down / rotate up on the
//   cell menu (`DE.Views.DocumentHolder.directionText`).
// * **Google Docs** — none of the conversions, no cell shifts, no split; a
//   header-row pin in Table properties.
//
// **Followed: Word, with ONLYOFFICE's "convert nested tables" box**, because the
// two products that offer these agree, and Word's dialogs are the superset (it
// has commas and the shift-up / shift-down cell moves ONLYOFFICE lacks).
//
// ## What this module is
//
// The dialogs and the two ribbon popovers, and the functions the command tree in
// `table_commands.mjs` calls. Every edit is ONE engine call (`table_tools.rs` in
// `casual-doc-wasm`, over `casual_doc_command::table_tools`) through the editor's
// one gated `runEdit`, so each is one undo step and every refusal is the engine's
// own sentence. Nothing here reads the document except through those calls.
import { t } from "./i18n.mjs";

/** The six table style options, in Word's Table Style Options order. */
export const LOOK_OPTIONS = Object.freeze([
  "headerRow",
  "firstColumn",
  "totalRow",
  "lastColumn",
  "bandedRows",
  "bandedColumns",
]);

/** The three cell text directions ONLYOFFICE's cell menu offers. */
export const TEXT_DIRECTIONS = Object.freeze(["horizontal", "rotateDown", "rotateUp"]);

/** Each option's and direction's catalogue key: the command id that names it,
 *  so the menu row and the popover say the same words. */
export const LOOK_KEYS = Object.freeze(Object.fromEntries(LOOK_OPTIONS.map((option) => [option, "table.look." + option])));
export const DIRECTION_KEYS = Object.freeze(
  Object.fromEntries(TEXT_DIRECTIONS.map((direction) => [direction, "table.textDirection." + direction])),
);

/** Text separators, in Word's dialog order. */
export const SEPARATORS = Object.freeze(["paragraph", "tab", "comma", "semicolon", "other"]);

/** Autofit choices for Convert Text to Table. */
export const AUTOFIT = Object.freeze(["fixed", "contents", "window"]);

/** The Insert Cells dialog's four choices, Word's order. */
export const INSERT_SHIFTS = Object.freeze(["right", "down", "row", "column"]);

/** The Delete Cells dialog's four choices, Word's order. */
export const DELETE_SHIFTS = Object.freeze(["left", "up", "row", "column"]);

/** The engine's own bounds for a new table (`casual_doc_command::table::MAX_NEW_TABLE`)
 *  and for a converted one (`table_tools::MAX_COLUMNS`, Word's 63). */
export const MAX_NEW_ROWS = 50;
export const MAX_NEW_COLUMNS = 20;
export const MAX_CONVERTED_COLUMNS = 63;

/** `raw` as a whole number in `min..=max`, or `null`. A blank, a fraction or a
 *  number out of range is `null`, never clamped: the dialog says what it wants
 *  rather than inserting a table nobody typed. O(1). */
export function wholeNumberIn(raw, min, max) {
  const text = String(raw ?? "").trim();
  if (!/^\d+$/.test(text)) return null;
  const value = Number(text);
  return value >= min && value <= max ? value : null;
}

/** A separator choice as the engine's two arguments, or `null` for "other" with
 *  nothing typed (ONLYOFFICE's "You must type a character" case). The first
 *  CHARACTER is taken, not the first UTF-16 unit, so an astral symbol is one
 *  separator. O(length of `other`). */
export function separatorArgs(choice, other = "") {
  const separator = SEPARATORS.includes(choice) ? choice : "tab";
  if (separator !== "other") return { separator, character: "" };
  const character = [...String(other)][0] ?? "";
  return character.trim() ? { separator, character } : null;
}

/**
 * The one engine call an Insert/Delete Cells choice makes, as `[method, args]`.
 *
 * The two whole-band choices are the band commands that already exist (Word's
 * "entire row" inserts above, "entire column" to the left), so the dialog never
 * grows a second row-insert path. O(1).
 */
export function cellShiftCall(kind, choice, anchor, focus) {
  if (kind === "insert") {
    if (choice === "row") return ["insertRow", [anchor, false]];
    if (choice === "column") return ["insertColumn", [anchor, false]];
    return ["insertTableCells", [anchor, focus, choice === "down" ? "down" : "right"]];
  }
  if (choice === "row") return ["deleteRow", [anchor]];
  if (choice === "column") return ["deleteColumn", [anchor]];
  return ["deleteTableCells", [anchor, focus, choice === "up" ? "up" : "left"]];
}

/** The status-line sentence after each kind of edit. Keys, so a test without a
 *  catalogue can still read them. */
export const DONE_KEYS = Object.freeze({
  insertTable: "table.tools.inserted",
  textToTable: "table.tools.convertedFromText",
  tableToText: "table.tools.convertedToText",
  split: "table.tools.split",
  insertCells: "table.tools.cellsInserted",
  deleteCells: "table.tools.cellsDeleted",
});

/** The checked radio's value in `root`'s group `name`, or `fallback`. */
function radioValue(root, name, fallback) {
  return root?.querySelector(`input[name="${name}"]:checked`)?.value ?? fallback;
}

let active = null;

/**
 * The tools the command tree, the palette and the ribbon call. A facade over
 * whichever instance `createTableTools` built, so `table_commands.mjs` and the
 * palette can import it without the editor context; before the editor boots
 * every member is a no-op that returns what "nothing here" means.
 */
export const tableTools = Object.freeze({
  openDialog: (kind) => active?.openDialog(kind),
  splitTable: () => active?.splitTable() ?? Promise.resolve(false),
  toggleLook: (option, on) => active?.toggleLook(option, on) ?? Promise.resolve(false),
  lookOptions: () => active?.lookOptions() ?? [],
  textDirection: () => active?.textDirection() ?? "",
  setTextDirection: (direction) => active?.setTextDirection(direction) ?? Promise.resolve(false),
  reflectProperties: () => active?.reflectProperties(),
});

/**
 * Builds the dialogs and popovers and installs them behind `tableTools`.
 *
 * `io` is the whole contact with the application:
 *
 *   `doc()`                 the open document, or null
 *   `caretNode()`           the caret's paragraph id, or null
 *   `selectionEnds()`       `[anchorNode, focusNode]` of the text selection
 *   `cellRange()`           `[anchorNode, focusNode]` of the cell range, or null
 *   `runEdit(fn, options)`  Promise<boolean>; one gated, undoable engine call
 *   `status(text, kind)`    the status line
 *   `registerModal(el, options)`, `registerPopover(btn, menu, reflect)`,
 *   `closePopover(p)`, `onButton(el, fn)`
 *   `fallbackFocus()`       where focus goes when a dialog's opener is gone
 *   `afterEdit()`           re-syncs the chrome
 */
export function createTableTools(io) {
  const el = (id) => document.getElementById(id);

  /** The two corners a cell command acts on: the cell range when one is drawn
   *  in this table, else the caret's cell alone. */
  function cellCorners() {
    const range = io.cellRange();
    if (range?.[0]) return [range[0], range[1] ?? ""];
    const caret = io.caretNode();
    return caret ? [caret, ""] : null;
  }

  /** Runs one engine call and says so when it landed. */
  async function edit(thunk, doneKey) {
    const landed = await io.runEdit(thunk, { gate: true });
    if (landed) {
      io.status(t(doneKey));
      io.afterEdit();
    }
    return landed;
  }

  // ---- Dialogs --------------------------------------------------------------

  /** One dialog: its element, a modal controller and its apply function. */
  function dialog(id, { initialFocus, apply, prepare }) {
    const element = el(id);
    if (!element) return null;
    const modal = io.registerModal(element, {
      initialFocus: () => element.querySelector(initialFocus),
      fallbackFocus: io.fallbackFocus,
      defaultAction: () => void apply(element),
    });
    for (const button of element.querySelectorAll("[data-dialog-cancel]")) {
      io.onButton(button, () => modal.close());
    }
    for (const button of element.querySelectorAll("[data-dialog-apply]")) {
      io.onButton(button, () => void apply(element));
    }
    return {
      open() {
        if (!io.doc()) return;
        if (prepare && prepare(element) === false) return;
        modal.open();
      },
      close: () => modal.close(),
      element,
    };
  }

  /** Marks `input` invalid with `message` and keeps the dialog open. */
  function refuse(input, message) {
    input?.setCustomValidity(message);
    input?.reportValidity();
    return false;
  }

  const dialogs = {
    insertTable: dialog("tableInsertDialog", {
      initialFocus: "#tableInsertColumns",
      prepare(root) {
        for (const input of root.querySelectorAll("input")) input.setCustomValidity("");
        return Boolean(io.caretNode());
      },
      async apply(root) {
        const rowsInput = root.querySelector("#tableInsertRows");
        const columnsInput = root.querySelector("#tableInsertColumns");
        const columns = wholeNumberIn(columnsInput.value, 1, MAX_NEW_COLUMNS);
        if (columns === null) {
          return refuse(columnsInput, t("table.tools.columnsRange", { max: MAX_NEW_COLUMNS }));
        }
        columnsInput.setCustomValidity("");
        const rows = wholeNumberIn(rowsInput.value, 1, MAX_NEW_ROWS);
        if (rows === null) return refuse(rowsInput, t("table.tools.rowsRange", { max: MAX_NEW_ROWS }));
        rowsInput.setCustomValidity("");
        const node = io.caretNode();
        const doc = io.doc();
        if (!node || !doc) return false;
        const landed = await edit(() => doc.insertTable(node, rows, columns), DONE_KEYS.insertTable);
        if (landed) dialogs.insertTable.close();
        return landed;
      },
    }),

    textToTable: dialog("textToTableDialog", {
      initialFocus: "#textToTableColumns",
      prepare(root) {
        const ends = io.selectionEnds();
        const doc = io.doc();
        if (!ends || !doc) return false;
        const columns = suggestedColumns(root, ends);
        if (columns < 0) {
          io.status(t("table.tools.selectParagraphs"), "warn");
          return false;
        }
        root.querySelector("#textToTableColumns").value = String(columns);
        root.querySelector("#textToTableColumns").setCustomValidity("");
        return true;
      },
      async apply(root) {
        const ends = io.selectionEnds();
        const doc = io.doc();
        if (!ends || !doc) return false;
        const other = root.querySelector("#textToTableOther");
        const args = separatorArgs(radioValue(root, "textToTableSeparator", "tab"), other?.value);
        if (!args) return refuse(other, t("table.tools.otherEmpty"));
        other?.setCustomValidity("");
        const columnsInput = root.querySelector("#textToTableColumns");
        const columns = wholeNumberIn(columnsInput.value, 1, MAX_CONVERTED_COLUMNS);
        if (columns === null) {
          return refuse(columnsInput, t("table.tools.columnsRange", { max: MAX_CONVERTED_COLUMNS }));
        }
        columnsInput.setCustomValidity("");
        const autofit = radioValue(root, "textToTableAutofit", "fixed");
        const landed = await edit(
          () => doc.convertTextToTable(ends[0], ends[1], args.separator, args.character, columns, autofit),
          DONE_KEYS.textToTable,
        );
        if (landed) dialogs.textToTable.close();
        return landed;
      },
    }),

    tableToText: dialog("tableToTextDialog", {
      initialFocus: 'input[name="tableToTextSeparator"]:checked',
      prepare: () => Boolean(io.caretNode()),
      async apply(root) {
        const node = io.caretNode();
        const doc = io.doc();
        if (!node || !doc) return false;
        const other = root.querySelector("#tableToTextOther");
        const args = separatorArgs(radioValue(root, "tableToTextSeparator", "tab"), other?.value);
        if (!args) return refuse(other, t("table.tools.otherEmpty"));
        other?.setCustomValidity("");
        const nested = root.querySelector("#tableToTextNested")?.checked === true;
        const landed = await edit(
          () => doc.convertTableToText(node, args.separator, args.character, nested),
          DONE_KEYS.tableToText,
        );
        if (landed) dialogs.tableToText.close();
        return landed;
      },
    }),

    insertCells: cellsDialog("insert", "tableInsertCellsDialog", "tableInsertCellsShift"),
    deleteCells: cellsDialog("delete", "tableDeleteCellsDialog", "tableDeleteCellsShift"),
  };

  /** The Insert or Delete Cells dialog. */
  function cellsDialog(kind, id, group) {
    return dialog(id, {
      initialFocus: `input[name="${group}"]:checked`,
      prepare: () => Boolean(cellCorners()),
      async apply(root) {
        const corners = cellCorners();
        const doc = io.doc();
        if (!corners || !doc) return false;
        const choice = radioValue(root, group, kind === "insert" ? "right" : "left");
        const [method, args] = cellShiftCall(kind, choice, corners[0], corners[1]);
        const landed = await edit(
          () => doc[method](...args),
          kind === "insert" ? DONE_KEYS.insertCells : DONE_KEYS.deleteCells,
        );
        if (landed) dialogs[kind === "insert" ? "insertCells" : "deleteCells"].close();
        return landed;
      },
    });
  }

  /** The column count the text needs at the dialog's separator, or -1. */
  function suggestedColumns(root, ends) {
    const doc = io.doc();
    const args = separatorArgs(
      radioValue(root, "textToTableSeparator", "tab"),
      root.querySelector("#textToTableOther")?.value,
    );
    if (!doc || !args) return 1;
    return doc.textToTableColumns?.(ends[0], ends[1], args.separator, args.character) ?? -1;
  }

  // A different separator divides the text differently, so the column count the
  // dialog suggests follows it, as Word's does.
  const textToTable = dialogs.textToTable?.element;
  textToTable?.addEventListener("change", (event) => {
    if (!(event.target instanceof HTMLInputElement)) return;
    if (event.target.name !== "textToTableSeparator" && event.target.id !== "textToTableOther") return;
    const ends = io.selectionEnds();
    if (!ends) return;
    const columns = suggestedColumns(textToTable, ends);
    if (columns > 0) textToTable.querySelector("#textToTableColumns").value = String(columns);
  });

  // ---- Style options and text direction -------------------------------------

  /** The style options on for the caret's table. O(document) — read when a
   *  surface opens, never on a keystroke. */
  function lookOptions() {
    const node = io.caretNode();
    const doc = io.doc();
    if (!node || !doc) return [];
    return [...(doc.tableLookOptions?.(node) ?? [])];
  }

  async function toggleLook(option, on) {
    const node = io.caretNode();
    const doc = io.doc();
    if (!node || !doc || !LOOK_OPTIONS.includes(option)) return false;
    const landed = await io.runEdit(() => doc.setTableLookOption(node, option, on), { gate: true });
    if (landed) {
      // Concatenated, never templated: the option name is its own catalogue entry.
      const state = on ? t("table.tools.lookOn") : t("table.tools.lookOff");
      io.status(state + " " + t(LOOK_KEYS[option]));
      io.afterEdit();
      reflectAll();
    }
    return landed;
  }

  function textDirection() {
    const node = io.caretNode();
    const doc = io.doc();
    return node && doc ? (doc.cellTextDirectionAt?.(node) ?? "") : "";
  }

  async function setTextDirection(direction) {
    const corners = cellCorners();
    const doc = io.doc();
    if (!corners || !doc || !TEXT_DIRECTIONS.includes(direction)) return false;
    const landed = await io.runEdit(
      () => doc.setCellTextDirection(corners[0], corners[1], direction),
      { gate: true },
    );
    if (landed) {
      io.status(t("table.tools.directionSet") + " " + t(DIRECTION_KEYS[direction]));
      io.afterEdit();
      reflectAll();
    }
    return landed;
  }

  /** Renders a popover's rows as menu items carrying their state. */
  function renderChoices(menu, values, labelKey, current, role, run) {
    if (!menu) return;
    menu.replaceChildren();
    for (const value of values) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "menu-item";
      row.setAttribute("role", role);
      const on = current(value);
      row.setAttribute("aria-checked", String(on));
      row.dataset.value = value;
      const mark = document.createElement("span");
      mark.className = "menu-item-icon";
      mark.setAttribute("aria-hidden", "true");
      mark.textContent = on ? "✓" : "";
      const label = document.createElement("span");
      label.className = "menu-item-label";
      label.textContent = t(labelKey(value));
      row.append(mark, label);
      io.onButton(row, () => void run(value, on));
      menu.append(row);
    }
  }

  const lookButton = el("tableLookBtn");
  const lookMenu = el("tableLookMenu");
  const directionButton = el("tableTextDirectionBtn");
  const directionMenu = el("tableTextDirectionMenu");

  function reflectLookMenu() {
    const on = new Set(lookOptions());
    renderChoices(lookMenu, LOOK_OPTIONS, (option) => LOOK_KEYS[option], (option) => on.has(option),
      "menuitemcheckbox", (option, was) => toggleLook(option, !was));
  }

  function reflectDirectionMenu() {
    const current = textDirection() || "horizontal";
    renderChoices(directionMenu, TEXT_DIRECTIONS, (direction) => DIRECTION_KEYS[direction],
      (direction) => direction === current, "menuitemradio", (direction) => {
        if (directionPopover) io.closePopover(directionPopover);
        return setTextDirection(direction);
      });
  }

  const lookPopover = lookButton && lookMenu ? io.registerPopover(lookButton, lookMenu, reflectLookMenu) : null;
  const directionPopover =
    directionButton && directionMenu ? io.registerPopover(directionButton, directionMenu, reflectDirectionMenu) : null;

  // The Table properties panel's own six boxes (Word keeps them on Table Design,
  // ONLYOFFICE in Table settings; the inspector is this editor's settings panel).
  const propertyBoxes = [...document.querySelectorAll("input[data-table-look]")];
  for (const box of propertyBoxes) {
    box.addEventListener("change", (event) => {
      // The panel commits its OWN fields on change; these six are not among
      // them, and this listener is the only thing that writes them.
      event.stopPropagation();
      void toggleLook(box.dataset.tableLook, box.checked);
    });
  }

  function reflectProperties() {
    const on = new Set(lookOptions());
    for (const box of propertyBoxes) box.checked = on.has(box.dataset.tableLook);
  }

  function reflectAll() {
    reflectProperties();
    if (lookMenu && !lookMenu.hidden) reflectLookMenu();
    if (directionMenu && !directionMenu.hidden) reflectDirectionMenu();
  }

  // ---- Ribbon buttons -------------------------------------------------------
  const ribbon = [
    ["tableSplitBtn", () => void instance.splitTable()],
    ["tableToTextBtn", () => instance.openDialog("tableToText")],
    ["tableInsertCellsBtn", () => instance.openDialog("insertCells")],
    ["tableDeleteCellsBtn", () => instance.openDialog("deleteCells")],
  ];
  for (const [id, run] of ribbon) {
    const button = el(id);
    if (button) io.onButton(button, run);
  }

  const instance = {
    openDialog(kind) {
      dialogs[kind]?.open();
    },
    async splitTable() {
      const node = io.caretNode();
      const doc = io.doc();
      if (!node || !doc) return false;
      return edit(() => doc.splitTable(node), DONE_KEYS.split);
    },
    toggleLook,
    lookOptions,
    textDirection,
    setTextDirection,
    reflectProperties,
    get popovers() {
      return { lookPopover, directionPopover };
    },
  };
  active = instance;
  return instance;
}
