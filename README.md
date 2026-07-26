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
> OpenDoc is in pre-release development. **Phase 0, Phase 1A (semantic modeling —
> every WordprocessingML construct family is a first-class, editable model
> value), and Phase 1B (the semantic writer, model → editable `.docx`) are
> complete.** A `.docx` reads end-to-end into the normalized model and writes
> back to a LibreOffice-valid `.docx` that reopens to an identical model
> (semantic fixed point over the real-producer corpus). The layout, pagination,
> and rendering path is now under active construction: typography and block
> layout (Phase 1C) and pagination (Phase 1D — including line-level splitting and
> incremental re-pagination) are in progress, and CPU rendering (Phase 1E) has an
> initial end-to-end implementation — real DOCX pages and tables shape through
> parley and rasterize to PNG via tiny-skia. This is a functional rendering
> **spine**, not yet a Word/LibreOffice-grade high-fidelity renderer. The crates
> are not published and the public API is not stable; an end-user editor is not
> available yet.

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

Phase 0 (runtime + package safety), Phase 1A (semantic import — every construct
family modeled), and Phase 1B (the semantic writer) are complete; the writer
round-trips the full modeled surface (import → write → reopen = identical model)
and its output opens cleanly in LibreOffice:

| Area | Available today |
| --- | --- |
| Document model | Deterministic paragraph/text model (schema v0) and a typed schema v1 (properties, styles, numbering, sections, media refs) with strict validation and total v0→v1 migration |
| Snapshot I/O | Strict, bounded normalized JSON import/export for v0 and v1 |
| Transactions | Grapheme-aware insert/delete, paragraph split/join, position mapping, and semantic inverses |
| History / Selection / Events | Revision-checked undo/redo; directed caret/range selection; bounded ordered event subscriptions |
| DOCX package reader | Security-bounded ZIP admission; relationship-based main-document discovery (transitional + ISO Strict); content-type and relationship-graph resolution; deterministic source snapshot |
| Semantic DOCX import | `.docx` → v1 model: paragraphs, runs, text (tab/break), direct run properties (bold/italic/underline/strike/size/RGB), paragraph formatting (alignment/indentation/spacing), styles (with `basedOn`), numbering (`numPr`), body section geometry, media references, inline drawings (embedded pictures → media-referencing drawing nodes), hyperlinks (external/internal, wrapping their runs), tables (grid/rows/cells with nested block content, `gridSpan`/`vMerge` merge geometry, nested tables), fields (simple `w:fldSimple` and complex `fldChar` sequences → instruction + cached result), text boxes (`w:txbxContent` in DrawingML or VML → an inline box holding block content, with `mc:AlternateContent` branch selection), footnotes/endnotes (the note parts parsed into note definitions with an inline reference from the body), and headers/footers (the header/footer parts parsed into definitions, referenced by page type from each section) — everything unmapped dispositioned in a deterministic compatibility report (no silent loss) |
| Round-trip | Retention mode retains the source package byte-for-byte and `casual-doc-export` reconstructs it, so an unedited `.docx` round-trips exactly — every tag, nested element, and part — verified by re-import producing an identical model. A LibreOffice differential harness measures text-content fidelity |
| Engineering | Reproducible benchmarks, generated + real-producer fixtures, dependency policy, package-reader fuzzing; every crate decomposed into focused modules |
| Portability | Required CI on Linux, macOS ARM64, Windows x64, WASM, and Rust 1.85 MSRV |

Every WordprocessingML construct is in scope for round-trip: what the semantic
model does not yet represent is preserved verbatim and reproduced.

**Phase 1A (semantic modeling) is complete:** every construct family is a
first-class, editable model value — paragraphs, runs (with the full property
tail), styles, numbering, sections, media, drawings, hyperlinks, tables (with
borders, shading, and margins), fields, text boxes, footnotes/endnotes,
headers/footers, comments, tracked changes, bookmarks, and content controls;
anything a producer wrote that is not yet modeled is reported and round-trips
via Retention (no silent loss).

**Phase 1B (the semantic writer) is complete:** `write_document` re-emits an
*edited* model as a valid `.docx` — body, tables, all inline constructs,
run/paragraph/section properties, styles, numbering, fontTable (incl. embedded
fonts), theme fontScheme, notes, comments, sections (including multi-section /
per-paragraph `sectPr`), headers/footers, and settings all survive the
model-fixed-point round-trip (import → write → reopen = identical model), and the
output opens cleanly in LibreOffice.

The **layout, pagination, and rendering path is now in active construction** —
this is a functional end-to-end rendering *spine*, not yet a Word-grade
high-fidelity renderer:

- **typography & block layout (Phase 1C, in progress)**: a `casual-doc-layout`
  crate shapes styled paragraphs into positioned lines via `parley` (UAX#14 line
  breaking, bidi, bold/italic face selection) and builds a block/flow galley from
  the model, including tables (columns taken from grid widths, or distributed
  evenly when absent); full run/paragraph property mapping, tab stops, and DOCX
  font-name resolution/fallback are still ahead;
- **pagination (Phase 1D, in progress)**: a single-section paginator slices the
  galley into pages with break control (page-break-before, keep-next/keep-lines,
  widow/orphan) and line-level paragraph splitting; incremental re-pagination
  reuses the unchanged page prefix and re-flows only from the edit onward
  (field-for-field identical to a full paginate); cross-page table row splitting,
  header repeat, footnotes, and multi-section pagination are still ahead;
