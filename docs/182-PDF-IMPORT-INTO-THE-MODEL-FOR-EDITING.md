# 182 — PDF into the model, for editing: fixed-layout frames first, reflow on demand

**Status:** Design proposed 2026-10-28; owner decision 2026-10-29 started it. P0 and P1 (headless) are built on `feature/pdf-import` (§16), G-1/G-2 in §16a; P2-P6 are not. **It changes product scope**,
which the owner confirmed on 2026-10-29 (Q1: fixed layout by default, hayro, encrypted refused, semantic save first).
**Decides:** ADR-076 (proposed; supersedes the proposed ADR-034). **Row:** `docs/109` RM-49.
**Refines:** `131` (keeps its invariants PDFR-I1..I9, its OCR policy §11 and its finding
families §16.1; replaces its native-evidence provider, its default output shape and its
delivery order). **Builds on:** `98` and ADR-031 (the PDF writer), `135` (MF-I1..I7),
`35` (dispositions), `47` (text boxes, their body properties and page-anchored floats), `119`/`87`
(custom geometry), `40` (fonts), `180` and ADR-073 (the worker), `181` (agents inherit the
format through `open_document`).

**Owner request, 2026-10-28:** "pdf to our modeling, for editing pdf".

---

## 0. Scope, stated first

Three documents say PDF editing is out today:

- SKILL §1: "**Out:** … PDF editing and forms".
- `98` non-goals: "Round-trip PDF *import*. This is export-only." `PdfAdapter` asserts it
  (`casual-doc-io/src/pdf.rs:72`, `can_import: false`; test at `:204`).
- `131` §4.2 and the proposed ADR-034: PDF import is an experiment that "changes no product
  scope".

The owner's request reverses that for **PDF as an input format whose pages become editable
model content**. This document designs that and nothing wider. Still out, unchanged:
editing PDF forms (AcroForm/XFA) as forms, digital signing, PDF JavaScript and actions,
annotations as PDF-native objects, and any server-side conversion. If Q1 is answered yes,
SKILL §1 and `98`'s non-goal are amended in the implementation PR that first sets
`can_import: true`, not before.

## 1. What the category does, and what that means for us

| Product | Family | How | Source |
| --- | --- | --- | --- |
| **Adobe Acrobat** Edit PDF | Fixed layout | Each paragraph becomes a text box; text rewraps inside its box, does not push neighbours or flow to the next page; full editing needs the font installed, an embedded subset allows only colour/size | helpx.adobe.com (§10) |
| **ONLYOFFICE** (source-verified) | Fixed layout | *Edit Text* on a page runs `CPDFDoc.EditPage` → native `scanPage` (`CDocxRenderer::ScanPagePptx`) → PresentationML shapes (`p:sp`, `p:pic`, `p:graphicFrame`) as `CPdfShape`/`CPdfImage`; every body `text_fit_Auto`; the page is then marked `isRecognized` and not re-scanned. Per page, on demand | `sdkjs/pdf/src/document.js` `EditPage`; `core/DocxRenderer/DocxRenderer.cpp` `ScanPage`. AGPL-3.0: read, never vendored |
| **Foxit** | Both | In-block editing with linkable blocks; a separate *Reflow Editing* mode | foxit.com (§10) |
| **Microsoft Word** PDF Reflow | Reflow | Converts a copy to a flow document; good for mostly-text files; documented losses: tables with cell spacing, page colours/borders, frames, footnotes spanning pages, endnotes, bookmarks, tags, font effects | support.microsoft.com (§10) |
| **Google Docs** open-PDF | Reflow | ≤ 2 MB; lists, tables, columns, notes "not likely to be detected" | support.google.com (§10) |
| **LibreOffice Draw** | Fixed layout | poppler (GPL) import, one text box **per line**; "Consolidate Text" joins them; its developers do not call it a high-fidelity editor | ask.libreoffice.org (§10) |

**Reading.** Products that let a user *edit a PDF* (Acrobat, ONLYOFFICE, Foxit) all use
positioned text frames; products that *convert a PDF into a word-processing document*
(Word, Docs) reflow, and lose the layout on anything but prose. The owner asked for both
("our modeling" and "editing PDF"), and the competitor we replace does frames. So: **frames
by default, reflow as an explicit conversion**, from one evidence pass. ONLYOFFICE's
choice to lower into its *presentation* model is the one thing we do differently: our
`TextBox` bodies are ordinary paragraphs through the same flow pipeline as the body
(the uniform-flow rule), so a frame gets lists, styles, spell check, tracked changes,
comments and agent edits for free.

## 2. The patterns, named before any design

| Problem | Known pattern | Where |
| --- | --- | --- |
| Many producers, one internal form | **Adapter / anti-corruption layer**: PDF objects never become editor state (MF-I1) | §3 |
| Two output shapes from one parse | **Pipeline with an intermediate representation**: content stream → `PageEvidence` (glyph runs, paths, images, marked content) → builder | §4 |
| Large files without freezing | **Resumable, budgeted job** (`step(budget)` / `cancel`), the shape `casual-doc-diff/src/job.rs:217` already uses; runs on the main thread cooperatively today and in the ADR-073 worker when it exists | §6 |
| Untouched content must stay byte-identical on save | **Incremental update** (ISO 32000 §7.5.6) with **copy-on-write at page granularity**: a page the user did not change is carried over, not regenerated | §7 |
| Glyph grouping | pdfminer's **char → line → block** margins; **XY-Cut++** (arXiv 2504.10258) for reading order; **Docstrum** for skewed scans | §5.2 |
| Header/footer detection | **Repetition across pages** + `/Artifact` marked content | §5.3 |
| Font → style | **Clustering** by (family, size, weight, italic); the dominant cluster is Normal | §5.3 |
| Trust the producer's structure when it exists | **Tagged PDF structure tree first**, heuristics second, ML third | §5.3 |

## 3. Architecture

```
 bytes ──► casual-doc-pdf-read  (hayro-syntax + hayro-interpret, a custom Device)
               │  PageEvidence per page: glyph runs {unicode, font, size, Tm·CTM, fill},
               │  paths {commands, fill rule, stroke}, images {decoded, transform, clip},
               │  marked content {MCID, struct role}, links, page boxes
               ▼
           casual-doc-pdf-import
               ├─ fixed::build   → v1::Document: one section per page, page-anchored TextBox /
               │                   AnchoredDrawing / WordprocessingGroup, z-order preserved
               ├─ reflow::build  → v1::Document: paragraphs, headings, lists, tables,
               │                   header/footer parts, notes, multi-column sections
               └─ PdfSourceState (I4 sidecar): page object ids, per-frame operator ranges,
                                   font object ids, struct-tree map; retained bytes
               ▼
     casual-doc-io  PdfAdapter: can_import (host-capability dependent), probe, staged import,
                    CompatibilityReport with the `131` §16.1 `pdf.import.*` families
               ▼
     editor / Native SDK / opendoc-mcp  (no PDF code: they open a v1::Document)
```

- **`casual-doc-pdf-read`** is new and wraps hayro. It is separate from `casual-doc-pdf`
  (the writer) because the writer must stay small and dependency-light (ADR-031), and the
  reader is the attack surface (§8).
