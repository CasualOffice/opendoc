# OpenDoc

[![Status: Pre-release](https://img.shields.io/badge/status-pre--release-orange.svg)](docs/06-ROADMAP-AND-DELIVERY.md)
[![Rust: 1.88+](https://img.shields.io/badge/rust-1.88%2B-black.svg?logo=rust)](rust-toolchain.toml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

**OpenDoc is a deterministic, embeddable Word-document engine written in Rust.**
It reads and writes `.docx`, holds the document in a normalized editable model,
and lays it out and renders it to pixels — for native, WebAssembly, and headless
applications that need real DOCX fidelity without a browser, a server, or a UI
framework.

It is developed by [CasualOffice](https://github.com/CasualOffice) as the future
document engine for Casual Docs and as an SDK other applications can embed.

## Why OpenDoc

Most ways to work with `.docx` force a trade-off: a full office suite you can't
embed, a converter that silently drops anything it doesn't understand, or a
browser editor that treats the DOM as the source of truth. OpenDoc is built the
other way around:

- **Loss-aware by design.** Content the semantic model doesn't yet represent is
  preserved and reproduced verbatim, or reported — never silently discarded.
- **Deterministic.** The same input, fonts, and engine version produce the same
  model, layout, and bytes, every time — so rendering can be regression-tested.
- **Embeddable and host-agnostic.** No mandatory DOM, server, React, or
  collaboration provider. The core targets Rust hosts, `wasm32-unknown-unknown`,
  desktop, and headless services alike.
- **Safe with untrusted files.** Packages are parsed under explicit entry, path,
  size, expansion, and resource limits.

## Features

- **DOCX import** into a normalized, editable model: paragraphs and runs with the
  full property tail, styles (`basedOn` chains) and theme, numbering, sections
  and page geometry, tables (merged cells, nested tables, borders, shading, cell
  margins), images and drawings, hyperlinks, fields, text boxes, footnotes and
  endnotes, headers and footers, comments, tracked changes, bookmarks, and
  content controls.
- **DOCX export** in two modes: byte-identical reconstruction of an unedited
  package, and a semantic writer that re-emits an *edited* model as a valid
  `.docx` that opens cleanly in LibreOffice.
- **Editing primitives**: grapheme-aware inserts/deletes, paragraph split/join,
  atomic transactions with semantic inverses, position mapping, revision-checked
  undo/redo, directed caret/range selection, and bounded ordered events.
- **Layout and rendering**: text shaping and line breaking via
  [`parley`](https://github.com/linebender/parley), an effective-property style
  cascade, pagination with break control, a backend-neutral display list, and a
  CPU raster backend that renders real pages and tables to PNG via
  [`tiny-skia`](https://github.com/RazrFalcon/tiny-skia) and glyph outlines from
  [`skrifa`](https://github.com/googlefonts/fontations).

## Quickstart

OpenDoc builds from source. Install
[Rust](https://www.rust-lang.org/tools/install), then clone and test the
workspace:

```sh
git clone https://github.com/CasualOffice/opendoc.git
cd opendoc
cargo test --workspace --all-features --locked
```

Render the first page of a bundled sample document to a PNG — the full pipeline
(import → paginate → compose → raster):

```sh
cargo run -p casual-doc-render --example render_docx_page -- page.png
```

On a machine with system fonts you can let the shaper fall back to installed
faces (useful for CJK, symbol, and complex-script text the bundled Latin faces
do not cover):

```sh
cargo run -p casual-doc-render --example render_docx_page \
  --features system-fonts -- page.png
```

The repository pins Rust **1.96.0** through `rust-toolchain.toml` and supports
Rust **1.88.0** as its minimum supported version (MSRV). Every pull request runs
the build, test, lint, docs, and WASM gates on the pinned toolchain plus a
separate locked all-target check on the MSRV.

## Example

Import a `.docx` into the model and read back its content:

```rust
use casual_doc_import::{import_package, ImportConfig, ImportMode};
use casual_doc_ooxml::{DocxPackage, PackageLimits};

let bytes = std::fs::read("document.docx")?;
let mut package = DocxPackage::open(&bytes, PackageLimits::default())?;
let outcome = import_package(
    &mut package,
    ImportConfig { mode: ImportMode::Semantic, ..ImportConfig::default() },
)?;

let document = outcome.document;
// `document` is the normalized model: paragraphs, runs, styles, tables, …
// Anything not yet modeled is captured in the compatibility report, not lost.
```

## Status & limitations

OpenDoc is in **pre-release development**: the crates are unpublished and the
public API is not yet stable. It is a maturing engine, not a finished product —
here is an honest picture of where it stands.

**What works today**

- The full DOCX **import → model → semantic write-back** path is complete: a
  `.docx` reads into the normalized model and writes back to a LibreOffice-valid
  `.docx` that reopens to an identical model (a semantic fixed point over the
  real-producer fixture corpus). An unedited package can also be reconstructed
  byte-for-byte.
- The **layout and rendering path is structurally strong.** Recent fidelity work
  (PRs #126–#135) added an effective-property style cascade
  (`docDefaults → styles/basedOn → direct`, giving correct sizes, families,
  bold/italic, theme colors, and paragraph/line spacing), Word header/footer band
  nesting, table cell margins (`tcMar`) and vertical alignment (`vAlign`),
  block-level SDT flow (which recovers tables of contents), a floating-object
  layer with real z-order (DrawingML `wpg` groups, floating text boxes,
  header/footer floats), VML shapes parsed and painted, and page-background color.
- Measured against **LibreOffice as the layout oracle**, page counts on the
  sample corpus are exact on 3 of 5 documents and within ±1 on the other 2.

**Known limitations (not yet done)**

The renderer is structurally strong but **not yet pixel-perfect Word-grade**:
text wrapping around floats is not yet implemented, so body text can overlap a
float; CJK fallback-font line metrics run slightly tall; a couple of ±1
page-count gaps remain; footer `PAGE`/`NUMPAGES` recompute has edge cases where
cached values can show; and floating-text-box export drops its anchor on
round-trip. Footnote and endnote **body** placement, inline math (OMML) layout,
and multi-column section layout are not done yet. Hit-testing over rendered
pages, WASM/GPU render backends, the Tauri desktop viewer, and a stable public
SDK are not started.

See the [rendering fidelity gap analysis](docs/46-RENDERING-FIDELITY-GAP-ANALYSIS.md)
for the evidence-backed diagnosis and prioritized roadmap, and the
[support matrix](docs/18-SUPPORT-MATRIX.md) for current-vs-target support.

## Workspace

| Crate | Responsibility |
| --- | --- |
| `casual-doc-sdk` | Host-facing engine and document-session facade |
| `casual-doc-model` | Normalized document values, IDs, invariants, and snapshot I/O |
| `casual-doc-transaction` | Atomic operations, inverses, and position mapping |
| `casual-doc-selection` | Logical caret/range validation and mapping |
| `casual-doc-ooxml` | Security-bounded OOXML package inspection |
| `casual-doc-import` | WordprocessingML semantic import into the normalized model |
| `casual-doc-export` | DOCX writers: byte-identical reconstruction and the semantic model → WordprocessingML writer |
| `casual-doc-layout` | Geometry, text shaping (`parley`), style cascade, block/flow galley, pagination, and the backend-neutral display list |
| `casual-doc-render` | CPU render backend: executes the display list on a `tiny-skia` pixmap, rasterizing glyphs from `skrifa` outlines |

Supporting tooling lives outside `crates/`: `tools/opendoc-benchmark`
(reproducible workloads and baselines), `tools/opendoc-fidelity` (LibreOffice
differential fidelity harness), and `fuzz/` (`opendoc-fuzz`, independently locked
package-reader fuzz targets). Internal crates are deliberately unpublished while
the architecture and public API contracts evolve.

## Roadmap

OpenDoc follows capability-gated delivery rather than feature claims based only
on design.

| Phase | Outcome | Status |
| --- | --- | --- |
| 0 | Runtime, model, package-safety, CI, corpus, and benchmark foundation | Complete |
| 1A | Semantic DOCX import + modeling (every construct family a first-class model value) | Complete |
| 1B | Semantic writer (model → valid editable `.docx`) | Complete |
| 1C | Typography and paragraph/block layout | Substantially implemented |
| 1D | Pagination and backend-neutral display list | Substantially implemented |
| 1E | CPU rendering; then WASM/GPU backends and hit testing | CPU rendering implemented; other backends and hit testing not started |
| 2 | Core editing SDK and DOCX save/reopen workflow | Planned |
| 3 | Advanced office-document features | Planned |
| 4 | Stable SDK surfaces and third-party embedding | Planned |
| 5 | Collaboration adapters and product migration | Planned |
| 6 | Stable 1.0 release | Planned |

Phases 1C–1E are structurally in place and improving in fidelity; they are not
yet declared complete. Product sequencing positions the **Tauri desktop
application** before the public editing SDK and the WASM/embedding surfaces — but
it is not started, and none of the rendering work above is a Word-grade or
release claim. Detailed deliverables and exit gates live in the
[roadmap](docs/06-ROADMAP-AND-DELIVERY.md).

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
- [Schema v1 design reference](docs/38-SCHEMA-V1-DESIGN-REFERENCE.md)
- [Rendering architecture research](docs/42-RENDERING-ARCHITECTURE-RESEARCH.md)
- [Layout/pagination/rendering design (Phases 1C–1E)](docs/43-PHASE-1C-LAYOUT-RENDERING-DESIGN.md)
- [Coverage gap audit](docs/44-COVERAGE-GAP-AUDIT.md)
- [Extensibility & collaboration seams](docs/45-EXTENSIBILITY-AND-COLLABORATION-SEAMS.md)
- [Rendering fidelity gap analysis & roadmap](docs/46-RENDERING-FIDELITY-GAP-ANALYSIS.md)

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

Read [CONTRIBUTING.md](CONTRIBUTING.md) before starting work, and please follow
our [Code of Conduct](CODE_OF_CONDUCT.md). Governance and decision ownership are
documented in [GOVERNANCE.md](GOVERNANCE.md).

## Community

- **Questions and ideas:** open a
  [GitHub issue](https://github.com/CasualOffice/opendoc/issues).
- **Bugs:** file an issue with a minimal, non-confidential reproduction.

## Security

Do not report vulnerabilities, malicious fixtures, or confidential documents in
public issues. Follow [SECURITY.md](SECURITY.md) and use
[GitHub private vulnerability reporting](https://github.com/CasualOffice/opendoc/security/advisories/new).

## License

OpenDoc is available under the [Apache License 2.0](LICENSE).
