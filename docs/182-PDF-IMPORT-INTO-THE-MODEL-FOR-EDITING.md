# 182 — PDF into the model, for editing: fixed-layout frames first, reflow on demand

**Status:** Design proposed 2026-10-28; owner decision 2026-10-29 started it. P0 and P1 (headless) are built on `feature/pdf-import` (§16), G-1/G-2 in §16a, P2 (the editor) in §17; P3-P6 are not. **It changes product scope**,
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
| G-3 | Text boxes cannot rotate their text or write vertically (**built**, §16b) | `docs/47` (the body-properties "not built" list) | P1 for rotated runs (else reported) |
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

## 19. P4 built: the byte-preserving save

Branch `lane/pdf-p4-save` (from `feature/pdf-import`). Headless: an edited PDF-origin
session saved through the registry with `PreserveWhenSafe` (or `ExactIfUnchanged` once
edited) appends an **incremental update** (ISO 32000-1 §7.5.6) to the original. The editor
reaches it when P2 lands.

### 19.1 Object-level read/write: no new crate

- **Read** with `hayro-syntax` 0.8, the parser import already admits hostile files
  through: raw dictionary bytes (`Dict::data`), page object ids, decoded content, the
  inheritance chain for `/Resources`. A small lexer in `casual-doc-pdf-read/src/content.rs`
  splits content streams and dictionaries into byte ranges.
- **Write** with this repository's own writer (`casual-doc-pdf/src/writer.rs`, ADR-031):
  `Writer::append_to` copies the original as the prefix and numbers objects from its
  `/Size`, `replace_object` re-emits an object under its own number and generation, and
  `finish_incremental` writes a table or a stream to match the original's last section,
  with `/Prev` and `/ID` (first element kept, second changed).
- **Rejected:** `hayro-write` 0.8 extracts pages *into a new* `pdf-writer` document (its
  docs call it internal); it cannot append a revision. `lopdf` (MIT) can, but its manifest
  (0.45, read from the local registry; 0.46 is current) brings about fifteen non-optional
  dependencies (`rand`/`getrandom`, which need a feature flag
  on `wasm32-unknown-unknown`, plus `aes`, `cbc`, `ecb`, `md-5`, `sha2`, `nom`, `rangemap`,
  `stringprep`, `weezl`, …) and a second parser of hostile input next to hayro's. Neither
  buys anything the two in-tree halves do not already do.
- `cargo deny --offline --locked check bans licenses sources`: bans ok, licenses ok,
  sources ok. `Cargo.lock` gains only edges between in-tree crates (and `hayro`, already
  locked, as a `casual-doc-io` dev-dependency for the guards).

### 19.2 Design

