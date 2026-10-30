# 176 — Three SDK products on one engine: Viewer, Editor, Native

**Status:** Design, proposed 2026-10-23; Viewer SDK phase 1 built 2026-10-24 (§11.1).
**Decides:** ADR-071 (proposed). **Row:** `docs/109` RM-46. **Built:** the measured viewer cut
(V0), then the Viewer SDK's stable read-only API, `crates/opendoc-viewer` (it was
`crates/casual-doc-viewer`), and the `@casualoffice/opendoc-viewer` package
(`packages/opendoc-viewer`, not published). The Editor SDK is not changed by this document.
**Builds on:** `05` (SDK API), `83` (packaging), `126` (embeddability, three phases), ADR-030
and `45` (extensibility invariants), ADR-005 (transactions), `98` (one layout for every
output), `40` (fonts).

**Owner decision, 2026-10-23.** Split OpenDoc into three products on one engine, and market
each one against named competitors:

1. **Viewer SDK.** Read-only preview, in the browser (WASM + JS) and on a server. Competes
   with docx-preview and mammoth.js.
2. **Editor SDK.** Today's `<opendoc-editor>`. Competes with ONLYOFFICE Docs.
3. **Native/headless SDK.** Create, read, modify and convert documents in code, from Java,
   Python, .NET and C. Competes with Apache POI (XWPF), python-docx, docx4j, the Open XML SDK
   and Aspose.Words.

**Owner scope clarification, same day, binding on the Viewer and Native SDKs.** Both are
API design only: no editing UX, no chrome, no UI work. Both are batch, request in and result
out (open → read or modify → save, render or convert). Neither needs interactive re-layout
or real-time re-render. **Both must be light**: they exclude the shell, selection, caret,
hit-testing, the incremental-paint machinery and collaboration. The Editor SDK stays as it
is and appears here only for contrast.

---

## 1. The finding that shapes the design

`docs/83` §2 designed preview as a `readOnly` **mode** of the editor runtime: mutation entry
points are bypassed at run time. That saves the reader nothing, because the bytes are already
downloaded. The browser artefact is `casual-doc-wasm`, and every host gets all of it:
25,955,980 B, of which 12,044,435 B is code (§4.1). That code includes the caret, selection,
hit-testing, the galley cache, the windowed body, the transaction log, diff and the
collaboration client, whether the host edits or not.

So a product here is a **dependency cut**, not a run-time mode. The cut is measured below,
and it removes 64% of the code.

## 2. Scope of each product

| | Viewer SDK | Editor SDK | Native/headless SDK |
| --- | --- | --- | --- |
| Job | Show a document exactly as laid out; thumbnails; PDF; text for search | Edit it, with chrome, review and co-editing | Create, read, modify, save and convert in code |
| Runs in | Browser (WASM), Node/Deno/Bun (the same WASM), server (native Rust) | Browser | JVM, CPython, .NET, C/C++, Rust; server and desktop |
| Interaction | None. A host blits pixels or shows a PNG | Full | None |
| Layout | Once per open (and once per host font registration) | Incremental, per keystroke | Once per render or convert call, after edits |
| Mutation | None | Transactions (ADR-005) | Closed operation set through `casual_doc_edit::apply` (ADR-030 I1) |
| Output | RGBA, PNG, PDF, plain text, the import report | The DOCX/ODT/… the editor saves | DOCX (loss-aware), ODT, HTML, Markdown, TXT, PDF, PNG |
| UI | None in scope (Q8) | `<opendoc-editor>`, chrome, white-label | None |

### 2.1 What "deterministic pixels" means here

The viewer cut opens with **only the bundled faces and the ones a host registers**
(`ParleyShaper::without_system_fonts`, `crates/casual-doc-viewer/src/lib.rs`
`ViewerOptions`). The same bytes then lay out to the same pages and pixels on a laptop, a CI
runner and a server. OS fallback (CJK, symbols from the machine) is an explicit
`ViewerOptions::with_system_fonts(true)`, and the host accepts that the pixels then depend
on the machine. This is the browser's configuration already. It was made the default here
after a measured failure (§9 R3).

## 3. Public API surface

Each product gets the **minimum stable surface** that does its job. The mapping column refers
to `docs/05`. *Existing* means a host can call it today. *Partial* means the capability exists
in the engine but not in this product's API. *Missing* means it is not built.

### 3.1 Viewer SDK

Browser and Node, `@casualoffice/opendoc-viewer` (name decided, §10). The shape below is
what `packages/opendoc-viewer/src/index.js` exports, over the wasm-bindgen façade in
`crates/opendoc-viewer/src/web.rs`:

```ts
import { openDocument, packageFontProvider } from "@casualoffice/opendoc-viewer";
const doc = await openDocument(bytes, {                  // rejects with a sentence on a bad file
  fontProvider: packageFontProvider("/fonts/"),          // the host serves pkg/fonts/ (§10, Q2)
});
for (let i = 0; i < doc.pageCount; i++) {
  const { widthPt, heightPt } = doc.pageSize(i);
  const { width, height, rgba } = doc.renderPage(i, { dpi: 96 * devicePixelRatio });
  const png = doc.renderPagePng(i, { dpi: 24 });         // thumbnail
  const text = doc.pageText(i);                          // the pages concatenate to doc.text()
}
doc.outline();                                           // [{ level, text, page }]
doc.fontNeeds();                                         // [{ requested, substitute, substituteAvailable }]
const pdf = doc.toPdf();                                 // shipped in the package build
```

Node uses the same module through `@casualoffice/opendoc-viewer/node`, which reads the
`.wasm` and the face files from disk. The server shape is the same crate used natively
(`ViewerDocument::open`/`open_with`, `page_sizes_twips`, `render_page`, `render_page_png`,
`page_text`, `plain_text`, `outline`, `font_needs`, `register_fonts`, `to_pdf`,
`document()`).

| Call | `docs/05` | Status |
| --- | --- | --- |
| `open(bytes, options)` (DOCX) | §4 `Engine::open_docx` | **Existing** in `opendoc-viewer`; options: faces to register before the first layout, OS fonts (native) |
| `open` for ODT, RTF, TXT, JSON | §4 | **Existing** behind `formats`. It costs 3.6 MB until P1 (§4.2) |
| `pageCount`, `pageSize`, `pageSizes` | §5 `scene` (subset) | **Existing** |
| `renderPage` (RGBA), `renderPagePng` | §5 `scene` | **Existing** |
| `toPdf` | §13 | **Existing** behind `pdf` |
| `text`, `pageText(i)` | (none) | **Existing**. Body only: headers, footers and notes are not included yet |
| `outline()` | (none) | **Existing**: level, text and page of each body heading, by the editor's heading rule |
| `fontNeeds()`, `missingCodePoints()`, `registerFonts`, `registerFallbackFont`, `fontProvider` | §10 `ResourceRequest::Font` | **Existing**: the host font hook (§10, Q2) |
| Page to SVG | (none) | **Not built**: the engine has no SVG backend, so it is not cheap. A vector page is the PDF |
| `importReport()`: what was degraded or dropped | (none) | **Partial**: `CompatibilityReport` exists in `casual-doc-import`/`-io`, but it is not exposed |
| `metadata()` | (none) | **Partial**: in `casual-doc-wasm`, not in the cut |
| `search(query)` → page rectangles | (none) | **Partial**: Find in the editor; geometry needs a hit-test-free rect query |
| `links()` per page | (none) | **Partial**: `linkAt` in the editor uses hit-testing; the viewer needs a per-page list instead |
| Pages of a huge document (windowed) | (none) | **Missing** in the cut: `WindowedBody` lives in `casual-doc-wasm` (R4) |
| `<opendoc-viewer>` element (scroll, zoom) | §3 | **Missing**, and out of scope under the "no UI" rule (Q8) |

Deliberately **absent**: `execute`, `selection`, `handle_key/pointer/ime`, `set_viewport`,
`subscribe`, `save_docx`. These are `docs/05` §5's interactive members, and they belong to
the Editor.

### 3.2 Editor SDK, for contrast only

Unchanged except its npm name: `@casualoffice/opendoc-editor` (built from `packages/opendoc-embed`), the `<opendoc-editor>` element, the host contract
(ADR-036), white-labelling (ADR-039), roles and presets (`docs/126`). It keeps
`casual-doc-wasm` and the whole of `docs/05` §5. Nothing in this document moves a byte of it.

### 3.3 Native/headless SDK

One Rust API, projected through one C ABI (§6), with idiomatic thin wrappers per language.
It covers four call families. All of them are batch calls.

