# Normalized Schema v1: Headers and Footers Design

**Status:** Accepted — 2026-07-25 (repository owner directive: complete the model)
**Tracker:** P1A-019 (schema v1 semantic extension), headers/footers slice
**Decision basis:** ADR-027, schema v1 (`38-…`), notes (`42-…`), importer no-skip
audit (`P1A-025`)

## Why

The no-skip audit confirmed header/footer parts are never read in Semantic mode:
their text, tables, text boxes, and images are silently absent, and the
`w:headerReference`/`w:footerReference` in `w:sectPr` is reported by name only.
This slice reads the header/footer parts, models each as a first-class
definition, and records which section references which header/footer for which
page type.

## Model

Headers and footers are block containers referenced from section boundaries:

```text
Definitions {
  … existing …
  headers: DefinitionMap<HeaderFooterId, HeaderFooter>,   // new
  footers: DefinitionMap<HeaderFooterId, HeaderFooter>,   // new
}

HeaderFooter { blocks: Vec<BlockNode> }   // content; may be empty

SectionBoundary {
  … existing …
  headers: Vec<HeaderFooterRef>,   // new (additive, omitted when empty)
  footers: Vec<HeaderFooterRef>,   // new
}

HeaderFooterRef { kind: HeaderFooterKind, reference: HeaderFooterId }
HeaderFooterKind { Default, First, Even }
```

A header/footer's `blocks` reuse the recursive block model, so it may contain
paragraphs, tables, text boxes, and images. `HeaderFooterId` is a new definition
id. A section may reference at most one header and one footer of each kind.

## Strict validation (additive)

- Each `HeaderFooterRef.reference` resolves in the map named by its position
  (`headers` for a `SectionBoundary.headers` entry, `footers` for a `footers`
  entry); a dangling reference is `DanglingHeaderFooterRef(NodeId)`.
- Each header/footer's `blocks` are validated recursively (fresh table/text-box
  depth), like note blocks.
- Id-uniqueness includes every `HeaderFooterId` and every id inside a
  header/footer's blocks; snapshot limits count them.
- `blocks` may be empty.

## v0 → v1 migration

Unchanged (v0 has no headers/footers); existing snapshots serialize identically
because the new maps and section vectors are omitted when empty.

## Import

- `import_package` resolves the `/header` and `/footer` relationships of the main
  document, reads each part, and parses it with the body parser in a
  **single-container mode** (the `w:hdr`/`w:ftr` root acts as document+body),
  returning one block list. Each part maps to a deterministic `HeaderFooterId`,
  and a relationship-id → `HeaderFooterId` index is built for both headers and
  footers.
- Body `w:sectPr` `w:headerReference`/`w:footerReference` (`r:id`, `w:type`)
  resolve `r:id` → `HeaderFooterId` and add a `HeaderFooterRef` (kind from
  `w:type`: `first`/`even`, else `default`) to the section's `headers`/`footers`.
  A reference whose `r:id` does not resolve is reported.
- Id order: document → styles → numbering → media → footnotes → endnotes →
  headers → footers → body.
- Header/footer-part media and external hyperlinks are reported in this slice
  (their modeling shares the note-part follow-up), never silently dropped.

## Round-trip and fidelity

Retention is unchanged (byte-exact). LibreOffice's txt export does not render
header/footer text, so the fidelity proxy is unaffected; correctness is covered
by unit tests asserting the modeled content and references.

## Out of scope (still reported + Retention-preserved)

Header/footer-part media and external hyperlinks (shared follow-up); the
`w:titlePg`/even-odd document settings that select which header applies; VML
images.
