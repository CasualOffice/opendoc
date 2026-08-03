# 94 — Multi-format Import and Export Architecture

**Status:** Accepted — implementation in progress
**Date:** 2026-08-04
**Tracker:** MFIO-001

## 1. Purpose

Define one deterministic, bounded import/export boundary for multiple document
formats without making the normalized model, editor, layout engine, or renderer
depend on a source file format.

The first additional office format is OpenDocument Text (`.odt`). The design
also covers normalized snapshots and plain text, and leaves a stable registration
seam for later Markdown/HTML interchange or trusted host-provided adapters.

This is an architecture design, not a support claim. No format becomes supported
until its security, corpus, round-trip, and compatibility gates pass.

Implementation snapshot (2026-08-04): Slices A–C and the bounded ODT package
checkpoint of Slice D are complete. Slice D's core `content.xml` importer now
maps paragraphs, headings, spans, explicit spaces, tabs, and line breaks into a
validated schema-v1 document with semantic-fact-derived identities and bounded
compatibility findings. The remaining Slice D surface and all ODT export/host
work remain incomplete. The core subset is registered as an import-only adapter
with definitive package evidence and optional exact-source retention; see docs
14, 18, and 95 for the exact status.

## 2. Scope

### In scope

- deterministic format detection from bytes plus optional host hints;
- explicit format selection for import and export;
- built-in and trusted host/plugin adapter registration;
- one format-neutral import result, export result, compatibility report, and
  source-preservation envelope;
- bounded package admission shared by ZIP-based formats;
- preservation-aware same-format save and reportable cross-format conversion;
- compatibility wrappers for the current DOCX-specific SDK/WASM calls;
- initial profiles for DOCX, ODT, normalized JSON, and plain text.

### Out of scope

- a spreadsheet model for `.ods`;
- a presentation model for `.odp`;
- treating HTML or Markdown as the document source of truth;
- executing macros, scripts, embedded programs, or external resources;
- automatic network retrieval by an adapter;
- claiming byte-identical edited packages across formats.

ODF is a family. This runtime's current normalized schema is a word-processing
model, so `.odt` is an additive adapter. `.ods` and `.odp` require distinct
semantic models and layout systems; recognizing their media types must produce a
typed unsupported-format result rather than importing them as text documents.

## 3. Existing constraints

The target architecture already describes DOCX/ODT/MD/TXT import-export as a
cross-cutting boundary, and layout/rendering do not inspect DOCX XML. The current
implementation is not yet adapter-neutral:

- `casual-doc-import` accepts `casual_doc_ooxml::DocxPackage` directly;
- `casual-doc-export` is a DOCX writer despite its generic crate name;
- `casual-doc-wasm::open` always admits a `DocxPackage`;
- `WasmDocument::exportDocx` is the only browser export operation;
- the current compatibility report describes WordprocessingML features and OPC
  parts;
- retained source/part state is not owned by the browser session, so the current
  browser semantic-save path cannot preserve all safe opaque source parts.

The multi-format work must correct these boundaries without weakening current
DOCX behavior.

## 4. Architectural invariants

### MF-I1 — The normalized document is the editing truth

Adapters map between source bytes and `v1::Document`. Format XML, package parts,
and producer objects never become editor state or layout input.

### MF-I2 — Detection never means trust

A suffix, MIME hint, or successful probe only selects a candidate adapter. The
selected adapter must still validate its package and document profile under
configured limits.

### MF-I3 — No silent loss

Every imported construct is mapped, degraded, or omitted and receives the
retention outcome defined by doc 35. Every export reports source-native data that
cannot be represented in the target format.

### MF-I4 — Preservation is format-tagged sidecar state

Opaque source material is not added to the normalized model. A session owns a
bounded `SourceEnvelope` tagged with its source format and adapter version. Only
the matching adapter may interpret or re-emit its records.

### MF-I5 — Cross-format export is semantic conversion

