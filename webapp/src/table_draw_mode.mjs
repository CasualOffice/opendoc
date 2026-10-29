// SPDX-License-Identifier: Apache-2.0
// Draw Table: a pointer MODE (`docs/153` `table.draw`).
//
// ## The competitive standard
//
// * **Word** — Insert ▸ Table ▸ Draw Table, and Layout ▸ Draw Table: the
//   pointer becomes a pencil; dragging on the page draws a table's outline, and
//   dragging inside a table draws a row or column line that splits the cells it
//   crosses. The mode stays on until Esc or the button is pressed again.
// * **ONLYOFFICE** — Table ▸ Draw table: the same pencil, the same two
//   gestures (`sdkjs/word/Editor/Document.js` `DrawTable`; read for the
//   behaviour, nothing copied), with Esc leaving the mode.
// * **Google Docs** — no pencil.
//
// **Followed: Word.** A drawn outline becomes a one-cell table of the drawn size,
// FLOATING at the drawn place (`w:tblpPr` against the page, the engine's
// `drawTable`), inserted after the paragraph where the drag began. A drag that
// starts inside a table draws a line: mostly horizontal splits the row it
// crosses, mostly vertical splits the column at the line's x, and the cells the
// line does not cross stay whole (`drawTableLine`). Each gesture is one undo
// step. Deliberate difference from Word: an outline drawn INSIDE a table cell is
// a nested table in line with the cell's text, not a floating one, because the
// layout floats top-level body tables only.
//
// PERFORMANCE. The gesture reads nothing from the document while it runs: one
// hit test at the press, then every pointermove writes four numbers into one
// preview node. O(1) per move, in a document of any size.

/** Below this (in twips, 1/10 in) a press-and-release is a click, not a drag. */
export const MIN_DRAG = 144;

/** The rectangle between two points, normalised. O(1). */
export function drawnBox(start, end) {
  const left = Math.min(start.x, end.x);
  const top = Math.min(start.y, end.y);
  return { left, top, width: Math.abs(end.x - start.x), height: Math.abs(end.y - start.y) };
}

/** Whether a drag is a line along `vertical` or `horizontal`: whichever way it
 *  went further, which is how Word straightens a hand-drawn pencil line. O(1). */
export function lineOrientation(start, end) {
  return Math.abs(end.y - start.y) > Math.abs(end.x - start.x) ? "vertical" : "horizontal";
}

/** The straightened line a drag draws, as `[x0, y0, x1, y1]`. O(1). */
export function straightLine(start, end) {
  if (lineOrientation(start, end) === "vertical") {
    const x = Math.round((start.x + end.x) / 2);
    return [x, Math.round(start.y), x, Math.round(end.y)];
  }
  const y = Math.round((start.y + end.y) / 2);
  return [Math.round(start.x), y, Math.round(end.x), y];
}

/** Whether a point sits inside a `[page, x, y, w, h]` cell box on `page`. O(1). */
export function insideCell(box, page, point) {
  if (!box || box.length < 5 || box[0] !== page) return false;
  const [, x, y, w, h] = box;
  return point.x >= x && point.x <= x + w && point.y >= y && point.y <= y + h;
}

let active = null;

/** The mode as the ribbon, the menu, the palette and the pointer reach it. Every
 *  member is a no-op before the editor boots. */
export const tableDrawMode = Object.freeze({
  toggle: () => active?.toggle() ?? false,
  armed: () => active?.armed() ?? false,
  disarm: (announce) => active?.disarm(announce),
  dragging: () => active?.dragging() ?? false,
  tryBegin: (page, event) => active?.tryBegin(page, event) ?? false,
  update: (event) => active?.update(event),
  finish: () => active?.finish() ?? false,
});

/**
 * Builds the mode and installs it behind `tableDrawMode`.
 *
 * `io`: `pointToTwip(page, event)`, `scaleOf(page)`, `anchorAt(page, event)`,
 * `doc()`, `runEdit(fn, options)`, `status(text, kind)`, `t(key)`,
 * `buttons()` (the controls that toggle it, to reflect `aria-pressed`),
 * `blocked()` (true when the document may not change, with its own message),
 * `afterEdit()`.
 */
