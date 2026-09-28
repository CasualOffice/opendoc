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
 * The box a resize-grip drag produces, before any snapping.
 *
 * The aspect-lock rule is kind-aware, to match the platform norm rather than a
 * single modifier convention: a PICTURE keeps its proportions by DEFAULT on a
 * corner drag (Word and Docs both lock aspect for images) and Shift frees it; a
 * TEXT BOX resizes freely by default and Shift locks. Either way the constraint
 * drives both edges from the axis that moved MORE, so the object keeps its
 * proportions no matter which way the pointer went.
 *
 * O(1).
 *
 * @param {{x: number, y: number, w: number, h: number, aspect: number}} start the box at pointer-down
 * @param {number} handleKind 0..7, clockwise from NW
 * @param {number} dx pointer dx in twips
 * @param {number} dy pointer dy in twips
 * @param {{lockAspect: boolean, minEdge: number}} rules
 * @returns {{x: number, y: number, w: number, h: number, isCorner: boolean, lockedAspect: boolean}}
 */
export function resizeFromDrag(start, handleKind, dx, dy, rules) {
  const [fw, fh] = RESIZE_FACTORS[handleKind] ?? [0, 0];
  const min = rules.minEdge;
  let w = Math.max(min, start.w + fw * dx);
  let h = Math.max(min, start.h + fh * dy);
  const isCorner = fw !== 0 && fh !== 0;
  const lockedAspect = isCorner && rules.lockAspect;
  if (lockedAspect) {
    if (Math.abs(w - start.w) >= Math.abs(h - start.h)) h = Math.max(min, Math.round(w / start.aspect));
    else w = Math.max(min, Math.round(h * start.aspect));
  }
  return {
    x: fw < 0 ? start.x + start.w - w : start.x,
    y: fh < 0 ? start.y + start.h - h : start.y,
    w,
    h,
    isCorner,
    lockedAspect,
    movesWest: fw < 0,
    movesNorth: fh < 0,
    changesWidth: fw !== 0,
    changesHeight: fh !== 0,
  };
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
export function snapEdge(edge, targets, axis, tolerance) {
  const best = bestCorrection([{ at: edge, id: "edge" }], targets ?? [], tolerance);
  if (!best) return { edge, guide: null };
  return {
    edge: edge + best.delta,
    guide: { axis, at: best.target.at, id: best.target.id },
  };
}
