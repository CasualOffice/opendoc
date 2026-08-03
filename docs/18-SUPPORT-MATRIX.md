# Support Matrix

**Status:** Accepted for Phase 0
**Last updated:** 2026-08-04

This document distinguishes target support from implemented support. A target is
not considered supported until its required CI and conformance gates pass.

A public, per-construct rendering of the fidelity gradient (modeled → rendered →
editable → round-trips) derived from this document, the execution tracker, and
the fidelity audits lives at `webapp/fidelity.html`
(`webapp/src/fidelity.js` holds its data). Keep the two consistent when support
advances; that page is a **draft pending owner review** before it is featured.

## Platform Tiers

| Tier | Contract |
| --- | --- |
| Tier 1 | Required CI on every change, release artifacts, and blocking regressions. |
| Tier 2 | Scheduled build/test coverage; regressions are release-blocking when reproducible. |
| Experimental | Best effort, no compatibility promise, and no release artifact requirement. |

## Native Targets

| Environment | Rust target | Planned tier | Current status |
| --- | --- | --- | --- |
| macOS Apple Silicon | `aarch64-apple-darwin` | Tier 1 | Required workspace tests implemented. |
| macOS Intel | `x86_64-apple-darwin` | Tier 2 | Compile coverage planned. |
| Windows 64-bit | `x86_64-pc-windows-msvc` | Tier 1 | Required workspace tests implemented. |
| Linux 64-bit glibc | `x86_64-unknown-linux-gnu` | Tier 1 | Required build, test, lint, docs, policy, and MSRV gates implemented. |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | Tier 2 | Planned after headless CLI exists. |

The first release line uses Rust 2024 edition, pins Rust 1.96.0 for development,
and supports Rust 1.88.0 as its MSRV. Every pull request checks both compiler
boundaries. The MSRV may only be raised through an ADR and a documented release
note.

## WebAssembly

| Environment | Planned tier | Current status |
| --- | --- | --- |
| `wasm32-unknown-unknown`, core model/transactions | Tier 1 | Required compile gate implemented. |
| Browser SDK in current Chrome, Edge, Firefox, Safari | Tier 1 | Planned; no browser runtime exists yet. |
| Browser worker execution | Tier 1 | Planned with WASM facade. |
| WASM threads | Experimental | Requires host opt-in and cross-origin isolation. |
| Node.js WASM headless use | Tier 2 | Planned after WASM facade. |

The browser policy at beta will cover the latest two stable major versions
available at release time. Exact versions belong in each release conformance
report.

## Host Modes

| Host mode | v1 target | Current status |
| --- | --- | --- |
| Rust library | Yes | Initial pre-release facade implemented. |
| Headless CLI/service | Yes | Planned. |
| Tauri desktop | Yes | Planned reference host. |
| Browser/WASM | Yes | Planned reference host. |
| C ABI | Yes | Planned after the Rust facade stabilizes. |
| React/Vue/Svelte wrappers | Optional | Must live outside the core runtime. |
| Native mobile UI | No | Out of scope for v1. |

## Format Capability Status

| Format/capability | v1 target | Current status |
| --- | --- | --- |
| Normalized JSON snapshot | Yes | Strict bounded schema-v1 import and deterministic compact export are implemented in the internal format registry; stable SDK/WASM host surfaces are pending. |
| Canonical normalized CBOR | Yes | Designed, not implemented. |
| DOCX import/export | Yes | Bounded ZIP inspection implemented; semantic import complete (every construct family modeled); the semantic writer (Phase 1B) is complete and round-trips the modeled surface (import → write → reopen = identical model). |
| TXT import/export | Yes | Bounded strict UTF-8 import, deterministic semantic LF export, exact retained unchanged bytes, and compatibility-loss reporting are implemented in the internal format registry; stable SDK/WASM host surfaces are pending. |
| Page render to raster (PNG) | Yes | CPU backend implemented: real pages, tables, images, and VML render via `tiny-skia`/`skrifa`; structurally strong, not yet pixel-perfect Word-grade (see doc 46). |
| PDF render/export | Yes | Backend decision pending. |
| ODT import/export | Later | In progress under docs 94–96. Bounded ODF 1.2–1.4 ODT package/profile admission and deterministic registry detection/dispatch are implemented. The internal semantic importer maps validated core paragraphs, headings, spans, explicit spaces, tabs, line breaks, safe external/internal hyperlinks, and bookmark points/ranges. A deterministic bounded ODF 1.4 writer exports the core text surface, reports unsupported model content, and can return explicitly retained unchanged source bytes exactly. Styles, lists, tables, notes, media, metadata, edit-tolerant preservation, stable SDK/WASM host integration, corpus conformance, schema validation, and interoperability gates remain incomplete; this is not yet a general ODT support claim or v1 release gate. |
| HTML/Markdown interchange | Later | Not an editing source of truth. |
| Macros/VBA execution | No | Blocked by policy. |

