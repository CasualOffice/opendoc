# Normalized Schema v1: Comments Design

**Status:** Accepted — 2026-07-25 (repository owner directive: complete the model,
100% of a round-trip must be convertible to the model)
**Tracker:** P1A-019 (schema v1 semantic extension), comments slice
**Decision basis:** ADR-027, schema v1 (`38-…`), notes (`42-…`), headers/footers
(`43-…`), extra-part media/links (`45-…`), importer no-skip audit (`P1A-025`)

## Why

The no-skip audit confirmed that `word/comments.xml` is never read in Semantic
mode: every comment's body text (and any images, tables, or text boxes inside a
comment) is silently absent from the model, the in-body `w:commentReference` is
reported by element name only with its `w:id` link dropped, and the comment
author/date/initials metadata is lost. This slice reads the comments part, models
each comment's block content and metadata as a first-class definition, and models
the in-body reference as an inline that resolves to it. It reuses the note-part
machinery wholesale (a comment is a block container keyed by `w:id`, exactly like
a footnote), so the incremental surface is small.

## Model

Comments are block containers, resolved by reference from body runs. They live in
the definition tables (like notes, styles, and numbering), not in the body:

```text
Definitions {
  … existing …
  comments: DefinitionMap<CommentId, Comment>,   // new (empty-omitted)
}

Comment {                                         // a comment's content + meta
  blocks:   Vec<BlockNode>,                       // may be empty
  author:   Option<String>,                       // <= 255 bytes, non-empty
  initials: Option<String>,                       // <= 255 bytes, non-empty
  date:     Option<String>,                       // <= 64 bytes, non-empty (ISO-8601 as written)
}

InlineNode {
  … existing …
  CommentReference(CommentReference)              // new
}

CommentReference { id: NodeId, comment: CommentId }
```

A comment's `blocks` reuse the recursive block model, so a comment may contain
paragraphs, tables, text boxes, and images — all handled by the shared body
parser. `CommentId` is a new definition id (deterministic v1 id). A
`CommentReference` is a leaf inline and a hard run-merge boundary.

Author/date/initials are retained as the producer wrote them — opaque, bounded
strings, never parsed or reformatted. A comment date is kept as its original
string so no timezone/precision information is lost.

## Strict validation (additive)

- Every `CommentReference.comment` resolves in `Definitions::comments`; a dangling
  reference is `DanglingCommentRef(NodeId)`.
- Each comment's `blocks` are validated recursively (`validate_block`), restarting
  the table/text-box depth budget (a comment is a fresh block container).
- `author`/`initials`, when present, are non-empty and at most 255 bytes
  (`comment.metadata` domain); `date`, when present, is non-empty and at most 64
  bytes (`comment.date` domain).
- Id-uniqueness includes every `CommentId` and every id inside a comment's blocks.
- Snapshot block/text limits count a comment's blocks and text.
- A comment's `blocks` may be empty (the model does not forbid an empty comment).

## v0 → v1 migration

No change. A v0 document has no comments; `Definitions::comments` is
`#[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]` so it is
omitted when empty, and every `Comment` field is `skip_serializing_if`. Existing
schema-v1 snapshots and the byte-exact migration golden are unchanged.

## Import

- `import_package` resolves the main-document `/comments` relationship (only the
  canonical relationship type — `commentsExtended`, `commentsIds`, and
  `commentsExtensible` end with different suffixes and are not matched) and reads
  it into a `PartSources` via `resolve_part_sources`, so images and external
  hyperlinks inside a comment resolve through the comment part's own `_rels`.
- `body::parse_comments` runs the body parser in note-container mode
  (`note_container == Some(b"comment")`): each `w:comment` is a block container
  keyed by `w:id`, allocated a `CommentId` in document order. `open_note` reads
  `w:author`/`w:date`/`w:initials` into a `CommentMeta` (dropped, not truncated,
  when empty or over the length bound). `close_note` unwinds text-box frames
  before finishing the paragraph, so a comment's text-box content is not dropped.
- `build_comments` maps `w:id → CommentId` for in-body resolution and adds the
  comment part's images to the shared media table before parsing.
- In-body `w:commentReference` (inside a run) resolves to a `CommentReference`
  (dangling id reported, not modeled). The range markers `w:commentRangeStart` /
  `w:commentRangeEnd` are **not** modeled and fall through to the reporter (see
  deferred, below).

Deterministic id-allocation order:
`document → styles → numbering → main media → footnotes → endnotes → headers →
footers → comments → body`.

## Explicitly deferred (reported, not modeled)

- `w:commentRangeStart` / `w:commentRangeEnd` (the anchored range a comment
  applies to) — the reference-to-definition link is modeled; the exact anchored
  span is dispositioned by element name and is a follow-up. No content is lost:
  the runs inside the range are modeled normally as body content.
- `commentsExtended` / `commentsIds` / `commentsExtensible` parts (threaded/parent
  relationships, durable ids, resolved/reply state) — reported at the package
  level, modeling deferred.
- A `w:commentReference` that appears **inside** a footnote/endnote/header/footer
  part (rather than the main body) is reported but not turned into a
  `CommentReference` node: those parts are parsed before the comment index exists
  (comments are built after them in the id-allocation order), so their
  `comment_ids` map is empty. This is the same established pattern as a
  `w:footnoteReference`/`w:endnoteReference` inside a secondary part (which also
  resolves against an empty map). The comment body itself is still fully modeled
  in `Definitions::comments`; only the in-part anchor link is dropped, and the
  drop is reported (never silent). Cross-part reference resolution is a follow-up.

## Test plan

- Model: a `CommentReference` resolves and round-trips; a dangling reference is
  rejected; an empty/oversized author/date is rejected by the domain check.
- Import: reference + body + metadata modeled; comment with no metadata modeled
  with `None` fields; dangling reference reported not modeled; a comment
  containing a text box preserves its content; oversized author/date dropped
  (not truncated); range markers reported.
