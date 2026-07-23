# Roadmap and Delivery Plan

## Strategy

Do not replace the existing editor in one release. Build the runtime as a parallel, test-driven engine and migrate product surfaces only after each capability passes explicit fidelity and interaction gates.

## Phase 0 — Foundation and extraction

**Duration:** 3–5 weeks
**Status:** Complete (2026-07-24)
**Exit evidence:** `31-PHASE-0-EXIT-REPORT.md`

Deliver:

- new repository/workspace;
- architecture decision records;
- CI for macOS, Windows, Linux, WASM;
- repository-owned synthetic fixture corpus, with external imports requiring
  explicit rights review;
- baseline report schemas, implemented-path reports, and capability readiness
  status;
- performance benchmark harness;
- license and contribution policy;
- minimal normalized model;
- DOCX package reader with security limits.

Exit gate:

- repeatable builds;
- corpus manifest;
- implemented-path baseline reports and future baseline ownership recorded;
- no production integration yet.

ADR-023 corrects the original dependency order: visual artifacts begin in Phase
1D when the renderer exists, and semantic round-trip artifacts begin in Phase 2
when the DOCX writer exists. Phase 0 does not create placeholder evidence for
unimplemented capabilities.

ADR-025 decomposes the original read-only phase into four independently gated
stages. Passing semantic import does not imply layout support, passing layout
does not imply pagination, and passing pagination does not imply rendering or
hit testing.

## Phase 1A — Semantic DOCX import

**Status:** Designing
**Design:** `32-PHASE-1A-SEMANTIC-DOCX-IMPORT-DESIGN.md`

Deliver:

- package content-type and relationship discovery;
- main `document.xml` body import;
- paragraphs and runs;
- basic paragraph and run properties;
- styles and themes;
- numbering definitions and references;
- section properties;
- internal relationships and media references;
- normalized semantic schema and deterministic JSON snapshots;
- complete, deterministic unsupported-content and compatibility reports.

Exit gate:

- a rights-reviewed DOCX fixture loads through the bounded package reader into
  the normalized model;
- export produces a byte-deterministic semantic JSON snapshot;
- every encountered unsupported or degraded construct is represented in the
  compatibility report;
- malformed, over-limit, externally targeted, or structurally inconsistent
  input fails with typed, redacted diagnostics and no partial session;
- no layout, pagination, rendering, editing, or save claim is made.

## Phase 1B — Typography and paragraph layout

**Status:** Not started

Deliver:

- font-provider abstraction;
- script and language segmentation;
- bidirectional text resolution;
- shaping;
- deterministic font fallback;
- line breaking;
- tabs and indentation;
- paragraph metrics and fragments.

Exit gate:

- representative paragraphs produce deterministic line fragments and metrics
  under a versioned fixed-font environment;
- Unicode script, bidi, fallback, tab, and indentation fixtures pass;
- no pagination or rendering claim is made.

## Phase 1C — Pagination and display list

**Status:** Not started

Deliver:

- pages and page ranges;
- margins and section geometry;
- paragraph fragmentation across pages;
- backend-neutral display-list primitives;
- visible-page and layout caches.

Exit gate:

- multi-section documents paginate deterministically;
- page and fragmentation invariants pass on native and WASM targets;
- cache invalidation and bounded-memory behavior are tested;
- no renderer or pointer hit-testing claim is made.

## Phase 1D — Renderer and hit testing

**Status:** Not started

Deliver:

- native reference renderer;
- WASM reference renderer;
- pointer-to-position hit testing;
- caret geometry;
- fixed-font visual regression testing.

Exit gate:

- representative documents render on native and WASM reference backends;
- fixed-font visual baselines pass within accepted tolerances;
- pointer and caret geometry agree with layout positions;
- a 100-page read-only benchmark meets the accepted target.

UI shells and Tauri integration are not Phase 1 deliverables. They begin only
after the runtime capabilities they consume have passed their own exit gates.

## Phase 2 — Core editing SDK

**Duration:** 10–14 weeks

Deliver:

- transactions;
- stable positions and mapping;
- caret/range selection;
- keyboard and pointer editing;
- IME;
- basic formatting;
- paragraphs/lists;
- undo/redo;
- incremental relayout;
- DOCX save;
- public command API.

