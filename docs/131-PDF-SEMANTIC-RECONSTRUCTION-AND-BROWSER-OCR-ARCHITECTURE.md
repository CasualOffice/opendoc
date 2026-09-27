# 131 — PDF Semantic Reconstruction and Browser OCR Architecture (Experimental)

> [!IMPORTANT]
> This is research and architecture for a possible future feature. PDF import is not
> implemented, scheduled, supported, or included in the current v1 commitment. The
> shipped PDF capability remains export-only.

**Status:** Experimental future-feature architecture; PDF import is not implemented,
not supported, and not part of the current v1 commitment. No implementation is
authorized by this document.
**Date:** 2026-09-27
**Tracker:** PDFR-000
**Proposed ADR:** ADR-034
**Depends on:** docs 14, 15, 18, 21, 35, 56, 83, 94, 98, 111, 113, and
116; `casual-doc-io`; `casual-doc-wasm`; the schema-v1 model and semantic DOCX
writer.

## 1. Decision summary

If the experimental feature is later accepted, add PDF input as **local semantic
reconstruction**, not as a direct PDF-to-DOCX converter and not as a new editing
model.

The browser receives local PDF bytes, extracts native PDF evidence first, invokes
OCR only for raster-only or uncertain regions, and sends a bounded, versioned
evidence stream to the OpenDoc Rust/WASM core. The core validates that evidence,
reconstructs a normalized `v1::Document`, produces a `SourceEnvelope` and
compatibility report, and only then creates an editable session. DOCX output uses
the existing semantic DOCX writer.

Any future graduated release baseline has **no document-processing server**. Document
bytes, page rasters, extracted text, OCR results, and the normalized model remain on
the user device. Static runtime and model assets may be downloaded from a
host-controlled, allowlisted origin and cached for offline reuse; that is asset
delivery, not remote document processing.

The proposed first production profile, if the experiment graduates, is:

1. native extraction for born-digital PDFs;
2. selective printed-text OCR using a small browser-ready model in a Worker;
3. deterministic Rust/WASM reconstruction;
4. explicit loss and inference reporting;
5. semantic export through existing OpenDoc format writers.

Large end-to-end document VLMs, including UnlimitedOCR, are evaluation candidates,
not baseline dependencies. No model becomes a supported provider until its browser
runtime, licence, resource, determinism, security, and corpus gates pass.

## 2. Problem and user outcome

A PDF is usually a final-layout description. It commonly contains glyph placement,
font resources, paths, images, clipping, and paint order, but it may not contain the
authoring concepts an editor needs: paragraphs, list identities, table cells, styles,
section breaks, header/footer ownership, or the relationship between visually adjacent
fragments. A scanned PDF may contain no text at all.

Therefore the product cannot promise a lossless PDF round trip into an editable word
processing model. It can provide a useful, honest workflow:

> **Import PDF as editable document** reconstructs the most defensible OpenDoc
> semantics from local evidence, preserves provenance outside the model, and names
> every material uncertainty or omission.

The completed workflow is:

```text
local PDF -> local evidence extraction -> local OCR where needed
          -> OpenDoc normalized model -> edit -> semantic DOCX/ODT/PDF export
```

It is explicitly not:

```text
PDF -> third-party DOCX converter -> reopen that DOCX
```

The distinction is load-bearing. OpenDoc remains responsible for model validity,
identities, layout, editing, transactions, compatibility reporting, and export.

## 3. Current repository constraints

The design extends, and must not weaken, these accepted constraints:

- the client-side fat-client architecture keeps model, layout, and paint local
  (`56`);
- Rust/WASM is the deterministic source of truth for import, model, and semantic
  export (`56`);
- all formats map into `v1::Document`; producer objects never become editor state
  (`94`, MF-I1);
- source-native information belongs in a bounded format-tagged sidecar (`94`,
  MF-I4);
- cross-format export writes modeled semantics and reports non-transferable source
  details (`94`, MF-I5);
- the host owns storage, network, model-asset, and external-resource policy (`94`,
  MF-I6);
- current PDF support is export-only: `PdfAdapter` advertises `can_import: false`,
  and `98` deliberately excludes PDF import;
- the current internal `FormatImporter::import` method is synchronous, while browser
  PDF reconstruction needs asynchronous Workers, optional asset loading, progress,
  cooperative yielding, and cancellation;
- the editor currently has a 4 GiB wasm32 address-space ceiling (`56`, `111`), and
  document-sized main-thread tasks are prohibited by the budget analysis in `116`.

Consequently PDF reconstruction is an additive staged-import capability. It must not
be forced into the current synchronous adapter method, and OCR weights must not be
loaded into the OpenDoc core's linear memory.

## 4. Experimental scope

### 4.1 In scope

- byte-authoritative PDF detection and bounded admission;
- password/encryption detection with a typed unsupported result in the first profile;
- extraction of native text, coordinates, page geometry, images, font/style clues,
  tagged-PDF hints, and safe link metadata;
- classification of pages and regions as native-text, raster-only, or hybrid;
- selective local OCR for raster-only or suspect regions;
- reading-order, paragraph, list, table, header/footer, image, and section
  reconstruction into supported schema-v1 semantics;
- page-by-page progress, cancellation, deterministic cleanup, and atomic commit;
- provenance, confidence, provider/version hashes, and compatibility findings;
- semantic DOCX, ODT, text, normalized JSON, and PDF export through existing writers;
- exact unchanged return of retained original PDF bytes when explicitly requested and
  policy permits it;
