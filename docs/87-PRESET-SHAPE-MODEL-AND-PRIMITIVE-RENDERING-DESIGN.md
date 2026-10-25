# 87 — Preset Shape Model and Rendering Design

**Status:** Implemented for the bounded first shape-fidelity slice.
**Date:** 2026-08-03
**Depends on:** docs 43, 46, 55, and the existing `P1F-F4` group/float layer.

## 1. Problem

The current DrawingML shape path has three correctness gaps:

1. a standalone anchored `wps:wsp` / `wps:cxnSp` with no text is reported and
   dropped;
2. an unrecognized `a:prstGeom@prst` becomes `ShapeGeometry::Other` and semantic
   export rewrites it as `rect`, losing the authored preset identity;
3. layout maps every non-line geometry to a rectangle, so even the already typed
   `ellipse` and `roundRect` presets paint incorrectly.

Custom geometry, gradients, rotation, and the complete preset catalog are larger
languages and must not be implied by this increment.

## 2. Decisions

### 2.1 Standalone shapes reuse the group child model

A bare anchored shape is normalized to an `InlineNode::Group` containing one
`GroupChild::Shape`, with an identity child transform over the authored drawing
extent. This reuses the existing floating placement, z-order, validation,
semantic export, and group-child render pipeline instead of adding a second
shape node with duplicate policy. A non-text inline shape still requires a true
in-flow composite box and remains a separate follow-up rather than being forced
onto a standalone line.

### 2.2 Preset identity and adjustments are additive

`GroupShape` retains:

- the existing coarse `ShapeGeometry` render classification;
- an optional original preset token for an unrecognized preset;
- a bounded ordered list of `a:avLst/a:gd` adjustment guides (`name`, `fmla`).

Known presets remain canonical enum values. Unknown presets keep their exact
bounded token and still render through the explicit rectangular fallback until
their geometry is implemented. Semantic export writes the retained token and
adjustment list, preventing the current unknown-preset-to-rectangle mutation.

Limits apply to preset token length, adjustment count, guide name length, and
formula length. Empty/over-limit values are rejected by model validation and
ignored/reported by import rather than truncated.

### 2.3 Primitive rendering extends existing paint contracts

The page float model and display list gain explicit ellipse and rounded-rectangle
primitives. Composition carries the authored rectangle, fill, and stroke without
platform geometry. The CPU renderer builds deterministic paths at paint scale:

- ellipse: an oval fitted to the authored bounding rectangle;
- rounded rectangle: a closed quadratic path with a radius derived from the first
  literal `adj` guide when available, otherwise the DrawingML default; radius is
  clamped to half the shorter side.

Rectangle and line output stay byte-for-byte on their existing paths. Unknown
presets continue to render as bounding rectangles and remain documented partial
support.

## 3. Custom geometry policy

`a:custGeom` is not silently classified as a supported preset. Import records a
compatibility entry and keeps retention-mode source bytes. Typed guides/paths and
semantic re-emission are a separate slice because DrawingML coordinates may be
guide formulas, not only numeric points. This increment does not claim lossless
semantic export for custom geometry.

## 4. Verification

The slice requires:

- model serde/validation tests for retained presets and adjustment limits;
- import tests proving standalone shapes survive and unknown presets/adjustments
  are modeled;
- semantic write/reopen fixed-point tests for retained preset identity/guides;
- layout/composition tests proving ellipse and round-rectangle primitives reach
  distinct display-list items;
- renderer pixel tests proving corners remain transparent while shape interiors
  paint;
- full formatting, workspace tests, strict Clippy, wasm, and rustdoc gates.

## 5. Explicit non-goals

- complete DrawingML preset geometry formulas;
- typed/rendered `a:custGeom` paths;
- gradients, patterns, effects, shadows, or per-side strokes;
- rotation, flips, 3D transforms, and text warp;
- standalone shape editing commands.
- non-text inline shape flow (anchored standalone shapes are covered here).

## 6. Implementation result

The bounded slice is complete. Standalone anchored non-text shapes now survive
import as groups of one; bounded unknown preset tokens and ordered adjustment
guides survive semantic export/reopen; ellipses and rounded rectangles reach
distinct layout, display-list, and CPU-renderer primitives; and custom geometry
is reported without an unsupported typed-fidelity claim. The public support
matrix remains partial because the explicit non-goals above are still open.

Verification completed with focused model/import/export/layout/render tests,
locked all-feature workspace tests, strict all-target/all-feature Clippy, the
wasm32 workspace check, Rust 1.88 compatibility, formatting, doc tests, strict
rustdoc, the DOCX package fuzz-target build, the benchmark smoke gate, a clean
web build, 26 web unit tests, and 122 Playwright tests.

## 7. Angular preset rendering (formerly doc 88)

**Status:** Implemented for the bounded angular-preset slice.
**Date:** 2026-08-03
**Depends on:** the primitive slice above and `P1F-SHAPE-PRESET-1`.

### 7.1 Problem

Doc 87 preserves unknown DrawingML preset tokens but intentionally renders them
as bounding rectangles. Three common, non-adjustable angular presets therefore
retain their identity while painting with the wrong silhouette:

- `triangle`;
- `rtTriangle`;
- `diamond`.

These presets do not require the DrawingML guide-formula language, so keeping
them in the generic fallback would leave a bounded model-to-render gap.

### 7.2 Decisions

#### 7.2.1 Add explicit semantic variants

`ShapeGeometry` gains `Triangle`, `RightTriangle`, and `Diamond`. Import maps the
three exact preset tokens to those variants, and semantic export maps them back
to the canonical tokens. The existing optional retained preset remains valid
only for `ShapeGeometry::Other`, preserving the invariant introduced by doc 87.

#### 7.2.2 Use one bounded polygon paint primitive

The float layout and display list gain a polygon primitive. Only typed presets
construct it in this slice, with a fixed three or four vertices derived from the
resolved shape rectangle:

- triangle: top-center, bottom-right, bottom-left;
- right triangle: top-left, bottom-right, bottom-left;
- diamond: top-center, right-center, bottom-center, left-center.

The renderer closes the polygon, uses non-zero winding fill, and applies the
existing shape stroke. No arbitrary package-provided point list enters the
paint contract, so vertex count and memory remain structurally bounded.

#### 7.2.3 Keep unsupported geometry honest

All other preset tokens continue to use `ShapeGeometry::Other`, retain their
bounded token and adjustment guides, and paint as an explicit rectangle
fallback. This slice does not evaluate guides, rotation, flips, custom paths, or
non-text inline shapes.

### 7.3 Implementation and verification

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

### 7.4 Explicit non-goals

- adjustable polygons such as parallelograms and many callouts;
- the complete DrawingML preset catalog;
- custom geometry or VML path evaluation;
- rotation, flips, effects, gradients, or shape editing;
- non-text inline shape flow.