```rust
// Open / create
let mut doc = opendoc::Document::open(&bytes, OpenOptions::default())?; // any registered format
let mut doc = opendoc::Document::create_blank()?;
let report = doc.import_report();          // the loss-aware edge: what did not map, and how

// Read: an immutable view over the normalized model, stable ids, no model structs exposed
for block in doc.body() {
    match block {
        Block::Paragraph(p) => { p.id(); p.style_name(); p.text(); for r in p.runs() { r.text(); r.bold(); } }
        Block::Table(t)     => { for row in t.rows() { for cell in row.cells() { cell.blocks(); } } }
        Block::ContentControl(c) => { c.tag(); c.blocks(); }
        Block::Other(o)     => { o.kind(); }   // preserved, reported, never silently dropped
    }
}
doc.sections(); doc.styles(); doc.comments(); doc.revisions(); doc.properties(); doc.media();
doc.headers(); doc.footers(); doc.notes();

// Modify: a batch of typed edits, lowered to casual_doc_edit::Operation and applied in order
doc.edit(|e| {
    e.insert_text(at, "Quarterly report")?;
    e.apply_paragraph_style(paragraph, "Heading 1")?;
    e.set_run_format(range, RunFormat::bold())?;
    e.insert_table(after, 3, 4)?;
    e.replace_all("ACME", "Contoso")?;
    e.accept_all_revisions()?;
    Ok(())
})?;                                       // all or nothing; returns the inverse batch

// Save, render, convert
let (docx, save_report) = doc.save(Format::Docx, SaveMode::PreserveWhenSafe)?;
let odt = doc.save(Format::Odt, SaveMode::Semantic)?;
let pdf = doc.to_pdf(PdfOptions::default())?;      // paginates the current state once
let png = doc.render_page_png(0, 150.0)?;
```

| Family | `docs/05` | Status |
| --- | --- | --- |
| `Engine::new`, `create_blank` | §4 | **Existing** in `casual-doc-sdk` (Phase 0) |
| `open` DOCX/ODT/RTF/TXT/JSON | §4 `open_docx` | **Partial**: the `casual-doc-io` registry does it; `casual-doc-sdk` opens normalized JSON only |
| `import_report` | (none) | **Partial**: `CompatibilityReport` and `PreservationLedger` exist; no SDK accessor |
| Read view (`body`, `runs`, `sections`, `styles`, `comments`, `revisions`) | §5 `snapshot` | **Partial**: `casual-doc-sdk` snapshots paragraphs only; the model holds all of it |
| Modify (`edit` batch) | §5 `execute`, §7 catalog | **Partial**: `casual_doc_edit::Operation` has 67 variants with inverses. `casual-doc-sdk` exposes 4 of them (insert, delete, split, join) |
| Save DOCX with exact/preserve/semantic modes and a report | §5 `save_docx` | **Partial**: `casual-doc-io` `ExportMode` and `CompatibilityReport` exist; no SDK accessor |
| Save ODT, HTML, Markdown, TXT | §13 | **Partial**: `casual-doc-io` writers exist |
| `to_pdf`, `render_page_png` | (none) | **Existing** in `casual-doc-viewer` (`pdf`) |
| C ABI | §1 "C ABI" | **Missing** (§6) |
| Java, Python, .NET packages | §13 | **Missing** |
| Plugins | §11 | **Missing**, and not in this product's first cut |

Deliberately **absent** from the Native SDK: selection, caret, viewport, input, events,
undo and redo stacks, collaboration and diff. `edit` returns the inverse batch, so a host
that wants undo has it without a history object.

**Why edits go through `Operation`, not a writable DOM.** A DOM of mutable model structs
would make every model field a public API. It would also bypass ADR-030 I1, the single
mutation choke point, and with it the invariants the operation set enforces: ids, section
and list integrity, review marks and protection. POI's own page warns of the cost of a
writable low-level surface: "Error checking may be missing in places, so it may be possible
to accidentally generate invalid files" ([POI document page][poi]). The read view is a
projection, and every write is a typed operation.

## 4. Measured: what the viewer needs, and what it costs

### 4.1 The cut and its size

**What the viewer links:** `casual-doc-model`, `casual-doc-package`, `casual-doc-ooxml`,
`casual-doc-import`, `casual-doc-layout` (with parley, fontique, skrifa, harfrust, ICU) and
`casual-doc-render` (tiny-skia, image, resvg).

**What it drops:** `casual-doc-edit`, `casual-doc-transaction`, `casual-doc-selection`,
`casual-doc-diff`, `casual-doc-wasm`, `casual-doc-sdk`, `casual-doc-export`, `casual-doc-odf`,
`casual-doc-rtf` and `casual-doc-io`. Of these, `pdf` adds back `casual-doc-pdf` and
`casual-doc-edit`, and `formats` adds back `casual-doc-io` with every writer.

The measurement was taken on 2026-10-23 at `cc37ca40` plus this branch, on macOS arm64, with
Rust 1.96.0 and wasm-pack 0.14.0. Every artefact was built with **exactly
`webapp/build.sh`'s settings**: `CARGO_PROFILE_RELEASE_OPT_LEVEL=z`, the root
`Cargo.toml` per-package `opt-level = 3` overrides, `wasm-opt -Oz` with the same enable
flags, `web-host-fonts`, and `CARGO_INCREMENTAL=0`. Compression is `gzip -9` and
`brotli -q 11`. Sections are read from the module headers.

| Artefact | `.wasm` raw | gzip -9 | brotli 11 | code section | data section | JS glue raw / gz |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| **Editor**, `casual-doc-wasm` (today) | 25,955,980 | 11,914,308 | 8,609,764 | 12,044,435 | 13,868,329 | 684,766 / 109,806 |
| **Viewer A**: DOCX only | 17,886,063 | 9,204,063 | 6,887,319 | 4,352,016 | 13,525,450 | 13,786 / 3,059 |
| Viewer B: A + `pdf` | 18,313,433 | 9,364,062 | 6,987,004 | 4,738,736 | 13,564,722 | 14,630 / 3,105 |
| Viewer C: B + `formats` | 21,951,615 | 10,557,246 | 7,767,166 | 8,224,710 | 13,711,408 | 14,630 / 3,105 |

Viewer A against the editor:

- 8,069,917 B smaller raw (−31.1%) and 2,710,245 B smaller gzipped (−22.7%);
- **−63.9% code**: 7,692,419 B less for the browser to compile before the first page;
- −98.0% JS glue (13,786 B against 684,766 B).

**What dominates now is fonts, not code.** The bundled faces that `web-host-fonts` keeps add
up to 9,317,548 B: Liberation × 12, Carlito × 4, Caladea × 4 and Noto Emoji. That is the sum
of the files `crates/casual-doc-layout/src/fonts.rs` includes. It is **52% of Viewer A** and
69% of its data section, and font bytes barely compress. Without them, about 4.2 MB of data
would remain (13,525,450 − 9,317,548 = 4,207,902 B). That figure is subtraction, not a
build. Taking the fonts out is the next lever, and it is an owner call (Q2), because these
faces are the metric-compatible substitutes the offline layout depends on.

### 4.2 What each feature costs

| Feature | Raw | gzip | Why |
| --- | ---: | ---: | --- |
| `pdf` | +427,370 | +159,999 | `casual-doc-pdf`, plus `casual-doc-edit` pulled in for `ParagraphIndex` alone (P2) |
| `formats` | +3,638,182 | +1,193,184 | The `casual-doc-io` registry registers readers **and writers** together, so opening ODT links the DOCX semantic writer, the ODT writer, HTML, Markdown and PDF (P1) |

### 4.3 Native artefact, memory and time

The binary is the `convert` example (`crates/casual-doc-viewer/examples/convert.rs`): open,
render every page to PNG at 96 dpi, and optionally write a PDF. It is a native release build
with the workspace profile, which is opt-level 3, thin LTO and stripped. On native, the
render crate also links OS font discovery.

| Binary | Raw | gzip -9 |
| --- | ---: | ---: |
| `convert`, DOCX only | 21,933,200 | 11,661,544 |
| `convert --features pdf` | 22,515,072 | 11,931,252 |
| `convert --features pdf,formats` | 27,489,232 | 14,146,425 |
| `opendoc-render` (the existing corpus tool, same pipeline) | 21,899,856 | 11,657,478 |

Peak resident set (`/usr/bin/time -l`) and wall time, three runs each. Every run is
reported, and the spread is the first run's cold cache:

| Document | Pages | Binary | Peak RSS | Open | Render all pages |
| --- | ---: | --- | --- | --- | --- |
| `sample.docx` (75,267 B) | 14 | DOCX only | 54.6–56.8 MB | 13–42 ms | 63–68 ms |
| `sample.docx` | 14 | `pdf`, writing the PDF | 56.0–58.0 MB | 13–42 ms | 63–69 ms |
| `fixtures/corpus/real-producer-rich.docx` (5,524 B) | 1 | DOCX only | 41.3–41.4 MB | 1–2 ms | 1 ms |
| `fixtures/corpus/real-producer-rich.docx` | 1 | `pdf` | 42.3–42.5 MB | 1–2 ms | 1 ms |

About 41 MB is the floor for a one-page file, and most of it is the font bundle mapped into
the process. Memory **inside the browser** was not measured. The native RSS is a proxy, not
a browser number.

### 4.4 Reproduce

