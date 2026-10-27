# 182 — PDF into the model, for editing: fixed-layout frames first, reflow on demand

**Status:** Design, proposed 2026-10-28. Nothing in it is built. **It changes product scope**,
which only the owner can confirm (Q1).
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
