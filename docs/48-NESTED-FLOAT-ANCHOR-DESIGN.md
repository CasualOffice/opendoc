# Nested float anchor placement design

Status: accepted for the bounded implementation described here.

## Problem

The float collector already walks model paragraphs nested in body tables, but
page lookup checks only top-level placed fragments. A float in a table cell
therefore falls back to page zero and the page content box instead of the
paragraph that owns its anchor.

Header/footer placement has a second failure: its collector visits only
top-level placed paragraph fragments. A table row in a running-content band is
never descended, so floating drawings inside its cells are invisible.

## Decisions and invariants

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

## Compatibility boundary

This slice does not implement text wrapping around floats, per-section page
geometry, table vertical merges, or VML absolute-text-box restoration. It fixes
only discovery and paragraph-relative placement for already modeled floats.
