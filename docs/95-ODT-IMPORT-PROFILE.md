# 95 — ODT Import Profile

**Status:** Accepted for Slice D implementation
**Date:** 2026-08-04
**Tracker:** MFIO-005
**Parent:** `94-MULTI-FORMAT-IMPORT-EXPORT-ARCHITECTURE.md`

## 1. Purpose

Define the first bounded OpenDocument Text (`.odt`) admission and semantic
import profile. This is an implementation contract, not a claim that ODT
fidelity or the public SDK surfaces are complete.

The work lands as reviewable commits:

1. ODF package/profile admission and manifest preservation facts;
2. bounded `content.xml` semantic import for core text structure;
3. styles, lists, tables, links, notes, bookmarks, media, and metadata;
4. registry integration, compatibility/preservation reporting, fixtures, fuzz,
   and full gates.

### Implementation status

- Checkpoint 1 is complete: bounded ODF 1.2–1.4 package/profile admission,
  manifest validation, encryption/active-content refusal, signature-presence
  facts, cancellation, regression tests, and the package fuzz target.
- Checkpoint 2 implements the bounded core `content.xml` pipeline: strict
  namespace/version/text-body validation; paragraphs and headings; flattened
  spans with reported deferred styling; explicit spaces, tabs, and line breaks;
  XML reference handling; semantic-fact-derived deterministic IDs; typed
  redacted failures; cancellation; and explicit compatibility findings for
  deferred constructs.
- Checkpoint 3 registers that bounded subset with definitive package-based
  detection, deterministic report translation, optional original-byte
  retention, and a dedicated `content.xml` fuzz target.
- Checkpoint 4 maps bounded external/internal hyperlinks without fetching,
  blocks unsafe URI schemes from becoming active model links, and maps bookmark
  points and paired ranges into definitions plus position-preserving markers.
  Missing/oversized targets, empty links, invalid names, and unpaired markers
  degrade explicitly without producing an invalid normalized document.
- Checkpoint 5 resolves bounded automatic paragraph/text styles from
  `content.xml` for paragraph alignment and direct bold, italic, underline,
  strike, RGB color, and half-point size. Nested spans cascade deterministically,
  explicit-off values override their parents, and unsupported style attributes
  remain findings. The matching doc 96 writer now forms a semantic fixed point
  for this subset.
- Checkpoint 6 admits optional `styles.xml` under an independent byte bound,
  resolves named paragraph/text styles and same-family parent chains for that
  subset, and reports shadowing, missing parents, and inheritance cycles without
  losing safe direct properties.
- Checkpoint 7 maps bounded bullet/decimal list styles, the first paragraph of
  each list item, and nested list levels into deterministic normalized numbering
  definitions. Implementation-dependent defaults, missing/conflicting levels,
  continuation, per-item overrides, and unsupported label details are reported;
  list count and depth are independently bounded.
- Checkpoint 8 maps tables in body order, including declared or inferred grids,
  header rows, bounded row/cell repetition, empty cells, nested tables and other
  implemented cell blocks, horizontal spans, and vertical spans. Covered cells
  must form the exact declared rectangle; orphaned, missing, overlapping, or
  out-of-range merge topology fails atomically. Table, expanded row/cell, and
  nesting limits are independent, and repeated nested content is also charged
  to the expanded paragraph/inline/text/table budgets before model construction.
- Checkpoint 9 maps inline footnote/endnote containers to typed note references
  and definitions. Note bodies reuse the recursive paragraph/list/table block
  pipeline, including when the reference occurs inside an enclosing table cell.
  Surrounding paragraph, hyperlink, span, list, and bookmark state is suspended
  and restored deterministically. Duplicate source IDs, nested notes, malformed
  containers, and over-limit note counts fail atomically. Authored citation text
  is reported as degraded because schema v1 does not model that display label.
- Style defaults and broader properties, advanced list counters/label layout,
  table formatting, media, metadata,
  and the remaining structures in sections 4 and 6 remain in progress. Generic
  WASM open/export methods and capability-driven browser Open/Save controls are
  implemented, while the native SDK and production ODT gates remain incomplete;
  this is still not a general support claim.

## 2. Normative package profile

An admitted ODT is a format-neutral `BoundedPackage` plus ODF rules:

