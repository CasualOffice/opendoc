# Casual Docs Runtime & SDK — Architecture Blueprint

**Status:** Draft v0.1
**Audience:** CasualOffice maintainers, SDK contributors, platform integrators
**Primary implementation:** Rust
**Primary hosts:** Tauri desktop, WebAssembly/web, headless/server
**License:** MIT

## Purpose

This document set defines the initial product and engineering blueprint for a reusable document editing runtime that can power Casual Docs and can also be embedded by third parties.

The runtime is not a UI toolkit and is not a DOCX-only editor. It is a deterministic document engine with:

- a stable document model;
- editing and transaction semantics;
- layout and pagination;
- rendering;
- import/export;
- collaboration hooks;
- extension APIs;
- native and WebAssembly bindings.

## Document set

1. `01-ORD.md` — outcome and product requirements.
2. `02-ARCHITECTURE.md` — target architecture and design principles.
3. `03-HLD.md` — major components, data flow, deployment, and interfaces.
4. `04-LLD.md` — detailed modules, traits, data structures, algorithms, and error model.
5. `05-SDK-API-SPEC.md` — public SDK surface and embedding contract.
6. `06-ROADMAP-AND-DELIVERY.md` — implementation phases, milestones, staffing, and acceptance gates.
7. `07-QUALITY-SECURITY-AND-COMPATIBILITY.md` — testing, performance, security, compatibility, and release criteria.
8. `08-ADR-REGISTER.md` — initial architecture decisions.
9. `09-REPOSITORY-AND-CONTRIBUTION.md` — proposed repository structure and engineering workflow.
10. `10-PROJECT-GOAL-AND-STANDARDS.md` — production goal and non-negotiable standards.
11. `11-DESIGN-FIRST-PROCESS.md` — required research, design, tracking, and delivery flow.
12. `12-COMPETITIVE-ANALYSIS.md` — current product and SDK comparison.
13. `13-UX-AND-BUG-HUNTING.md` — UX review areas and defect policy.
14. `14-EXECUTION-TRACKER.md` — current project execution state.
15. `15-CI-AND-RELEASE-GATES.md` — automated quality and release gates.
16. `16-DOCUMENTATION-MAINTENANCE.md` — documentation ownership and freshness.
17. `17-GLOSSARY.md` — canonical project terminology.
18. `18-SUPPORT-MATRIX.md` — platform, host, format, and feature targets.
19. `19-WORKSPACE-SCAFFOLD-DESIGN.md` — accepted initial Rust workspace.
20. `20-ERROR-CODE-REGISTRY.md` — stable public error taxonomy.
21. `21-PARSER-LIMITS.md` — bounded parsing and resource policy.
22. `22-NORMALIZED-SCHEMA-V0.md` — first normalized-model contract.
23. `23-DOCX-FIXTURE-CORPUS.md` — fixture rights, metadata, and comparison policy.
24. `24-TRANSACTION-SEMANTICS.md` — edit, mapping, inverse, and history semantics.
25. `25-NORMALIZED-SNAPSHOT-IO.md` — strict bounded JSON load/export contract.
26. `26-SELECTION-FOUNDATION.md` — caret/range state and transaction mapping.
27. `27-RUNTIME-EVENT-FOUNDATION.md` — ordered bounded session event delivery.
28. `28-DOCX-PACKAGE-READER.md` — bounded ZIP package admission and part reads.
29. `29-BENCHMARK-AND-BASELINE-HARNESS.md` — reproducible timing and report contract.
30. `30-PHASE-0-CLOSURE-DESIGN.md` — corpus, fuzzing, and exit-evidence closure.
31. `31-PHASE-0-EXIT-REPORT.md` — accepted Phase 0 evidence and deferrals.

## Recommended project names

- Product/runtime: **Casual Document Runtime**
- Rust workspace repository: `opendoc`
- Core crate: `casual_doc`
- Public SDK facade: `casual_doc_sdk`
- Native renderer: `casual_doc_renderer`
- WebAssembly package: `@casualoffice/document-runtime`
- Tauri integration crate: `casual_doc_tauri`

## Core boundary

The engine owns document state, transactions, selection, layout, pagination, hit testing, and serialization.

Host applications own windows, menus, dialogs, authentication, storage policy, network policy, telemetry, and product-specific UI.

## Non-negotiable constraints

- Native-first, WebAssembly-compatible architecture.
- No browser DOM as the source of truth.
- Deterministic behavior for the same document, fonts, viewport, and engine version.
- No mandatory server dependency.
- No mandatory React dependency.
- No mandatory collaboration provider.
- Loss-aware DOCX round-trip behavior.
- Stable, versioned public API.
