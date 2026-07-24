# Phase 1A Semantic DOCX Import Design

**Status:** Accepted (architecture-level) — 2026-07-24
**Decision:** ADR-027; see `36-ADR-027-ACCEPTANCE-RECORD.md`
**Tracker:** P1A-001
**Implementation:** Read-path and schema-v1 slices unblocked; importer code still
gated on the accepted normalized schema v1 and artifact schemas

## Outcome

Load an admitted DOCX package into the normalized OpenDoc model and emit an
atomic, deterministic import bundle containing:

1. a semantic JSON snapshot;
2. an immutable bounded source-package snapshot;
3. source-to-model provenance;
4. a typed preservation ledger; and
5. a complete compatibility report carrying a `model_outcome` and a
   `retention_outcome` per `35-DISPOSITION-TAXONOMY.md` for every non-fully-mapped
   construct encountered during import.

The first end-to-end path is:

```text
.docx bytes
  -> bounded DOCX package reader
  -> content types and relationship graph
  -> main document part
  -> styles, themes, numbering, sections, and media references
  -> paragraphs, runs, and basic properties
  -> mapping registry
  -> normalized OpenDoc model + provenance + preservation ledger
  -> deterministic semantic JSON + source snapshot + compatibility report
```

Phase 1A validates whether the normalized model can represent useful
WordprocessingML semantics before typography, pagination, rendering, or editor
integration make model changes expensive. It does not implement DOCX writing,
but each accepted mapping defines its reverse strategy and edit-invalidation
scope before import code is accepted.

## Evidence

The design is based on:

- [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/),
  including WordprocessingML, Open Packaging Conventions, Markup Compatibility,
  and Transitional migration features;
