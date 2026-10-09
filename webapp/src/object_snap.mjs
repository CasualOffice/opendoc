// Where a dragged object wants to land, and the lines that say why.
//
// Dragging a floating image used to put it exactly where the pointer left it
// and nothing else. Measured on the `float` fixture: dropped with its centre
// 5px from the page's centre line, it stayed 5px off, with no line drawn and no
// pull — so "centre this image on the page" was a job for the properties panel
// and a calculator. Word draws green alignment guides and pulls; Google Docs
// draws blue ones and pulls. It is the same mechanism in both, and it is the
// difference between placing an image and typing its coordinates.
//
// ## The known pattern
//
// This is SNAPPING with proximity thresholding — the same rule a ruler tab
// stop, a timeline clip and a CAD grid all use. Two halves:
//
//   * a fixed set of TARGET lines, derived once from the page (they cannot move
//     during a drag, because the page does not);
//   * on each pointer sample, the smallest signed correction that brings any of
//     the object's own reference edges within tolerance of any target.
//
// Each axis resolves independently and contributes at most one correction, so
// an object can be centred horizontally while its top sits on nothing.
//
// ## Why the module is pure
//
// Same split as `pointer_cursor.mjs` / `pointer_hover.mjs`: the arithmetic that
// decides where the object goes is answerable in node with plain numbers, and
// the half that paints a line on an overlay is not. Keeping them apart is what
// lets `object_snap.test.mjs` drive "does a 5px miss become a 0px hit" without
// a browser, an engine or a document.
//
// ## Cost
//
// `pageSnapTargets` is O(1) and is called ONCE per gesture, at pointer-down.
// `snapBox` and `snapEdge` are O(targets) with targets fixed at 4 and 2 — so
// the whole gesture is O(1) in document size, which is the `docs/107` §4
// budget a drag has to meet because it fires on every pointer move.

/** Handle index → which edges it drags, as (width factor, height factor).
 *  Clockwise from NW: NW, N, NE, E, SE, S, SW, W. */
const RESIZE_FACTORS = [
  [-1, -1], [0, -1], [1, -1], [1, 0], [1, 1], [0, 1], [-1, 1], [-1, 0],
];

/**
 * What the modifiers held during a grip drag mean, for every object kind.
 *
 * This used to be an expression at the call site, and it had invented a
 * convention no product uses: `lockAspect: isImage ? !shift : shift`, so Shift
 * FREED a picture and LOCKED a text box. Word, Google Docs and ONLYOFFICE all
 * agree that Shift CONSTRAINS and never frees — ONLYOFFICE's
 * `ResizeTracks.js` reads `ShiftKey === true || getNoChangeAspect()`, an OR, so
 * holding Shift can only add the constraint.
 *
 * What constrains WITHOUT Shift is the object's own "Lock aspect ratio" —
 * DrawingML's `noChangeAspect`, which the engine publishes with the selection
 * as `locksAspectRatio` (`docs/109` FID-AT-09) — and not its kind. Word honours
 * the flag both ways: a picture whose file states no lock stretches on a corner
 * drag, and a shape whose file locks its ratio keeps it. An ABSENT flag is
 * unlocked, because that is how Word reads the same file. Word writes the lock
 * on every picture it inserts, and so does this editor's own Insert ▸ Picture,
 * so an inserted picture keeps its proportions exactly as it always has here.
 * Distorting a locked picture stays reachable — through the inspector's width
 * and height fields, which is where Word and ONLYOFFICE put it too.
 *
 * Ctrl (or Cmd) resizes about the object's CENTRE, both edges moving together;
 * ONLYOFFICE routes the same modifier to `resizeRelativeCenter`.
 *
 * O(1).
 *
 * @param {{kind?: string|null, locksAspect?: boolean, shiftKey: boolean, ctrlKey: boolean, metaKey: boolean}} at
 * @param {number} minEdge the smallest edge a drag may leave, in twips
 * @returns {{lockAspect: boolean, fromCentre: boolean, minEdge: number}}
 */