```sh
df -h /                                   # stop under 8 GB free
export CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_OPT_LEVEL=z
# the editor, exactly as webapp/build.sh builds it
wasm-pack build crates/casual-doc-wasm --target web --out-dir /tmp/sdk/editor \
  --out-name casual_doc_wasm -- --features web-host-fonts
# the viewer variants
wasm-pack build crates/casual-doc-viewer --target web --out-dir /tmp/sdk/viewer-a \
  --out-name casual_doc_viewer -- --no-default-features --features wasm,web-host-fonts
#   viewer-b: --features wasm,web-host-fonts,pdf
#   viewer-c: --features wasm,web-host-fonts,pdf,formats
stat -f %z /tmp/sdk/*/*_bg.wasm; gzip -9 -c FILE | wc -c; brotli -q 11 -c FILE | wc -c
# native
unset CARGO_PROFILE_RELEASE_OPT_LEVEL
cargo build --release --locked -p casual-doc-viewer --example convert [--features pdf[,formats]]
/usr/bin/time -l target/release/examples/convert sample.docx /tmp/out --pdf
```

The tables above are the V0 measurement of 2026-10-23, before the owner's font decision, and
they stay as the record of that cut. What the package ships now is §4.5.

### 4.5 The Viewer SDK package as built (V1)

`packages/opendoc-viewer/build.sh` builds `@casualoffice/opendoc-viewer` with the owner's
decisions (§10): DOCX only, `pdf` on, and the `core-fonts` policy, which embeds Liberation
Sans alone and ships every other face as a file in `pkg/fonts/` that the host serves through
the font hook. The table below is **generated**: `tools/measure-size.mjs` writes `size.json`
from the build and renders this block from it, and `test/size_record.test.mjs` fails if the
two disagree. The CI job `viewer-package` rebuilds the module on every change and
`test/size_budget.test.mjs` holds it under a ceiling.

<!-- viewer-size:begin (generated by packages/opendoc-viewer/tools/measure-size.mjs; do not edit) -->

Built by `packages/opendoc-viewer/build.sh` with features `wasm,core-fonts,pdf`, Rust 1.96.0, wasm-bindgen 0.2.129; gzip level 9 and brotli quality 11 (Node zlib).

| Artefact | Raw | gzip | brotli | code section | data section |
| --- | ---: | ---: | ---: | ---: | ---: |
| `opendoc_viewer_bg.wasm` (engine + core font) | 11,172,897 | 5,493,049 | 4,198,359 | 5,013,436 | 6,148,897 |
| `opendoc_viewer.js` (glue) | 23,611 | 4,227 | 3,737 | | |
| `pkg/fonts/` (21 host-served faces, fetched only when a document needs them) | 9,763,176 | 5,144,368 | 4,237,221 | | |

<!-- viewer-size:end -->

## 5. Packaging, versioning and the feature-gating plan

### 5.1 Crates and features

| Crate | Viewer (browser) | Viewer (server) | Editor | Native |
| --- | :---: | :---: | :---: | :---: |
| `casual-doc-model`, `-package`, `-ooxml`, `-import` | ✓ | ✓ | ✓ | ✓ |
| `casual-doc-layout`, `-render` | ✓ | ✓ | ✓ | ✓ |
| `casual-doc-pdf` | `pdf` | `pdf` | ✓ | ✓ |
| `casual-doc-odf`, `-rtf`, readers in `-io` | `formats` | `formats` | ✓ | ✓ |
| `casual-doc-export` and the writers in `-io` | — | — | ✓ | ✓ |
| `casual-doc-edit` | — (after P2) | — (after P2) | ✓ | ✓ (modify) |
| `casual-doc-transaction`, `-selection`, `-diff` | — | — | ✓ | — (Q4) |
| `casual-doc-wasm` | — | — | ✓ | — |
| `casual-doc-viewer` | ✓ | ✓ | — | ✓ (render/convert) |
| `casual-doc-ffi` (new, §6) | — | — | — | ✓ |

Rules:

1. **Features add, never subtract.** `default = []` is the smallest useful thing (DOCX →
   pixels). A feature that removed capability would make `--all-features`, which CI
   lints, the smallest build, which is backwards.
2. **One engine version.** Every crate and package carries the workspace version, so a
   Viewer page and an Editor page render one document identically when both are on `x.y.z`.
3. **The editor is never a consumer of the viewer crate.** If it were, a viewer refactor
   could move editor bytes. The two share the engine crates below them, not each other.

### 5.2 Distribution

| Channel | Viewer | Editor | Native |
| --- | --- | --- | --- |
| npm | `@casualoffice/opendoc-viewer` (WASM + `.d.ts`, runs in Node too) | `@casualoffice/opendoc-editor`, from `packages/opendoc-embed` (pre-releases, §8.3) | — |
| crates.io | `opendoc-viewer` | — | `opendoc` (one façade crate; the `casual-doc-*` crates stay internal) |
| Maven Central | — | — | `org.casualoffice:opendoc`, one jar with per-platform natives |
| PyPI | — | — | `opendoc`: abi3 wheels for manylinux/musllinux x64 and arm64, macOS universal2, Windows x64 and arm64 |
| NuGet | — | — | `CasualOffice.OpenDoc`, with `runtimes/{rid}/native/` |
| Release asset | `opendoc` CLI (`convert`, `render`) | — | `libopendoc` + `opendoc.h` |

All names are proposals (Q1). Every crate is `publish = false` today, so crates.io needs a
façade crate first, which is the reason for one public `opendoc` crate rather than eighteen.

### 5.3 Versioning

SemVer, `0.y` until the owner declares an API stable, and `docs/05` §12's policy from 1.0.
There are three version numbers, and each one is independent:

- the **engine/package version**, one for every product;
- the **C ABI version** (`opendoc_abi_version()`), bumped only when a symbol's signature or
  ownership rule changes, so a Java jar can refuse a mismatched native;
- the **schema versions** (normalized JSON, `docs/05` §12).

## 6. Binding strategy: recommendation

**Recommended: one handle-based C ABI (`casual-doc-ffi`) as the single contract, with a
generated header, and thin hand-written idiomatic wrappers per language.**

| Option | For | Against |
| --- | --- | --- |
| **C ABI + generated header (cbindgen), wrappers per language** | One unsafe surface, one ownership rule (opaque handles, `opendoc_free_*`, error as code plus message). It serves C/C++, which `docs/05` §1 promises, and every runtime can call C: Java FFM, .NET `LibraryImport`, Python `ctypes`/`cffi` | The wrappers are hand-written. Mitigation: a conformance suite run through every language against the same fixtures |
| PyO3 (Python), JNI (Java), a .NET-specific binding | Most idiomatic per language | Three bindings written against Rust directly, three unsafe surfaces, three release pipelines, and still no C |
| UniFFI | Generated idiomatic bindings | Officially "Kotlin, Swift and Python" ([UniFFI][uniffi]). Java and C# are third-party generators ([UniFFI README][uniffi-readme]: `IronCoreLabs/uniffi-bindgen-java`, `NordSecurity/uniffi-bindgen-cs`). There is no C output |
| WASM everywhere (Wasmtime/Chicory hosts) | One artefact | A WASM runtime dependency in every JVM/.NET host, and slower than native. Unmeasured, so not recommended without a measurement |

**Java:** the Foreign Function & Memory API was finalized in **JDK 22**, and its JEP's goal
is to "Replace the brittle machinery of `native` methods and the Java Native Interface"
([JEP 454][jep454]). Java 8–21 users would need a JNI shim over the same C ABI. Whether to
carry that shim is Q3.

**The `unsafe` exception.** The workspace sets `unsafe_code = "forbid"`. A C ABI needs
`#[unsafe(no_mangle)]` and raw pointers, so `casual-doc-ffi` must be the one crate with a
recorded exception (a crate-level `[lints]` override), fuzzed at the boundary, and nothing
else may relax the rule (P4).

## 7. Competitive positioning

Competitor claims are quoted from each project's own public page, linked. Anything the cited
page does not state is marked *not stated*. That means unverified, not absent. OpenDoc
cells say **today** only for what this repository ships, and say *planned* otherwise.

### 7.1 Viewer SDK against docx-preview and mammoth.js

| | OpenDoc Viewer | docx-preview ([docxjs][docxjs]) | mammoth.js ([mammoth][mammoth]) |
| --- | --- | --- | --- |
| Licence | Apache-2.0 | Apache-2.0 | BSD-2-Clause |
| Goal | The laid-out page | "render/convert DOCX document into HTML document with keeping HTML semantic as much as possible" | "simple and clean HTML by using semantic information in the document, and ignoring other details" |
| Pagination | **Today**: its own pagination engine, the same one the editor uses | Breaks only at manual breaks, `lastRenderedPageBreak` and section changes. "Realtime page breaking is not implemented" | Not stated. Output is "an HTML fragment" |
| Styling fidelity | **Today**: fonts, sizes, colours, borders, drawings and charts drawn from the display list | "limited by HTML capabilities" | Ignores "font, text size, colour" by design. Table borders "currently ignored" |
| Thumbnails | **Today**: `renderPagePng` | "it can't be efficiently used for thumbnails" | Not stated |
| PDF | **Today**: real-text vector PDF (`pdf` feature) | Not stated | Not stated |
| Deterministic pixels across machines | **Today**: bundled faces by default (§2.1) | Not stated: it is browser HTML/CSS layout | Not stated |
| Server-side | **Today**: the same crate natively, or the WASM in Node | Not stated | Not stated |
| Formats | DOCX; ODT, RTF and TXT behind `formats` | DOCX | DOCX |
| Download | 9.2 MB gzip (§4.1), mostly fonts | Not measured here | Not measured here |

