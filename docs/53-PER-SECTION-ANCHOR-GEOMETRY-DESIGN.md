# Per-section anchor geometry design

Status: accepted for the bounded implementation described here.

## Problem

Multi-section pagination already flows each section with its own page size,
margins, and column layout. Floating-object placement runs afterward, but it is
given only the first section's `PageConfig`. Consequently a picture, text box, or
group anchored in a later section can resolve `relativeFrom="page"`,
`relativeFrom="margin"`, and physical margin-edge positions against stale
geometry.

The failure also affects a `continuous` section that shares a page with its
predecessor. `Page::section` can describe only one section, so choosing geometry
from the page alone loses the source section of a float in the other band.

## Decisions

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

## Tests

The bounded acceptance set must prove:

- a later next-page section with a different page size and margins positions
  page- and margin-relative floats from its own geometry;
- two continuous sections sharing one page retain different source geometries;
- a column-relative float uses the actual containing column, including through a
  table-cell paragraph;
- a header/footer float uses the section recorded for its page.

Assertions target final `PlacedAnchor` rectangles, not only section identifiers.

## Compatibility boundary

This slice does not select and measure distinct header/footer definitions for
every section; the existing running-content pass still owns that separate gap.
It does not implement exact character-relative glyph anchoring, mirrored-margin
page layout, simple-position override, percentage anchor offsets, or
square/tight/through wrapping. Inside/outside-margin parity remains tied to the
later mirrored-margin geometry slice.