- **`casual-doc-pdf-import`** depends only on `casual-doc-model` and the reader, following
  the RTF template (`casual-doc-rtf`, `docs/110`). Its adapter lives in `casual-doc-io`
  because `SourceEnvelope::new` is `pub(crate)` (`casual-doc-io/src/artifact.rs:75`).
- **Every surface inherits it.** Once the adapter is registered, `opendoc-mcp`'s
  `open_document`, the Native SDK and the viewer open PDFs with no further work; an agent
  can read, comment and suggest into a PDF-origin document through `181`'s registry.

### 3.1 Why hayro, not PDF.js (a change from `131`)

`131` chose PDF.js in a browser Worker emitting a JSON evidence stream into Rust. Measured
against this repository's constraints:

| | hayro (Rust) | PDF.js (JS) |
| --- | --- | --- |
| Licence | Apache-2.0 OR MIT, 0.8.0 (2026-10-04, crates.io) | Apache-2.0 |
| Runs headless (Native SDK, MCP, CI) | Yes, same code | No: needs a JS runtime, so a second path |
| Determinism (PDFR-I8) | One implementation everywhere | Browser-dependent, evidence must be canonicalised across a boundary |
| Dependency policy | A Rust crate under `cargo deny` | Would be the **first runtime npm dependency** (ADR-042 item 7) |
| Glyph → Unicode | `as_unicode()`: ToUnicode, then glyph list, then `uniXXXX` names; Type3 | `getTextContent` items (string-level, normalised) |
| Paths, images, marked content | `Device` trait: `draw_glyph_run`, `draw_path`, `draw_image`, `begin_marked_content` | Operator list; text and graphics come from different APIs |
| Codecs we lack | `hayro-jbig2` 0.3.1, `hayro-jpeg2000` 0.4.1 (same licence) | Built in |
| Maturity | Younger; tested by its author on 1,600+ PDFs; encryption and some blend modes listed as gaps | Mature, the reference viewer |

hayro wins on every constraint this repository treats as non-negotiable (one engine for
browser, desktop and headless; deterministic; no mandatory JS dependency). Its youth is the
risk, so P0 measures it on the corpus before anything depends on it, and **PDFium via
`pdfium-render`** (Apache/MIT; a multi-MB WASM payload) is the named fallback. PDF.js stays
useful as a **test oracle** (text-extraction cross-check in CI), never as a runtime part.

## 4. The two outputs

### 4.1 Fixed layout (the default on open)

Every PDF page becomes:

- a **section** whose `PageSize` is the page's CropBox (else MediaBox), rotation applied,
  margins zero, followed by a section break, with **one empty anchor paragraph**. Floats are
  placed on the page their anchoring paragraph lands on (`casual-doc-layout/src/anchor.rs:4-8`),
  so each page's frames anchor to that page's paragraph and cannot drift to another page.
- each **text block** (§5.2) → a page-anchored `TextBox` (`v1/body.rs:2136`) with
  `wrap: None`, insets zero, `TextBoxBodyProperties` overflow *grow* (Acrobat's behaviour:
  the box keeps its width and grows downward on edit), holding real paragraphs and runs.
  Line pitch is set with `LineSpacingRule::Exact` from the measured baselines, so an
  unedited frame lays out on the PDF's baselines.
- each **image** → an `AnchoredDrawing` with `CropRect` for the clip.
- each **path group** → a `WordprocessingGroup` of `GroupShape { path: CustomGeometry }`.
- **z-order** from paint order, via `relative_height`.

Edits rewrap within the frame. Linking frames so text flows between them (Foxit) is
**not** in the first release; it is Q4.

### 4.2 Reflow (an explicit command: "Convert to flowing document")

From the same evidence, kept in the sidecar for the session:

- reading order: the struct tree if tagged, else XY-Cut++;
- repeated top/bottom blocks and `/Artifact` content → header/footer parts;
- font clusters → a style sheet (Normal, Heading 1–n, captions);
- bullets/enumerators with hanging indent → numbering (`76`);
- ruling-line grids → tables; whitespace-aligned columns → tables only above a confidence
  threshold, else plain paragraphs and a `pdf.import.table_inferred` finding;
- superscript markers matched to bottom-of-page blocks → footnotes;
- column bands → `SectionColumns`.

