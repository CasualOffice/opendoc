# 47 — Floating objects and text boxes

Status: accepted for the bounded implementation described here.

## Problem

The importer already models a DrawingML text box's optional extent, fill, outline,
and floating anchor, but the inline import path discards those properties. Layout
then invents a black one-pixel outline and sizes every inline box to the available
column. The semantic writer also emits every text box as a minimal inline shape,
which silently loses floating anchors and authored appearance on save.

## Decisions and invariants

1. `TextBox::extent` is meaningful for both inline and floating DrawingML boxes.
   A missing or zero dimension is not guessed during import.
2. Layout honors a positive authored width and height. A missing or zero width
   falls back to the available flow width; a missing or zero height falls back to
   the flowed content height plus the existing internal inset.
3. Fill, outline color, and outline width travel from import through layout to
   paint. No outline is fabricated when the document does not provide one.
4. Floating text boxes continue through the established anchor placement layer.
   Inline text boxes remain paragraph flow items.
5. Semantic export emits `wp:inline` or `wp:anchor` from the model and preserves
   the extent, anchor, z-order key, fill, and outline. Import → export → import
   must therefore reach a semantic fixed point for these properties.
6. Existing resource and nesting bounds remain unchanged. Text-box content still
   uses the shared block-flow pipeline rather than a second renderer.

## Compatibility boundary

This slice did not add `wps:bodyPr` insets, vertical anchoring, rotation,
autofit, overflow clipping, or text wrapping around floating objects. Insets,
vertical anchoring, bounded autofit, and overflow policy are designed separately
in `docs/47-TEXT-BOX-APPEARANCE-DESIGN.md#doc-52`; rotation and the deliberate
legacy-VML fallback remain later work.

The sections below preserve the bounded designs formerly filed as docs 48 and
51–54. For current support claims, check the implementation and the active
[backlog](109-BACKLOG.md).

<a id="doc-48"></a>

## Nested float anchor placement design (former doc 48)

Status: accepted for the bounded implementation described here.

### Problem

The float collector already walks model paragraphs nested in body tables, but
page lookup checks only top-level placed fragments. A float in a table cell
therefore falls back to page zero and the page content box instead of the
paragraph that owns its anchor.

Header/footer placement has a second failure: its collector visits only
top-level placed paragraph fragments. A table row in a running-content band is
never descended, so floating drawings inside its cells are invisible.

### Decisions and invariants

1. Add one read-only fragment-tree locator that mirrors the composition
   geometry for table rows, cell margins, vertical alignment, and nested block
   stacking.
2. A paragraph match returns its real page-local rectangle and page index.
   Lookup searches placed page fragments in page order; it never silently
   substitutes another nested paragraph.
3. Body model traversal remains the source of float document order. Only target
   lookup becomes recursive.
4. Header/footer collection enumerates every paragraph in the selected placed
   band fragment tree, resolves the matching source paragraph, and uses the
   located rectangle directly. A paragraph is collected once per selected band
   occurrence, so running content still repeats per page.
5. The recursion is over the already bounded `BlockFragment` tree and allocates
   no second layout tree. Paint order, z-order, anchor semantics, and text flow
   are unchanged.
6. Split table rows are handled naturally: empty head-cell fragments do not
   match, while the page chunk containing the paragraph supplies its target.

### Compatibility boundary

This slice did not implement text wrapping around floats, per-section page
geometry, table vertical merges, or VML absolute-text-box restoration. It fixed
only discovery and paragraph-relative placement for already modeled floats.
Per-section page/margin/column geometry is implemented separately by P1F-53
(`docs/47-TEXT-BOX-APPEARANCE-DESIGN.md#doc-53`).

<a id="doc-51"></a>

## Float Text Reflow Design (former doc 51)

Status: implemented first increment for `P1F-FLOAT-REFLOW`

### Problem

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

### Shared-flow invariant

Float exclusion behavior must not be body-only. The same collector and paragraph
shaper are used for:

- body paragraphs;
- header and footer variants, including repetition on later pages;
- paragraphs inside table cells and vertically merged cells;
- paragraphs nested through block/inline content controls and revisions;
- floating objects represented as pictures, text boxes, or groups.

No DOM, renderer, or header-specific layout branch is a source of truth.

### First increment: top-and-bottom barriers

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

### Explicit next increment

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

### Verification

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