**Operator ranges, at import.** `PdfSource::page_evidence` tokenizes the page stream,
checks that hayro's own `UntypedIter` reads the same operator sequence, and interprets an
*instrumented* copy in which show `k` (`Tj`, `TJ`, `'`, `"`) is wrapped as
`/OpenDocShow<k> BMC … EMC`. The device reads the marker back, so every glyph run carries
`show: Option<u32>` (`None` for a form XObject's glyphs), and `PageEvidence` carries the
page `object` and the `shows` count. A stream the two tokenizers disagree about, or one
that already uses the tag, is interpreted as is with `shows: None`. The importer records a
`SourceMap` (`casual-doc-pdf-import`): per page its object and count, per frame its
`TextBox` node, the shows its glyphs came from, and whether every glyph has one.

**Unchanged-ness, at save (G-6).** The adapter stores a `Baseline` in `PdfSourceState`
(the I4 sidecar) at import: from `casual-doc-diff`'s projection, each section's digest (its
blocks' `subtree_hash` plus its `SectionBoundary`), each frame's value hash, a digest of
everything on the page but the frames, a digest of the definitions without `sections`, and
of every story but the body and comments. The save takes the same digests again. No
`Operation` is added (ADR-030 I2). If the whole-document `content_digest` equals the
import's (an edit undone), the original is written back
(`pdf.export.original_written_back`).

**The edited page.** Removed = edited ∪ deleted frames; their shows are dropped by
`content::remove_shows`, which keeps every other operator byte for byte (`'` becomes
`T*`, `aw ac (s) "` becomes `aw Tw ac Tc T*`). Drawn = edited ∪ added frames: the page is
laid out by the same pagination pass the semantic writer uses, its display list is reduced
to those frames' glyph runs (clip and layer brackets kept), and `write_incremental_update`
transcribes it into a Form XObject with its own resources and a `/Matrix` from displayed
page space to the page's user space (the inverse of hayro's `initial_transform`). The page
object is replaced: its other entries raw and in order (so `/Parent`, boxes and `/Annots`
survive), its effective resources inlined with the form added under a name it does not
use, `/Contents [kept overlay]`, `kept` bracketed in `q`/`Q`. An unchanged page, its
content and its resources get no object in the update.

**Fallback, reported.** Anything not provably safe is saved semantically with
`pdf.export.preserve_fallback.<reason>` (`Degraded`, `NotRetained`): `definitions_changed`,
`stories_changed`, `page_count_changed`, `pagination_changed`, `page_objects_changed` (a
picture, shape, anchor or section setting), `page_operators_unnumbered`,
`frame_text_not_removable` (glyphs from a form XObject), `operator_shared_with_kept_frame`
(one `TJ` paints two frames), `kept_text_would_move` (a kept show follows a removed one on
the same line with no `Td`/`TD`/`Tm`/`T*` between: the text matrix advances by what a show
draws), `clipping_text` (render mode 4–7), `content_unbalanced`, `content_untokenizable`,
`frame_not_text_only` (numbering, a style, a drop cap, fill, border, link, or a non-text
inline), `page_size_changed`, `page_object_moved`, `xref_unreadable`,
`no_retained_source`.

**Reported on success:** `pdf.export.incremental_update` (pages rewritten),
`pdf.export.page_carried_over` (`Preserved`), `pdf.export.earlier_revision_retained` (the
replaced text is still in the file, in the earlier revision: an incremental update is not a
redaction), `pdf.export.signature_covers_earlier_revision` when `/AcroForm /SigFlags` has
bit 1, `pdf.export.certification_invalidated` (`Degraded`) when the catalog has `/Perms`,
and the writer's own font findings for the overlay.

Complexity: O(document) at save (one projection, one pagination, the prefix copy), never
per edit; the import-side numbering is O(page stream bytes).

### 19.3 Guards, each driven red by a mutation (SKILL §4)

All in `crates/casual-doc-io/tests/pdf_preserve.rs` unless named. The file is re-opened and
rendered with hayro, and `xref_chain` walks every `/Prev` and checks each in-use entry
points at its own `n g obj` header, because hayro repairs a broken chain silently.

| Guard | Mutation | Red output |
| --- | --- | --- |
| `an_edited_frame_is_appended_as_an_update_and_the_unchanged_page_is_carried_over`: the original is the prefix; objects 3, 5, 6 are not in the update; page 2's parts are equal; page 2 renders identically; page 1 has the edit and not the replaced text | treat every page as edited | `left: Some(2) right: Some(1)` (the count) |
| same, with the counts also mutated to stay 1 | as above | `6 0 obj rewritten` |
| same: the replaced text is gone from the new revision | `remove_shows` keeps every show | the page text still holds `The quick brown fox …` |
| same: the chain resolves | `/Prev` off by one | `the section at 1820 is neither a table nor a stream` |
| `a_signed_file_keeps_its_signed_revision_intact_when_a_page_is_edited` (a generated file with a `/Sig` dictionary, a `/ByteRange` over the whole signing revision, and a digest of those bytes as its dummy `/Contents`; table and xref-stream variants) | the update drops the original's last byte | `assertion failed: bytes.starts_with(&signed)` |
| same: the update's entries point at their objects | a replaced object's offset + 1 | `object 4 points at 3920, not at 4 0 obj` |
| `what_cannot_be_preserved_falls_back_to_the_semantic_writer_and_says_why`: a deleted picture | skip the non-text digest comparison | `left: [] right: ["…preserve_fallback.page_objects_changed"]` |
| same: a frame given a fill | drop the fill check from `text_only` | `left: [] right: ["…frame_not_text_only"]` |
| `a_deleted_frame_loses_its_text_and_an_added_one_is_drawn` | treat a deleted frame as unchanged | the page text still begins `Synthetic prose` |
| `an_operator_that_also_paints_a_kept_frame_or_moves_one_is_not_removed` | skip the shared-operator check | `left: [] right: ["…operator_shared_with_kept_frame"]` |
| `pdf-read` `content::tests::a_kept_show_after_a_removed_one_on_the_same_line_is_refused` | never set the moved flag | `left: Ok([…]) right: Err(KeptShowWouldMove)` |
| `pdf-read` `every_page_stream_glyph_run_names_its_show_operator` (every corpus page) | the device ignores the markers | `synthetic-active-content.pdf page 0: a run without a show` |

`an_undone_edit_writes_the_original_back_and_semantic_mode_regenerates` and the
`content`/`revision` unit tests (tokenizer agreement with hayro on strings, arrays, inline
images and marked content; a forged tag refused; dictionary splitting; the tail) pin the
rest. The P1 oracle (`every_page_matches_hayros_render_of_the_original`) stays green on the
instrumented interpretation.

### 19.4 Not built, and why

- **Pictures and shapes are not edited in place.** Only text operators carry ranges, so a
  changed or deleted picture or shape on a page falls back. Path and image ranges are the
  same mechanism (wrap `Do`, `BI` and painting operators) when needed.
- **The overlay draws in the layout's faces**, as the semantic writer does (Liberation for
  an unembedded Times, for example), not in the PDF's embedded subset; unedited frames keep
  their original glyphs.
- **Text inside form XObjects** is not removable; a frame whose glyphs come from one falls
  back.
- **The replaced text stays recoverable** in the earlier revision. Reported; a redacting
  save is a full rewrite, not an update.
- **A file hayro had to repair** is not detected when its last `startxref` still parses;
  the update then chains to a broken section, which readers repair as they did before.
- **Hybrid-reference files** (`/XRefStm`) get a table update without restating `/XRefStm`.
- **Not reachable from the editor** until P2; the registry path is.

## 16b. Engine gaps built

Branch `lane/pdf-engine-gaps`. The engine gaps P1 and §16a left open, in the
model, layout, render, GPU and DOCX crates, and the importer change that uses
them.

