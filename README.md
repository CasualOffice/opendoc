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

## Repository Status

Status: design and foundation phase.

Implementation should not begin casually. Any substantial build work should start with an approved design note, an ADR when architectural, and an updated tracker entry.

## License

MIT. See [LICENSE](LICENSE).