- `META-INF/manifest.xml`, `mimetype`, and `content.xml` are required;
- `mimetype` is ZIP entry zero, stored, has no local-header extra field, and is
  exactly `application/vnd.oasis.opendocument.text` with no BOM or newline;
- the manifest root is `manifest:manifest`, and versions `1.2`, `1.3`, and `1.4`
  are accepted;
- the manifest has one root `/` entry whose media type equals `mimetype`;
- every non-`mimetype`, non-`META-INF/` package file has exactly one manifest
  file entry; duplicate, unsafe, missing, or contradictory entries fail closed;
- a `manifest:encryption-data` descendant produces a typed unsupported-encrypted
  result; encrypted payload bytes are never passed to XML parsers;
- signature files may be retained for exact unchanged export but are never
  represented as a valid signature after semantic import or edit;
- scripts, macros, event listeners, and executable embedded content are blocked
  by the first profile and never executed or fetched;
- all XML parsing rejects DTDs, external entities, malformed UTF-8/XML, excess
  depth, excess elements/attributes, and excess accumulated text.

The ODF rules above follow OpenDocument 1.4 Part 2 sections 2.2, 3.2–3.5, and
4.2–4.4. The 1.2 and 1.3 profiles use the same admitted package invariants with
their version-specific manifest value.

## 3. Document profile

`content.xml` must have an `office:document-content` root with a supported
`office:version`, one `office:body`, and an `office:text` body. Spreadsheet,
presentation, drawing, chart, and database bodies are recognized as non-text
ODF and rejected with a typed unsupported-document-kind result.

The importer maps into `casual_doc_model::v1::Document`; XML is never retained
as editor state or used as layout truth.

## 4. Semantic mapping order

The initial mapping is intentionally layered:

| ODF source                                  | Normalized destination                             | First-profile disposition                                                                                      |
| ------------------------------------------- | -------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| `text:p`, `text:h`                          | paragraph, heading paragraph properties            | Mapped                                                                                                         |
| character data, `text:span`                 | runs and run properties                            | Mapped for supported style properties; otherwise degraded/reported                                             |
| `text:s`, `text:tab`, `text:line-break`     | spaces, tab, line break                            | Mapped                                                                                                         |
| `text:a`                                    | hyperlink with visible children                    | Mapped for safe internal/external targets; no fetch                                                            |
| `text:list` / list item                     | numbering definitions and paragraph numbering refs | Mapped for bullet/number basics; unsupported label detail reported                                             |
| `table:table` / rows / cells                | recursive normalized table                         | Mapped; spans and covered cells validated                                                                      |
| `text:note`                                 | footnote/endnote definition and inline reference   | Mapped                                                                                                         |
| bookmark start/end/point                    | bookmark definitions and markers                   | Mapped                                                                                                         |
| `draw:frame` + package image                | media definition and drawing                       | Mapped for embedded package images; linked images blocked/not fetched                                          |
| `meta.xml` core metadata                    | `DocumentProperties.core`                          | Title, subject, creator, description, language, dates, keywords mapped; duplicate/unsupported fields reported |
| `meta:generator`                             | `DocumentProperties.app.application`              | Mapped; remaining ODT statistics/user-defined metadata reported                                               |
| change tracking                             | revision nodes                                     | Deferred within Slice D until pairing/order evidence is complete; preserved/reported, never silently flattened |
| formulas, scripts, events, OLE, foreign XML | none in first profile                              | Blocked or preserved/reported according to safety                                                              |

## 5. Determinism and identity

- Namespace seeds are derived from admitted semantic source facts, not ZIP entry
  order or host filenames.
- XML attribute order and manifest entry order do not change the normalized
  document, report ordering, or preservation manifest.
- Definition maps and compatibility findings use stable sorted identities.
- Import is atomic: no document or partial preservation state escapes after any
  package, XML, relationship, resource, or model-validation failure.

## 6. Preservation and reporting

The adapter envelope is tagged with the ODT format ID and adapter version. In
retention mode it owns the original bytes and bounded admitted part bytes. Parts
and constructs are classified as consumed, preserved, blocked, or rejected.

