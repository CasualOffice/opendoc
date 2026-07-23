# CI and Release Gates

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
```

Additional gates should be added as crates appear:

- parser fuzz smoke;
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
| Formatting | Not started | Add once workspace exists. |
| Linting | Not started | Add clippy after Rust scaffold. |
| Unit tests | Not started | Add with first crate. |
| WASM build | Not started | Add with WASM-compatible core. |
| Fuzzing | Not started | Add parser fuzz targets. |
| Corpus tests | Not started | Requires fixture plan. |
| Visual regression | Not started | Requires renderer and fixed fonts. |
| Benchmarking | Not started | Requires benchmark harness. |
| Release artifacts | Not started | Define before beta. |