**Our weakness, said plainly:** both rivals are far smaller downloads. We did not measure
them, so no ratio is claimed. The answer to that is P3 and Q2, not a claim.

### 7.2 Editor SDK against ONLYOFFICE Docs

Unchanged from `docs/126` and `.claude/skills/opendoc/references/why.md` §1: Apache-2.0
against AGPL-3.0, local-first with no server against `x2t` with no WASM build, and host
customisation ungated against `_applyCustomization` gated on `_licensed`. Not restated here.

### 7.3 Native SDK against POI, python-docx, docx4j, Open XML SDK and Aspose.Words

| | OpenDoc Native | Apache POI XWPF ([POI][poi]) | python-docx ([docs][pydocx], [licence][pydocx-gh]) | docx4j ([README][docx4j]) | Open XML SDK ([README][oxsdk]) | Aspose.Words ([layout][aspose-layout], [products][aspose]) |
| --- | --- | --- | --- | --- | --- | --- |
| Licence | Apache-2.0 | Apache-2.0 | MIT | Apache-2.0 | MIT | Commercial ("Pricing", "Buy Now"); terms not read |
| Languages | Rust today. C, Java, Python and .NET *planned* (§6) | Java | Python | Java | .NET | .NET, Java, C++, "Python via .NET", "Node.js via .NET" |
| Read/write DOCX | Read **today** (normalized model). Write **today** in `casual-doc-io`. SDK surface *planned* (§3.3) | "covers many common use cases for reading and writing .docx files"; "moderately functional" | "creating and updating Microsoft Word (.docx) files" | "creating, editing, and saving OpenXML" | Typed markup and package operations | Yes |
| Page layout | **Today**: own pagination engine | Not stated for XWPF | None: "it has no rendering function" | Not stated | Not stated; "not intended to directly provide higher-level abstractions" | "implements its own page layout engine" that "mimics the way Microsoft Word's page layout engine works" |
| PDF | **Today**: real-text vector PDF from that layout | Word-to-FO converters, PDF "when using with Apache FOP" (HWPF only) | Not stated | "Export as PDF, including charts; choice of 3 strategies" | Not stated (third-party OfficeIMO) | PDF, XPS, PostScript, PCL |
| Page images | **Today**: PNG/RGBA per page | Not stated | Not stated | Not stated | Not stated | "BMP, PNG or JPEG" |
| Loss-aware round trip | **Today** in the engine: per-feature `CompatibilityReport` + `PreservationLedger`, export modes `exact_if_unchanged`/`preserve_when_safe`/`semantic`. SDK accessor *planned* | Not stated; "may be possible to accidentally generate invalid files" | Not stated | Not stated | Not stated | Not stated |
| Needs Word installed | No | Not stated | Not stated | Not stated (one module is "conversion-via-microsoft-graph") | Not stated | "does not utilize Microsoft Word" |
| ODT | **Today** (read and write, `formats`) | Not stated | Not stated | Not stated | Not stated | Not stated in the cited pages |

**The edge, stated precisely.** Among the permissive libraries above, only OpenDoc both
**lays the document out** and **reports what a round trip lost**. python-docx says outright
that it cannot lay out. The pages cited for POI, docx4j and the Open XML SDK do not claim a
layout engine. Aspose.Words has one and is commercial. Our gap is that **none of the
non-Rust bindings exist yet**. Until §8's N1 ships, the edge holds in Rust only.

## 8. Prerequisites and phased plan

### 8.1 Prerequisites

| # | What | Why | Size |
| --- | --- | --- | --- |
| P1 | Split `casual-doc-io` into reader and writer features | `formats` costs 3.6 MB raw because opening ODT links every writer (§4.2) | M |
| P2 | Move `ParagraphIndex` out of `casual-doc-edit`, into model or layout | `casual-doc-pdf` depends on the edit crate for it alone | S |
| P3 | A host-fonts mode for **all** bundled faces, not just Roboto | Fonts are 52% of the viewer. This is the font-provisioning lane's call (Q2) | M |
| P4 | `casual-doc-ffi` with a recorded `unsafe` exception, cbindgen header, fuzzed boundary | §6 | L |
| P5 | One publishable façade crate (`opendoc`); everything else stays `publish = false` | §5.2 | S |
| P6 | PDF export with OS fonts on must survive a giant system face | §9 R3: a 188,589,668 B macOS face fails `MAX_FONT_BYTES` (64 MiB) and aborts the whole PDF | M |
| P7 | Expose `CompatibilityReport` as a stable, versioned value | It is the edge (§7.3), and no SDK exposes it | S |
| P8 | A windowed (bounded-memory) layout in the cut, or a stated page/block ceiling | `WindowedBody` lives in `casual-doc-wasm` (R4) | M |

### 8.2 Phases: Viewer first

The Viewer goes first because its API is the smallest, it has no mutation contract to get
wrong, and the Native SDK's render and convert half **is** the viewer cut behind a C ABI.

| Phase | What a host gets | Exit gate (a host does it, in this repo) |
| --- | --- | --- |
| **V0**: this change | `casual-doc-viewer`, measured | The tables in §4, and tests that fail when render or PDF breaks (§11) |
| **V1**: Viewer SDK | `@casualoffice/opendoc-viewer` on npm; `importReport`, `outline`, `metadata`, `links`, `search` rects; P1, P2, P7; a CI size ceiling for the viewer module | A static page in the repo opens a DOCX and shows every page and a PDF with the package alone; `wasm_size`-style guard on the viewer artefact. **Phase 1 built (§11.1)**: the package, the example page, `outline`, per-page text, the font hook, the size ceiling. Still open: `importReport` (P7), `metadata`, `links`, `search` rects, P1, P2. Publishing is built (§8.3, pre-releases under `next`) |
| **V2**: server viewer | `opendoc` CLI and crate; deterministic-render golden across Linux and macOS | The same DOCX renders byte-identical PNGs on both CI platforms |
| **N1**: Native read + convert | C ABI (P4) and Python, Java and .NET packages for open, read view, `import_report`, render, PDF and save-as | One conformance suite of fixtures, run through all four languages, with identical outputs |
| **N2**: Native modify | `edit` batches over `Operation`; DOCX save with a report | A program in each language creates a document with a heading, a table and a tracked change, saves it, and the reopened file matches |
| **E**: Editor | Unchanged: `docs/126` and `109` HF-109 | — |

### 8.3 Releasing to npm (pre-releases)

`.github/workflows/npm-publish.yml` publishes `@casualoffice/opendoc-viewer`
(`packages/opendoc-viewer`) and `@casualoffice/opendoc-editor` (`packages/opendoc-embed`).
It runs only when dispatched by hand, and only ever publishes a **pre-release** under the
npm `next` tag; `latest` is never touched, so `npm install <name>` without a version does not
resolve to a pre-release. `webapp/tests/npm_publish_workflow.test.mjs` asserts each of
those properties.

Inputs: `package` (`viewer`, `editor` or `both`), `version` (semver **with** a pre-release
part, for example `0.1.0-alpha.1`; `0.1.0` is refused) and `dry_run` (default `true`).

What a run does, per package: build (the viewer through `packages/opendoc-viewer/build.sh`,
Rust from `rust-toolchain.toml`, wasm-pack 0.15.0), run the package's own tests, set the
version with `npm version --no-git-tag-version` in the runner's checkout (the editor also
regenerates `src/release.mjs`, which reports the installed version), `npm pack --dry-run`,
then `npm publish --tag next --access public --provenance`, with `--dry-run` when
`dry_run` is true. The version is never committed: the tree stays at `0.1.0`.

Auth: npm trusted publishing (OIDC) with npm 11.6.2, through the `github-pages`
environment, which holds the npm credentials. If that environment's secret `NPM_PASSWORD`
is set (an npm access token), only the publish steps receive it as `NODE_AUTH_TOKEN`. A
package's **first** publish needs that token, because npm lets a trusted publisher be
configured only on a package that exists; after it, add the trusted publisher on npmjs.com
(repository `CasualOffice/opendoc`, workflow `npm-publish.yml`, environment `github-pages`)
and the token can be removed. A real publish runs from `main` only.

Steps:

1. Dry run from `main`, and read the `npm pack --dry-run` file list in the log:
   `gh workflow run npm-publish.yml --ref main -f package=both -f version=0.1.0-alpha.1 -f dry_run=true`
2. Publish: the same command with `-f dry_run=false`.
3. Check: `npm view @casualoffice/opendoc-viewer dist-tags` shows the version under `next`
   and no `latest`. A version number can never be reused, even after an unpublish, so the
   next attempt takes the next pre-release number (`0.1.0-alpha.2`).

## 9. Risks