It produces a **new** document (like Word's PDF Reflow) and the fixed-layout session stays
open; reflow is not an undoable transform of the frames, because no edit history can
survive a change of document shape. Offered automatically when the PDF is tagged or every
page is single-column prose. Every inference is reported (PDFR-I4).

## 5. Reconstruction

### 5.1 Native evidence (P1)

A `Device` implementation collects, per page, glyph runs with Unicode, font object id,
size, the full text matrix × CTM, fill colour and render mode; paths with fill rule and
stroke; images decoded to the formats the model holds (PNG/JPEG; JBIG2, JPX and CCITT are
decoded by hayro and re-encoded as PNG, reported as `pdf.import.image_reencoded`); marked
content and struct-tree roles; link annotations. Bounded by §8's limits.

### 5.2 Glyphs → lines → blocks

pdfminer's model, with its parameters fixed in code and tested: characters join a line
under `char_margin` horizontal gap and `line_overlap` vertical overlap; a space is inserted
above `word_margin`; lines join a block when they overlap horizontally within
`line_margin`. Rotated text (any non-axis-aligned `Tm`) becomes its own frame with the
rotation on the frame (model gap G-3, §9). A run boundary is any change of font, size,
colour or render mode.

### 5.3 Structure (P3, reflow only)

Struct tree first, then the heuristics of §2, then (P6, optional) an ML layout model. The
fixed-layout output needs none of this, which is why it ships first.

### 5.4 Fonts

| Case | Rendering | Editing |
| --- | --- | --- |
| Embedded TrueType/OpenType (incl. subsets) | The embedded program, registered for the session | New characters the subset lacks fall back to a substitute matched on family name, flags and metrics; one `pdf.import.font_substituted` finding per font, not per glyph |
| Embedded CFF / Type1C | Same, through skrifa (already in the tree, reads CFF) | Same |
| Embedded Type1 (PostScript) / Type3 | Rendered as outlines through `GroupShape` paths when no substitute exists, or substituted | Substituted; reported |
| Not embedded | Font registry by name (`40`) | Same |
| ToUnicode missing or wrong | glyph list → `uniXXXX`; if still unknown, the run is image-only text, marked `pdf.import.native_text_invalid`; OCR (P5) recovers it | Not editable until recovered |

This needs a model slot for a font program that is not a DOCX `.odttf` (gap G-1, §9).

## 6. Staged, cancellable import

`FormatImporter::import` is synchronous (`casual-doc-io/src/registry.rs:69-76`) and the
editor opens on the main thread (`webapp/src/editor_file_open.mjs:192`). PDF import adds,
beside it and without changing it:

```rust
pub trait StagedImporter {
    fn begin(&self, req: ImportRequest, limits: Limits) -> Result<Box<dyn ImportJob>, ImportError>;
}
pub trait ImportJob {
    /// Do at most `budget` of work (pages, operators), then return. O(budget).
    fn step(&mut self, budget: Budget) -> Progress;     // Pending { page, of } | Done | Failed
    fn cancel(&mut self);
    fn finish(self: Box<Self>) -> Result<ImportArtifact, ImportError>; // atomic (PDFR-I3)
}
```

The same shape as `casual-doc-diff`'s job. Today the webapp drives it from
`requestIdleCallback`-sized slices with a progress bar and Cancel, so no task exceeds the
`116` budget; when ADR-073's worker lands, the job moves there unchanged. Small PDFs
(under a page threshold measured in P0) may complete in one step. Nothing is committed to
the session until `finish` succeeds; Cancel leaves the open document untouched.

## 7. Saving

| Target | Path | Release |
| --- | --- | --- |
| PDF, unchanged session | The retained original bytes (`ExactIfUnchanged`, MF-I4) | P2 |
| PDF, edited | **Semantic**: the existing writer renders the model (`98`). Correct, but every page is regenerated | P2 |
| PDF, edited, **preserving** | **Incremental update**: pages whose section is unchanged are carried over as their original objects; an edited page keeps its non-text content stream operators and resources, drops the operators of edited frames (ranges recorded in `PdfSourceState`), and draws the new frame text with our writer's text layer; appended as an incremental update. Signatures on unchanged revisions stay verifiable as *earlier revisions* | P4 |
| DOCX / ODT | Semantic (MF-I5): frames become text boxes, which Word opens and edits | P2 |

The writer may move to **krilla** (Apache/MIT; CFF/TTF subsetting, tagged PDF, PDF/A,
PDF/UA) for P4 and RM-07; that is ADR-031's question to reopen (Q6), not a prerequisite.
Object-level read/write for the incremental update uses `hayro-write` or **lopdf** (MIT,
0.46.0); chosen in P4.

## 8. Security and limits

The reader parses hostile input. It runs under `21`-style limits (file size, object count,
xref depth, decompressed bytes per stream and per document, operators per page, image
pixels, nesting), set from P0's measurements and fuzzed in the `fuzz-build` job like the
DOCX reader. Encrypted files: detected and refused with `pdf.import.encrypted_unsupported`
(no password UI in the first release, Q7). JavaScript, launch actions, embedded files and
external references are never executed or fetched (MF-I6) and are reported
(`pdf.import.active_content_blocked`, `pdf.import.attachment_omitted`). No document byte
leaves the device (PDFR-I6).

## 9. Model and engine gaps this needs

| # | Gap | Evidence | Needed by |
| --- | --- | --- | --- |
| G-1 | Embedded font programs are DOCX `.odttf` only (`EmbeddedFace` carries a `font_key` GUID and `part_name`) | `v1/properties.rs` `EmbeddedFace`; `casual-doc-layout/src/font_registry.rs:425` | P1 |
| G-2 | `CustomGeometry` has no **even-odd fill rule**; caps of 1,024 commands and 64 paths per shape | `v1/custom_geometry.rs:43-46` | P1 (vector-heavy pages degrade and are reported until then) |
| G-3 | Text boxes cannot rotate their text or write vertically | `docs/47` (the body-properties "not built" list) | P1 for rotated runs (else reported) |
| G-4 | No async/staged import; `SourceEnvelope` is not serialisable (cannot cross a worker) | `registry.rs:69`; ADR-073 | P2 (§6) |
| G-5 | ~30 scripts render as notdef; Arabic/Indic shaping unverified; CJK/emoji fetched | `docs/109` FONT-01/02/03 | Substituted text in those scripts (embedded fonts cover the unedited text) |
| G-6 | A model op to record "page carried over unchanged" is not needed: unchanged-ness is computed from the section's content digest against the import digest (`casual-doc-diff/src/identity.rs`), so no `Operation` is added (ADR-030 I2) | — | P4 |

The OOXML exporter needs no change: page-anchored text boxes already export.

## 10. Delivery

One PR per phase, each with its guards driven red (SKILL §4).

| Phase | Delivers | Exit gate |
| --- | --- | --- |
| **P0** decision and measurement | Q1–Q8 answered; ADR-076 accepted; a rights-safe corpus (born-digital prose, multi-column, forms-as-layout, slides-to-PDF, tagged, scanned, hostile) committed or referenced; a hayro spike measuring, per file: parse success, Unicode coverage of glyphs, time and peak memory per page, vs PDF.js text as oracle | The spike's numbers are a committed artifact; limits for `21` set from them; the hayro-vs-PDFium decision recorded |
| **P1** fixed-layout import, headless | `casual-doc-pdf-read`, `casual-doc-pdf-import::fixed`, `PdfAdapter` import behind a feature, G-1, the report families; Native SDK and `opendoc-mcp` open PDFs | Open → render → page-by-page pixel comparison against hayro's own render of the original (an oracle we did not write) under a stated tolerance; edit a frame → save DOCX → reopens in Word with text boxes; hostile corpus refused within budget; fuzz target added |
| **P2** in the editor | File → Open accepts `.pdf` (`editor_file_pick.mjs:17`, both `accept=` lists in `editor.html`, `format_io.mjs`), the staged job with progress and Cancel, the import summary, PDF export semantic + exact-if-unchanged; SKILL §1 and `98` amended | e2e: open a corpus PDF, edit a frame, export PDF, the text layer (`casual-doc-pdf/src/inspect.rs`) holds the edit; no main-thread task over budget during import |
| **P3** reflow conversion | §4.2 and §5.3 | Per construct family (headings, lists, tables, header/footer, notes, columns) a semantic fixture and a loss-report fixture; reflowed tagged PDFs match their struct tree's reading order |
| **P4** preserving save | §7 incremental update; G-6 | An unchanged page is byte-identical in the saved file; a signed PDF edited on one page keeps its original signature valid for the earlier revision |
| **P5** scanned PDFs | `131` §11's OCR (PaddleOCR.js PP-OCRv5, Tesseract.js fallback), selective by region | `131` PDFR-003's gate |
| **P6** optional ML layout | `docling-layout-heron-onnx` or PP-DocLayoutV2/V3 (both Apache-2.0) through ONNX Runtime Web, opt-in and downloaded on demand | `131` PDFR-005's gate; the CPU heuristic path stays the supported baseline |

## 11. Licences: what may and may not be used

Allowed (all within `deny.toml`): hayro family (Apache-2.0 OR MIT), lopdf (MIT), krilla
(MIT OR Apache-2.0), pdfium-render (Apache/MIT) with PDFium (BSD-3/Apache), PDF.js
(Apache-2.0, test oracle), PaddleOCR / PP-DocLayout / Docling layout models (Apache-2.0),
Tesseract.js (Apache-2.0). Versions checked on crates.io 2026-10-28.

**Never** (licence): MuPDF, PyMuPDF, pymupdf4llm (AGPL-3.0); poppler and LibreOffice's PDF
import (GPL); ONLYOFFICE `core` and `sdkjs` (AGPL-3.0, read for facts only); MinerU (custom
licence with revenue thresholds; 2.5 weights tagged AGPL); Marker/Surya weights
(revenue-capped OpenRAIL-M); Nougat weights (CC-BY-NC); LayoutLMv3/LayoutReader weights
(CC-BY-NC-SA); Mistral OCR and the Adobe PDF Services APIs (cloud, break local-first).

## 12. Risks

| # | Risk | Mitigation |
| --- | --- | --- |
| R1 | hayro is young; a parse gap means an unopenable file | P0 measures it; PDFium fallback named; every failure is a typed refusal, never a partial document |
| R2 | Frame-per-paragraph output feels like "a pile of text boxes" in DOCX | That is what Acrobat and ONLYOFFICE produce too; the reflow command is the answer for prose, and the import summary says which to use |
| R3 | Substituted fonts reflow edited text out of its frame | Frames grow downward; a finding names the substitution; the original subset is kept for unedited text |
| R4 | Main-thread stalls on large files | §6's budgeted job from the first release in the editor; page-count limit until the worker exists |
| R5 | Scope creep into forms, signatures, annotations | §0 lists them out; each would need its own ADR |
| R6 | Overclaiming fidelity | Support matrix (`18`) lists families graded from the corpus, not from successes (SKILL §9.3) |

## 13. Open questions for the owner

- **Q1. Scope.** Confirm PDF becomes an importable, editable format (fixed layout + reflow),
  and that SKILL §1's "PDF editing" exclusion is narrowed to forms, signatures, annotations
  and scripts.
- **Q2. Default on open.** Fixed layout (Acrobat/ONLYOFFICE), with reflow offered for tagged
  or single-column prose — or ask the user each time?
- **Q3. Native provider.** hayro first with PDFium as fallback (this document), or `131`'s
  PDF.js?
- **Q4. Linked frames.** Text flowing between frames (Foxit) in a later phase, or never?
- **Q5. Preserving save (P4).** Is byte-preserving incremental save required for the first
  editor release, or is semantic PDF export acceptable until P4?
- **Q6. Writer.** Reopen ADR-031 to consider krilla for P4 and tagged PDF/PDF-A (RM-07)?
- **Q7. Encrypted PDFs.** Refuse in the first release, or add a password prompt (open-only
  passwords; permissions are advisory)?
- **Q8. OCR and ML.** Keep `131`'s OCR plan as P5 and ML layout as optional P6, both
  downloaded on demand and never mandatory?

## 14. Sources

- Acrobat text boxes: <https://helpx.adobe.com/acrobat/desktop/edit-documents/edit-text-in-pdfs/adjust-text-boxes.html>;
  fonts: <https://helpx.adobe.com/acrobat/kb/error-no-available-system-font.html>
- Word PDF Reflow: <https://support.microsoft.com/en-us/office/opening-pdfs-in-word-1d1d2acc-afa0-46ef-891d-b76bcd83d9c8>
- Google Docs open-PDF: <https://support.google.com/drive/answer/176692>
- Foxit reflow editing: <https://resource-cms.foxit.com/foxit-pdf-editor-for-windows/reflow-editing-pro-only>
- ONLYOFFICE (AGPL, facts only): <https://github.com/ONLYOFFICE/sdkjs/blob/master/pdf/src/document.js>,
  <https://github.com/ONLYOFFICE/core/blob/master/DocxRenderer/DocxRenderer.cpp>
- LibreOffice Draw PDF import: <https://ask.libreoffice.org/en/question/268140/>
- hayro: <https://github.com/LaurenzV/hayro>; krilla: <https://github.com/LaurenzV/krilla>;
  lopdf: <https://github.com/J-F-Liu/lopdf>; pdfium-render: <https://github.com/ajrcarey/pdfium-render>;
  PDF.js text API: <https://github.com/mozilla/pdf.js/blob/master/src/display/api.js>
- pdfminer layout parameters: <https://pdfminersix.readthedocs.io/en/latest/topic/converting_pdf_to_text.html>;
  XY-Cut++: <https://arxiv.org/abs/2504.10258>
- Docling: <https://github.com/docling-project/docling>; heron ONNX:
  <https://huggingface.co/docling-project/docling-layout-heron-onnx>; PaddleOCR:
  <https://github.com/PaddlePaddle/PaddleOCR>; OmniDocBench (measures text/table/formula
  to Markdown, not editability): <https://github.com/opendatalab/OmniDocBench>
