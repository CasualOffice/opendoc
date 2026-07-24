# OpenDoc

[![Status: Pre-release](https://img.shields.io/badge/status-pre--release-orange.svg)](docs/06-ROADMAP-AND-DELIVERY.md)
[![Rust: 1.85+](https://img.shields.io/badge/rust-1.85%2B-black.svg?logo=rust)](rust-toolchain.toml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

OpenDoc is an open-source, deterministic document runtime written in Rust. It
is being built for native, WebAssembly, and headless applications that need a
shared document model, transactional editing, layout, rendering, and
loss-aware document interchange.

The project is developed by [CasualOffice](https://github.com/CasualOffice) as
the future document engine for Casual Docs and as an embeddable SDK for other
applications.

> [!IMPORTANT]
> OpenDoc is in pre-release development. Phase 0 is complete and Phase 1A
> (semantic DOCX import) is in progress: a `.docx` reads end-to-end into the
> normalized model, validated against LibreOffice at 100% text-content fidelity.
> The crates are not published and the public API is not stable. Layout,
> rendering, DOCX save, and an end-user editor are not available yet.

## Design Goals

- **Deterministic behavior:** identical inputs and configuration should produce
  identical model, layout, and serialization results.
- **Transactional editing:** every mutation is validated, revisioned, mapped,
  and applied atomically.
- **Portable core:** the same runtime architecture targets Rust hosts,
  `wasm32-unknown-unknown`, desktop applications, and headless services.
- **Secure document handling:** untrusted packages are processed with explicit
  entry, path, size, expansion, and resource limits.
- **Loss-aware interoperability:** unsupported document content must be
  preserved, rejected, or reported explicitly, never silently discarded.
- **Host independence:** the runtime does not require a browser DOM, a UI
  framework, a server, or a collaboration provider.

## Current Capabilities

Phase 0 (runtime + package safety) is complete; Phase 1A (semantic import) is
being built on top of it:

| Area | Available today |
| --- | --- |
| Document model | Deterministic paragraph/text model (schema v0) and a typed schema v1 (properties, styles, numbering, sections, media refs) with strict validation and total v0→v1 migration |
| Snapshot I/O | Strict, bounded normalized JSON import/export for v0 and v1 |
| Transactions | Grapheme-aware insert/delete, paragraph split/join, position mapping, and semantic inverses |
| History / Selection / Events | Revision-checked undo/redo; directed caret/range selection; bounded ordered event subscriptions |
| DOCX package reader | Security-bounded ZIP admission; relationship-based main-document discovery (transitional + ISO Strict); content-type and relationship-graph resolution; deterministic source snapshot |
| Semantic DOCX import | `.docx` → v1 model: paragraphs, runs, text (tab/break), direct run properties (bold/italic/underline/strike/size/RGB), paragraph formatting (alignment/indentation/spacing), styles (with `basedOn`), numbering (`numPr`), and body section geometry — table-cell text flattened; everything unmapped dispositioned in a deterministic compatibility report (no silent loss) |
| Round-trip | Retention mode retains the source package byte-for-byte (tier-1 byte floor) so unmapped content is preserved; a LibreOffice differential harness measures text-content fidelity |
| Engineering | Reproducible benchmarks, generated + real-producer fixtures, dependency policy, package-reader fuzzing; every crate decomposed into focused modules |
| Portability | Required CI on Linux, macOS ARM64, Windows x64, WASM, and Rust 1.85 MSRV |

The following capabilities are planned but are **not implemented**:

- edit-tolerant (tier-2) provenance and DOCX **writing** / verified byte round-trip;
- tables, images, fields, headers/footers, and tracked changes as model semantics;
- text shaping, pagination, layout, display lists, and rendering;
- browser, Tauri, C ABI, and npm distribution surfaces;
- collaboration adapters and production application integration.

See the [Phase 0 exit report](docs/31-PHASE-0-EXIT-REPORT.md) for accepted
evidence and the [support matrix](docs/18-SUPPORT-MATRIX.md) for the distinction
between current and target support.

The current DOCX design keeps the normalized OpenDoc model as the future live
editing source of truth while proposing bounded source provenance and typed
preservation for fidelity. Semantic JSON is a deterministic model artifact, not
a replacement for OOXML or a standalone round-trip guarantee.

## Getting Started

OpenDoc currently builds from source. Install
[Rust](https://www.rust-lang.org/tools/install), then clone and test the
workspace:

```sh
git clone https://github.com/CasualOffice/opendoc.git
cd opendoc
cargo test --workspace --all-features --locked
```

The repository pins Rust 1.96.0 through `rust-toolchain.toml` and supports Rust
1.85.0 as its minimum Rust version. Every pull request runs the primary build,
test, lint, docs, and WASM gates on the pinned development toolchain and a
separate locked all-target check on Rust 1.85.0. The pinned toolchain also
installs Clippy, rustfmt, and the WASM target.

Run the primary local quality gates with:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --doc --workspace --all-features --locked
cargo check --workspace --all-features --locked \
  --target wasm32-unknown-unknown
cargo +1.85.0 check --workspace --all-targets --all-features --locked
RUSTDOCFLAGS="-D warnings" \
  cargo doc --workspace --all-features --no-deps --locked
```

Run the deterministic benchmark smoke suite with:

```sh
cargo run -p opendoc-benchmark --release --locked -- \
  --smoke \
  --output target/benchmarks/local-smoke.json
```

CI additionally enforces dependency licenses and sources, RustSec advisories,
fixture checksums, locked metadata, the platform matrix, and fuzz-target
compilation.

## Workspace

| Package | Responsibility |
| --- | --- |
| `casual-doc-sdk` | Host-facing engine and document-session facade |
| `casual-doc-model` | Normalized document values, IDs, invariants, and snapshot I/O |
| `casual-doc-transaction` | Atomic operations, inverses, and position mapping |
| `casual-doc-selection` | Logical caret/range validation and mapping |
| `casual-doc-ooxml` | Security-bounded OOXML package inspection |
| `casual-doc-import` | WordprocessingML semantic import into the normalized model |
| `opendoc-benchmark` | Reproducible workload and baseline reporting |
| `opendoc-fidelity` | LibreOffice differential text-fidelity harness |
| `opendoc-fuzz` | Independently locked package-reader fuzz targets |

Internal crates are deliberately unpublished while architecture and public API
contracts evolve.

## Roadmap

OpenDoc follows capability-gated delivery rather than feature claims based only
on design:

| Phase | Outcome | Status |
| --- | --- | --- |
| 0 | Runtime, model, package-safety, CI, corpus, and benchmark foundation | Complete |
| 1A | Semantic DOCX import, normalized snapshots, and compatibility reports | In progress |
| 1B | Typography and paragraph layout | Not started |
| 1C | Pagination and backend-neutral display list | Not started |
| 1D | Native/WASM rendering and hit testing | Not started |
| 2 | Core editing SDK and DOCX save/reopen workflow | Planned |
| 3 | Advanced office-document features | Planned |
| 4 | Stable SDK surfaces and third-party embedding | Planned |
| 5 | Collaboration adapters and product migration | Planned |
| 6 | Stable 1.0 release | Planned |

Detailed deliverables and exit gates are maintained in the
[roadmap](docs/06-ROADMAP-AND-DELIVERY.md). Work does not begin until its design
is accepted and its tracker entry defines the verification gates.

### Immediate Milestone

The next milestone is deliberately limited to this end-to-end path:

```text
.docx
  -> secure package reader
  -> relationships and main document part
  -> paragraphs, runs, styles, themes, numbering, sections, and media references
  -> normalized OpenDoc model
  -> deterministic semantic JSON snapshot
  -> complete compatibility report
```

This milestone does not include typography, pagination, rendering, hit testing,
UI, or Tauri integration. See the
[proposed Phase 1A design](docs/32-PHASE-1A-SEMANTIC-DOCX-IMPORT-DESIGN.md), the
[DOCX engine research](docs/33-DOCX-ENGINE-COMPETITOR-RESEARCH.md), and the
[proposed OOXML fidelity architecture](docs/34-OOXML-FIDELITY-ARCHITECTURE.md).

## Documentation

- [Architecture blueprint](docs/00-README.md)
- [Outcome requirements](docs/01-ORD.md)
- [Architecture](docs/02-ARCHITECTURE.md)
- [SDK API specification](docs/05-SDK-API-SPEC.md)
- [Roadmap and delivery](docs/06-ROADMAP-AND-DELIVERY.md)
- [Quality, security, and compatibility](docs/07-QUALITY-SECURITY-AND-COMPATIBILITY.md)
- [Architecture decision register](docs/08-ADR-REGISTER.md)
- [Design-first delivery process](docs/11-DESIGN-FIRST-PROCESS.md)
- [Execution tracker](docs/14-EXECUTION-TRACKER.md)
- [CI and release gates](docs/15-CI-AND-RELEASE-GATES.md)
- [Support matrix](docs/18-SUPPORT-MATRIX.md)
- [Phase 0 exit report](docs/31-PHASE-0-EXIT-REPORT.md)
- [Proposed Phase 1A semantic import design](docs/32-PHASE-1A-SEMANTIC-DOCX-IMPORT-DESIGN.md)
- [DOCX engine competitor research](docs/33-DOCX-ENGINE-COMPETITOR-RESEARCH.md)
- [Proposed OOXML fidelity architecture](docs/34-OOXML-FIDELITY-ARCHITECTURE.md)
- [Import disposition taxonomy](docs/35-DISPOSITION-TAXONOMY.md)
- [ADR-027 acceptance record](docs/36-ADR-027-ACCEPTANCE-RECORD.md)
- [Phase 1A decision research (Word/ONLYOFFICE/LibreOffice)](docs/37-PHASE-1A-DECISION-RESEARCH.md)
- [Normalized schema v1 design](docs/38-NORMALIZED-SCHEMA-V1-DESIGN.md)

The numbered documents in `docs/` are the source of truth for accepted
architecture, behavior, delivery status, and compatibility claims.

## Contributing

Contributions are welcome through issues and pull requests. OpenDoc uses a
design-first workflow for substantial behavior and architecture changes:

1. Define the required outcome and constraints.
2. Record relevant specifications, compatibility evidence, and alternatives.
3. Discuss and accept the design.
4. Create or update the execution tracker item.
5. Implement with tests, documentation, and CI coverage.

Read [CONTRIBUTING.md](CONTRIBUTING.md) before starting work. Governance and
decision ownership are documented in [GOVERNANCE.md](GOVERNANCE.md).

## Security

Do not report vulnerabilities, malicious fixtures, or confidential documents
in public issues. Follow [SECURITY.md](SECURITY.md) and use
[GitHub private vulnerability reporting](https://github.com/CasualOffice/opendoc/security/advisories/new).

## License

OpenDoc is available under the [Apache License 2.0](LICENSE).