| # | Risk | Mitigation |
| --- | --- | --- |
| R1 | **Two front doors drift**: the viewer opens a file the editor refuses, or the reverse | The cut calls the same importer and the same ceilings (`package_limits` mirrors `casual-doc-wasm`'s `viewer_limits`). V1 adds a parity test over the corpus: same page count, and same pixels |
| R2 | The viewer's size win is eaten by fonts | P3 and Q2. Until then, say "9.2 MB gzip, 52% fonts" and not "small" |
| R3 | **Measured: native PDF fails with OS fonts on.** On this Mac, `ParleyShaper::new()` resolved a 188,589,668 B system face and `to_pdf` returned `font … is unusable: font file is 188589668 bytes, over the limit` | The default is deterministic (§2.1). P6 makes the PDF writer subset from collections or skip a face with a finding, not abort |
| R4 | Huge documents: the cut lays the whole body out, and the editor's windowed path is not in it | P8. Until then the viewer inherits the whole-layout memory profile, which is measured only on small files here |
| R5 | An `unsafe` C boundary in a `forbid(unsafe_code)` workspace | One crate, a recorded exception, fuzzing and handle validation (P4) |
| R6 | Four language packages are four release pipelines | One C ABI, one conformance suite, and wrappers kept thin enough to generate later |
| R7 | **Overclaiming against competitors** | §7 quotes their own pages, and marks the rest *not stated*. No competitor size or speed is claimed, because none was measured |
| R8 | The Native read view freezes model internals into a public API | The view is a projection with its own types (`docs/05`: "Internal model and transaction types are not re-exported") |

## 10. Owner decisions (2026-10-23), and what is still open

**Decided:**
- **Build order: Viewer first.** It is the smallest step from today's code, competes directly with docx-preview, and forces the stable read API the Native SDK also needs.
- **Viewer fonts (Q2): a small core inside the package, the rest supplied by the host** through the existing font-registry seam. No CDN fetch by default. Pixels stay deterministic when the host serves the same faces.
- **Viewer formats (Q5): DOCX only by default.** ODT, RTF and TXT are opt-in features, because they add about 3.6 MB today through the format registry's writers.
- **Java (Q3): both.** The FFM API (JDK 22+) first; a JNI shim for JDK 8-21 later, if customers ask.

- **Package names (Q1), decided 2026-10-23** (each checked free on its registry that day):
  - npm: `@casualoffice/opendoc-viewer` (Viewer), `@casualoffice/opendoc-editor` (Editor; today's `packages/opendoc-embed` publishes under this name).
  - crates.io: `opendoc-viewer` (Viewer) and `opendoc` (the Native/headless facade).
  - PyPI: `opendoc`. Maven: `org.casualoffice:opendoc`. NuGet: `CasualOffice.OpenDoc`.
  - Reserve every name before the first release, so nobody squats them. The npm credentials live in the repository's `github-pages` environment (owner, 2026-10-24; §8.3); the other registries' credentials are still for the owner.

**Still open:** Q1 (non-npm registry credentials only), Q4 (whether Native batch edits need the transaction log), Q6 (native platform list), Q7 (whether Node needs a native addon or WASM is enough) and Q8 (a minimal `<opendoc-viewer>` element).

### Original open questions (for reference)

- **Q1. Names and registry ownership.** Are `@casualoffice/opendoc-viewer`, crate `opendoc`,
  Maven `org.casualoffice:opendoc`, PyPI `opendoc` and NuGet `CasualOffice.OpenDoc` right,
  and who holds those registry credentials?
- **Q2. Fonts in the viewer.** Keep the 9.3 MB bundle (offline, deterministic) or fetch the
  faces from the host (smaller, needs a network or a host font set)? Or ship two builds?
- **Q3. Minimum Java.** FFM requires JDK 22+. Do we carry a JNI shim for Java 8, 11, 17
  and 21?
- **Q4. Transactions in the headless SDK.** Is `casual_doc_edit::apply` (ADR-030 I1) enough
  for batch edits, or does ADR-005 require the `casual-doc-transaction` log there too?
  That log would add the collaboration session to a binary that does not collaborate.
- **Q5. Default formats for the Viewer.** DOCX only (smallest), or every format (+3.6 MB
  raw until P1)?
- **Q6. Native platform matrix.** Linux x64/arm64 (glibc and musl), macOS arm64/x64 and
  Windows x64/arm64. Is any of these out?
- **Q7. Node.** Is the WASM package enough on servers running Node, or do we also ship a
  native Node addon?
- **Q8. A viewer element.** Under "no UI work", is a minimal `<opendoc-viewer>` (scroll, zoom,
  no chrome) part of the Viewer SDK later, or is it the host's job forever?

## 11. What this change built, and its evidence

- `crates/casual-doc-viewer`: `ViewerDocument` (`open`, `open_with`, `page_count`,
  `page_size_twips`, `render_page`, `render_page_png`, `plain_text`, `register_font`,
  `to_pdf`), the `WasmViewer` façade and the `convert` example. It is additive: no editor
  crate depends on it, and `casual-doc-wasm` is untouched.
- Tests (`cargo test -p casual-doc-viewer`, default and `--all-features`):
  - every page renders and is not blank;
  - a page past the end is refused with the count;
  - a thumbnail is a PNG;
  - the plain text carries the body;
  - garbage is refused (DOCX only), or opens as text (`formats`);
  - the default leaves OS fonts off and renders repeatably;
  - the PDF has one page object per laid-out page.
- Mutations, each seen red:
  - With the `render` call removed, `a_docx_opens_paginates_and_renders_every_page` fails
    with "page 0 rendered blank".
  - With the default shaper switched to `ParleyShaper::new()`,
    `the_pdf_has_one_page_per_laid_out_page` fails with `Encode("pdf: font 268435464 is
    unusable: font file is 188589668 bytes, over the limit")`. This is R3, and it is
    machine-dependent: the test is red only where such a face is installed.


### 11.1 Viewer SDK phase 1 (2026-10-24)

- **Crate.** `crates/opendoc-viewer` (renamed from `casual-doc-viewer`, §10 Q1) adds
  `open_with` faces, `page_sizes_twips`, `page_text`, `outline`, `font_needs`,
  `missing_code_points`, `register_fonts` (one re-layout for any number of faces) and
  `register_fallback_font`, and refuses a non-positive DPI. Still no editing crate.
- **Font policy (§10, Q2).** `casual-doc-layout`'s `viewer-core-fonts` feature, `wasm32` only
  like `external-web-fonts`, embeds Liberation Sans alone; every other family keeps its
  `FontId` block and resolver identity with empty placeholders, and the shaper uses a
  host-registered face under the family's own name. Native builds, `--all-features`
  included, keep the whole bundle, so no native test or golden moves. The shaper now records
  the requested families it could not find; that list is `font_needs`.
- **A defect the policy exposed, fixed for the class.** A field value (`PAGE`, `NUMPAGES`)
  is re-shaped at pagination, and its run kept the face id of its source run instead of the
  face that shaped it. With Carlito served by the host, the value was shaped in Carlito and
  outlined from Liberation Sans: `sample.docx`'s footer "2 of 14" painted as "Ï of ãÕ". The
  run now carries the shaping face (`casual-doc-layout` `shape_field_run`); on the bundled
  path the two ids are the same, so native output does not change.
- **Package.** `packages/opendoc-viewer`: `build.sh` (DOCX only, `pdf` on, `core-fonts`, the
  host-served faces and their licences in `pkg/fonts/`), the JS API `src/index.js` and its
  types, `src/node.js` for Node, `examples/browser/index.html` (a page that opens a DOCX and
  shows every page, the outline and the PDF with the package alone) and
  `examples/node/render.mjs`. Not published.
- **Guards.** `crates/opendoc-viewer/tests/api.rs` (render determinism per page across opens
  and repeats, page text concatenating to the body text, page text across a page break, the
  outline's levels and pages, a missing family reported and filled by a host face, the PDF's
  page count and byte determinism) and `packages/opendoc-viewer/test/` in the CI job
  `viewer-package` (the same API through the built module in Node, the core-font policy on
  the shipped artefact, the field-face defect, the size budget, and the generated size
  record). Each was seen red; the mutations are in the commit message.

## 12. The Native SDK's API: research, then decision (owner, 2026-10-30)

**Owner goal, 2026-10-30.** Make OpenDoc ready to beat docx-js, Apache POI and the paid SDKs
(Aspose.Words and others) at handling DOCX and other formats, both as a headless SDK and as
a rendering and preview engine. **Language order**, by usage and by how readily AI agents
adopt a library: JavaScript/TypeScript (Node and the browser, headless) first, then Python,
then Java; .NET later. **API rule:** research the popular libraries first, and make ours
standard and familiar, because similarity drives adoption.

### 12.1 What the incumbents' APIs look like

Read from each project's public documentation on 2026-10-30. Anything a page did not show
is marked *unverified*. The fetch tool returns pages paraphrased with short verbatim
fragments, so the snippets below are rebuilt from those fragments, not pasted whole.

| Library | Licence | Create / modify idiom | Open existing | Layout, PDF, images | Source |
| --- | --- | --- | --- | --- | --- |
| **docx** (docx-js) | MIT | Declarative tree: `new Document({ sections: [{ properties, headers: { default: new Header({children}) }, footers, children: [new Paragraph({ children: [new TextRun({ text, bold: true })] })] }] })`, then `Packer.toBuffer(doc)` / `toBlob`. `Table` → `TableRow` → `TableCell({children})`; `HeadingLevel.HEADING_1`; `ImageRun({ type, data, transformation: { width, height } })` in pixels; `PageBreak`; `ExternalHyperlink({ children, link })`; `numbering: { config: [{ reference, levels: [{ level, format, text, alignment }] }] }`; `styles: { paragraphStyles: [{ id, name, basedOn, run, paragraph }] }`; comments through `CommentRangeStart`/`End`/`Reference` | No reader. `patchDocument({ data, patches })` fills `{{placeholder}}`s; an unmatched patch is silently skipped | None stated | [README][docxjs-readme], [usage docs][docxjs-docs] |
| **docxtemplater** | MIT or GPL-3.0 core; paid modules | `new Docxtemplater(zip, { paragraphLoop: true, linebreaks: true })`, `doc.render(data)`, `doc.toBuffer()`. Tags `{name}`, loops/conditions `{#x}…{/x}` (or `{/}`), inverted `{^x}`, `{.}`; options `delimiters`, `nullGetter`, `parser` | Template only | None. Images, HTML, tables, charts, footnotes, ODT, subtemplates and more are **paid modules** (€500/yr per module up to €9000/yr) | [tag types][dt-tags], [configuration][dt-config], [errors][dt-errors], [pricing][dt-pricing] |
| **mammoth.js** | BSD-2-Clause | Read only: `convertToHtml({ path \| buffer \| arrayBuffer }, { styleMap })`, `extractRawText`; result `{ value, messages: [{ type, message }] }` | Yes, to HTML | "simple and clean HTML"; ignores font, size and colour; writes no DOCX | [README][mammoth] |
| **docx-preview** | Apache-2.0 | Render only: `renderAsync(data, bodyContainer, styleContainer, options)` (`breakPages`, `ignoreLastRenderedPageBreak`, `renderHeaders`, `renderComments`, …) | Yes, to DOM | Pages break only at explicit or last-rendered breaks; fields (so a TOC) unsupported | [README][docxjs] |
| **python-docx** | MIT | Imperative: `Document()`, `add_heading(text, level)`, `add_paragraph(text, style)`, `p.add_run(text).bold = True`, `add_table(rows, cols)`, `table.cell(r, c).text`, `add_picture(path, width=Inches(1.25))`, `add_page_break()`, `save(path)`; `document.add_comment(runs, text, author, initials)` (body only, no replies) | `Document(path \| file)`; `paragraphs`, `tables`, `sections[i].header.paragraphs`, `core_properties` | None found | [docs][pydocx-home], [quickstart][pydocx-quick], [comments][pydocx-comments] |
| **docxtpl** | LGPL-2.1 | `DocxTemplate(path)`, `render(context)`, `save()`; Jinja2 `{{ }}`, `{%p %}`, `{%tr %}`, `{%tc %}` | Template only | Plain tags must sit in one run | [docs][docxtpl] |
| **Apache POI XWPF** | Apache-2.0 (ASF; not restated on the pages read) | `new XWPFDocument()`, `createParagraph()`, `createRun()`, `XWPFRun.setText/setBold/setFontSize`, `createTable(rows, cols)`, `createHeader(HeaderFooterType.DEFAULT)`, `write(OutputStream)` | `new XWPFDocument(InputStream)`, `getParagraphs()`, `getTables()`, `XWPFWordExtractor.getText()` | "moderately functional"; may "generate invalid files"; no XWPF layout; FO/PDF only for old `.doc` | [component][poi], [XWPFDocument][poi-doc], [XWPFRun][poi-run] |
| **docx4j** | Apache-2.0 (+ commercial edition) | `WordprocessingMLPackage.createPackage()`, `getMainDocumentPart().addStyledParagraphOfText("Title", …)`, `addParagraphOfText`, `save(File)` | `Docx4J.load`; JAXB object model | `Docx4J.toPDF` ("3 strategies", XSL-FO) | [README][docx4j], [Docx4J.java][docx4j-api], [blog][docx4j-blog] |
| **Aspose.Words** | Commercial | `new Document()`, `DocumentBuilder` (`write`, `writeln`, `insertBreak(BreakType.PAGE_BREAK)`, `startTable`/`insertCell`/`endRow`/`endTable`, `moveToHeaderFooter`, `insertImage`, `insertHyperlink`, `insertField`) | `new Document(path)`, `getText()`, `getPageCount()`, `getRange().replace(a, b)`, `acceptAllRevisions()`, `compare`, `mailMerge` | Own layout engine; `save("out.pdf")` infers the format from the extension; `LayoutCollector` page indices | [Document][aspose-doc], [DocumentBuilder][aspose-builder], [Range.Replace][aspose-replace], [LayoutCollector][aspose-layout-collector] |

**What the table says.** Each permissive library does one of the jobs: generate (docx-js),
fill (docxtemplater, docxtpl), read and modify (python-docx, POI, docx4j), convert to HTML
(mammoth) or preview in a browser (docx-preview). Only Aspose lays a document out and prints
it, and it is commercial. Among the permissive ones, docx4j's PDF goes through XSL-FO, not a
layout engine.

### 12.2 Decision: one engine, familiar front doors

We **adopt each ecosystem's dominant idiom verbatim**, so a user changes the import and
little else. Underneath, every front door lowers to the same Rust core (`crates/opendoc`),
so the languages cannot drift.

| Layer | Idiom adopted | Where it is built |
| --- | --- | --- |
| **JS generation** | docx-js's declarative tree, the same class names and the same option names (`new Document({ sections })`, `Paragraph`, `TextRun({ bold, italics, size, color, font, underline })`, `HeadingLevel`, `Table`/`TableRow`/`TableCell({ columnSpan, rowSpan })`, `ImageRun({ data, transformation })`, `ExternalHyperlink`, `InternalHyperlink`, `Bookmark`, `PageBreak`, `FootnoteReferenceRun`, `PageNumber`, `TableOfContents`, `numbering.config`, `styles.paragraphStyles`, `comments.children`, `Packer.toBuffer/toBlob/toBase64String`). A docx-js user's object literal is, field for field, a valid spec | `packages/opendoc/src/builder.js` serialises to `DocumentSpec` JSON; `crates/opendoc/src/builder.rs` writes the package |
| **JS templating** | docxtemplater's tags (`{x}`, `{#x}…{/x}`, `{^x}`, `{.}`, dotted paths), options (`delimiters`, `nullGetter` as a constant, `paragraphLoop` and `linebreaks` always on) and error shape (`properties.errors[]` with `id` and `explanation`) | `crates/opendoc/src/template.rs`; `doc.render(data)` in JS |
| **JS read, modify, convert** | python-docx-like readable names in camelCase (`doc.paragraphs`, `doc.tables`, `doc.sections`, `doc.comments`), mammoth's result shape for conversion messages (`{ value, messages }`), Aspose's one-call `save(format)` with the format named | `packages/opendoc/src/index.js` |
| **Python** | python-docx's surface (`Document()`, `Document(path)`, `add_heading`, `add_paragraph(text, style)`, `add_run(text).bold = True`, `add_table(rows, cols)`, `cell(r, c).text`, `add_page_break`, `paragraphs`, `tables`, `sections`, `core_properties`, `save`), plus docxtpl's `render(context)` with docxtemplater tags | `bindings/python` (PyO3) |
| **Java** | POI XWPF's naming (`OpenDocument` ≈ `XWPFDocument`: `createParagraph()`, `createRun()`, `setText`, `setBold`, `getParagraphs()`, `getTables()`, `write(OutputStream)`) and Aspose's `save(path)` by extension | `bindings/java` (Panama FFM over `crates/opendoc-ffi`) |
| **Rust** | The `docs/176` §3.3 shape: `Document::open`, a read projection, `doc.edit(\|e\| …)` batches, `save(Format, SaveMode)` with a report | `crates/opendoc` |

**What we add that none of them has in one permissive package**, each of which is a test in
this repository:

1. **Layout, page images and real-text PDF** from the editor's own pagination
   (`render_page_png`, `to_pdf`, `page_count`, `page_text`, `outline`). This is Aspose's
   capability, under Apache-2.0.
2. **A loss report on every open and every save** (`import_report`, `SaveReport.losses`)
   and three save modes: `exact` returns the original bytes when the document is unchanged,
   `preserve` keeps verbatim-retained parts, `semantic` regenerates. No library in §12.1
   reports loss.
3. **Templating in the same package**, with images, tables, footnotes and headers kept
   because the engine edits the document rather than the XML text. docxtemplater charges for
   several of these as modules.
4. **Every edit goes through the engine's checked operation set** (ADR-030 I1): ids,
   protection (a read-only document refuses), atomic batches and an exact undo. POI warns it
   "may be possible to accidentally generate invalid files"; here a batch either applies
   whole or not at all.
5. **The same engine everywhere.** The Node package, the CLI, Python, Java and the
   in-browser editor lay out one document to the same pages. Deterministic pixels across
   platforms are a CI gate (§12.5).

**Deliberately not copied.**

- docx-js's silent skip of an unknown option. Here an unknown spec field is an error naming
  the field, except inside `TextRun` and `Paragraph`, whose options flatten into one object.
- docxtemplater's default `nullGetter`, which prints `undefined`. Here a missing value prints
  `""`, or is an `undefined_value` error with `strict`.
- python-docx's writable proxies over the XML. Here a run's `bold = True` is a typed edit.

**Offsets.** Edits address text by **character** (Unicode scalar) offsets into a paragraph's
`text`, which is a Python `str` index. JavaScript UTF-16 indices differ only outside the BMP,
and the JS package converts.

### 12.3 The Rust core (S1), as built

`crates/opendoc` is the one public façade (`docs/176` §5.2). It re-exports no `casual-doc-*`
type.

- **Open and create.** `Document::open` / `open_with(bytes, OpenOptions)` go through the
  `casual-doc-io` registry: DOCX, ODT, RTF, TXT and JSON, plus PDF with the `pdf-import`
  feature. The source is retained, so an exact save works. `Document::create()` and
  `from_spec(DocumentSpec, media)` build a WordprocessingML package and open it. It carries
  Word's built-in style ids and names (Normal, Title, Heading 1-6, List Bullet, List Number,
  Quote, Caption, Table Grid, Hyperlink, the TOC and note styles), an A4 page and Calibri
  11 pt, which the bundled Carlito lays out metric-identically.
- **Read view** (`opendoc::view`): `body()` returns `Block::{Paragraph, Table,
  ContentControl, Other}` with stable ids. Paragraphs carry their style name, text, heading
  level, alignment, list membership and runs. Runs carry text, direct formatting, hyperlink,
  tracked change, field, image and note/comment references. Also `paragraphs()`, `tables()`,
  `sections()`, `styles()`, `comments()` (with anchors), `revisions()`,
  `headers_footers()`, `notes()`, `media()` plus `media_bytes()`, `properties()` and
  `text()`. Everything serialises to the camelCase JSON every binding reads.
- **Edit** (`doc.edit(|e| …)`, or `edit_commands(&[EditCommand])` from JSON). The text,
  format, hyperlink and paragraph calls lower to `casual_doc_edit::Operation` and apply
  through `casual_doc_edit::apply` after `refuse_if_protected`. They are: `insert_text`,
  `delete_text`, `replace_text`, `replace_all`, `set_run_format`, `clear_formatting`,
  `set_hyperlink`, `set_paragraph_style`, `set_alignment`, `set_page_break_before`,
  `insert_paragraph`, `append_paragraph`, `append_heading`, `append_run`, `delete_block`.
  Tables: `insert_table`, `set_cell_text`, `add_row`, `delete_row`. Review edits lower
  through `casual_doc_review::lower`, the editor's own lowering: `add_comment`,
  `suggest_insert`, `suggest_delete`, `suggest_replace`, `accept_all_revisions`,
  `reject_all_revisions`. Properties: `set_properties`. The batch runs on a copy and
  replaces the document only if every call succeeded. It returns an exact `Undo`, which
  `doc.undo` applies and which returns the redo.
- **Template:** `render_template(data, TemplateOptions)` and `template_tags()`, the
  docxtemplater idiom of §12.2 lowered to `InsertText`, `DeleteText`, `InsertBlocks`, `DeleteBlocks`, `InsertRow`, `DeleteRow` and `SetInlines`.
- **Save:** `save(Format, SaveMode) -> (bytes, SaveReport)` to DOCX, DOTX, ODT, TXT, JSON,
  HTML, Markdown or PDF. A same-format save honours the mode. A cross-format save is semantic
  and its report says `lossyByNature` for the renditions. `opendoc::convert(bytes, to)` does
  both steps in one call.
- **Render:** `page_count`, `page_sizes`, `page_text`, `render_page` (RGBA),
  `render_page_png`, `to_pdf`, `outline` and `font_needs`. One layout serves all of them,
  cached until the next edit. It uses bundled faces by default (`RenderOptions` adds faces or
  allows OS fonts).
- **Not built (stated, not hidden):**
  - inserting pictures, headers, footers or notes by edit (the builder writes all four);
  - page or section edits;
  - paragraph loops inside headers and footers (value tags work there);
  - SVG (no backend, §3.1);
  - opening HTML or Markdown (export-only adapters).

**One defect class found and handled in the SDK.** A comment's anchor from
`casual_doc_review` is measured in the layout anchor stream, which counts a footnote
reference's printed number. The read view's `text` does not count it, so the first build
returned `"noted "` for a comment on `" noted"` after a footnote. `Document::comments` and
`revisions` now map anchors through both length functions (`ParagraphSpace`). The engine-side
disagreement is the one recorded on `casual_doc_review::body::inline_anchor_len`.

**An engine limit the builder found (open for the import lane).** A complex field whose
begin and end sit in **one** paragraph, with a hyperlink in its cached result, fails the
whole DOCX open. Model validation refuses a hyperlink inside a `Field`'s inlines (a field's
result is leaf-only), and the importer turns that into "a fault in the document engine".
A one-entry linked table of contents written that way is a valid file. The builder now
writes Word's own shape: the field's end goes in a paragraph after the last entry, so the
TOC is a field range (`a_linked_table_of_contents_opens_and_lists_each_heading`). The
importer still refuses the single-paragraph shape from any other producer. The fix belongs
in `casual-doc-import`: degrade the field, as its result text, instead of failing the
document. It is `docs/109` RM-46's open item.

