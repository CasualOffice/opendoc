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
  losing safe direct properties. A named *character* (`style:family="text"`) style
  is preserved as a referenced schema-v1 `Style` identity: the run carries a
  `RunProperties.style_ref` to a `StyleKind::Character` definition (whose name is
  the original `style:name` and whose run properties are the inheritance-resolved
  subset) instead of the style being flattened onto each run. A named *paragraph*
  (`style:family="paragraph"`) style is likewise preserved: the paragraph carries a
  `ParagraphProperties.style_ref` to a `StyleKind::Paragraph` definition (with the
  inheritance-resolved paragraph properties) instead of being flattened. Character
  and paragraph styles use separate name→id maps, so a text and a paragraph style
  sharing a name are preserved as distinct definitions. The doc 96 writer re-emits
  each named style once in styles.xml and references it by name, a byte fixed point.
- Checkpoint 7 maps bounded bullet/decimal list styles, the first paragraph of
  each list item, and nested list levels into deterministic normalized numbering
  definitions. Implementation-dependent defaults, missing/conflicting levels,
  continuation and unsupported label details are reported; a list-item
  `text:start-value` maps to a per-instance numbering start override (a
  conflicting later mid-list restart is still reported);
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
- Master-page header/footer content is mapped as a bounded plain-text subset:
  `style:header`/`style:footer` become `HeaderFooterKind::Default` definitions and
  section references, `style:header-left`/`style:footer-left` become
  `HeaderFooterKind::Even` (and set `even_and_odd_headers`). Paragraphs of plain
  runs, `text:s`, `text:tab`, and `text:line-break` are mapped; run/paragraph
  formatting, headings, lists, tables, links, notes, first-page regions, and
  additional master-pages are explicit findings rather than silent loss. All
  header/footer node ids are minted from the same page-geometry generator that
  mints the section id, so identity stays deterministic and globally unique; the
  document is re-validated so any invariant violation fails the import atomically.
- Embedded-image `draw:frame`s are mapped as bounded references: an inline
  `draw:frame` with a `draw:image` whose `xlink:href` is a safe internal package
  part becomes a `Drawing` node plus a `MediaReference` (relationship/part name
  and extension-inferred media type). No image bytes are decoded or held in the
  model — the packaged part is only referenced — so semantic export cannot
  reproduce the bytes and reports the drawing as a loss (media export stays
  coupled to source preservation). External/linked hrefs, parent-directory
  traversal, absolute paths, and in-document fragments are blocked without
  fetching; `svg:width`/`svg:height` map to an EMU extent and `svg:title`/`desc`
  to alt text. The sub-parse is bounded by the same depth/element/attribute/text
  budgets as the body. Manifest media-type cross-check remains future hardening.
- A floating (anchored) image `draw:frame` — `text:anchor-type="page"` or
  `"paragraph"` — maps to an `AnchoredDrawing` (first increment): the reference
  edges come from the anchor type (page → page/page, paragraph → column/paragraph),
  `svg:x`/`svg:y` become the offset placement, and `draw:z-index` the stacking
  order. The frame's `style:family="graphic"` style supplies the text wrap
  (`style:wrap` + `style:run-through` → wrap mode and z-band; a one-sided/dynamic
  wrap degrades to square with a finding) and the `fo:margin-*` text-exclusion
  distances; positioning stays offset-only and the alignment/expanded-reference
  properties are still deferred. `char`/`frame` anchor types keep the image inline
  with a finding, and a floating frame without an extent falls back to the inline
  image. A negative or out-of-range `svg:x`/`svg:y` (the unsigned length codec
  rejects it) is clamped to zero and reported (`odf.draw.anchor-offset-clamped`).
- Document style defaults are mapped: `office:styles` `style:default-style`
  entries for the paragraph and text families feed the bounded supported subset
  (paragraph alignment; direct bold/italic/underline/strike/RGB-color/half-point
  size) into `DocumentDefaults`, the cascade base the shared model already
  honors, so unstyled runs inherit them without baking properties into each run.
  Unsupported default properties remain findings. The matching doc 96 writer
  emits an `office:styles` default-style block, forming a semantic and byte fixed
  point for this subset.
