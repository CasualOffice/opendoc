# OpenDoc developer site and pre-release WASM editor

> [!IMPORTANT]
> The editor is a **pre-release developer surface**, not a stable SDK or
> supported product. It exists to exercise real `.docx` import, rendering,
> interaction, editing, and write-back in a browser. Public SDK packaging and
> stability are tracked separately (docs 56 & 57, Phase 1G).

## Routes

- `/` is the developer landing page.
- `/editor.html` opens the editor for a local DOCX.
- `/editor.html?demo=1` opens the repository-owned sample automatically.

The editor loads `casual-doc-wasm` (import → paginate → render, compiled to
WebAssembly) and keeps document bytes in the browser. Nothing is uploaded to a
document server. External font retrieval remains host-owned and is documented
below. This is the browser-first surface where the editor and DOCX fidelity are
developed and fine-tuned (doc 56: fat client, Rust→WASM, canvas paint).

Milestones landed here:

- **P1G-001** (WASM bridge + first pixel): `open`, `pageCount`, `pageSize`,
  `renderPage`.
- **Web font provisioning**: external Roboto/Noto families plus coverage-driven
  CJK / complex-script fallbacks (see below).
- **P1G-003** (copy in view): drag to select, ⌘C to copy. The caret and
  selection highlight are **drawn from the engine's own geometry** (`hitTest`,
  `caretRect`, `selectionRects`), so they match the painted glyphs exactly. This
  is the first read-only slice of the scalable interaction pipeline
  (`docs/58-INTERACTION-SELECTION-EDITING-ARCHITECTURE.md`) that editing —
  text, then tables/images/floats/headers — extends.
- **Link interaction and authoring**: imported external hyperlinks activate
  through a host-owned `http`/`https`/`mailto` allowlist; internal hyperlinks
  and TOC rows resolve their bookmark and scroll to the target caret/page.
  Insert → Link creates, updates, or removes an undoable same-paragraph link.
- **Editing surface**: text entry and formatting, paragraph styles, lists and
  checklists, table editing (insert, split/merge, properties, formulas), picture
  insert from file or clipboard, structured paste of external tables and lists,
  find/replace, bookmarks, fields, comments, and tracked changes with
  Editing / Suggesting / Read-only modes. Insert → Symbol and Insert → Emoji add
  categorized glyph pickers; ⌘/Ctrl+P prints. Every mutation routes through the
  same gated, undoable path, so read-only fails closed and Suggesting is tracked.
- **Status bar**: live word, character, paragraph, and page counts. The strip
  sheds its lowest-value indicators as the window narrows, in
  `data-status-priority` order, rather than giving the page a horizontal
  scrollbar — see the ladder in `src/style.css`. `shell-reference-polish.spec.mjs`
  measures the footer and its live-control half against their own boxes, because
  `documentElement.scrollWidth` alone cannot see a strip that overflows behind an
  ancestor's `overflow`.

Native find/AT overlay remains a later milestone.

## Run it locally

```bash
cd webapp
./build.sh       # builds ./pkg and stages the demo + site image
./serve.py       # no-cache server at http://localhost:8099/
```

`./pkg/`, `./sample.docx`, `./demo.docx`, and `./assets/editor.jpg` are
generated (git-ignored); `build.sh` and the Pages workflow rebuild them.
`sample.docx` is the public/default document, while `demo.docx` is retained
only for the internal `?fixture=rich` browser-test route. `./assets/fonts/` is
checked in because it is required for deterministic, offline-capable editor
chrome.

## Deployment

`.github/workflows/pages.yml` builds the WASM and publishes this directory to
**GitHub Pages** on pushes to `main` that touch `webapp/` or the engine crates.
The custom domain is `opendoc.casualoffice.org`. The deployed editor remains
pre-release; the landing page labels current support and future goals separately.

## Chrome fonts and icons

The landing page and editor chrome self-host **Inter** and **Material Symbols
Outlined** from `assets/fonts/`; both routes load them through `src/fonts.css`.
No Google Fonts request is made at runtime. Inter is licensed under the SIL
Open Font License 1.1 and Material Symbols under Apache License 2.0; their
license texts are stored beside the binaries.

These UI assets are separate from document-content font provisioning below:
changing the chrome font never changes DOCX layout, pagination, or exported
font names.

## Document fonts (external families and script coverage)

The web build omits Roboto's four static blobs from the WASM artifact. Before
first paint, the host fetches commit-pinned variable upright/italic faces for
**Roboto**, **Noto Sans**, and **Noto Serif** from jsDelivr, registers all
successful responses in one bounded batch, and repaginates once. The URLs and
family metadata live in `src/web_fonts.mjs`, so a self-hosted deployment can
replace the CDN policy without changing the Rust SDK.

After named-family registration, the viewer inspects `missingCoverage()`, works
out which additional scripts are needed, and fetches only the matching Noto
fallbacks — Japanese/Korean/Simplified Chinese (CJK OTFs, ~16 MB each), plus
Arabic, Devanagari, Hebrew, and Thai. Assets use immutable commit URLs and are
cached across documents. If fetching fails, opening continues with the
metric-compatible faces retained in WASM and the status surface reports the
coverage limitation.

Not yet covered: **color emoji** (a separate font case) and high-quality
**Japanese line breaking** (parley currently has no `ja` segmentation
dictionary).

## Notes / limitations

- The `.wasm` is large (bundled fonts + shaping/ICU data). Acceptable for a test
  harness; size trimming is deferred.
- All pages render eagerly. Virtualized scroll, zoom-to-fit, and a page-bitmap
  cache are P1G-002.
- Worker threading (`SharedArrayBuffer`) needs COOP/COEP headers GitHub Pages
  cannot set, so rendering runs on the main thread here (P1G-005 territory).

## Attribution

Editor icons use **Material Symbols Outlined** by Google (Apache License 2.0).
Editor and site chrome use **Inter** (SIL Open Font License 1.1). Both are
self-hosted; complete license texts are in `assets/fonts/`.