### 12.4 Guards (S1)

`crates/opendoc/tests/sdk.rs` (19 tests) and `tests/deterministic_pixels.rs`. These run in
CI's `test` job (Linux) and `platform` job (macOS arm64, Windows x64). Builder and template
unit tests are in the crate.

- **Mutation:** `layout()` forced to `with_system_fonts(true)`. `pages_render_the_same_pixels_on_every_platform`
  went red: `sample.docx page 1 … fc24f2a86aa20ddb` against the golden's `9305778ed26f9531`.
  It was green again once restored.
- **Mutation (red before the fix):** without the anchor mapping,
  `a_docx_js_shaped_spec_builds_every_story` failed with `left: "noted " right: " noted"`.

### 12.5 The deterministic-pixels gate (V2)

`tests/pixels.golden` holds an FNV-1a hash of each page's **RGBA** (not the PNG, so an
encoder change cannot move it) for the first three pages of the committed `sample.docx` and
for one built document, at 72 dpi with the bundled faces. It is written by
`OPENDOC_BLESS=1 cargo test -p opendoc --test deterministic_pixels` and never by hand. It was
blessed on macOS arm64. CI's Linux `test` job and the Windows `platform` job are its first
cross-platform check, and a red on one platform alone is exactly the finding the gate exists
to make.

