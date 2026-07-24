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
| P1A-001 | Semantic DOCX import design | Codex | Accepted | `32-…`; accepted (architecture-level) 2026-07-24 via ADR-027. Importer code gated on schema v1. |
| P1A-002 | DOCX engine competitor source study | Codex | Done | `33-DOCX-ENGINE-COMPETITOR-RESEARCH.md`; extended by `37-PHASE-1A-DECISION-RESEARCH.md`. |
| P1A-003 | OOXML fidelity architecture | Codex | Accepted | `34-…`; ADR-027 **accepted** 2026-07-24 (`36-ADR-027-ACCEPTANCE-RECORD.md`). |
| P1A-005 | Read path: relationship-based main-document discovery (R1) | Claude Code | In review | Implemented in `casual-doc-ooxml`: admitter now requires only `[Content_Types].xml` + `_rels/.rels`; bounded `quick-xml` parse of `_rels/.rels` + `[Content_Types].xml`; `officeDocument` discovery (transitional+strict), root-relative target resolution with escape rejection, content-type binding, fail-closed typed errors (`MissingMainDocument`/`AmbiguousMainDocument`/`UnsupportedMainDocumentType`/`MalformedPackageXml`); `main_document_part()` accessor. Fixtures regenerated to valid OPC; 5 new tests; all gates green (fmt, clippy, test, wasm, MSRV, doc, deny, fuzz-lock, benchmark smoke). |
| P1A-006 | Deterministic import ID/namespace seed (R3) | — | Not started | Input-derived, order-independent seed; reordered-ZIP + native/WASM golden. |
| P1A-009 | Source package snapshot (Tier-1 provenance) | Claude Code | In review | `casual-doc-ooxml`: `source_snapshot()` returns a deterministic `SourcePackageSnapshot` (ordered part manifest with content types + sizes/compression, main document, relationship graph). No decompressed text. Byte-stable; 1 new test; all gates green. Hashing/part-level rels are follow-ups. |
| P1A-008 | Normalized schema v1 design + implementation | Claude Code | In review | `38-…` accepted 2026-07-24; implemented in `casual-doc-model` as additive `pub mod v1` (v0 root untouched → zero downstream breakage). Multi-agent blueprint (parallel specs → synthesis → adversarial verify) drove the design; all 5 verification fixes folded in (FontRef struct variant, `serde(default)` on overrides, seed-conditional determinism, empty-extensions rejection, canonical idempotence). Types + strict serde + validation (id-uniqueness, dangling refs, basedOn cycle/kind, numbering levels, property domains, adjacent runs) + deterministic total v0→v1 migration + 12 tests incl. byte-exact golden. All gates green. |
| P1A-007 | Content-types + main-document relationship graph | Claude Code | In review | `casual-doc-ooxml`: retain parsed `[Content_Types].xml` (`content_type()` accessor, override-then-default); resolve the main document's part-level relationships (`<dir>/_rels/<name>.rels`) into `DocumentRelationship` (id, type, raw target, `TargetMode` internal/external, resolved part), base-relative resolution with root-escape rejection; external targets never resolved/fetched; missing `_rels` → empty. 2 new tests; all gates green. |
| P1A-004 | Phase 1A design reconciliation | Claude Code | Designing | Multi-agent design-readiness assessment (verdict: accept-with-changes, 6 blockers) + adversarial verification (1 blocker + 4 minors, all fixed). Added `35-DISPOSITION-TAXONOMY.md` (dual-axis), `36-ADR-027-ACCEPTANCE-RECORD.md` (D1–D11 + reconciliations R1–R4), and `37-PHASE-1A-DECISION-RESEARCH.md` (MS Word / ONLYOFFICE / LibreOffice, cited); amended ADR-007; renamed Round-trip→Retention. Competitive research (Word/ONLYOFFICE/LibreOffice, cited in doc 37) resolved R1 (relationship-based main-doc discovery, relax admitter), R4 + R2 (hybrid flatten-then-preserve tables; run boundaries as offset spans before merge), D8 (accept Strict+Transitional, normalize at decode), D4 (Semantic+Retention modes, doc-21 ceilings, fail-closed ODC-1003), D5 (two-tier anchor: whole-part snapshot + content-relative offset spans, never node IDs), D9 (per-class security-conservative policy: inert macros, invalidate signatures, opaque-bounded OLE, external non-fetch ODC-1005). D1/D2/D3/D6/D7/D11 Proposed. Pending owner sign-off to flip Proposed→Accepted: D4/D5/D8/D9/D10 (+ D1/D2/D3/D6/D7/D11). Open follow-ups: R3 deterministic ID-seed; code changes for R1/R2/R4; add nested-package recursion-depth row to doc 21; schema v1 offset-unit choice. |

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