Exporting an ODT-origin document to DOCX (or the reverse) writes modeled
semantics. Source-native opaque records are never smuggled into an unrelated
package. Any non-transferable record is explicitly dispositioned in the export
report.

### MF-I6 — Hosts own policy

Adapters cannot fetch external resources, choose passwords, execute document
code, or load plugins. They issue bounded resource/policy requests through host
interfaces.

### MF-I7 — Deterministic dispatch

For identical bytes, hints, registry contents, limits, and adapter versions,
probing selects the same adapter or returns the same ambiguity/error. Registry
iteration order cannot affect the result.

## 5. Format identity and descriptors

Public format identity is an extensible string newtype, not a closed enum:

```rust
pub struct FormatId(String);

pub mod formats {
    pub const DOCX: &str = "org.openxmlformats.wordprocessingml.document";
    pub const ODT: &str = "org.oasis.opendocument.text";
    pub const NORMALIZED_JSON: &str = "org.casualoffice.normalized-json";
    pub const TEXT: &str = "text.plain";
}
```

A `FormatDescriptor` supplies:

- stable `FormatId`;
- display name;
- accepted MIME types and extensions;
- import/export availability;
- source versions/profiles accepted by the adapter;
- export profiles emitted by the adapter;
- whether unchanged reconstruction and edit-tolerant preservation are
  available;
- adapter origin (`built_in` or a trusted plugin identifier).

String IDs keep the public SDK additive when hosts register a format. Unknown IDs
remain serializable and produce `unsupported` when no matching adapter exists.

## 6. Open and detection contract

The format-neutral SDK shape is:

```rust
pub struct OpenRequest {
    pub bytes: Bytes,
    pub format: FormatSelection,
    pub file_name_hint: Option<String>,
    pub mime_hint: Option<String>,
    pub options: OpenOptions,
}

pub enum FormatSelection {
    Auto,
    Explicit(FormatId),
}

impl Engine {
    pub async fn open(&self, request: OpenRequest)
        -> Result<DocumentSession, SdkError>;
}
```

Detection proceeds in a fixed sequence:

1. enforce the global input-byte limit;
2. collect registered importers in ascending `FormatId` order;
3. use an explicit format, when supplied, as the sole candidate;
4. otherwise run bounded byte probes that return `no_match`, `possible`, or
   `definite` plus a stable evidence code;
5. reject multiple definite matches or an unresolved top-score tie as
   `ambiguous_format`;
6. admit/parse through the selected adapter, which independently validates the
   claimed format;
7. return the normalized document, media/resources, compatibility report, and
   source envelope atomically.

File names and MIME values are tie-breaking hints only. They cannot override
contradictory package evidence. ZIP-based DOCX and ODT detection happens after
one generic bounded ZIP-directory admission: OPC metadata identifies DOCX;
ODF's `mimetype` and manifest identify ODT.

## 7. Adapter contracts

The internal registry uses object-safe, read-only adapter interfaces. These are
internal first; stabilizing them as a public plugin ABI is a separate API review.

```rust
trait FormatImporter: Send + Sync {
    fn descriptor(&self) -> &FormatDescriptor;
    fn probe(&self, source: &ProbeSource<'_>) -> ProbeResult;
    fn import(&self, request: ImportRequest<'_>)
        -> Result<ImportArtifact, FormatError>;
}

trait FormatExporter: Send + Sync {
    fn descriptor(&self) -> &FormatDescriptor;
    fn export(&self, request: ExportRequest<'_>)
        -> Result<ExportArtifact, FormatError>;
}
```

`ProbeSource` exposes only bounded prefix bytes and admitted package metadata; a
probe cannot decompress arbitrary parts. `ImportRequest` and `ExportRequest`
carry cancellation, limits, resource providers, and host policy.

Adapters are pure with respect to session state. Import builds a complete
artifact before the session is created. Export observes an immutable document
snapshot and cannot mutate the session.

