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
content refusal, bounded XML/text/paragraph/inline/list/table/note/report resources,
cooperative cancellation, normalized-model validation, and explicit findings
for every deferred construct family. Synthetic fixtures additionally lock table
block order, bounded repetition and nesting, empty-cell normalization, and
strict horizontal/vertical covered-cell topology. The independently locked
`odt_content` fuzz target compiles in pull-request CI. Rights-reviewed ODF
fixtures remain a Slice D completion requirement.

Synthetic note fixtures additionally require typed footnote/endnote reference
resolution, recursive note-body block placement (including notes referenced
from table cells), deterministic identity, custom-citation loss reporting, and
atomic rejection of nested, duplicate, malformed, or over-limit notes.

The matching ODT semantic-export gate requires deterministic recursive table
XML, independent row/cell/column bounds, model → ODT → model equality, byte-stable
re-export for the supported geometry, and explicit visible fallback for merge
topology that cannot be represented safely.

Note export additionally requires canonical footnote/endnote containers,
independent occurrence bounds, recursive note-body fixed points, unique IDs for
reused definitions, and explicit findings for shared, nested, or unreferenced
note shapes that do not map one-for-one to ODT's inline note ownership.

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

### Experimental PDF semantic-reconstruction gates

Doc 131 and proposed ADR-034 describe an experimental future feature, not current
support. No PDF-import dependency, model, or product claim may land until PDFR-000
accepts the architecture and assigns owners to these future gates:

- strict versioned `PdfEvidenceV1` decoding with boundary, far-over-limit,
  cancellation, and no-partial-session tests;
- evidence-replay determinism that produces identical normalized models and
  compatibility reports without invoking a parser or OCR runtime;
- rights-safe born-digital, scanned, hybrid, multilingual, table, formula, and
  degraded-image corpora, plus malformed/encrypted/active-content/oversized hostile
  PDFs;
- native-text precedence and native/OCR conflict tests proving that OCR cannot
  silently replace valid PDF text;
- semantic `PDF -> model -> DOCX -> model`, edit-save-reopen, and model-validation
  gates for the declared supported surface;
- visual comparison against source page rasters, reported separately from semantic
  and editability results;
- Worker isolation, bounded cancellation latency, peak JS/WASM/GPU memory, and
  main-thread long-task budgets;
- an offline cached import and a network-interception proof that no document byte,
  raster, extracted text, OCR token, or reconstructed content leaves the browser;
- immutable parser/runtime/model artifact hashes, dependency/licence review, and
  browser operator-coverage tests;
- exact capability discovery and honest failure on devices that lack a required
  local profile; no cloud fallback;
- CPU/WASM cross-browser coverage for every browser claimed by the baseline profile,
  with separate named-device gates for any WebGPU profile;
- fuzz targets for evidence decoding and Rust reconstruction, and hostile-PDF browser
  tests around the selected native provider.

Until those gates exist and pass, CI proves only the implemented export-only PDF
capability in doc 98.

### Experimental document-assistance, semantic-search, and MCP gates

Doc 132 and proposed ADR-035 describe an experimental future feature, not current
support. No model/runtime dependency, MCP package, hosted provider, or product claim
may land until DAI-0 accepts the architecture and assigns owners to these future
gates:

- scenario-contract tests for selection, section, table, story, and whole-document
  scopes, including empty, ambiguous, protected, and unsupported scopes;
- proposal validation proving base-revision checks, stale-result refusal, explicit
  preview/accept/reject, one-transaction commit, one-step undo, and save/reopen
  preservation for every mutable recipe;
- prompt-injection and untrusted-content tests proving document text, comments,
  fields, hyperlinks, retrieved chunks, and provider output cannot grant tools,
  broaden scope, bypass host policy, or commit mutations;
- incremental semantic-index tests for transaction invalidation, deleted or moved
  anchors, event-journal gaps, full-rebuild recovery, deterministic chunk identities,
  and no influence on serialized document bytes;
- rights-safe multilingual retrieval and summarization corpora with named relevance,
  grounding/citation, faithfulness, structure-preservation, and abstention thresholds;
- Worker-isolation, cancellation, main-thread long-task, peak JS/WASM/GPU memory,
  model-load, query-latency, and million-paragraph incremental-update budgets;