- browser-first delivery with no remote document inference.

### 4.2 Non-goals

- treating PDF paint operators or OCR Markdown as editor state;
- claiming recovery of the original authoring document;
- direct generation of DOCX by an OCR provider;
- pixel-perfect editability for arbitrary PDFs;
- editing PDF forms, annotations, signatures, attachments, JavaScript, or multimedia
  as PDF-native objects;
- executing actions, launch commands, scripts, embedded programs, or external-resource
  fetches;
- bypassing OpenDoc validation because a provider calls its output structured;
- shipping a multi-gigabyte VLM as a mandatory web dependency;
- silently falling back to cloud OCR when local capability is unavailable;
- making PDF import a prerequisite for the existing DOCX/ODT/TXT/JSON paths.

Doc 98's “PDF import is a non-goal” remains the current product truth. This experimental
document records a possible future design and changes no product scope. Scope changes
only if ADR-034 is separately accepted and the experiment passes its graduation gates.

## 5. Terminology and capability profiles

### 5.1 Terms

- **Native evidence:** information read from PDF objects without recognizing page
  pixels: text strings/glyphs, transforms, fonts, images, paths, marked content, tags,
  links, and page boxes.
- **Optical evidence:** text or structure inferred from rendered pixels.
- **Reconstruction:** deterministic conversion of validated evidence into normalized
  OpenDoc semantics.
- **Provider:** a host capability that extracts or recognizes evidence. A provider is
  not trusted to construct an OpenDoc document.
- **Evidence manifest:** versioned metadata that identifies every parser, model,
  preprocessing rule, runtime, and hash that affected an import.
- **Reconstruction profile:** the declared local capability level used for one import.

### 5.2 Profiles

| Profile | Required runtime | Intended coverage | Release posture |
| --- | --- | --- | --- |
| `native` | PDF parser/renderer Worker | Born-digital and tagged PDFs | Candidate first graduation target |
| `ocr-cpu` | `native` + small OCR model on WASM CPU | Printed scans and hybrid pages | Candidate OCR graduation target |
| `ocr-webgpu` | `native` + verified WebGPU provider | More difficult layouts on capable desktop browsers | Experimental and capability-gated |
| `experimental-vlm` | Quantized document VLM with verified browser port | Tables, formulas, handwriting, complex reading order | Experimental until all gates pass |

The host may offer only the profiles it can execute locally. A missing profile produces
a typed capability error or an explicit lower-profile choice; it never causes a remote
upload.

## 6. Architectural invariants

### PDFR-I1 — The normalized model remains authoritative

PDF objects, PDF.js objects, OCR tokens, Markdown, HTML tables, and VLM responses are
evidence only. Only Rust/WASM reconstruction may create `v1::Document` nodes.

### PDFR-I2 — Native evidence precedes OCR

Born-digital text is not rasterized and re-recognized by default. OCR is used only for
raster-only regions or when a deterministic native-evidence quality check identifies a
specific defect. A provider may not silently replace native text.

### PDFR-I3 — Reconstruction is staged but commits atomically

Progressive extraction may retain bounded staging state, but cancellation, failure,
or a hard-limit breach returns no editable session. The session is published only
after complete model validation and report finalization.

### PDFR-I4 — No silent inference

An inferred construct that materially affects editing—reading order, table topology,
list identity, heading level, repeated header/footer ownership, or formula
interpretation—must meet its acceptance rule or be represented conservatively and
reported as degraded. Confidence alone does not authorize invention.

### PDFR-I5 — OCR never writes DOCX

OCR providers emit `PdfEvidenceV1`. The existing semantic writers emit DOCX/ODT/PDF
from the normalized model. Provider-specific output cannot bypass the model or the
compatibility report.

### PDFR-I6 — Browser-local means no document egress

No document byte, page raster, extracted text, OCR token, or reconstructed node is
sent over the network. Model/runtime fetches use host policy, exact version pins, and
integrity hashes. Offline cached operation is an acceptance gate for the baseline.

### PDFR-I7 — Work is bounded and interruptible

PDF admission, rendering, OCR, evidence transfer, reconstruction, and validation have
independent limits. Page-sized work is chunked so the UI event loop can regain control.
No whole-document raster or unbounded provider response is retained.

### PDFR-I8 — Determinism is declared, not assumed

The evidence manifest records parser, renderer, OCR model, model weights hash,
runtime/backend, preprocessing, decoding, reconstruction schema, and limits. A profile
cannot be called deterministic until identical-input fixture tests reproduce identical
evidence and normalized models on every platform claimed by that profile.

### PDFR-I9 — Cross-format output is semantic

PDF-origin opaque records are not copied into DOCX or ODT. Exact original-PDF return is
allowed only for an unchanged session with explicitly retained source bytes. Edited PDF
export is a new PDF rendered from the OpenDoc model.

## 7. Layer architecture