## Feature Profile

| Area | v1 expectation | Current status |
| --- | --- | --- |
| Paragraphs, marks, lists | Supported | Modeled and imported (Phase 1A); shaped, laid out, and rendered via the style cascade; edit surface pending. |
| Tables and merged cells | Supported | Modeled and imported (Phase 1A); direct widths, horizontal grid spans, direct table/row alignment, visually RTL grid/margin/border geometry, direct table/row cell spacing with separate table/cell borders, cell margins, vertical alignment, common styled/segmented borders, cross-page row splitting, conforming vertical merges, table-style conditional shading/text/borders, and intrinsic sizing for modeled inline pictures/object previews/math/fields/text boxes render. Style-provided row properties/margins/spacing, exact art/compound borders, floating tables, and the edit surface remain pending (see docs 46, 49, 50, 55, 89, 90, 91, and 92). |
| Sections, headers, footers | Supported | Modeled and imported (Phase 1A); one section's default/first/even headers and footers flow through the shared body pipeline and render with nested blocks, tables, images, text boxes, floats, and page fields. Multi-section body/column geometry renders, but running-content selection, width, first-page state, and band reservation are still derived from the first section; parity starts, final-page column balancing, true next-column breaks, and the edit surface remain pending (see doc 55). |
| Images and anchors | Supported | Modeled and imported (Phase 1A); rendered via the z-ordered float layer (groups, floating text boxes, header/footer floats). Paragraph/line-relative `topAndBottom` wrapping now reserves shared flow in the body, nested table cells, headers, and footers; square/tight/through and page-coupled reflow remain pending. Edit surface pending. |
| Comments and tracked changes | Supported | Modeled, imported, and semantically round-tripped; revision wrappers currently render transparently without final/original/markup view selection, and comments have no visible page/review UI. Edit surface pending (see doc 55). |
| Fields and notes | Supported or render-only by subtype | Modeled and imported (Phase 1A); `PAGE`/`NUMPAGES` recompute in body, headers/footers, and inline/anchored text boxes. Other fields use cached results and fielded paragraphs do not soft-wrap. Footnote/endnote reference and body placement remain pending (see doc 55). |
| Math (OMML) | Preserve all; model/render common subset | Raw OMML subtrees round-trip unchanged. A bounded typed projection and deterministic atomic inline layout cover rows/text, fractions, sub/superscripts, radicals, and delimiters. Matrices, n-ary operators, accents, limits, and other advanced structures remain explicit text fallback; semantic math editing is not implemented (docs 55 and 86). |
| Shapes, text boxes, VML | Preserve or flatten with warning | Standalone anchored DrawingML shapes normalize to the shared group/float model; unknown bounded preset identities and adjustment guides survive semantic export, and rectangle/line/ellipse/round-rectangle/triangle/right-triangle/diamond primitives paint distinctly. DrawingML text boxes preserve extent/fill/outline plus independent `bodyPr` insets, anchoring, overflow, and autofit across body, cells, groups, headers, and footers. VML positioning and a bounded safe body-float subset share that model. Non-text inline shapes, exact additional preset/custom paths, gradients, rotation/vertical writing, linked boxes, side wrapping, and page-coupled reflow remain pending (docs 52, 54, 55, 87, and 88). |
| Real-time collaboration | Adapter-based | Post local transaction stability. |
| Accessibility semantics | Required | Designed, not implemented. |

## Required Release Evidence

A target becomes supported only when the release includes:

- a green required CI matrix;
- target-specific smoke tests;
- a published compatibility profile;
- parser and resource-limit conformance;
- documented known limitations;
- deterministic fixture results where layout or rendering applies.
