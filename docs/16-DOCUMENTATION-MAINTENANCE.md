# Documentation Maintenance

## Purpose

Documentation is part of the implementation contract. It must remain current as architecture, APIs, compatibility, and workflows change.

## Source of Truth

Use `00-README.md` to find the active contract for a topic. Code and executable
tests establish current behavior; documents explain its contract and evidence.
An older design or audit is a dated record, not proof that a feature still has
the same status.

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

Use `109-BACKLOG.md` as the only working queue. Update the row in the same
change as the work, including when verification changes the scope or finds an
earlier claim stale. `14`, `99`, `104`, `105`, and `106` are evidence archives,
closed to new rows. Batch plans such as `168` may describe task order and
acceptance criteria, but their state must agree with `109`.
The [tracker folder](tracker/README.md) holds completed history and generated
pending and in-progress views; edit `109`, then regenerate the views. A
completed row moves into `tracker/completed.md` with its evidence.

## Document Budget

Prefer an existing contract, ADR, or backlog row to a new file. Create a
numbered document only for a durable design or decision that needs independent
review. Keep one current status statement in the backlog; link to it from
design notes instead of copying a second status table. Generated measurements
must be regenerated from their inputs.

Before merging or retiring historical documents, check references in code,
tests, ADRs, backlog rows, and the public site. Preserve needed evidence in a
current contract or the backlog, redirect references, and record the original
Git revision in [the retirement index](tracker/completed/retired-documents.md).
Keep a separate document when it remains the active contract for a safety or
compatibility decision.

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
- the `109` backlog row and any batch-plan state agree with the code;
- ADRs reflect decisions;
- CI gates reflect new checks;
- compatibility notes reflect user-visible behavior;
- docs do not overstate implemented support;
- open questions are captured.
