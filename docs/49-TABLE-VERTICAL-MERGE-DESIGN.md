# Table vertical-merge layout design

Status: accepted for the bounded implementation described here.

## Problem

`w:vMerge` survives model, import, and export, but table flow ignores it.
Restart and continuation cells are therefore measured and painted as unrelated
cells: continuation content can appear, an interior horizontal edge cuts through
the merge, vertical alignment uses one row instead of the merged box, and a page
break can separate the content owner from its continuation rows.

## Decisions and invariants

1. Resolve merge topology by half-open grid-column range before flowing cells.
   A `Continue` joins only an active `Restart` covering the same columns.
2. A conforming restart cell owns the merged content. Continuation cells retain
   model identity but contribute no content size and emit no independent paint.
   The normalized model and semantic writer remain unchanged.
3. An orphan or range-mismatched `Continue` is laid out as an ordinary cell.
   This is the deterministic, no-hidden-content fallback for malformed producer
   data.
4. Resolve ordinary row minima first. A merged cell then constrains the sum of
   the rows it covers; any deficit is assigned to the first non-exact row in the
   run. If every covered row is exact, their authored total wins and merged
   content is clipped to that total.
5. The restart fragment carries the final merged height. Composition, hit
   testing, and nested-float paragraph lookup share that cell-box height and
   resolve margins/vertical alignment against it. Paint uses the closing
   continuation cell's bottom border; interior continuation boxes are skipped,
   so no horizontal edge or duplicate shading crosses the merge.
6. Every boundary crossed by a valid merge becomes a keep-with-next table
   boundary. A merge group that fits a page moves whole when the remaining page
   space is insufficient. An oversized group stays intact and deterministically
   overflows one page rather than losing content or painting a cross-page box.
7. A merge wholly contained in repeated header rows repeats as a unit. A merge
   crossing from a repeated header row into body rows disables repetition for
   that crossing group because a header-only clone cannot reproduce the box.
8. Horizontal spans compose with vertical merges by exact grid range. A
   continuation never shifts later cells in its row.

## Compatibility boundary

This slice does not add table-style cascade, cell spacing, bidi/alignment,
floating tables, or styled/segmented border paint. Differently styled side
segments within one vertical merge continue to use the restart cell's side
appearance; segmented borders are the next independent table slice.
