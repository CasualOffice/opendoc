# CI and Release Gates

**Status:** Accepted for Phase 0
**CI provider:** GitHub Actions
**Development toolchain:** Rust 1.96.0
**MSRV:** Rust 1.88.0
**Last updated:** 2026-08-04

## Purpose

CI is part of the product architecture. The runtime must be built with automated checks for correctness, compatibility, security, performance, and public API stability.

## Initial CI Goals

Before implementation is considered serious, CI should support:

- formatting;
- linting;
- unit tests;
- documentation checks;
- dependency audit;
- license checks;
- platform matrix build;
- WASM build;
- fixture/corpus test hooks;
- benchmark hooks;
- fuzz target hooks.

## Pull Request Contract

Every pull request and push to `main` runs required checks with stable job names:

- `format`;
- `lint`;
- `test`;
- `benchmark-smoke`;
- `fuzz-build`;
- `docs`;
- `wasm`;
- `platform`;
- `dependency-policy`;
- `repository-policy`.

Scheduled CI adds dependency advisories and a bounded seeded DOCX package fuzz
campaign. Pull-request CI builds the format-neutral ZIP, DOCX, and ODT package
fuzz targets; seeded ODT campaigns become required when the rights-reviewed ODT
corpus lands in MFIO-007.

The ODT semantic-import gate additionally requires namespace/prefix and
attribute-order invariance, strict version/document-kind checks, DTD and active
content refusal, bounded XML/text/paragraph/inline/report resources,
cooperative cancellation, normalized-model validation, and explicit findings
for every deferred construct family. The initial core-text checkpoint exercises
these properties with synthetic fixtures; rights-reviewed ODF fixtures and a
dedicated `content.xml` fuzz target remain Slice D completion requirements.

Release workflows are separate and receive no write permission during pull
request validation.

Workflow permissions default to read-only. Third-party actions are pinned to a
full commit SHA and annotated with the corresponding release. Dependabot keeps
action and Cargo updates reviewable.

Rust dependencies use the committed `Cargo.lock`, even for this library
workspace, so CI and security review operate on a reproducible graph.
Repository policy also verifies every committed fixture against the SHA-256
record in `fixtures/manifest.json`.

## Rust Toolchain Policy

Every pull request continuously checks both supported compiler boundaries:

- Rust 1.96.0 runs formatting, strict Clippy, tests, documentation, WASM,
  benchmark smoke, repository policy, and dependency policy;
- Rust 1.88.0 runs a locked workspace check with all targets and features.

The development toolchain catches current compiler and tooling behavior. The
MSRV job prevents syntax, manifest, or dependency changes from silently raising
the minimum compiler version. A change is not mergeable if either boundary
fails.

The MSRV may be raised only through an accepted ADR, updated support matrix,
release note, and green replacement CI job.

## Target Matrix

| Target | Required |
| --- | --- |
| macOS | Yes |
| Windows | Yes |
| Linux | Yes |
| `wasm32-unknown-unknown` | Yes |
| Headless CLI/service | Yes |
| Rust 1.96.0 development toolchain | Yes |
| Rust 1.88.0 MSRV | Yes |

## Rust Gates

The core compiler checks are:

```sh
cargo +1.96.0 fmt --all -- --check
cargo +1.96.0 clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo +1.96.0 test --workspace --all-features --locked
cargo +1.96.0 test --doc --workspace --all-features --locked
cargo +1.96.0 check --workspace --all-features --locked --target wasm32-unknown-unknown
cargo +1.88.0 check --workspace --all-targets --all-features --locked
```

The deterministic visual-containment gate is part of the workspace test run and
can be invoked directly with:

```sh
cargo +1.96.0 test -p casual-doc-render --test visual_containment --locked
```

It imports the repository-generated `visual-containment.docx`, paginates twice
for field-for-field determinism, validates drop-cap, cross-paragraph float, and
split-table collision invariants, then renders all five pages at 96 DPI through
the pinned bundled Roboto faces. The committed manifest records the physical
page size, renderer, font set, scale, page count, and raw RGBA FNV-1a hash.
System fonts and web-fetched host fonts are deliberately excluded from this
baseline.

Additional gates should be added as capabilities appear:

- structure-aware XML and relationship fuzzing;
- snapshot serialization tests;
- DOCX corpus import tests;
- per-format detection, ambiguity, and explicit-selection tests;
- per-format parser limits, corpus import, semantic reopen, and preservation tests;
- cross-format export compatibility reports proving that target-inexpressible
  source data is never dropped silently;
