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
1 when the renderer exists, and semantic round-trip artifacts begin in Phase 2
when the DOCX writer exists. Phase 0 does not create placeholder evidence for
unimplemented capabilities.

## Phase 1 — Read-only runtime

**Duration:** 8–12 weeks

Deliver:

- core document model;
- styles and numbering;
- DOCX import;
- text shaping;
- paragraph layout;
- basic tables;
- pages;
- display list;
- native and web reference renderer;
- outline/text extraction;
- hit testing;
- Tauri and browser read-only examples.

Exit gate:

- representative documents render;
- 100-page benchmark meets read-only targets;
- visual regression system operational;
- no editing claim yet.

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
4. Copy round-trip audit methodology.
5. Define normalized schema v0.
6. Define IDs and position semantics.
7. Define error codes.
8. Add ZIP limits.
9. Add XML limits.
10. Parse package relationships.
11. Parse document body.
12. Parse paragraphs/runs.
13. Parse styles.
14. Parse numbering.
15. Parse sections.
16. Add normalized JSON debug export.
17. Add deterministic snapshot tests.
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

Mitigation: feature flags, lazy codecs, binary event/display-list transport, profiling from phase 1.

### UI pressure bypassing architecture

Mitigation: official apps may use only public SDK APIs after beta.

### Collaboration mismatch

Mitigation: collaboration is adapter-based and delayed until local transaction semantics are stable.