Exit gate:

- minimal editor can load, edit, undo, save, reopen;
- supported content round-trips without data loss;
- typing and relayout targets achieved.

## Phase 3 — Office document features

**Duration:** 12–20 weeks

Deliver:

- advanced tables;
- sections;
- headers/footers;
- footnotes/endnotes;
- images/drawings;
- comments;
- tracked changes;
- fields;
- bookmarks;
- multi-column layout;
- accessibility semantics;
- clipboard interoperability.

Exit gate:

- official Tauri alpha usable for real documents;
- defined compatibility matrix published;
- automated corpus pass threshold reached.

## Phase 4 — SDK beta and third-party embedding

**Duration:** 6–10 weeks

Deliver:

- stable Rust facade;
- WASM/npm package;
- C ABI;
- API reference;
- integration guides;
- plugin API;
- sample vertical editor;
- headless CLI;
- package signing and release automation.

Exit gate:

- one independent integration completed;
- no direct access to internal crates required;
- API review and threat model complete.

## Phase 5 — Collaboration and web migration

**Duration:** 8–16 weeks

Deliver:

- collaboration adapter contract;
- bridge to current Yjs/Hocuspocus workflow where feasible;
- stable transaction wire format;
- remote presence;
- collaborative undo policy;
- WASM editor feature flag in Casual Docs;
- staged web migration.

Exit gate:

- two-user editing tests pass;
- offline/reconnect tests pass;
- no unsupported silent conflict behavior.

## Phase 6 — 1.0

Deliver:

- API stability guarantee;
- LTS branch policy;
- full compatibility report;
- security review;
- performance report;
- migration guide from current `@casualoffice/docs` editor;
- plugin and SDK examples;
- official Tauri editor built on public SDK only.

## Recommended initial team

Minimum serious team:

- 1 document-model/OOXML engineer;
- 1 text-layout/rendering engineer;
- 1 Rust systems/SDK engineer;
- 1 editor interaction/selection engineer;
- 1 QA automation/fidelity engineer;
- part-time product/design and security support.

A solo implementation is possible but should be planned in years, not months. The layout, OOXML, IME, tables, and cross-platform rendering work are each substantial.

## First 30 implementation tasks

1. Create workspace and CI.
2. Define support matrix.
3. Catalogue current DOCX fixtures.
4. Define repository-owned compatibility audit methodology.
5. Define normalized schema v0.
6. Define IDs and position semantics.
7. Define error codes.
8. Add ZIP limits.
9. Add XML limits.
10. Design normalized schema v1 and compatibility reports.
11. Parse package relationships.
12. Parse document body.
13. Parse paragraphs/runs and basic properties.
14. Parse styles, themes, and numbering.
15. Parse sections and media references.
16. Emit deterministic normalized JSON and compatibility reports.
17. Add deterministic semantic snapshot tests.
18. Define font-provider interface.
19. Select shaping stack.
20. Shape a single run.
21. Lay out a single paragraph.
22. Build a display list.
23. Render a page natively.
24. Render the same page in WASM.
25. Add visual-diff harness.
26. Add viewport and page cache.
27. Add hit testing.
28. Create Tauri read-only sample.
29. Create browser read-only sample.
30. Publish first architecture preview release.

## Release channels

- `nightly`: every main build;
- `preview`: architecture/API experiments;
- `alpha`: feature-complete slices, unstable API;
- `beta`: API freeze candidate;
- `stable`: semantic versioning and compatibility policy.

## Major risks

### Rewrite scope

Mitigation: parallel engine, narrow slices, strict exit gates.

### Typography mismatch

Mitigation: fixed-font corpus, shared shaping, deterministic font resolution, reference comparisons.

### DOCX long tail

Mitigation: compatibility profiles, preservation bags, explicit warnings, corpus-driven work.

### WASM performance and binary size

Mitigation: feature flags, lazy codecs, binary event/display-list transport,
and profiling from the first affected Phase 1 stage.

### UI pressure bypassing architecture

Mitigation: official apps may use only public SDK APIs after beta.

### Collaboration mismatch

Mitigation: collaboration is adapter-based and delayed until local transaction semantics are stable.