- Licences flagged in §11: MinerU <https://github.com/opendatalab/MinerU/blob/master/LICENSE.md>;
  Surya <https://github.com/datalab-to/surya>; Stirling-PDF
  <https://github.com/Stirling-Tools/Stirling-PDF/blob/main/LICENSE>
- **Not verified from a primary source:** how ONLYOFFICE serialises an edited page back to
  PDF; the internals of Nitro, PDFgear, Smallpdf and iLovePDF; the dots.ocr weights licence.
  None is load-bearing here.

## 15. What this change is

Documentation only: this document, ADR-076 (proposed, superseding the proposed ADR-034) and
`docs/109` RM-49. No code, no dependency, no scope change until Q1 is answered.

## 16. Built: P0 and P1 (headless)

Branch `feature/pdf-import`, owner decision 2026-10-29 (start, with this document's
recommended defaults: fixed layout, hayro with PDFium as fallback, encrypted refused,
semantic PDF save first). Headless only: the editor does not open PDFs (P2), and SKILL §1
and `98` are amended in the P2 PR.

### 16.1 P0: the measurement and the decision

`tools/opendoc-pdf-measure` runs hayro-syntax 0.8.0 + hayro-interpret 0.8.0 through a
counting `Device` and records per file: parse outcome and reason, glyphs and the share with
Unicode, Type3 glyphs, paths and even-odd fills, images with codecs and declared pixels,
marked content, embedded program kinds and whether each embedded sfnt maps every character
it drew, operators, decoded content bytes, and time and peak heap per page. The committed
artifact is `fixtures/pdf/measurement.json`; its deterministic half is re-derived by the
crate's test, so a number quoted here cannot drift silently (SKILL §9.1).

The rights-safe corpus (`fixtures/pdf/`, all generated by the harness or by this
repository's writer): 10 synthetic construct files (prose, two columns, vectors with an
even-odd ring, raw/Flate/DCT images with a clip, rotated text, tagged content, Type3 without
`ToUnicode`, `CropBox` and `/Rotate`, active content, an embedded whole TrueType), 9 fixture
DOCX files rendered by our PDF writer, and 9 hostile files.

| Measured | Value | Source |
| --- | --- | --- |
| Corpus files parsed / refused | 26 / 2 (the refusals: fake `/Encrypt`, garbage) | `measurement.json` `aggregate` |
| Glyphs with Unicode | 3,879 of 3,885 (the 6 misses are the Type3 glyphs with no `ToUnicode`, by construction) | same |
| Embedded fonts registrable for editing | 1 of 32: none of our writer's 12 subsets has a Unicode cmap; the Caladea file's does | same |
| Flate bomb (96 MiB) / operator flood (2,000,000) heap peak, uncapped | 235 MB / 188 MB | same, `timing` |
| Private local run (`~/Downloads`, 4 files, aggregate only, nothing committed) | 4/4 parse; 56 pages; 100,354 of 100,813 glyphs with Unicode (99.5%); 34 of 116 fonts registrable; max page 6.8 ms and 14.5 MB heap; max 19,274 operators per page; max image 7.7 MP; codecs DCT, Flate, raw | one run, 2026-10-29 |

Not met by any corpus file, so absent rather than zero: JBIG2, JPX, CCITT.

