# Styled and Segmented Table Border Design

Status: accepted for `P1F-TBL-STYLED`

## Problem

The table flow engine currently resolves one color and width for each complete
cell side. Composition then paints that side as one solid rectangle. This loses
two independent pieces of authored information:

1. the visible line style (`double`, `dotted`, `dashed`, and dash/dot families);
2. the separate winners along a grid-spanning cell edge when differently styled
   cells abut portions of that edge.

The model and DOCX round trip retain the original `ST_Border` token, so this is a
layout/paint fidelity defect rather than source-data loss.

## Contract

1. `ResolvedEdge` carries a closed, serializable paint pattern in addition to
   color and total width. The first implementation supports solid, double,
   dotted, dashed, dot-dash, and dot-dot-dash families.
2. Unsupported line and art-border tokens resolve deterministically to the
   existing solid fallback. The source token remains preserved in the document
   model; layout does not claim exact art-border rendering.
3. Top and bottom cell sides may carry independently resolved segments. Segment
   offsets and lengths are final twip geometry relative to that cell side, not
   model grid indexes or device pixels.
4. A segment boundary is introduced at every clamped grid boundary of an
   abutting cell. Border conflict resolution runs independently for every
   interval using the current cell side and only the cells that abut that
   interval. Adjacent segments with the same winner are coalesced.
5. Non-spanning edges and the leading/trailing sides keep the compact whole-edge
   representation. Top/bottom segments override their whole-edge fallback at
   composition; the fallback remains available for compatibility and inspection.
6. A vertical-merge restart copies both the whole closing edge and its resolved
   segments from the final continuation, so style changes below the merge are
   not flattened or lost.
7. Composition emits portable filled rectangles. Double borders use two parallel
   bands inside the authored total width. Dashed families split the longitudinal
   axis into deterministic on/off runs whose phase is anchored to the physical
   page coordinate. The two cells that share an edge therefore produce identical
   dash placement even when their segment partitions differ. This keeps
   appearance backend-independent and needs no DOM or renderer-only state.
8. Dash expansion is resource-bounded. If an edge would exceed the fixed
   per-edge rectangle budget, composition paints the edge as its solid fallback
   rather than allocating an unbounded display list.
9. Border style participates in paragraph layout-cache hashing so a style-only
   edit cannot reuse stale paint.

## Scope and deferrals

This slice covers visible table and paragraph border line styles plus segmented
horizontal cell edges. It does not implement:

- art-border glyphs or exact compound styles beyond `double`;
- diagonal cell borders;
- non-zero cell-spacing conflict behavior;
- table-style/conditional-format border cascade;
- theme/automatic border color resolution.

Those limitations remain explicit in the rendering fidelity analysis and support
matrix.

## Verification

Focused regressions cover:

- style-token mapping into the resolved edge;
- distinct winners on each half of a grid-spanning horizontal edge;
- adjacent equal segment coalescing;
- solid, double, dotted, dashed, dot-dash, and dot-dot-dash paint geometry;
- bounded fallback for pathological dash counts;
- paragraph cache hashing of the border pattern.

Repository gates remain formatting, strict Clippy, workspace tests and doc tests,
WASM checking, documentation generation, and diff validation.
