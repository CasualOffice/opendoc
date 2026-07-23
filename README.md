# Casual Document Runtime

Production-grade document runtime and SDK for CasualOffice.

This repository is the build target for a deterministic, embeddable document engine that can power native desktop, web, headless, and third-party document editing experiences. The source of truth for the current architecture is the `docs/` directory.

## Goal

Build a production-grade document runtime, not an MVP and not a side project.

The runtime must provide:

- a stable normalized document model;
- transaction-based editing semantics;
- deterministic layout and pagination;
- backend-neutral rendering;
- loss-aware DOCX import/export;
- collaboration adapter hooks;
- native, WebAssembly, and headless SDK surfaces;
- security, compatibility, performance, and CI gates from the beginning.

## Working Principles

- Design first.
- Discuss and finalize design before implementation.
- Create or update a tracker before execution.
- Implement in small, reviewable slices.
- Keep CI, tests, docs, and compatibility notes current.
- Treat UX review, competitive analysis, and bug hunting as part of engineering, not as optional polish.

## Documentation Map

- [Architecture Blueprint](docs/00-README.md)
- [Outcome Requirements](docs/01-ORD.md)
- [Target Architecture](docs/02-ARCHITECTURE.md)
- [High-Level Design](docs/03-HLD.md)
- [Low-Level Design](docs/04-LLD.md)
- [SDK API Specification](docs/05-SDK-API-SPEC.md)
- [Roadmap and Delivery](docs/06-ROADMAP-AND-DELIVERY.md)
- [Quality, Security, and Compatibility](docs/07-QUALITY-SECURITY-AND-COMPATIBILITY.md)
- [ADR Register](docs/08-ADR-REGISTER.md)
- [Repository and Contribution Plan](docs/09-REPOSITORY-AND-CONTRIBUTION.md)
- [Project Goal and Standards](docs/10-PROJECT-GOAL-AND-STANDARDS.md)
- [Design-First Delivery Process](docs/11-DESIGN-FIRST-PROCESS.md)
- [Competitive Analysis](docs/12-COMPETITIVE-ANALYSIS.md)
- [UX and Bug Hunting](docs/13-UX-AND-BUG-HUNTING.md)
- [Execution Tracker](docs/14-EXECUTION-TRACKER.md)
- [CI and Release Gates](docs/15-CI-AND-RELEASE-GATES.md)
- [Documentation Maintenance](docs/16-DOCUMENTATION-MAINTENANCE.md)
- [Project Glossary](docs/17-GLOSSARY.md)
- [Support Matrix](docs/18-SUPPORT-MATRIX.md)
- [Workspace Scaffold Design](docs/19-WORKSPACE-SCAFFOLD-DESIGN.md)
- [Error Code Registry](docs/20-ERROR-CODE-REGISTRY.md)
- [Parser and Resource Limits](docs/21-PARSER-LIMITS.md)
- [Normalized Schema v0](docs/22-NORMALIZED-SCHEMA-V0.md)
- [DOCX Fixture Corpus Plan](docs/23-DOCX-FIXTURE-CORPUS.md)
- [Transaction Semantics](docs/24-TRANSACTION-SEMANTICS.md)
- [Normalized Snapshot I/O](docs/25-NORMALIZED-SNAPSHOT-IO.md)
- [Selection Foundation](docs/26-SELECTION-FOUNDATION.md)
- [Runtime Event Foundation](docs/27-RUNTIME-EVENT-FOUNDATION.md)
- [DOCX Package Reader](docs/28-DOCX-PACKAGE-READER.md)
- [Benchmark and Baseline Harness](docs/29-BENCHMARK-AND-BASELINE-HARNESS.md)
- [Phase 0 Closure Design](docs/30-PHASE-0-CLOSURE-DESIGN.md)

## Repository Status

Status: Phase 0 foundation implementation.

The current Phase 0 runtime provides normalized blank documents, stable node
identity, grapheme-aware text insertion/deletion, paragraph split/join,
operation inverses, undo/redo, revision-aware snapshots, position mapping, and
stable SDK errors. Strict bounded normalized JSON v0 load/export and canonical
directed selection mapped through every edit are also available, with bounded
sequence-ordered transaction and selection events. Security-bounded DOCX ZIP
inspection and on-demand part reads are implemented; DOCX semantic import,
layout, rendering, collaboration, and persistent history are not yet claimed.
The initial package/model benchmark runner, CI smoke gate, and named-environment
baseline are also available.

## Development

The workspace uses Rust 1.96.0 with an MSRV of 1.85.0.

```sh
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo run -p opendoc-benchmark --release --locked -- --smoke \
  --output target/benchmarks/local-smoke.json
```

See [Contributing](CONTRIBUTING.md), [Security](SECURITY.md), and
[Governance](GOVERNANCE.md).

## License

MIT. See [LICENSE](LICENSE).