export function createTableDrawMode(io) {
  let on = false;
  let drag = null;

  function reflect() {
    for (const button of io.buttons()) button?.setAttribute("aria-pressed", String(on));
    if (on) document.body.dataset.tableDraw = "on";
    else delete document.body.dataset.tableDraw;
  }

  function disarm(announce = false) {
    if (drag) {
      drag.box.remove();
      drag = null;
    }
    if (!on) return;
    on = false;
    reflect();
    if (announce) io.status(io.t("tableDraw.off"));
  }

  function paint() {
    if (!drag) return;
    const { sx, sy } = io.scaleOf(drag.page);
    const style = drag.box.style;
    if (drag.kind === "line") {
      const [x0, y0, x1, y1] = straightLine(drag.start, drag.current);
      style.left = `${Math.min(x0, x1) * sx}px`;
      style.top = `${Math.min(y0, y1) * sy}px`;
      style.width = `${Math.max(1, Math.abs(x1 - x0) * sx)}px`;
      style.height = `${Math.max(1, Math.abs(y1 - y0) * sy)}px`;
      return;
    }
    const box = drawnBox(drag.start, drag.current);
    style.left = `${box.left * sx}px`;
    style.top = `${box.top * sy}px`;
    style.width = `${Math.max(1, box.width * sx)}px`;
    style.height = `${Math.max(1, box.height * sy)}px`;
  }

  // Esc leaves the mode, ahead of every other Escape handler: a mode you cannot
  // leave is the worst kind (the shape pencil's rule).
  document.addEventListener(
    "keydown",
    (event) => {
      if (event.key !== "Escape" || !on) return;
      event.preventDefault();
      event.stopPropagation();
      disarm(true);
    },
    true,
  );

  const instance = {
    toggle() {
      if (on) {
        disarm(true);
        return false;
      }
      if (!io.doc() || io.blocked()) return false;
      on = true;
      reflect();
      io.status(io.t("tableDraw.armed"));
      return true;
    },
    armed: () => on,
    disarm,
    dragging: () => !!drag,

    /** Begins a gesture; false when the mode is off, so the press falls through. */
    tryBegin(page, event) {
      if (!on || event.button !== 0) return false;
      const doc = io.doc();
      if (!doc) return false;
      const start = io.pointToTwip(page, event);
      const anchor = io.anchorAt(page, event);
      const node = anchor?.node ?? "";
      const inCell = !!node && doc.inTable(node) && insideCell([...(doc.cellRect?.(node) ?? [])], page.pageNumber, start);
      const box = document.createElement("div");
      box.className = inCell ? "table-draw-line" : "table-draw-preview";
      box.setAttribute("aria-hidden", "true");
      page.overlay?.appendChild(box);
      drag = { page, start, current: start, kind: inCell ? "line" : "table", node, box };
      paint();
      event.preventDefault();
      page.overlay?.setPointerCapture?.(event.pointerId);
      return true;
    },

    update(event) {
      if (!drag) return;
      drag.current = io.pointToTwip(drag.page, event);
      paint();
    },

    /** Finishes the gesture: one engine call, then the mode stays on for the
     *  next line, as Word's pencil does. */
    finish() {
      if (!drag) return false;
      const { page, start, current, kind, node, box } = drag;
      box.remove();
      drag = null;
      const doc = io.doc();
      const moved = Math.max(Math.abs(current.x - start.x), Math.abs(current.y - start.y));
      if (!doc || !node || moved < MIN_DRAG) {
        io.status(io.t("tableDraw.armed"));
        return true;
      }
      if (kind === "line") {
        const line = Int32Array.from([page.pageNumber, ...straightLine(start, current)]);
        void io.runEdit(() => doc.drawTableLine(node, line), { gate: true }).then((landed) => {
          if (landed) {
            io.status(io.t("tableDraw.split"));
            io.afterEdit();
          }
        });
      } else {
        const r = drawnBox(start, current);
        const rect = Int32Array.from([r.left, r.top, r.width, r.height].map(Math.round));
        void io.runEdit(() => doc.drawTable(node, rect), { gate: true }).then((landed) => {
          if (landed) {
            io.status(io.t("tableDraw.drawn"));
            io.afterEdit();
          }
        });
      }
      return true;
    },
  };
  active = instance;
  reflect();
  return instance;
}