- offline and network-interception proofs for every profile advertised as local-only,
  with no silent cloud fallback and explicit handling of unavailable WebGPU/WASM
  capabilities;
- model/runtime artifact hashes, licence/provenance review, cache-integrity tests,
  cross-browser operator coverage, and deterministic provider/profile discovery;
- MCP protocol/conformance, schema-version, capability-discovery, authorization,
  audit/redaction, rate/size-limit, cancellation, and approval-boundary tests for each
  supported transport and topology;
- browser-companion tests, if that topology is accepted, proving explicit user
  pairing, origin/session binding, least privilege, expiry/revocation, and no ambient
  access to unrelated tabs or documents.

Until these gates exist and pass, CI makes no document-assistance, semantic-search,
summarization, embedded-model, or MCP support claim.

### Durable version-history, restore, and diff gates

Docs 139–140 define proposed required v1 behavior, not current support. The existing
Undo/Redo and doc 112 autosave/crash-recovery gates do not establish durable version
history. Before the support matrix may claim it, CI must include:

- version/checkpoint/commit schema compatibility, bounded decoding, unknown-version
  refusal, and deterministic metadata grouping tests;
- fidelity-complete checkpoint corpora across DOCX, ODT, RTF, TXT, and normalized JSON,
  including media, source envelopes, unknown safe parts, comments/revisions, drawings,
  tables, notes, fields, headers/footers, and metadata;
- checkpoint -> restore -> save -> reopen semantic and preservation fixed points, proving
  that normalized JSON is never used alone when it would lose resources;
- injected failure before and after every restore state-machine transition, proving the
  visible head is always either the complete old state or complete restored state;
- restore-as-new-commit, retained later history, idempotent retry, stale-head refusal,
  in-session one-step Undo, and after-reload reversibility tests;
- golden structure-aware diffs for every declared construct family, stable-ID and
  independently imported alignment, deterministic ordering, exact navigation anchors,
  and explicit `not_compared`/ambiguous/missing-resource findings;
- author/origin attribution tests and an explicit incomplete state when compaction or
  snapshot-only history prevents per-change attribution;
- capability tests for read, name, create, restore, copy, download, delete, and audit at
  the UI, host-contract, SDK, and service boundaries;
- metadata-only panel reads, pagination, cancellation, storage byte limits, quota/eviction
  disclosure, pin-preserving compaction, corruption isolation, and cross-tab collision
  tests;
- offline/network-interception proof for local history and a check that ordinary
  DOCX/ODT export contains no hidden OpenDoc timeline;
- keyboard, screen-reader, high-contrast, locale/time-zone, narrow-viewport, and touch
  journeys for list -> preview -> diff -> restore -> return;
- main-thread long-task, checkpoint, preview-open, restore, replay, diff, peak-memory, and
  storage benchmarks on small, media-heavy, 500-paragraph, 8,000-paragraph, and
  pathological documents;
- crash/reload tests during checkpoint, compaction, version deletion, and restore, plus a
  browser-store schema upgrade from doc 112 without draft loss.

Until these gates pass, current support is accurately described as session Undo/Redo,
review history, and single-slot crash recovery—not durable version history or diff.

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
| Durable version history / restore / diff | Design only | Not implemented. Docs 139–140 define future fidelity-checkpoint, append-only restore, typed diff, attribution, capability, retention, offline, crash-atomicity, accessibility, and performance gates. Doc 112 covers crash-recovery drafts only. |
| PDF semantic reconstruction | Experimental design only | Not implemented or supported. Doc 131 defines future evidence, privacy, offline, determinism, security, semantic, visual, editability, memory, cancellation, and browser-profile gates. Current PDF CI covers export only. |
| Document assistance / semantic search / MCP | Experimental design only | Not implemented or supported. Doc 132 and proposed ADR-035 define future scenario, proposal, preservation, injection-resistance, semantic-index, retrieval/summarization-quality, privacy/offline, resource, cross-browser, MCP-conformance, authorization, approval, and optional-companion gates. |
| Release artifacts | Not started | Define before beta. |

## Failure Policy

- `main` must not knowingly remain red;
- flaky tests are bugs and cannot be solved by unconditional retry;
- platform-only failures receive a reproducer or explicit blocked tracker item;
- a security advisory is evaluated before dependency update automation is
  merged;
- checks may be temporarily relaxed only through a documented, time-bounded ADR.
