# Support Matrix

**Status:** Accepted for Phase 0
**Last updated:** 2026-09-15 (targeted staleness corrections; see below)

> **Staleness note — 2026-09-15 (`105-AUDIT-2026-09-TRACKER.md` §3.4).** This document
> was last revised wholesale on 2026-08-04 and had drifted behind the code in one
> consistent direction: several rows still read "edit surface pending" for surfaces that
> now edit, and the Notes, Comments and Accessibility rows described work that has since
> shipped. Those specific claims are corrected below. The **current** per-construct
> gradient lives in `webapp/src/fidelity.js`, which is under an honesty guard
> (`webapp/tests/fidelity_data.test.mjs`) and is the artifact to trust when the two
> disagree. Treat any remaining "pending" in this document as needing a code check
> before it is quoted.

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
| Browser SDK in current Chrome, Edge, Firefox, Safari | Tier 1 | A WASM runtime and reference webapp **exist and are exercised in CI** (455 browser tests across 114 specs), but only on headless Chromium — so this is not yet a four-browser claim. No Firefox or WebKit run exists, which also means no VoiceOver path (`105` UX-024). |
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
| Browser/WASM | Yes | WASM facade and reference webapp implemented; generic multi-format Open and target-selectable Save are available with visible compatibility findings. |
| C ABI | Yes | Planned after the Rust facade stabilizes. |
| React/Vue/Svelte wrappers | Optional | Must live outside the core runtime. |
| Native mobile UI | No | Out of scope for v1. A *native* app shell is not planned; the browser build is the mobile story. |
| Mobile/tablet browser | Yes | **Supported target (owner decision, 2026-09-02).** The WASM webapp is expected to open and edit documents on phone and tablet browsers. Partly met as of 2026-09-15: a `(pointer: coarse)` block now raises menu rows, panel closes, review controls and inputs to 44px/16px (HF-060 closed), page rasters are virtualised and DPR-capped, and 54 pointer-event handlers already receive touch — so the engine side is sound. **Not met, and this is the gap that makes the claim false today:** there is no editable focus owner, so tapping the page raises **no soft keyboard** and the editor cannot accept a character on a phone or tablet (`105` UX-001); there is no touch-specific handling at all (no `touchstart`/`touchmove`/`pointerType`/`maxTouchPoints`, and the only zoom gesture keys off `ctrlKey`+wheel, which a touchscreen pinch does not produce) (`105` UX-018); coarse sizing does not reach the inline ribbon buttons, rails, footer zoom or drag handles; there is no breakpoint below 620px, so at 390px the whole Home tab collapses into one dropdown (`105` UX-019); and Playwright runs desktop Chromium only. Tracked as HF-083, HF-088, HF-097 in `104-HOTFIX-TRACKER.md` and UX-001/UX-018/UX-019 in `105`. **This row is the one place the matrix still overstates support; see `105` §6 question 4 — either the slice is funded or the claim is downgraded.** |

## Format Capability Status