**G-3, turned text.** Three DrawingML sources turn a text box's text, and all three
are modelled, laid out, painted, hit-tested and round-tripped:

- `wps:bodyPr@vert` → `TextBoxBodyProperties::vertical_text` (`TextBoxVerticalText`,
  all seven `ST_TextVerticalType` tokens retained). `vert`, `eaVert` and
  `mongolianVert` lay out as a quarter turn clockwise, `vert270` as three; the lines
  wrap at the frame's HEIGHT (the content is flowed in the frame with width and
  height exchanged, insets turned so each stays on its physical side).
- `wps:bodyPr@rot` → `TextBoxBodyProperties::text_rotation` (60000ths, the text
  alone, about the frame centre; Word writes `rot="0"` on every text box).
- a group child's `a:xfrm@rot` (`GroupTextBox::rotation`, modelled before but painted
  upright) now turns the frame's chrome and its text together, at any angle. Flips
  turn the outline only; the words stay readable, as in Word.

`TextBox` has no shape-rotation field and adding one would break the struct
literals in `casual-doc-wasm` (not this lane's). So a lone floating text box whose
`a:xfrm` is rotated or flipped is imported as a **group of one**, the route a lone
non-rectangular text box already took; its rotation was previously dropped in
silence. An inline one is reported (`rot`).

- **Layout.** `AnchorContent::TextBox` carries `orientation: TextOrientation`
  (`page.rs`): the total clockwise turn and whether the content box is exchanged.
  `TextOrientation::content_box` / `transform` / `to_content` are the one
  definition compose and hit-test share. `hit_test_text_box` maps a point into a
  turned box's upright space and resolves against that box's lines only, topmost
  first; exact at quarter turns, nearest twip otherwise.
- **Render.** `compose_anchor` wraps the text in one `PaintItem::PushLayer` about the
  frame centre — the seam the layer was introduced to wait for — and a turned
  frame's chrome in another (its border is a flat `Rect`, which carries no
  transform). tiny-skia, vello and the PDF writer all already honour layers; no
  renderer changed for G-3.
- **DOCX.** `rot` and `vert` are written on `wps:bodyPr` when not default.
- **Importer.** A rotated run is turned back upright about the page centre, grouped
  with the other runs at its angle (so every line of a `/Rotate` page groups as
  usual), and each block becomes a one-child group whose `GroupTextBox` is turned by
  the angle, placed so that turning it about its own centre puts the text where the
  PDF drew it. `pdf.import.text_rotated` is now `Mapped`, not a loss. A wide page's
  section says `Landscape`. The oracle no longer exempts either rotated page.

**GPU even-odd.** `SparseCanvas::set_fill_rule`, forwarded to `vello_cpu` and
`vello_hybrid`; the encoder sets `EvenOdd` around an even-odd path's fill only and
restores `NonZero` straight after (clip layers stay non-zero). §16a's "Not yet" is
closed.

**The DOCX writer's silent picture loss.** Measured, missing bytes WERE reported
(`docx.export.media.missing_bytes`, tested). The silence §16.3 found was the
relationship target: `media_target` stripped a `word/` prefix and otherwise wrote the
part name unchanged, so a picture at `media/x.png` was addressed, from `word/`, as
`word/media/x.png` — a dangling relationship. `word_relative_target` (a part outside
`word/` is `../<name>`) now serves every `word/` part's relationships: pictures,
embedded font faces and embedded objects (the family). A part name no relationship
can address, or one that would shadow a generated part (any `.xml` binary part:
`PartList::push` keeps the first of two parts under one name in silence), is
reported `docx.export.media.invalid_part_name` / `docx.export.embedded_font.invalid_part_name`
and left out whole. Font programs already take generated names.

**Guards, each driven red by a mutation:**

