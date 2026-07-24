# Normalized Schema v1 Design

**Status:** Accepted — 2026-07-24 (repository owner)
**Tracker:** P1A-008
**Decision basis:** ADR-027 (`36-ADR-027-ACCEPTANCE-RECORD.md`), ADR-014
(grapheme positions), ADR-018 (strict bounded JSON v0)
**Supersedes for import:** `22-NORMALIZED-SCHEMA-V0.md` is retained as the v0
baseline; v1 is a superset reached by deterministic migration.

## Why v1

Schema v0 represents only paragraphs, text runs, and four inline marks. Phase 1A
import (doc 32) needs first-class paragraph/run properties, style and numbering
definitions, sections, theme references, and media references, plus stable
identity that the provenance sidecar (D5) can anchor to. v0 cannot hold these
without abusing marks or the inert extension map (explicitly rejected in
doc 32). v1 adds them as typed model values — never as OOXML attribute bags.

Importer content-mapping code is gated on **accepting this schema** and the
artifact schemas; the package/read layers (P1A-005, P1A-007) do not depend on it.

## Design rules (inherited)

- Typed, first-class properties — no generic key/value maps as the primary model.
- Extended grapheme clusters are the canonical text-offset unit (ADR-014; D5),
  so provenance spans share the model's position space.
- Deterministic: identical input + config → byte-identical snapshot; map keys
  serialize in lexical order; arrays preserve document order.
- Strict: unknown fields are rejected on load (ADR-018).
- OOXML element names, relationship ids, prefixes, and part paths are provenance,
  never model identity (doc 32).
- No silent data loss: anything not represented is dispositioned in the
  compatibility report per `35-DISPOSITION-TAXONOMY.md`.

## Versioned envelope

Every snapshot carries an explicit `schema_version` (`0` or `1`). A v0 document
loads unchanged; a v1 loader rejects `schema_version > 1`. The envelope adds a
`definitions` section (styles, numbering, theme, media) alongside the existing
`body`. v0 has no `definitions`; migration synthesizes an empty one.

```text
Document {
  schema_version: 1,
  id: DocumentId,
  body: [BlockNode],          // ordered
  definitions: Definitions,   // new in v1
}
```

## Body: block and inline nodes

Block nodes (ordered in `body`):

- `Paragraph { id, properties: ParagraphProperties, inlines: [InlineNode] }`

Inline nodes (ordered within a paragraph), replacing v0's flat run list:

- `Run { id, properties: RunProperties, text: String }` — text is a grapheme
  sequence; offsets into it are grapheme indices.
- `Tab { id }` — an explicit `w:tab`.
- `Break { id, kind: Line | Page | Column }` — an explicit `w:br`.

v1 keeps the flat body (no nested tables/containers); nested paragraphs from
tables, text boxes, and SDT are flattened into the body per R4, and their
container geometry lives in the preservation ledger, not the model.

## Property model

Properties are typed structs of optional, supported fields. Unsupported source
properties are not stored on the node; they are dispositioned in the report and,
when retained, recorded in the preservation ledger.

- `ParagraphProperties { style_ref: Option<StyleId>, numbering: Option<NumberingRef>,
   alignment: Option<Alignment>, indentation: Option<Indentation>,
   spacing: Option<Spacing>, ... }`
- `RunProperties { style_ref: Option<StyleId>, bold, italic, underline, strike:
   Option<bool>, color: Option<ThemeColorRef | RgbColor>, size_half_points:
   Option<u32>, font_ref: Option<ThemeFontRef | FontName>, ... }`

The v0 marks (bold/italic/underline/strike) become the corresponding
`RunProperties` booleans. Property *values* are enumerations or measured units,
not raw OOXML strings.

## Definitions

- **Styles** — `Style { id: StyleId, kind: Paragraph | Character, based_on:
  Option<StyleId>, paragraph: Option<ParagraphProperties>, run:
  Option<RunProperties> }`, plus document defaults. `based_on` forms an
  inheritance chain; **cycles are rejected with a typed error** and dispositioned
  (a cycle fixture is required, doc 32).
- **Numbering** — abstract definitions separated from instances:
  `AbstractNumbering { id, levels: [NumberingLevel] }` and
  `NumberingInstance { id, abstract_ref, overrides }`. Paragraphs reference an
  instance + level via `ParagraphProperties.numbering`.
- **Sections** — ordered `SectionBoundary` values capturing supported
  page/column metadata (size, margins, columns) without layout results. Body
  and per-paragraph `sectPr` normalize into one ordered boundary sequence.
- **Theme references** — retained as semantic intent (`ThemeColorRef`,
  `ThemeFontRef`) so color/font meaning survives without embedding the theme.
- **Media references** — `MediaReference { id, relationship_id, media_type,
  part_name }` identifying the package part without decoding bytes.

Referential integrity: every `StyleId` / numbering / media / section reference
must resolve within `definitions`; a dangling reference is a typed load error.

## Identity and determinism

- Node ids (`Paragraph`, `Run`, `Tab`, `Break`) and definition ids are
  import-generated, stable, and assigned in canonical document order.
- The import namespace / documentId seed is input-derived and independent of ZIP
  and relationship enumeration order (R3, open). Node ids share the model
  position space so split/join remaps provenance spans (D5) without re-seeding.
- Grapheme offsets are computed with the same segmentation the transaction layer
  already uses, so native and WASM agree.

## v0 → v1 migration

Deterministic and total:

- `schema_version` 0 → 1; add an empty `definitions`.
- Each v0 paragraph → v1 `Paragraph` with default `ParagraphProperties`.
- Each v0 run → v1 `Run`; its marks map to `RunProperties` booleans; text and
  ids are preserved exactly.
- No v0 construct is dropped; migration is lossless and reversible in shape.

Golden vectors assert byte-identical v0→v1 output. The migration is covered by
the schema test suite before importer code lands.

## Strict validation

On load, v1 rejects: unknown object fields; `schema_version > 1`; duplicate ids;
zero/degenerate ids; dangling style/numbering/section/media references;
`based_on` cycles; grapheme offsets outside a run's text; and any property value
outside its declared domain. Errors are typed and carry no document text.

## Provenance hosting

v1 stores no source spellings. The provenance sidecar (D5) references v1 node and
definition ids and carries the source part + document-order path + grapheme-offset
spans. Because ids are stable under the accepted split/join ops and property
edits, anchors survive editing; the model never gains OOXML identity.

## Out of scope for v1

Tables/nesting as model structure (R4 keeps them flattened + ledger geometry);
drawings, fields, comments, notes, tracked changes as model semantics (report +
ledger only); layout/pagination/rendering results; DOCX writing.

## Open items

- Exact unit encodings (twips vs EMUs vs points) per measured property — pin
  before implementation.
- The concrete `NumberingLevel` field set for the first profile.
- Whether document defaults are a distinct definition or a synthetic style id.
- R3 seed derivation (tracked separately).

## Acceptance gate

v1 is accepted as its own slice when: the field inventory above is fixed with
unit encodings; the strict validator and typed errors are specified; the
deterministic v0→v1 migration has golden vectors; and referential-integrity and
`based_on`-cycle behavior are defined. Only then does importer content-mapping
code (doc 32 slices 5–7) begin.