<a id="doc-52"></a>

## Text-box body properties design (former doc 52)

Status: accepted for the bounded implementation described here.

### Problem

`wps:bodyPr` is currently discarded. Layout substitutes one 72-twip inset on
every side, always places content at the top, never applies autofit, and lets
nested content paint outside an authored text-box extent. The same hard-coded
behavior is used for inline boxes, floating boxes, grouped boxes, and boxes in
headers and footers.

### Model and import decisions

1. `TextBox` and `GroupTextBox` carry one `TextBoxBodyProperties` value.
2. The four DrawingML insets remain independent signed EMU coordinates. Omitted
   left/right values resolve to 91,440 EMU (0.1 inch); omitted top/bottom values
   resolve to 45,720 EMU (0.05 inch).
3. Vertical anchoring is modeled as top, center, or bottom; omitted means top.
4. Horizontal overflow is `overflow` or `clip`. Vertical overflow is
   `overflow`, `clip`, or `ellipsis`. Omitted means `overflow`.
5. Autofit is a choice:
   - absent/`noAutofit`: authored dimensions remain fixed;
   - `spAutoFit`: the shape grows vertically to contain its flowed content and
     insets, but never shrinks below a positive authored height;
   - `normAutofit`: the authored `fontScale` and `lnSpcReduction` percentages are
     retained and consumed while flowing the content.
6. Invalid enum tokens or percentages are reported through the compatibility
   reporter and fall back to their schema defaults. Signed inset coordinates are
   represented as `i32`, matching `ST_Coordinate32` without an unbounded integer
   path.
7. Semantic export emits the modeled `wps:bodyPr` attributes and autofit child.
   Import → write → reopen must reach a semantic fixed point for inline,
   floating, and grouped text boxes.

### Layout and paint decisions

1. A text box's inner width is `outer width - left inset - right inset`, clamped
   to one twip. Its content uses the existing recursive block flow; there is no
   text-box-only paragraph, table, image, or object renderer.
2. A fixed positive authored height remains the outer height except for
   `spAutoFit`. A missing/zero height resolves to content height plus top/bottom
   insets.
3. Top/center/bottom anchoring offsets the already-flowed block stack within the
   inner height. Negative free space never moves overflowing content upward.
4. `normAutofit@fontScale` scales run font size, letter spacing, and baseline
   shift while shaping. `lnSpcReduction` is applied only to percentage line
   spacing, as required by DrawingML.
5. `horzOverflow="clip"` and `vertOverflow="clip"` push a bounded paint clip on
   the selected axis around nested paragraphs, tables, images, and drawings.
   `vertOverflow="ellipsis"` currently uses the same safe clip but does not
   synthesize an ellipsis; the semantic value is preserved and this visual
   limitation remains explicit.
6. The resolved content origin and clip policy travel with both
   `InlineTextBox` and floating `AnchorContent::TextBox`. Composition therefore
   cannot accidentally fall back to a global inset.

### Context invariant

Body paragraphs, table cells (including nested tables), headers, and footers all
enter the same block-flow and float-placement functions. Tests must exercise an
inline body box, a nested cell box, a floating header box, a floating footer box,
and a grouped box. No feature is considered supported if it only works in the
main document body.

### Compatibility boundary

This slice does not implement text rotation/vertical writing, `anchorCtr`,
columns inside a text body, preset text warp, exact vertical ellipsis generation,
or automatic computation of a missing `normAutofit@fontScale`. Legacy VML
`inset`, CSS positioning, and VML-specific overflow remain in the later VML
positioning slice; existing VML boxes retain their prior uniform-inset fallback.

<a id="doc-53"></a>

## Per-section anchor geometry design (former doc 53)

Status: accepted for the bounded implementation described here.

### Problem

Multi-section pagination already flows each section with its own page size,
margins, and column layout. Floating-object placement runs afterward, but it is
given only the first section's `PageConfig`. Consequently a picture, text box, or
group anchored in a later section can resolve `relativeFrom="page"`,
`relativeFrom="margin"`, and physical margin-edge positions against stale
geometry.

The failure also affects a `continuous` section that shares a page with its
predecessor. `Page::section` can describe only one section, so choosing geometry
from the page alone loses the source section of a float in the other band.

### Decisions

1. Body float collection carries the source section in document order. The
   paragraph that owns a section break remains in the ending section; subsequent
   body blocks advance to the next section.