| Format/capability | v1 target | Current status |
| --- | --- | --- |
| Normalized JSON snapshot | Yes | Strict bounded schema-v1 import and deterministic compact export are available through the registry, generic WASM host API, and browser Open/Save controls; the native SDK surface remains pending. |
| Canonical normalized CBOR | Yes | Designed, not implemented. |
| DOCX import/export | Yes | Bounded ZIP inspection implemented; semantic import complete (every construct family modeled); the semantic writer (Phase 1B) is complete and round-trips the modeled surface (import → write → reopen = identical model). |
| TXT import/export | Yes | Bounded strict UTF-8 import, deterministic semantic LF export, exact retained unchanged bytes, and compatibility-loss reporting are available through the registry, generic WASM host API, and browser Open/Save controls; the native SDK surface remains pending. |
| Page render to raster (PNG) | Yes | CPU backend implemented: real pages, tables, images, and VML render via `tiny-skia`/`skrifa`; structurally strong, not yet pixel-perfect Word-grade (see doc 46). |
| PDF export | Partial | **Phase 0 shipped** (`crates/casual-doc-pdf`, ADR-031, doc 98): the shared `DisplayList` is transcribed into a deterministic vector PDF — real selectable/searchable text, embedded TrueType **subset** fonts at the shaper's own glyph ids, vector rules/borders/shapes/gradients, image XObjects embedded once, JPEG passed through unencoded. Registered in `casual-doc-io` as the export-only `application.pdf` adapter (opt-in via `register_pdf_exporter`); **not yet wired into the browser host**, which still prints a raster. Not covered: tagged PDF, hyperlink/outline/destination annotations, PDF/A, encryption, page ranges, CFF subsetting (such faces embed whole), SVG pictures. Doc 98 §"Implementation status" carries the family-by-family table. |
| ODT import/export | Later | In progress under docs 94–96. Bounded ODF 1.2–1.4 admission, deterministic registry detection/dispatch, generic WASM open/export, and browser Open/target-selectable Save are implemented. The semantic importer maps validated core paragraphs, headings, spans, explicit spaces, tabs, line breaks, safe external/internal hyperlinks, bookmark points/ranges, automatic/named style chains for paragraph alignment and a bounded direct run-formatting subset, bullet/decimal/letter/Roman list styles and nested list levels, ordered recursive tables with header/repeated rows, repeated/empty cells, nested blocks, validated horizontal/vertical merge geometry, typed footnote/endnote references with recursive note bodies, and bounded `meta.xml` document properties (core fields, statistics, editing duration, and typed custom values). The deterministic ODF 1.4 writer emits the matching core/style/list/table/note/metadata subset, reports unsupported formatting, unsafe merge geometry, or non-one-to-one note ownership in the UI, and can return retained unchanged source bytes exactly. Style defaults/broader properties, advanced list continuation/item overrides and label layout, table formatting, authored note-citation labels, media, edit-tolerant preservation, native SDK integration, corpus conformance, schema validation, and interoperability gates remain incomplete; this is not yet a general ODT support claim or v1 release gate. |
| HTML/Markdown interchange | Later | Not an editing source of truth. |
| Macros/VBA execution | No | Blocked by policy. |

## Feature Profile

