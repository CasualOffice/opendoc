// SPDX-License-Identifier: Apache-2.0
// Word's Edit Points on a shape (`docs/153` object.edit-points, `docs/109`
// FID-SH-02).
//
// Right-click a shape ▸ Edit Points: the outline's vertices appear as small
// black squares on the shape; dragging one reshapes the outline live; Enter,
// clicking away or choosing Edit Points again keeps the new outline as ONE
// undoable action, and Esc puts it back as it was. That is Word's interaction
// (PowerPoint's is the same); ONLYOFFICE has no Edit Points for document
// shapes.
//
// The engine converts the shape — a preset at its current size and adjust
// values, or an authored freeform — into literal path commands in the shape's
// own EMU box (`casual-doc-wasm` `shape_points.rs`, `shapeEditPoints`), and
// writes the edited commands back as a custom geometry (`setShapeEditPoints`).
// Nothing is written while dragging: the session drags its own copy and
// previews it as an SVG outline over the page, so a drag is O(points) per
// pointer move and never touches the document.
//
// Moving a vertex moves the Bézier control points attached to it by the same
// amount, so a curve keeps its shape around the point rather than kinking —
// Word's "smooth point" default.
//
// What this does NOT do yet, said rather than hidden: adding and deleting
// points, changing a segment between straight and curved, and dragging the
// control handles themselves (Word's right-click-on-a-point menu); and a point
// dragged outside the box is drawn outside it rather than growing the box.
// Edit Points on a ROTATED shape is refused with a reason (the drag would
// need the inverse rotation; the shape keeps its outline).
import { beginGripDrag } from "./object_crop_chrome.mjs";
import { t } from "./i18n.mjs";

const SVG_NS = "http://www.w3.org/2000/svg";

/**
 * Every vertex of `points`: the last point of each command that has one.
 * O(commands).
 *
 * @param {{paths: {commands: {type: string, points: number[][]}[]}[]}} points
 * @returns {{path: number, command: number}[]}
 */
export function vertices(points) {
  const out = [];
  points.paths.forEach((path, p) => {
    path.commands.forEach((command, c) => {
      if (command.points.length) out.push({ path: p, command: c });
    });
  });
  return out;
}

/**
 * `points` with one vertex moved by (`dx`, `dy`) EMU, and the Bézier control
 * points that touch it moved with it: the entering control (the cubic's second
 * control point of the same command) and the leaving one (the next command's
 * first control point, if it is a cubic). A closed path whose last vertex sits
 * on its start moves both together, so closing the outline cannot tear it.
 * Pure; O(commands of that path).
 */
export function moveVertex(points, ref, dx, dy) {
  const next = structuredClone(points);
  const commands = next.paths[ref.path].commands;
  const shift = (pt) => [pt[0] + dx, pt[1] + dy];
  const at = commands[ref.command];
  const original = at.points[at.points.length - 1];
  at.points[at.points.length - 1] = shift(original);
  if (at.type === "cubicTo") at.points[1] = shift(at.points[1]);
  const following = commands[ref.command + 1];
  if (following?.type === "cubicTo") following.points[0] = shift(following.points[0]);
  // A closed subpath that draws back onto its start before closing (most
  // presets do) has the same point twice: the `moveTo` and the vertex before
  // the `close`. They are one point to the person dragging it.
  const start = commands.findLastIndex((c, i) => i <= ref.command && c.type === "moveTo");
  let last = start;
  while (last + 1 < commands.length && commands[last + 1].type !== "moveTo" && commands[last + 1].type !== "close") last += 1;
  const closed = commands[last + 1]?.type === "close";
  const same = (pt) => Math.abs(pt[0] - original[0]) < 0.5 && Math.abs(pt[1] - original[1]) < 0.5;
  if (closed && last > start) {
    const tail = commands[last];
    const tailEnd = tail.points.length - 1;
    if (ref.command === start && same(tail.points[tailEnd])) {
      tail.points[tailEnd] = shift(tail.points[tailEnd]);
      if (tail.type === "cubicTo") tail.points[1] = shift(tail.points[1]);
    } else if (ref.command === last && same(commands[start].points[0])) {
      commands[start].points[0] = shift(commands[start].points[0]);
      const leaving = commands[start + 1];
      if (leaving?.type === "cubicTo") leaving.points[0] = shift(leaving.points[0]);
    }
  }
  return next;
}

/**
 * Maps a point in the shape's EMU box onto the placed box (`[x, y, w, h]`
 * page twips), honouring the shape's flips. O(1).
 */
export function toPlaced(point, points, box) {
  const [bx, by, bw, bh] = box;
  const fx = points.widthEmu > 0 ? point[0] / points.widthEmu : 0;
  const fy = points.heightEmu > 0 ? point[1] / points.heightEmu : 0;
  return [bx + (points.flipH ? 1 - fx : fx) * bw, by + (points.flipV ? 1 - fy : fy) * bh];
}

/** A pointer delta in page twips as an EMU delta in the shape's box. O(1). */
export function toShapeDelta(dxTwips, dyTwips, points, box) {
  const [, , bw, bh] = box;
  const dx = bw > 0 ? (dxTwips / bw) * points.widthEmu : 0;
  const dy = bh > 0 ? (dyTwips / bh) * points.heightEmu : 0;
  return [points.flipH ? -dx : dx, points.flipV ? -dy : dy];
}