2. Anchor reference boxes are derived from that section's `SectionBoundary`.
   The caller's `PageConfig` is only the deterministic fallback for a malformed
   or sectionless document.
3. Paragraph lookup returns both the paragraph rectangle and the top-level placed
   fragment's horizontal flow column. `relativeFrom="column"` therefore resolves
   against the actual column that contains the anchoring paragraph, including for
   paragraphs nested in tables and SDTs.
4. Page, main-margin, left-margin, right-margin, top-margin, and bottom-margin
   boxes are distinct. They use the source section's page size and physical
   margins; header/footer reservation does not redefine an OOXML page-margin
   anchor.
5. Header/footer floats repeat from the selected placed band as before. Their
   frame geometry comes from the section recorded on that page, rather than the
   document's first section.
6. Pictures, floating text boxes, and groups continue through the same target
   resolver. Z-order, group transforms, text-box flow, and paint behavior do not
   fork by object kind.
7. No new mutable layout tree or model field is introduced. Section geometry is
   resolved read-only from the normalized document, and located column geometry
   comes from immutable placed fragments.

### Tests

The bounded acceptance set must prove:

- a later next-page section with a different page size and margins positions
  page- and margin-relative floats from its own geometry;
- two continuous sections sharing one page retain different source geometries;
- a column-relative float uses the actual containing column, including through a
  table-cell paragraph;
- a header/footer float uses the section recorded for its page.

Assertions target final `PlacedAnchor` rectangles, not only section identifiers.

### Compatibility boundary

This slice does not select and measure distinct header/footer definitions for
every section; the existing running-content pass still owns that separate gap.
It does not implement exact character-relative glyph anchoring, mirrored-margin
page layout, simple-position override, percentage anchor offsets, or
square/tight/through wrapping. Inside/outside-margin parity remains tied to the
later mirrored-margin geometry slice.

<a id="doc-54"></a>

## VML Text-Box Positioning Design (former doc 54)

Status: accepted for the bounded P1F-VML-POS implementation slice.

### Problem

Legacy Word VML is already parsed into the shared drawing and text-box model,
but its positioning bridge is incomplete:

- `mso-position-horizontal` and `mso-position-vertical` alignments are ignored;
- left/right/inner/outer and top/bottom margin reference frames collapse to a
  smaller set of anchors;
- `w10:wrap`, `mso-wrap-mode`, and the four `mso-wrap-distance-*` values are
  discarded, so positioned VML pictures and shapes cannot participate in the
  existing float-reflow path;
- `v:textbox@inset`, `v-text-anchor`, and `mso-fit-shape-to-text` are parsed
  incompletely or replaced by one hard-coded inset;
- header/footer text boxes can be positioned, but body text boxes are forced
  inline unconditionally after a previous all-float implementation caused
  severe overlap in real VML-primary documents.

The last point is a correctness constraint, not merely a missing feature. The
body fallback must not be removed until the layout engine can honor the source
object's exclusion semantics.

### Source semantics

Microsoft's Office conformance notes define the Word-specific VML position
tokens:

- `mso-position-horizontal` selects absolute, left, center, right, inside, or
  outside placement within the frame named by
  `mso-position-horizontal-relative`;
- `mso-position-vertical` selects absolute, top, center, bottom, inside, or
  outside placement within the vertical relative frame;
- the relative-frame vocabularies include physical and mirrored margin areas;
- `margin-left` and `margin-top` supply offsets for absolute positioning.

Word always honors `v:textbox@inset`. VML also defines `v-text-anchor` for
vertical text placement and `mso-fit-shape-to-text` for growing a shape to its
content.

Primary references:

- [Microsoft Office notes for VML shape positioning](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/edfd844d-bf1a-4109-81cd-4caa788f1449)
- [Microsoft Office notes for VML text boxes](https://learn.microsoft.com/en-us/openspecs/office_standards/ms-oi29500/f88c8cef-d1f4-42bf-b2da-6aa9f2dd128e)
- [VML text-box element and inset/property surface](https://learn.microsoft.com/en-us/windows/win32/vml/msdn-online-vml-textbox-element)
- [VML wrap modes](https://learn.microsoft.com/en-us/windows/win32/vml/msdn-online-vml-mso-wrap-mode-attribute)
- [VML wrap-distance semantics](https://learn.microsoft.com/en-us/windows/win32/vml/msdn-online-vml-mso-wrap-distance-left-attribute)
- [VML vertical text anchor](https://learn.microsoft.com/is-is/windows/win32/vml/msdn-online-vml-v-text-anchor-attribute)
- [VML shape-autofit behavior](https://learn.microsoft.com/en-us/windows/win32/vml/msdn-online-vml-mso-fit-shape-to-text-attribute)

### Decisions

#### 1. Keep VML parsing neutral

`vml.rs` remains a best-effort, model-independent parser. It gains neutral
position-alignment, wrap, distance, and text-body values. Mapping to
`DrawingAnchor` and `TextBoxBodyProperties` stays in `body.rs`.

Malformed or unknown tokens use the existing bounded fallback rather than
failing document import.

#### 2. Preserve all anchor frames and alignments that the model can express

VML frames map onto the existing page, margin, column/character, physical
margin-strip, and mirrored-margin anchors. Relative alignment maps to the
existing horizontal and vertical alignment variants; absolute placement keeps
the signed twip offset.

Percentage offsets remain out of scope because the model has no percentage
position representation.

#### 3. Carry VML wrap semantics through every positioned VML object

The parser records the common square, tight, through, top-and-bottom, and none
modes from `w10:wrap` or `mso-wrap-mode`, plus the four independent wrap
distances. A child shape inherits group-level wrap data when it does not
override it.

The importer maps these values onto `DrawingAnchor` for VML pictures, shapes,
and text boxes. This makes local top-and-bottom VML objects use the reflow path
already shared by body, table-cell, header, and footer content. Unsupported
square/tight/through page-level exclusion remains preserved in the model but is
still flow-neutral, as documented by the float-reflow design.

#### 4. Restore body positioning only inside the proven-safe envelope

A body VML text box becomes a float only when all of the following hold:

1. it has a genuine absolute position and positive box dimensions;
2. its vertical frame is paragraph, text, or line;
3. its wrap mode is top-and-bottom.

That is exactly the local exclusion case the layout engine can honor today.
Page- or margin-relative body boxes, no-wrap overlays, and side-wrapped boxes
remain inline. This preserves readable document order and avoids restoring the
known overprint regression.

Header/footer text boxes retain their existing positioned behavior because
they are page furniture measured and repeated in a bounded band. Degenerate
header/footer boxes still fall back inline.

#### 5. Defer every VML text box until its shape metadata is available

`w:txbxContent` closes before `w:pict`, so the current body path emits an inline
box before the VML shape has been parsed. Both body and running-content boxes
will instead queue their flowed blocks until `commit_pict`. This permits one
container-aware decision while preserving document order. An unmatched or
malformed shape drains to an inline box so text is never lost.

#### 6. Preserve text-box body properties in both float and fallback paths

Explicit VML insets map independently to the shared body-property insets.
`v-text-anchor` maps top/middle/bottom to top/center/bottom placement, and
`mso-fit-shape-to-text` maps to shape autofit.

The safe floating path also retains the authored extent, fill, and stroke. The
inline safety fallback retains inset/alignment/autofit and appearance but does
not force the authored absolute extent; using a fixed, potentially undersized
height with overflow enabled can recreate the overlap this policy prevents.

### Coverage

Required regressions:

- parser coverage for relative alignment, physical/mirrored frames, wrap mode,
  wrap distances, non-uniform inset, vertical text anchor, and shape autofit;
- positioned VML image/shape carries wrap and alignment into its anchor;
- paragraph-relative top-and-bottom body text box becomes a positioned float;
- unsafe page-relative/no-wrap body text box remains inline while retaining its
  body properties and appearance;
- positioned header and footer text boxes retain their anchor, extent, inset,
  vertical alignment, fill, and stroke;
- a positioned VML text box inside a body table cell follows the same safe
  decision and remains discoverable by the nested-float path;
- malformed/unmatched VML text-box content drains inline without loss.

### Explicit follow-ups

- percentage offsets (`mso-left-percent` / `mso-top-percent`);
- exact inside/outside odd/even parity;
- square/tight/through and page-level fixed-point reflow;
- VML vertical writing/rotation and linked text-box chains;
- exact generic VML path geometry, gradients, and per-side strokes;
- emitting legacy VML rather than normalized DrawingML on semantic export.