| Area | v1 expectation | Current status |
| --- | --- | --- |
| Paragraphs, marks, lists | Supported | Modeled and imported (Phase 1A); shaped, laid out, and rendered via the style cascade. **Editing ships**: typing, IME preedit, selection, navigation, undo/redo, alignment, indentation, spacing (including `atLeast`/`exact` line rules), `keepNext`/`keepLines`, widow/orphan control, and bullet/numbered/checklist lists. Remainders are bounded: document-grid character snapping is not applied, before/after autospacing is a font-size approximation, and multilevel-list gallery authoring is not there. |
| Tables and merged cells | Supported | Modeled and imported (Phase 1A); direct widths, horizontal grid spans, direct table/row alignment, visually RTL grid/margin/border geometry, direct table/row cell spacing with separate table/cell borders, cell margins, vertical alignment, common styled/segmented borders, cross-page row splitting, conforming vertical merges, table-style conditional shading/text/borders, and intrinsic sizing for modeled inline pictures/object previews/math/fields/text boxes render. Style-provided row properties/margins/spacing, exact art/compound borders, and floating tables remain pending (see docs 46, 49, 50, 55, 89, 90, 91, and 92). **Editing ships**: insert, row/column, merge/split, sort, formula, style, borders and sizing, in every surface. |
| Sections, headers, footers | Supported | Modeled and imported (Phase 1A); one section's default/first/even headers and footers flow through the shared body pipeline and render with nested blocks, tables, images, text boxes, floats, and page fields. Multi-section documents resolve running content **per section**: OOXML link-to-previous inheritance (reference absence) is per variant and transitive back through earlier sections, `w:titlePg` is evaluated against the owning section's first page, `w:evenAndOddHeaders` against the page's number, and each band is re-flowed and re-placed at that section's own content width and `w:pgMar/@w:header`/`@w:footer` — so an orientation change re-breaks the running text at the new width. A variant its flag does not switch on is never selected and reserves no band height; a variant that is switched on but undefined paints a blank band rather than falling back to the default, as ECMA-376 §17.10.5 requires. `evenPage`/`oddPage` parity starts ship, and their inserted blank page keeps the preceding section's running content and geometry. Band height is still reserved once per section from the tallest *selectable* variant instead of per page as Word does, so a first-page header tall enough to overflow the top margin costs body area on every page of its section; final-page column balancing and true next-column breaks remain pending (see doc 55). **Header/footer editing ships in full** — enter by double-click or the hover marker, author with the same pipeline as the body including comments and tracked changes, create a band where none exists, and toggle the different-first-page and different-odd-even variants. Inserting or splitting a *section* is still not authorable. |
| Images and anchors | Supported | Modeled and imported (Phase 1A); rendered via the z-ordered float layer (groups, floating text boxes, header/footer floats). Paragraph/line-relative `topAndBottom` wrapping now reserves shared flow in the body, nested table cells, headers, and footers; tight/through use the square bounding box rather than `wp:wrapPolygon` contours, and page-coupled reflow is a bounded 3-pass fixed point. **Editing ships**: insert (file or clipboard), select, move, resize, wrap, reorder, crop by dragging, alt text, delete. Replacing a picture's bytes in place, and authoring rotation/flip or picture borders/effects, are not there. |
| Comments and tracked changes | Supported | Modeled, imported, and semantically round-tripped. A review view policy **now exists** (`ReviewView::{Editing, Markup}`, with struck deletions and author colour); Word's *Original* and *Simple Markup* views do not. **Editing ships**: Suggesting mode, accept/reject individually and in bulk, a comment sidebar with anchored highlights, add/reply/resolve/reopen/edit/delete, and Open/Resolved/All filtering — in any surface. Known limits: comment ranges are single-paragraph only, and the engine paints a comment highlight only under the read-only markup view, so comments are not part of printed page output (`105` FID-R-08 context). |
| Fields and notes | Supported or render-only by subtype | Modeled and imported (Phase 1A); `PAGE`/`NUMPAGES` recompute in body, headers/footers, and inline/anchored text boxes. Other fields use cached results and fielded paragraphs do not soft-wrap; there is no field engine. **Footnote and endnote layout ships** — reference markers, page-bottom bands, the separator rule (short for a fresh note, full width for a continuation), space reservation, per-column bands, cross-page continuation and end-of-document endnote placement — and a note can be inserted and its body edited like any other surface. Note *options* do not: number format, restart, footnote↔endnote conversion and separator customization are all modeled and unconsumed (`105` FID-L-05). |
| Math (OMML) | Preserve all; model/render common subset | Raw OMML subtrees round-trip unchanged. A bounded typed projection and deterministic atomic inline layout cover rows/text, fractions, sub/superscripts, radicals, and delimiters. Matrices, n-ary operators, accents, limits, and other advanced structures remain explicit text fallback; semantic math editing is not implemented (docs 55 and 86). |
| Shapes, text boxes, VML | Preserve or flatten with warning | Standalone anchored DrawingML shapes normalize to the shared group/float model; unknown bounded preset identities and adjustment guides survive semantic export, and rectangle/line/ellipse/round-rectangle/triangle/right-triangle/diamond primitives paint distinctly. DrawingML text boxes preserve extent/fill/outline plus independent `bodyPr` insets, anchoring, overflow, and autofit across body, cells, groups, headers, and footers. VML positioning and a bounded safe body-float subset share that model. Non-text inline shapes, exact additional preset/custom paths, gradients, rotation/vertical writing, linked boxes, side wrapping, and page-coupled reflow remain pending (docs 52, 54, 55, 87, and 88). |
| Real-time collaboration | Adapter-based | Post local transaction stability. |
| Accessibility semantics | Required | **Partly implemented.** A screen-reader document mirror projects real `h1`–`h6`, `ul`/`ol` with correct nesting, and `table` with `th[scope]`; ARIA roles cover the ribbon, menus, rails and all ten modal dialogs; two live regions carry status and alerts; and an AA contrast sweep over every text node in both themes runs in CI. Not met: the mirror is rebuilt wholesale on every edit and has no caret relationship, so assistive technology can read the document but cannot locate the caret or confirm an edit (`105` UX-020); it projects the body only, so headers/footers, notes, text boxes and comments are invisible to AT; four `role="radiogroup"` containers own `aria-pressed` buttons rather than radios (`105` UX-021); below 620px the live regions are `display:none` and therefore pruned from the tree (`105` UX-017); and there is no automated rule engine (no axe) (`105` UX-024). |

## Required Release Evidence

A target becomes supported only when the release includes:

- a green required CI matrix;
- target-specific smoke tests;
- a published compatibility profile;
- parser and resource-limit conformance;
- documented known limitations;
- deterministic fixture results where layout or rendering applies.