- **CPU rendering (Phase 1E, initial end-to-end implementation)**: a
  `casual-doc-render` backend executes the backend-neutral display list on a
  `tiny-skia` pixmap, rasterizing glyph runs from real `skrifa` outlines of the
  same face the shaper used — real DOCX pages and tables render to PNG.

The following are **not started yet** (nothing is excluded — this is the
progression):

- **font management** beyond the modeled data: all OOXML font data (rFonts +
  hint, fontTable, theme fontScheme, and embedded fonts) is modeled and
  round-tripped, but runtime font resolution/substitution/metrics and fallback is
  designed and accepted (full scope), not yet implemented;
- **hit-testing, caret, and selection over rendered pages**, and WASM/Canvas and
  GPU render backends;
- the **Tauri desktop viewer/editor**;
- **stable public SDK and WASM/C-ABI/npm distribution surfaces**;
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
| `casual-doc-export` | DOCX writers: Retention (byte-identical reconstruction) and the semantic model → WordprocessingML writer (`write_document`) |
| `casual-doc-layout` | Device-independent geometry, text shaping (`parley`), block/flow galley, pagination, and the backend-neutral display list |
| `casual-doc-render` | CPU render backend: executes the display list on a `tiny-skia` pixmap, rasterizing glyphs from `skrifa` outlines |
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
| 1A | Semantic DOCX import + modeling (every construct family a first-class, editable model value), normalized snapshots, compatibility reports, and font-data modeling | **Complete** |
| 1B | Semantic writer (model → valid editable `.docx`) | **Complete** |
| 1C | Typography and paragraph/block layout | In progress |
| 1D | Pagination and backend-neutral display list (incl. line splitting + incremental re-pagination) | In progress |
| 1E | CPU rendering (native), then WASM/GPU backends and hit testing | Initial end-to-end (CPU) implementation; hit-testing and other backends not started |
| 2 | Core editing SDK and DOCX save/reopen workflow | Planned |
| 3 | Advanced office-document features | Planned |
| 4 | Stable SDK surfaces and third-party embedding | Planned |
| 5 | Collaboration adapters and product migration | Planned |
| 6 | Stable 1.0 release | Planned |

> [!NOTE]
> Product sequencing: the **Tauri desktop application** is positioned **before the
> public editing SDK and the WASM/third-party embedding surfaces**. The rendering
> engine now exists as an initial CPU spine (Phase 1E); once it reaches visual
> fidelity the desktop app is built next, and the SDK and WASM/embedding surfaces
> follow it. Tauri is the product goal, not a current deliverable — it is not
> started.

Detailed deliverables and exit gates are maintained in the
[roadmap](docs/06-ROADMAP-AND-DELIVERY.md). Work does not begin until its design
is accepted and its tracker entry defines the verification gates.

### Immediate Milestone

Import → model → semantic write-back is complete. The current milestone is the
end-to-end **rendering spine** — turning the model into pixels:

```text
.docx
  -> secure package reader -> semantic import -> normalized OpenDoc model
  -> text shaping + block/flow galley (casual-doc-layout, parley)
  -> pagination (single-section, break control, line splitting, incremental)
  -> backend-neutral display list
  -> CPU raster to PNG (casual-doc-render, tiny-skia + skrifa)
```

This is a functional spine, **not yet a Word/LibreOffice-grade high-fidelity
renderer**: full table fidelity (auto-fit, min widths, row splitting, header
repeat, border-conflict resolution), font resolution/fallback, tabs/justification
/hanging indents, multi-section pagination, footnotes, and hit-testing are still
ahead. UI and Tauri integration remain out of scope for this milestone. See the
[Phase 1C–1E layout/rendering design](docs/43-PHASE-1C-LAYOUT-RENDERING-DESIGN.md),
the [rendering architecture research](docs/42-RENDERING-ARCHITECTURE-RESEARCH.md), and
the [schema v1 design reference](docs/38-SCHEMA-V1-DESIGN-REFERENCE.md).

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
- [DOCX engine competitor research](docs/33-DOCX-ENGINE-COMPETITOR-RESEARCH.md)
- [Proposed OOXML fidelity architecture](docs/34-OOXML-FIDELITY-ARCHITECTURE.md)
- [Import disposition taxonomy](docs/35-DISPOSITION-TAXONOMY.md)
- [ADR-027 acceptance record](docs/36-ADR-027-ACCEPTANCE-RECORD.md)
- [Phase 1A decision research (Word/ONLYOFFICE/LibreOffice)](docs/37-PHASE-1A-DECISION-RESEARCH.md)
- [Schema v1 design reference (consolidated: import architecture, base schema, and every modeled construct)](docs/38-SCHEMA-V1-DESIGN-REFERENCE.md)
- [Phase 1B exit report (semantic writer)](docs/41-PHASE-1B-EXIT-REPORT.md)
- [Rendering architecture research](docs/42-RENDERING-ARCHITECTURE-RESEARCH.md)
- [Phase 1C–1E layout/pagination/rendering design](docs/43-PHASE-1C-LAYOUT-RENDERING-DESIGN.md)

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