export function resizeRulesFor(at, minEdge) {
  return {
    lockAspect: at.locksAspect === true || at.shiftKey === true,
    fromCentre: at.ctrlKey === true || at.metaKey === true,
    minEdge,
  };
}

/**
 * The box a resize-grip drag produces, before any snapping.
 *
 * The aspect constraint drives both edges from the axis that moved MORE, so the
 * object keeps its proportions no matter which way the pointer went, and it
 * applies to CORNERS only — an edge midpoint changes one axis by definition, and
 * ONLYOFFICE gates the same rule on its `bAspect`, which is
 * `numberHandle % 2 === 0`, i.e. the four corners in our own index order.
 *
 * With `fromCentre`, the object grows about its centre: the pointer delta counts
 * twice and BOTH edges on the axis move, which is why the origin is recomputed
 * from the centre rather than from a pinned edge.
 *
 * O(1).
 *
 * @param {{x: number, y: number, w: number, h: number, aspect: number}} start the box at pointer-down
 * @param {number} handleKind 0..7, clockwise from NW
 * @param {number} dx pointer dx in twips
 * @param {number} dy pointer dy in twips
 * @param {{lockAspect: boolean, fromCentre?: boolean, minEdge: number}} rules
 * @returns {{x: number, y: number, w: number, h: number, isCorner: boolean, lockedAspect: boolean}}
 */
export function resizeFromDrag(start, handleKind, dx, dy, rules) {
  const [fw, fh] = RESIZE_FACTORS[handleKind] ?? [0, 0];
  const min = rules.minEdge;
  const fromCentre = rules.fromCentre === true;
  const reach = fromCentre ? 2 : 1;
  let w = Math.max(min, start.w + reach * fw * dx);
  let h = Math.max(min, start.h + reach * fh * dy);
  const isCorner = fw !== 0 && fh !== 0;
  const lockedAspect = isCorner && rules.lockAspect;
  if (lockedAspect) {
    if (Math.abs(w - start.w) >= Math.abs(h - start.h)) h = Math.max(min, Math.round(w / start.aspect));
    else w = Math.max(min, Math.round(h * start.aspect));
  }
  const x = fromCentre
    ? Math.round(start.x + (start.w - w) / 2)
    : (fw < 0 ? start.x + start.w - w : start.x);
  const y = fromCentre
    ? Math.round(start.y + (start.h - h) / 2)
    : (fh < 0 ? start.y + start.h - h : start.y);
  return {
    x,
    y,
    w,
    h,
    isCorner,
    lockedAspect,
    fromCentre,
    movesWest: fw < 0,
    movesNorth: fh < 0,
    changesWidth: fw !== 0,
    changesHeight: fh !== 0,
  };
}

/**
 * Where a finished grip drag actually commits its origin.
 *
 * A FLOATING object commits the rectangle it was dragged to, origin and all. An
 * INLINE one has no anchor to move — the paragraph owns its origin — so it
 * commits its new SIZE at the origin it started from. That is not a smaller
 * feature: it is the whole reason all eight grips are truthful for an inline
 * object. The PREVIEW still shows the opposite edge pinned, because that is what
 * Word and ONLYOFFICE draw and what the eye is tracking; only the commit drops
 * the origin delta, which the engine would otherwise refuse outright with
 * "inline resize cannot move its flow anchor".
 *
 * O(1).
 *
 * @param {{x: number, y: number}} box the previewed rectangle at release
 * @param {{x: number, y: number}} start the rectangle at pointer-down
 * @param {boolean} anchored whether the object carries its own anchor (floating)
 * @returns {{x: number, y: number}}
 */
export function resizeCommitOrigin(box, start, anchored) {
  return anchored === true ? { x: box.x, y: box.y } : { x: start.x, y: start.y };
}