```text
┌──────────────────────────────── Browser host ───────────────────────────────┐
│                                                                            │
│ Local File/Blob                                                            │
│      │                                                                     │
│      ▼                                                                     │
│ SDK import coordinator                                                     │
│      │ detect/admit through OpenDoc                                        │
│      ▼                                                                     │
│ PDF evidence Worker                                                        │
│  ┌────────────────────┐       ┌─────────────────────────────────────────┐  │
│  │ native PDF provider │──────▶│ page/region classifier                 │  │
│  │ parse + render      │       │ native / raster / hybrid              │  │
│  └────────────────────┘       └──────────────────┬──────────────────────┘  │
│                                                   │ uncertain pixels       │
│                                                   ▼                        │
│                                      ┌──────────────────────────────┐      │
│                                      │ local OCR provider           │      │
│                                      │ WASM CPU / gated WebGPU      │      │
│                                      └──────────────┬───────────────┘      │
│                                                     │                      │
│                  bounded transferable PdfEvidenceV1 batches                │
│                                                     │                      │
└─────────────────────────────────────────────────────┼──────────────────────┘
                                                      ▼
┌──────────────────────────── OpenDoc Rust/WASM core ─────────────────────────┐
│ Evidence validation -> deterministic reconstruction -> model validation     │
│          -> ImportArtifact(document, resources, source, report, format)     │
│                                │                                           │
│                         atomic session commit                              │
└────────────────────────────────┬───────────────────────────────────────────┘
                                 ▼
                    edit / layout / render / transactions
                                 │
                  semantic DOCX / ODT / JSON / TXT / PDF
```

### 7.1 Browser host responsibilities

- local file access and user consent;
- Worker lifecycle;
- static parser/runtime/model asset policy;
- cache/storage quotas;
- feature detection for WebAssembly, SIMD, threads, and WebGPU;
- progress, cancellation, and profile selection UI;
- transferring bounded evidence batches;
- guaranteeing that document-derived data is never used in a network request.

### 7.2 Provider responsibilities

- parse/render/recognize within requested limits;
- emit only the versioned evidence schema;
- include stable page/region identities and provenance;
- return deterministic stable error codes without document text;
- release page rasters and temporary tensors after each batch;
- never create model nodes or choose export behavior.

### 7.3 Rust/WASM responsibilities

- byte-authoritative format detection and PDF profile admission;
- evidence-schema and manifest validation;
- coordinate normalization and bounds enforcement;
- deterministic source precedence, reading-order, grouping, and semantic mapping;
- deterministic `NodeId` creation from source hash plus canonical evidence identity;
- resource deduplication and validation;
- doc-35 compatibility findings;
- complete schema-v1 validation and atomic `ImportArtifact` construction;
- all later edits and exports.

## 8. Staged import contract

### 8.1 Why the current importer is insufficient

The implemented `FormatImporter` performs one synchronous
`import(bytes) -> ImportArtifact`. Changing every existing adapter to async or allowing
an adapter to call arbitrary JavaScript would broaden a proven security and
determinism boundary for one exceptional format.

Retain the synchronous contract for byte-native adapters. Add an internal staged
capability behind the already-async target SDK `Engine::open` surface.

Conceptual contract:

```rust
pub enum ImportExecution {
    Immediate,
    Staged {
        provider: ProviderCapability,
        evidence_schema: EvidenceSchemaId,
    },
}

pub trait StagedFormatImporter: Send + Sync {
    fn descriptor(&self) -> &FormatDescriptor;
    fn probe(&self, request: ProbeRequest<'_>) -> ProbeResult;
    fn begin(&self, request: StagedImportRequest<'_>)
        -> Result<Box<dyn StagedImportJob>, AdapterError>;
}

pub trait StagedImportJob: Send {
    fn plan(&self) -> &ImportPlan;
    fn push_evidence(
        &mut self,
        batch: PdfEvidenceBatch,
    ) -> Result<BatchReceipt, AdapterError>;
    fn finish(self: Box<Self>)
        -> Result<ImportArtifact, AdapterError>;
    fn cancel(self: Box<Self>);
}
```

This is an architectural shape, not a stable public Rust API. Exact ownership and
borrowing are decided in the API slice.

Detection and execution availability are separate facts. The future registry must be
able to recognize authoritative PDF bytes even when the current host lacks an admitted
local evidence provider, returning `local_capability_unavailable` rather than the
misleading `unsupported_format`. `availableImportFormats()` advertises PDF as usable
only when both the staged adapter and its required local provider are registered. The
current registry and UI continue to advertise export only.

### 8.2 State machine

```text
Detected
   │
   ▼
Admitted ── hard failure ──▶ Failed
   │
   ▼
Inventoried (page count, boxes, encryption, feature flags)
   │
   ▼
Extracting native evidence ◀──────────────┐
   │                                      │ next page/region
   ▼                                      │
Classifying ── raster/uncertain ──▶ OCR ──┘
   │ native/merged evidence
   ▼
Reconstructing in bounded batches
   │
   ▼
Validating
   │
   ├─ success ──▶ Committed
   ├─ cancel  ──▶ Cancelled
   └─ failure ──▶ Failed
```

`Cancelled` and `Failed` destroy page rasters, tensors, provider state, evidence
batches, provisional resources, and the uncommitted model. They leave the previously
open session unchanged.

### 8.3 Progress contract

Progress is phase-based, not a fabricated percentage derived only from page count:

```text
admitting | inventory | extracting | recognizing | reconstructing | validating
```

Each event carries completed/known work units, current page when safe to disclose to
the local UI, whether the total is exact or estimated, and a cancellable flag. Model
download progress is a separate host event so it is never confused with document
processing.

### 8.4 Cooperative main-thread behavior

