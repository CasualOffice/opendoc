# Phase 1A Semantic DOCX Import Design

**Status:** Proposed for discussion
**Decision date:** Not accepted
**Tracker:** P1A-001
**Implementation:** Not started

## Outcome

Load an admitted DOCX package into the normalized OpenDoc model and emit two
deterministic artifacts:

1. a semantic JSON snapshot; and
2. a complete compatibility report for every unsupported, degraded, preserved,
   blocked, or rejected construct encountered during import.

The first end-to-end path is:

```text
.docx bytes
  -> bounded DOCX package reader
  -> content types and relationship graph
  -> main document part
  -> styles, themes, numbering, sections, and media references
  -> paragraphs, runs, and basic properties
  -> normalized OpenDoc model
  -> deterministic semantic JSON + compatibility report
```

Phase 1A validates whether the normalized model can represent useful
WordprocessingML semantics before typography, pagination, rendering, or editor
integration make model changes expensive.

## Evidence

The design is based on:

- [ECMA-376](https://ecma-international.org/publications-and-standards/standards/ecma-376/),
  including WordprocessingML, Open Packaging Conventions, Markup Compatibility,
  and Transitional migration features;
- [ISO/IEC 29500-1:2016](https://www.iso.org/standard/71691.html), the current
  published fundamentals and markup-language reference as of 2026-07-24;
- [Microsoft WordprocessingML paragraph documentation](https://learn.microsoft.com/en-us/office/open-xml/word/working-with-paragraphs);
- [Microsoft OPC relationship overview](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/opc/open-packaging-conventions-overview).

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
- compatibility report schema v1;
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
- DOCX writing or round-trip claims;
- image decoding;
- automatic external-resource fetching;
- complete table, drawing, field, note, comment, tracked-change, or embedded
  object semantics.

Out-of-scope content must still be represented in the compatibility report and
handled according to the accepted preservation policy.

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
6. Stream the main document in source order, producing bounded semantic builder
   events.
7. Validate style, numbering, section, and media references.
8. Normalize IDs, property values, maps, and source-order arrays.
9. Validate the complete schema v1 model.
10. Emit the semantic snapshot and compatibility report atomically.

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
- a bounded preservation area for unsupported source constructs;
- deterministic import-generated node and definition IDs;
- an explicit schema version and deterministic v0-to-v1 migration;
- strict rejection of unknown normalized-schema fields.

OOXML element names, relationship IDs, and source part paths are provenance,
not normalized model identity. They are retained only when needed for
compatibility reporting or future preservation.

## Compatibility Report

The report is versioned independently from the normalized schema. Every entry
contains:

- stable warning or error code;
- severity;
- disposition: `preserved`, `degraded`, `omitted`, `blocked`, or `rejected`;
- feature identifier;
- source part;
- structural location without document text;
- bounded occurrence count;
- optional relationship or namespace identifier;
- concise remediation or support-phase reference.

Completeness means that every admitted part and every traversed unsupported
element, attribute, relationship, or markup-compatibility branch has an
explicit disposition. Repeated equivalent findings may be aggregated only when
the count and first bounded locations remain deterministic.

Entries are ordered by package part, source document order, stable code, and
feature identifier. Reports contain no timestamps, local paths, random values,
or document text.

## Determinism

For identical package bytes, import options, and engine version:

- relationship traversal order is stable;
- node and definition IDs are stable;
- map serialization uses lexical key order;
- semantic arrays preserve document order;
- warning aggregation and ordering are stable;
- semantic JSON and compatibility JSON are byte-identical across supported
  native platforms and WASM.

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
- expected compatibility JSON;
- package and generator provenance;
- license and SHA-256;
- explicit unsupported-feature dispositions.

Golden updates require a semantic diff and compatibility-impact review.

## Proposed Implementation Slices

Implementation begins only after this design and normalized schema v1 are
accepted.

1. Content types, package relationships, and part graph.
2. Normalized schema v1 and v0 migration.
3. Styles, document defaults, themes, and numbering.
4. Main document paragraphs, runs, text, tabs, breaks, and basic properties.
5. Sections and media references.
6. Compatibility report and preservation accounting.
7. End-to-end semantic fixtures, deterministic snapshots, fuzzing, and
   benchmarks.

Each slice updates the tracker and adds its own parser limits, fixtures, tests,
and compatibility notes.

## Acceptance Gates

- design and schema v1 are accepted before importer implementation;
- a rights-reviewed DOCX follows the complete package-to-model pipeline;
- semantic and compatibility JSON are byte-deterministic;
- every unsupported construct has an explicit deterministic disposition;
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
- unsupported XML preservation representation and byte budget;
- streaming XML parser and namespace-processing dependency;
- whether tables are rejected or preserved as opaque blocks in the first
  semantic profile;
- public SDK import result and compatibility-report API;
- stable warning-code registry ownership.
