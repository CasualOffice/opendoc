# Execution Tracker

## Purpose

This tracker records project execution state. It is intentionally lightweight at the beginning and should become more detailed as implementation starts.

Update this file whenever work begins, changes scope, or finishes.

## Status Values

- Not started
- Researching
- Designing
- Finalizing
- Ready
- In progress
- Blocked
- In review
- Done

## Foundation Tracker

| ID | Workstream | Status | Acceptance Gate | Notes |
| --- | --- | --- | --- | --- |
| F-001 | Repository bootstrap docs | Done | Root docs, agent docs, license, process docs added. | Initial setup complete. |
| F-002 | Project glossary | Done | Common terms defined and linked from README. | `17-GLOSSARY.md`. |
| F-003 | Support matrix | Done | OS/WASM/headless targets documented. | `18-SUPPORT-MATRIX.md`. |
| F-004 | CI design | Done | Required checks and platforms documented. | `15-CI-AND-RELEASE-GATES.md`. |
| F-005 | Workspace scaffold design | Done | Rust workspace layout finalized. | `19-WORKSPACE-SCAFFOLD-DESIGN.md`. |
| F-006 | Error code registry | Done | Stable initial error taxonomy. | `20-ERROR-CODE-REGISTRY.md`. |
| F-007 | Parser limits spec | Done | ZIP/XML/image/package limits specified. | `21-PARSER-LIMITS.md`. |
| F-008 | Normalized schema v0 design | Done | Model primitives and serialization draft accepted. | `22-NORMALIZED-SCHEMA-V0.md`. |
| F-009 | DOCX fixture corpus plan | Done | Corpus manifest format and source policy defined. | `23-DOCX-FIXTURE-CORPUS.md`. |
| F-010 | Competitive analysis pass 1 | Done | Findings recorded in `12-COMPETITIVE-ANALYSIS.md`. | Primary sources checked 2026-07-24. |
| F-011 | Phase 1 capability decomposition | Done | Phase 1A through 1D have independent scope and exit gates. | ADR-025 and `06-ROADMAP-AND-DELIVERY.md`. |
| F-012 | Apache-2.0 license policy | Done | Apache-2.0 metadata, text, fixtures, and contribution terms agree. | ADR-026. |
| P0-001 | Deterministic model transaction slice | Done | Blank document, grapheme-aware insertion, atomic transaction, snapshots, and tests. | Native, WASM, MSRV, docs, lint, and policy gates pass. |
| P0-002 | Transaction semantics and history | Done | Insert, delete, split, join, mapping, inverse, and history semantics accepted and implemented. | 17 unit tests plus SDK doc test; native/WASM/MSRV gates pass. |
| P0-003 | Normalized snapshot loading | Done | Strict schema v0 JSON load, validation, limits, and deterministic round trip. | 25 unit tests plus SDK doc test; native/WASM/MSRV gates pass. |
| P0-004 | Selection foundation | Done | Caret/range invariants and position mapping implemented. | 31 unit tests plus SDK doc test; native/WASM/MSRV gates pass. |
| P0-005 | Runtime event foundation | Done | Ordered transaction and selection events with safe subscription lifecycle. | 36 unit tests plus SDK doc test; bounded lag and atomic failure gates pass. |
| P0-006 | DOCX package reader | Done | Security-bounded ZIP admission, metadata, part reads, and generated package fixtures. | 44 unit tests plus SDK doc test; native/WASM/MSRV and parser-policy gates pass. |
| P0-007 | Benchmark and baseline harness | Done | Reproducible package/model timing, reports, and regression thresholds. | 50 unit tests plus SDK doc test; baseline comparison, native/WASM/MSRV, and CI smoke gates pass. |
| P0-008 | Phase 0 corpus and evidence closure | Done | Seven generated fixtures, fuzz infrastructure, and linked exit evidence. | Full acceptance matrix passed; see `31-PHASE-0-EXIT-REPORT.md`. |

## Active Work

| ID | Title | Owner | Status | Links |
| --- | --- | --- | --- | --- |
| P1A-001 | Semantic DOCX import design | Codex | Designing | `32-PHASE-1A-SEMANTIC-DOCX-IMPORT-DESIGN.md`; implementation is blocked until design acceptance. |

## Completed Work

| ID | Title | Completed | Notes |
| --- | --- | --- | --- |
| F-001 | Repository bootstrap docs | 2026-07-24 | Added root docs, initial license, agent instructions, process docs, CI gates, tracker, competitive analysis, UX/bug hunting, and docs maintenance. |
| F-002-F-010 | Foundation design batch | 2026-07-24 | Finalized glossary, support, CI, workspace, errors, limits, schema v0, fixture corpus, ADRs, and competitive pass 1. |
| P0-001 | Deterministic model transaction slice | 2026-07-24 | Added three-crate Rust workspace, atomic grapheme insertion, public snapshots/errors, 10 unit tests, doc test, WASM/MSRV checks, and CI/security policy. |
| P0-002 | Transaction semantics and history | 2026-07-24 | Added delete/split/join operations, mapping steps, semantic inverses, SDK undo/redo, stable history error, and atomicity coverage. |
| P0-003 | Normalized snapshot loading | 2026-07-24 | Added strict bounded JSON v0 load/export, semantic limits, duplicate/unknown rejection, redacted SDK errors, and collision-safe imported editing. |
| P0-004 | Selection foundation | 2026-07-24 | Added canonical directed session selection, strict revision/position validation, atomic edit/history mapping, and a fourth focused workspace crate. |
| P0-005 | Runtime event foundation | 2026-07-24 | Added bounded future-only event subscriptions, stable sequence ordering, explicit lag gaps, transaction/selection causes, and atomic journal publication. |
| P0-006 | DOCX package reader | 2026-07-24 | Added bounded ZIP preflight, safe path and codec policy, cancellable verified part reads, five generated fixtures, and CI checksum enforcement. |
| P0-007 | Benchmark and baseline harness | 2026-07-24 | Added four deterministic release workloads, typed reports, named-environment regression comparison, an Apple M4 baseline, and required CI smoke. |
| P0-008 | Phase 0 corpus and evidence closure | 2026-07-24 | Added two package fixtures, exact corpus policy, independently locked package-reader fuzzing, scheduled security coverage, and an accepted exit report backed by a green 12-check matrix. |
| F-011 | Phase 1 capability decomposition | 2026-07-24 | Split semantic import, typography/layout, pagination/display list, and rendering/hit testing into independently gated stages. |
| F-012 | Apache-2.0 license policy | 2026-07-24 | Adopted Apache-2.0 for the whole project before accepting external contributions. |

## Open Questions

- Should the crate family retain the `casual-doc-*` names if public package-name availability later requires a change?
- Which fixed font set should be used for deterministic layout baselines?