| Guard | Mutation | Red output |
| --- | --- | --- |
| `render-gpu/tests/even_odd_fill.rs` (vello_cpu always; vello_hybrid on a real adapter, skips without one) | drop `set_fill_rule(EvenOdd)` | both painters: `under even-odd the same-wound inner square is a hole`, left `(255, 0, 0)` right `(255, 255, 255)` |
| same | drop the restore to `NonZero` | both: `the even-odd rule is restored to non-zero after its fill` |
| `pdf/tests/even_odd_operators.rs` (operators read back with `inspect`) | `even_odd()` returns false | left `["f"]` right `["f*"]`; left `["W"]` right `["W*"]` |
| `export/tests/export_report.rs` `a_picture_stored_outside_word_is_written_where_its_relationship_points` | the old target rule | `word/_rels/document.xml.rels declares rId2 -> word/media/pdf-image1.png, which the package does not contain` |
| `…a_face_and_a_chart_outside_word_stay_addressable` | the old rule; then the old rule for embedded objects alone | `declares rIdF1 -> word/fonts/font1.odttf`; `declares rIdChart1 -> word/charts/chart1.xml` |
| `…a_picture_whose_part_name_cannot_be_written_is_reported` (six names) | `is_writable_part_name` always true | the report entry is missing |
| `export/tests/turned_text_box_round_trip.rs` (Word-shaped markup → model → DOCX → model) | importer ignores `bodyPr@rot` | left `[(0, Vertical270, None), …]` right `[(5400000, Vertical270, None), …]` |
| same | lone rotated box not grouped | `(0, Horizontal, None)` for `(0, Horizontal, Some(1800000))` |
| same | writer drops `rot` | `the body's rotation and direction are written` |
| `layout/tests/turned_text_box.rs` `vertical_lines_wrap_at_the_frames_height…` | never exchange the content box | `the line runs the frame's 3 in height`, left 2 right 1 |
| `…turned_text_is_composed_inside_one_layer…` | no text layer | `a quarter-turn layer about the frame centre` not found |
| `…a_click_on_turned_text_resolves_to_the_character_drawn_there` (90°, 270°, 180°, and 30° in the group test) | `to_content` identity | `a hit in the box` (none), `30 degrees: line start` |
| `…a_grouped_text_box_turns_its_frame_and_text_together` | group child flowed with rotation 0 | `the text turns with its shape`, left rotation 0 right 1800000 |
| `render/tests/turned_text_box.rs` (pixels: ink taller than wide, inside the frame, on the side the line's top faces; upright control) | no text layer | `Vertical: the line runs down the page, ink (0, 245, 55, 255)` |
| `pdf-import/tests/oracle.rs` `rotated_text_pages_match_the_oracle` (rotated-run page 0.963 / 1.000, `/Rotate 90` page 1.000 / 1.000) | importer drops the child's rotation | `synthetic-rotated-text.pdf page 1: recall 0.507 precision 0.534`; page-boxes page 2 `recall 0.071` |

`the_comparison_sees_a_reported_visual_loss` now shows the comparison can fail on
the one remaining exemption (Type3: recall 0.767, precision 0.742).

**Not built, and why:**

- An INLINE text box's `vert`/`rot` (it flows inside a line; the same orientation
  would have to ride `InlineTextBox`), the stacked `wordArtVert` readings, `eaVert`'s
  upright East Asian glyphs, and VML `layout-flow` (`docs/47`).
- Caret and selection GEOMETRY inside a turned box is still reported in the box's
  upright space (`text_box_line_boxes`); a host drawing a caret there must apply the
  box's `TextOrientation::transform`. Hit-testing is turned.
- The CPU raster composites each layer through a page-sized offscreen pixmap, so a
  page of N turned frames costs N page-sized composites (the GPU backend and the
  PDF writer transform vectors and pay nothing extra). Bounding the offscreen by the
  layer's content is the follow-up if a measured page needs it.
- The webapp's object inspector does not expose `rot`/`vert` (`webapp/**` is another
  lane's).

## 17. P2 built: the editor

Branch `lane/pdf-p2-editor` (from `feature/pdf-import` at `502ff112`). Owner, 2026-10-29:
complete the roadmap; this is P2.

### 17.1 What is built

- **Wasm (`crates/casual-doc-wasm`).** Feature `pdf-import` (on in `webapp/build.sh`) compiles
  `casual-doc-io/pdf-import`. `src/pdf_import.rs` is the staged binding over
  `PdfStagedImport`: `beginPdfImport(bytes)` (admission, O(file)), `step(pages)` (O(pages),
  returns `pending`/`done`/`cancelled`/`failed`), `pageCount`/`pagesDone`, `cancel()`, and
  `finish()`, which consumes the job and is **atomic**: it validates the model and builds the
  session through `session_from_import`, the same function every other open now ends in
  (split out of `open_document_bounded`, unchanged otherwise). The binding exists without
  the feature and refuses with a sentence, so the webapp's import always links. The
  synchronous `open` still refuses a PDF (the built-in registry stays export-only), so a host
  cannot import a PDF on the main thread by accident. `casual-doc-io` gained one accessor,
  `PdfStagedImport::page_count`, so the bar is determinate before the first page.
- **Save.** `exportAs("application.pdf", "exact_if_unchanged")` of an unchanged session
  (revision 0) opened from a PDF returns the retained original bytes (MF-I4); edited, or
  asked for `semantic`, it is the real-text writer (`docs/98`). DOCX is the semantic writer:
  frames are text boxes.
- **Webapp.** File → Open, the drop target and the host's open all reach `openBytes`, which
  routes on the `%PDF-` header (the bytes decide, as detection does for every format):
  `editor_file_pick.mjs` accepts `.pdf`, the Open picker's `accept=` lists `.pdf` and
  `application/pdf`, `format_io.mjs` names PDF both ways. `pdf_import_scheduler.mjs` (pure)
  runs the job in `requestIdleCallback` slices of at most 40 ms, a page at a time, predicting
  the next page from the slowest seen and always doing at least one page; Safari falls back
  to zero-delay tasks. `pdf_import.mjs` is the modal progress dialog (`registerModal`):
  title, a `role="progressbar"` with value and value text, a polite live region for the
  phases only, Cancel focused; Escape, the backdrop and the close button are all Cancel.
  Nothing replaces the open document until `finish` succeeds; Cancel leaves it as it was
  and says so. An encrypted PDF is refused in words (`pdfImport.encrypted`).
- **Summary.** After the pages are on screen the findings dialog opens once
  (`compat_findings.mjs`), led by the disclosure: converted on this device, nothing
  uploaded; a reconstruction of the pages as frames, pictures and shapes, not the PDF; the
  original untouched, written back byte for byte when saved as PDF before an edit. The
  `pdf.import.*` findings are grouped by what the import did (Not brought in / Rebuilt
  approximately / Inferred from the page layout), from the model outcome, because the
  adapter reports every loss as `not_retained` and "Not kept" would merge an inferred
  reading order with a dropped form. All 24 families have words (`findings_catalogue.mjs`),
  located by first page. Every string is in `en_strings.mjs` and the 18 catalogues.
