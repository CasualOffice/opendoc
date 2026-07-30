# 82 — Review Identity and History Safety

**Status:** Implemented by P1G-REVIEW-035.
**Date:** 2026-07-31.
**Depends on:** docs 14, 15, 45, 59, 68, and 81.

## Problem

The first comments/suggestions implementation reused `Revision.revision_id`
for two incompatible meanings:

1. the producer-facing WordprocessingML `w:id`; and
2. an OpenDoc card/decision group such as `opendoc-format:…`.

`CT_TrackChange/@w:id` is an `ST_DecimalNumber`, so exporting those OpenDoc
strings creates schema-invalid DOCX. The implementation also stores a complete
body plus the complete comments map for every suggested keystroke. Consecutive
typing shares one visible card but still retains one whole-document inverse per
character. Finally, group decisions trust a caller-supplied prefix without
validating the expected members.

## Decisions

### 1. Separate serialized revision identity from editor grouping

`Revision.revision_id` remains the lossless producer-facing OOXML value. Imported
values stay opaque so OpenDoc does not rewrite unsupported producer data.
Editor-authored revisions receive a unique, non-negative decimal value from a
document-session allocator and therefore serialize as schema-valid `w:id`.

OpenDoc decision grouping is represented separately as optional model metadata:

- a stable opaque group id;
- a closed group kind: typing, replacement, or formatting.

The semantic DOCX writer does not serialize this metadata as `w:id`, and the
importer never infers it from equal or adjacent producer ids. OOXML has no
standard replacement-card grouping field; inventing a heuristic could join
unrelated revisions destructively. A future custom-part design may preserve
OpenDoc-only grouping across DOCX reopen. Until then, standard revision content
round-trips, while the OpenDoc card grouping is session/model metadata.

### 2. Use one scoped review operation

Replace the whole-document `ReplaceReviewState` history vehicle with a
first-class `UpdateReviewState` operation. Its payload contains only:

- the complete inline vectors of the paragraphs changed by the review command;
- an optional comments-definition map when comment metadata changed.

`apply` resolves every paragraph before mutation, rejects duplicate targets,
applies the scoped replacements atomically, validates the final document, and
returns the exact previous paragraph/comment values as its inverse. A suggested
keystroke therefore retains one paragraph inverse, not the entire document and
all comments.

The comments map remains a map-level snapshot in this slice because thread
creation/deletion can affect several definitions. It is bounded by existing
document admission/model limits and is no longer copied by revision-only edits.

### 3. Coalesce suggestion typing by gesture

The existing host-owned typing-session token and exact caret continuity remain
the merge boundary. A review typing tick may merge only when:

- session ids match;
- the previous review caret equals the new start;
- the editor group id and group kind match;
- Redo is empty; and
- the action is a continuing insertion, not a new replacement range.

When ticks merge, history keeps the first tick's inverse and discards later
intermediate inverses. Undo therefore restores the paragraph before the entire
gesture in one step; Redo captures and restores the final paragraph in one step.
History stacks are capped at 256 user actions, dropping the oldest entry when
the bound is exceeded.

### 4. Validate atomic groups before deciding

Group decisions use the separate group id and require:

- exactly one deletion plus one insertion for replacement/formatting;
- one or more insertions and no other kind for typing;
- one paragraph, non-nested top-level members, contiguous document order;
- matching group kind, author, and date on every member; and
- identical text on both halves of a formatting group.

Any missing, duplicated, mixed, nested, cross-paragraph, or otherwise damaged
group fails closed without mutation. Imported revisions without OpenDoc group
metadata remain individually reviewable.

## Compatibility and safety

- Imported opaque `w:id` values are preserved exactly.
- Newly authored `w:id` values are decimal and unique among modeled inline
  revisions in the open document.
- Editor grouping never changes the standard revision id.
- Public mutation continues through `casual_doc_edit::apply`.
- No browser DOM state becomes model authority.
- The operation and history bounds are deterministic and provider-independent.

## Verification

P1G-REVIEW-035 requires:

- model serialization tests proving editor group metadata is separate;
- edit apply/inverse/rollback tests for scoped review state;
- editor-authored insertion, deletion, replacement, and formatting export tests
  asserting decimal `w:id`;
- one-gesture/one-Undo/one-Redo tests across multiple suggested keystrokes;
- history-cap and inverse-size regression tests;
- damaged/incomplete/mixed/cross-paragraph group rejection tests;
- existing comment, suggestion, move, workspace, WASM, and browser suites.