**Decision: hayro.** It parsed every non-hostile file, recovered Unicode for every glyph that
has a mapping, and every hostile file is refused by our limits before hayro does the work.
PDFium (`pdfium-render`) is not added; it stays the named fallback. The limits are in `docs/21`
"PDF Import Defaults", each with its measured basis. `cargo deny --locked check licenses bans
sources`: bans ok, licenses ok, sources ok (one new duplicate warning, `fearless_simd`).

### 16.2 P1: what is built

- **`casual-doc-pdf-read`.** `PdfSource::open` admits (size, header, encryption refused with
  a sentinel password so an empty user password is refused too, objects, pages, page side,
  and a bounded-inflate pre-pass over every Flate stream before anything decodes one) and
  inventories active content without running or extracting it. `page_evidence(i)` counts the
  page's operators, learns font identities from the resources, and interprets the page
  through a collecting `Device`: glyph runs, paths, images (DCT passed through, the rest
  re-encoded as PNG with alpha), marked content, clips, page boxes. O(page) per call.
- **`casual-doc-pdf-import`.** `fixed::build` as §4.1: a section per page, one anchor
  paragraph, text blocks by pdfminer's grouping (parameters fixed in `group.rs`) as
  page-anchored `TextBox`es with Exact line pitch and grow-on-overflow, images as
  `AnchoredDrawing` with `CropRect`, consecutive paths as one `WordprocessingGroup`, z-order by
  paint order. Uses G-1 (an embedded sfnt whose cmap covers the text it drew is registered as
  an `EmbeddedFontProgram`, bytes in the resource map; otherwise substituted and reported) and
  G-2 (`with_fill_rule(EvenOdd)`). `ImportJob` (`begin`/`step(pages)`/`cancel`/`finish`) is
  atomic; `import_pdf` drives it.
- **Report families** (`report::FEATURES`): `active_content_blocked`, `annotation_omitted`,
  `attachment_omitted`, `clip_ignored`, `encrypted_unsupported`, `font_embedded`,
  `font_substituted`, `form_omitted`, `image_clip_ignored`, `image_reencoded`,
  `image_undecodable`, `invisible_text`, `native_text_invalid`, `optional_content_flattened`,
  `page_size_clamped`, `paragraph_boundary_inferred`, `pattern_flattened`,
  `reading_order_inferred`, `tag_structure_ignored`, `text_render_mode_degraded`,
  `text_rotated`, `transparency_flattened`, `type3_glyphs_substituted`, `vector_split`, each
  `pdf.import.*`.
- **`casual-doc-io`.** `PdfImportAdapter` behind feature `pdf-import`: probe on `%PDF-`, import
  through the job, retained original written back exactly by an `ExactIfUnchanged` export of
  an unchanged session. The capability is explicit: `builtin_registry()` never imports PDF;
  `enable_pdf_import` / `builtin_registry_with_pdf_import` replace the export-only
  registration.
- **Headless reach.** `opendoc-viewer` (feature `pdf-import`) and `casual-doc-ai` (feature
  `pdf-import`, enabled by `opendoc-mcp`, whose `open_document` now accepts `.pdf`). The viewer
  also registers a document's own faces at open and reads text-box text in `plain_text`.
- **Fuzzing.** `fuzz/fuzz_targets/pdf_import.rs`, built by the `fuzz-build` job.

What a PDF page becomes, for `synthetic-prose.pdf` page 1: a section of 12,240 × 15,840 twips
with zero margins; one paragraph ending that section; inside it three `TextBox`es anchored to
the page at the heading, the first four-line paragraph and the second, each holding one
paragraph whose lines are separated by line breaks, Exact 280-twip pitch (the 14 pt
leading), Times New Roman (substituting the unembedded Times-Roman, reported).

### 16.3 Guards, each driven red by a mutation (SKILL §4)

| Guard | Mutation | Red output |
| --- | --- | --- |
| `opendoc-pdf-measure` `the_committed_measurement_matches_a_fresh_run` | edit a glyph count in the artifact | `"synthetic-prose.pdf" drifted: re-run the harness` |
| `pdf-read` `the_hostile_corpus_is_refused_by_the_right_bound` | remove the bomb pre-pass | left `"accepted"`, right `"max_stream_decoded_bytes"` |
| `pdf-read` `unicode_matches_the_independent_inspector_on_every_writer_file` (vs `casual-doc-pdf/src/inspect.rs`) | drop the mapping of `e` | fails at `tests.rs:78` on the first writer file |
| `pdf-import` `tests/oracle.rs` `every_page_matches_hayros_render_of_the_original`: 72 dpi, ink recall and precision within 2 px ≥ 0.85 on every page (lowest observed 0.878) | text frames 4 pt lower | 8+ pages fail, e.g. `synthetic-prose.pdf page 1: recall 0.546 precision 0.520` |
| same, while integrating G-2 | (a real defect, not a mutation) group `a:off` set to its page position on top of the anchor | `synthetic-vector.pdf page 1: recall 0.138 precision 0.144`; fixed |
| `pdf-import` `the_even_odd_rule_reaches_the_model` | force `NonZero` | `exactly the donut is even-odd: [NonZero, …]`. The oracle alone does not catch this (the hole is ~11% of the page's ink) |
| `pdf-import` `tests/docx_round_trip.rs` (edit a frame, save DOCX, reopen: same frames, pictures, crops, shapes, edit, bytes, sections; no non-mapped writer entry) | pictures stored under `media/` | `synthetic-images.pdf: picture bytes`. The DOCX writer did not report that loss; the census caught it |
| `pdf-import` `grouping_work_doubles_when_a_pages_lines_double` (counter, not clock) | unbounded block window | `ratio 4.01 (79800 -> 319600)` |
| `opendoc-mcp` `an_agent_opens_a_pdf_reads_it_and_suggests_into_a_frame` | drop `pdf` from the allowlist | `open_document refused: unsupported-format` |
| `opendoc-viewer` `a_pdf_opens_paginates_renders_and_exports` | (before the fix) `plain_text` without text boxes | empty text, assertion at `pdf_import.rs:22` |

### 16.4 Not built, and why

- **P2 (editor):** File → Open, the staged job driven from the UI with progress and Cancel,
  SKILL §1 and `98` amended. The built-in registry deliberately does not import PDF.
- **G-3:** rotated text (and every page with `/Rotate` 90/270, whose text reads rotated) is
  placed upright in its own frame and reported `text_rotated`; the oracle exempts those pages
  only because they report it.
- **Bare CFF and Type1 programs** are substituted, not wrapped into `OTTO`. **A second subset**
  of a family and style already registered is substituted (the slot is taken).
- **Annotations, links, forms, signatures, scripts** are counted and reported, not imported.
- **Linked frames, reflow (P3), preserving save (P4), OCR (P5).**
- **Measured caveats:** the oracle tolerance is ink agreement, so it cannot see colour or a
  filled hole smaller than ~15% of a page's ink; the PDF's line ends are kept as hard
  line breaks inside a frame, so an edit rewraps only up to the next one; a line joins a block only
  within the last 32 blocks, so content streams that interleave more than 32 columns line by
  line split into more frames.

## 16a. Model gaps G-1/G-2 — built

Branch `lane/pdf-model-gaps`. Both gaps of §9 that sit in the model, layout, render and
DOCX writer; the reader and importer crates are untouched.

**G-1, the font-program slot.** `Definitions::font_programs: Vec<EmbeddedFontProgram>`
(`v1/properties.rs`, `#[non_exhaustive]`, built with `EmbeddedFontProgram::new` and
`with_*`): family, `FontProgramStyle` (the four `w:embed*` slots), `FontProgramFormat`
(`TrueType`, `OpenTypeCff`, `BareCff`), `part_name`, `subsetted`, `source_name`. The bytes are
a resource-map part, unobfuscated, exactly like `MediaReference` and `.odttf` faces — not a
model field, so a snapshot stays the size of its metadata. Validated (non-empty, bounded,
at most `MAX_FONT_PROGRAMS` = 4,096). Serde default; omitted when empty.

- **Layout.** `register_embedded_fonts` registers programs after the `.odttf` faces, in the
  same top tier, under the same aggregate budget and the same failure report. The model's
  style is authoritative over the face's own OS/2 (a PDF subset's often is wrong). A bare CFF
  is reported `unsupported-format` (the importer must wrap it as `OTTO`); bytes contradicting
  the declared format are `not-sfnt`; absent bytes are `missing-part`. No host change: the wasm
  host already passes its resource map.