- **Scope.** SKILL §1 now lists PDF (import and edit) under In, and PDF forms, signatures,
  annotations and scripts under Out; `docs/98`'s import non-goal points here.
- **Not needed:** a disabled "Convert to flowing document" command. No surface offers one
  yet, and the command-surface rule (SKILL §10) is about a control that exists; P3 adds the
  command with its surfaces.

### 17.2 Guards, each driven red by a mutation (SKILL §4)

| Guard | Mutation | Red output |
| --- | --- | --- |
| wasm `pdf_import::tests::an_unchanged_pdf_saves_back_byte_for_byte_and_an_edited_one_is_regenerated` | never take the retained bytes (`.filter(\|_\| false)`) | `an unchanged session is the original file (76502 bytes written, 2024 read)` |
| wasm `a_cancelled_import_produces_no_document` | `cancel()` does nothing | `left: "pending"`, `right: "cancelled"` |
| `pdf_import_scheduler.test.mjs` "no slice overruns its budget" | drop the per-slice budget check | `a slice took 700 ms against 40` |
| same, "Cancel between slices…" | never consult `cancelled()` | `+ 'done'` / `- 'cancelled'` |
| `compat_findings.test.mjs` "a PDF import finding is grouped by what the import did" | drop the `pdf.import.` branch | `+ 'lost'` / `- 'pdfOmitted'` |
| `findings_catalogue.test.mjs` "every PDF import finding … is catalogued" (ids read from `report.rs`) | delete the `vector_split` entry | `a PDF finding the dialog would print as its id` |
| e2e `pdf-import.spec.mjs` main-thread test: 275-page PDF made by the editor, no long task before the finish mark over 200 ms | one slice with an unbounded budget | `importing 275 pages blocked the main thread for 543 ms` (green: no long task at all before the finish) |
| e2e Cancel test | the scheduler never sees Cancel | `Expected: "Opening synthetic-prose.pdf was cancelled — the document you had open is still here"`, `Received: ""` |
| e2e open test (progress, pages, summary, edit a frame, save PDF unchanged = original bytes, edited PDF and DOCX reopen holding the edit) | drop the disclosure section | `[data-kind="pdfDisclosure"]` not found |

`modal-roster.mjs` carries the dialog (held open by an idle-callback gate in
`pdf-import-helpers.mjs`), so `dialog-contract`, `dialog-fit` and `phone-dialog-sheet` hold it
to the modal contract; all pass.

**Measured, not guarded:** the finish (validation and layout) is one task, 650 ms and 580 ms
on the 275-page PDF, 470-500 ms at 118 pages: the same O(document) open every format pays
(`docs/116` §7), now after the import rather than inside it.

### 17.3 Wasm size: over both ceilings, for the owner

Measured with `build.sh`'s settings (wasm-pack 0.14.0, Binaryen `-Oz`, release `z`):

| Build | Total | Code | Data | gzip -9 |
| --- | ---: | ---: | ---: | ---: |
| `502ff112` (base) | 26,992,235 | 13,009,766 | 13,935,168 | 12,243,545 |
| this branch, `pdf-import` off | 26,993,325 | 13,010,547 | 13,935,262 | 12,244,060 |
| this branch, `pdf-import` on (shipped) | 29,827,011 | 14,739,317 | 15,036,004 | 13,539,863 |
| **delta (on vs base)** | **+2,834,776 (+10.5%)** | **+1,729,551** | **+1,100,836** | **+1,296,318** |

`wasm_size.test.mjs` fails on this branch (27,010,000 / 13,035,000) and its ceilings are
**not** raised here. Of the data, about 576 KB is hayro-interpret's default features:
`embed-fonts` (the Foxit standard-14 `.pfb` faces, 320 KB) and `embed-cmaps` (256 KB).
Options:

1. **Ship it in the main module** and raise both ceilings by the measured delta plus the
   usual ~128 KiB headroom (29,960,000 / 14,870,000).
