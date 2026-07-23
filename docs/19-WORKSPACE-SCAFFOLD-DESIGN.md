# Workspace Scaffold Design

**Status:** Accepted for Phase 0
**Decision date:** 2026-07-24
**Tracker:** F-005, P0-001

## Outcome

Create a Rust workspace that can grow into the architecture in `03-HLD.md`
without publishing empty crates or exposing internal implementation details.

## Repository and Package Names

- repository and workspace: `opendoc`;
- public Rust facade: `casual-doc-sdk`;
- internal crates: `casual-doc-*`;
- future JavaScript package: `@casualoffice/document-runtime`.

Package-name availability must be verified before public publication. Renaming
before the first public release is allowed; renaming after release requires a
migration plan.

## Initial Workspace

```text
opendoc/
├── crates/
│   ├── casual-doc-model/
│   ├── casual-doc-transaction/
│   └── casual-doc-sdk/
├── docs/
├── .github/
├── Cargo.toml
└── rust-toolchain.toml
```

Only three crates are created in the first slice:

- `casual-doc-model`: normalized nodes, IDs, invariants, and immutable values;
- `casual-doc-transaction`: operation validation, atomic application, revisions,
  and position mapping;
- `casual-doc-sdk`: host-facing engine/session facade, snapshots, and stable
  errors.

Layout, scene, OOXML, renderer, selection, collaboration, FFI, WASM, and plugin
crates are added when their first tested behavior is implemented.

The first incremental expansion adds `casual-doc-selection` for P0-004. It owns
validated directed selections and maps them through transaction position maps;
the SDK remains the public boundary.

## Dependency Direction

```text
casual-doc-sdk
    -> casual-doc-selection
        -> casual-doc-transaction
        -> casual-doc-model
    -> casual-doc-transaction
        -> casual-doc-model
    -> casual-doc-model
```

Internal crates must not depend on the SDK facade. Model code must not depend on
layout, OOXML, rendering, platform APIs, or host UI.

## Initial Vertical Slice

P0-001 proves the architecture through one complete behavior:

1. create a blank normalized document;
2. obtain an immutable public snapshot;
3. address its initial paragraph by stable ID and grapheme offset;
4. insert text through one atomic transaction;
5. reject stale revisions and invalid positions with stable SDK codes;
6. emit a new revision and deterministic snapshot;
7. map positions across the insertion;
8. run natively and compile for `wasm32-unknown-unknown`.

No DOCX, renderer, async runtime, persistence, or host UI is included in this
slice.

## Public Boundary

The SDK defines its own public value objects and converts to internal types.
Consumers do not receive mutable model references and do not need to depend on
internal crates.

The initial API is pre-release and may change, but all exposed errors already use
the stable registry format. Panics are not part of the public contract.

## Rust Policy

- Rust edition 2024;
- MSRV 1.85.0;
- resolver version 3;
- `unsafe_code = "forbid"` in foundation crates;
- workspace Clippy lints deny suspicious and correctness issues;
- dependencies declared centrally at the workspace root;
- minimal default features;
- deterministic tests with no clock, network, locale, or system-font reliance.

## Dependency Policy

The first slice permits:

- `serde` for explicit snapshot value serialization;
- `unicode-segmentation` for grapheme-boundary semantics.

Every added dependency requires:

- a compatible license;
- a maintained upstream;
- an explanation in the implementing design or PR;
- inclusion in dependency audit and license gates.

An error-derive dependency is intentionally unnecessary for the initial slice;
error boundaries are small enough to implement directly.

## Rejected Alternatives

**Create every HLD crate immediately**
Rejected because empty crates create ownership and API boundaries before
behavior proves them.

**Single monolithic crate**
Rejected because public compatibility, transaction correctness, and the model
need distinct ownership and dependency boundaries.

**Expose model crate types from the SDK**
Rejected because it would make internal representation part of the public
compatibility contract.

**Start with the DOCX parser**
Rejected because parser output needs finalized model, error, and limit
contracts first.

## Acceptance Gates

- native format, lint, unit, and documentation tests pass;
- WASM target compiles;
- SDK mutation occurs only through a transaction-backed method;
- stale revision, invalid position, and grapheme behavior have tests;
- docs and tracker state match implemented capability.