## 8. Import and export artifacts

```rust
pub struct ImportArtifact {
    pub document: Document,
    pub resources: DocumentResources,
    pub source: SourceEnvelope,
    pub report: CompatibilityReport,
    pub format: FormatProfile,
}

pub struct ExportArtifact {
    pub bytes: Bytes,
    pub report: CompatibilityReport,
    pub format: FormatProfile,
    pub mime_type: String,
    pub suggested_extension: String,
}
```

`SourceEnvelope` contains only validated, bounded records:

- source format/profile and adapter version;
- immutable source snapshot needed for an unchanged reconstruction contract;
- typed preservation ledger;
- safe opaque package parts;
- provenance/mapping records;
- content hashes used to validate preservation decisions.

The report vocabulary remains doc 35's two axes but feature locations become
format-neutral: format id, source part, XML namespace/local name or logical
feature, bounded location, occurrence count, model outcome, retention outcome,
and optional ledger record id.

## 9. Export selection and save semantics

```rust
pub struct ExportRequestOptions {
    pub format: FormatId,
    pub profile: Option<String>,
    pub mode: ExportMode,
}

pub enum ExportMode {
    Semantic,
    PreserveWhenSafe,
    ExactIfUnchanged,
}

impl DocumentSession {
    pub async fn export(&self, options: ExportRequestOptions)
        -> Result<ExportArtifact, SdkError>;
}
```

- `Semantic` writes only normalized semantics and reports target limitations.
- `PreserveWhenSafe` lets a matching source adapter re-emit validated opaque
  records that remain safe after the edits.
- `ExactIfUnchanged` returns the original byte stream only when the session has
  not changed and the source adapter recorded an exact-reconstruction floor;
  otherwise it fails rather than silently degrading to semantic export.

Exporting to a different format always behaves semantically. The source envelope
may inform the report but is not embedded into the target.

## 10. Package substrate

Introduce `casual-doc-package` as a format-neutral, bounded ZIP container layer:

- central-directory verification;
- normalized safe paths and duplicate rejection;
- overlap, special-entry, compression, expansion-ratio, per-entry, aggregate,
  and entry-count limits;
- cancellation;
- metadata-only probes;
- bounded on-demand part reads;
- deterministic package assembly primitives.

`casual-doc-ooxml` retains OPC content types, relationships, macro policy, and
DOCX profile validation. A new `casual-doc-odf` crate owns ODF `mimetype`,
`META-INF/manifest.xml`, encryption/signature declarations, and ODT profile
validation. Format-specific code must not be pushed into the generic ZIP crate.

ODF encrypted packages are detected but rejected with a typed policy/unsupported
error in the first profile. Signature files are never claimed valid after an
edit; unchanged exact export may retain original signatures only as part of the
unchanged original byte stream.

## 11. Initial format profiles

| Format | Import | Export | Preservation target |
| --- | --- | --- | --- |
| DOCX | Existing semantic importer | Existing semantic writer | Preserve current behavior, then wire the retained-parts sidecar into sessions |
| Normalized JSON | Existing strict, bounded schema-v1 loader | Existing deterministic compact schema-v1 exporter | Exact unchanged bytes when explicitly retained; no opaque semantic sidecar |
| Plain text | Strict, bounded UTF-8 | UTF-8 with deterministic LF newline policy | Exact unchanged bytes when explicitly retained; otherwise semantic only |
| ODT | New ODF 1.2–1.4 text-document importer | New writer; emitted version finalized from interoperability evidence | Unchanged package plus safe edit-tolerant foreign-part preservation |

The ODT first supported surface is paragraphs/spans, named and automatic styles,
lists, tables, page/master-page geometry, headers/footers, notes, hyperlinks,
bookmarks, images/frames, metadata, and common tracked changes. Unsupported ODF
features must be retained or reported under doc 35; they are not silently
flattened.