### 12.5a The JavaScript package (S2), as built

`packages/opendoc`, npm name `@casualoffice/opendoc` (proposed; the owner decides it with the
other registry names, §10 Q1). It is the wasm build of `crates/opendoc` (feature `wasm`, no
DOM, no `web-sys`) and is not published.

- **Entry points.** `.` resolves to `src/node.js` under Node, which loads the module from disk
  with a top-level await, so every call is synchronous. Elsewhere it resolves to
  `src/index.js`; call `await init()` first. `./docx` is the docx-js-familiar builder.
  `./node` adds `openFile` and `saveFile`.
- **Surfaces.**
  - The `Document` class: reads, python-docx-like edits, `edit(commands)` batches with undo,
    `render(data)` templates, `save(format, { mode })` with its report, and render, PDF and
    page text.
  - `Template`, docxtemplater's facade.
  - `convertToHtml`, `extractRawText` and `convertToMarkdown`, in mammoth's `{ value,
    messages }` shape.
  - `convert`, `formats` and `version`.
  - `OpenDocError`, with a stable `code`; template errors also carry docxtemplater's
    `multi_error` shape.
- **Types.** `types/index.d.ts`, `node.d.ts` and `docx.d.ts`. `examples/typescript` is
  type-checked with `tsc --noEmit` against an installed tarball.
- **Fonts.** Every bundled face is embedded, so a headless render does not depend on the
  machine. The cost is the download (`npm pack`: 12.7 MB packed, 27.1 MB unpacked on
  2026-10-30). This number is not published elsewhere: P3's host-served faces apply to this
  package as they do to the Viewer.
- **Guards.** `packages/opendoc/test/` runs in the new CI job `sdk-package`. It covers the API,
  docx-js's README example unchanged, every builder construct read back, templates and their
  errors, the `npm pack` contents and export paths, and the benchmark record (§12.7).

### 12.6 The CLI (S3): `opendoc`, also the server preview engine

`tools/opendoc-cli` builds one binary, `opendoc`, over `crates/opendoc` with PDF import on:

```text
opendoc convert  IN OUT [--to FMT] [--mode exact|preserve|semantic] [--report FILE] [--strict-loss]
opendoc render   IN [--out DIR] [--format png|pdf] [--dpi N] [--pages 1-3,5]
opendoc info     IN [--json]
opendoc diff     OLD NEW [--json]          # the editor's Compare (casual-doc-diff), as JSON or a summary
opendoc template TEMPLATE DATA.json OUT [--delimiters "[[,]]"] [--strict] [--null TEXT]
opendoc build    SPEC.json OUT [--media FILE]...
opendoc formats
common: --system-fonts  --font FILE  --author NAME
```

- **Exit codes.** 0 is success. 1 is a document or engine error, and the message starts with
  its stable code (`open:`, `template:`, …). 2 is a usage error. 3 means
  `convert --strict-loss` refused a save that would lose content.
- **As a preview server.** `render` lays out with the bundled faces only, unless
  `--system-fonts` is given. The same document then gives the same PNG bytes on every run;
  `render_writes_deterministic_pngs_for_the_pages_asked` checks that within one machine. The
  cross-platform half of V2 is the pixel golden of §12.5: the CLI draws through the same
  `Document::render_page_png`.
- **Guards.** `tools/opendoc-cli/tests/cli.rs` runs the binary end to end for convert (every
  format, an exact save byte-identical to the input, a JSON report), render, info, build,
  template, diff and the exit codes. Mutation: the usage exit code changed from 2 to 1 turned
  `usage_errors_exit_2_and_document_errors_exit_1` red (`left: Some(1) right: Some(2)`).

### 12.7 Benchmark and conformance suite

`benchmarks/sdk/run.mjs` runs the same inputs through OpenDoc and through the permissive
competitors installed on the machine, each library and task in its own process:

| Task | OpenDoc is measured against |
| --- | --- |
| Generation | docx-js, running the same program |
| Templating | docxtemplater, with the same template |
| DOCX to HTML and to text | mammoth |
| DOCX to PDF | LibreOffice headless, when installed |
| Round trip | python-docx, when given |

The suite measures time, peak memory, page counts, and round-trip survival (package parts
kept, body text unchanged, OpenDoc's own loss report). It writes
`benchmarks/sdk/results.json`. The table below is rendered from that file by
`benchmarks/sdk/report.mjs`, and `packages/opendoc/test/bench_record.test.mjs` fails if the
two disagree. To run it:
`node benchmarks/sdk/run.mjs --deps DIR [--python PY] [--soffice PATH]`, on an idle machine.
DIR holds an `npm install` of docx, mammoth, docxtemplater and pizzip. Nothing from it is
committed.

**Read it honestly.** The table makes four points.

- OpenDoc is faster than mammoth at HTML and text, and about level with docx-js at generation.
  It is slower than docxtemplater at filling a template.
- It uses more memory in most tasks, because the module carries a layout engine and every
  bundled face. Its floor is the module itself, and docx-js passes it on the largest input.
- docx-preview needs a DOM, so it is not measured here.
- POI and docx4j are measured from Java (§12.9) when a JDK and their jars are present.

<!-- sdk-bench:begin (generated by benchmarks/sdk/report.mjs from benchmarks/sdk/results.json; do not edit) -->

Measured on darwin arm64 25.3.0, Apple M4 (10 cores, 16 GB), Node v24.22.1; median of 5 runs after a warm-up, each library and task in its own process. Versions: opendoc 0.0.1, docx 9.15.0, docxtemplater 3.71.1, mammoth 1.13.0.

| Task | Input | OpenDoc | Competitor | OpenDoc speed | OpenDoc peak RSS | Competitor peak RSS |
| --- | --- | ---: | ---: | --- | ---: | ---: |
| Generate DOCX (the same docx-js program) | 100 paragraphs + table | 4.79 ms | docx-js: 4.16 ms | 1.2× slower | 246.5 MB | 88.6 MB |
| Generate DOCX (the same docx-js program) | 1000 paragraphs + table | 15.78 ms | docx-js: 17.64 ms | 1.1× faster | 270.8 MB | 150.6 MB |
| Generate DOCX (the same docx-js program) | 5000 paragraphs + table | 60.49 ms | docx-js: 78.96 ms | 1.3× faster | 326.6 MB | 421.7 MB |
| Fill a template (the same template and data) | 100-item loop | 3.78 ms | docxtemplater: 2.71 ms | 1.4× slower | 291.2 MB | 92.9 MB |
| Fill a template (the same template and data) | 1000-item loop | 12.99 ms | docxtemplater: 9.8 ms | 1.3× slower | 324.5 MB | 110.1 MB |
| DOCX to HTML | sample | 16.78 ms | mammoth: 33.86 ms | 2.0× faster | 320.8 MB | 226.6 MB |
| DOCX to HTML | scale | 23.75 ms | mammoth: 55.62 ms | 2.3× faster | 307.7 MB | 288.3 MB |
| DOCX to plain text | sample | 15.57 ms | mammoth: 32.15 ms | 2.1× faster | 311.2 MB | 228.5 MB |
| DOCX to plain text | scale | 16.36 ms | mammoth: 46.46 ms | 2.8× faster | 320 MB | 275.4 MB |
| DOCX to PDF | sample | 57.68 ms | libreoffice: — | — | 358.1 MB | — |
| DOCX to PDF | scale | 272.73 ms | libreoffice: — | — | 365.8 MB | — |

**Round trip** (open, save, reopen; `kept` is input package parts present in the output, `text` whether the body text survived, `losses` OpenDoc's own report):

| Fixture | Parts | OpenDoc preserve: kept / text / losses | OpenDoc semantic: kept / text / losses | python-docx: kept / text |
| --- | ---: | --- | --- | --- |
| sample.docx | 24 | 24 / same / 1 | 18 / same / 5 | 24 / same |
| real-producer-footnotes.docx | 12 | 11 / same / 0 | 11 / same / 0 | 12 / same |
| real-producer-header-footer.docx | 13 | 12 / same / 0 | 12 / same / 0 | 13 / same |
| real-producer-hyperlinks.docx | 9 | 9 / same / 0 | 9 / same / 0 | 9 / same |
| real-producer-libreoffice.docx | 9 | 9 / same / 0 | 9 / same / 0 | 9 / same |
| real-producer-rich.docx | 10 | 10 / same / 0 | 10 / same / 0 | 10 / same |
| real-producer-table-list.docx | 10 | 10 / same / 0 | 10 / same / 0 | 10 / same |
| real-producer-table-merges.docx | 9 | 9 / same / 0 | 9 / same / 0 | 9 / same |
| synthetic-rich-metadata.docx | 7 | 7 / same / 0 | 7 / same / 0 | 6 / same |
| synthetic-declared-family-substitution.docx | 6 | 6 / same / 0 | 6 / same / 0 | 6 / same |

**Pages** (OpenDoc's layout):

| Fixture | OpenDoc pages |
| --- | ---: |
| sample.docx | 14 |
| real-producer-footnotes.docx | 1 |
| real-producer-header-footer.docx | 1 |
| real-producer-hyperlinks.docx | 1 |
| real-producer-libreoffice.docx | 1 |
| real-producer-rich.docx | 1 |
| real-producer-table-list.docx | 1 |
| real-producer-table-merges.docx | 1 |
| synthetic-rich-metadata.docx | 1 |
| synthetic-declared-family-substitution.docx | 1 |

- `scale` is 3,000 paragraphs and 30 headings written by docx-js at run time.
- LibreOffice was not given (--soffice), so its PDF timings and page counts are absent.
- Round trip: `partsKept` counts the input's package parts present in the output; `textSame` re-opens the output with OpenDoc and compares the body text; `losses` is OpenDoc's own save report (no other library reports loss).

<!-- sdk-bench:end -->

[poi]: https://poi.apache.org/components/document/
[pydocx]: https://python-docx.readthedocs.io/en/latest/api/text.html
[pydocx-gh]: https://github.com/python-openxml/python-docx
[docx4j]: https://github.com/plutext/docx4j
[oxsdk]: https://github.com/dotnet/Open-XML-SDK
[aspose-layout]: https://docs.aspose.com/words/net/rendering/
[aspose]: https://products.aspose.com/words/
[docxjs]: https://github.com/VolodymyrBaydalka/docxjs
[mammoth]: https://github.com/mwilliamson/mammoth.js
[uniffi]: https://mozilla.github.io/uniffi-rs/latest/
[uniffi-readme]: https://github.com/mozilla/uniffi-rs
[jep454]: https://openjdk.org/jeps/454

[docxjs-readme]: https://raw.githubusercontent.com/dolanmiu/docx/master/README.md
[docxjs-docs]: https://github.com/dolanmiu/docx/tree/master/docs/usage
[dt-tags]: https://docxtemplater.com/docs/tag-types/
[dt-config]: https://docxtemplater.com/docs/configuration/
[dt-errors]: https://docxtemplater.com/docs/errors/
[dt-pricing]: https://docxtemplater.com/pricing/
[pydocx-home]: https://python-docx.readthedocs.io/en/latest/
[pydocx-quick]: https://python-docx.readthedocs.io/en/latest/user/quickstart.html
[pydocx-comments]: https://python-docx.readthedocs.io/en/latest/user/comments.html
[docxtpl]: https://docxtpl.readthedocs.io/en/latest/
[poi-doc]: https://poi.apache.org/apidocs/dev/org/apache/poi/xwpf/usermodel/XWPFDocument.html
[poi-run]: https://poi.apache.org/apidocs/dev/org/apache/poi/xwpf/usermodel/XWPFRun.html
[docx4j-api]: https://raw.githubusercontent.com/plutext/docx4j/master/docx4j-core/src/main/java/org/docx4j/Docx4J.java
[docx4j-blog]: https://www.docx4java.org/blog/?p=148
[aspose-doc]: https://reference.aspose.com/words/java/com.aspose.words/document/
[aspose-builder]: https://reference.aspose.com/words/net/aspose.words/documentbuilder/
[aspose-replace]: https://reference.aspose.com/words/net/aspose.words/range/replace/
[aspose-layout-collector]: https://reference.aspose.com/words/java/com.aspose.words/layoutcollector/