The evidence Worker performs PDF parsing/rendering and OCR. The OpenDoc core consumes
small evidence batches through resumable builder calls and yields between them. No
single reconstruction call may scale with total document size or exceed the browser
long-task gate.

Moving the entire live `WasmDocument` into a Worker is not a prerequisite for the first
slice. The staged builder is unpublished session state; it can accept one page or a
bounded group of regions, return to the event loop, and publish only at `finish`.

## 9. `PdfEvidenceV1`

### 9.1 Requirements

The evidence format is:

- internal and versioned;
- strict: unknown fields are rejected until a versioned migration accepts them;
- bounded before allocation where possible and after decoding always;
- transportable in page/region batches;
- independent of PDF.js, PaddleOCR, Tesseract, ONNX Runtime, or any VLM;
- incapable of expressing arbitrary OpenDoc nodes;
- canonical enough to hash and replay in tests.

### 9.2 Manifest

```rust
pub struct PdfEvidenceManifestV1 {
    pub source_sha256: [u8; 32],
    pub source_bytes: u64,
    pub page_count: u32,
    pub extractor: ProviderIdentity,
    pub renderer: ProviderIdentity,
    pub ocr: Option<ProviderIdentity>,
    pub execution_backend: ExecutionBackend,
    pub preprocessing_profile: String,
    pub decoding_profile: String,
    pub coordinate_scale: u32,
    pub limits_profile: String,
}

pub struct ProviderIdentity {
    pub id: String,
    pub version: String,
    pub artifact_sha256: [u8; 32],
}
```

Provider strings and versions have bounded lengths and a restricted syntax. A model
identifier without an exact artifact hash is not admissible for a release profile.

### 9.3 Coordinate system

Provider floating-point coordinates are not stored directly. Before transfer they are
normalized into page-local signed integers at **1/1000 PDF point**. The manifest fixes
that scale. Rust validates page boxes and converts to OpenDoc twips with one documented
rounding rule.

This prevents provider-specific float formatting from changing sort order, identities,
or output across hosts while retaining substantially more precision than the model
needs.

### 9.4 Page evidence

Each page batch may contain:

- media/crop/trim box and rotation;
- native text spans with Unicode text, direction, transform-derived box, baseline,
  font identity clue, size, color, marked-content/tag identity, and paint order;
- OCR spans with text, box/polygon, script/language clue, integer confidence, and
  source raster-region identity;
- images with resource identity, placement, crop clue, and separately bounded bytes;
- vector/layout cues such as rules, filled rectangles, and separator lines—not an
  unbounded copy of every PDF paint operator;
- link rectangles and sanitized targets;
- tagged-PDF structural hints;
- block/table/formula candidates with provider provenance;
- page classification and quality findings.

Confidence uses an integer range `0..=10_000`. It is evidence metadata, not a universal
probability claim. Thresholds are provider/profile-specific, versioned, and tested.

### 9.5 Provenance and precedence

Every text span is one of:

```text
native-pdf | optical | provider-derived-structure
```

Native and optical spans that cover the same region are retained as competing evidence
until Rust applies the profile's deterministic conflict rule. The losing span is not
silently discarded when its text materially differs; the result is reported and the
source envelope retains the bounded conflict record.

### 9.6 What evidence cannot contain

- serialized `v1::Document` fragments;
- DOCX XML or packages emitted by a provider;
- executable code or provider callbacks;
- unbounded HTML/Markdown;
- URLs to fetch page resources;
- arbitrary provider extension maps;
- raw page rasters after their recognition batch completes.

## 10. Extraction and reconstruction policy

### 10.1 Page inventory and classification

Classify at page and region granularity using deterministic facts:

- native Unicode coverage and density;
- text-box intersection with visible page content;
- replacement/control-character rate;
- invisible-text and image-overlay patterns;
- image coverage;
- repeated identical spans;
- transform/box validity;
- tagged structure availability.

Possible classifications are `native`, `raster`, and `hybrid`. Classification itself
does not create document semantics.

### 10.2 Native-first merge

1. Admit valid native spans.
2. Render only raster pages or selected uncertain regions.
3. OCR those regions.
4. De-duplicate overlapping native/OCR spans geometrically.
5. Prefer valid native Unicode unless the declared quality rule proves it unusable.
6. Record material conflicts and low-confidence optical text.

This prevents a common failure in OCR-first converters: converting already-correct PDF
text into a lower-quality recognition result.

### 10.3 Reading order

Reading order is reconstructed from page geometry, writing direction, columns, tagged
structure, separators, and provider candidates. The algorithm must use stable
coordinate ordering and explicit tie-breaks. A VLM reading order may be one candidate;
it is not automatically authoritative.

Ambiguous order is represented conservatively as independently editable paragraphs in
the selected stable order and reported as `pdf.import.reading_order_inferred` or
`pdf.import.reading_order_ambiguous`.

### 10.4 Paragraphs, runs, and styles

- Baseline proximity, indentation, line gaps, alignment, and font clues group spans
  into lines and paragraphs.
- Unicode content and reading order have higher authority than visual line wrapping;
  PDF line endings are not automatically paragraph endings.
- Repeated style clues create deduplicated normalized styles only when their semantic
  identity is defensible; otherwise direct formatting is safer.
- Font names are clues, not proof that a face is available. Missing faces use the
  existing font-substitution policy and produce a finding when metrics/appearance may
  change.
