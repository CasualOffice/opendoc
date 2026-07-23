# Contributing

This repository is building a production-grade document runtime. Contributions should protect correctness, fidelity, security, performance, and maintainability.

## Development Contract

Before implementation:

1. Read the relevant docs in `docs/`.
2. Write or update the design note for the work.
3. Discuss and finalize the approach.
4. Add or update the tracker item in `docs/14-EXECUTION-TRACKER.md`.
5. Identify required tests, compatibility impact, and CI gates.

During implementation:

1. Keep changes narrowly scoped.
2. Prefer existing architecture and documented module boundaries.
3. Add focused tests with each behavior change.
4. Update docs when behavior, public API, compatibility, or process changes.
5. Avoid silent data loss, implicit network access, and unbounded parsing.

Before merge:

1. Run formatting, linting, type checks, and relevant tests.
2. Confirm the tracker status is current.
3. Update ADRs for architectural decisions.
4. Document performance, security, and compatibility impact.
5. Ensure CI is green.

## Pull Request Requirements

Every PR should include:

- problem statement;
- design summary;
- tests run;
- compatibility impact;
- performance impact when relevant;
- security impact when parsing, input, file, font, image, plugin, or network behavior changes;
- public API impact;
- documentation changes;
- tracker link or item id;
- ADR link when applicable.

## Quality Bar

The default expectation is production quality:

- deterministic behavior where specified;
- stable error handling;
- no panics across public API boundaries;
- no document content in normal logs;
- bounded parsing and resource limits;
- reproducible tests;
- fixtures for document compatibility work;
- visual and structural regression coverage for layout/rendering work.

## Local Checks

The current foundation gate is:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --doc --workspace --all-features --locked
cargo check --workspace --all-features --locked --target wasm32-unknown-unknown
cargo doc --workspace --all-features --no-deps --locked
```

Dependency policy additionally uses:

```sh
cargo deny --locked check
cargo audit --deny warnings
```

## Commit Style

Use short, factual commit messages. Conventional prefixes are preferred when useful:

- `docs:`
- `feat:`
- `fix:`
- `test:`
- `ci:`
- `refactor:`
- `perf:`
- `security:`

## License

By contributing, you agree that your contribution is provided under the MIT license used by this repository.
