# Agent Instructions

These instructions apply to Codex, Claude Code, and any other coding agent working in this repository.

## Repository Boundary

This repository is `/Users/sachin/Desktop/melp/services/opendoc`.

The sibling repository `/Users/sachin/Desktop/melp/services/docs` is reference material only. Do not modify it unless explicitly asked. Use it only for studying existing editor behavior, competitive implementation patterns, and migration lessons.

## Mission

Build a production-grade document runtime and SDK. This is not an MVP, prototype, or side project.

The objective is a deterministic, secure, embeddable document engine with serious DOCX fidelity, native/web/headless support, stable APIs, and CI-backed quality gates.

## Required Workflow

1. Read the relevant docs before acting.
2. Design first.
3. Discuss and finalize substantial designs before implementation.
4. Update `docs/14-EXECUTION-TRACKER.md` before or alongside work.
5. Implement in small, reviewable increments.
6. Add or update tests with behavior changes.
7. Update docs and ADRs when decisions, APIs, compatibility, or workflows change.
8. Keep CI requirements current.

## Engineering Priorities

In order:

1. correctness and document safety;
2. deterministic behavior;
3. security and resource bounds;
4. compatibility and round-trip fidelity;
5. performance;
6. API stability;
7. UX quality;
8. maintainability.

## Design Rules

- Public mutation must go through commands and transactions.
- Host applications own policy: storage, network, auth, telemetry, plugins, resources.
- The runtime must not depend on the browser DOM as source of truth.
- Unsupported document data must be preserved where safe or reported explicitly.
- No silent data loss in release behavior.
- No mandatory server, React, or collaboration provider dependency.

## Documentation Rules

- Keep docs accurate as work changes.
- Prefer numbered design docs under `docs/` for durable project knowledge.
- Use ADRs for architectural decisions.
- Use the tracker for execution state.
- Record open questions instead of hiding uncertainty.

## Verification Rules

Before claiming work is complete, run the relevant checks. If checks do not exist yet, document the gap and add the expected future gate to `docs/15-CI-AND-RELEASE-GATES.md`.

For future Rust code, expect at minimum:

- formatting;
- linting;
- unit tests;
- doc tests where useful;
- fuzz/corpus hooks for parsers;
- performance benchmarks for hot paths.

## Communication

Be direct and factual. Surface risks early. Do not overstate support or fidelity. If a feature is partial, say so.