- Broader style properties, advanced list counters/label layout,
  table formatting, media, metadata, header/footer formatting and fields, and the
  remaining structures in sections 4 and 6 remain in progress. Generic
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
- scripts, macros, event listeners, and executable embedded content are never
  executed or fetched. A macro/script *storage part* declared in the manifest
  (e.g. `Basic/…`) fails package admission closed. Inline active content in
  `content.xml` (`office:scripts`, `script:event-listener`) is instead dropped
  wholesale with a security finding — the subtree is never modeled and never
  re-emitted, so no handler code survives — because an empty `office:scripts` is
  ubiquitous in real producer output and must not reject an otherwise-valid
  document;
- character data outside a modeled paragraph, and style-property children outside
  the modeled subset (`style:tab-stops`, drop-caps and background images inside
  `style:paragraph-properties`), are dropped with a finding rather than failing
  the import, so authentic LibreOffice/Word output is admitted;
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
| `draw:frame` `text:anchor-type="page"`/`"paragraph"` + image | `AnchoredDrawing` | Mapped for offset-positioned floating images (page/paragraph anchor, `svg:x`/`svg:y`, `draw:z-index`, default Square wrap); graphic-style wrap/alignment/distances deferred with a finding |
| `text:page-number`/`-count`, `text:date`, `text:time`, `text:bookmark-ref`, `text:sequence` | typed `Field` nodes | Mapped for supported field kinds; computed cache and unsupported formats dropped; a field inside a hyperlink degrades |
| `office:annotation`                         | `CommentReference` + comment definition             | Mapped as a point comment (author, date, flattened body text); the `office:annotation-end` range and thread metadata are dropped |
| `text:table-of-content`                     | block content control (`BlockSdt`, TOC gallery)     | Mapped: index-body entries become the control's blocks, `text:name`→tag; level-template source dropped; empty TOC dropped |
| `text:tracked-changes` + `text:change-start`/`-end` | inline `Revision` (insertion)               | Mapped for same-paragraph insertions (author/date from `office:change-info`); deletions/moves/format-changes/block-spanning ranges degrade |
| `draw:frame` > `draw:text-box`              | inline `TextBox`                                    | Mapped for inline boxes (body flattened to one plain-text paragraph); size/fill/border/floating anchor dropped |
| `office:forms` + `draw:control`             | FORMTEXT `Field` (text-input form)                  | Mapped for `form:text` (name); checkbox/dropdown and rich control attributes degrade |
| `meta.xml` core metadata                    | `DocumentProperties.core`                          | Title, subject, creator, description, language, dates, keywords mapped; duplicate/unsupported fields reported |
| `meta:generator`                             | `DocumentProperties.app.application`              | Mapped; remaining ODT statistics/user-defined metadata reported                                               |
| `meta:document-statistic`                    | `DocumentProperties.app`                           | Page, word, character, and paragraph counts map when numeric; unknown/malformed counters reported             |
| `meta:user-defined`                           | `DocumentProperties.custom`                       | Named text, integer, boolean, float, and date values map; unsupported/invalid values reported               |
| `meta:editing-duration`                       | `DocumentProperties.app.totalTime`                | ISO-8601 hour/minute/second durations map to bounded minutes; malformed values reported                      |
| change tracking                             | revision nodes                                     | Deferred within Slice D until pairing/order evidence is complete; preserved/reported, never silently flattened |
| formulas, scripts, events, OLE, foreign XML | none in first profile                              | Blocked or preserved/reported according to safety                                                              |

## 5. Determinism and identity

`meta.xml` is subject to the same XML depth, element, attribute, and aggregate
attribute-byte ceilings as content/style XML. Duplicate custom-property names
retain the first value and produce a deterministic degraded finding.
Metadata matching uses bounded local-name matching and does not rely on
producer-chosen XML prefixes; full foreign-namespace disambiguation remains a
future hardening item.