- Page-positioned text that cannot be represented without inventing flow may use a
  supported text-box/floating-object model or degrade to ordered paragraphs with a
  finding. It must never disappear.

### 10.5 Lists and headings

List markers, indentation, repeated numbering patterns, tagged structure, and style
evidence may infer a list. A visual bullet alone does not prove shared list identity.
Uncertain lists remain paragraphs containing their visible markers.

Heading levels require tagged structure, a consistent document-wide typography rule,
or another profile-approved signal. Large type alone is insufficient for a confident
heading claim.

### 10.6 Tables

Table reconstruction combines ruling lines, aligned cell boundaries, whitespace,
repeated row geometry, tagged structure, and optional provider candidates.

The accepted topology must be rectangular and pass schema-v1 table validation. When a
merged-cell topology is uncertain or invalid, preserve visible reading order using
paragraphs or a simpler valid table and report the degradation. Provider-emitted HTML
is parsed as bounded evidence; it is never inserted into the document or trusted as a
valid table by itself.

### 10.7 Headers, footers, and page furniture

Repeated content in stable top/bottom page bands is a candidate header/footer. Repeated
watermarks, page numbers, and document body text must be distinguished using frequency,
position, overlap, and tagged evidence. If ownership is uncertain, retain visible
content in page-positioned constructs or body order and report the result rather than
silently deleting it as “repetition.”

### 10.8 Images, drawings, and formulas

- Embedded image bytes are preferred to screenshots when safe and decodable.
- Vector art may be reconstructed only for geometry the model can represent; otherwise
  use a bounded raster fallback with an explicit finding.
- Formula recognition is an independent capability. Plain OCR text must not be labeled
  as typed mathematics without a formula provider and validation.
- Alt text is imported only from trustworthy source metadata. A caption or model guess
  is not silently converted into authored alt text.

## 11. OCR and model policy

### 11.1 Baseline recommendation

The first browser OCR evaluation should use the official PaddleOCR.js pipeline with a
small PP-OCR detection/recognition pair through ONNX Runtime Web. The project documents
browser frontend inference, WASM execution, custom ONNX model assets, and dedicated
Worker mode. It is a recognition component, not the semantic reconstruction engine.

Tesseract.js is a useful printed-text fallback and independent baseline. It requires an
external PDF renderer and is not a layout/table reconstruction solution.

### 11.2 Advanced candidates

OvisOCR2 and comparably small document parsers may be evaluated as optional WebGPU
providers after a reproducible browser port exists. Their Markdown or structured output
supplies candidate reading order, tables, or formulas but still requires geometric
evidence and Rust validation.

UnlimitedOCR, dots.mocr, full PaddleOCR-VL, MinerU, and similar large document VLMs are
not production browser dependencies in this design. UnlimitedOCR's published path uses
Python/CUDA, vLLM or SGLang and custom model code. Its long-context attention work does
not remove browser download, resident-weight, operator, GPU, or compatibility costs.

### 11.3 Provider admission checklist

Before a provider can move from evaluation to a supported profile:

1. licence and model-weight terms pass repository policy;
2. all artifacts are immutable and SHA-256 pinned;
3. no runtime downloads executable code or uses `trust_remote_code`-equivalent behavior;
4. the browser runtime/operator set is documented and tested;
5. model download, peak CPU memory, peak GPU memory, and per-page latency are measured;
6. exact evidence reproducibility is tested on every claimed browser/backend;
7. malformed images and adversarial outputs fail closed;
8. the repository corpus proves improvement over native extraction plus baseline OCR;
9. the provider can be omitted without breaking native PDF reconstruction;
10. public capability discovery reports its actual availability.

### 11.4 WebGPU policy

WebGPU is acceleration, not the universal correctness baseline. The portable profile
uses CPU/WASM. A WebGPU profile is exposed only where the runtime and required operators
are verified. If exact cross-device evidence cannot be demonstrated, the provider stays
experimental and cannot silently change release-profile imports.

## 12. Browser runtime, memory, and caching

### 12.1 Worker topology

- one PDF extraction/rendering Worker;
- one OCR inference context in that Worker or a second Worker when measurement proves
  parallelism helps within memory bounds;
- the interactive OpenDoc WASM module remains separate;
- `SharedArrayBuffer` is optional optimization, not a baseline requirement;
- transferable `ArrayBuffer`/`ImageBitmap` ownership is used for page batches;
- WebGPU in a Worker is capability-tested rather than assumed.

Keeping inference separate prevents model weights from consuming the OpenDoc core's
4 GiB wasm32 linear address space. Browser process and GPU memory still require explicit
budgets.

### 12.2 Streaming discipline

- inventory metadata may cover the document;
- at most a small configured number of page rasters are live;
- render resolution is selected per region and capped by decoded pixels;
- OCR tensors and raster buffers are released after evidence transfer;
- evidence is pushed and validated page-by-page;
- original embedded resources are deduplicated by content hash;
- full-document Markdown, HTML, or page-image arrays are prohibited.

### 12.3 Asset caching

Parser/runtime/model assets are versioned static resources. The host may cache them in
Cache Storage or OPFS under quota policy. The UI shows download size before an optional
model pack is fetched. Cache eviction removes only reproducible assets, never the user's
document or committed session.

The offline gate loads an already-cached application, disables the network, and imports
a PDF successfully.

