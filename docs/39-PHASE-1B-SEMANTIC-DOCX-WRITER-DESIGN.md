# Phase 1B — Semantic DOCX Writer Design (model → WordprocessingML)

**Status:** Proposed — 2026-07-25
**Tracker:** P1B-001 (first Phase-1B design; opens the export/writer phase)
**Decision basis:** ADR-027, schema v1 (`38-SCHEMA-V1-DESIGN-REFERENCE.md`),
Phase-1A semantic import (all construct families now modeled), the existing
Retention writer (`casual-doc-export::write_package`).

## Why (and how this differs from Retention export)

Phase 1A made the schema-v1 `Document` a complete, editable model of a DOCX:
every content-bearing and structural construct is first-class. The **Retention**
writer (`write_package`) already reproduces a package **byte-identically** from
the retained original parts — but it is a byte floor, blind to the model: edit
the model and Retention re-emits the *original* bytes.

Phase 1B adds the **semantic writer**: serialize the `Document` model itself back
to WordprocessingML, so an edited model becomes a valid, editable `.docx`. This
is what makes the model a source of truth (not just a lens over retained bytes)
and is the critical path to the **Tauri desktop app** (edit model → write docx).

| | Retention writer (done) | Semantic writer (this phase) |
|---|---|---|
| Input | `RetainedSource` (original part bytes) | `Document` (v1 model) |
| Output fidelity | byte-identical parts | model-equivalent, LibreOffice-valid |
| Reflects edits | no | yes |
| Round-trip proof | import→write→reopen = identical parts | import(Semantic)→write→reopen→import(Semantic) = **equal model** |

The two coexist: Retention is the safety floor for un-edited documents and for
constructs still reported-not-modeled; the semantic writer is the editable path.

## Public API (in `casual-doc-export`)

```rust
/// Serializes a v1 Document to a DOCX package. `media` supplies binary image
/// bytes by part name (the model carries MediaReference metadata, not bytes);
/// pass an empty map to omit media (the XML still references it, or drop the
/// dangling drawing — see "Media").
pub fn write_document(
    document: &Document,
    media: &BTreeMap<String, Vec<u8>>,
) -> Result<Vec<u8>, ExportError>;
```

A convenience `write_document_from_import(&Import)` pulls `media` out of the
import's `retained_source` when present (the common hybrid case: semantic XML +
retained image binaries).

## Architecture

A deterministic tree-walk emitting WordprocessingML via `quick_xml::Writer`
(no DOM), then package assembly reusing the Retention writer's ZIP path
(`zip` crate, `CompressionMethod::Stored`, fixed `DateTime`) so output is
**byte-deterministic** for a given model.

```
write_document
├── xml/document.rs   body: BlockNode/InlineNode → w:body/w:p/w:r/... + w:sectPr
├── xml/properties.rs RunProperties/ParagraphProperties → w:rPr/w:pPr
├── xml/table.rs      Table → w:tbl/w:tblPr/w:tblGrid/w:tr/w:tc
├── xml/styles.rs     Definitions.styles → word/styles.xml
├── xml/numbering.rs  Definitions abstract/instances → word/numbering.xml
├── xml/notes.rs      footnotes/endnotes → word/footnotes.xml / endnotes.xml
├── xml/headers.rs    headers/footers → word/headerN.xml / footerN.xml
├── xml/comments.rs   comments → word/comments.xml
├── rels.rs           per-part relationship graphs (r:id ↔ target)
├── content_types.rs  [Content_Types].xml (defaults + overrides)
└── package.rs        deterministic ZIP assembly (reuse Retention path)
```

### Relationship & id generation

The model stores resolved values (a hyperlink's URL, a media part name), not the
original `r:id`s. The writer **re-mints** relationship ids deterministically
(`rId1`, `rId2`, … in document order per part) and builds each part's `_rels`.
Note/header/footer/comment/style/numbering references are re-linked by the
writer's own numbering. Bookmarks re-emit `w:id`s in first-seen order.

### Content types & minimal package

`[Content_Types].xml` gets the standard defaults (`rels`, `xml`) plus overrides
for each emitted part and each media content type. `_rels/.rels` points at
`word/document.xml` as the `officeDocument`. The package is admissible by the
existing `casual-doc-ooxml` reader (the reader's invariants are the writer's
target).

## Media (the one place the model is not self-sufficient)

The model's `MediaReference` carries `{relationship_id, media_type, part_name}`
but **not the image bytes** (deliberately — the model is text). So:

- `write_document(document, media)` takes a `part_name → bytes` map. When a
  `Drawing`'s media part is present, the writer emits the media part + its
  relationship + a content-type override.
- When the bytes are **absent** (a model edited without carrying media), the
  writer drops that drawing and records it in the returned report (no dangling
  `r:embed`). This is the writer's only lossy case, and it is explicit.
- The hybrid `write_document_from_import` supplies media from the retained
  source, so a normal import→edit-text→write keeps images.

## Round-trip contract (the Phase-1B test backbone)

The **semantic fixed point**: for any real fixture,

```
import(Semantic) → write_document → DocxPackage::open → import(Semantic)  ==  the original model
```

i.e. the model survives a write/reopen unchanged. This is the dual of the
Retention byte round-trip and the primary correctness gate. A second gate:
LibreOffice opens the written `.docx` without error (the fidelity harness,
extended to the semantic writer).

Determinism gate: `write_document(m)` twice yields identical bytes.

## Slice plan (implement top-down; each additive, each its own tracker item + review)

1. **P1B-002 — core body**: `w:document/w:body`, paragraphs, runs, and the run/
   paragraph property subsets, `w:sectPr` from the section boundary. Package
   assembly + content types + `.rels`. The semantic-fixed-point harness on a
   text-only fixture.
2. **P1B-003 — tables**: `w:tbl` (grid, rows, cells, `tblPr`/`trPr`/`tcPr`,
   borders/margins/shading).
3. **P1B-004 — inline constructs**: hyperlinks (+ rels), fields, tabs/breaks,
   drawings (+ media parts/rels, the hybrid media path), note/comment references,
   bookmarks, tracked-change revisions, content controls, text boxes.
4. **P1B-005 — definition parts**: styles.xml, numbering.xml, footnotes/endnotes,
   headers/footers (+ section refs), comments.xml.
5. **P1B-006 — fidelity + corpus**: run the semantic fixed-point over the whole
   round-trip corpus; extend the LibreOffice harness; add the writer to CI.

## What is deferred / out of scope for Phase 1B

- Reproducing bytes for constructs still reported-not-modeled (Retention remains
  the path for those); the semantic writer emits only modeled constructs.
- The theme part, settings, web settings, custom XML — emitted as minimal
  defaults or omitted; not modeled, so not reconstructed.
- Perfect visual parity with the original producer's layout (the semantic writer
  targets model-equivalence + validity, not pixel fidelity — that is the
  rendering engine's concern, Phase 1D).

## Backward-compatibility / risk

New code path only; no change to the model, the importer, or the Retention
writer. The primary risk is the writer emitting XML the reader (or LibreOffice)
rejects — caught by the fixed-point + LibreOffice gates on the corpus. Bounded,
deterministic, no `unsafe`, MSRV-clean.
