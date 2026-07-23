# Repository and Contribution Plan

## 1. Repository decision

The runtime is developed in the separate repository:

`CasualOffice/opendoc`

Do not place the Rust runtime inside `CasualOffice/docs`. The runtime has a broader product boundary, release lifecycle, issue taxonomy, and consumer ecosystem.

The existing `docs` repository becomes one host/consumer during migration.

## 2. Branch policy

- `main` must compile and pass required tests;
- short-lived feature branches;
- protected main;
- required review for public API, unsafe code, parser, renderer, and schema changes;
- signed tags for releases.

## 3. Pull request requirements

Every PR should include:

- problem and design summary;
- tests;
- compatibility impact;
- performance impact when relevant;
- security impact when parsing/input changes;
- public API impact;
- fixture additions for DOCX behavior;
- ADR link for architectural changes.

## 4. Labels

- `area:model`
- `area:transactions`
- `area:layout`
- `area:rendering`
- `area:ooxml`
- `area:wasm`
- `area:ffi`
- `area:collaboration`
- `area:plugins`
- `area:accessibility`
- `kind:bug`
- `kind:feature`
- `kind:compatibility`
- `kind:performance`
- `kind:security`
- `status:needs-design`
- `status:blocked`
- `good-first-issue`

## 5. Documentation required before coding

- terminology glossary;
- supported feature matrix;
- normalized model schema;
- transaction semantics;
- unit and coordinate system;
- font resolution policy;
- compatibility policy;
- parser limits;
- error code registry;
- public API policy.

## 6. Coding standards

- deny warnings in CI for core crates;
- `rustfmt` and `clippy`;
- explicit feature flags;
- no panics across public boundaries;
- no document content in normal logs;
- unsafe code isolated and documented;
- deterministic tests;
- avoid platform-specific behavior in core;
- benchmark changes to hot paths.

## 7. Dependency policy

- Apache-2.0/MIT/BSD-compatible dependencies preferred;
- dependency license scan in CI;
- avoid copyleft dependencies in core distribution unless intentionally isolated and legally reviewed;
- pin or audit parsers, font, image, and GPU dependencies carefully;
- generate SBOM for releases.

## 8. Versioned artifacts

Each release publishes:

- crates;
- npm package;
- native binaries/libraries;
- C header;
- TypeScript definitions;
- schema files;
- conformance report;
- SBOM;
- checksums/signatures;
- changelog;
- migration notes.

## 9. Initial issue epics

1. Runtime foundations.
2. OOXML package and parser.
3. Normalized model.
4. Typography.
5. Paragraph layout.
6. Pagination.
7. Scene and rendering.
8. Transactions and history.
9. Selection and input.
10. Tables.
11. DOCX export.
12. WASM SDK.
13. Tauri reference host.
14. Accessibility.
15. Collaboration adapter.
16. Public beta hardening.
