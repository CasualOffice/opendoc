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
| F-002 | Project glossary | Not started | Common terms defined and linked from README. | Required before implementation. |
| F-003 | Support matrix | Not started | OS/WASM/headless targets documented. | Required for CI planning. |
| F-004 | CI design | Not started | Required checks and platforms documented. | See `15-CI-AND-RELEASE-GATES.md`. |
| F-005 | Workspace scaffold design | Not started | Rust workspace layout finalized. | Based on HLD. |
| F-006 | Error code registry | Not started | Stable initial error taxonomy. | Needed before public API. |
| F-007 | Parser limits spec | Not started | ZIP/XML/image/package limits specified. | Security gate. |
| F-008 | Normalized schema v0 design | Not started | Model primitives and serialization draft accepted. | Blocks parser work. |
| F-009 | DOCX fixture corpus plan | Not started | Corpus manifest format and source policy defined. | Blocks fidelity gates. |
| F-010 | Competitive analysis pass 1 | Not started | Findings recorded in `12-COMPETITIVE-ANALYSIS.md`. | Required before UX baseline. |

## Active Work

| ID | Title | Owner | Status | Links |
| --- | --- | --- | --- | --- |
| F-001 | Repository bootstrap docs | Codex | Done | README, CONTRIBUTING, LICENSE, AGENTS, numbered docs. |

## Completed Work

| ID | Title | Completed | Notes |
| --- | --- | --- | --- |
| F-001 | Repository bootstrap docs | 2026-07-24 | Added root docs, MIT license, agent instructions, process docs, CI gates, tracker, competitive analysis, UX/bug hunting, and docs maintenance. |

## Open Questions

- What exact repository/package name should be used for the Rust workspace?
- Should the initial license remain MIT through first release, or move to Apache-2.0 later?
- Which CI provider and release channels should be treated as authoritative?
- Which fixed font set should be used for deterministic layout baselines?