The normalized JSON probe is definite only when the bytes parse and validate as
a schema-v1 snapshot under the configured snapshot limits. The text probe is
only possible because every normalized JSON snapshot is also UTF-8; therefore a
valid JSON snapshot wins without relying on a filename hint. Explicit selection
still runs the selected adapter's complete validation.

Plain-text import accepts an optional leading UTF-8 BOM, maps CRLF, CR, and LF
to paragraph boundaries, preserves empty and trailing paragraphs, and maps tabs
to explicit tab nodes. Other C0 controls are rejected. IDs are derived
deterministically from canonicalized text and document order. Semantic text
export uses UTF-8 without a BOM and LF line endings. It emits the final revision
projection, cached field text, math fallback text, table cells separated by tabs,
and recursive blocks separated by newlines. Formatting, targets, non-text
objects, definitions, resources, and opaque structures receive bounded,
deterministically ordered compatibility findings when they cannot survive.

## 12. SDK and WASM migration

Compatibility is additive:

- add `Engine::open(OpenRequest)` and `DocumentSession::export(...)`;
- retain `open_docx`/`save_docx` as convenience wrappers for at least two minor
  releases after the generic API becomes stable;
- keep WASM `open(bytes)` but change it to auto-detect supported formats;
- add `openAs(bytes, formatId)` for explicit selection;
- add `sourceFormat`, `availableExportFormats()`, and
  `exportAs(formatId, options)`;
- retain `exportDocx()` as a compatibility wrapper;
- make the webapp's file picker and Save/Save As UI capability-driven rather
  than extension hard-coded.

Format detection and export capabilities belong to the engine. The browser DOM
does not decide format identity from a file-name suffix.

## 13. Security and resource policy

Every adapter must provide tests for:

- malformed input and truncated XML;
- path traversal, duplicate entries, overlapping ZIP entries, and ZIP bombs;
- XML entity/DTD refusal and nesting/attribute/text limits;
- external references without automatic fetching;
- encrypted and signed input policy;
- scripts, macros, embedded executables, and active content;
- cancellation at package and XML traversal boundaries;
- bounded compatibility reporting and preservation storage;
- deterministic behavior under ZIP entry and XML attribute reordering where
  order is not semantically meaningful.

An adapter may impose tighter limits than the global package ceiling. It may not
raise a hard ceiling or bypass host policy.

## 14. Crate and delivery plan

### Slice A — contracts and regression lock

- add format IDs, descriptors, artifacts, registry, deterministic detection,
  and capability queries;
- wrap the existing DOCX path without changing its parsing/writing behavior;
- retain all current DOCX entry points as wrappers;
- add ambiguity, explicit-selection, and registry-order tests.

### Slice B — generic package extraction

- extract the verified ZIP substrate from `casual-doc-ooxml`;
- keep OOXML/OPC admission behavior and errors stable;
- add package-level fuzz/corpus coverage independent of DOCX.

### Slice C — second simple adapters

- route normalized JSON through the registry;
- implement deterministic UTF-8 plain-text import/export;
- use these two different package shapes to prove the contract before ODT.

### Slice D — ODF package and ODT import

- implement ODF package admission and format/version checks;
- map ODT content, styles, page styles, metadata, media, and common review
  constructs into `v1::Document`;
- carry a complete dual-axis compatibility report and preservation envelope.

### Slice E — ODT export and host surfaces

- implement semantic ODT writing and matching-source preservation;
- add native and WASM generic export;
- update web open/save UI and capability messaging.

### Slice F — conformance and production gates

- OASIS Relax NG validation for emitted ODT;
- rights-reviewed multi-producer corpus;
- import determinism, semantic reopen, unchanged reconstruction, and
  edit-save-reopen gates;
- LibreOffice interoperability and deterministic page/render comparisons;
- parser fuzzing, benchmarks, and published limitations.

