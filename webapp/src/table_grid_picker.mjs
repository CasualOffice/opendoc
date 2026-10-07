// Insert ▸ Table's size grid — the hover grid Word and Google Docs both open from
// their Insert Table button.
//
// Extracted from `main.js` (`109` HF-085) when its size label was found printing
// the same table two ways: "4 × 3" here and "3×4 table" on the Table band one
// click later. The fix is one formatter (`tableSizeLabel`, beside the band's own
// hint in `table_band.mjs`), and the extraction is what paid the `main.js` ratchet
// for the round that fixed it. The grid reached the application through exactly
// three things — the caret's insertion, the popover pair and the Viewing-mode
// refusal — so it arrives here as an `io` and nothing in it reads editor state.
//
// It is a real grid, not eighty anonymous buttons: rows carry `role="row"` (with
// `display: contents`, so the ten-column CSS grid is untouched), every cell has
// the size it inserts as its accessible name, and one roving tab stop plus arrow
// keys makes it navigable. Insertion lives on the cell's `click`, so the pointer
// and the keyboard travel the same path instead of the keyboard having none.
import { has, t } from "./i18n.mjs";
import { tableSizeLabel } from "./table_band.mjs";

/** The grid's extent: Word's own Insert Table grid is 10 × 8. */
export const GRID_ROWS = 8;
export const GRID_COLS = 10;

/**
 * Builds the grid into `io.picker` and wires it to its popover.
 *
 * @param {object} io
 * @param {HTMLElement} io.picker `#gridPicker`, the container the cells go in.
 * @param {HTMLElement} io.label `#gridLabel`, which previews the size under the pointer.
 * @param {HTMLElement} io.button `#insertTableBtn`.
 * @param {HTMLElement} io.menu `#insertTableMenu`.
 * @param {Function} io.registerPopover the editor's popover manager.
 * @param {Function} io.closePopover
 * @param {() => boolean} io.blocked the Viewing-mode refusal. It reports its own
 *        sentence; `true` means "refused, do not open".
 * @param {(rows: number, columns: number) => Promise<boolean>} io.insert inserts
 *        the table at the caret; `false` when there was nowhere to insert it.
 * @param {() => void} io.focusEditor puts the keyboard back on the document.
 * @returns {{ popover: object }}
 */