The import report uses the format-neutral dual-axis outcomes from doc 94. Safe
unknown package parts and XML constructs are preserved when feasible and always
reported. Cross-format export does not copy ODF-native opaque data into DOCX or
JSON. Exact unchanged export now returns explicitly retained original ODT bytes
under doc 96. Partial semantic ODT writing is also implemented there;
edit-tolerant preservation remains incomplete.

## 7. Security limits

In addition to `PackageLimits`, `OdfImportLimits` bounds:

- manifest, `content.xml`, and optional `styles.xml` input bytes (independently);
- XML depth, element count, attribute count, and attribute bytes;
- accumulated character data;
- paragraphs, inline nodes, tables, rows, cells, lists, notes, and nesting depth;
- retained part count and bytes;
- compatibility findings.

Configured limits may tighten but never exceed compiled hard ceilings.

## 8. Acceptance gates

Slice D is complete only when:

1. malformed, traversal, duplicate, overlapping, high-expansion, encrypted,
   wrong-mimetype, wrong-order, extra-field, active-content, and DTD cases fail
   with stable redacted errors;
2. ODF 1.2, 1.3, and 1.4 fixtures import deterministically;
3. ZIP and XML reorder tests preserve semantic identity where order is not
   meaningful;
4. core text, styles, lists, tables, links, notes, bookmarks, media, and metadata
   have positive and limit tests;
5. every unsupported construct has a compatibility/preservation outcome;
6. dedicated package/content fuzz targets compile under the independent fuzz
   lockfile;
7. workspace test, strict Clippy, rustdoc, MSRV, WASM, format, and diff gates
   pass.

## 9. Provisional coverage accounting

Coverage is not one scalar. The official ODF 1.4 Relax NG schema contains 599
distinct element names and 1,300 distinct attribute names across text,
spreadsheet, presentation, drawing, chart, database, forms, and shared style
namespaces. `.ods`, `.odp`, and other non-text bodies are intentionally outside
this runtime's word-processing model, so dividing implemented ODT elements by
the entire schema would not measure useful ODT fidelity.

Until MFIO-007 generates a versioned element/attribute disposition inventory,
the checkpoint-9 audit records these conservative engineering ranges:

| Measure | Implemented now | Remaining | Interpretation |
| --- | ---: | ---: | --- |
| Shared schema-v1 capacity for ODT-relevant semantic families | 60–70% | 30–40% | The common model already has paragraphs, runs, lists, tables, links/bookmarks, notes, sections, headers/footers, media, fields, comments, revisions, math, drawings, and document properties, largely because DOCX uses the same model. ODT-specific defaults, master/page-style semantics, indexes, variables, advanced fields, and some drawing/style concepts still need additive modeling or preservation-only treatment. |
| ODT adapter coverage of the broad ODT semantic/schema surface | 20–30% | 70–80% | Counts implemented import/export mappings, not merely model types. Core text, a bounded style/list subset, links/bookmarks, table structure/merges, and notes are mapped; the remaining families listed below are not. |
| Typical editable text-document feature set | 50–60% | 40–50% | A user-weighted view of ordinary prose documents, not a standards-conformance percentage. It gives more weight to text, common formatting, lists, tables, links, and notes than to scripts, forms, indexes, embedded objects, or specialized fields. |
| Package admission and security profile | 75–85% | 15–25% | Core MIME/manifest/version/ZIP bounds, encryption refusal, DTD and active-content controls are implemented; signature validation, broader producer corpus evidence, and conformance/interoperability campaigns remain. |

The largest remaining ODT adapter families are style defaults and broader
paragraph/run/table properties; page/master styles, sections, headers, and
footers; frames and embedded media; metadata; fields, variables, indexes/TOC,
and cross-references; annotations and tracked changes; formulas and embedded
objects; edit-tolerant preservation; and schema/corpus/interoperability gates.
These ranges must move only with an auditable inventory, not by counting tests
or treating partial feature families as complete.

## 10. Normative references

- OASIS, [OpenDocument Version 1.4, Part 2: Packages](https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part2-packages/OpenDocument-v1.4-os-part2-packages.html).
- OASIS, [OpenDocument Version 1.4, Part 3: OpenDocument Schema](https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part3-schema/OpenDocument-v1.4-os-part3-schema.html).
- OASIS, [OpenDocument Version 1.4 Relax NG schemas](https://docs.oasis-open.org/office/OpenDocument/v1.4/os/schemas/).