- **DOCX.** A TrueType program is embedded as `word/fonts/program{n}.odttf`, obfuscated per
  ECMA-376 Part 1 §17.8.1 with a `w:fontKey` derived deterministically from the program, under
  its family's `w:font` (the document's own entry when one exists, in schema slot order).
  Not carried, and reported: CFF-flavoured or bare CFF (`docx.export.font_program.not_carried`
  — `.odttf` is obfuscated TrueType, and this writer does not claim a consumer reads CFF
  from one), absent bytes (`…missing_bytes`), a second subset of a slot already filled
  (`…slot_taken`).

**G-2, the fill rule.** `CustomGeometry::fill_rule: FillRule` (`NonZero` default, `EvenOdd`),
serde default, omitted when non-zero. `CustomGeometry` is now `#[non_exhaustive]` with a
builder (`from_paths`, `with_guides`, `with_handles`, `with_connections`, `with_text_rect`,
`with_fill_rule`); the one cross-crate literal (`casual-doc-import/src/vml_map.rs`) moved to it.

- **Render.** The rule rides `AnchorPath::fill_rule` into `ShapeGeometry::Path::fill_rule`;
  tiny-skia fills with `FillRule::EvenOdd` for it. The PDF writer (`casual-doc-pdf`) emits
  `f*`/`W*` for it. **Not yet:** `casual-doc-render-gpu` fills every path non-zero (its
  `SparseCanvas` has no fill-rule parameter); an even-odd ring paints solid there.
- **DOCX.** DrawingML cannot state a rule: `CT_Path2D` (§20.1.9.15) has only `w`, `h`, `fill`,
  `stroke`, `extrusionOk`, and the standard does not say which rule a consumer applies. So the
  writer emits a geometry for which the question does not arise: `CustomGeometry::to_non_zero`
  (`v1/fill_rule.rs`) re-orients each filled path's subpaths by nesting parity — the
  contour-orientation normalisation font tools apply — which makes every winding number 0 or
  ±1, where the rules agree. When subpaths cross or touch, a subpath self-intersects, a
  coordinate is a guide name, or an open stroked subpath would have to reverse, it writes the
  geometry as authored and reports `docx.export.custom_geometry.even_odd_fill`
  (`degraded`, `not_retained`, charged to the part it was written in). Curves are tested at 8
  chords each.

**Guards, each driven red by a mutation:**

| Guard | Mutation | Red output |
| --- | --- | --- |
| `fill_rule.rs` `a_same_wound_hole_is_reversed_so_non_zero_keeps_it`, `nesting_parity_decides_each_orientation`, `a_reversed_cubic_keeps_its_shape` | never flip a subpath | `the centre is a hole` left 2, right 0; `(75.0, 75.0)` left true, right false |
| `v1/tests.rs` `a_snapshot_without_a_fill_rule_reads_as_non_zero_and_even_odd_round_trips` | drop `skip_serializing_if` on `fill_rule` | `a non-zero geometry writes what it was read from: {…"fillRule":"nonZero"}` |
| `render/tests/even_odd_fill.rs` `an_even_odd_path_paints_its_inner_square_as_a_hole` (whole page: model → layout → tiny-skia) | renderer fills even-odd with `Winding`; separately, layout drops the geometry's rule | `under even-odd the same-wound inner square is a hole, got [255, 0, 0, 255]` (both) |
| `…a_non_zero_path_fills_its_same_wound_inner_square` | renderer fills every path even-odd | `…is filled, got [255, 255, 255, 255]` |
| `layout/tests/embedded_fonts.rs` four `font_program` tests | skip the programs loop | `faces` left 0, right 1 / 2; failures left `[]`, right `[NotSfnt, MissingPart]` |
| `…a_bold_font_program_serves_bold_runs` | register every program regular | `bold=false picked the program its style declares`. The first version stayed green under this mutation (registration order coincided with style); it now crosses the bytes and registers bold first |
| `export/tests/pdf_model_gaps.rs` `a_truetype_font_program_is_embedded_as_an_obfuscated_odttf` | XOR with the key un-reversed | `an independent ECMA-376 decoder recovers the program` left ≠ right |
| `…an_uncarried_font_program_is_reported_by_reason` | skip the TrueType check | the two `not_carried` entries missing. Its format and signature halves are redundant for these inputs, so it is mutated whole |
| `…an_even_odd_ring_is_written_re_wound_so_its_hole_survives` | write the authored geometry | the reversed inner square absent from `document.xml` |
| `…an_even_odd_geometry_that_cannot_be_re_wound_is_reported` | drop the loss note | left `[]`, right `[("docx.export.custom_geometry.even_odd_fill", Some("word/document.xml"))]` |

The `snapshot_without_font_programs` test pins the G-1 serde default and validation.

## 18. P3 built: reflow, wrapped CFF and Type1 programs, links and annotations