- [ISO/IEC 29500-1:2016](https://www.iso.org/standard/71691.html), the current
  published fundamentals and markup-language reference as of 2026-07-24;
- [Microsoft WordprocessingML paragraph documentation](https://learn.microsoft.com/en-us/office/open-xml/word/working-with-paragraphs);
- [Microsoft OPC relationship overview](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/opc/open-packaging-conventions-overview).

The competitor source study and proposed fidelity boundaries are:

- `33-DOCX-ENGINE-COMPETITOR-RESEARCH.md`;
- `34-OOXML-FIDELITY-ARCHITECTURE.md`.

The importer follows relationship types and resolved targets. It does not
assume that the main document or related parts use conventional ZIP paths.

## Current Model Finding

Normalized schema v0 is intentionally insufficient for this milestone. It
represents:

- document and paragraph identity;
- paragraphs and text runs;
- bold, italic, underline, and strike marks;
- an inert bounded extension map.

It has no first-class representation for paragraph/run property values, style
definitions, numbering, sections, themes, relationships, or media references.
Import implementation cannot begin by squeezing these concepts into marks or
opaque extensions.

The v0 extension map is not an OOXML round-trip mechanism. It lacks typed
source ownership, ordering, provenance, edit invalidation, conflict handling,
and future save disposition. Phase 1A requires independent source snapshot,
provenance, and preservation-ledger contracts.

The first accepted Phase 1A design slice must define normalized schema v1,
including deterministic v0-to-v1 migration and strict v1 JSON validation.

## In Scope

### Package graph

- package content types;
- package-level office-document relationship;
- part-level relationships;
- internal target resolution and normalized part names;
- external-target identification without network access;
- missing, duplicate, cyclic, invalid, and unsupported relationship reporting.

### Semantic parts

- main WordprocessingML document body;
- paragraphs and runs;
- text, explicit tabs, and explicit breaks;
- basic paragraph and run properties;
- paragraph and character style definitions and inheritance references;
- document defaults;
- theme color and font references needed by imported properties;
- abstract numbering, numbering instances, levels, and paragraph references;
- section properties represented in the body;
- media relationships and metadata references without image decoding.

### Artifacts

- normalized schema v1 semantic snapshot;
- immutable bounded source-package snapshot;
- deterministic source-to-model provenance;
- typed preservation-ledger schema;
- compatibility report schema v1;
- versioned import and reverse-mapping registry;
- repository-owned semantic golden fixtures;
- stable importer errors and warning codes;
- import correctness and bounded-work benchmarks.

## Out of Scope

- font resolution, shaping, bidi resolution, and line breaking;
- paragraph layout and metrics;
- pagination and page caches;
- display-list generation;
- native or WASM rendering;
- hit testing and caret geometry;
- UI, browser, or Tauri hosts;
- DOCX writer implementation or round-trip claims;
- image decoding;
- automatic external-resource fetching;
- complete table, drawing, field, note, comment, tracked-change, or embedded
  object semantics.

Out-of-scope content must still be represented in the compatibility report and
handled according to the accepted preservation policy.

Writer implementation remains in Phase 2. Reverse mapping, dirty scope,
invalidation, and unsupported-save disposition are in scope as design
requirements because importer choices must not discard information that a
future writer needs.

## Import Pipeline

1. Admit the package under `PackageLimits`; no XML is read before package
   admission succeeds.
2. Parse `[Content_Types].xml` and package relationships under XML and
   relationship limits.
3. Locate the main document through its relationship type.
4. Resolve the main document's relationships and classify internal and external
   targets.
5. Parse theme, styles, and numbering definitions before resolving effective
   references from document content.
6. Process markup compatibility and stream the main document in source order,
   producing bounded source-shaped decoder events.
7. Apply versioned mapping rules to build semantic state, provenance, typed
   preservation entries, and diagnostics.
8. Validate style, numbering, section, and media references.
9. Normalize IDs, property values, maps, and source-order arrays.
10. Validate the schema v1 model and every retained source artifact.
11. Emit the import bundle atomically.

No partially valid `DocumentSession` is returned. Inspection diagnostics may be
returned with a failed import, but they cannot expose document text.

## Normalized Schema v1 Requirements

The exact JSON shape remains a blocking design decision. It must provide:

- first-class paragraph and run properties rather than OOXML attribute bags;
- style definitions with stable IDs, type, inheritance, defaults, and supported
  property values;
- numbering definitions separated from paragraph numbering references;
- ordered section boundaries and supported page/column metadata without layout
  results;
- theme references needed to retain semantic color and font intent;
- media references that identify relationship, media type, and package part
  without decoding bytes;
- deterministic import-generated node and definition IDs;
- an explicit schema version and deterministic v0-to-v1 migration;
- extended-grapheme-cluster offsets as the canonical text unit for positions and
  provenance spans (ADR-014; ADR-027 D5), pinned before importer implementation;
- strict rejection of unknown normalized-schema fields.

OOXML element names, relationship IDs, and source part paths are provenance,
not normalized model identity. They are retained in separately versioned and
bounded source artifacts when needed for diagnostics, preservation, or future
save planning.

Semantic JSON is the normalized model's deterministic diagnostic encoding. It
is not the source document representation and cannot independently support a
DOCX round-trip claim.

## Fidelity Artifacts

The exact schemas remain blocking design decisions. They must follow
`34-OOXML-FIDELITY-ARCHITECTURE.md`:

- the source snapshot records admitted parts, content types, relationships,
  hashes, retained safe bytes, and explicit non-retention dispositions;
- provenance connects semantic owners and properties to source regions and
  mapping-rule versions via a content-relative offset-span anchor (source part +
  document-order block path + character-offset span over normalized paragraph
  text + source `w:r` index), captured before run normalization and never a
  mutable model node ID (see `36-ADR-027-ACCEPTANCE-RECORD.md` D5);
- every preservation entry has a typed owner, anchor, source order, namespace
  context, byte accounting, invalidation scope, conflict policy, and planned
  save disposition;
- the mapping registry defines source vocabulary, semantic target, unconsumed
  preservation, reverse mapping, dirty scope, security policy, fixtures, and
  support state for each feature.

These artifacts are internal unless a later SDK design deliberately exposes a
bounded inspection view.

## Compatibility Report

The report is versioned independently from the normalized schema. Every entry
contains:

- stable warning or error code;
- severity;
- disposition on two axes per `35-DISPOSITION-TAXONOMY.md`: a `model_outcome`
  (`mapped`, `degraded`, or `omitted`) and a `retention_outcome` (`preserved`,
  `not-retained`, `blocked`, `rejected`, or `not-applicable`);
- feature identifier;
- source part;
- structural location without document text;
- bounded occurrence count;
- optional relationship or namespace identifier;
- concise remediation or support-phase reference.

Completeness means that every admitted part and every traversed unsupported
element, attribute, relationship, or markup-compatibility branch has an
explicit disposition on both axes of `35-DISPOSITION-TAXONOMY.md`. Repeated
equivalent findings may be aggregated only when the count and first bounded
locations remain deterministic.

Entries are ordered by package part, source document order, stable code, and
feature identifier. Reports contain no timestamps, local paths, random values,
or document text.

A `preserved` retention outcome is valid only when the report references a
validated source-snapshot or preservation-ledger record. Emitting a warning
without retaining the declared content is `not-retained`, not `preserved`.

## Determinism

For identical package bytes, import options, and engine version:

- relationship traversal order is stable;
- node and definition IDs are stable;
- map serialization uses lexical key order;
- semantic arrays preserve document order;
- warning aggregation and ordering are stable;
- source-snapshot manifests and retained-part hashes are stable;
- semantic JSON, provenance, preservation ledger, and compatibility JSON are
  byte-identical across supported native platforms and WASM.

Package ZIP entry order must not change the result when package semantics are
equivalent.

## Security and Resource Policy

- DTDs, custom entities, and external entities are rejected;
- XML parsing is namespace-aware, streaming, depth-limited, and cancellable;
- relationship targets are resolved as OPC part URIs under existing path
  safety rules;
- external targets are never fetched during import;
- every XML, relationship, text, definition, preservation, and diagnostic count
  has a secure default and non-bypassable hard ceiling;
- image bytes are referenced but not decoded;
- parser errors and reports omit document text and host paths;
- a limit or structural failure creates no session.

The XML parser dependency and any schema-generation tooling require a separate
dependency ADR before implementation.

## Fixture and Test Plan

The first semantic corpus is repository-owned or explicitly rights-reviewed and
contains:

- one real-producer DOCX with paragraphs, runs, basic properties, styles,
  numbering, sections, theme references, and one media relationship;
- one equivalent package with reordered ZIP entries;
- one Strict/Transitional dialect probe;
- missing and dangling relationship cases;
- style and numbering inheritance cycles;
- unknown namespace and markup-compatibility cases;
- external relationship cases;
- every XML, relationship, semantic, preservation, and diagnostic limit
  boundary.

Each successful fixture has:

- expected semantic JSON;
- expected provenance and preservation-ledger snapshots;
- expected compatibility JSON;
- package and generator provenance;
- license and SHA-256;
- explicit unsupported-feature dispositions.

Golden updates require a semantic diff and compatibility-impact review.

## Proposed Implementation Slices

Implementation begins only after this design and normalized schema v1 are
accepted.

1. Fidelity architecture acceptance, artifact schemas, and mapping-registry
   format.
2. Content types, package relationships, source snapshot, and part graph.
3. Normalized schema v1, v0 migration, and provenance.
4. Markup compatibility and typed preservation ledger.
5. Styles, document defaults, themes, and numbering.
6. Main document paragraphs, runs, text, tabs, breaks, and basic properties.
7. Sections and media references.
8. Compatibility reporting and preservation accounting.
9. End-to-end semantic fixtures, deterministic snapshots, fuzzing, and
   benchmarks.

Each slice updates the tracker and adds its own parser limits, fixtures, tests,
and compatibility notes.

## Acceptance Gates

- fidelity architecture, mapping registry, artifact schemas, and normalized
  schema v1 are accepted before importer implementation;
- a rights-reviewed DOCX follows the complete package-to-model pipeline;
- semantic and compatibility JSON are byte-deterministic;
- every unsupported construct has an explicit deterministic disposition;
- every `preserved` disposition references retained, bounded, validated data;
- every imported semantic feature has a declared reverse mapping,
  invalidation scope, and unsupported-save policy;
- no external resource is fetched;
- malformed and over-limit inputs return typed errors with no partial session;
- reordered package entries do not change semantic output;
- native, WASM, pinned Rust, MSRV, docs, policy, audit, corpus, fuzz, and
  benchmark gates pass;
- the exit report makes no layout, pagination, rendering, editing, or save
  claim.

## Open Decisions

- exact normalized schema v1 shape and v0 migration API;
- initial Strict versus Transitional conformance profile;
- source snapshot, provenance, and preservation-ledger schemas and byte budgets;
- mapping-registry format, ownership, versioning, and initial feature inventory;
- markup-compatibility branch selection and retention rules;
- streaming XML parser and namespace-processing dependency;
- whether tables are rejected or preserved as opaque blocks in the first
  semantic profile;
- internal `ImportBundle` and public SDK inspection/report API;
- stable warning-code registry ownership.

Each of these decisions is tracked to a chosen or pending option in the
ADR-027 acceptance record (`36-ADR-027-ACCEPTANCE-RECORD.md`). The disposition
wording is consolidated into a single proposed taxonomy in
`35-DISPOSITION-TAXONOMY.md` (decision D3, accepted). Importer
implementation is blocked until every acceptance-record decision is signed and
the shipped-code reconciliations R1–R4 are resolved.
