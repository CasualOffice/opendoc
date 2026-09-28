// Word's Shape Fill and Shape Outline menus — the two live controls its Shape
// Format tab is built around.
//
// Out of `main.js` with the drawing work, to pay for it under the line ratchet.
// It is the same 50 lines it always was; what it gains is an `io` seam, so the
// swatch helpers, the palette and the selected shape's own format are supplied
// rather than reached for:
//
//   `menu(kind)`     the popover element for "fill" or "outline"
//   `format()`       the selected shape's `{fill, outline, outlineWidthEmu}`
//   `heading(text)`  a menu heading element
//   `swatchGrid(cells)` / `swatchCell(...)`  the shared colour-grid builders
//   `colors()`       the standard palette
//   `weights()`      Word's outline weights, in points
//   `emuPerPoint`
//
// Cost: O(colors + weights), a fixed ~30 nodes, once per open.

/** Renders a shape color menu: the reset row Word leads with ("No fill" /
 *  "No outline"), the standard palette, and — for the outline — its weights. */
export function renderShapeMenu(kind, io) {
  const menu = io.menu(kind);
  const format = io.format();
  const active = (kind === "fill" ? format.fill : format.outline)?.toLowerCase() ?? null;
  menu.replaceChildren();

  const reset = document.createElement("button");
  reset.type = "button";
  reset.className = "color-row-action";
  reset.dataset.shapeNone = "1";
  reset.innerHTML =
    '<span class="color-chip color-chip-none"></span>' +
    `<span>${kind === "fill" ? "No fill" : "No outline"}</span>`;
  menu.appendChild(reset);

  menu.appendChild(io.heading("Standard colors"));
  menu.appendChild(
    io.swatchGrid(
      io.colors().map((hex) => {
        const cell = io.swatchCell("text", hex, hex, hex.toUpperCase(), hex.toLowerCase() === active);
        // The grid helper tags cells for the TEXT handler; retag so a shape click
        // cannot be mistaken for a text-color one.
        delete cell.dataset.color;
        cell.dataset.shapeColor = hex;
        return cell;
      }),
    ),
  );

  if (kind === "outline") {
    const current = format.outlineWidthEmu ?? null;
    menu.appendChild(io.heading("Weight"));
    const list = document.createElement("div");
    list.className = "shape-weight-list";
    for (const points of io.weights()) {
      const emu = Math.round(points * io.emuPerPoint);
      const row = document.createElement("button");
      row.type = "button";
      row.className = "color-row-action shape-weight";
      row.dataset.shapeWeight = String(emu);
      if (current === emu) row.classList.add("is-active");
      row.innerHTML =
        `<span class="shape-weight-rule" style="--w:${Math.max(1, points)}px" aria-hidden="true"></span>` +
        `<span>${points} pt</span>`;
      list.appendChild(row);
    }
    menu.appendChild(list);
  }
}