2. **Trim the reader first** (the reader lane's crate): `hayro-interpret` with
   `default-features = false`, standard-14 fonts resolved to the metric-compatible
   Liberation faces the layout crate already ships, predefined CMaps fetched on demand.
   About -0.6 MB; the rest is the reader and importer code.
3. **A lazy second module**: a small `casual-doc-pdf-wasm` (reader, importer, model, serde)
   loaded on the first PDF, handing the main module a normalized-JSON snapshot package
   plus the original bytes; the main module stays at the base size. Costs: a second copy of
   the model and serde code in that module, a new crate, and the exact-bytes save moving to
   the host (it keeps the original and writes it while the revision is 0).
4. **Feature off by default** with a PDF-capable build variant: the off build measures +1,090 B
   against the base, so it fits; the shipped editor would not open PDFs.

### 17.4 Not built

- The finish and layout are one main-thread task (above); splitting them is `docs/116` §7's
  lazy measure pass, not this phase. The worker (ADR-073) is where the whole job goes.
- Compare (`#compareFile`) does not accept PDF: its diff job imports both sides through the
  synchronous registry, which stays export-only for PDF. `docs/182` P2's "both `accept=`
  lists" is corrected here: only the Open picker opens documents.
- A restored draft or version of a PDF-origin document does not re-show the summary.
- Reflow (P3), preserving save (P4), OCR (P5), ML layout (P6).

## 20. P5/P6 built: OCR and optional ML layout (headless)

Branch `lane/pdf-p5-ocr`. Headless only, like §16: the editor (`webapp/**`,
`casual-doc-wasm`) and the `casual-doc-io` adapter do not pass a provider yet, so
every surface today gets the no-model path (§20.3). Reflow (P3) is what consumes the
P6 regions.

### 20.1 Decisions, licences and sources

**One Rust OCR path, on `rten`.** The same choice as §3.1 for hayro: one engine for
browser, desktop, MCP and CI, deterministic per target, under `cargo deny`, no npm
runtime dependency. `docs/131` §11's PaddleOCR.js/Tesseract.js-in-a-Worker plan is
superseded for the runtime. `rten` 0.26 (Robert Knight; MIT OR Apache-2.0, pure Rust)
runs ONNX graphs directly and builds for `wasm32-unknown-unknown` (single-threaded
there).

**Owner decision (2026-10-29): PP-OCRv5 is the default provider; `ocrs` stays as the
fallback.** The first implementation ran `ocrs`; its weights are why it is not the
default:

| | PP-OCRv5 mobile (default, feature `paddle`) | `ocrs` 0.13.1 (fallback, feature `ocrs`) |
| --- | --- | --- |
| Engine code | ours (`casual-doc-ocr/src/paddle.rs`) on `rten` | `ocrs` (MIT OR Apache-2.0) on `rten` |
| Weights licence | **Apache-2.0**, model cards at `huggingface.co/PaddlePaddle/PP-OCRv5_mobile_det`, `…/PP-OCRv5_mobile_rec`, `…/latin_PP-OCRv5_mobile_rec` (and the other `*_PP-OCRv5_mobile_rec` packs) | **CC-BY-SA-4.0** for the published checkpoints (`huggingface.co/robertknight/ocrs`, card `license: cc-by-sa-4.0`); the current weights `ocrs-cli` downloads (`ocrs-models.s3-accelerate.amazonaws.com`) carry no licence statement; the training repository has no licence file |
| Format the host supplies | ONNX, converted from PaddlePaddle's release with PaddlePaddle's `paddle2onnx` (2.1.0) | `.rten` or ONNX |
| Languages | multilingual pack (Chinese, English, Japanese, 18,383 entries) and per-script packs; several at once | Latin alphabet (96 symbols) |
| Prose fixture scanned at 200 dpi, character similarity with the native text | **1.0000** (one or two packs) | 0.9885 |
| Rendered CJK sample (STHeiti, 36 px; one-off, not committed) | exact, word confidence ≥ 0.998 | cannot read it |
| Confidence | per character from the softmax | per character from the log-softmax (`ocrs` exposes none; decoded here) |

Neither weight set is ever committed, bundled or fetched by a library: the host passes
bytes (a file, a cache, its own `fetch` with an integrity hash). `ocrs` weights are not
on §11's never-list (CC-BY-SA permits commercial use; share-alike binds the weights,
not the code using them), but they are not permissive, which is why they are only the
fallback. **P6 layout:** `docling-layout-heron-onnx` (`huggingface.co/docling-project/docling-layout-heron-onnx`,
card `license: apache-2.0`, an RT-DETRv2 export of `docling-layout-heron`, 17 labels).
PP-DocLayout is Apache-2.0 too but published only in Paddle format; it is not wired.

**`cargo deny --locked check licenses bans sources`: bans ok, licenses ok, sources ok.**
New crates: `ocrs`, `rten` family (`rten`, `rten-tensor`, `rten-imageproc`, `rten-onnx`,
`rten-model-file`, …), all MIT OR Apache-2.0; `sha2` was already in the tree.

### 20.2 `rten` runs the PP-OCRv5 graphs (verified, not assumed)

The three converted graphs (`det.onnx` 4.8 MB, `rec.onnx` 16.6 MB, `latin_rec.onnx`
8.1 MB) load and run in `rten` 0.26 with no unsupported operator. Numerics against
onnxruntime 1.19.2 on the same inputs (a rendered text page, BGR-normalised as
PaddleOCR does; a rendered text line):

| Graph | Output | Largest absolute difference |
| --- | --- | --- |
| `det.onnx` (DB) | `[1, 1, 480, 640]` probability map | 1.4e-4 |
| `rec.onnx` (multilingual) | `[1, 60, 18385]` class probabilities | 7.2e-6 |
| `latin_rec.onnx` | `[1, 60, 838]` | 3.0e-6 |

The heron graph also runs in `rten` (inputs `images: u8 [n, 3, 640, 640]`,
`orig_target_sizes`; outputs `labels`, `boxes`, `scores`), about 0.45 s per page in a
release build on an Apple M-series CPU. Its regions on `synthetic-prose.pdf` page 1:
`SectionHeader` 0.92 over the heading, `Text` 0.98 and 0.97 over the two paragraphs.