- schema/profile validation for every emitted standardized package format;
- round-trip tests;
- visual layout snapshot tests;
- benchmark regression checks;
- public API diff checks;
- schema migration tests.

### Pending comments and suggestions gates

The completeness audit in doc 81 found that the current review tests cover the
happy-path browser workflow but are not sufficient for a production-complete
tracked-change claim. The following gates are required before comments and
suggestions can graduate from a partial capability:

- validate all exported tracked-change attributes against the WordprocessingML
  schema; authored inline revision `w:id` lexical form already has a numeric
  regression gate;
- open and save editor-authored comments and revisions through at least Word
  and LibreOffice compatibility oracles, then verify semantic fixed points;
- exercise insertion, deletion, replacement, formatting, move, comment-thread,
  and decision Undo through export/reopen tests;
- run a mixed-revision editing matrix across normal text, pending revisions,
  hyperlinks, inline content controls, paragraph boundaries, lists, and tables;
- verify Original, Final, and markup projections consistently across layout,
  hit-testing, copy, search, statistics, outline, and accessibility text;
- enforce the Editing/Suggesting/Viewing command matrix for every public and UI
  mutation entry point;
- benchmark retained sidebar behavior at 100 and 1,000 review items; suggestion
  typing history is already coalesced and bounded, while scale/latency gates
  remain pending;
- run keyboard, focus-retention, screen-reader, high-contrast, narrow-viewport,
  and touch review checks.

These are tracked by P1G-REVIEW-035 through P1G-REVIEW-039. Until those slices
close the gates, CI may prove the implemented baseline but not Word/Google Docs
parity or complete tracked-change support.

## Release Gates

### Preview

- workspace builds;
- basic docs complete;
- design docs current;
- no known critical security issue;
- tracker current.

### Alpha

- feature slice complete;
- relevant tests passing;
- compatibility limitations documented;
- benchmark numbers captured;
- public API marked unstable.

### Beta

- public API reviewed;
- compatibility profile published;
- corpus thresholds met;
- security threat model reviewed;
- schema migration tests passing;
- docs and examples complete.

### Stable

- semantic versioning active;
- no known critical or high data-loss issue;
- conformance report published;
- performance report published;
- release artifacts signed or checksummed;
- changelog and migration notes complete.

## CI Tracker

| Gate | Status | Notes |
| --- | --- | --- |
| Formatting | Implemented | Required Phase 0 workflow gate. |
| Linting | Implemented | Clippy denies warnings for all targets/features. |
| Unit tests | Implemented | Native workspace and doc tests. |
| WASM build | Implemented | Foundation crates compile for `wasm32-unknown-unknown`. |
| Platform/MSRV | Implemented | macOS 15 ARM64, Windows 2025 x64, pinned Rust 1.96, and Rust 1.88 checks run on every PR. |
| Dependency policy | Implemented | Licenses, sources, versions, and RustSec advisories. |
| Fuzzing | Initial package target implemented | Pull requests compile the independently locked target; scheduled security CI runs a bounded seeded campaign. |
| Corpus tests | Package, semantic, round-trip, and generated rendering corpus implemented | Generated package/security/notes/visual fixtures plus real-producer round-trip fixtures run in workspace tests; repository policy rejects missing, unmanifested, or checksum-mismatched DOCX files. |
| Visual regression | Initial deterministic gate implemented | Rights-safe five-page containment DOCX; collision invariants and raw RGBA hash use the bundled Roboto set at a pinned page size and 96 DPI. |
| Benchmarking | Initial harness implemented | Package/model smoke is required; named-environment comparison is manual until a controlled runner is provisioned. |
| Comments and suggestions integrity | Partial | P1G-REVIEW-035 supplies numeric authored inline revision ids, scoped atomic review inverses, coalesced/bounded suggestion history, and fail-closed editor-group decisions. P1G-REVIEW-036 adds one deterministic Final-with-markup byte projection and standard one-copy `w:rPrChange` formatting with structured card deltas and import/export decisions. Doc 81 retains the pending full-schema/consumer, command-matrix, mixed-editing, scale, accessibility, and responsive gates in P1G-REVIEW-037 through P1G-REVIEW-039. |
| Release artifacts | Not started | Define before beta. |

## Failure Policy

- `main` must not knowingly remain red;
- flaky tests are bugs and cannot be solved by unconditional retry;
- platform-only failures receive a reproducer or explicit blocked tracker item;
- a security advisory is evaluated before dependency update automation is
  merged;
- checks may be temporarily relaxed only through a documented, time-bounded ADR.
