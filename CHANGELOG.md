# Changelog

All notable user-visible, integrator-visible, compatibility, security, and
migration changes are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
OpenDoc will use semantic versioning when its public package line begins.

## Unreleased

### Changed

- Licensed the entire project under Apache License 2.0.

### Added

- Pinned-source architecture research for LibreOffice, ONLYOFFICE, Open XML
  SDK, and Apache POI, plus a proposed OOXML fidelity architecture covering
  source snapshots, provenance, typed preservation, mapping rules, and future
  save planning.
- Production project foundation, design process, tracker, security policy, and
  CI contract.
- Initial normalized model, atomic text-insertion transaction, position mapping,
  SDK snapshot facade, and stable error codes.
- Grapheme-range deletion, paragraph split/join, semantic inverses, complete
  operation mapping, and revisioned undo/redo.
- Strict bounded normalized schema v0 JSON loading, deterministic export,
  semantic resource limits, redacted failures, and imported-ID collision
  avoidance.
- Canonical directed session selection with revision validation, grapheme-safe
  endpoints, and atomic mapping through edits, undo, and redo.
- Bounded future-only runtime event subscriptions with stable sequencing,
  transaction/selection causes, independent cursors, and explicit lag gaps.
- Security-bounded DOCX ZIP admission, deterministic part metadata, cancellable
  on-demand reads, and repository-owned package fixtures.
- Reproducible package/model benchmark runner with typed reports,
  named-environment comparison, an initial Apple M4 baseline, and CI smoke.
- Mixed-Unicode and unknown-safe-part DOCX fixtures with byte-exact package
  coverage.
- Independently locked DOCX package fuzz target, required pull-request build,
  and bounded scheduled sanitizer campaign.

### Security

- Dependency license/source/advisory checks.
- Bounded parser and resource-limit specification.
- ZIP entry, expansion, path, overlap, encryption, macro, and compression
  enforcement before DOCX package admission.
- Nightly libFuzzer coverage for arbitrary package admission and verified part
  reads without adding fuzz dependencies to the production workspace.