The pipeline around the graphs mirrors PaddleOCR with the models' `inference.yml`
parameters fixed in code (DB `thresh` 0.3, `box_thresh` 0.6, `unclip_ratio` 1.5,
`min_size` 3; recognition height 48, width ≥ 320, greedy CTC). **Differences from
PaddleOCR, stated:** candidate boxes are axis-aligned (PaddleOCR uses rotated minimum
rectangles and a perspective crop), the box score is the component's mean probability
(PaddleOCR: the mean inside its polygon), and the long side is capped at 4,000 px
rather than resized to 960 (the scan reads better at full resolution). Not built: the
text-line orientation classifier (an upside-down line reads upside down) and deskew.

### 20.3 What is built

- **`casual-doc-ocr`** (new). Provider traits and owned evidence, no PDF knowledge
  (PDFR-I1, I5): `OcrProvider::recognize(&Raster, &OcrLimits, &dyn Interrupt) -> OcrPage`
  (lines of words with pixel boxes and confidences), `LayoutProvider::regions`,
  `ProviderManifest` (provider, version, SHA-256 of every model and dictionary,
  decoding, declared determinism). Providers: `PaddleOcr` (feature `paddle`, the
  default, `RecognitionPack`s = model + dictionary, several at once: each line is read
  by each pack and the most confident reading kept), `OcrsProvider` (feature `ocrs`),
  `HeronLayout` (feature `layout-heron`), `HeuristicLayout` (always; title, headings,
  page furniture, text, figures; no tables or lists, which PDFR-I4 forbids guessing
  from geometry). A line's box is the ink's vertical extent inside the detected box
  (`Raster::ink_rows`), so its height means the same thing for every detector.