/** An SVG path `d` for `points` placed on the page at `scale`. O(points). */
export function outlinePath(points, box, scale) {
  const px = (pt) => {
    const [x, y] = toPlaced(pt, points, box);
    return `${(x * scale.sx).toFixed(2)} ${(y * scale.sy).toFixed(2)}`;
  };
  const letter = { moveTo: "M", lineTo: "L", quadTo: "Q", cubicTo: "C" };
  return points.paths
    .flatMap((path) => path.commands.map((c) => (c.type === "close" ? "Z" : `${letter[c.type]} ${c.points.map(px).join(" ")}`)))
    .join(" ");
}

/**
 * The Edit Points session.
 *
 * @param {object} ed the editor context: `doc`, `objectSelection`, `reviewMode`,
 *        `pages`, `scaleOf(page)`, `blockMutationInViewing()`, `runEdit()`,
 *        `setStatus()`, `focusEditorSurface()`.
 */
export function createEditPoints(ed) {
  /** `{ node, points, start, page, box, layer }` while editing, else null. */
  let session = null;

  function teardown() {
    if (!session) return;
    session.layer.remove();
    window.removeEventListener("keydown", onKey, true);
    document.removeEventListener("pointerdown", onOutside, true);
    session = null;
  }

  function onKey(event) {
    if (!session) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      cancel();
    } else if (event.key === "Enter") {
      event.preventDefault();
      event.stopPropagation();
      void commit();
    } else if (event.key === "Delete" || event.key === "Backspace") {
      // Word deletes the POINT here; until that lands, the key must not reach
      // the editor and delete the whole shape out from under the session.
      event.preventDefault();
      event.stopPropagation();
    } else if (event.metaKey || event.ctrlKey) {
      // An application chord (undo, save, select all) acts on the document as
      // it is: leave the mode first, with nothing changed.
      cancel();
    }
  }

  function onOutside(event) {
    if (session && !session.layer.contains(event.target)) void commit();
  }

  function paint() {
    const s = session;
    const scale = ed.scaleOf(s.page);
    s.layer.replaceChildren();
    const svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("class", "shape-points-outline");
    svg.setAttribute("aria-hidden", "true");
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", outlinePath(s.points, s.box, scale));
    svg.appendChild(path);
    s.layer.appendChild(svg);
    for (const ref of vertices(s.points)) {
      const command = s.points.paths[ref.path].commands[ref.command];
      const [x, y] = toPlaced(command.points[command.points.length - 1], s.points, s.box);
      const grip = document.createElement("div");
      grip.className = "shape-points-handle";
      grip.style.left = `${x * scale.sx}px`;
      grip.style.top = `${y * scale.sy}px`;
      grip.addEventListener("pointerdown", (event) => drag(event, ref));
      s.layer.appendChild(grip);
    }
  }

  function drag(event, ref) {
    const s = session;
    if (!s) return;
    const from = { x: event.clientX, y: event.clientY, points: s.points };
    beginGripDrag(event, {
      onMove: (move) => {
        if (!session) return;
        const scale = ed.scaleOf(s.page);
        const [dx, dy] = toShapeDelta((move.clientX - from.x) / scale.sx, (move.clientY - from.y) / scale.sy, s.points, s.box);
        s.points = moveVertex(from.points, ref, dx, dy);
        paint();
        move.preventDefault();
      },
      onEnd: (cancelled) => {
        if (!session) return;
        if (cancelled) s.points = from.points;
        paint();
      },
    });
  }

  /** Keeps the edited outline as one undoable action, and leaves the mode. */
  async function commit() {
    const s = session;
    if (!s) return;
    const changed = JSON.stringify(s.points) !== JSON.stringify(s.start);
    teardown();
    if (!changed) {
      ed.setStatus("");
      return;
    }
    const applied = await ed.runEdit(() => ed.doc.setShapeEditPoints(s.node, JSON.stringify(s.points)), { gate: true });
    if (applied) ed.setStatus(t("object.editPoints.done"));
  }

  /** Leaves the mode with nothing changed. */
  function cancel() {
    if (!session) return;
    teardown();
    ed.setStatus(t("object.editPoints.cancelled"));
    ed.focusEditorSurface?.();
  }

  /** Right-click ▸ Edit Points: enters the mode on the selected shape, or, if
   *  it is already on, keeps the edit (Word's toggle). */
  function toggle() {
    if (session) {
      void commit();
      return;
    }
    const selection = ed.objectSelection;
    if (!ed.doc || !selection || selection.kind !== "shape") return;
    if (ed.blockMutationInViewing()) return;
    if (ed.reviewMode === "suggesting") {
      ed.setStatus(t("object.editPoints.suggesting"), "error");
      return;
    }
    let points;
    try {
      points = JSON.parse(ed.doc.shapeEditPoints(selection.node));
    } catch {
      ed.setStatus(t("object.editPoints.unavailable"), "error");
      return;
    }
    if (points.rotation) {
      ed.setStatus(t("object.editPoints.rotated"), "error");
      return;
    }
    const rect = ed.doc.objectRect(selection.node); // [page, x, y, w, h] twips
    const page = ed.pages[rect[0] - 1];
    if (rect.length < 5 || !page?.wrap) return;
    const layer = document.createElement("div");
    layer.className = "shape-points-layer";
    page.wrap.appendChild(layer);
    session = { node: selection.node, points, start: points, page, box: rect.slice(1, 5), layer };
    window.addEventListener("keydown", onKey, true);
    document.addEventListener("pointerdown", onOutside, true);
    paint();
    ed.setStatus(t("object.editPoints.hint"));
  }

  return {
    toggle,
    cancel,
    get active() {
      return session !== null;
    },
  };
}
