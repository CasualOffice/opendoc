# 62 — Footnote and Endnote Pagination Design

**Status:** Proposed implementation design.
**Date:** 2026-07-28
**Depends on:** `P1F-INLINE-FLOOR` note reference marks.

## Problem

Footnote and endnote definitions are modeled and semantically round-tripped, but
layout currently treats note bodies as invisible. `Page::footnotes` exists, yet
`paginate.rs::build_page` initializes it empty and no pass reserves space or
places note content. This causes two separate fidelity failures:

1. reference marks may be visible only after `P1F-INLINE-FLOOR`;
2. note bodies do not reserve bottom-page space, so body content and page counts
   diverge from Word-like layout.

This design covers note-body placement. It does not change import/export
semantics and does not attempt byte-exact Word XML regeneration.

## Goals

- Place footnote bodies on the page containing each footnote reference.
- Reserve a deterministic bottom band and repaginate when the reservation changes
  body flow.
- Flow note content through the same block pipeline as body/header/footer content,
  including paragraphs, tables, inline images, text boxes, and page fields.
- Keep pagination terminating under oversized notes, recursive notes, malformed
  references, and repeated reflow.
- Preserve current behavior for documents without note references.
- Keep endnotes visible in a bounded first implementation, while leaving full
  per-section/end-of-document policy as explicit follow-up if needed.

## Non-Goals

- Full Word-compatible continuation notes across many pages.
- Footnote separator/continuation separator customization.
- Restart and custom-number-format parity beyond modeled reference identity.
- Editing operations for note bodies.
- Generic conversion of unsupported note internals.

## Data Flow

### 1. Reference Discovery

The layout layer needs note anchors after inline collection. `P1F-INLINE-FLOOR`
already turns a `NoteReference` into a superscript marker run. This design adds
a note-reference side channel to paragraph flow:

- `FlowItem::NoteReference { kind, note }` or equivalent marker metadata attached
  to the shaped line;
- `Line` carries zero or more note references in model order;
- `paragraph_hash` includes note reference ids so cached galley reuse stays
  correct.

The visible superscript glyph remains a normal run. The note metadata is for
pagination only.

### 2. Note Body Galley

Before pagination, the document driver builds a deterministic note cache:

- each footnote definition becomes a `Vec<BlockFragment>` by calling the shared
  block-flow path at the current section content width;
- note bodies must not themselves trigger note placement; nested note references
  inside note bodies are visible markers only for this slice;
- missing definitions are impossible after model validation, but defensive lookup
  treats them as absent rather than panicking.

For multi-section documents, footnotes are flowed at the owning section's content
width. A future exact implementation may use the note area's width after separator
geometry is added; for this slice the section content width is the correct bounded
base.

### 3. Fixed-Point Reservation

Footnotes affect available body height, and available body height affects which
page a reference lands on. The driver therefore runs a bounded convergence loop:

1. paginate body with the current per-page footnote reservations;
2. collect footnote references from placed body fragments per page;
3. compute each page's required footnote band height from the referenced note
   galleys, capped by a deterministic maximum;
4. shrink each page's body content area by that reserved height and repaginate;
5. stop when page reservations and reference-to-page assignments are unchanged.

The loop has a hard iteration cap. If it does not converge, the engine uses the
last deterministic layout and emits/report-surfaces a compatibility limitation
once that reporting seam exists for layout. Until then, tests must cover stable
convergence for the supported cases.

### 4. Placement

When convergence completes, each page receives `Page::footnotes`:

- the band is placed at the bottom of `Page::content_area` after reservation;
- note fragments stack top-to-bottom in reference order;
- the page body remains above the reserved band;
- oversized note content overflows inside the footnote band rather than causing
  unbounded pagination loops.

The display-list compositor already paints `Page::footnotes` as placed fragments
or must be extended to do so in the same pass that paints body fragments.

### 5. Endnotes

Endnotes are not page-bottom footnotes. The first bounded implementation should
make them visible without destabilizing body pagination:

- reference marks are visible through `P1F-INLINE-FLOOR`;
- endnote bodies are appended after the last body block, or after the owning
  section when the modeled section policy is consumed;
- the appended endnote block run uses ordinary body pagination, not `Page::footnotes`.

Full `w:endnotePr` placement parity (`docEnd`/`sectEnd`) and restart behavior can
land after the footnote fixed-point loop is stable.

## Implementation Slices

### Slice A — Footnote Metadata Through Layout

- Add note-reference metadata to shaped lines.
- Keep the visible marker behavior from `P1F-INLINE-FLOOR`.
- Add tests proving a body paragraph, table cell, header/footer, and text box can
  expose the reference metadata without changing visible marker output.

### Slice B — Single-Section Footnote Bands

- Build footnote galleys for definitions.
- Add the fixed-point reservation loop for single-column single-section documents.
- Fill `Page::footnotes` and paint it.
- Acceptance: a reference near the page bottom pushes body content to the next
  page when its note band needs space.

### Slice C — Tables, Split Paragraphs, And Running Content

- Ensure references discovered in split paragraphs land on the page containing
  the split line.
- Ensure references in table cells follow row/table pagination.
- Do not reserve footnote bands for references inside headers/footers in this
  slice; those markers remain visible but note-body placement is body-flow owned.

### Slice D — Multi-Section And Endnote Visibility

- Resolve footnotes using the current page's section width and page geometry.
- Append endnote bodies according to the simplest modeled policy.
- Add generated corpus fixtures for footnote and endnote references.

## Invariants

- A document with no note references produces byte-identical `PaginatedLayout`.
- The convergence loop terminates under a fixed iteration cap.
- `repaginate == paginate` remains true for supported single-section cases.
- Footnote bands are page-local and deterministic.
- Note body flow reuses the same layout pipeline as ordinary block content.
- Note reference markers are never silently dropped, even if note body placement
  is degraded.

## Tests

Required synthetic tests:

- one short footnote on a single page;
- a footnote that forces body content to the next page;
- two references on the same page preserving source order;
- a split paragraph whose second-page reference places its note on page two;
- a table-cell reference in a row split across pages;
- a note body containing a table and inline image;
- an oversized note that terminates with bounded overflow;
- an endnote body appended after the main body;
- no-reference document layout is unchanged.

The restricted real-document probes from doc 60 remain local evidence only until
rights-approved fixtures exist.

## Open Questions

- What public compatibility-report seam should layout use for non-convergence or
  unsupported continuation-note behavior?
- Should the first implementation reserve a separator line, and if so should it
  be modeled as display-list paint or a synthetic paragraph fragment?
- Should note numbering use source definition order, explicit `w:numFmt`, or a
  later field-like numbering resolver?