Branch `lane/pdf-p3-reflow` (from `502ff112`). Headless: the engine, its API and its
guards. No editor or MCP surface is wired (that is the P2 lane's and the session
layer's, and the decision below says where it goes).

### 18.1 Reflow: "Convert to flowing document" (§4.2, §5.3)

`PdfImport::reflow()` (and the pure `reflow::build(state, resources, namespace)`)
returns a **new** validated `v1::Document` with its own resources and report; the
fixed-layout document is not touched (a test asserts it is equal before and after,
and that two runs are equal: PDFR-I8). It reads only `PdfImport::source`, the
`PdfSourceState` sidecar the import job now fills while it builds the frames:
per page, text grouped by pdfminer's margins *within* each marked-content
sequence (so a block never straddles two structure elements), ruling lines (thin
filled rectangles and stroked axis-aligned segments), the pictures by the part the
fixed build stored them under (referenced, not copied), the annotations, and
counts of what reflow cannot place; per document, the fonts and the programs the
fixed build registered, the structure tree and the active-content inventory. No
second parse.

| Pass | Where | Known pattern | Reported |
| --- | --- | --- | --- |
| Page furniture: blocks in the top/bottom 10% repeated on ≥ half the pages (and ≥ 2), digit runs normalised so page 9 and page 10 match, plus `/Artifact` content in those bands; a page number equal to the page index becomes a `PAGE` field; pieces on one baseline become one paragraph with centre/right tab stops | `reflow/mod.rs` | repetition across pages + `/Artifact` (§2) | `header_footer_inferred`; artifacts elsewhere `artifact_omitted` |
| Ruled grids: union-find over rule intersections, a component with ≥ 2 rules each way and ≥ 2 cells is a grid; missing inner borders become `grid_span` / `vertical_merge`; every character goes to the cell holding its centre | `reflow/tables.rs` | union-find; interval index per grid line | `table_inferred` |
| Whitespace tables: lines sharing a baseline in disjoint blocks link the blocks; `confidence = aligned share × (1 − prose share) × min(1, rows/3)`; ≥ 0.75 is a table, 0.4–0.75 stays paragraphs | `reflow/tables.rs` | — | `table_inferred`; below threshold `table_topology_degraded` |
| Footnotes: superscript runs (≤ 0.8 of the line's size, raised) matched to lower-half, smaller-than-body blocks whose lines start with the same marker, the note below its reference; the reference becomes a `NoteReference` | `reflow/notes.rs` | index by marker text | `footnote_inferred` |
| Reading order: the structure tree when ≥ 80% of a page's text elements are tagged (untagged ones keep their XY position after the tagged one before them); else XY-Cut++ | `reflow/mod.rs`, `reflow/order.rs` | XY-Cut++ (arXiv 2504.10258): pre-mask cross-layout elements, density-driven recursive cut, re-insertion | `reading_order_inferred` (XY-cut pages only) |
| Paragraphs: break at a marker, a font change, a gap > 0.6 line, a short line ending a sentence, a first-line indent; join lines with a space; de-hyphenate a letter-hyphen-lowercase join; continue a paragraph across a column or page when it ran to the column's end without terminal punctuation and the next starts lowercase in the same font, or when both pieces belong to the same structure element | `reflow/text.rs`, `reflow/emit.rs` | — | `paragraph_boundary_inferred`, `dehyphenation_inferred` |
| Styles: every character votes for its (family, size, bold, italic); the winner is Normal; a larger (×1.15) or bolder key whose paragraphs are ≥ 80% short (≤ 3 lines, ≤ 200 characters, not prose-ending) is a heading level, ranked by size then weight. A tagged `H1`–`H6` (through the `RoleMap`) decides outright, and font clusters never classify a tagged paragraph | `reflow/text.rs` | clustering (§2) | `style_inferred`, `heading_inferred` |
| Lists: a marker set an indent tab from its text is put back on its line first (pdfminer splits it off); runs of bullet/enumerator paragraphs of one kind with a hanging indent (a continuation aligned with the text, or two items whose markers and text starts align), numbers consecutive per level; levels by marker position; one abstract definition per run with the geometry's indent and hanging | `reflow/mod.rs`, `reflow/emit.rs` | — | `list_inferred` |
| Columns: a vertical cut whose parts are tall and of similar width is a column region; a change of column count starts a continuous section with `SectionColumns` | `reflow/order.rs`, `reflow/emit.rs` | — | `columns_inferred` |

Also reported: `vector_omitted` (vector art that is not a rule), `text_rotated`
(rotated runs are reflowed upright where they start, never dropped),
`tag_structure_ignored` (a truncated tree, or a tagged file no page could be
ordered by), and the link and note families of §18.3.

**Command registry: not registered, on purpose.** `casual-doc-command`'s registry
declares edits: each entry lowers to `Operation`s on the open document, enforced at
the AI layer's choke point (`emits`). Reflow is not an edit: it creates a second
document from the import's source state, which an open session does not carry once
the document is saved as DOCX, and no `Operation` replaces a document (ADR-030 I2).
It is a session-level operation beside `open_document`, so it belongs in the session
layer (`opendoc-mcp`'s tools, the Native SDK, the editor's File menu), each calling
`PdfImport::reflow()`. None of those is this lane's.

### 18.2 Fonts: bare CFF and Type1 made OpenType (§5.4, closing §16.4)

`fonts.rs`, on read-fonts (MIT/Apache, already under skrifa; the `agl` feature is
enabled, no new crate or version):

- **Bare CFF** (`FontFile3`/`Type1C`) is *wrapped*: its bytes become the `CFF `
  table of an `OTTO` sfnt whose `head`, `hhea`, `maxp` 0.5, `OS/2` v4, `name`,
  `cmap` (format 4, and 12 above the BMP), `post` 3.0 and `hmtx` are built from the
  program (advances from its charstrings, bounds from its outlines). Outlines and
  hints are the producer's, byte for byte.
- **Type1** (`FontFile`) is *converted*: read-fonts' Type1 interpreter evaluates each
  charstring (subroutines and `seac` flattened) and the outline is re-encoded as an
  unhinted Type2 charstring in a CFF this module writes. Outlines are exact; hints
  are dropped.
- The `cmap` comes from glyph names through the Adobe Glyph List. A program is
  registered only if that `cmap` covers every character drawn **and** its advances
  agree (5%, at most 10% of characters off) with where the PDF placed them, so a
  glyph name that lies cannot draw the wrong letter silently.
- **A reader gap found on the way:** hayro hands the device no program for a simple
  `/Type1` font (`OutlineFont::font_data` is `None` for `Type1`), so the common
  bare-CFF case never reached the importer; P1's `BareCff` was only ever a CID-keyed
  Type0. `admit.rs` now reads a simple Type1 font's `FontFile3`/`FontFile` from its
  descriptor, bounded by `max_font_program_bytes`.

### 18.3 Links and annotations (both outputs)

`casual-doc-pdf-read` gains `annots.rs` (`PdfSource::annotations(page)`: kind,
rectangle in page space, author, contents, ISO 8601 date; links resolved: `URI`,
`GoTo`, `/Dest`, named destinations through `/Dests` and the `/Names /Dests` tree;
`javascript:`, `vbscript:`, `file:` and `data:` URIs and non-navigation actions are
`Blocked`) and `structure.rs` (`PdfSource::structure_tree()`, the tree flattened in
reading order with each leaf's role lineage, `RoleMap` applied, cycles refused,
bounded). `device.rs` and `evidence.rs` are untouched.

- A link becomes a hyperlink over the characters whose centre its rectangle holds,
  in the fixed frames and in reflow (`hyperlink_inferred`); a `GoTo`/`Dest` becomes
  an internal hyperlink to a `_PdfDestN` bookmark at the frame (fixed) or paragraph
  (reflow) at the destination's top (`bookmark_inferred`); a link to nothing, a
  blocked one, or one over no text is `link_unresolved`.
- A `Text` or `FreeText` note becomes a comment with its author, date and text,
  anchored on the nearest frame (fixed) or line (reflow): `comment_inferred`.
- Every other annotation is `annotation_omitted`; popups count with their note.
  Admission's blanket `annotation_omitted` for links and notes is gone: each page now
  reports what it did not import.

### 18.4 Guards, each driven red by a mutation (SKILL §4)

Fixtures are rights-safe synthetic PDFs written by `src/synth.rs` from constants in
the test (P0's approach; generated rather than committed, so P0's
`fixtures/pdf/measurement.json` does not move). Font fixtures are Caladea's own
outlines (Apache-2.0, bundled) re-encoded as a bare CFF and as an eexec-encrypted
Type1 program.

| Guard | Mutation | Red output |
| --- | --- | --- |
| `fonts` `a_bare_cff_is_wrapped_as_an_otto_font_skrifa_reads_with_the_same_outlines` | cmap off by one glyph | `outline of 'H'` left `[(174, -7), (78, 23), …]` right `[(481, 317), (206, 317), …]` |
| `fonts` `a_type1_program_is_converted_with_the_same_outlines_and_advances` | Type2 curve's second control point relative to the start, not the first control | `outline of 'H'` left `[…, (328, 37), (320, 0), …]` right `[…, (304, 37), (299, 0), …]` |
| `a_wrapped_program_whose_advances_contradict_the_pdf_is_substituted` | advances always agree | `assertion failed: …font_programs.is_empty()` |
| `bare_cff_and_type1_programs_are_made_opentype_and_register_for_layout` (layout registers both, 0 failures) | `admit.rs` reads no simple-Type1 program | left `[]` right `[("SynthCff", OpenTypeCff), ("SynthType1", OpenTypeCff)]` |
| `order` `two_columns_read_column_by_column_under_a_spanning_title` | no pre-mask; separately, never a column cut | left `[0, 1, 4, 2, 5, 3, 6]` right `[0, 1, 2, 3, 4, 5, 6]` (both) |
| `order` `ordering_work_doubles_when_the_elements_double` | gap-beside-a-mask check scans every earlier span | `two columns: ratio 3.96 (255503 -> 1011003)` |
| `a_tagged_pdf_reflows_in_its_structure_tree_order` (paint and geometry agree with each other and disagree with the tree; the H1 is in the body font) | structure tree ignored | left `[(heading 1, "Tagged heading…"), (None, "Read last…"), (None, "Read first…")]` right `[(None, "Read first…"), (heading 1, …), (None, "Read last…")]` |
| `headings_become_styles_and_lines_become_paragraphs` | no heading level from font clusters; separately, line-end hyphen kept | left `(None, "Annual Synthetic Review")` right `(Some("heading 1"), …)`; `"…while the compre-hension of the committee…"` |
| `a_lone_numbered_line_is_not_a_list` | any marker run is a list | `assertion failed: paragraphs(…).iter().all(\|p\| p.properties.numbering.is_none())` |
| `ruling_grids_and_aligned_columns_become_tables` | no line is placed in a grid | first table left `[["Item","Qty","Price"], …]` right `[["R1C1","R1C2","R1C3"], …]` |
| `two_column_prose_on_a_shared_grid_is_not_a_table` | prose share ignored in the confidence | `assertion failed: …all(\|b\| !matches!(b, BlockNode::Table(_)))` |
| `repeated_top_and_bottom_lines_become_header_and_footer_parts` | repetition never enough | body contains `Synthetic Quarterly Report` |
| `superscripts_matched_to_bottom_blocks_become_footnotes` | superscript threshold 0.5 | body left 3 paragraphs right 1: `"…the plan1 and then…", "1 The plan is synthetic.", …` |
| `a_numbered_line_at_the_bottom_with_no_reference_stays_body_text` | a note needs no reference | `assertion failed: …footnotes.is_empty()` |
| `column_bands_become_section_columns_and_read_column_by_column` | no column regions; separately, no paragraph continues into the next element | `the title's section, then the columns'` left 1 right 2; left `"…over the last quarter"` right `"…over the last quarter and agreed that every number was written here."` |
| `links_become_hyperlinks_and_notes_become_comments_in_both_outputs` | no character under a link; bookmark always on the anchor paragraph; notes not made comments | `fixed: []`; left `None` right `Some("Appendix")`; `fixed` left 0 right 1 |
| `the_loss_report_names_every_construct_reflow_could_not_keep` (exact set of non-mapped findings; nothing visible dropped) | vector art not reported; separately, rotated text dropped | `vector_omitted` missing from the set; the body lacks `Sideways note` |
| `furniture_and_note_work_doubles_when_the_pages_double` | repetition found by scanning every earlier block | `furniture: ratio 3.82 (880 -> 3360)` |
| `footnote_work_doubles_when_the_notes_on_a_page_double` | references re-scanned for every note line | `notes: ratio 3.41 (14292 -> 48692)` |
| `table_work_doubles_when_the_rows_double` | every rule against every rule; separately, every line searches every grid | `tables: ratio 3.67 (2247 -> 8247)`; `ratio 3.20 (1047 -> 3347)` |
| `marker_and_paragraph_work_doubles_when_the_items_double` | each marker scans every earlier line | `paragraphs: ratio 3.94 (23249 -> 91499)` |
| `annots` `pdf_dates_become_iso_8601_with_their_offset`, `text_strings_decode_utf16_and_pdfdoc` | (written red: a non-digit after the year parsed as a date; `\r` dropped before it became `\n`) | left `Some("2026-01-01T00:00:00Z")` right `None`; left `"ab"` right `"a\nb"` |

**Defects the guards found in the code, not mutations:** the first ruled-table pass
scanned every line of the page per grid and every rule per cell border
(`tables: ratio 3.36 (922 -> 3097)`); it is now one pass over the lines with grids
sorted by top and a running maximum of bottoms, and an interval index per grid line.
The page-doubling test came out *sub*-linear (`order: ratio 1.64`) because page 9's
footer and page 10's normalised differently (`#` against `##`); digit runs now
normalise to one `#`. The grid and marker searches also gained the explicit bounds
their binary searches had only implied.

### 18.5 Not built, and why

- **CID-keyed CFF** (`CIDFontType0C`) stays substituted: its code-to-glyph mapping
  is the PDF's CMap, and `GlyphEvidence` carries no glyph id. Wrapping it needs a
  glyph id per glyph in the evidence (`device.rs`/`evidence.rs`, the other lane's):
  **requested field: `GlyphEvidence::glyph: Option<u32>`** (hayro's `glyph_id()`).
- **Type1 hints** are dropped in conversion (outlines exact; hinted small-size
  rasterisation differs). A Type1 matrix other than `[0.001 0 0 0.001 0 0]`, or a
  program whose names miss the AGL, stays substituted.
- **Reflow, not yet:** tables from tagged `Table`/`TR`/`TD` roles (tables come from
  geometry; the roles order them); list nesting from `L`/`LI` (markers are still
  parsed); footnotes continued across pages; note symbols kept (`*`, `†` are
  renumbered); first-page and even/odd header variants; page breaks; justification
  (start, centre and end are inferred); floating pictures (pictures are inline in
  reading order); vector art; XY-Cut++'s label priorities (no layout model, P6);
  `reading_order_ambiguous` is not emitted.
- **Annotations, not yet:** reply threads (`/IRT`) are separate comments, not
  threaded; a `FreeText` note's visible text is not drawn in fixed layout (it is
  the comment's text); links over rotated text.
- **Reach:** no editor command, MCP tool or SDK call invokes `reflow()` (§18.1);
  `docs/109` is not edited by this lane.