export function createTableGridPicker(io) {
  const { picker, label } = io;
  const cells = [];
  // The idle line is the markup's own, routed through its key, so a locale
  // change that relabels the menu relabels this too.
  const idleKey = label.dataset.i18n;
  const idleFallback = label.textContent;
  const idle = () => (idleKey && has(idleKey) ? t(idleKey) : idleFallback);

  picker.setAttribute("role", "grid");
  for (let r = 1; r <= GRID_ROWS; r++) {
    const row = picker.ownerDocument.createElement("div");
    row.setAttribute("role", "row");
    row.style.display = "contents";
    const line = [];
    for (let c = 1; c <= GRID_COLS; c++) {
      const cell = picker.ownerDocument.createElement("button");
      cell.type = "button";
      cell.className = "gc";
      cell.dataset.r = String(r);
      cell.dataset.c = String(c);
      cell.setAttribute("role", "gridcell");
      cell.tabIndex = r === 1 && c === 1 ? 0 : -1;
      row.appendChild(cell);
      line.push(cell);
    }
    picker.appendChild(row);
    cells.push(line);
  }
  /** The cell at 1-based `r`/`c`, or undefined outside the grid. */
  const cellAt = (r, c) => cells[r - 1]?.[c - 1];

  /** Names the grid and every cell in the language now in force. Run on OPEN
   *  rather than at construction: this is built while `main.js` is still being
   *  evaluated, before any catalogue is installed, and a locale change since the
   *  last opening must reach the eighty names too. O(cells) — eighty. */
  function relabel() {
    picker.setAttribute("aria-label", t("table.sizeGrid"));
    for (const line of cells) {
      for (const cell of line) {
        cell.setAttribute("aria-label", t("table.sizeSpoken", { columns: Number(cell.dataset.c), rows: Number(cell.dataset.r) }));
      }
    }
  }

  function highlight(rows, columns) {
    for (const line of cells) {
      for (const cell of line) {
        const on = Number(cell.dataset.r) <= rows && Number(cell.dataset.c) <= columns;
        cell.classList.toggle("on", on);
      }
    }
    label.textContent = rows ? tableSizeLabel(columns, rows) : idle();
  }

  /** Moves the single tab stop to `r`/`c`, previews that size, and focuses it —
   *  the roving-tabindex pattern, so Tab enters the grid once and the arrows do
   *  the rest. */
  function focusCell(r, c) {
    const cell = cellAt(r, c);
    if (!cell) return;
    for (const line of cells) for (const other of line) other.tabIndex = other === cell ? 0 : -1;
    highlight(r, c);
    cell.focus();
  }

  async function insertFrom(cell) {
    if (!cell) return;
    if (!(await io.insert(Number(cell.dataset.r), Number(cell.dataset.c)))) return;
    io.closePopover(popover);
    io.focusEditor();
  }

  const at = (event) => event.target.closest(".gc");
  picker.addEventListener("pointermove", (e) => {
    const cell = at(e);
    if (cell) highlight(Number(cell.dataset.r), Number(cell.dataset.c));
  });
  picker.addEventListener("pointerleave", () => highlight(0, 0));
  // Keep a pointer press from collapsing the document selection the table is
  // about to be inserted into; the click that follows is what inserts.
  picker.addEventListener("pointerdown", (e) => {
    if (at(e)) e.preventDefault();
  });
  picker.addEventListener("click", (e) => {
    const cell = at(e);
    if (cell) void insertFrom(cell);
  });
  picker.addEventListener("focusin", (e) => {
    const cell = at(e);
    if (cell) highlight(Number(cell.dataset.r), Number(cell.dataset.c));
  });
  picker.addEventListener("keydown", (e) => {
    const cell = at(e);
    if (!cell) return;
    const r = Number(cell.dataset.r);
    const c = Number(cell.dataset.c);
    const step = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1] }[e.key];
    if (step) {
      if (!cellAt(r + step[0], c + step[1])) return; // at an edge: stay put rather than wrapping to a different size
      e.preventDefault();
      focusCell(r + step[0], c + step[1]);
    } else if (e.key === "Home") {
      e.preventDefault();
      focusCell(e.ctrlKey || e.metaKey ? 1 : r, 1);
    } else if (e.key === "End") {
      e.preventDefault();
      focusCell(e.ctrlKey || e.metaKey ? GRID_ROWS : r, GRID_COLS);
    } else if (e.key === "Escape") {
      e.preventDefault();
      io.closePopover(popover);
      io.button.focus();
    }
    // Enter and Space need no handling: these are real buttons, so the browser
    // turns them into the same `click` the pointer path uses.
  });
  // The grid picker is a dialog-opening insert, so it refuses BEFORE it opens —
  // the same as Symbol, Emoji, Field and Drop cap, and for the reason
  // `insert-surface.spec.mjs` states for those: a reader must never be led into
  // choosing a size that cannot be applied. Registered ahead of `registerPopover`
  // so it runs first on the same button, and it stops there; the refusal itself is
  // still the editor's one choke point, not a disabled control.
  io.button.addEventListener("click", (event) => {
    if (!io.menu.hidden || !io.blocked()) return;
    event.preventDefault();
    event.stopImmediatePropagation();
  });
  const popover = io.registerPopover(io.button, io.menu, () => {
    // Opening puts the keyboard inside the grid at 1×1. Without this the popover
    // opened behind the focus ring and Tab walked past it into the rest of the
    // page, which is what made the whole picker pointer-only.
    relabel();
    focusCell(1, 1);
    highlight(0, 0);
  });
  return { popover };
}