- **`casual-doc-pdf-import::ocr`** (new) and its wiring (`ImportJob::begin_with_ocr`,
  `import_pdf_with_ocr`, `PdfImport::{ocr, layout}`):
  1. *Classify* each page from native evidence alone: regions are pictures that valid
     native glyphs cover less than 1% of, and the glyphs of runs with no Unicode;
     `Native` (none: never rasterised, PDFR-I2), `Raster`, `Hybrid`.
  2. *Rasterise selectively*: each region alone through `hayro::render_into` with a
     transform mapping it to the raster origin, at 150 dpi (unmapped runs at ≥ 40 px
     per em), white background.
  3. *Recognise* under per-raster `OcrLimits` and import-wide `max_pages` and
     `max_total_pixels`, polling the host's `Interrupt` between pages, regions and
     lines.
  4. *Merge native-first* (`docs/131` §10.2): a word over valid native glyphs is
     dropped and, when the readings differ by more than a third of their letters,
     counted as `native_ocr_conflict`; a word over unmapped glyphs only replaces them
     (they are still counted as `native_text_invalid`, the producer's defect); a word
     over raster is kept. Under 0.8 confidence: kept and counted
     (`ocr_low_confidence`); under 0.3: dropped and counted. Lines split by the
     detector on one row are joined.
  5. *Build*: each optical line becomes an ordinary glyph run (characters spread over
     each word's box, em = ink height × 1.1) under a font index no PDF font has, after
     the page's own paint, so the fixed-layout builder frames it like native text, above
     the retained picture. `OcrTextPlacement::Hidden` makes the runs hidden instead
     (a searchable-scan text layer).
  6. *Layout (P6)*: with a `LayoutProvider`, each merged page's regions land in
     `PdfImport::layout` (heron gets a 96 dpi page raster). The fixed-layout document
     does not depend on them.
- **With no provider** every region is reported `pdf.import.ocr_unavailable` and the
  import succeeds: pictures stay pictures, unmapped runs stay U+FFFD. New families:
  `native_ocr_conflict`, `ocr_applied`, `ocr_limit_reached`, `ocr_low_confidence`,
  `ocr_unavailable`.
- **Bounds (PDFR-I7).** Pixels per raster are refused before any model runs; a region
  that would have to drop under 100 dpi to fit is skipped and reported; lines and
  characters per raster, pages and pixels per import, and the host's time budget stop
  OCR and report the rest (`ocr_limit_reached`) without failing the import; Cancel
  abandons it (`Progress::Cancelled`, no document, PDFR-I3).
- **Determinism (PDFR-I8), declared:** `Determinism::SameTarget` for the model
  providers (identical raster, model bytes, limits and build target give identical
  evidence; x86-64, aarch64 and wasm32 may round differently and are not claimed equal),
  `Exact` for the heuristic. Decoding is greedy, ties go to the lower label, regions and
  rows are sorted with total tie-breaks. Tested: two imports of the same scan with the
  same model give equal documents, reports and summaries, for each provider.
- **Tests.** `crates/casual-doc-pdf-import/tests/ocr.rs`. Most use a scripted provider
  (fixed words at fixed positions), so the merge, budget and cancel rules run in CI
  without a model. The model tests read `OPENDOC_PADDLE_MODELS`, `OPENDOC_OCR_MODELS`
  and `OPENDOC_LAYOUT_MODEL`, run every provider whose files are present and print why
  when none is; CI sets none of them and needs no network.
  `crates/casual-doc-ocr/fetch-test-models.sh` downloads the pinned weights (SHA-256),
  converts PP-OCRv5 (needs a Python with `paddle2onnx`; the conversion reproduced
  byte-identical ONNX here) and extracts the dictionaries with YAML's `BaseLoader`
  (`safe_load` would coerce entries).
- **Checks run:** `cargo +1.96.0 fmt --all --check`; clippy `-D warnings` on both
  crates with all features and with none; the tests of both crates; `cargo check` of
  `casual-doc-ocr` (default and all features) and of `casual-doc-pdf-import` (all
  features) for `wasm32-unknown-unknown`; `cargo deny` as in §20.1.

### 20.4 Measured

| Measured | Value |
| --- | --- |
| Prose page 1 scanned at 200 dpi, imported at 150 dpi: character similarity with the native text (reading order, whitespace ignored); the test's stated threshold is 0.95 | PP-OCRv5 1.0000, two packs 1.0000, `ocrs` 0.9885 |
| Frames on that page | 3, as natively; first-run sizes 19/10.5/10.5 pt against native 18/11/11 |
| Ink height per em on that page | 0.97 (Helvetica Bold heading), 0.86 (Times body) → `EM_PER_LINE_BOX` 1.1 |
| `ocrs` on the same page at 100/150/200/300 dpi | best at 150; 300 reads worse (the detector resizes its input to a fixed size), hence the 150 dpi default |
| `ocrs` with the older Hugging Face checkpoints | noise (similarity 0.41): they load but do not match `ocrs` 0.13's preprocessing |

### 20.5 Guards, each driven red by a mutation (SKILL §4)

| Guard (`tests/ocr.rs` unless noted) | Mutation | Red output |
| --- | --- | --- |
| `native_text_is_never_overwritten_by_ocr` | skip the valid-native-glyph check in the merge | `OCR must not overwrite native text` (ocr.rs:359) |
| `with_no_model_a_scan_reports_ocr_unavailable_and_still_imports` | record `ocr_limit_reached` instead of `ocr_unavailable` | `OCR unavailable is reported, not skipped silently` (ocr.rs:292) |
| `cancel_during_ocr_abandons_the_import_and_returns_nothing` | treat a provider's `Cancelled` as a skipped region | `cancel lands inside the page`: left `Pending { page: 1, of: 3 }`, right `Cancelled` |
| `the_page_budget_stops_ocr_and_reports_the_rest` | `page_budget = true` | provider calls left 3, right 1 |
| `the_pixel_budgets_stop_ocr_before_rendering` | drop the import-wide pixel clause | provider calls left 2, right 1 |
| `a_spent_time_budget_stops_ocr_and_the_import_still_succeeds` | ignore `Stop::OutOfTime` | provider calls left 3, right 0 |
| `low_confidence_words_are_kept_and_reported_and_the_floor_drops_them` | keep words under the floor | `under the floor: dropped` (ocr.rs:434) |
| `ocr_replaces_glyphs_whose_unicode_is_unrecoverable_and_nothing_else` | never record the replaced glyphs | `the unmapped glyphs are replaced` (ocr.rs:404) |
| `a_born_digital_page_is_native_and_never_rasterised` | add a whole-page region to every page | classes left `[Hybrid, Hybrid]`, right `[Native, Native]` |
| `a_scanned_page_imports_with_its_text_when_a_model_is_supplied` (models present) | `EM_PER_LINE_BOX` 1.5 | `pp-ocrv5: size 52 vs native 36` |
| same | PP-OCRv5 line height from the padded detector box, not the ink | `pp-ocrv5: size 31 vs native 22` |
| `paddle.rs` `a_component_under_the_box_threshold_is_dropped_and_one_over_it_unclipped` | box threshold 0 | left 2, right 1 |

**Not guarded, stated:** a wrong channel order or input scale in PP-OCRv5's detector
preprocessing (RGB for BGR, values halved) left the end-to-end test green: the scan
is grey, so channel order is invisible, and the model tolerated the scale. The
preprocessing was checked once against onnxruntime (§20.2); a committed
reference-output test needs a reference vector produced outside Rust and is not built.
Determinism has no mutation that breaks it on one target without also breaking
correctness; the two-run equality test is the evidence.

### 20.6 Not built, and why

- **Host wiring.** `casual-doc-io`'s `PdfImportAdapter`, `opendoc-mcp`, the viewer and
  the editor (`webapp/**`, `casual-doc-wasm`) still call `ImportJob::begin`, so they
  report `ocr_unavailable` and never run OCR. Each needs a way for the host to supply
  model bytes (a path setting headless; Cache Storage/OPFS plus a download prompt that
  shows the size, `131` §12.3, in the browser). Not this lane's files.
- **No CJK fixture is committed** (rendering one needs a system font); CJK recognition
  is the one-off measurement in §20.1. Optical CJK frames are drawn with the builder's
  default face, so they inherit FONT-01/02/03 (G-5).
- **PP-OCRv5:** orientation classifier, deskew, rotated boxes (§20.2).
- **Erasing the picture behind visible OCR text** (Acrobat's "editable text"): the
  scan's ink shows through where the substitute face differs.
- **Partially mapped words:** a word over both valid and unmapped glyphs keeps the
  native glyphs and leaves the unmapped ones U+FFFD.
- **P6:** heron's regions are evidence only until P3 reflow consumes them;
  PP-DocLayout is not wired (Paddle format only).
- **Determinism across targets** (x86-64, aarch64, wasm32) is declared, not tested.
- **The time budget in the browser** needs the host's `Interrupt` over
  `performance.now()`; `Deadline` is native-only.

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