## 15. Acceptance gates

The architecture was accepted with the four decisions below. A format is
supported only when:

1. detection and explicit selection are deterministic and fail closed;
2. package/XML limits and fuzz gates pass;
3. every traversed construct has a legal doc-35 disposition;
4. supported semantics pass import → export → reopen equivalence;
5. same-format unsupported safe content meets the stated preservation profile;
6. cross-format losses are present in the export report;
7. emitted packages validate against the selected format profile;
8. the public SDK/WASM capability surface reports exactly what is available;
9. documentation and the support matrix state partial features honestly.

## 16. Accepted decisions

1. **D1 — Extensible format IDs:** use a string `FormatId`, not a closed public
   enum.
2. **D2 — Detection policy:** byte/package evidence is authoritative; file names
   and MIME values are hints only.
3. **D3 — Preservation boundary:** keep format-native opaque state in a tagged
   session sidecar and never copy it across formats.
4. **D4 — Delivery order:** land the generic contract and DOCX regression lock,
   validate it with normalized JSON/TXT, then implement ODT.

D1–D4 were accepted by the owner on 2026-08-04.

## 17. Implementation status

Slice A is complete on `feature/multi-format-io`:

- new `casual-doc-io` workspace crate;
- strict extensible `FormatId` and capability descriptors;
- deterministic registry with explicit selection, byte-first probing, stable
  ambiguity results, and hints limited to possible-match tie breaking;
- format-neutral import/export artifacts, resource collection, compatibility
  report, and opaque format-tagged `SourceEnvelope`;
- built-in DOCX adapter delegating to the existing bounded package reader,
  semantic importer, and semantic/preservation writers;
- semantic, preserve-when-safe, and exact-if-unchanged export modes;
- DOCX compatibility entry points elsewhere remain unchanged;
- focused detection/registry/DOCX reopen tests plus workspace format, test,
  strict Clippy, rustdoc, MSRV, and WASM checks.

Slice B is complete on `feature/multi-format-io`:

- `casual-doc-package` owns format-neutral bounded ZIP central-directory
  validation, safe path normalization, deterministic entry metadata, cooperative
  cancellation, and verified on-demand part reads;
- `casual-doc-ooxml` delegates ZIP admission to that substrate while retaining
  OPC discovery, content-type and relationship validation, and macro rejection;
- public DOCX package types and typed error behavior remain compatible;
- independent generic-package regression tests and a dedicated fuzz target cover
  arbitrary ZIP profiles without requiring DOCX structure.

Slice C is complete on `feature/multi-format-io`:

- the built-in registry now exposes DOCX, normalized JSON, and plain-text import
  and export capabilities in stable format-ID order;
- normalized JSON reuses the strict bounded schema-v1 parser and deterministic
  compact serializer, including explicit output-limit validation;
- plain text has bounded UTF-8 admission, deterministic semantic IDs, explicit
  tab and paragraph mapping, canonical LF export, final revision projection,
  exact retained unchanged bytes, and bounded loss reporting;
- JSON's validated definite probe outranks text's possible probe, while explicit
  format selection still performs complete adapter validation;
- stable SDK, WASM, and web generic-format surfaces remain deferred to Slice E.

## 18. Normative references

- OASIS, *OpenDocument Version 1.4, Part 1: Introduction*, OASIS Standard,
  2025-10-06: <https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part1-introduction/OpenDocument-v1.4-os-part1-introduction.html>
- OASIS, *OpenDocument Version 1.4, Part 2: Packages*:
  <https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part2-packages/OpenDocument-v1.4-os-part2-packages.html>
- OASIS, *OpenDocument Version 1.4, Part 3: Schema*:
  <https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part3-schema/OpenDocument-v1.4-os-part3-schema.html>
- OASIS ODF 1.4 Relax NG schemas:
  <https://docs.oasis-open.org/office/OpenDocument/v1.4/os/schemas/>
