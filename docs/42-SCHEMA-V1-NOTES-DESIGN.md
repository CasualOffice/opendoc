# Normalized Schema v1: Footnotes and Endnotes Design

**Status:** Accepted — 2026-07-25 (repository owner directive: complete the model,
close the fidelity gap)
**Tracker:** P1A-019 (schema v1 semantic extension), notes slice
**Decision basis:** ADR-027, schema v1 (`38-…`), tables (`39-…`), text boxes
(`41-…`), importer no-skip audit (`P1A-025`)

## Why

The no-skip audit confirmed that `word/footnotes.xml` / `word/endnotes.xml` are
never read in Semantic mode: all footnote/endnote body text (and any images,
tables, or text boxes inside a note) is silently absent from the model, and the
in-body `w:footnoteReference`/`w:endnoteReference` is reported by element name
only, with its `w:id` link dropped. This is the one remaining LibreOffice-visible
text-fidelity gap (the footnotes fixture scores 93%). This slice reads the note
parts, models each note's block content as a first-class definition, and models
the in-body reference as an inline that resolves to it.

## Model

Notes are block containers, resolved by reference from body runs. They live in the
definition tables (like styles and numbering), not in the body:

```text
Definitions {
  … existing …
  footnotes: DefinitionMap<NoteId, Note>,   // new
  endnotes:  DefinitionMap<NoteId, Note>,   // new
}

Note { blocks: Vec<BlockNode> }             // a note's content; may be empty

InlineNode {
  … existing …
  NoteReference(NoteReference)              // new
}

NoteReference { id: NodeId, kind: NoteKind, note: NoteId }
NoteKind { Footnote, Endnote }
```

A note's `blocks` reuse the recursive block model, so a note may contain
paragraphs, tables, text boxes, and images — all handled by the shared body
parser. `NoteId` is a new definition id (deterministic v1 id). A `NoteReference`
is a leaf inline and a hard run-merge boundary.

## Strict validation (additive)

- Every `NoteReference.note` resolves in the map named by its `kind`
  (`footnotes` for `Footnote`, `endnotes` for `Endnote`); a dangling reference is
  `DanglingNoteRef(NodeId)`.
- Each note's `blocks` are validated recursively (`validate_block`), restarting
  the table/text-box depth budget (a note is a fresh block container).
- Id-uniqueness includes every `NoteId` and every id inside a note's blocks.
- Snapshot block/text limits count a note's blocks and text.
- A note's `blocks` may be empty (OOXML separator notes are skipped at import, but
  the model does not forbid an empty note).

## v0 → v1 migration

Unchanged. v0 has no notes; the migration golden is unaffected.

## Import

- `import_package` resolves the `/footnotes` and `/endnotes` relationships of the
  main document, reads those parts, and parses each with the body parser in a new
  **note-container mode**: the parser recognizes `w:footnote`/`w:endnote` as block
  containers (like `w:body`), keyed by the note's `w:id`, and returns a list of
  `(w:id, blocks)`. Separator / continuation-separator notes (`w:type` present and
  not `normal`) are skipped.
- Each note part resolves its **own** relationships
  (`DocxPackage::part_relationships`), so images and hyperlinks inside a note are
  modeled — the audit's silent-drop of note-part images is closed.
- OOXML `w:id` strings map to deterministic `NoteId`s; the maps are built before
  the body so a body `w:footnoteReference w:id`/`w:endnoteReference w:id` resolves
  to a `NoteReference` (dangling/unknown id → reported and dropped).
- Everything a note carries that is not modeled is reported; in Retention it is
  preserved.

## Round-trip and fidelity

Retention is unchanged (byte-exact). The fidelity harness, after extracting the
body, appends each footnote's and endnote's block text (in id order), so note
body text counts toward the text proxy — closing the footnotes gap.

## Out of scope (still reported + Retention-preserved)

Footnote/endnote **properties** (`w:footnotePr`: numbering format, restart,
position); the automatically-numbered mark's rendered number; comments and
tracked changes; headers/footers (the next extra-part slice).
