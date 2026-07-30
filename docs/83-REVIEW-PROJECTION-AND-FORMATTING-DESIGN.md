# 83 — Review Projection and Formatting Changes

**Status:** Implemented by P1G-REVIEW-036.
**Date:** 2026-07-31.
**Depends on:** docs 14, 15, 45, 59, 68, 81, and 82.

## Problem

The runtime currently treats every `Revision` wrapper as transparent text.
Insertion and deletion children are therefore both shaped, painted, copied,
searched, counted, and exposed to outline/navigation queries. A replacement
contributes `OLDNEW` to ordinary document behavior. Editor-authored character
formatting is also represented as a deletion/insertion pair, so changing bold or
font duplicates otherwise identical text on the canvas.

This is both a fidelity defect and a position-safety defect: layout and document
queries do not share an explicit answer for which tracked content exists in the
active view.

## Decisions

### 1. Establish Final-with-markup as the runtime projection

The active editor projection in this slice is **Final with markup**:

- insertion and move-destination content contributes text;
- deletion and move-source content contributes zero text;
- comments, revision metadata, cards, and decision actions remain visible;
- property changes render their current value while retaining their prior value
  for Reject and review metadata.

`ModelPos.offset` is the UTF-8 byte space of this projected paragraph text.
Every layout and editor text consumer must use the same rule. Hidden revision
content therefore has zero active-view width and cannot shift a later caret,
selection, copy range, search result, statistic, outline entry, or comment/review
anchor.

Original and expanded All-Markup views remain future presentation modes. They
must not be enabled by independently changing layout: each requires an explicit
view-position mapping and read-only/editing policy. The accepted minimum in
REVIEW-GAP-006 is one documented, consistent Final-with-markup default; this
slice implements that minimum without claiming the deferred view switcher.

### 2. Keep one projection predicate

`RevisionKind` owns the closed visibility predicate. Layout flow, plain-text
collection, edit range length, rich copy, link/comment/revision anchor walks,
find, statistics, outline, accessibility-facing text, notes, and floating-content
discovery call that predicate instead of inventing local revision rules.

Imported and exported revision content remains untouched in the authoritative
model. Projection is a read/edit-coordinate policy, never a destructive model
normalization and never the DOCX source of truth.

### 3. Author formatting with standard property-change markup

Character-format suggestions use the model's existing OOXML representation:

- the run carries the **current** properties;
- `RunProperties.prop_change` carries author/date/numeric `w:id` and the complete
  **prior** property snapshot;
- optional typed OpenDoc group metadata is separate from serialized `w:id`, as in
  doc 82;
- the text and run identity exist once.

Accept removes `prop_change` and keeps current properties. Reject restores the
prior properties. Both are scoped paragraph operations with exact Undo/Redo.
Imported `w:rPrChange` values use the same card and decision path.

A multi-run editor formatting gesture shares one typed group. Before an atomic
group decision, members must be top-level, contiguous runs in one paragraph with
matching author/date/group kind and non-nested prior snapshots. Damage fails
closed.

### 4. Expose structured formatting deltas

Review inventory reports the supported old/new values rather than the generic
“Changed formatting” label. The bounded delta covers:

- bold, italic, underline, and strike;
- font family and half-point size;
- text color and highlight;
- superscript/subscript vertical alignment.

The sidebar derives readable copy from that structure. It does not inspect the
canvas or diff arbitrary JSON.

## Compatibility and safety

- DOCX import/export retains all hidden deletion/move-source content.
- Editor-authored formatting serializes as `w:rPrChange`, not a fabricated
  deletion/insertion replacement.
- Existing imported opaque property-change ids remain lossless; new ids are
  non-negative decimal strings.
- Review projection is deterministic and browser-independent.
- Mutation continues through scoped `UpdateReviewState` commands.
- Unsupported Original/All-Markup editing is not silently approximated.

## Verification

P1G-REVIEW-036 requires:

- layout tests proving deletion/move-source content has zero Final-with-markup
  width while insertion/move-destination content remains;
- edit-range and hit-test regressions proving a hidden revision before ordinary
  text does not shift its caret;
- copy, rich-copy, find, statistics, outline, comment-anchor, and review-anchor
  tests against replacement content;
- editor formatting tests proving one visible text copy, structured deltas,
  Accept/Reject, Undo/Redo, numeric `w:id`, and `w:rPrChange` export/reopen;
- imported `w:rPrChange` inventory and decision coverage;
- incomplete/mixed/cross-paragraph formatting-group rejection;
- release WASM build and full browser coverage.

Implemented verification:

- 151 model, 22 edit, 293 layout, and 76 WASM tests pass in the all-feature
  workspace gate, including explicit Original/Final projection, hidden-deletion
  edit offsets, projected hit-testing/copy/Find/statistics/Outline, collapsed
  nested review/comment anchors, one-copy formatting, multi-run atomic decisions,
  and imported/exported `w:rPrChange`;
- strict all-target/all-feature Clippy and doc tests pass;
- 20 frontend unit tests pass;
- the release WASM build passes;
- all 50 Playwright tests pass, including structured formatting cards and a
  standalone zero-width deletion card/marker.