## 13. Determinism and identity

Determinism is defined for identical:

- source bytes;
- selected profile;
- provider artifacts and hashes;
- runtime/backend version;
- preprocessing and decoding parameters;
- limits;
- font environment used after reconstruction;
- OpenDoc reconstruction version.

Required rules:

- coordinate quantization happens once before evidence transfer;
- region and span sorting uses canonical keys with total tie-breaks;
- model decoding is greedy/fixed and uses no random sampling;
- locale, clock, random number generation, and browser iteration order cannot affect
  output;
- `NodeId`s derive from source hash, page, evidence identity, semantic role, and stable
  occurrence—not Worker completion order;
- parallel extraction may finish out of order, but commit consumes canonical page order;
- compatibility findings are deterministically aggregated and sorted;
- an evidence replay fixture bypasses inference and must construct the identical model.

Evidence replay is important: it separates parser/OCR drift from reconstruction drift
and makes the Rust mapping testable without downloading a model in every workspace test.

## 14. Security and resource bounds

### 14.1 Threats

- malformed cross-reference/object streams and recursive object graphs;
- decompression bombs, oversized images, extreme page boxes, and excessive objects;
- hostile embedded fonts and images;
- encrypted content and password prompts;
- JavaScript, launch actions, embedded files, multimedia, and unsafe URI schemes;
- external resource references;
- OCR/VLM prompt injection embedded in page content;
- hallucinated or structurally invalid provider output;
- model/runtime supply-chain substitution;
- memory exhaustion through page concurrency or retained tensors;
- UI denial of service through long main-thread tasks or non-yielding providers.

Document text is data. Instructions printed inside a page never alter provider policy,
select models, access tools, fetch resources, or bypass validation.

### 14.2 Initial limits for measurement

These are **provisional design defaults**, not accepted release constants. PDFR-000
must validate them against hostile fixtures and the rights-reviewed corpus before they
enter `21-PARSER-LIMITS.md` as normative values.

| Limit | Provisional default | Provisional hard ceiling |
| --- | ---: | ---: |
| Source PDF bytes | reuse 256 MiB global input limit | reuse 1 GiB global ceiling |
| Pages | 2,000 | 10,000 |
| Page width or height | 14,400 pt | 72,000 pt |
| Rasterized pixels per page | 25 megapixels | 100 megapixels |
| Aggregate in-flight raster pixels | 50 megapixels | 200 megapixels |
| Evidence records per page | 100,000 | 1,000,000 |
| OCR regions per page | 4,096 | 16,384 |
| Evidence bytes per page | 16 MiB | 64 MiB |
| Total recognized Unicode scalars | reuse 50,000,000 semantic limit | reuse 200,000,000 ceiling |
| Compatibility findings | bounded/aggregated under format-report policy | no unbounded per-token rows |

Provider model bytes, browser cache, CPU memory, and GPU memory are host/runtime budgets,
not document parser limits. Each still needs a secure default, opt-in higher tier, and
measured failure behavior.

### 14.3 Fail-closed behavior

- encrypted PDFs return a typed unsupported/encrypted error in the first profile;
- active content is never executed;
- external targets are recorded only after scheme sanitation and are never fetched;
- attachments are omitted or retained only under an explicit future bounded policy;
- one over-limit page fails the atomic import rather than creating a silently partial
  document;
- diagnostics contain page/feature identity and counts, not extracted document text;
- cancellation is checked between PDF objects/pages, render tiles, OCR regions,
  evidence batches, and reconstruction batches.

## 15. Source envelope and export semantics

### 15.1 PDF source state

The PDF `SourceEnvelope` may contain:

- source format/profile and adapter/reconstruction version;
- the evidence manifest;
- source hash and optional retained original bytes;
- bounded page-level provenance and native/OCR conflict records;
- compatibility-ledger references;
- safe resource hashes needed to explain reconstruction.

It does not retain full page rasters or inference tensors. Evidence needed only to build
the normalized model is discarded after commit unless a bounded diagnostic record has a
specific consumer.

### 15.2 Same-format behavior

- `ExactIfUnchanged`: may return retained original PDF bytes when the session is
  unchanged and original retention was requested.
- `PreserveWhenSafe`: not advertised for edited PDF-origin sessions in the first
  profile. Merging edits back into arbitrary PDF objects is outside scope.
- `Semantic`: creates a new PDF from the OpenDoc model through the existing PDF writer.

Changing PDF import capability therefore requires revisiting the current PDF descriptor:
`can_import` becomes host-capability dependent, while exact-unchanged support depends on
retention policy. Export availability remains independent.

### 15.3 DOCX and other targets

PDF-origin to DOCX/ODT is always semantic conversion. The target writer receives the
normalized document and resources, never provider output or PDF opaque state. Losses
caused by the target format are added to the export report independently of the import
report.

## 16. Compatibility reporting and UX

### 16.1 Stable finding families

Initial feature identifiers should include, at minimum:

```text
pdf.import.active_content_blocked
pdf.import.attachment_omitted
pdf.import.encrypted_unsupported
pdf.import.native_text_invalid
pdf.import.native_ocr_conflict
pdf.import.ocr_low_confidence
pdf.import.reading_order_inferred
pdf.import.reading_order_ambiguous
pdf.import.paragraph_boundary_inferred
pdf.import.list_inferred
pdf.import.table_inferred
pdf.import.table_topology_degraded
pdf.import.header_footer_inferred
pdf.import.font_substituted
pdf.import.vector_rasterized
pdf.import.formula_unrecognized
pdf.import.tag_structure_ignored
```

