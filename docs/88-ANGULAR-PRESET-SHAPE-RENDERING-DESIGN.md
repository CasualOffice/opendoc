# 88 — Angular Preset Shape Rendering Design

**Status:** Implemented for the bounded angular-preset slice.
**Date:** 2026-08-03
**Depends on:** doc 87 and `P1F-SHAPE-PRESET-1`.

## 1. Problem

Doc 87 preserves unknown DrawingML preset tokens but intentionally renders them
as bounding rectangles. Three common, non-adjustable angular presets therefore
retain their identity while painting with the wrong silhouette:

- `triangle`;
- `rtTriangle`;
- `diamond`.

These presets do not require the DrawingML guide-formula language, so keeping
them in the generic fallback would leave a bounded model-to-render gap.

## 2. Decisions

### 2.1 Add explicit semantic variants

`ShapeGeometry` gains `Triangle`, `RightTriangle`, and `Diamond`. Import maps the
three exact preset tokens to those variants, and semantic export maps them back
to the canonical tokens. The existing optional retained preset remains valid
only for `ShapeGeometry::Other`, preserving the invariant introduced by doc 87.

### 2.2 Use one bounded polygon paint primitive

The float layout and display list gain a polygon primitive. Only typed presets
construct it in this slice, with a fixed three or four vertices derived from the
resolved shape rectangle:

- triangle: top-center, bottom-right, bottom-left;
- right triangle: top-left, bottom-right, bottom-left;
- diamond: top-center, right-center, bottom-center, left-center.

The renderer closes the polygon, uses non-zero winding fill, and applies the
existing shape stroke. No arbitrary package-provided point list enters the
paint contract, so vertex count and memory remain structurally bounded.

### 2.3 Keep unsupported geometry honest

All other preset tokens continue to use `ShapeGeometry::Other`, retain their
bounded token and adjustment guides, and paint as an explicit rectangle
fallback. This slice does not evaluate guides, rotation, flips, custom paths, or
non-text inline shapes.

## 3. Implementation and verification

The implemented slice maps the three presets through import, semantic model,
canonical export, anchor layout, display composition, raster rendering, and the
WASM object-kind projection. Coverage includes:

- import tests for all three token-to-variant mappings;
- semantic write/reopen fixed-point coverage;
- layout/composition tests for exact polygon vertices;
- renderer pixel tests proving bounding-box corners remain unpainted while each
  polygon interior paints;
- honest support-matrix updates;
- the full locked Rust workspace tests, formatting, Clippy, native/MSRV/WASM
  checks, warning-free docs, fuzz-target build, benchmark smoke, web build, 26
  web unit tests, and all 122 Playwright tests before publication.

## 4. Explicit non-goals

- adjustable polygons such as parallelograms and many callouts;
- the complete DrawingML preset catalog;
- custom geometry or VML path evaluation;
- rotation, flips, effects, gradients, or shape editing;
- non-text inline shape flow.