/** A reference line an object can align to, in page-local twips.
 *
 * `id` is a stable token, not a sentence: the caller decides whether to say
 * anything about it and in which language.
 *
 * @typedef {{ at: number, id: string }} SnapTarget
 */

/** The alignment targets a page offers, in page-local twips.
 *
 * Only lines the engine can state cheaply are here. The left and right text
 * margins and the page's own two centre lines come from geometry already in
 * hand at pointer-down; the top and bottom margins would need
 * `pageSetupSections`, which walks the document, so they are deliberately
 * absent rather than bought at that price. That is a stated limit, not an
 * oversight: an object can be centred vertically but cannot yet snap to the
 * top margin.
 *
 * O(1).
 *
 * @param {{widthTwip: number, heightTwip: number, marginStartTwip: number, marginEndTwip: number}} page
 * @returns {{vertical: SnapTarget[], horizontal: SnapTarget[]}} vertical lines
 *   are x positions (they run down the page); horizontal lines are y positions.
 */
export function pageSnapTargets(page) {
  const width = Number(page?.widthTwip) || 0;
  const height = Number(page?.heightTwip) || 0;
  const start = Number(page?.marginStartTwip) || 0;
  const end = Number(page?.marginEndTwip) || 0;
  const vertical = [];
  const horizontal = [];
  if (width > 0) {
    vertical.push({ at: start, id: "leftMargin" });
    vertical.push({ at: width / 2, id: "pageCentre" });
    if (width - end > start) vertical.push({ at: width - end, id: "rightMargin" });
  }
  if (height > 0) horizontal.push({ at: height / 2, id: "pageMiddle" });
  return { vertical, horizontal };
}

/** The three reference positions an object edge-set offers on one axis:
 *  its near edge, its centre, and its far edge. */
function referencesOf(origin, extent) {
  return [
    { at: origin, id: "near" },
    { at: origin + extent / 2, id: "centre" },
    { at: origin + extent, id: "far" },
  ];
}

/** The smallest correction that brings any of `references` onto any of
 *  `targets`, or `null` when nothing is within `tolerance`.
 *
 *  Ties go to the FIRST target in order, which is why `pageSnapTargets` lists
 *  the margins before the centre: at the exact midpoint of a narrow page a
 *  margin is the line the user was aiming at.
 *
 *  O(references × targets) = O(9) at the call sites here.
 *
 *  @returns {{delta: number, target: SnapTarget, reference: string} | null}
 */
function bestCorrection(references, targets, tolerance) {
  let best = null;
  for (const reference of references) {
    for (const target of targets) {
      const delta = target.at - reference.at;
      const distance = Math.abs(delta);
      if (distance > tolerance) continue;
      if (best === null || distance < Math.abs(best.delta)) {
        best = { delta, target, reference: reference.id };
      }
    }
  }
  return best;
}

/** A guide line the host should draw, named by the target it sits on. */
/** @typedef {{ axis: "vertical"|"horizontal", at: number, id: string }} SnapGuide */

/** Pulls a whole box onto the nearest alignment lines (a MOVE gesture).
 *
 * The box keeps its size: only its origin moves. Each axis is resolved
 * independently, so a box can snap horizontally and be free vertically.
 *
 * O(1).
 *
 * @param {{x: number, y: number, w: number, h: number}} box page-local twips
 * @param {{vertical: SnapTarget[], horizontal: SnapTarget[]}} targets
 * @param {number} tolerance the snap radius in twips (convert from CSS px at
 *   the CURRENT zoom, so the pull is the same distance on screen at any zoom)
 * @returns {{x: number, y: number, guides: SnapGuide[]}}
 */
