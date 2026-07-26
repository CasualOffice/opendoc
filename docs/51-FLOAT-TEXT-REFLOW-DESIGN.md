# Float Text Reflow Design

Status: implemented first increment for `P1F-FLOAT-REFLOW`

## Problem

The float pass positions DrawingML pictures, text boxes, and groups after
pagination. Every `wp:wrap*` mode is therefore painted like `wrapNone`: body and
running-band text can occupy the same pixels as a foreground float. The model
also drops the `wp:anchor@distT/distB/distL/distR` exclusion distances and the
semantic writer emits zeros.

Full square/tight/through wrapping is page-coupled: page-relative floats can
exclude lines in paragraphs other than their anchor paragraph, narrower lines can
increase paragraph height, and that height can move content or the float to a new
page. A post-paint shift would corrupt pagination. This design therefore lands a
safe shared-flow increment first and leaves the page-level fixed-point algorithm
explicit.

## Shared-flow invariant

Float exclusion behavior must not be body-only. The same collector and paragraph
shaper are used for:

- body paragraphs;
- header and footer variants, including repetition on later pages;
- paragraphs inside table cells and vertically merged cells;
- paragraphs nested through block/inline content controls and revisions;
- floating objects represented as pictures, text boxes, or groups.

No DOM, renderer, or header-specific layout branch is a source of truth.

## First increment: top-and-bottom barriers

1. `DrawingAnchor` gains typed non-negative EMU wrap distances. Import captures
   `distT`, `distB`, `distL`, and `distR`; export writes the modeled values; model
   validation enforces the existing `MAX_EMU` bound. Malformed or out-of-range
   source attributes are bounded deterministically and reported.
2. A `wrapTopAndBottom` float whose vertical reference is `paragraph` or `line`
   and whose position is an offset contributes a non-painting flow barrier to its
   anchor paragraph. The barrier clears through
   `max(0, verticalOffset + extentHeight + distB)`.
3. Barriers are normalized to the paragraph start because paragraph/line anchors
   currently resolve against the paragraph box, not the XML run-marker cursor.
   Multiple barriers in one paragraph coalesce to their maximum clearance rather
   than summing and over-reserving.
4. The barrier becomes an empty line box. It affects ordinary line stacking,
   row/header-band measurement, pagination, clipping, and repeated running
   content, but emits no glyph/image/shape paint item.
5. Barrier height participates in the galley cache key. A wrap-distance or extent
   edit cannot reuse stale pagination.
6. `wrapNone` and unsupported wrap configurations remain byte-for-byte unchanged.

## Explicit next increment

The following need a page-level, bounded fixed-point exclusion pass and are not
claimed by this first increment:

- square left/right/both/largest-side line narrowing;
- tight/through contour polygons;
- page/margin-relative top-and-bottom floats affecting other paragraphs;
- negative/top exclusion reaching the preceding paragraph;
- horizontal `distL/distR` consumption (preserved now for the square pass);
- collision policy for overlapping floats and `allowOverlap`;
- float-driven cross-page re-pagination and widow/keep interactions.

The fixed-point pass must operate on body and selected header/footer variants
through one exclusion interface, converge to identical page carry state, and have
a hard iteration/resource bound with an explicit diagnostic on fallback.

## Verification

Focused tests cover:

- wrap-distance model validation and JSON fixed point;
- DOCX import/export preservation of all four distances;
- body top-and-bottom reservation without duplicate paint;
- multiple-barrier coalescing and cache-key invalidation;
- table-cell nesting;
- header and footer band height/placement, including repetition on page two;
- `wrapNone`, square, and unsupported anchor frames remaining flow-neutral.

All repository formatting, strict Clippy, tests/doc tests, documentation, WASM,
and diff gates remain required.