Healthy fully mapped evidence remains the implicit `mapped + not-applicable` default
and is not enumerated, matching doc 35. Repeated uncertainties aggregate by feature and
bounded page/location information; the report must not produce one row per OCR token.

### 16.2 User-facing workflow

The UI labels the action **Import PDF as editable document**. It must disclose:

- processing is local and the document is not uploaded;
- the active profile and any optional model download;
- progress phase and cancellation;
- whether advanced local recognition is unavailable on this device;
- a post-import summary of inferred, degraded, omitted, or blocked content;
- that the result is a reconstruction and may not retain original PDF layout semantics.

The original PDF remains untouched. A user can cancel before commit without changing
the currently open document.

## 17. Proposed module boundaries

Names are provisional and exist to assign responsibility, not to pre-create crates.

| Component | Responsibility | Must not own |
| --- | --- | --- |
| `casual-doc-pdf-import` | PDF probe, evidence validation, deterministic reconstruction, PDF source state | Browser APIs, model download, DOCX writing |
| `casual-doc-io` | Descriptor, detection, staged dispatch, `ImportArtifact`/report integration | PDF algorithms, OCR runtime |
| `casual-doc-wasm` | Staged builder bindings, progress/cancel bridge, atomic session commit | Network/model policy |
| web `pdf-import` controller | Worker orchestration, feature detection, asset/cache policy | Document semantics |
| PDF provider Worker | Native parse/render and page inventory | OpenDoc model |
| OCR provider | Optical spans/structure candidates | Session creation or export |

The first implementation may keep `casual-doc-pdf-import` as a module until its
dependency direction and test surface justify a crate, following ADR-011.

## 18. Delivery plan

This is an unscheduled experimental sequence, not an approved roadmap. The PDFR rows
after PDFR-000 are created in the execution tracker only if ADR-034 graduates from
proposal to an accepted planned feature.

### PDFR-000 — Decision, corpus, limits, and dependency review

- review and accept/reject ADR-034;
- decide whether PDF.js is the browser native-evidence provider;
- audit candidate parser/OCR/runtime licences and artifact delivery;
- create rights-safe digital, scanned, and hybrid fixtures plus hostile PDFs;
- measure and accept exact parser/inference limits;
- define browser capability/error vocabulary.

**Gate:** no implementation dependency is added and `can_import` stays false until this
slice is accepted.

### PDFR-001 — Evidence schema and replay reconstruction

- define strict `PdfEvidenceV1` and canonical encoding;
- implement bounded Rust validation;
- reconstruct paragraphs/runs/images/page geometry from committed evidence fixtures;
- create `ImportArtifact`, source envelope, findings, and deterministic IDs;
- prove evidence replay produces identical normalized models.

**Gate:** no PDF parser or OCR dependency is needed; malformed evidence and limits fail
atomically.

### PDFR-002 — Native digital-PDF browser path

- add PDF byte detection and staged capability discovery;
- run the selected PDF parser/renderer in a Worker;
- extract native text/geometry/resources and tagged hints;
- import born-digital fixtures without OCR;
- add progress, cancellation, offline, memory, and main-thread gates.

**Gate:** digital PDFs import locally, no document-derived network request occurs, and
the feature is still honest about inferred semantics.

### PDFR-003 — Baseline scanned-PDF OCR

- integrate the accepted small browser OCR provider;
- implement page/region classification and selective rasterization;
- merge native and optical evidence under deterministic precedence;
- add multilingual printed-text corpus coverage;
- expose the `ocr-cpu` profile.

**Gate:** OCR improves the scanned subset without regressing the digital subset; CPU
WASM works in the supported baseline browser matrix.

### PDFR-004 — Layout, tables, and difficult pages

- add column/reading-order and repeated-furniture reconstruction;
- add bounded table topology inference;
- add formula/handwriting providers only as separately gated capabilities;
- extend edit-save-reopen and visual corpus gates.

**Gate:** each promoted construct family has semantic, visual, editability, and loss
report evidence. Benchmark scores alone are insufficient.

### PDFR-005 — Optional WebGPU/VLM profile

- evaluate compact models against the same corpus;
- verify browser operator coverage, downloads, memory, determinism, and cancellation;
- expose only through capability discovery and explicit user choice;
- retain the CPU/native path as an independent supported profile.

**Gate:** unsupported devices degrade to an honest local profile, never to cloud OCR.

### PDFR-006 — Production conformance

- cross-browser matrix for every claimed profile;
- fuzz/hostile corpus and dependency-policy gates;
- named-device performance and memory baselines;
- accessibility of progress/errors/report UI;
- published support profile and known limitations.

## 19. Test and release architecture

### 19.1 Corpus dimensions

The rights-reviewed corpus must independently vary:

- born-digital, scanned, and hybrid PDFs;
- single/multi-column reading order;
- tables with and without ruling lines, spans, and repeated headers;
- paragraphs, headings, lists, headers/footers, footnotes, page numbers, and watermarks;
- embedded images, vector art, clipped text, rotations, and unusual page boxes;
- Latin, CJK, RTL, Indic, and mixed scripts;
- formulas, handwriting, skew, blur, low contrast, and photographed pages;
- tagged and untagged PDFs;
- malformed, encrypted, active-content, oversized, recursive, and decompression-hostile
  inputs.

