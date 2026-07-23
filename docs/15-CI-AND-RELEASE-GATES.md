# CI and Release Gates

**Status:** Accepted for Phase 0
**CI provider:** GitHub Actions
**Last updated:** 2026-07-24

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
campaign.
Release workflows are separate and receive no write permission during pull
request validation.

Workflow permissions default to read-only. Third-party actions are pinned to a
full commit SHA and annotated with the corresponding release. Dependabot keeps
action and Cargo updates reviewable.

Rust dependencies use the committed `Cargo.lock`, even for this library
workspace, so CI and security review operate on a reproducible graph.
Repository policy also verifies every committed fixture against the SHA-256
record in `fixtures/manifest.json`.

## Target Matrix

| Target | Required |
| --- | --- |
| macOS | Yes |
| Windows | Yes |
| Linux | Yes |
| `wasm32-unknown-unknown` | Yes |
| Headless CLI/service | Yes |

## Future Rust Gates

Expected commands once the workspace exists:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --doc --workspace
cargo build --target wasm32-unknown-unknown
cargo test --workspace --locked
```

Additional gates should be added as capabilities appear:

- structure-aware XML and relationship fuzzing;
- snapshot serialization tests;
- DOCX corpus import tests;
- round-trip tests;
- visual layout snapshot tests;
- benchmark regression checks;
- public API diff checks;
- schema migration tests.

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
| Platform/MSRV | Implemented | macOS 15 ARM64, Windows 2025 x64, and Rust 1.85 checks. |
| Dependency policy | Implemented | Licenses, sources, versions, and RustSec advisories. |
| Fuzzing | Initial package target implemented | Pull requests compile the independently locked target; scheduled security CI runs a bounded seeded campaign. |
| Corpus tests | Phase 0 package corpus implemented | Seven generated package/security fixtures run in workspace tests; semantic and visual layers remain pending. |
| Visual regression | Not started | Requires renderer and fixed fonts. |
| Benchmarking | Initial harness implemented | Package/model smoke is required; named-environment comparison is manual until a controlled runner is provisioned. |
| Release artifacts | Not started | Define before beta. |

## Failure Policy

- `main` must not knowingly remain red;
- flaky tests are bugs and cannot be solved by unconditional retry;
- platform-only failures receive a reproducer or explicit blocked tracker item;
- a security advisory is evaluated before dependency update automation is
  merged;
- checks may be temporarily relaxed only through a documented, time-bounded ADR.
