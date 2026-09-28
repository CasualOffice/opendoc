// The shapes Insert ▸ Shapes offers, grouped the way Word groups them.
//
// WHY THIS FILE EXISTS. `insertShape` takes the OOXML `a:prstGeom@prst` token
// and resolves it through `ShapeGeometry::from_preset_token` — the same table
// import and export use — so every preset the engine models is insertable the
// moment it is modeled. The gallery in `main.js` offered SEVEN, hand-written,
// in the engine's old camelCase spellings, while the model, the layout painter
// and the exporter all knew twenty-two. Fifteen presets were modeled, painted
// and unofferable. That is the repository's most expensive recurring pattern
// (SKILL §9.4: "built is not reachable"), and the fix is not a longer hand list
// — it is ONE table, in OOXML tokens, that a test can compare against the
// engine's own answer.
//
// COMPETITIVE STANDARD. Word's Insert ▸ Shapes is a popover GALLERY: a grid of
// icon-only cells under category headings (Lines, Rectangles, Basic Shapes,
// Block Arrows, Stars and Banners), each cell carrying the shape's name as its
// tooltip and accessible name, arrow keys walking the grid. Google Docs sends
// you to a separate drawing canvas, which is a worse fit for a word processor
// that already paints anchored drawings inline. So: Word.
//
// The outlines below are normalised to a 0..1 box and are the ONE geometry
// source for both the gallery cell and the rubber-band preview drawn while the
// shape is being dragged out on the page — a preview that did not match the
// cell you picked would be a lie at the exact moment the user is aiming.
// They are previews, not the renderer: the engine paints the real shape from
// the preset's `a:avLst` guides. Each polygon is drawn at that preset's
// documented DEFAULT adjustment so the two agree on arrival.
//
// Cost: O(presets), a fixed 22, computed once at module load.

/** A regular star's outline, in the 0..1 box: `points` outer vertices at
 *  `radius`, alternating with inner ones at `inner`, apex up. */
function star(points, radius, inner) {
  const vertices = [];
  for (let i = 0; i < points * 2; i += 1) {
    const r = i % 2 === 0 ? radius : inner;
    const angle = (-90 + (i * 180) / points) * (Math.PI / 180);
    vertices.push([0.5 + r * Math.cos(angle), 0.5 + r * Math.sin(angle)]);
  }
  return vertices;
}

/** A regular polygon inscribed in the 0..1 box, apex up. */
function regular(sides) {
  const vertices = [];
  for (let i = 0; i < sides; i += 1) {
    const angle = (-90 + (i * 360) / sides) * (Math.PI / 180);
    vertices.push([0.5 + 0.5 * Math.cos(angle), 0.5 + 0.5 * Math.sin(angle)]);
  }
  return vertices;
}

/**
 * Every preset, in Word's own gallery order.
 *
 * `token` is the OOXML token — what `insertShape` takes, what the document
 * carries, and what a test can check against the engine's table. `key` is the
 * localisation key for the shape's name; it is a LITERAL string at every call
 * site so the extractor can see it. `outline` is the preview geometry: a
 * polygon, or one of the two shapes a polygon cannot express.
 */