### 19.2 Correctness gates

- exact native text and Unicode preservation where PDF evidence is valid;
- OCR character/word error by script and degradation tier;
- reading-order edit distance;
- table topology and cell-content accuracy;
- schema-v1 validation and deterministic identities;
- import report completeness and legal doc-35 outcomes;
- `PDF -> model -> DOCX -> model` semantic comparison for the supported surface;
- edit, save, reopen, undo, and transaction behavior on reconstructed nodes;
- no partial session after failure or cancellation.

### 19.3 Visual and editability gates

- render reconstructed OpenDoc pages and compare against source PDF page rasters using
  a declared tolerance and fonts;
- distinguish semantic success from visual similarity;
- test reflow after editing, not only initial visual overlap;
- verify tables, lists, paragraphs, and images are actually editable rather than one
  page-sized bitmap or text box;
- reopen emitted DOCX in Word and LibreOffice on the rights-cleared compatibility set.

### 19.4 Browser and privacy gates

- no page/OCR/reconstruction task exceeds the accepted main-thread long-task budget;
- cancellation latency is bounded at every phase;
- peak JS, WASM, and GPU memory are measured;
- model assets are cached and an offline import succeeds;
- a network interceptor proves that no document-derived request leaves the page;
- baseline CPU/WASM tests run on every browser claimed in doc 18;
- WebGPU tests run only for explicitly claimed browser/device classes;
- unsupported capability and quota-exhaustion paths are tested.

### 19.5 Security gates

- boundary-accepted, boundary-rejected, and far-over-limit fixtures for every new
  limit;
- fuzz targets for evidence decoding and Rust reconstruction;
- hostile PDF browser tests in an isolated Worker;
- unsafe actions/resources never execute or fetch;
- model/runtime artifacts match pinned hashes;
- diagnostic messages never contain extracted document content.

## 20. Open decisions

No implementation begins until the owner accepts or changes these decisions:

1. **Product scope:** accept PDF semantic reconstruction as a planned input capability,
   superseding doc 98's import non-goal only for the new staged path.
2. **Native browser provider:** accept PDF.js as the first candidate subject to
   dependency/security review, or select another permissively licensed parser.
3. **Baseline OCR:** accept PaddleOCR.js/PP-OCR small or tiny as the first evaluation,
   with Tesseract.js as a comparison/fallback rather than the semantic engine.
4. **Portable profile:** require CPU/WASM as the universal baseline; keep WebGPU/VLM
   optional.
5. **Source retention:** decide whether exact unchanged original-PDF return is enabled
   by default, opt-in, or prohibited for storage-sensitive hosts.
6. **Initial language set:** choose the script/language packs that are release-blocking;
   model size and offline cache cost depend on this.
7. **Mobile scope:** decide whether mobile/tablet may use native-only reconstruction or
   whether OCR is required there; current mobile/browser support is already incomplete
   in doc 18.
8. **Large-document policy:** accept the provisional page/pixel limits only after
   measurements and decide whether partial page-range import is a separate future
   product feature. The first profile remains atomic whole-document import.

## 21. Experimental graduation and architecture acceptance gate

The feature remains experimental until all research and prototype evidence is reviewed.
It is ready to become planned implementation work only when:

1. ADR-034 is accepted with decisions 1–8 recorded;
2. doc 21 contains reviewed PDF/evidence/OCR limits;
3. doc 18 states the selected profiles without claiming implementation;
4. dependency licences, source pins, and artifact hashes pass policy review;
5. the rights-safe and hostile fixture plan is committed;
6. the staged import API preserves existing synchronous adapters;
7. main-thread, memory, privacy, determinism, cancellation, and offline gates have
   owners in doc 15;
8. the tracker splits implementation into the reviewable PDFR slices above.

Until then PDF remains export-only in product behavior.

## 22. Research references

Primary sources consulted for this design:

- Mozilla, PDF.js, browser PDF parser/renderer and display API:
  <https://github.com/mozilla/pdf.js> and
  <https://mozilla.github.io/pdf.js/api/draft/module-pdfjsLib.html>.
- PaddlePaddle, official PaddleOCR.js browser SDK, WASM/ONNX and Worker options:
  <https://github.com/PaddlePaddle/PaddleOCR/tree/main/paddleocr-js>.
- Microsoft, ONNX Runtime Web execution providers and browser support:
  <https://onnxruntime.ai/docs/get-started/with-javascript/web.html> and
  <https://onnxruntime.ai/docs/tutorials/web/env-flags-and-session-options.html>.
- Tesseract.js, browser/WASM support and PDF-rendering requirement:
  <https://github.com/naptha/tesseract.js/blob/master/docs/faq.md>.
- Baidu, UnlimitedOCR implementation/runtime and technical report:
  <https://github.com/baidu/Unlimited-OCR> and
  <https://arxiv.org/abs/2606.23050>.
- OvisOCR2 technical report, compact end-to-end document parsing candidate:
  <https://arxiv.org/abs/2607.13639>.

Benchmark claims in model papers are evidence about recognition datasets, not proof of
OpenDoc editability, browser suitability, determinism, privacy, or DOCX fidelity. The
repository-specific gates in §19 remain authoritative.