The first bounded `style:page-layout-properties` geometry is also mapped into a
schema-v1 section: page width/height, margins, and portrait/landscape
orientation use deterministic unit conversion and IDs.
Equal-width column count, gap, and separator flags are mapped from the same
page-layout element.
Writing modes `lr-tb`, `tb-rl`, and `bt-lr` map to the section text-direction
model.
The first master-page's `style:header`/`style:footer` (and `-left` even-page)
regions are lifted into that same section as `HeaderFooter` definitions and
references, using node ids drawn from the section id generator so header/footer
identity is deterministic and disjoint from body content.

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
   wrong-mimetype, wrong-order, extra-field, active-content-storage-part
   (manifest-declared macro/script parts), and DTD cases fail with stable
   redacted errors; inline active content in `content.xml` is instead dropped
   with a security finding (§10);
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

## 10. Published limitations (interoperability status)

This is the honest, user- and integrator-facing statement of what the ODT
adapter does and does not do today. It is a *bounded, deterministic subset* of
OpenDocument Text, not a general ODT support claim.

**Admitted and semantically mapped.** ODF 1.2–1.4 text packages: paragraphs with
a bounded property subset (alignment, indentation incl. hanging, spacing incl.
percent line-height, keep-with-next/keep-together/break-before) and runs with a
bounded direct/named-style property subset (bold, italic, underline, strike, RGB
colour, half-point size, font family, superscript/subscript, all-caps,
small-caps; parent-style chains, document style defaults); bounded bullet/decimal
lists with nesting and per-item
start-value overrides; tables with inferred/declared grids, header/repeated rows,
and rectangular merges; safe internal and external hyperlinks and paired
bookmarks; footnotes and endnotes; page-layout geometry and the first master
page's header/footer regions (plain text); embedded images as reference-only
`MediaReference` values (no image bytes enter the model); and core document
metadata.

**Admitted but dropped with a finding (never modeled, never re-emitted).**
Character data outside a modeled paragraph; style-property children outside the
subset (`style:tab-stops`, drop-caps, background images); inline active content
in `content.xml` (`office:scripts`, `script:event-listener`) — the subtree is
consumed wholesale, so no macro or handler code survives. A macro/script *storage
part* declared in the manifest (e.g. `Basic/…`) instead fails package admission
closed.

**Preserved opaquely but not modeled.** Under source retention, referenced image
bytes and safe unknown non-semantic parts (thumbnails, settings, configurations)
are carried verbatim through a `PreserveWhenSafe` edit; reserved/active-content
and orphaned parts are never repackaged.

**Deliberately out of scope.** Relax NG schema validation against the official
ODF grammar is **not** performed and no validator dependency is bundled. The
conformance bar for admission is structural well-formedness plus the bounded,
fail-closed package/content profile in §2–§7 and the security limits — not
grammar conformance. Full Relax NG validation would require a mature validator
dependency parsing untrusted schemas; it is intentionally excluded to keep the
dependency and attack surface small. `.ods`/`.odp`/other non-text bodies are also
out of scope by design.

**Not yet done (additive modeling).** The unmapped families enumerated in §9 —
advanced fields/variables, indexes/TOC and cross-references, annotations and
tracked changes, formulas and embedded objects, forms, and broader style/table
properties.

**Round-trip contract and interoperability evidence.** Nine real fixtures —
authentic LibreOffice conversions of the real-producer DOCX corpus plus a full
sample document (`fixtures/corpus/*.odt`) — are each imported, validated, and
re-exported to a **byte-exact canonical fixed point**. The guarantee is that our
*own* output round-trips stably (export of the reopened document equals export of
the twice-reopened document). Byte-equality with the *original producer bytes* is
deliberately **not** claimed: the first ingest of foreign XML mints fresh
canonical node ids and normalises producer-specific style names, citation labels,
and whitespace encoding, exactly as a semantic (lossy-by-design) importer must.
`ExactIfUnchanged` export is the separate mechanism for returning the original
bytes verbatim when the document is unedited.

## 11. Normative references

- OASIS, [OpenDocument Version 1.4, Part 2: Packages](https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part2-packages/OpenDocument-v1.4-os-part2-packages.html).
- OASIS, [OpenDocument Version 1.4, Part 3: OpenDocument Schema](https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part3-schema/OpenDocument-v1.4-os-part3-schema.html).
- OASIS, [OpenDocument Version 1.4 Relax NG schemas](https://docs.oasis-open.org/office/OpenDocument/v1.4/os/schemas/).
