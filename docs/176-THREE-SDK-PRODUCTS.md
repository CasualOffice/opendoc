# 176 — Three SDK products on one engine: Viewer, Editor, Native

**Status:** Design, proposed 2026-10-23. **Decides:** ADR-071 (proposed). **Row:** `docs/109`
RM-46. **Built in this change:** the measured viewer cut, `crates/casual-doc-viewer`, and
nothing else. The Editor SDK is not changed by this document.
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

Browser and Node, `@casualoffice/opendoc-viewer` (name: Q1). The shape below is what
`crates/casual-doc-viewer/src/web.rs` exports today:

```ts
import init, { WasmViewer } from "@casualoffice/opendoc-viewer";
await init();
const doc = WasmViewer.open(bytes);            // throws a sentence on a bad file
doc.registerFont(robotoBytes);                 // optional; re-paginates once
for (let i = 0; i < doc.pageCount; i++) {
  const [wTw, hTw] = doc.pageSizeTwips(i);
  const bmp = doc.renderPage(i, 96 * devicePixelRatio);   // RGBA for putImageData
  const png = doc.renderPagePng(i, 24);                    // thumbnail
}
const text = doc.plainText();
const pdf = doc.toPdf();                       // `pdf` feature
```

The server shape is the same crate used natively (`ViewerDocument::open`, `render_page`,
`render_page_png`, `plain_text`, `to_pdf`, `document()`).

| Call | `docs/05` | Status |
| --- | --- | --- |
| `open(bytes)` (DOCX) | §4 `Engine::open_docx` | **Existing** in `casual-doc-viewer` |
| `open` for ODT, RTF, TXT, JSON | §4 | **Existing** behind `formats`. It costs 3.6 MB until P1 (§4.2) |
| `pageCount`, `pageSizeTwips` | §5 `scene` (subset) | **Existing** |
| `renderPage` (RGBA), `renderPagePng` | §5 `scene` | **Existing** |
| `toPdf` | §13 | **Existing** behind `pdf` |
| `plainText` | (none) | **Existing**. Body only: headers, footers and notes are not included yet |
| `registerFont(bytes)` | §10 `ResourceRequest::Font` | **Existing**, synchronous. Host-pull only |
| `importReport()`: what was degraded or dropped | (none) | **Partial**: `CompatibilityReport` exists in `casual-doc-import`/`-io`, but it is not exposed |
| `outline()`, `metadata()` | (none) | **Partial**: in `casual-doc-wasm`, not in the cut |
| `search(query)` → page rectangles | (none) | **Partial**: Find in the editor; geometry needs a hit-test-free rect query |
| `links()` per page | (none) | **Partial**: `linkAt` in the editor uses hit-testing; the viewer needs a per-page list instead |
| Pages of a huge document (windowed) | (none) | **Missing** in the cut: `WindowedBody` lives in `casual-doc-wasm` (R4) |
| `<opendoc-viewer>` element (scroll, zoom) | §3 | **Missing**, and out of scope under the "no UI" rule (Q8) |

Deliberately **absent**: `execute`, `selection`, `handle_key/pointer/ime`, `set_viewport`,
`subscribe`, `save_docx`. These are `docs/05` §5's interactive members, and they belong to
the Editor.

### 3.2 Editor SDK, for contrast only

Unchanged: `@casualoffice/opendoc-embed`, the `<opendoc-editor>` element, the host contract
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

There is **no size ratchet** on the viewer yet. `webapp/tests/wasm_size.test.mjs` guards the
editor module only. Adding the viewer artefact to CI with a ceiling is part of phase V1 (§8),
and until then the numbers above are a dated measurement, not a guarantee.

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
| npm | `@casualoffice/opendoc-viewer` (WASM + `.d.ts`, runs in Node too) | `@casualoffice/opendoc-embed` (exists, unpublished) | — |
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
| **V1**: Viewer SDK | `@casualoffice/opendoc-viewer` on npm; `importReport`, `outline`, `metadata`, `links`, `search` rects; P1, P2, P7; a CI size ceiling for the viewer module | A static page in the repo opens a DOCX and shows every page and a PDF with the package alone; `wasm_size`-style guard on the viewer artefact |
| **V2**: server viewer | `opendoc` CLI and crate; deterministic-render golden across Linux and macOS | The same DOCX renders byte-identical PNGs on both CI platforms |
| **N1**: Native read + convert | C ABI (P4) and Python, Java and .NET packages for open, read view, `import_report`, render, PDF and save-as | One conformance suite of fixtures, run through all four languages, with identical outputs |
| **N2**: Native modify | `edit` batches over `Operation`; DOCX save with a report | A program in each language creates a document with a heading, a table and a tracked change, saves it, and the reopened file matches |
| **E**: Editor | Unchanged: `docs/126` and `109` HF-109 | — |

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
  - Reserve every name before the first release, so nobody squats them. Who holds the registry credentials is still for the owner.

**Still open:** Q1 (registry credentials only), Q4 (whether Native batch edits need the transaction log), Q6 (native platform list), Q7 (whether Node needs a native addon or WASM is enough) and Q8 (a minimal `<opendoc-viewer>` element).

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