export const SHAPE_GROUPS = Object.freeze([
  {
    id: "lines",
    key: "shapes.group.lines",
    shapes: [{ token: "line", key: "shapes.line", outline: { kind: "line" } }],
  },
  {
    id: "rectangles",
    key: "shapes.group.rectangles",
    shapes: [
      {
        token: "rect",
        key: "shapes.rect",
        outline: { kind: "polygon", points: [[0, 0], [1, 0], [1, 1], [0, 1]] },
      },
      // `adj` 16667 of the shorter side; at 20x14 that is a visible round.
      { token: "roundRect", key: "shapes.roundRect", outline: { kind: "roundRect", radius: 0.16667 } },
    ],
  },
  {
    id: "basic",
    key: "shapes.group.basic",
    shapes: [
      { token: "ellipse", key: "shapes.ellipse", outline: { kind: "ellipse" } },
      {
        token: "triangle",
        key: "shapes.triangle",
        outline: { kind: "polygon", points: [[0.5, 0], [1, 1], [0, 1]] },
      },
      {
        token: "rtTriangle",
        key: "shapes.rtTriangle",
        outline: { kind: "polygon", points: [[0, 0], [1, 1], [0, 1]] },
      },
      {
        token: "diamond",
        key: "shapes.diamond",
        outline: { kind: "polygon", points: [[0.5, 0], [1, 0.5], [0.5, 1], [0, 0.5]] },
      },
      { token: "pentagon", key: "shapes.pentagon", outline: { kind: "polygon", points: regular(5) } },
      {
        // `adj` 25000 — the horizontal inset of the four corner vertices.
        token: "hexagon",
        key: "shapes.hexagon",
        outline: {
          kind: "polygon",
          points: [[0.25, 0], [0.75, 0], [1, 0.5], [0.75, 1], [0.25, 1], [0, 0.5]],
        },
      },
      {
        // `adj` 29289 — the regular octagon when the box is square.
        token: "octagon",
        key: "shapes.octagon",
        outline: {
          kind: "polygon",
          points: [
            [0.29289, 0], [0.70711, 0], [1, 0.29289], [1, 0.70711],
            [0.70711, 1], [0.29289, 1], [0, 0.70711], [0, 0.29289],
          ],
        },
      },
      {
        // `adj` 25000 — each top inset; wide edge at the bottom.
        token: "trapezoid",
        key: "shapes.trapezoid",
        outline: { kind: "polygon", points: [[0.25, 0], [0.75, 0], [1, 1], [0, 1]] },
      },
      {
        // `adj` 25000 — the horizontal offset of the top edge.
        token: "parallelogram",
        key: "shapes.parallelogram",
        outline: { kind: "polygon", points: [[0.25, 0], [1, 0], [0.75, 1], [0, 1]] },
      },
      {
        // `adj` 25000 — the arm thickness inset.
        token: "plus",
        key: "shapes.plus",
        outline: {
          kind: "polygon",
          points: [
            [0.25, 0], [0.75, 0], [0.75, 0.25], [1, 0.25], [1, 0.75], [0.75, 0.75],
            [0.75, 1], [0.25, 1], [0.25, 0.75], [0, 0.75], [0, 0.25], [0.25, 0.25],
          ],
        },
      },
    ],
  },
  {
    id: "arrows",
    key: "shapes.group.arrows",
    shapes: [
      {
        token: "rightArrow",
        key: "shapes.rightArrow",
        outline: {
          kind: "polygon",
          points: [[0, 0.25], [0.5, 0.25], [0.5, 0], [1, 0.5], [0.5, 1], [0.5, 0.75], [0, 0.75]],
        },
      },
      {
        token: "leftArrow",
        key: "shapes.leftArrow",
        outline: {
          kind: "polygon",
          points: [[1, 0.25], [0.5, 0.25], [0.5, 0], [0, 0.5], [0.5, 1], [0.5, 0.75], [1, 0.75]],
        },
      },
      {
        token: "upArrow",
        key: "shapes.upArrow",
        outline: {
          kind: "polygon",
          points: [[0.25, 1], [0.25, 0.5], [0, 0.5], [0.5, 0], [1, 0.5], [0.75, 0.5], [0.75, 1]],
        },
      },
      {
        token: "downArrow",
        key: "shapes.downArrow",
        outline: {
          kind: "polygon",
          points: [[0.25, 0], [0.25, 0.5], [0, 0.5], [0.5, 1], [1, 0.5], [0.75, 0.5], [0.75, 0]],
        },
      },
      {
        token: "leftRightArrow",
        key: "shapes.leftRightArrow",
        outline: {
          kind: "polygon",
          points: [
            [0, 0.5], [0.25, 0.2], [0.25, 0.35], [0.75, 0.35], [0.75, 0.2], [1, 0.5],
            [0.75, 0.8], [0.75, 0.65], [0.25, 0.65], [0.25, 0.8],
          ],
        },
      },
      {
        // `adj` 50000 — the point depth.
        token: "chevron",
        key: "shapes.chevron",
        outline: {
          kind: "polygon",
          points: [[0, 0], [0.5, 0], [1, 0.5], [0.5, 1], [0, 1], [0.5, 0.5]],
        },
      },
      {
        // Word calls this one "Pentagon" in its Block Arrows row; the regular
        // pentagon above is the Basic Shapes one. `homePlate` is the token.
        token: "homePlate",
        key: "shapes.homePlate",
        outline: { kind: "polygon", points: [[0, 0], [0.5, 0], [1, 0.5], [0.5, 1], [0, 1]] },
      },
    ],
  },
  {
    id: "stars",
    key: "shapes.group.stars",
    shapes: [
      // `adj` 19098 of 50000 → the regular pentagram.
      { token: "star5", key: "shapes.star5", outline: { kind: "polygon", points: star(5, 0.5, 0.19098) } },
      { token: "star4", key: "shapes.star4", outline: { kind: "polygon", points: star(4, 0.5, 0.125) } },
    ],
  },
]);

/** Every offered preset, flattened — the gallery's keyboard order and the set a
 *  test compares against the engine's own table. */
export const SHAPE_PRESETS = Object.freeze(SHAPE_GROUPS.flatMap((group) => group.shapes));

/** The localisation key for a preset's name, or `null`. O(presets). */
export function shapeNameKey(token) {
  return SHAPE_PRESETS.find((shape) => shape.token === token)?.key ?? null;
}

/**
 * The SVG source for one preset's preview, drawn in a `size`-unit square.
 *
 * Returned as an element description rather than a string so the caller builds
 * real nodes — `innerHTML` with a token in it is how a gallery becomes an
 * injection site, and these tokens come from a table but the habit is the point.
 *
 * O(vertices).
 */
export function shapePreviewShape(outline, size = 20) {
  const at = (value) => Math.round(value * size * 1000) / 1000;
  if (outline.kind === "ellipse") {
    return { tag: "ellipse", attrs: { cx: at(0.5), cy: at(0.5), rx: at(0.46), ry: at(0.46) } };
  }
  if (outline.kind === "line") {
    return { tag: "line", attrs: { x1: at(0.06), y1: at(0.94), x2: at(0.94), y2: at(0.06) } };
  }
  if (outline.kind === "roundRect") {
    return {
      tag: "rect",
      attrs: {
        x: at(0.04), y: at(0.04), width: at(0.92), height: at(0.92),
        rx: at(outline.radius), ry: at(outline.radius),
      },
    };
  }
  return {
    tag: "polygon",
    attrs: {
      points: outline.points
        .map(([x, y]) => `${at(0.04 + x * 0.92)},${at(0.04 + y * 0.92)}`)
        .join(" "),
    },
  };
}
