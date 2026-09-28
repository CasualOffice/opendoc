// Insert ▸ Shapes, as Word draws it: a popover gallery of icon cells under
// category headings, arrow keys walking the grid, the shape's name on every
// cell as its tooltip AND its accessible name.
//
// The catalogue is `shape_catalogue.mjs`; this is the DOM and the keyboard. A
// gallery rather than 22 inline ribbon buttons, because the ribbon has to fit
// 1280px and because Word, ONLYOFFICE and LibreOffice all put shapes behind one
// trigger — a row of 22 buttons is not a smaller version of a gallery, it is a
// different and worse control.
//
// Cost: O(presets) to render, a fixed 22, once per open.
import { SHAPE_GROUPS, shapePreviewShape } from "./shape_catalogue.mjs";

const SVG_NS = "http://www.w3.org/2000/svg";

/** One preset's preview, as a real SVG node built from the catalogue outline.
 *  Never `innerHTML`: a preview built by string concatenation is an injection
 *  site that happens to be safe today. */
function previewFor(shape) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 20 20");
  svg.setAttribute("class", "shape-cell-preview");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  const spec = shapePreviewShape(shape.outline, 20);
  const node = document.createElementNS(SVG_NS, spec.tag);
  for (const [name, value] of Object.entries(spec.attrs)) node.setAttribute(name, String(value));
  svg.appendChild(node);
  return svg;
}

/**
 * Fills `menu` with the whole catalogue.
 *
 * @param {HTMLElement} menu the popover element
 * @param {{t: (key: string) => string, onPick: (token: string) => void}} io
 * @returns {HTMLElement[]} every cell, in keyboard order
 */
export function renderShapeGallery(menu, io) {
  menu.replaceChildren();
  const cells = [];
  for (const group of SHAPE_GROUPS) {
    const heading = document.createElement("div");
    heading.className = "menu-heading shape-group-heading";
    heading.id = `shapeGroup-${group.id}`;
    heading.textContent = io.t(group.key);
    menu.appendChild(heading);

    const grid = document.createElement("div");
    grid.className = "shape-grid";
    grid.setAttribute("role", "group");
    grid.setAttribute("aria-labelledby", heading.id);
    for (const shape of group.shapes) {
      const label = io.t(shape.key);
      const cell = document.createElement("button");
      cell.type = "button";
      cell.className = "shape-cell";
      cell.setAttribute("role", "menuitem");
      cell.dataset.shapeGeometry = shape.token;
      // Word shows the name as a tooltip on a picture-only cell. A tooltip is
      // not an accessible name, so the cell carries both — and the name is
      // localised, which is why it is a key rather than the OOXML token.
      cell.title = label;
      cell.setAttribute("aria-label", label);
      cell.appendChild(previewFor(shape));
      cell.addEventListener("click", () => io.onPick(shape.token));
      grid.appendChild(cell);
      cells.push(cell);
    }
    menu.appendChild(grid);
  }
  return cells;
}

/**
 * The cell the arrow keys move to.
 *
 * A gallery is a GRID, so Left/Right walk the flat order (which crosses a
 * category boundary the way Word's does) and Up/Down move by a row within the
 * current grid, falling into the neighbouring category when there is no row
 * above or below. Home/End go to the ends. Returns the new index, or -1 when
 * the key is not one this control owns.
 *
 * Pure arithmetic over the cell list, so `tests/shape_gallery.test.mjs` can
 * assert the whole walk without a browser.
 *
 * O(cells) worst case, a fixed 22.
 *
 * @param {string} key the `KeyboardEvent.key`
 * @param {number} index the focused cell
 * @param {number[]} rowStarts the flat index each grid row begins at
 * @param {number} count how many cells there are
 */
export function nextGalleryIndex(key, index, rowStarts, count) {
  if (count === 0) return -1;
  const at = index < 0 ? 0 : index;
  switch (key) {
    case "ArrowRight":
      return (at + 1) % count;
    case "ArrowLeft":
      return (at - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    case "ArrowDown":
    case "ArrowUp": {
      // The row this cell is in, and its column within it.
      let row = 0;
      for (let i = 0; i < rowStarts.length; i += 1) if (rowStarts[i] <= at) row = i;
      const column = at - rowStarts[row];
      const target = key === "ArrowDown" ? row + 1 : row - 1;
      if (target < 0 || target >= rowStarts.length) return at;
      const start = rowStarts[target];
      const end = target + 1 < rowStarts.length ? rowStarts[target + 1] : count;
      return Math.min(start + column, end - 1);
    }
    default:
      return -1;
  }
}

/**
 * Where each visual row of the gallery starts, given the cells and the columns
 * each category grid lays out. A category with seven shapes and four columns
 * contributes two rows.
 *
 * O(groups).
 */
export function galleryRowStarts(columns) {
  const starts = [];
  let flat = 0;
  for (const group of SHAPE_GROUPS) {
    for (let taken = 0; taken < group.shapes.length; taken += columns) {
      starts.push(flat + taken);
    }
    flat += group.shapes.length;
  }
  return starts;
}
