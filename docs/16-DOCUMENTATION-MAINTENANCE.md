# Documentation Maintenance

## Purpose

Documentation is part of the implementation contract. It must remain current as architecture, APIs, compatibility, and workflows change.

## Source of Truth

The `docs/` directory is the project source of truth until code and generated API docs exist.

Root-level files provide entry points:

- `README.md`;
- `CONTRIBUTING.md`;
- `AGENTS.md`;
- `CLAUDE.md`;
- `SKILLS.md`;
- `LICENSE`.

## Update Rules

Update documentation when changing:

- public API;
- serialized schemas;
- error codes;
- command behavior;
- transaction semantics;
- document model;
- parser limits;
- compatibility behavior;
- layout/rendering assumptions;
- security policy;
- CI gates;
- release process;
- repository workflow.

## ADR Rules

Use `08-ADR-REGISTER.md` for architectural decisions. Each accepted ADR should include:

- decision;
- why;
- consequences;
- alternatives considered where useful;
- date;
- status.

## Tracker Rules

Use `14-EXECUTION-TRACKER.md` to keep execution state current. A tracker item should not remain stale after work changes direction.

## Research Freshness

When docs rely on external current information, record:

- source;
- date checked;
- summary;
- decision impact.

This is required for dependency choices, browser/WASM behavior, platform APIs, security practices, and competitor analysis.

## Documentation Review Checklist

Before closing substantial work:

- README still points to the right docs;
- tracker status is current;
- ADRs reflect decisions;
- CI gates reflect new checks;
- compatibility notes reflect user-visible behavior;
- docs do not overstate implemented support;
- open questions are captured.