export function snapBox(box, targets, tolerance) {
  const guides = [];
  let { x, y } = box;
  const vertical = bestCorrection(referencesOf(box.x, box.w), targets.vertical ?? [], tolerance);
  if (vertical) {
    x = box.x + vertical.delta;
    guides.push({ axis: "vertical", at: vertical.target.at, id: vertical.target.id });
  }
  const horizontal = bestCorrection(referencesOf(box.y, box.h), targets.horizontal ?? [], tolerance);
  if (horizontal) {
    y = box.y + horizontal.delta;
    guides.push({ axis: "horizontal", at: horizontal.target.at, id: horizontal.target.id });
  }
  return { x, y, guides };
}

/** Pulls ONE moving edge onto the nearest alignment line (a RESIZE gesture).
 *
 * A resize is not a move: the opposite edge is pinned, so only the edge under
 * the pointer is a candidate and the box's centre is not. Returns the snapped
 * edge position and the guide to draw, or the input unchanged.
 *
 * O(1).
 *
 * @param {number} edge the moving edge's page-local position, in twips
 * @param {SnapTarget[]} targets the targets for this edge's axis
 * @param {"vertical"|"horizontal"} axis which axis the edge moves along
 * @param {number} tolerance the snap radius in twips
 * @returns {{edge: number, guide: SnapGuide | null}}
 */
/** Pulls a RESIZED box's moving edges onto the page's alignment lines.
 *
 * The whole point of snapping a resize is that "make this image reach the right
 * margin" is a drag and not an arithmetic problem. Each axis resolves
 * independently, and on each axis the edge NOT under the pointer is pinned.
 *
 * Two kinds of drag are deliberately left alone, and it is the same reason both
 * times — a constraint the user asked for must not be broken to buy a snap:
 *
 *   * an aspect-LOCKED corner, because correcting one axis onto a line has to
 *     drive the other off the ratio;
 *   * a CENTRE resize, because both edges on an axis are moving, so pulling one
 *     onto a line shoves the other off by the same amount.
 *
 * Pass `snap` as null to suppress the pull entirely (that is what Alt does).
 *
 * O(1) — `snapEdge` is O(targets) with targets fixed at 4 and 2.
 *
 * @param {ReturnType<typeof resizeFromDrag>} box
 * @param {{targets: {vertical: SnapTarget[], horizontal: SnapTarget[]}, tolerance: number} | null} snap
 * @param {number} minEdge
 * @returns {{box: {x: number, y: number, w: number, h: number}, guides: SnapGuide[]}}
 */
export function snapResizedBox(box, snap, minEdge) {
  let { x, y, w, h } = box;
  const guides = [];
  if (!snap || box.lockedAspect || box.fromCentre) return { box: { x, y, w, h }, guides };
  if (box.changesWidth) {
    const moving = box.movesWest ? x : x + w;
    const hit = snapEdge(moving, snap.targets.vertical, "vertical", snap.tolerance);
    if (hit.guide) {
      const pinnedRight = x + w; // whichever edge is NOT under the pointer stays
      if (box.movesWest) {
        w = Math.max(minEdge, pinnedRight - hit.edge);
        x = pinnedRight - w;
      } else {
        w = Math.max(minEdge, hit.edge - x);
      }
      guides.push(hit.guide);
    }
  }
  if (box.changesHeight) {
    const moving = box.movesNorth ? y : y + h;
    const hit = snapEdge(moving, snap.targets.horizontal, "horizontal", snap.tolerance);
    if (hit.guide) {
      const pinnedBottom = y + h;
      if (box.movesNorth) {
        h = Math.max(minEdge, pinnedBottom - hit.edge);
        y = pinnedBottom - h;
      } else {
        h = Math.max(minEdge, hit.edge - y);
      }
      guides.push(hit.guide);
    }
  }
  return { box: { x, y, w, h }, guides };
}

export function snapEdge(edge, targets, axis, tolerance) {
  const best = bestCorrection([{ at: edge, id: "edge" }], targets ?? [], tolerance);
  if (!best) return { edge, guide: null };
  return {
    edge: edge + best.delta,
    guide: { axis, at: best.target.at, id: best.target.id },
  };
}
