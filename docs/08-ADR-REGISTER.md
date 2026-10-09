# Architecture Decision Record Register

## ADR-001 — Rust as the core implementation language

**Decision:** Use Rust for the document runtime.

**Why:** memory safety, native performance, strong cross-platform story, WASM compilation, and suitable abstraction boundaries.

**Consequence:** browser integration requires explicit binary/API boundaries and careful WASM size management.

## ADR-002 — Engine model is independent of the browser DOM

**Decision:** Do not make DOM/contenteditable the source of truth.

**Why:** deterministic pagination, shared native/web behavior, and testable layout.

**Consequence:** the project must implement selection, IME, hit testing, accessibility mapping, and clipboard behavior.

## ADR-003 — Backend-neutral display list

**Decision:** Layout outputs a scene/display list rather than directly painting.

**Why:** multiple renderers, testability, caching, headless use, and web/native reuse.

## ADR-004 — Stable SDK facade over internal crates

**Decision:** Consumers depend on `casual-doc-sdk`, not internal crates.

**Why:** internal evolution without ecosystem breakage.

## ADR-005 — Commands and transactions are the only supported mutation path

**Decision:** No direct public mutable model access.

**Why:** undo, collaboration, validation, events, and deterministic invalidation.

**Made true in practice by ADR-043** (`147`). Until then this was aspirational on the live
path: `casual-doc-wasm` referenced `casual-doc-transaction` zero times and applied
`casual-doc-edit` operations directly against a flat undo stack with no revision chain. The
live path now routes every mutation through one transaction envelope, and an exhaustive
source guard fails the build if a second route appears.

## ADR-006 — Collaboration is adapter-based

**Decision:** Core does not depend on Yjs or a specific CRDT.

**Why:** preserve local-first and embedding freedom.

**Consequence:** transaction and anchor semantics must be designed before collaboration implementation.

## ADR-007 — Preserve unsupported OOXML where safe

**Decision:** Maintain extension bags and package-part preservation.

**Why:** avoid unnecessary data loss.

**Amended by ADR-027:** for imported OOXML document content, preservation is
delivered by the typed preservation ledger and versioned mapping registry in
`34-OOXML-FIDELITY-ARCHITECTURE.md`, not by generic extension bags. Extension
bags may remain for normalized-model schema evolution but do not satisfy OOXML
preservation.

## ADR-008 — Parallel migration, not one-shot rewrite

**Decision:** Existing Casual Docs remains available while the runtime grows.

**Why:** lowers product and engineering risk.

## ADR-009 — Trusted native plugins in v1

**Decision:** Initial plugin system is in-process and trusted.

**Why:** manageable scope and performance.

**Later:** sandboxed WASM plugins.

## ADR-010 — Deterministic configured font set

**Decision:** Fidelity testing and reproducible layout require a declared font environment.

**Why:** system font availability otherwise causes layout variation.

## ADR-011 — Progressive workspace boundaries

**Decision:** Start with `casual-doc-model`, `casual-doc-transaction`, and
`casual-doc-sdk`; add later HLD crates only with their first tested behavior.

**Why:** crate boundaries should represent proven ownership and dependency
direction, not placeholders.

**Consequence:** the HLD remains the target structure, while the physical
workspace grows incrementally.

## ADR-012 — Rust 2024 with an explicit MSRV

**Decision:** Use Rust edition 2024, resolver 3, and MSRV Rust 1.85.0.

**Why:** edition 2024 is the current language baseline and an explicit MSRV makes
consumer compatibility testable.

**Consequence:** raising MSRV requires an ADR update, CI change, and release note.

## ADR-013 — SDK-owned public value objects

**Decision:** The SDK facade defines host-facing IDs, positions, snapshots, and
errors instead of re-exporting internal crate types.

**Why:** internal crates must evolve without making representation details part
of the consumer compatibility contract.

**Consequence:** the facade performs explicit, tested conversions at its
boundary.

## ADR-014 — Grapheme-boundary runtime positions

**Decision:** Runtime text positions are local extended-grapheme boundaries plus
affinity, not UTF-8 byte offsets or global UTF-16 indexes.

**Why:** caret and selection behavior must not split user-perceived characters.

**Consequence:** import/export and language bindings must convert their native
offset conventions explicitly.

## ADR-015 — Stable string error registry

**Decision:** Public errors use non-recycled `ODC-NNNN` string codes with
severity and redacted structured context.

**Why:** string codes remain stable across Rust, WASM, C ABI, logs, and support
workflows.

**Consequence:** internal error variants are mapped at the SDK boundary and
cannot leak as a public compatibility contract.

## ADR-016 — Bounded parsing is mandatory

**Decision:** All format parsers enforce configured defaults and non-bypassable
hard ceilings before or during resource consumption.

**Why:** document packages, XML, images, fonts, and extension payloads are
untrusted input.

**Consequence:** parser implementations must expose limit accounting and
boundary tests from their first merge.

## ADR-017 — Semantic inverse operations

**Decision:** Transactions generate operation-level inverses against a working
document; session history stores forward/inverse operation lists.

**Why:** undo must preserve exact marked content without retaining a full
document snapshot for every edit.

**Consequence:** every new mutating operation must define mapping and inverse
behavior before implementation.

## ADR-018 — Strict bounded normalized JSON v0

**Decision:** Normalized JSON loading rejects unknown fields, validates a
pre-parse byte limit and post-parse semantic limits, and returns a session only
after full invariant validation.

**Why:** generic deserialization is not a sufficient security or compatibility
boundary.

**Consequence:** schema evolution requires an explicit versioned migration path;
v0 does not silently accept future fields.

## ADR-019 — Selection is mapped session state

**Decision:** Canonical logical selection lives in the session, is validated
against the normalized document, and is mapped atomically through every
transaction without incrementing document revision for selection-only changes.

**Why:** commands, history, IME, hit testing, and collaboration need one
engine-owned selection contract independent of host UI.

**Consequence:** selection is not part of normalized document serialization and
the session commit path must validate mapped endpoints before publication.

## ADR-020 — Events begin as a bounded session journal

**Decision:** Record SDK-owned events in a bounded, sequence-ordered session
journal and expose synchronous future-only polling before adding callback,
async, or language-specific bridges.

**Why:** one canonical journal makes mutation ordering, lag detection, memory
bounds, and callback lock safety explicit across every future transport.

**Consequence:** slow consumers receive an exact dropped-event count and must
refresh snapshots; the Phase 0 journal retains the latest 256 events.

## ADR-021 — Pin a minimal ZIP profile at the project MSRV

**Decision:** Build the package reader on exactly `zip` 7.2.0 with default
features disabled and only stored/Deflate DOCX input enabled.

**Why:** `zip` 7.2.0 supports the project's Rust 1.85 MSRV; encryption and
unrelated codecs increase dependency and attack surface without helping DOCX
compatibility. This text is corrected to match the accepted locked dependency.

**Consequence:** upgrades require MSRV, WASM, license, advisory, malformed-input,
and corpus review; OpenDoc still enforces its own path and expansion limits.

## ADR-022 — Separate benchmark smoke from regression gates

**Decision:** Run deterministic release-mode benchmark smoke on every pull
request, but compare wall-clock performance only on a named controlled
environment.

**Why:** shared hosted runners can prove workload and report correctness but do
not provide stable enough timing for a production regression gate.

**Consequence:** baseline reports carry explicit environment identity, and a
future dedicated-runner workflow is required before timing regressions become a
blocking repository check.

## ADR-023 — Align baseline evidence with capability ownership

**Decision:** Phase 0 establishes baseline schemas, policies, implemented-path
reports, and readiness status. Visual baselines begin with the Phase 1D
renderer; semantic DOCX round-trip baselines begin with the Phase 2 writer.

**Why:** a visual snapshot without an OpenDoc renderer and a round trip without
an importer/writer are placeholder artifacts, not compatibility evidence.

**Consequence:** the Phase 0 exit report names those later owners explicitly and
cannot imply layout, rendering, or save support.

**Update (2026-07):** the semantic writer was delivered as Phase 1B ahead of
typography; semantic DOCX round-trip baselines therefore begin with the Phase 1B
writer, and typography/pagination/renderer renumber to 1C/1D/1E respectively. See
the tracker (`14-EXECUTION-TRACKER.md`) and README roadmap.

## ADR-024 — Isolate and continuously build parser fuzz targets

**Decision:** Keep `cargo-fuzz` targets in an independently locked `fuzz/`
workspace, compile them on pull requests, and execute bounded seeded campaigns
in scheduled security CI.

**Why:** parser fuzz dependencies require nightly instrumentation and do not
belong in the product/MSRV graph, while build-only review gates avoid random
pull-request failures.

**Consequence:** fuzz crashes become minimized regression fixtures; scheduled
campaign limits and dependency pins are reviewed repository policy.

## ADR-025 — Decompose the read-only runtime into capability gates

**Decision:** Replace the monolithic Phase 1 with Phase 1A semantic DOCX import,
Phase 1B typography and paragraph layout, Phase 1C pagination and display list,
and Phase 1D renderer and hit testing.

**Why:** OOXML semantics, typography, pagination, and rendering have different
failure modes, dependencies, fixtures, and evidence. Combining them hides
causes, encourages placeholder integration, and makes compatibility claims
ambiguous.

**Consequence:** each stage has an independent exit report. Phase 1A cannot
claim visual support, and UI or Tauri work cannot begin merely because semantic
import succeeds.

**Update (2026-07):** the semantic writer was delivered as Phase 1B ahead of
typography; typography/pagination/renderer renumber to 1C/1D/1E respectively. See
the tracker (`14-EXECUTION-TRACKER.md`) and README roadmap.

## ADR-026 — License the whole project under Apache-2.0

**Decision:** License all repository-owned source, documentation, generated
fixtures, and accepted contributions under Apache License 2.0, expressed with
the SPDX identifier `Apache-2.0`.

**Why:** Apache-2.0 provides explicit copyright and patent grants, patent
termination terms, and established redistribution conditions appropriate for a
widely embedded SDK. The policy is being established before outside
contributions or public package releases.

**Consequence:** package metadata, fixture provenance, contribution terms, and
public documentation must consistently name Apache-2.0. Contributions are
accepted under the same terms unless explicitly agreed otherwise.

## ADR-027 — OOXML fidelity: normalized model + bounded source artifacts

**Decision:** Accepted 2026-07-24. Use a normalized OpenDoc model as the runtime
source of truth, paired with a bounded immutable DOCX source snapshot, a
provenance map, and a typed preservation ledger, all owned by one versioned
import/export mapping registry. See `34-OOXML-FIDELITY-ARCHITECTURE.md` and the
signed acceptance record `36-ADR-027-ACCEPTANCE-RECORD.md` (decisions D1–D11;
reconciliations R1/R2/R4 resolved, R3 an open implementation task).

**Why:** a WYSIWYG runtime needs editor-oriented semantics while production DOCX
compatibility needs source distinctions and unsupported-but-safe content that
normalization cannot represent; designing both directions before import prevents
irreversible fidelity loss. Grounded in `33-` and `37-` competitor research.

**Consequence:** amends ADR-007 (typed ledger + registry supersede generic
extension bags for OOXML); adopts the dual-axis disposition taxonomy
(`35-DISPOSITION-TAXONOMY.md`); provenance offset spans use grapheme units
(ADR-014). Acceptance is architecture-level and does not skip the schema-v1 and
artifact-schema deliverables that gate importer code.

## ADR-028 — Streaming, namespace-aware XML parser dependency

**Decision:** Accepted 2026-07-24. Adopt `quick-xml` as the OOXML XML reader for
the DOCX read path, used in streaming (`Reader`) mode only.

**Why:** relationship and document parsing need a bounded, namespace-aware,
pull-based reader. `quick-xml` is pure-Rust, `#![forbid(unsafe)]`-compatible in
our usage, has no proc-macro or network dependencies, builds on
`wasm32-unknown-unknown`, and does not resolve DTDs or external entities.

**Security requirements (mandatory):** no DTD processing, no entity/parameter
expansion, no external-entity or network resolution; namespace-aware; depth-,
count-, and byte-bounded via `21-PARSER-LIMITS.md`; cancellable; retained XML is
treated as data and never reparsed under weaker settings. Added under the
dependency policy in `deny.toml`.

**Consequence:** satisfies the "separate dependency ADR before implementation"
gate in docs 32 and 34; the parser is wrapped behind an internal bounded reader
so limits and entity-disabling are enforced in one place.

## ADR-029 — Raise the MSRV to Rust 1.88 for the Phase 1C text stack

**Decision:** Accepted 2026-07-26. Raise the workspace (and `fuzz/` workspace)
Minimum Supported Rust Version from 1.85.0 to **1.88.0**, and update the CI MSRV
job to 1.88.0.

**Why:** the accepted Phase 1C layout/rendering design
(`43-PHASE-1C-LAYOUT-RENDERING-DESIGN.md`, research in `42-…`) adopts the modern
Rust text stack — `parley` (line layout), `fontique` (font enumeration/fallback),
`harfrust` (shaping), `skrifa`, and `icu4x` 2.x (Unicode segmentation/bidi).
`parley` 0.11 and `fontique` 0.11 require Rust 1.88; `icu4x` 2.x requires 1.86.
Pinning back to `parley` 0.7 to preserve MSRV 1.85 would start a production,
Word-grade engine on a year-old text stack with weaker complex-script support —
a poor long-term trade for a subsystem the product leans on heavily. Rust 1.88
(released mid-2025) is well-established.

**Consequence:** satisfies the support-matrix rule that "the MSRV may only be
raised through an ADR and a documented release note" (`18-SUPPORT-MATRIX.md`).
The MSRV CI gate now checks 1.88.0; `06`/`15`/`18` are updated. No existing code
changes (raising the MSRV never breaks lower-version-compatible code); the bump
is a prerequisite landed ahead of the `parley` line-shaper slice (P1C-001).

## ADR-030 — Reserve the extensibility seams for collaboration & agentic layers

**Decision:** Accepted 2026-07-26. Commit to four architectural invariants —
(I1) a single mutation choke point through the operation channel, (I2) a closed,
serializable, composable + invertible operation set, (I3) `NodeId`-based anchors
with position math confined to `ModelPos`, and (I4) AI/derived data in a sidecar
store keyed by `NodeId`, never in the `v1` model — so that concurrent editing
(OT **or** CRDT), MCP/agent-driven editing, and RAG/vector retrieval can each be
added later **as adapters, without a core rewrite**. Full rationale and the
per-layer landing plan: `45-EXTENSIBILITY-AND-COLLABORATION-SEAMS.md`.

**Why:** a remote collaborator, an OT/CRDT peer, and an AI agent all *observe then
apply operations* — one clean operation channel + observation stream + stable
anchors future-proofs all three at once. The invariants are cheap to hold now (a
review-checklist item) and expensive to retrofit later; the only real risk is
integer-offset assumptions leaking out of `ModelPos` (which I3 prevents).

**Consequence:** the four invariants become a review checklist applied to every
model/mutation PR (see doc 45 §"Review checklist"), so the in-flight Phase-1F
construct-family work reinforces the seams. This ADR does **not** choose OT vs CRDT
(still pending below); it guarantees either remains additive.

## ADR-031 — PDF export backend and writer/subsetter build-vs-buy

**Decision:** *Accepted for Phase 0; build-vs-buy resolved as hand-rolled.* A
`casual-doc-pdf` backend transcribes the shared `DisplayList` (ADR-003) into a real,
deterministic PDF with a selectable text layer and embedded **subset** fonts — never
rasterized pages or outlined text. Editor↔PDF parity is guaranteed structurally (the
exporter reuses the editor's layout pass verbatim and performs no layout of its own).
Word parity is *feature* parity (outline/links/metadata, then tagged PDF / PDF-A),
delivered in phases; layout fidelity to Word is inherited from the layout engine, not
re-implemented here.

The two open sub-decisions are now settled: **(a)** the PDF container writer and
**(b)** the TrueType font subsetter are both **hand-rolled**, adding no crate to
`Cargo.lock`. The subsetter is small because it never renumbers glyphs — `Identity-H`
plus `/CIDToGIDMap /Identity` lets it keep the glyph identity space and simply empty
unused outlines, which is also what removes the "PDF drew a different glyph than the
shaper chose" class of defect. CFF outlines are **not** subset; such a face is embedded
whole and the export reports it. Three crates already in the tree are reused rather than
added: `flate2`, `skrifa` and `image`. Full design, current support matrix and remaining
work: `98-PDF-EXPORT-AND-PRINT-DESIGN.md` §"Implementation status".

**Why:** the display-list seam already makes a PDF backend additive; the only open
questions are the dependency posture (writer/subsetter) and the Phase-2 scope gate
(tagged PDF / PDF-A). Recording them here keeps the "PDF generation backend" pending
item from being decided implicitly in code.

**Consequence:** Phase 0 is implemented and registered in `casual-doc-io` as the
export-only `application.pdf` adapter, so Rust and browser hosts reach it through the
same registry/WASM seam as DOCX and ODT; the web File surface exposes it as Export as
PDF. Physical Print remains a separate raster path. The Phase-2 accessibility/archival gate remains a separate
in/out decision because it ≈ doubles the effort and shapes the semantic `StructureTree`
from day one — nothing in Phase 0 forecloses it. Phase 1's semantic features (links,
outline, destinations) stay blocked on that `StructureTree` bridge, which layout does not
emit yet.

## ADR-032 — Keep `opt-level = 3`; buy WASM load time with delivery, not codegen

**Decision:** Leave `[profile.release] opt-level` at its default (3). Do not trade
engine speed for WASM bytes. Load time is bought at the delivery layer instead —
paint from the bundled metric-compatible faces before the ~9.5 MB of named web fonts
arrive, preload the engine at parse time, and warm it from the site on intent.

**Why:** measured, not assumed. Against `webapp/pkg/casual_doc_wasm_bg.wasm` (18.9 MB
raw / 8.58 MB gzipped at the time of writing), the codegen levers buy little and cost
a great deal:

| build | raw | gzip | typing 100 graphemes | model load 100 paragraphs |
|---|---|---|---|---|
| `opt-level = 3` (current) | 18.9 MB | 8.58 MB | baseline | baseline |
| `wasm-opt -Oz` only | 18.8 MB | 8.58 MB | not measured | not measured |
| `opt-level = "s"` | 16.9 MB | 8.12 MB | **+29%** | **+19%** |
| `opt-level = "z"` | 16.0 MB | 7.85 MB | **+78%** | **+110%** |

(`opendoc-benchmark`, 15 samples per case, run-to-run noise +/-2%.) The best case saves
0.73 MB gzipped — about 8% of the engine, and under 4% of what the editor used to
transfer before first paint — for typing that is measurably slower on every keystroke.
`opt-level` is also workspace-wide, so it would compile the layout and render hot
paths for size in native builds too, where the download does not exist at all.
`wasm-opt -Oz` is separately not worth configuring: it produced no gain over `-O`.

**Consequence:** WASM size stays a delivery and dependency-footprint problem. Real
reductions must come from what is compiled in (feature-trimming heavy dependencies,
splitting rarely-used format support out of the first payload), not from asking the
compiler to pessimize the hot paths. Re-open only with numbers from the same
benchmark, and only for a lever that does not tax runtime.

## ADR-033 — Collaboration is operational transformation over the closed op set, with snapshot/replay versioning

**Status:** Proposed (owner decision taken 2026-09-15). Designed in
`107-COLLABORATION-OT-SNAPSHOT-REPLAY-DESIGN.md`; the product, restore, checkpoint,
and diff contracts are refined by docs 139–140. The provider/embed architecture
and phase/evidence checklist are docs 143–144. No real-time collaboration support
is implemented or implied while this ADR remains proposed.

**Decision:** Collaborative editing uses **operational transformation**, carried by
`casual-doc-transaction` transactions over the closed `casual-doc-edit` operation set, with a
durable ordered operation log. **Versioning is snapshot-plus-replay** over that log: a
validated checkpoint plus the operations after it reconstitutes any revision. The
normalized snapshot (`25`) is the semantic replay projection, but doc 112 proved it cannot
be the sole restore artifact because it omits binary resources and retained source data;
restorable versions therefore carry the fidelity-complete source-format artifact specified
by doc 140. Operations are transformed only against a **totally ordered** log supplied by
an optional relay.

**Why OT rather than the cheaper alternative.** The competitor this project is positioned
against (`106`) uses neither OT nor a CRDT but a server-ordered change log plus pessimistic
object locks, so matching it does not require OT. OT is chosen anyway because:

- the hard prerequisite is already paid — the op set is closed (invariant I2) and **every
  operation already returns its inverse**;
- `PositionMap::map` already performs affinity-correct position transform;
- pessimistic locks refuse the second writer and need an online lock authority, which is
  structurally hostile to local-first operation;
- OT makes offline-then-merge possible, and makes version history, restore, and document
  compare/combine consequences of one mechanism rather than three separate features.

**Tractability over 47 operations.** Operations are classified in three tiers: **T1
positional** (~9 text/inline ops — full pairwise transform, the hot path); **T2
node-addressed** (the bulk — addressed by `NodeId` per invariant I3, so they need anchor
liveness plus a tombstone rule, not offset math); **T3 document-scope** (serialised,
last-writer-wins). A tombstoned operation is **reported through the disposition taxonomy**
(`35`), never silently dropped.

**Consequences:**

- The closed op set becomes load-bearing twice — undo *and* transform. Adding an operation
  without a tier classification and transform rules must fail CI.
- The operation log becomes persisted, versioned data, so the operation vocabulary becomes a
  compatibility surface needing a schema-version policy.
- TP1 convergence must be **proven by property test**, not asserted. TP2 is avoided by
  construction via the totally ordered log.
- The relay orders and fans out operations only. It does not parse documents, convert
  formats, hold the authoritative model, or gate single-user editing — **no mandatory server**
  (ADR-006 unchanged; invariants I1/I4).
- **Editing must stay light** (owner constraint): `107` §4 states seven measurable budgets —
  per-keystroke work O(1) in document size, transform cost O(concurrent ops since base),
  typing coalesced per run, no paragraph-rewrite op on the typing path, periodic not
  per-operation snapshots, incremental invalidation for remote operations, and a bounded log.
- **Prerequisite — now met on the live path by ADR-043 (`147`).** The live editing path used
  to reference `casual-doc-transaction` zero times and apply `casual-doc-edit` operations
  directly, so ADR-005 was not honoured in practice and there were two parallel operation
  sets. `147` unified them: the envelope is re-founded on `v1::Document` and the
  `casual-doc-edit` op set, every WASM mutation is a transaction appended to an ordered
  revision log, and undo is derived from that log. The Phase-0 **v0** stack
  (`casual-doc-sdk`, `casual-doc-selection::TextSelection`) is deliberately left behind the
  boundary `147` §6 names; it edits a model no product surface renders, and retiring it is
  `126`'s v1 SDK work.
- A CRDT adapter remains possible later behind the same seam for peer-to-peer or
  partition-tolerant merge, which relay-ordered OT deliberately does not attempt.

## ADR-034 — Experimental local PDF semantic reconstruction

**Status:** Proposed experimental future feature; not accepted, implemented, supported,
or part of the current v1 commitment. Designed in
`131-PDF-SEMANTIC-RECONSTRUCTION-AND-BROWSER-OCR-ARCHITECTURE.md`.

**Proposed decision:** If the experiment later graduates, import PDFs through a staged,
browser-local reconstruction path. Native PDF evidence is extracted before selective
OCR; providers emit a strict bounded `PdfEvidenceV1` stream; Rust/WASM alone validates
that evidence and creates the normalized `v1::Document`, source envelope, and
compatibility report. OCR providers never emit DOCX or bypass the normalized model.
Any future graduated release baseline has no remote document processing and no silent
cloud fallback.

**Why proposed:** PDF is a fixed-layout format and often lacks editable authoring
semantics. Treating OCR Markdown or a third-party converted DOCX as truth would bypass
the repository's normalized-model, loss-reporting, security, and determinism
invariants. A staged evidence boundary can reuse the existing model and writers while
keeping provider-specific inference outside editor state.

**Proposed consequence:** the synchronous `FormatImporter` contract remains unchanged
for current formats; a separate staged capability sits behind the target async SDK open
surface. PDF parsing/OCR runs in bounded Workers, model weights remain outside the live
OpenDoc WASM memory, and commit is atomic after schema validation. Doc 98's PDF-import
non-goal and `PdfAdapter::can_import == false` remain authoritative until this ADR is
accepted and the experimental graduation gates in doc 131 pass. PDF.js,
PaddleOCR.js/ONNX Runtime Web, Tesseract.js, OvisOCR2, and UnlimitedOCR are research
candidates only; this proposed ADR accepts no dependency or model.

## ADR-035 — Experimental document assistance, semantic retrieval, and MCP adapter

**Status:** Proposed experimental future feature; not accepted, implemented, supported,
or part of the current v1 commitment. Designed in
`132-EXPERIMENTAL-DOCUMENT-ASSISTANCE-SEMANTIC-SEARCH-AND-MCP-ARCHITECTURE.md`.

**Proposed decision:** If this experiment later graduates, make user scenarios—not
MCP tools or a particular model—the product boundary. A shared Document Assistance
Layer resolves explicit document scope, gathers bounded structured context, invokes a
host-approved provider, and returns a reviewable result or typed change proposal.
Accepted mutations pass through the normal command/transaction path as one undoable
unit. Keep embeddings, chunks, summaries, provider data, and unaccepted proposals in a
rebuildable `NodeId`-anchored sidecar. MCP is an optional external adapter over these
same services; it is neither a core dependency nor an alternate mutation path.

**Why proposed:** translation, rewriting, formatting, text/table transformation,
summarization, semantic search, and agent workflows share scope, context, policy,
review, and commit requirements. Designing those services once prevents the embedded
assistant, host SDK, and MCP server from acquiring incompatible behavior or bypassing
document-safety guarantees. Browser-local retrieval is feasible behind Worker and
provider boundaries, but no single embedding or generation model fits every browser,
language, document size, and host policy.

**Proposed consequence:** PDF/OCR remains a separate import architecture under doc
131 and ADR-034. No model, inference runtime, vector database, MCP package, network
service, or browser companion is selected by this ADR. Graduation requires the DAI-0
owner decisions and the privacy, injection-resistance, stale-proposal, determinism,
retrieval-quality, resource, cross-browser, conformance, and approval gates in docs
132 and 15. Until then, older MCP/package language in doc 83 is aspirational only.

## ADR-036 — The host contract is one schema with two transports, gated as a fourth door

**Status:** Accepted and implemented (`docs/126` phase 2). Schema:
`webapp/src/host_contract.mjs`; Rust declaration: `crates/casual-doc-sdk/src/host.rs`.
Design: `125-EMBED-AND-HOST-CONTRACT-DESIGN.md`, `05-SDK-API-SPEC.md` §§4-8.

**Decision:** A host commands the editor and hears about it through **one schema**, from
which both transports are built: the in-process session a host holds a reference to, and the
`postMessage` bridge. The bridge is an envelope plus an origin check over the in-process
session — not a second implementation — so a verb, a command, an event or a refusal code
cannot exist on one transport and be missing from the other. Five decisions follow from it:

1. **A command id is the addressing unit.** The editor's registry is already reached by id
   (`runCommandById`, `keymap.mjs`, 111 ribbon controls with `data-command`), so a host uses
   the same ids a person's keyboard does. Value commands whose members are generated from the
   document, the font inventory or markup (measured: 93 of 214 ids on the `rich` fixture) are
   declared as **family prefixes**, the same two kinds `ribbon_faces.mjs` distinguishes —
   never a third vocabulary.
2. **Capabilities gate the API, not just the chrome.** The gate runs BEFORE dispatch, so a
   host without `edit` never reaches a command's `run`. This is the **fourth** enforcement
   layer after the browser sandbox, the engine's Viewing choke point and the chrome's
   disabled-with-a-reason, and it must not be the unlocked one — `109` HF-632 was exactly that
   lie one layer down (a `readonly` embed offered Save, ran it, no download fired, and the
   status line said "Saved").
3. **A refusal is a VALUE, not only a toast.** `execute` resolves `{ok: false, refusal}` with
   a machine-readable `code`; the same refusal is also emitted as an event, from one place, so
   a host that awaits and a host that listens learn the same thing. A host that cannot hear a
   refusal re-issues it forever.
4. **`postMessage` never posts to a wildcard.** The editor's own origin is always accepted;
   additional origins are named explicitly by the deployment (`?hostOrigin=`); `*` and `null`
   are rejected rather than honoured, so a typo narrows the allowlist. An unknown origin gets
   no reply at all — an error reply tells a prober what the frame is. (ONLYOFFICE posts
   everything to `"*"` under a literal `// TODO: specify explicit origin`, and has no
   correlation id, so their host cannot learn whether a command was refused.)
5. **Events carry handles, never documents.** `change` carries a revision integer and a dirty
   flag; `save`/`export` carry a format, a name and a byte COUNT. Per-interaction work stays
   O(1) in document size (`docs/107` §4), and a host that wants bytes asks for them.

**Deliberate deviation from `docs/125` §8.** That section sketched a ten-event set and said
host events would be `casual_doc_sdk::RuntimeEvent`-derived with `ErrorCode` as the refusal
vocabulary. What shipped is `docs/126`'s named minimum — `ready`, `change`, `selection`,
`save`, `export`, `error`, `refusal` — because that is the owner's own list and it is what a
host needs to drive an editor rather than a transaction log. The refusal codes are **not**
`ErrorCode` in different clothes: they answer why a *host command* was refused
(`unknown-command`, `capability-withheld`, `unavailable`, `engine-refused`, `threw`,
`bad-request`, `timeout`), where `ErrorCode` answers how an engine operation failed. The two
are related by `HostRefusal::for_error`, which maps every `ErrorCode` onto one of them, so a
native host and a browser host branch on one vocabulary.

**The Rust facade declares the vocabulary and does not run the editor.** `casual-doc-sdk`
carries the events, refusal codes, verbs and version, `HostEvent::from(&RuntimeEvent)` is the
derivation `125` §8 asked for, and `host_parity.rs` reads the editor's schema and fails in
both directions. Convergence of the runtime is **not** in this ADR: the live editing path
applies `casual-doc-edit`'s ops directly and references `casual_doc_transaction` zero times
(`109` CQ-002, `docs/125` §9 row 5, graded **L**), and that unification is ADR-005's debt,
owed before OT (ADR-033) regardless.

**Consequences:**

- Adding a command to the editor without declaring it — or declaring one the editor does not
  offer — fails the build, in three states a family needs (caret, table, object).
- A command declared ungated is checked against the ENGINE (run as `owner`, revision and
  counts must not move), because every other assertion reads `requires` from the contract and
  so cannot see a mis-declaration.
- The contract version is a compatibility surface: additive changes (a command, an event, a
  refusal code) do not bump it; a changed requirement, a lost payload field or a new envelope
  shape does.
- Three commands (`file.print`, `file.open`, `insert.image`) hand control to the operating
  system and are exercised with `query` rather than `execute`, with a coverage assertion so a
  skip cannot outlive its command.
- No engine operation was added (ADR-030 I2): the contract composes existing registry
  commands.

## ADR-037 — Break insertion adds exactly one operation, for creating a section boundary

**Status:** Accepted and implemented (`docs/130` §4.3, OO-022). Design and the full
inheritance table: the module header of `crates/casual-doc-edit/src/breaks.rs`, which is where
it stays current; operation: `Operation::SpliceSectionBoundary`.

**Context:** `docs/130` ranked "insert a page, column or section break" first among the
ONLYOFFICE toolbar gaps and classified it **engine** — "there is no break-insertion
operation". ADR-030 invariant I2 keeps the op set closed, so the question was how much of it
genuinely needed a new operation. Captions and the paragraph-spanning field each landed with
none; #635 added two and had to argue for them.

**Decision:**

1. **Page and column breaks add nothing.** `w:br` is an inline node, so authoring one is the
   existing `InsertInlineObject` / `RemoveInlineObject` pair that Shift+Enter already uses,
   and layout has always paginated an *imported* page break. The gap was authoring, not
   capability.
2. **A section break adds exactly one operation**, `SpliceSectionBoundary`, because every
   other `SetSection*` operation edits a boundary that already exists: a section could be
   reformatted and never created. It is **one** variant rather than an insert/remove pair
   because `Some`/`None` makes it its own inverse in both directions — the shape
   `SetStyleDefinition` already uses for the style registry — and it anchors on `SectionId`,
   never on a list index, so the anchor is a node id like every other operation's (doc 45 I3).
3. **The caret's section keeps its identity and keeps the content before the break**; the
   content after it becomes the new section. This is what holds the addition to one operation:
   the existing boundary is never rewritten, so nothing has to set its start type. Per
   ECMA-376 a boundary's `w:type` says how *that* section starts, which is also how
   `flow::section_break_forces_page` already reads it, so the kind the user picks belongs on
   the new, second section.
4. **The new boundary inherits everything except four fields** — a fresh `id`, the chosen
   `section_type`, a cleared `page_numbering.start` (it means *restart numbering here*;
   copying it would silently renumber every page after the break) and a cleared
   `section_change` (a tracked format change recorded against the section it sits on; a copy
   would fabricate a second identical revision). Identical geometry on both sides is the
   point: a break moves the following text onto the page its start type asks for and nowhere
   else.
5. **Where a break cannot be honoured it refuses with a reason.** Layout charges a section
   break to a section only on a top-level body paragraph and only the body paginator consumes
   a forced break, so a table cell, text box, content control, header/footer, note or comment
   each get their own sentence rather than a stored break nothing reads. Two of those —
   a table cell and a body-level content control — are **stricter than Word**, deliberately
   and recorded in the code.

**Consequences:**

- The closed set is 51 variants. A `SpliceSectionBoundary` transforms as an ordered-list
  insert/remove anchored on a section id, which is what OT (ADR-033) will need of it.
- The whole section split is three operations in one `apply_action_caret_as` group, so it is
  one history entry and undoes in one step. A break that undid in two would be a defect.
- A document that declares no `w:sectPr` at all — a plain-text or JSON import — has no section
  to split and refuses with its own reason. That is consistent with `setPageSetup`, which is
  equally unavailable there because it needs an existing section id; materialising the implicit
  body section is separate work.
- `Blank Page` is no longer blocked: it is two page breaks, and needs no engine work.

## ADR-038 — Durable version history lives on the draft seam; count and bytes are ceilings, age is a window, and a named version is never pruned

**Status:** Accepted; storage, policy and restore implemented, **not yet reachable from the
product** (the panel, the menu entries and the catalogue strings are a separate lane).
Requirements: `docs/139`. Architecture: `docs/140`. Rows: `104` HF-068 / `105` OO-004.
Code: `webapp/src/version_history.mjs`, schema v3 in `webapp/src/drafts.mjs`, defaults in
`webapp/src/settings_defaults.mjs`.

**Context:** `docs/139` and `docs/140` settle the shape of version history and deliberately
leave ten product and twelve architecture questions open. The owner then ruled on the two
that block a first implementation: history is **client-side, on IndexedDB, built on the
autosave path — no server, no relay, no collaboration**, and retention is **configurable, in
config or settings, "around 20-30" versions or "retain for 7 days"**. That ruling names both
a count and an age and does not say what happens when they disagree, which is the question
this ADR exists to answer. `docs/140` §13 meanwhile said retention works from a byte budget
"**not** a raw version count"; the owner's ruling overrides that sentence, and it has been
corrected rather than left to contradict the shipped policy.

**Decision:**

1. **One store, schema v3.** Four object stores — `documents`, `version_meta`,
   `checkpoint_blobs`, `history_ops` — are added to the existing `opendoc-drafts` database,
   as HF-068's decision cell requires ("same store as HF-011 — not a second store"). The
   upgrade is additive (`if (!contains)`), so a version-1 or version-2 database comes through
   it with every draft and every personal-dictionary word intact. A **pin is a boolean on the
   version row**, not the separate `pins` store `docs/140` §7.1 sketched: the pin protects
   exactly that row, and a second store would be a second thing to keep consistent with it.
2. **A checkpoint is the artifact autosave already produces.** The draft path exports the
   document in its own source format through Save's export ladder, because `docs/112` §3.2
   measured that a normalized-JSON snapshot drops `binary_resources` and the retained
   `source_envelope` — every picture gone. Version capture therefore takes **the same bytes**
   rather than exporting a second time: one document-sized cost per capture instead of two,
   one format decision in the codebase instead of two, and byte-for-byte what Save would have
   written. Artifacts are **content-addressed by SHA-256**, so a Save followed by *Name this
   version* costs one ~300-byte row and no second copy of the document.
3. **Count and bytes are ceilings; age is a window with a floor.** This is the answer to the
   disagreement the owner's ruling left open, and each half has a different job:
   - the **count** (default 25, inside the owner's 20–30 band) and the **byte budget**
     (default 120 MB) bound storage. They always apply and they prune the oldest eligible
     version first, because a ceiling that can be talked out of applying is not a bound;
   - the **age window** (default 7 days) bounds staleness and privacy — history holds content
     the user deliberately deleted (`docs/139` §12) — but it is *a promise to keep versions
     for at least seven days*, never an instruction to delete on day eight. The newest
     `keepFloor` versions (default 3) survive it regardless of age, so a document nobody has
     touched for a fortnight still has a past. Age-only pruning would empty the timeline of
     an untouched document, which is the opposite of retaining it.
4. **A named version is never pruned by automatic retention.** `docs/139` §8.7 and VH-006
   already say so, and Google Docs prunes unnamed history while keeping named versions. When
   a ceiling cannot be met without deleting a pin, the capture is **refused and reported**
   (`history.fullPinned`, published as a refusal) rather than the pin being deleted "with
   disclosure". A **pin limit** of 15 against a count cap of 25 is what keeps that refusal
   rare: pins can never fill the store, so ten slots always remain for automatic capture.
   This also answers `docs/139` §18 question 5 — yes, named versions get a count limit in
   addition to the byte ceiling, and the limit exists to protect the refusal from being the
   normal case.
5. ~~**An explicit Save always creates a version**~~ — **REVERSED by the owner on 2026-09-28:
   a capture with nothing new in it is suppressed** (`docs/139` §18 question 3). The original
   decision reasoned from storage — a no-change Save costs a row, not a document — and storage
   was never what was wrong with it. The owner's words are *"version should not be logged if
   nothing has changed"*, and the cost is to the timeline: opening a document laid down an
   `import` entry identical to the head and saving an unmodified one laid a `saved` entry beside
   it, so a reader got rows they cannot tell apart or act on, and real versions were pushed out
   of a 25-row budget by them.
   A capture whose artifact is byte-identical to the lineage head now reports `history.unchanged`
   and writes nothing. It is a **per-reason** decision: `open`, `save` and autosave's four
   triggers are suppressed, because all of them are implicit; `name` and `manual` are not,
   because they are explicit user acts and a command that appears to do nothing is the worse
   failure; and `pre_restore`, `restore` and `recovery` are not, because they are integrity
   captures and decision 9 below states the pre-restore invariant over a *record existing*.
   The comparison is content addressing doing the job it was built for — the SHA-256 checkpoint
   id the capture has already computed, against the one the head already stores, inside the
   transaction that is already reading those rows. Nothing is hashed, read or walked twice, and
   `shouldCapture` (decision 10) is untouched and still O(1) with no storage access: a content
   question needs bytes and the editing path may not spend them.
6. **History is on by default and tied to autosave** (`docs/139` §18 question 2), for
   autosave's own reason: a document-safety net nobody switches on is not one. Turning
   autosave off turns history off with it — one switch must not promise what the other has
   stopped doing.
7. **A version can be pinned without being named** (`docs/139` §18 question 4). The label is
   what makes a version findable; the pin is what makes it durable, and they are separable.
8. **Lineage identity is minted, and `documentKey` is only a rejoin hint.** `docs/140` §4.1
   forbids deriving identity from a filename, a byte hash or a timestamp. A reopened file
   finds its history through the bounded `documentKey` sample hash plus a name match; a hint
   collision can only join two timelines that should have been separate — confusing, never
   lossy — and *Make a copy* mints a new lineage explicitly.
9. **Restore is prepare-then-commit, and the pre-restore checkpoint is a precondition.** The
   current state becomes a version **before** the head moves, and if that capture is refused
   the restore is refused with it: there is no state in which the work is in neither place,
   which is what `docs/112`'s own history means by *never let a restore leave the work in
   neither place*. The commit is one IndexedDB transaction that compare-and-sets the head, so
   an interrupted restore leaves the complete old head with a prepared record that the next
   boot resolves by reading which head actually committed — never by guessing from timestamps.
   An idempotency key makes a retried restore return the first result instead of restoring
   twice. Nothing is deleted to make a restore possible.
10. **The capture decision is O(1) and touches no storage.** Autosave's contribution to
    history is `VersionCapturePolicy.shouldCapture`: three comparisons over two numbers it
    holds itself. A version is laid down at most once per `versionIntervalMinutes` (default
    10) and only when the engine revision watermark has moved, so autosave never becomes
    O(versions) and a 5-second quiesce cadence cannot spend a 25-version budget in two
    minutes.

**Consequences:**

- Storage cost is explicit and bounded: worst case at the defaults is 25 × the document's own
  Save artifact, capped at 120 MB, inside an origin quota shared with drafts. `docs/112`
  measured that artifact at 1.0 MB for a real 14-page DOCX and 2.4 MB for a 471-page,
  320,000-word document.
- Quota is a **reported** state, never a silent stop: one eligible version is released and the
  write retried once, and a still-failing write returns `history.quotaExhausted`, which
  `historyStatusKind` classifies as a refusal so `status_channel.mjs` escalates it to the
  assertive region and the toast. A browser eviction is reported as an eviction rather than as
  an empty timeline.
- **Compression stays deferred**, and `docs/112` §4.3's suggestion that HF-068 revisit it is
  answered with a reason rather than a number: the artifacts are DOCX/ODT ZIP containers,
  where the 37× that `docs/112` measured on normalized JSON does not apply, and content
  addressing already removes the duplicate-save cost that made many-snapshot storage look
  expensive. Measuring recompression on real ZIP artifacts remains `docs/140` §20 question 6.
- Per-change authorship is still **not claimed**: a version records one actor, because exact
  attribution needs the transaction-path unification in `docs/107` (H4). Nothing here asserts
  otherwise, and the version diff service (H3) is not built.
- What is **not** reachable yet: File ▸ Version history, the panel, preview, copy/download and
  the settings controls for the four retention numbers. The storage layer is complete and
  guarded but a user cannot get to it, which by this repository's own rule (SKILL §9.4)
  means the row stays open until the wiring lands.

**Alternatives rejected:**

- **Age-only retention.** It deletes the entire history of a document nobody edited for a
  week, which nobody means by "retain for 7 days".
- **Count-only retention.** Twenty-five checkpoints of a media-heavy document can be hundreds
  of megabytes, and the quota is shared with the crash-recovery drafts whose whole purpose is
  to be there after a crash.
- **Pruning a named version with disclosure.** A named version that disappears on day eight
  makes naming a lie; refusing the new capture keeps the promise the user was given and says
  what to do about it.
- **A separate version database.** Two stores to migrate, two quotas, two clear controls and
  two explanations, against one owner decision (D-1) that says the opposite.
- **Storing normalized JSON.** Measured data loss (`docs/112` §3.2). It remains useful as a
  derived diff projection, which is `docs/140` §11's business, not this one's.

## ADR-039 — White-labelling is a validated configuration, and chrome composition is a second axis

**Status:** accepted and implemented (`docs/126` phase 3).
**Context:** `docs/126` phase 3, `docs/125` §7 and §2 F4, `docs/63`, `109` HF-109.
**Supersedes nothing.** Extends **ADR-036** (the host contract) with the two axes a
host configures and the one thing they may not: legibility.

### The decisions

**1. The seam is a GENERATOR over a data file, not a runtime that accepts CSS.**
`webapp/brand.json` is the whole configuration; `webapp/tools/build-brand.mjs` turns it
into three committed artifacts — `src/brand.css`, `src/brand.mjs` and two generated
regions in `editor.html` — with `--check` failing the build in both directions, the
same contract the four existing generators hold rather than a fifth convention. So
white-labelling is "edit one JSON file, run one command", which is what `docs/126`'s
"no code changes — configuration only" means in practice, and the shipped
configuration is all-null so the default build IS the product and a white-label is
visibly a decision somebody made.

The alternative — a host stylesheet the editor loads and trusts — was rejected because
it cannot be validated. Which brings us to:

**2. A host palette that fails AA is REFUSED, at generate time, with the measured
ratio and a value that would pass.** Not corrected, and not warned about.

`docs/126` states the constraint: "a white-label that ships unreadable text is a worse
outcome than no white-labelling." Three candidates, and the other two are worse:

* **Correct it silently** — ship the host's brand in a colour the host did not choose.
  `src/contrast.mjs` already refuses to do this one layer down, in as many words:
  "Nudging a document's colour until it passes produces a preview that is a lie about
  the style." A whole product in an unapproved accent is that lie with a bigger blast
  radius, and the host finds out from a screenshot.
* **Warn and ship** — precisely ONLYOFFICE's failure mode, recorded in `docs/125`
  §1.1: a custom logo "is not blocked — it is *nagged*. It applies, then raises a
  'paid feature' modal. Worse than refusing, because the integrator ships it and then
  discovers the modal." A warning in a build log is read once.
* **Refuse** — the failure lands on the person who can still fix it, at the moment it
  is free to fix, with the number and a working value. One edit for them; nothing for
  their readers.

The floors are WCAG's own split, so a brand colour is not refused for being a hairline:
text pairs clear 4.5:1 (SC 1.4.3), edges and icons clear 3:1 (SC 1.4.11). The palette
is measured AS IT WILL RENDER — our tokens with the host's written over them, per
theme, with `var()` and `color-mix()` resolved — because a brand colour measured in
isolation passes when it is unreadable only against the surface it will be painted on.

**3. The overridable token set is a PUBLIC CONTRACT; the rest are internal.** This
answers `docs/125` Q-B, which asked whether publishing the token names freezes them.
Eighteen names are publishable and frozen: the accent family, the surfaces, the
foregrounds, the lines and the primary action. Everything else is refused —

* **geometry and z-order** (`--radius`, `--space-*`, `--fs-*`, `--h-*`, `--z-*`),
  because `docs/63` calls the flattened radii a considered decision and says to propose
  visual changes rather than make them. A seam that let a host round every corner would
  be making them on the owner's behalf, for every deployment, with nobody ever seeing
  the proposal;
* **the `--paper*` layer**, because the sheet is white in both themes on purpose so
  that everything drawn onto the raster has a fixed contrast partner. A host who
  darkened it would put the dark theme's insertion and deletion marks on a dark page at
  roughly 2:1, and no existing guard would see it — the paper tokens are in neither
  role list, deliberately, because they are not a theme. A host who wants dark reading
  wants dark chrome and a white page, which is what Word, Docs and ONLYOFFICE all do.

**4. A host's brand outranks a visitor's stored preference, and the answer travels in
the stylesheet that pinned it.** `docs/125` §2 F4 was still true: `applySettings()` wrote
an inline `--accent` on `:root` from `localStorage` (an inline declaration beats any
author stylesheet) and REMOVED a host's `data-theme` whenever the stored theme was
`system`, which is the default. Rather than adding a second configuration channel for
"did the host pin this", the generator emits `--brand-accent-pinned: 1` into
`src/brand.css` and the runtime reads it back. That is what makes a white-labelled
build a swapped static file and nothing else: no JSON to fetch, no ordering to get
wrong, no window in which the editor was our colour and then became theirs.

A host may pin the accent, because that is brand. It may **not** pin light/dark: that is
a reader's preference about their own eyes. A pinned accent disables the two Settings
colour controls WITH A REASON rather than removing them — a control inside a surface the
visitor was offered, which is where "never a dead control" applies, and what Word does
for a policy-managed setting.

**5. String overrides are a LAYER on `t()`, and the per-locale set is DERIVED.**
`setOverrides` is consulted before the nineteen catalogues in the same two lookup
functions, with `*` for every language and an exact tag to narrow; `*` outranks a
fallback language but not the reader's own. Registering a host's words as a twentieth
catalogue was rejected: an override for `de` would lose to our own `de` when the chain
reached it first, an every-language override would have no tag to live under, and
`locale_coverage.test.mjs`'s orphan gate would be refusing the host's own words.

A host renames the product in ONE field and gets all nineteen languages, because the
override set is generated by substitution over the committed catalogues rather than
hand-written per locale. Nineteen hand-written overrides are nineteen chances to forget
one, and the one forgotten is the one a reader sees.

**6. Chrome composition is a SECOND AXIS, resolved by the same authority.**
`capabilities.mjs` gains `REGIONS` (eighteen) and `resolveRegions` beside
`resolveCapabilities`. A region is deliberately **not** a capability: a withheld region
is a presentation decision about the host's page, a withheld capability is a permission,
and collapsing them would make every hidden band read as a refusal — and would let a
host defeat a permission by showing a surface.

The two rules compose as the owner's container notes require. Within a surface a role
DOES get, a command that cannot run now is disabled and says why. A role with no
business with a whole surface does not get the surface: **a `readonly` container has no
editing ribbon — not a ribbon full of greyed buttons, no ribbon.** "Never, for you"
versus "not right now".

`readonly` gets READING chrome — the menu bar rather than the ribbon, the navigation
rail, the status bar's page count and zoom, the find card. The menu bar rather than the
ribbon is not a detail: it is where File ▸ Print lives, and `print` is the one
capability `readonly` is granted. `preview` gets NONE of the eighteen, because it is the
runtime as a layout and rendering engine and `docs/126`'s own test of the difference is
"could a static image replace it? For `preview`, nearly."

`CAPABILITY_AFFORDANCES` is the joint between the axes: every capability a preset grants
must have an affordance in chrome that preset shows. That guard is what shaped reading
chrome rather than being written after it.

**7. Policies are per capability, not per tier.** `resolveCapabilities` takes a
withhold list (`?can=-print,-download`) applied after the preset, by the same function,
which is what makes the roles presets over the capability set rather than a parallel
mechanism. `?chrome=-ribbon` is the same parser over the region vocabulary. Both only
ever narrow, whether or not an entry carries its minus sign, and an unknown entry is
dropped — dropping narrows nothing, so a typo in a host's URL can never widen the
result. A guard proves every role is reachable as a narrowing of the role above it,
through that one function.

**8. Three version numbers, one place.** `package` is what npm resolved, `contract` is
what the editor speaks, `engine` is the Rust workspace the WebAssembly came from. They
are not collapsed, because `contract` is deliberately stable across package releases
(an added command, event or refusal code is additive — `docs/05` §12) and tying it to
the package version would make hosts re-pin for changes that break nothing. What was
missing was not one number but one PLACE:
`packages/opendoc-embed/src/release.mjs`, generated, with each field re-derived from its
source by a guard. No build commit in it — that is stamped at deploy time from
`GITHUB_SHA`, and a committed file carrying a hash is fabricated provenance.

**9. Ungated, and said so in the code.** No licence check, no tab counting, no
`_licensed` branch, no modal. ONLYOFFICE gates exactly this behind two server-supplied
flags — `LayoutManager._applyCustomization` early-returns when `!_licensed`, and the
logo and About block sit behind a second flag — and refuses to let an integrator remove
their About button at all (`Main.js:2639` force-sets `customization.about = true`). The
permissive licence is the wedge, so gating this would surrender the only structural
advantage we have. Every module that implements a piece of it says so.

### Consequences

- Editing `brand.json` without regenerating fails `build.sh`; hand-editing
  `src/brand.css` fails it too, so a deployment cannot drift into unreadable text.
- The overridable eighteen are now a compatibility surface: renaming one is a breaking
  change to every white-labelled deployment, and the refusal list is where that is
  enforced.
- A guard failure at generate time is the designed outcome for a bad palette, so a host
  whose brand fails AA cannot ship until they choose a legible pairing.
- Adding a region means an entry in `REGIONS`, a rule in `style.css`, and an affordance
  answer if a capability depends on it — all three asserted.
- Per-GROUP selection inside a band is NOT in this decision: eleven ribbon groups carry
  no `data-group`, so that roster would be half-expressible, and the honest grain for a
  host is the band.
- No engine operation was added (ADR-030 I2): theme and chrome resolve at boot and
  nothing here touches the document.

## ADR-040 — The version timeline is a right-hand panel, a withholdable chrome region, and a restore that confirms once

**Status:** Accepted and implemented. Requirements: `docs/139`. Architecture: `docs/140`.
Rows: `104` HF-068 / `105` OO-004. Builds on **ADR-038** (the store) and **ADR-039** (chrome
region composition). Code: `webapp/src/version_panel.mjs`, `webapp/src/version_policy.mjs`,
the `history` region in `webapp/src/capabilities.mjs`, and the four seams in
`webapp/src/main.js`.

**Context:** ADR-038 landed the whole durable store — checkpoints, naming, pinning,
retention, atomic restore — and said in its own status line that it was *not yet reachable
from the product*. A subsystem that is built and unreachable is the most expensive recurring
pattern in this repository (SKILL §9.4, and the owner's report: *"prs are half as no ui in
history"*). This ADR records the five decisions the interface had to make that the store
could not, and it answers `docs/139` §18 question 6, which ADR-038 left open.

The interface is designed from the Google Docs standard first, which is the standing rule
and which `docs/139` §3 already names as the model. The table in `version_panel.mjs`'s header
is the full mapping; what follows is only what it decided differently, and why.

**Decision:**

1. **A right-hand side panel, not a full-window page.** Google Docs replaces the whole window
   with a version-history view. This editor already has an established shape for "a list
   beside the document" — Outline, Pages, and the comments sidebar — and one mechanism beats
   two (SKILL §8). A panel also keeps the document visible next to the timeline, which is
   what makes previewing an entry legible: the reader can see the canvas change. It is
   mutually exclusive with the comments sidebar for the reason Outline and Pages are mutually
   exclusive with each other: the canvas is never squeezed from both sides at once.

2. **Two durable surfaces, and a chord, and no ribbon cost for the primary one.** File ▸
   Version history is the primary entry, and it is where all three references put it — Google
   Docs (File ▸ Version history ▸ See version history), ONLYOFFICE (`DE.Views.FileMenu.btnHistory`),
   Word (File ▸ Info). It spends no ribbon width. The second surface is a View-band button
   beside the other two panel toggles, because the timeline IS a panel toggle in this product;
   that is information architecture this editor already has, not a claim that Word or Docs
   file version history under View. `⌘⌥⇧H` is Google Docs' own chord for revision history,
   taken rather than invented. `105` UX-004 — a capability reachable from one place — is the
   recurring defect this closes for the timeline before it can open.

3. **The timeline is a WITHHOLDABLE CHROME REGION (`history`), and withholding it removes the
   COMMAND.** Version history is already gated on the `autosave` capability, because it rides
   the autosave path and one switch must not promise what the other has stopped doing
   (ADR-038). That covered the permission axis. It did not cover reachability: the command
   palette and the chord belong to no region, so a host that withheld `band.file` and
   `band.view` would still have a visitor one keystroke away from a timeline of a document's
   past. So the region takes the row out of the **registry** rather than hiding a button, and
   every surface loses it at once. It is deliberately **not** in reading chrome: no preset
   below `edit` grants `autosave`, so a reader's timeline could only ever be empty and
   disabled, and a presentation that can never say anything is a dead control.

   Adding the region made four existing guards fail — a selector, a CSS rule, a description on
   the embedding page, and the mirrored copy in the embed package — which is the argument for
   declaring it in `capabilities.mjs` rather than inventing a second mechanism.

4. **Restore confirms, every time.** `docs/139` §18 question 6 asked whether the confirmation
   could be skipped when the current head is already checkpointed and unchanged. It cannot,
   and the reason is not caution: **the confirmation is the only place the reader is told that
   their current work becomes a version of its own.** Google Docs does not ask, and can afford
   not to — its restore is an undoable edit to a server-side document. Here it replaces the
   document in the tab, so the sentence that makes restore legible as non-destructive has to be
   read before it happens rather than after. The card also states that the restored document is
   unsaved until it is written to a file.

   The order of operations is `docs/140` §9 exactly: validate the target's bytes against the
   hash its key claims → capture the current document as a version → parse in isolation →
   compare-and-set the head in one IndexedDB transaction → only then activate the canvas,
   through the ordinary open path. A refused pre-restore capture refuses the restore with it,
   which is `docs/112`'s "never let a restore leave the work in neither place" one level up.

5. **Read-only preview reuses the existing choke point.** A preview sets `readOnlyReason` and
   viewing mode, which is the editor's one fail-closed gate: `blockMutationInViewing()` already
   refuses every mutation route — typing, paste, toolbar, tables, review decisions, the SDK and
   the host bridge — and `editRefusalMessage` already prefers `readOnlyReason` over every other
   sentence, so a preview refuses an edit by *saying it is a preview*. A second gate would have
   put the reason for a refusal in two places, and a gate enforced by hiding buttons is not
   enforced.

6. **Wording is a module, not a DOM concern.** ADR-038's store returns status codes and no
   English on purpose. `version_policy.mjs` is the other half of that promise: one sentence per
   `HISTORY_STATUS` code, through `t()` with a literal key, plus the day grouping and a row's
   words. It is pure, so "does every refusal have a sentence a reader can act on" is a node
   question — and writing that guard found a real defect on the first run, where the panel's
   classifier disagreed with the store's about an unknown code and would have answered a
   failure with silence.

### Consequences

- Every `HISTORY_STATUS` code is now a compatibility surface with a translated sentence in
  nineteen catalogues. Adding a code without wording it fails `version_policy.test.mjs`.
- A host who withholds `history` withholds the capability from every surface, including the
  palette and the chord. A host who grants the region but withholds `autosave` gets a panel
  whose entry is disabled with the autosave reason, which is state explaining itself rather
  than a dead control.
- Structural diff (`docs/140` H3) is **not** built, so "Show changes" ships present and
  disabled with that as its reason. Make a copy and Download a version (`docs/139` VH-007)
  are **not** built and are not implied by the panel: they need a format→MIME answer the
  export registry owns.
- Restore is one confirmation and is not yet one Undo step (`docs/139` VH-015): the
  pre-restore version is stored, which is what makes both in-session Undo and after-reload
  reversal possible later, and reversing a restore today means restoring the pre-restore
  version from the timeline.
- No engine operation was added (ADR-030 I2). The preview and the restore both go through
  `open`, which is the ordinary format path with the ordinary admission limits.

## ADR-041 — A version diff is anchored content alignment, not node identity, and it runs as a budgeted job the host places

**Status:** Accepted and implemented for the engine and the facade. Requirements: `docs/139`
§9 (VH-008, VH-009). Architecture: `docs/140` §11, H3. Builds on **ADR-038** (the store) and
**ADR-040** (the panel that ships "Show changes" disabled). Code: `crates/casual-doc-diff/**`
and `crates/casual-doc-wasm/src/diff.rs`. The panel and overlay that consume it are a separate
lane and are **not** built, so "Show changes" stays disabled until they are.

**Context:** `docs/140` §11.2 step 2 instructed the diff to "match stable `NodeId` identities
when the states share the same retained lineage", and `docs/140` §20 question 8 left the
structural matching algorithm open. Both are settled here, and the first one is settled by
being **contradicted**.

**Decision:**

1. **Node identity is an anchor, never a match key.** Every id in this model — `NodeId` and
   every definition id that wraps one — is minted by `IdGenerator::new(config.id_namespace)`,
   a counter that restarts at one for each import. Two checkpoints are two independent
   imports, so nothing about the two id spaces relates them:

   - a **DOCX** pair uses namespace 1 on both sides, so the same id names a *different*
     paragraph on each side the moment anything is inserted above it. Matching on it would
     misalign everything below the insertion, confidently.
   - a **plain-text** pair derives its namespace from a hash of the whole text
     (`casual_doc_io::text::text_namespace`), so the two sides share **no** id at all.
     Matching on it would report every paragraph as deleted and re-added.

   Opposite failures, same conclusion. Both are guarded:
   `node_ids_are_ordinal_across_two_parses_so_they_are_anchors_and_not_match_keys` and
   `plain_text_checkpoints_share_no_node_ids_at_all`. **`docs/140` §11.2 step 2 is corrected
   in place** rather than left to be rediscovered.

   What ids are used for is what they are good for: every change record carries the id of the
   block on each side, in the `{node, start, end}` shape the review surface already emits, so
   a host can navigate a session parsed from those same bytes. Because a parse is
   deterministic, the ids in a preview session opened from a checkpoint are the ids the diff
   reported for it — the same property that makes them useless as match keys makes them
   reproducible as anchors. For a *live* document, whose ids have moved on since it was
   parsed, the anchor's container `path` is the reliable half.

   The identities that genuinely survive two parses are the ones the **source format** writes,
   and they are used wherever they exist: a comment's `w16cid:durableId` (else `w14:paraId`),
   a style's `w:name`, a bookmark's name, a media part's package path, and a header's semantic
   position (section ordinal plus page type, not its id). A body paragraph's `w14:paraId` is
   **not** among them: this engine reports that attribute as a located loss on save (FID-R-03),
   so it is not available to match on. Retaining it would make a later diff materially better,
   and that is now a recorded reason to do it rather than a side effect.

2. **The algorithm is anchored sequence alignment over a hash forest, not tree edit
   distance.** This answers `docs/140` §20 question 8. Composed from established parts:
   a **Merkle hash tree** over the ordered container forest, so an identical subtree costs one
   comparison; **common prefix/suffix trim**; **patience anchoring** (keys unique on both
   sides, then the longest increasing subsequence of their pairings — Bram Cohen,
   `git diff --patience`); **Myers' greedy O(ND) diff** (1986) on what is left; a second pass
   of the same aligner on a weaker key to tell an edited block from a deletion plus an
   addition; and a **hash join** on subtree hash for moves.

   **General tree edit distance (Zhang–Shasha and descendants) is rejected.** It is O(n²) at
   best with tree-depth factors on top, and its relabel/reparent edit script has no
   counterpart in what a reader of a document wants to know. A word-processing body is an
   *ordered* forest of typed containers, so aligning sibling lists and recursing into matched
   containers is both exact and near-linear.

   Two thresholds are decisions rather than tuning. A **move** is reported only when its
   content is unique on both sides; anything else is a deletion plus an insertion with an
   `ambiguous_match` finding, never a guess. A **pairing** inside a replace region is accepted
   outright when the kinds correspond one to one — one cell replaced by one cell is that cell,
   however different its text — and only on a similarity threshold when the region is ragged.

3. **Typed field names are reflected from the model, not listed.** A formatting or property
   change names `spacing.beforeTwips`, `borders.top.sizeEighthPoints`,
   `runProperties[0].bold`, read from the type's own serde field names. A hand-written list is
   the SKILL §5a defect in a new place: adding a field to a struct breaks every literal loudly
   and every list that reads it silently. `Definitions` is the one exception — reflecting it
   would be an O(document) encode of both sides, because it holds the headers, footers, notes
   and comments — so its fields are named explicitly and
   `every_definitions_field_is_either_compared_or_a_story` derives the list from the model's
   own source and fails until a new field is accounted for.

4. **The presentation vocabulary is review's vocabulary.** `insertion`, `deletion`,
   `move_from`, `move_to`, `formatting` — the exact strings `casual-doc-wasm`'s review
   projection already emits — plus `property` for a definition change that review markup has
   no inline form for. A version diff is a different *thing* (two snapshots, no authorship,
   nothing accept-able), but a reader does not have two vocabularies for "this sentence was
   added". No English is minted in the engine; ADR-040 §6 already settled that wording is a
   module, and a diff record carries a family, a kind and typed field names for
   `version_policy.mjs` to word through `t()`.

5. **The host places the work; the engine makes that possible.** Diffing two documents is
   O(document), and SKILL §8 forbids running that on the main thread uninterrupted. So the
   engine exposes a **budgeted coroutine**: `DiffJob::step(sides, budget)` does at most that
   much work and returns, holds no borrow of either document, reports blocks actually
   projected, and `cancel()` produces nothing. `beginVersionDiff(leftBytes, rightBytes)` wraps
   it with the two parses as their own slices.

   A Worker is the right home and is **not available today**: `webapp/` contains no `new
   Worker` at all, the wasm module is instantiated once on the main thread and holds the live
   document, and sharing that memory needs `SharedArrayBuffer`, which needs COOP/COEP headers
   GitHub Pages cannot send. A *separate* wasm instance in a worker needs no shared memory,
   which is why this facade takes bytes in and hands JSON out and references nothing in the
   live session: moving it is a `webapp/` change with no engine change. Until then the main
   thread drives it in slices, as `background_measure.mjs` already does for measuring a long
   document.

**Consequences**

- No engine operation was added (ADR-030 I2); a diff is a read, and the closed 53-variant set
  is untouched.
- A diff does **not** paginate. `import_for_diff` runs the format registry and stops, so it
  skips the shaper and the paginator — the expensive half of opening a document.
- `VersionDiff::complete` is false whenever any finding is present, so a diff cannot be
  labelled the complete document diff while a family was skipped (`docs/139` VH-009). The
  families that are deliberately not characterised, and what a reader sees instead, are
  enumerated in `docs/140` §11.7 and in the crate's own documentation.
- Media bytes are the host's, not the model's: the facade computes part digests from the
  imported resources so a replaced image is seen. A caller that supplies none gets a
  `missing_resource` finding rather than a diff that calls two different images the same
  image.
- Complexity is guarded by **doubling**, not by a clock: two guards build documents of n and
  2n and assert the comparison counter roughly doubles. The first of them found a real
  quadratic while being written — a fully rewritten body shares no key, and Myers was spending
  O(n·(n+m)) proving it — which is why a region with zero common keys now short-circuits.
- The panel is a follow-on lane. `docs/140` §11.8 says exactly what it should call.

## ADR-042 — Proofing is a versioned optional language-pack system, checked in a worker, behind an SDK boundary

**Status:** Proposed as a whole; **Increments A and B accepted and implemented**. Design:
`docs/146` (the owner's architecture of 2026-09-28, reproduced in the repository with its
verification corrections in place). Predecessor: `docs/114`, which is the record of proofing
**as shipped** and is not superseded. Code for Increment A: `webapp/src/proof_protocol.mjs`,
`webapp/src/proof_worker.js`, `webapp/src/spell_check.mjs`. Code for Increment B:
`webapp/src/proof_packs.mjs` (pure), `webapp/src/proof_store.mjs`,
`webapp/src/proof_sdk.mjs`, `webapp/src/proof_languages.mjs`,
`webapp/src/proofing_chrome.mjs`, `webapp/tools/build-pack.mjs` and `webapp/packs/`.

**Context:** proofing shipped as an in-process, main-thread, body-only spelling and grammar
pass over the page window (`docs/114`). It parses an ~84,000-word list into a `Set` on the
main thread, runs every rule there, and has no versioning story for its data. `docs/146`
proposes extending that seam into a versioned, optional **language-pack system** with the
checking in a **Web Worker**, and sequences the work as five increments. This ADR records
the decisions, including four the owner answered on 2026-09-29 that `docs/146` §10 had left
open, so later increments are not designed against guesses.

**Decision:**

1. **Checking happens in a worker, behind a pure protocol.** The rules that decide a
   finding live in `proof_protocol.mjs`, which is pure and in `PURE_MODULES`; the worker is
   a transport shim over it, and an in-process responder over the *same* function is the
   fallback where `Worker` is unavailable. One implementation of the rule, two transports —
   a second implementation is how two answers to one question diverge.
2. **Findings are versioned and stale ones are discarded.** A request carries
   `documentId`, `paragraphId` and `paragraphRevision`; a reply is dropped unless all three
   still match. Positions are never identified by canvas coordinates.
3. **The wire is UTF-16, the engine is UTF-8, and the conversion happens at one boundary.**
   Findings cross the worker boundary as JS string indices and are converted to engine byte
   offsets only in the document adapter, guarded by round-trip tests over combining
   characters, emoji, RTL and mixed scripts.
4. **A failed or absent language asset means spelling is UNAVAILABLE for that language, and
   says so.** It must never mean "an empty dictionary", which flags every word — the defect
   this increment found and fixed. Grammar is decided independently of every spelling asset.
5. **Corrections keep exactly one mutation path.** `doc.replaceRanges`, through the existing
   undoable command, in every mode. No second path is added for proofing, now or in a later
   increment (ADR-005, ADR-030 I2).
6. **Proofing is a separate optional package, not a webapp feature** (owner, 2026-09-29).
   The SDK surface is `configureProofing`, `installPack`, `removePack`,
   `checkRange`/`checkDocument`, `onFindings`, `dispose`, with **network and storage
   providers injected by the host**. Nothing in the protocol or the worker may reach a
   webapp global, assume the editor's own `fetch`, or assume its storage; an embedder must
   be able to omit proofing entirely or supply its own transport. The package is not
   published yet — building behind the boundary now makes publishing packaging work rather
   than a rewrite.
7. **A small, audited runtime dependency is allowed** (owner, 2026-09-29). `webapp/package.json`
   no longer has to stay at zero runtime dependencies for the pack and worker work. This
   reverses the constraint `docs/114` §2.1 and §11.1 reasoned from, and what it unblocks is
   the pack format being free to assume **Hunspell-shaped** inputs later — `nspell` (MIT) is
   the named likely candidate when more languages come. It is **not** needed for the English
   increment and is not being added now. Any dependency still gets a licence and provenance
   review before it lands, and offline-first is not negotiable.
8. **The pack size ceiling is ~50 MB per language** on the lowest supported device (owner,
   2026-09-29). That is the cap to design tiers against, not the target, and it does not
   remove the measurement obligation: cold pack load and worker memory are still instrumented
   on desktop and on a midrange phone and published per pack (`docs/146` §9). Browser storage
   is quota-bound and evictable, so an install degrades honestly when the quota refuses.
9. **Terminology stays browser-local and document-local by default, with a host-provided
   sync seam** (owner, 2026-09-29). No server, nothing leaves the machine; but the document
   profile's interface must admit an injected host provider, so a host that wants to sync
   terminology can, later, without the interface being rewritten. This binds Increment C's
   design; for Increment A it only forbids baking browser-local storage into that interface.

**Still open, deliberately:** **which source corpus is legally and practically suitable for a
redistributed context pack** (`docs/146` §10, Increment D). A dataset's licence is not
inferable from an engine's, and LanguageTool's own grammar rules carry an **LGPL** notice
that must not be copied in without a deliberate compatibility and attribution review. Left
unanswered rather than assumed.

**Consequences:**

- Increment A ships the stability half only: the grammar/dictionary decoupling, the pure
  contracts, the worker, stale-finding rejection and the offset round-trips. **No download
  UX, no pack manifest, no terminology profile, no confusion pairs.**
- **Increment B ships installation**, and adds exactly one thing to the checking model: a
  **fourth `supplement` tier** in `isKnownWord`, per locale, beside the dictionary, the
  shipped glossary and the user's own words. It is deliberately NOT unioned into the
  glossary — the tiers differ in lifetime (a pack is installable, removable and versioned;
  the glossary is part of the build), and collapsing them would make "which tier accepted
  this word" unanswerable and would let `suggestionsFor`'s glossary ranking score a general
  vocabulary. The seam Increment A left — `` `${BASIC_PACK_VERSION}.${assetsCounter}` `` — is
  replaced by `activePackVersion`, which names every active pack and its immutable version,
  so installing or removing one invalidates the result cache and the suggestion memo instead
  of serving the previous data's answers. The eight-step installer of `docs/146` §6 is a state
  machine over **injected** effects in a pure module, `digest` included; the store is a
  separate `opendoc-proofing` database whose activation is one transaction and which retains
  the prior version for rollback; and `webapp/packs/` is generated by
  `tools/build-pack.mjs --check` from a committed candidate list, filtered against
  `dict/en-US.txt`, `dict/en-GB.txt` and `dict/glossary.txt` so every word it ships is one the
  base tier really lacks.
- **`checkDocument` ships as a named refusal, not as a no-op.** The scan is windowed and
  body-only and whole-document enumeration is an engine export that does not exist, so a
  `checkDocument` resolving to an empty finding list would report a clean document by not
  looking at it — the failure `docs/146` §1 forbids. It returns a refusal code with a
  translatable reason, and `checkRange` refuses the same way for a range outside the scanned
  window rather than answering "nothing wrong there". **Increment D's redistributable context
  corpus is untouched and its licence question stays open:** every word in the shipped pack is
  original to this repository and ships under its Apache-2.0 licence, which is the only reason
  a pack could ship before that question is answered.
- The main thread stops parsing the word list and stops running the rules. The coordinator's
  remaining per-scan cost is engine reads for the paragraphs in the page window, which is
  the O(window) it already was.
- The result cache stays on the **coordinator** rather than in the worker, so an unchanged
  window costs zero messages; the key is still the one `docs/146` §4 specifies. Recorded as
  a deviation in `docs/146` §2 rather than left as a silent difference.
- `docs/114` §8's remaining list — non-body stories, `w:noProof`, custom-dictionary
  management, grammar breadth, a whole-document sweep — is **not** closed by this increment
  and moves under `docs/146`'s P2/P3 rows.
- Non-body stories are blocked on **enumeration in the engine**, not on the host: `hitTest`
  is body-only and `moveCaret` stops at a story boundary by design, while painting already
  works for those nodes. Footnote and endnote bodies have no host-reachable route at all.
  That is `crates/**` work and is why `docs/146` ranks it P3.

## ADR-043 — Transactions are the one mutation path, and undo is a projection of the revision log

**Status:** Accepted. Designed in `147-TRANSACTIONS-AS-THE-ONE-MUTATION-PATH.md`, which
implements `107` §2.1 P-1…P-3 and is the prerequisite ADR-033 named. Supersedes nothing;
it makes ADR-005 true in practice for the first time.

**Amended 2026-10-01.** "Every change" was not true when this was accepted: five call sites in
`casual-doc-wasm` wrote straight into a definition table — one media registration and four
numbering definitions — and sent only the paragraph re-pointing through the log, so the
definition was outside the commit. Undo left it behind, a refusal after it left it behind, and
a session would have fanned out a paragraph or a drawing naming a definition no other replica
held. The exhaustiveness guard counted what went THROUGH the envelope and was structurally
blind to what never went near it.

Closed by three operations modelled on `SetStyleDefinition` — `SetAbstractNumbering`,
`SetNumberingInstance`, `SetMediaReference` — each ordered first in the same transaction as
the nodes that name it (an ADR-030 I2 op-set change: 58 operations, and the compiler found all
eleven exhaustive matches that had to decide). The guard now forbids the facade taking a
**mutable borrow of the definitions at all**, because a first version listing table names was
defeated in one line by `let defs = …definitions_mut();`. `147` §6a records the mechanism,
the three consequences, and the two things it does not cover (`settings`/`sections`, and a
refusal guard that cannot be written while `to_js` panics on a native target).

**Decision:** Every change to a live (`v1`) document is a `casual_doc_transaction::Transaction`
applied through one function, which appends one `Commit` to an ordered, append-only
`RevisionLog` and advances the document's `RevisionId` by one. Undo and redo are **reads of
that log** — a backward scan for the target group, then the group's inverse operations applied
as a new transaction — not a second store maintained beside it. `casual-doc-transaction` is
re-founded on `casual_doc_model::v1::Document` and takes a dependency on `casual-doc-edit`, so
the envelope is defined over the closed operation set rather than owning a rival one.

**Why this shape.** It is the ProseMirror / CodeMirror factoring — transaction, invertible
steps, a position map per commit, history derived from the transaction stream — which is
command sourcing with a single mutation choke point. The repository already had the choke
point (`45` I1), the invertible closed op set (`45` I2) and the envelope; they had never been
joined, and a flat `Vec<HistoryEntry>` had grown in the gap.

**Two decisions this takes explicitly, because they were being taken by accident:**

1. **The live operation vocabulary is byte-addressed, not grapheme-addressed.**
   `24-TRANSACTION-SEMANTICS.md` rejected UTF-8 operation offsets for the Phase-0 v0 model.
   The v1 op set (`59`) is byte-addressed in the same anchor space as hit-testing (`58` §3),
   and `107` §2.1 chose it as the survivor without noticing it reverses that rejection. It is
   reversed here, deliberately: grapheme boundaries are a *caret-movement* rule, which belongs
   to the selection layer, not to the operation vocabulary. `Affinity` is retained, because it
   is orthogonal to the unit and is load-bearing for OT's insert-at-the-same-boundary
   tie-break.

2. **Transaction application does not clone or whole-document-validate.** `24`'s atomic
   pipeline cloned the document and revalidated it per transaction. Both are O(document) and
   would violate `107` B1 on every keystroke. Atomicity on the live path is provided by the
   rule the choke point already used: a single operation needs no snapshot because
   `casual_doc_edit::apply` validates before it mutates; a multi-operation group takes one
   working copy. `24` is corrected in place.

**Consequences:**

- ADR-005 becomes checkable rather than aspirational: an exhaustive source guard in
  `casual-doc-wasm` fails the build if the editor reaches the operation set outside the
  envelope, or if a history read resolves against anything but the log.
- The log keeps **forward** operations as well as inverses. That is what OT needs and roughly
  doubles history memory for the same undo depth; the bound moves from 256 undo entries to 256
  undo *groups*, so user-visible undo depth is unchanged.
- Undo granularity is a `GroupId` on the commit. Typing coalescing is "continue the previous
  group", and the review-typing rule that keeps only the first paragraph snapshot becomes a
  declared coalescing mode rather than an ad-hoc stack rewrite.
- **The Phase-0 v0 stack is not migrated.** `casual-doc-sdk` and
  `casual-doc-selection::TextSelection` edit the schema-v0 model, which no product surface
  renders, opens or saves, and the SDK's whole public API is shaped by it. That engine moves
  unchanged into `casual_doc_transaction::v0` and keeps its own pipeline. The two paths share
  no document, so this is a boundary, not a half-migration; retiring it is `126`'s v1 SDK work.
- `transform` is still absent, and this ADR does not add it. It makes it possible: an ordered
  log of transformable operations with per-commit position maps now exists on the live path.

## ADR-044 — A phone is a region preset of one shell, not a second application

**Status:** Accepted, 2026-09-30. Designed in
`148-PHONE-LAYOUT-COMPETITIVE-ANALYSIS-AND-DESIGN.md`. Upholds rather than reopens
`137`'s cancellation and `122` §6's "one responsive shell, no second build".

**Decision:** The phone experience is a third **region preset** of the existing shell —
`body.phone-mode`, set by `phone_chrome.mjs` at or below 620px — beside the ribbon and
compact chromes. One bundle, one `command_taxonomy.mjs`, one command registry, one set of
surfaces and one set of guards; the phone paints fewer regions of them and arranges the
rest for a thumb. It is **not** a second front end, a second build target, or a second
place where a command's home is decided.

**Context.** The owner asked for "completely separate controls and layout", explicitly not
"web view on mobile", with "no horizontal scroll". Every reference agrees that a phone
needs a different *experience*; they disagree about what that costs. ONLYOFFICE ships a
whole second front end per editor — `web-apps/apps/documenteditor/mobile` is a
Framework7-React application beside the ExtJS desktop one, with its own toolbar, router and
view tree, and there are four of them — while sharing exactly one thing with the desktop
app: the compiled engine, switched by a runtime flag (`sdkjs/common/apiBase.js:66`,
`isMobileVersion` from `config['mobile']`). Google steers phone users off the web surface
and onto native apps entirely. Word for the web narrows its ribbon to one line and stops
there.

**Why one shell.** Embeddability is the product (`SKILL.md` §1): the wedge is a permissive
licence plus a runtime a host can embed, and a host that must choose between two bundles
with two registries and two gating paths does not have an embeddable library. The sibling
editor that built two selectable chromes **deleted them** and replaced the idea with
per-region visibility flags resolved in one file (`137`), which `chrome_regions.mjs`
already implements here for capability withholding. A device class is a preset of that
mechanism, not a new one.

**Consequences.**

- Surface parity survives by construction: the phone tier hides regions in CSS and removes
  nothing from the DOM, so `one-axis-navigation.spec.mjs`'s palette-orphan guard reads the
  same surfaces at every width and no command can lose a home on one device class.
- The ribbon is not a navigation axis a phone can hold (measured: 550px of tab strip in a
  109px box at 390px), so the phone runs the compact chrome. The stored preference is
  untouched, so a window that widens past the rung gets back the chrome its owner chose.
- ~~**The document surface is a named exception to "no horizontal scroll".**~~
  **Retired 2026-10-01 by ADR-046.** A 6.5in text column cannot be both 390px wide
  and readable; the answer was reflow, which both references ship (Google's
  Pageless, ONLYOFFICE's `api.ChangeReaderMode()`), and which was layout-engine
  work. It is built, in both halves, and `#viewport` now measures **384 into 390**
  at the phone rung where it measured 794 into 326. The exception is struck from
  `148` §6 and §8, from `style.css`'s phone block and from
  `phone-no-horizontal-scroll.spec.mjs`, whose two tripwires both fired and have
  been deleted. (This bullet said "superseded in part by ADR-045"; the engine
  design is ADR-046 and `151` — the citation drifted while the document was being
  renumbered.) The one horizontal scroll that survives is a table too wide for the
  column, in a scroller of its own, which tells the reader something true about a
  table instead of something false about the document.
- Pinch-zoom is **not** suppressed, though ONLYOFFICE suppress it. They have a canvas-level
  pinch to put in its place and we do not (`105` UX-018), and removing magnification with
  nothing behind it is an accessibility failure rather than a decision.

## ADR-045 — `transform` is a pure function of an operation and the inverse recorded for it, and refusing is a first-class answer

**Status:** Accepted (2026-09-30). Designed in
[`150`](150-OPERATIONAL-TRANSFORM-OVER-THE-CLOSED-OP-SET.md). Implements ADR-033 and `107`
§3/§6.3; builds on ADR-043 / `147`.

**Decision.** Concurrent operations are reconciled by operational transform over the closed
op set, with a server imposing the total order — ADR-033, unchanged. `transform` rebases one
operation over one concurrent **`Change`**: the operation *and* the inverse
`casual_doc_edit::apply` returned for it. It is pure — no document, no lookup — and it lives
in `casual-doc-transaction` with **zero call sites in the engine**, because OT is dormant at
one editor and a source guard fails the build if that stops being true.

**Why the inverse, and not the operation alone.** A spreadsheet delete names its victims by
address, so the operation describes itself. A document delete names them by **identity**, and
the identities are only in the inverse: `DeleteBlocks { container, index, count }` does not
say which nodes died, and `JoinParagraphs { first, second }` does not say where the join
boundary fell. Invariant I2 (every operation returns its inverse) was built for undo; it turns
out to be what makes a *pure* transform possible at all. The sibling spreadsheet engine
solved the same problem by passing side tables in; here the side table is already on every
commit.

**Four decisions this takes explicitly:**

1. **Two outcomes for "nothing survived", not one.** `Satisfied` (the intention is already
   achieved; nothing lost) and `Tombstoned` (the anchor was destroyed; **must** be reported
   through the disposition taxonomy, `35`). A single no-op return — which is what the sibling
   uses, because a cell is emptied and never destroyed — would have made every tombstone
   silent, against the no-silent-loss rule.
2. **A rebase may produce several operations, and that needs no new operation.** A concurrent
   split divides a same-paragraph range; a concurrent formatting write leaves the earlier one
   holding up to three pieces. `Transaction` already carries a sequence, so the multiplicity
   lives in the envelope. ADR-030 I2 is not touched.
3. **Refusal is an answer.** Nine enumerated pairs return `Unsupported` rather than a guess,
   because an untransformed operation applied to a state it was not written against diverges
   the replicas *quietly*. The surface is a constant the build checks two ways. A caller must
   treat a refusal, and equally an `apply` rejection of a rebased operation, as "these two
   cannot be merged".
4. **TP1 only.** Proven over 3,153 generated concurrent pairs with every guard driven red.
   TP2 is not provided and is not needed while a server orders everything; **if peer-to-peer
   or merge-after-long-divergence is ever required, this ADR is void and needs a successor**,
   not an extension.

**Consequences:**

- `casual-doc-transaction` gains `transform`, `BlockPlacement`/`BlockIndex`, and
  `Commit::changes()`; `casual-doc-edit` exposes `field_text_len` so transform and `apply`
  compute a field insertion's width from one rule rather than two.
- **One seam is added, deliberately.** `BlockPlacement` hands the transform the one fact two
  operations cannot tell each other: where a paragraph sits among its container's blocks. It
  must be resolved against the *base* state, which neither replica holds while transforming —
  a session that cannot guarantee that passes `NoPlacement` and takes the refusal.
- Three findings are recorded rather than acted on, because each would need an op-set change
  (I2): block operations are index-addressed rather than node-addressed; `Pos` carries no
  affinity, so an insertion at a run boundary cannot say which side it meant; and operations
  do not carry the run identities they cause `apply` to mint, so two replicas' deterministic
  snapshots are not byte-identical.
- `107` §3.1, §3.3, §8 Q1 and §8 Q4 are corrected in place. §8 Q4's guess that table geometry
  was "the most likely place TP1 fails" was half right: the index arithmetic converges and the
  *carried payload* does not.

## ADR-046 — Reflow is a layout VIEW parameter, tiled, and the document stays editable in it

**Status:** Accepted 2026-09-30; **implemented, both halves** — the engine's
`LayoutView` and `setLayoutView` seam, and the shell's `view.reflow`, width feed,
seamless tiles, withheld chrome and forced-paper printing (`151` §6), 2026-10-01.
Specified in `151-REFLOW-PAGELESS-LAYOUT-DESIGN.md`. Completes the consequence
ADR-044 left open, and **retires `#viewport`'s exemption from ADR-044's
no-horizontal-scroll rule**: 384 into 390 at the phone rung, where it measured
794 into 326. **Amended 2026-10-01 by ADR-048** (below),
which supplies the rule this ADR left implicit — *which* width the caller passes — and
corrects one sentence of its evidence (below). The mechanism decided here is not
reopened: because the measure is a parameter of the seam, the cap costs one clamp in the
caller and no engine change at all. **That prediction held exactly**: ADR-048 shipped on
2026-10-02 with zero lines changed under `crates/`, which is the strongest available
evidence that the seam chosen here was the right seam.

**Decision.** A pageless/reflow view is a **`LayoutView` parameter threaded to the one
place layout geometry is decided**, not a second layout path and not a document edit:

- `LayoutView::{Paged, Reflow { content_width, tile_height, gutter }}` enters
  `casual-doc-layout`'s driver beside `ReviewView`, defaulting to `Paged`, so every
  existing caller is byte-for-byte unchanged and `geometry_snapshot.golden` does not
  move. Under `Reflow` the driver synthesises a `PageConfig` from the caller's width
  instead of from `SectionBoundary`, forces `ColumnLayout::single`, suspends the
  page-shaped *constraints* (`page_break_before`, `keep_next`, `keep_lines`,
  widow/orphan, section parity) and suppresses the page-shaped *furniture* (headers,
  footers, borders, watermarks, line numbers, section `w:vAlign`).
  **All of it happens in the driver.** No paginator, no `columns.rs`, no
  `running.rs`/`page_border.rs`/`line_number.rs`/`watermark.rs` and no line of
  `flow.rs` changed: the constraints are cleared once on the flowed galley (which
  covers all three paginators), and the furniture is suppressed by building an empty
  section plan, so the passes that place it find nothing. The design's own version
  wanted edits in four of those files; one mechanism turned out to be available and
  the diff is smaller for it.
- **Tiles are trimmed to their content**, and a zero gap is not enough without it.
  The paginator carries the chunk that does not fit, leaving up to a line of slack at
  each cut, so untrimmed tiles drawn edge to edge still show a blank band at every
  tile boundary. A reflow-only final pass cuts each tile's page box to its content
  extent; it is idempotent, it only ever shortens (so a page-anchored float below the
  text is held, not clipped), and it makes the painted column independent of the tile
  height — which is what lets the guard be exact rather than tolerant.
- **A `PAGE`/`NUMPAGES` field prints a refusal, not a tile index.** The refusal lives
  in the per-page labels the field pass already takes, not in a new field on
  `PaginatedLayout` (7 literals across two crates — the two-green-PRs-make-main-red
  hazard). The engine refuses rather than deferring to the host because a host cannot
  un-print a number the engine has already shaped into a glyph run.
- **The paginator still runs**, cutting the galley into fixed-height **tiles**. It has
  to: `compose_page` rasterises one `Page` into one `Surface`, and a browser canvas
  maxes out near 32,767px, so "one tall page" works on a fixture and fails on a real
  document. Tiles keep `renderPage`, `hitTest`, the caret, every overlay and the whole
  windowed path working with no change — which is *why* the document stays editable.
- **It is a view, never an edit.** No `Operation`, no revision bump, nothing on the
  export path. `setPageSetup` would produce the same visual result today and must not
  be used: it would persist a 390px-wide "page" into the user's DOCX, which is the
  silent-data-loss class the engineering priority order forbids outright.
- **It is a per-viewer preference**, stored beside the theme, defaulting on below the
  phone rung. Google made Pageless a *file* setting; we do not, because one person's
  phone must not reformat another person's monitor.

**Why this shape.** The flow engine is **already width-parametric end to end** —
`flow::build_galley(document, shaper, content_width)` and its eleven siblings take the
width as an explicit argument, and justification, indents, tab stops, auto-width
tables, text boxes and float wrap all resolve against whatever width they are handed.
That is the "uniform flow pipeline" invariant this repository has been holding on
purpose, and reflow is the first thing to collect on it. What is missing is only where
the width comes from. Both references converge on the same mechanism — ONLYOFFICE's
`CDocumentReadView` is forty lines of overrides substituting a synthetic `SectPr`
(`word/Editor/Layout/ReadView.js`) — so a second paginator would be inventing a
parallel path the prior art does not have.

**Why the document stays editable.** A phone browser is our whole mobile story
(`18-SUPPORT-MATRIX.md`), so a reading mode you must leave in order to type is not an
answer. **CORRECTED 2026-10-01 by `154` §2.3(d):** this paragraph previously justified
the choice as a divergence from ONLYOFFICE, on the ground that "their reader mode sets
`SelectEnabled = false` and is **not editable**." **That is false in source.**
`SelectEnabled` has exactly two consumers, both in
`common/Scrolls/mobileTouchManagerBase.js` (`:928`, `:1873`), and both gate **touch
selection handles**, not mutation; read-only in ONLYOFFICE is
`api.asc_addRestriction(Asc.c_oAscRestrictionType.View)`, which `ChangeReaderMode()`
never calls. Microsoft's Immersive Reader is editable too (first-party). **The decision
is unchanged and correct; only its evidence was wrong — and it was load-bearing here,
which is why it is corrected in place rather than left to `151`.**

**Consequences.**

- Entering or leaving reflow is O(document) — a full re-shape, because the galley
  cache is width-scoped. It is a *mode change*, not an interaction: it is never
  driven straight off a resize event. Resize is **quantised to 16 CSS px, floored,
  and debounced 150ms trailing** (`reflow_view.mjs`), which makes a resize inside
  one bucket cost one division and no document work at all; the debounce only
  bounds a drag that crosses buckets. **Not yet cancellable and shows no
  progress** — `151` §8 item 6 carries that honestly rather than this line
  claiming it.
- A `PAGE` field and the page counter resolve against **tiles**, which are not pages.
  The shell shows neither in reflow rather than printing a number that is wrong.
- Print forces `Paged` unconditionally, in `print.mjs`'s `withPagedLayout`, with
  the restore in a `finally`. **PDF export needed no guard**: `export_as_inner`
  takes `&self.document` and never reads `self.layout` or `self.layout_view`, so
  the writer re-paginates from the document's own sections. That is this ADR's
  "nothing on the export path" holding by construction — verified, not assumed.
- A table too wide for the reflow width keeps a horizontal scroller **of its own** —
  Google's arbitration. That is the one horizontal scroll that survives, and it tells
  the reader something true about a table rather than something false about the page.
  **Built 2026-10-06** (`151` §6.3d, `MeasureFit::Scroll`): from 2026-10-05 until then the
  table was fitted to the column instead (FID-R-13), which kept the content and narrowed the
  author's columns; the scroller restores this decision as written.
- Entering or leaving reflow **discards the galley cache and rebuilds whole**, never
  resuming a layout built in the other view: a reflow pass clears the break flags on
  the galley it retains, and a paged rebuild served one of those fragments would
  silently lose the author's page break.
- **A windowed body is refused reflow, with the reason** — `151` §4.5 row 7 proposed a
  windowed *variant* and that is wrong. A windowed body is already read-only (every
  mutation is refused at `apply_group`), and reflow's defining promise is that the
  document stays editable in it, so a variant would be a second thing with different
  guarantees sold under the same name. Never a silent no-op: a toggle reading
  "Reflow: on" over an unchanged, still-panning page is the worst outcome available.
- **Open:** page- and margin-anchored drawings have no referent in reflow. Word keeps
  them and lets them overlap, Docs inlines them. Undecided; `151` §8 item 1 records
  it, and `LayoutView::approximations()` reports it to the host so a reader is told
  rather than left to notice.

## ADR-047 — The collaboration relay orders and fans out; it holds no document and runs no transform

**Status:** Accepted for implementation, 2026-09-30. Specified in
`152-COLLABORATION-PROTOCOL-SESSION-AND-IDENTITY.md`. Builds `107` 6.6's foundation on
ADR-033's decision and ADR-045's transform. Answers `143` §16 Q1 and `107` §8 Q6.

**Decision.** Four parts, and the first is the one that had to be chosen rather than copied.

1. **The relay is a dumb, substitutable ordering service.** It assigns the total order,
   suppresses duplicates by `(client, seq)`, retains a bounded tail of operations it never
   reads into, and fans out. It holds **no document**, runs **no transform**, and interprets
   no operation. A submission written against a position the document has moved past is
   refused with `StaleBase{current}`; the client rebases it locally and resubmits with the
   same sequence number.
2. **`casual-doc-transaction::{protocol, session, wire}`** — pure data and two pure state
   machines, in the engine crate, with no transport, no clock and no I/O. Both ends compile
   from it. A relay binary, when it exists, is a workspace member under `server/` and nothing
   under `crates/` may depend on it.
3. **Rollback/replay, against a horizon on the revision log.** `RevisionLog` gains a
   `horizon` — the revision up to which commits have been *ordered*. A rebase rewrites only
   commits above it, which are by construction this replica's own unacknowledged work, and a
   rewritten commit keeps its transaction id, undo group, label and origin, so undo is
   untouched. **A replica with nothing pending does not roll back at all**: the arrival goes
   through `RevisionLog::apply`, the same call a keystroke makes.
4. **An introduction needs a private space; a reference needs the order.** Every participant
   mints `NodeId`s in an `IdSpace` derived from the document's own space and the relay-assigned
   participant number, and that derivation is **injective in the participant number** — so two
   replicas cannot mint the same id. Ids an operation merely *names* are safe because the
   session is totally ordered.

**Amended 2026-10-01 (owner), and it changes what this ADR claims rather than what it built.**
The relay is substitutable and a standalone document needs none — but "collaboration is
optional" was the wrong phrasing for the lifecycle. A replica with no connection cannot learn
that a second person began editing, so a room that came into being on the *second* participant
would make that transition unobservable from the first participant's side. There are therefore
**two modes**: standalone editing with no room at all, where no server is required and never
will be; and a **shared** document — reached by a link, or embedded by a host — which **joins a
room from its first open even with one participant**. One doc, one room.

The room is created by the **host**, not by the first client: a client cannot name a room it
has not been told about, and letting it invent one would make the room id client-controlled,
which the host-signed grant exists to prevent. A lone writer in a room pays **no** transform,
**no** document clone and **no** round trip before its own edit is visible, so the common case
does not pay for the rare one. Work done standalone travels as the **snapshot the room is
created from** and never as operations, which is forced rather than chosen: the offline mint
space is reserved, so an operation introducing an offline-minted id is refused by every
receiver — and `ClientSession::joined` settling the log at `head` is the single line that keeps
those two rules from contradicting each other. `152` §2a records the measurements, the open
questions and the guard.

**Why a dumb relay, given the sibling engine transforms server-side.** Our `transform` takes
its concurrent operation as a `Change` — the operation *and the inverse `apply` returned for
it* (ADR-045, `150` §2.3) — because our deletes name their victims by identity rather than by
address. Only `apply` produces an inverse. **A relay that transforms must therefore be a full
document replica running the whole engine**, which a spreadsheet's self-describing operations
do not require. That would contradict `143` §6 ("the provider owns the transform *never*"),
make a third-party or managed relay impossible (`143` P3), put the document and the engine's
attack surface on the server, and reproduce ONLYOFFICE's shape — the thing this project exists
to be an alternative to. It is also the **one-way** choice: nothing in this protocol forbids a
deployment that links this crate and transforms server-side, and clients cannot tell the
difference except that they are refused less.

**What choosing this costs, stated plainly because it is real.** Ping-pong under contention: a
refused client pays one extra round trip. Progress is guaranteed while each retry is against a
strictly newer head — the arrivals that caused the refusal arrive on the same ordered
connection, and `flush` refuses to resubmit until this replica's position has reached the one
the refusal named — but **the head is not guaranteed to stop moving**, so sustained
many-writer contention can starve a slow client. And no server-side snapshot verification by
replay, which `150` §9.3 blocks anyway until operations carry the identities they cause to be
minted.

**The measurement that decides whether this was right has now been run** (2026-10-01, `152`
§10 Q3). With *W* simultaneous writers the relay refuses `(W - 1) / 2` chunks per chunk it
orders, and the worst-placed writer needs `W` attempts: **linear in the writer count, not
quadratic** — the ping-pong this decision described and nothing worse. Everybody's work
lands; nobody starves. A **lone writer is never refused**, so single-user editing pays nothing
for the machinery. Measured at 1/2/4/8/16 writers by
`the_dumb_relay_s_refusal_rate_is_the_ping_pong_and_nothing_worse`, deterministically and
with no clock. What is *not* decided is whether that shape is acceptable at a given latency:
at 16 simultaneous writers a chunk costs eight extra round trips. Coalescing (`107` §4 B3)
and pipelining are what move it, and neither needs the relay to hold a document — so the
decision stands and the next measurement belongs with the `107` §4 benchmarks.

**Consequences.**

- **`147` is corrected in place:** the log has two positions, not one. "Nothing rewrites a
  commit" holds for everything below the horizon, which is everything anybody else has seen.
- **`150` §10 Q1 is closed**, not worked around: the rollback driver *is* at the base state
  when a `BlockPlacement` is needed, so it builds one there and nowhere else.
- **`150` §10 Q2 is a blocker, not a question.** A `Coalesce::ContinueKeepingFirstInverse`
  commit records no inverse, so it cannot be rolled back — **suggesting mode cannot take part
  in a session** until `147`'s envelope retains inverses and drops them at undo-read time.
  Refused with a stable code rather than diverged.
- Rebasing a *sequence* of unordered commits needs the arrival's inverse at each intermediate
  state, and that composition is three operations where `Change` carries one. The driver
  therefore **probes**: it applies the arrival's image at each base state the rollback reveals,
  keeps the inverses, and applies them straight back. This rests on one invariant, and it is
  the one undo already rests on (ADR-030 I2).
- ~~**The live editor's minting namespace is derived from the document today**~~ —
  **closed 2026-10-01, and the closure amends part 4 of this decision.** The editor now mints
  every identity through the model's `IdSpace`: `IdSpace::participant(base, number)` after a
  room assigns a participant number, and a **reserved offline space** `IdSpace::local(base)`
  before one does, so a document with no session and no server still mints, in O(1) per
  keystroke. Three consequences of the closure:
  - the derivation moved from `base ^ (K · (c + 1))` to `base ^ (K · (c + 2))`, so `base ^ K`
    is free for the offline space and **two** participant numbers are refused rather than one;
  - the partition lives in **`casual-doc-model`**, not in `casual-doc-transaction::wire` —
    identity is a property of the model, and hosting it in the collaboration crate would have
    made single-user editing depend on it. `wire` re-exports `IdSpace` and adds
    `wire::space_of`, so there is one derivation and not two, and
    `the_live_editor_has_no_collaboration_dependency` still holds unchanged;
  - **`PROTOCOL_VERSION` goes 1 → 2.** No wire field changed — the space is derived from the
    `client` field a message already carries — but a version-1 peer and a version-2 peer
    compute different spaces for one participant number and would refuse each other's every
    introduction while both believed the message well formed.

  `152` §4.4 records the mechanism, the id families enumerated from the code, the
  backward-compatibility case (a normalized JSON snapshot preserves node ids verbatim, so the
  allocator is seeded above what the document already holds), and four mutation proofs.
  `150` §9.3 is **narrowed, not closed**: snapshot verification by replay across replicas is
  still blocked, now solely because operations do not declare the identities they cause to be
  minted.
- `docs/20` gains `ODC-7002`…`ODC-7009`. `ODC-7001` is reused for `CannotMerge`.
- The two O(document) costs — one document clone and one `BlockIndex` build — are on the
  **contended** path only: a remote edit arriving while this replica has unacknowledged work.
  Never on a keystroke, never on an uncontended arrival.
- Deliberately not decided or built here: the byte codec, the relay binary, presence,
  collaborative undo, the host-signed grant, and durability. `152` §9 and §10 say why for each.

## ADR-048 — The reflow measure is capped in characters, and reading is a width POLICY, not a second layout mechanism

**Status:** Proposed 2026-10-01; **Accepted and implemented 2026-10-02** — the clamp, the
four-step per-viewer control on three surfaces, the centred column, and the engine-ceiling
mirror that closes the `ColumnTooWide` refusal. Raised by the owner's challenge that
"the complete view and wider content is not at all suitable for reading", and specified in
`154-READING-VIEW-MEASURE-AND-DOCUMENT-FOLDING-COMPETITIVE-ANALYSIS.md`. **Amends ADR-046**
(supplies the caller rule it left implicit, and corrects one sentence of its evidence);
**does not reopen it**. The two calls it left to the owner are implemented as proposed —
**80 characters and four steps** — and the default is **Reading**, which is one assignment
(`REFLOW_WIDTH_DEFAULT` in `webapp/src/reflow_view.mjs`): a reader opening a document on a
1440px screen should not be handed a 241-character line. **The owner may overrule it by
changing that one line.**

**Amended 2026-10-06 — the owner overruled the default, and a fifth step was added.** The owner
compared reflow with Google Docs' pageless view and judged *"at present width of page is too
small"*. Measured at 1440×900 with the outline open, Reading's 469px column was **narrower than
the 624px text column the same Letter document shows on paper**, so turning the pages off took
width away. The default is now **Wide** — the document's own page width, the page with its
margins taken away (`pageSetup().pageSize.widthTwips`, derived, inventing no constant), capped by
the window like every step. Reading (WCAG's 80) stays one click away; a stored choice is never
reinterpreted. The rest of this ADR stands: one clamp, per-viewer, ≥2 surfaces. Google's own
default step and its step widths could not be verified (`151` §6.2a says which sources were
reachable), so Wide is chosen on our measurement, not as a claimed match.

**What implementation changed about this ADR, recorded rather than smoothed over.**

- **The published cap in twips was wrong by four.** This ADR and `154` §5.1 said *468 CSS
  px / 7,020 twips*; 7,020 was 468 px converted *back* to twips, so the pixel rounding
  counted twice. The exact value is `round(80 × 0.3991 em × 11 pt × 20)` = **7,024** twips
  = 468.27 px, so **468 px is unchanged and right**. Corrected in place above and in `154`
  §5.1; pinned by `reflow_view.test.mjs`, which is how it was found.
- **The engine's 22in bound is MIRRORED in the shell and applied to every step, Full
  included.** The ADR said the cap "removes the `ColumnTooWide` refusal as a side effect,
  since a capped column cannot reach 22in" — true of the three capping steps and **false of
  the explicit `full`**, which this ADR also requires. A named Full that threw on a 4K
  monitor would be the same defect reached through the new control, so
  `REFLOW_MAX_CONTENT_TWIP` is a second, never-waived ceiling beside the viewer's cap and
  Full means "as wide as the window, up to the widest column the engine can shape".
- **"On a phone both reduce to `available`" is true of the two POLICIES and not of every
  step.** Fit, Reading and Full all reduce to `available` at the 390px rung, so the 60
  characters and ADR-044's retired horizontal-scroll exemption are untouched. `narrow` is 55
  characters = 4,829 twips against the rung's 5,280, so it genuinely binds on a phone — and
  should, because a reader chose it. The guards therefore assert *equality* for the default
  and the two policies and *direction* (never wider than the window) for every step.
- **`fit` ships labelled "Paper".** The View band already carries Fit width and Fit page in
  its Zoom group; a third Fit in one band is a worse menu than one accurate noun. The policy
  keeps the name `fit` in code.
- **The CJK 40 is a live, tested branch and is not reached in the shipped product.** The
  target is chosen by the resolved face's *measured* mean advance (full-width above 0.70 em)
  rather than by a family name, which is the only non-guessing signal available. No CJK face
  is bundled, so no entry in the advance table is full-width and a CJK document gets the
  Latin fallback today. That is the font-provisioning gap, recorded here rather than left to
  be discovered.
- **`stylePreview("Normal")` and `pageSetup()` are the two getters the cap reads**, chosen
  because each is O(styles) or O(1); the caret-aware `pageSetupSections`/`sectionLayout`
  resolve a node by walking every paragraph and a cap is not worth a document walk per
  render.

**The defect this answers, with the number.** `webapp/src/reflow_view.mjs`'s
`reflowMeasure` has a minimum (`REFLOW_MIN_CONTENT_TWIP`) and **no maximum**, so the
reading column is the window minus two 16px gutters at every width: 352 CSS px at the
390px phone rung (**60 characters** of 11pt Calibri — correct, and the only width ever
evaluated), but **1,408 px — 241 characters — at a 1440px window**. WCAG 2.1 SC 1.4.8
(AAA) requires that *"a mechanism is available"* for *"Width … no more than 80 characters
or glyphs (40 if CJK)"*; we have no such mechanism in any configuration. And above a
reachable width it **fails outright**: `LayoutView::reflow` refuses a column over 22in, so
a 2,160px window at 100% zoom — or 1,104px at 50%, and 50% is both a `ZOOM_STEPS` entry and
`FIT_ON_OPEN_FLOOR` — throws, and `reflow_chrome.mjs`'s `sync` reverts the preference to
paper and shows the reader a message about twips. Derivations in `154` §3 and §9.

**Decision.**

- **The column is capped: `content_width = min(available, policy_cap)`.** One clamp, in the
  **caller**. **No `crates/` change is required** — ADR-046 made the measure a parameter of
  the seam, which is why this costs one line of arithmetic rather than a layout pass.
  The cap also removes the `ColumnTooWide` refusal as a side effect, since a capped column
  cannot reach 22in.
- **The cap is a target in CHARACTERS**, because that is the unit every reference and
  standard in `154` §2 is stated in and the only one that transfers across faces and sizes.
  It resolves to twips as `target_chars × mean_advance(default_face, default_size)`,
  measured by the shaper the engine already has. Fallback where the face's metrics are
  unavailable: **0.40 em per character** (≈32 em at 80 characters), justified by a measured
  0.393–0.431 em spread across the four bundled base text faces — an approximation stated
  with its error, not a constant with no source.
- **The default target is 80 characters (40 CJK), from WCAG 2.1 SC 1.4.8.** Chosen over
  Bringhurst's widely-quoted 66 for an evidence reason rather than a typographic one: 80 is
  normative, first-party and quotable, and 66 could not be verified against his text
  (`154` §7 item 1). 80 is also the conservative end, so the default errs towards the paper
  the reader is used to. At 11pt Calibri that is **468 CSS px / 7,024 twips**.
  (**Corrected on implementation**: this read 7,020, which was 468 px converted back to
  twips and so carried the pixel rounding twice. The exact value is
  `round(80 × 0.3991 em × 11 pt × 20)` = 7,024 = 468.27 px, so 468 px is unchanged.
  `154` §5.1 carries the same correction, and `reflow_view.test.mjs` pins 7,024.)
- **Above the cap the column is CENTRED on the application desk** — not widened, not given
  a paper edge, not split into columns. `#viewport.is-reflow` already removes the sheet
  shadow and radius, so a tile becomes a text column on the app background, which is what
  Docs, Immersive Reader and every reader in `154` §2.4 do.
- **Two width POLICIES over ONE layout mechanism.** `LayoutView::Reflow` is unchanged; the
  policies differ only in which `X` goes into `min(available, X)`:
  - **Fit** — `X` = the document's own text measure (the first section's content width;
    624px at Letter default). The *pageless authoring* policy. It never makes a line
    **longer** than the paper the author is writing for, and it invents no constant: the
    number comes from the document.
  - **Reading** — `X` = the character target. The desktop default.
  On a phone both reduce to `available`, so the phone's 60 characters and ADR-044's retired
  horizontal-scroll exemption are untouched.
- **"Reading view" is a PRESET, not a third mode.** It sets four independently useful
  settings — pageless layout, Reading width, reduced chrome, folded outline (ADR-049) —
  exactly as Word ships Focus (chrome) and Immersive Reader (measure) as separate things
  that compose. One registry row writing four preferences is not a parallel path.
- **The width control is per-viewer, four steps, ≥2 surfaces**, including an explicit
  **Full** (uncapped). Per-viewer because that is what Google does and states in as many
  words: *"Your text width choice won't affect how collaborators see your docs."* Full is
  named on purpose — a host embedding in a 400px column has the narrow case already, and a
  named Full makes the default's narrowness discoverable rather than mysterious.
- **Nothing here becomes a document property.** Google makes *pageless* one
  (`DocumentStyle.documentFormat`, `DocumentMode.PAGELESS`) and explicitly does **not** make
  text width one. Both stay per-viewer here, and the decisive reason ADR-046 §3.4 did not
  give is that **neither DOCX nor ODT has anywhere to put it**: a document property we
  cannot serialise is a sidecar, and a sidecar that reformats every collaborator's screen is
  a worse trade than a per-viewer default. ADR-044's reasoning stands as the second reason.
- **The document stays editable, and the reason changes.** ADR-046 justified this as a
  divergence from a read-only field. There is no read-only field (correction above;
  Immersive Reader is editable too, first-party). The justification is the mobile-support
  one alone, and it is sufficient.
- **A reader type-size control is NOT bundled with this** (`151` §8 item 2 stands) — but its
  status improves: once the cap is in characters, changing the size changes the width and
  leaves the measure put, so it stops being a second policy and becomes a consequence. Both
  references ship one; ONLYOFFICE ship *only* one (a nine-step point ladder) and no cap.

**Why this shape.** The field converges and `151` §2.3 had removed the strongest vote from
the record. Google caps (Text width: Narrow/Medium/Wide, per-viewer); Word caps twice over
(Immersive Reader's four-step **Column Width**, whose documented purpose is *"changes line
length to improve focus and comprehension"*, and Read Mode's adjustable columns); WCAG
1.4.8 supplies the number. ONLYOFFICE do **not** cap — they take the paper's width divided
by the device pixel ratio and grow the type instead — and they ship **no desktop reading
view at all** (`ChangeReaderMode` has zero hits under `apps/documenteditor/main`), so on
precisely the surface this ADR is about they are not a reference. `154` §2 has every
citation and §7 lists every claim that could not be verified.

**Consequences.**

- A desktop resize above the cap changes nothing at all in the document, so most desktop
  resizes become free for a second and better reason than `151` §6.2's quantisation. The
  quantum and the debounce still bind below the cap and are unchanged.
- `MAX_REFLOW_COLUMN`'s doc comment must stop describing a correct caller as having
  "converted units wrongly". **STILL OPEN**, deliberately: it is a comment-only `crates/`
  edit of no behavioural value whose price is a full Rust gate sweep, and the shell now
  mirrors the bound so no correct caller reaches it. The refusal itself **was** verified in
  a browser before being masked (`154` §7 item 20): with the shell ceiling lifted and Full
  chosen at 50% zoom on a 1,440px window, `#viewport` loses `is-reflow` because `sync`
  catches the throw and reverts the viewer to paper. That is the mutation proof for the
  paint-tier guard and the browser reproduction in one run.
- The cap's guard must **count characters on a shaped line**, not divide a column by a mean
  advance; the figures above are arithmetic on a measured mean and accurate to roughly ±5%,
  which is ample to establish a 3× discrepancy and not the form a committed guard may take.
- **Closed by implementation:** the default step is **Reading (80)**, there are **four**
  steps, and they are labelled Narrow / Reading / Paper / Full. All three remain owner calls
  and all three are one-line changes; the default is `REFLOW_WIDTH_DEFAULT`.
- **Open, and deliberately NOT built here:** the "Reading view" preset (it would set reduced
  chrome, which belongs to another lane), a reader type-size control (`151` §8 item 2), and
  a guard that counts characters on a *shaped* line rather than dividing by a mean advance
  (`154` §7 item 19) — the shell has no per-line text API, so the conversion is pinned as a
  pure function in `reflow_view.test.mjs` and the painted geometry is pinned at the paint
  tier.
- **Open:** WCAG 1.4.8 item 3 — there is no mechanism to un-justify a justified document in
  the reading view. Named so it is not rediscovered.

## ADR-049 — Folding is a per-viewer block visibility filter keyed on the outline, not a layout view

**Status:** **ACCEPTED, 2026-10-02**, on the owner's instruction not to wait for a decision
but to settle it from ONLYOFFICE's source and confirm it. Proposed 2026-10-01, specified in
`154` §5.3; the evidence that closed it is `157-ONLYOFFICE-LARGE-DOCUMENT-AND-FOLDING-SOURCE-FINDINGS.md`.
Raised by the owner ("collapsible and expandable based on outlines like VS Code does with
code"). Independent of ADR-046 and ADR-048 by design; that independence is the decision —
and it was honoured in practice: ADR-048 was implemented on 2026-10-02 with **no folding
work and no folding seam**.

**Built as of 2026-10-02, and this supersedes the earlier "nothing built" status line:** the
**persistence tier** is done — `ParagraphProperties::collapsed`, the import arm, the export
writer with its `w15` namespace declarations, `fixtures/generated/collapsed-headings.docx`
(the first `.docx` in the repository to carry `w15:collapsed`), and six guards in
`casual-doc-export/tests/collapsed_heading_state.rs`, each driven red by a production
mutation.

**The behaviour and affordance tiers are built as of 2026-10-04** (`109` FOLD-001 and
FOLD-004), and this supersedes the "not started" line that stood here: `casual-doc-layout`'s
`fold.rs` carries `FoldSet` and the one `heading_level` rule for the whole product, the
facade exposes `setFold`/`foldAll`/`unfoldAll`/`foldToLevel`/`foldState`/`setFoldSet`,
`outline_panel.mjs` is a real `role="tree"`, and `main.js` wires the chrome. A folded H1 with
40 paragraphs under it paginates to one page where five is unfolded, and
`geometry_snapshot.golden` did not move. Three gaps are carried as `109` FOLD-002 (a
positioned table in a folded range), FOLD-003 (a windowed body is not foldable) and FOLD-005
(RTF import drops every heading's identity) rather than left implied here.

**Why it is Accepted rather than still Proposed.** The Proposed status was waiting on a
competitive answer and there is one, read from `reference/sdkjs` at `72b0421` and
`reference/web-apps` at `9c0ca53` and re-verified rather than taken from a report:

- **ONLYOFFICE do not fold.** `grep -rinI "collaps" sdkjs/word` → **0 hits** across the
  whole 232-file Word engine, and the complete exported outline API
  (`word/Editor/DocumentOutline.js:501-512`) has no Collapse or Expand, so their panel has
  nothing to call.
- **They ignore `w15:collapsed`, structurally.** No `Collapsed` in the `CParaPr` constructor
  (`word/Editor/Styles.js:16232`, whose only outline field is
  `this.OutlineLvl = undefined; // Для TableOfContents`); none in the x2t↔sdkjs interchange
  enum `word/Editor/Serialize2.js:243 c_oSerProp_pPrType` (values 0–49, only
  `outlineLvl: 34`); an unallocated record reaches `Serialize2.js:9411 default: res =
  c_oSerConstants.ReadUnknown;` and is discarded. The state cannot survive an edit round-trip
  through their editor.
- **Word is therefore the standard**, which is the house rule where a competitor's code has
  nothing — and Word's behaviour settles the only branch that mattered: **collapsed content
  occupies no pages.** ONLYOFFICE's pagination loop (`word/Editor/Document.js:3707`) has no
  visibility filter at the block tier at all, so their architecture confirms the negative: a
  panel-only fold cannot reproduce Word.
- **The fold range is pinned by their own arithmetic**, which they compute for *selection*:
  `private_GetNextSiblingOrHigher` (`word/Editor/DocumentOutline.js:400`) scans forward to
  the first following heading whose level number is `<=` the fold's.

**The one thing in this ADR that was wrong, and it is an implication rather than a
decision.** "Orthogonal to `LayoutView`" and "not part of reflow" are both right. But this
ADR and `154` §5.3 together read as though the mechanism were nearly local, because the
three precedents it reuses all exist. **It is an engine change**, unambiguously, and saying
so is the correction.

**The gap, as it stood on 2026-10-01.** There was no folding anywhere:
`webapp/src/outline_panel.mjs` renders a **flat** list of buttons with `lvl-1`…`lvl-6`
classes, no disclosure, no `aria-expanded`, no `role="tree"`; no fold state exists in
`webapp/src`; and **`w15:collapsed` was not parsed** — the `CT_OnOff` element in a heading's
`w:pPr`, Word's `w15` namespace, by which Word persists a collapsed heading and which
Microsoft's Open Specifications define. No `.docx` in the repository carried one (38
packages, every XML part, zero hits), so the loss-coverage gate had never had the
opportunity to flag the drop. Word has folding; Google has folding; **ONLYOFFICE has none**
(zero `collaps` hits anywhere in `sdkjs/word/`, and `CDocumentOutline` exposes no Collapse or
Expand).

**Of that gap, the `w15:collapsed` half is closed as of 2026-10-02** — parsed, modelled
tri-state, written, round-tripped, with a fixture and six mutation-proven guards. The two
`webapp/src` sentences still stand, and so does the absence of any layout filter.

**Decision.**

- **Folding is NOT part of reflow, and must not be built into it.** The thing it operates on
  is the *heading tree*, which is identical on paper and in a reflowed column. Word's
  folding works in Print Layout; Google's Pageless-only restriction buys a reader nothing
  and is an artefact of where Google built it. Coupling two orthogonal settings is the
  specific mistake ADR-048 corrects, and it must not be repeated one ADR later.
- **The mechanism is a per-viewer block visibility filter — a `FoldSet` of collapsed heading
  `NodeId`s threaded to the flow pass**, so the blocks of a collapsed subtree contribute no
  fragments. **Not a second flow path.** Three precedents it reuses rather than parallels:
  the filter already exists one tier down (`flow.rs:6500` drops a run whose cascaded
  `w:vanish` is on); the per-viewer view parameter already exists twice (`ReviewView`,
  `LayoutView`, both defaulting to today's behaviour); and the byte-space projection already
  exists for hidden deletions (`casual-doc-wasm/src/lib.rs:9553`, guarded at `:32635`).
- **Folded content projects to a single boundary position**, so a caret can never land
  inside what the eye cannot see. It must **not** take `ReviewView::Markup`'s escape of
  being "never fed to caret/selection/hit-test" — the document stays editable while folded.
- **Two tiers of state, which is Google's model and the only one that can honour a Word
  file**: `w15:collapsed` is the **document default**, parsed, modelled, round-tripped and
  exported; the live per-viewer fold state is the **engine's**, read back through
  `foldState()`, and a viewer's toggling is never written into the file. Google states this
  split outright — an editor sets the saved default for everyone, a viewer's own changes
  "will not be saved". **Corrected 2026-10-04 (FOLD-001/FOLD-004):** this sentence used to
  say the live set "lives beside `docReflow` in `prefs.mjs`", and that was wrong rather than
  merely different. `prefs.mjs` persists across sessions, and `NodeId`s are **minted at
  import** — so a persisted fold set would restore one document's folds onto whatever ids a
  later open happened to assign, folding arbitrary headings of a document the reader never
  collapsed. The cross-session tier is `w15:collapsed`, which is a real document property
  with real identity, and the engine seeds the set from it at open. There is one `FoldSet`
  and it is the layout's.
- **Find searches folded text and reveals by unfolding.** `findText` walks the **model**
  (`casual-doc-wasm/src/lib.rs:3503`), so it already searches folded content by
  construction; the work is that revealing a match unfolds its ancestors. VS Code's
  behaviour.
- **A selection crossing a folded heading takes the whole subtree**, out loud: one undoable
  operation that states its scope ("Deleted section and N paragraphs"), never a quiet
  cascade. Copy copies the subtree. No source answers this for any competitor, so it is
  decided from this repository's own rule — no silent loss, and say what you did.
- **Print, PDF and DOCX export are always fully expanded.** Word reportedly prints only
  expanded content and Notion reportedly drops collapsed toggles from a PDF; **both are
  rejected** as the silent-data-loss class the engineering priority order forbids. The seam
  exists: `print.mjs`'s `withPagedLayout` already forces `Paged` with the restore in a
  `finally`, and forcing "unfolded" belongs in the same wrapper, next to the rule it is a
  rule about. DOCX export writes `w15:collapsed`, so the *state* survives while the
  *content* always does. **Built 2026-10-04 (FOLD-001):** `expandFolds` in the same wrapper,
  restoring through the new `setFoldSet` so a reader who collapsed two hundred headings pays
  one re-layout back rather than two hundred. The measurement that scoped it is worth
  recording, because it is narrower than this item implies: the real-text PDF writer and
  `exportAs` for DOCX/ODT/text were already expanded *by accident* — they encode or paginate
  from the MODEL and the session's fold set never reaches them — and the path that really
  printed folded was the 150-DPI raster fallback, which reads `pageCount`/`renderPage` off
  the live layout. The rule is enforced at the seam anyway, because `casual_doc_io::pdf`'s
  own doc comment names seeding the export's pagination from the session as planned work, and
  that change would turn the accident into the bug.
- **The accessibility mirror is filtered by the same set.** `webapp/src/a11y_mirror.mjs` is
  model-derived, so an unfiltered mirror would read out what a sighted reader has folded
  away — the fold would be a lie to one class of reader. The disclosure carries
  `aria-expanded` and the collapsed heading announces how much is hidden.
- **One state, two surfaces.** `outline_panel.mjs` becomes a real `role="tree"` of
  `treeitem`s, and **the panel's disclosure and the in-body chevron drive the same
  `FoldSet`**. Everyone else ships panel-tree-collapse and body-folding as two unrelated
  features with two states (ONLYOFFICE ship only the first, over the same generic `TreeView`
  widget their version-history panel uses); having one state is an improvement rather than
  parity, and it satisfies the ≥2-surfaces floor by construction.
- **A fold is never an access control.** Whatever the fold state, the content is in the
  file, in the export, in find, and in the accessibility mirror.

**Why this shape.** Folding is what makes a long document navigable rather than merely
scrollable, and the question it answers — "where am I and what else is there" — is a
property of the outline, not of the page. That is also why it must work in both layout
views: the heading tree does not change when the paper goes away.

**The fold range, now stated exactly** (it was not, and `157` §3.4 derives it). The
collapsed heading's **own paragraph stays visible**; the hidden range is every block from
`heading_index + 1` through `next_sibling_or_higher − 1` inclusive — all following content up
to but excluding the next heading whose outline-level *number* is the same or lower,
transitively including deeper headings inside it. A fold with no following sibling runs to
the end of its container. This agrees with Microsoft's Open Specifications wording for the
attribute ("immediately subsequent paragraphs with a higher heading level number appear
collapsed"), and it is **derived from the outline, never stored** — storing it would duplicate
a fact the heading tree already carries and let the two disagree after an edit.

### What `casual-doc-layout` owes — the specification, written here and deliberately not implemented

`casual-doc-layout` and `casual-doc-render` were **not touched** by the branch that accepted
this ADR; the filter lands in the same `flow.rs` block iteration another lane is editing, so
it is specified rather than built. Eight decisions, each with its reason:

1. **`FoldSet` is a layout INPUT, not derived — and this is a real decision, not a
   formality.** It is the set of **collapsed heading `NodeId`s** only: O(folded headings),
   which is a handful, and it is per-viewer state the shell already owns. It is threaded as
   the third view parameter beside `ReviewView` (`crates/casual-doc-layout/src/flow.rs:106`)
   and `LayoutView` (`crates/casual-doc-layout/src/document_layout.rs:162`), defaulting to
   empty so every existing caller is byte-for-byte unchanged and `geometry_snapshot.golden`
   does not move. **Deriving the set inside layout would mean walking the document to find
   collapsed headings, which is O(document) per pass and breaches `107` B1** — so it is
   passed in.
2. **The hidden RANGE, by contrast, is derived — but inside the walk that already happens,
   never in a second pass.** The named pattern is a **lexer resume state** (an incremental
   tokenizer's line-start state; equally, the paginator's existing continuation state). The
   flow pass already visits blocks in document order, so suppression is a one-integer state
   machine on that walk: on reaching a block whose `NodeId` is in the `FoldSet`, record
   `suppress_above_level = its outline level` and emit the heading normally; thereafter skip
   every block until one whose `outline_level` is `Some(l)` with `l <= suppress_above_level`,
   at which point suppression clears (and may immediately re-arm if that block is itself
   folded). Nested folds inside a suppressed range need no stack: the outer level already
   dominates. **This is why it is not a second flow path** — it is a `continue` and an
   integer.
3. **The windowed paginator needs that integer as part of its resume token.** A window that
   starts mid-document cannot know whether its first block is inside a folded range, and
   recomputing it from the start would be the O(document) cost item 1 avoids. So
   `suppress_above_level` joins whatever continuation state the windowed paginator already
   carries across window boundaries (section, numbering). **If it does not carry one, that is
   the finding, and folding must not invent a parallel one** (`SKILL` §8: one mechanism, not
   two). A fold toggle invalidates layout from the toggled heading forward, which
   `incremental.rs`/`dirty_pages` already expresses.
4. **Skip BEFORE measurement, not measure-then-clip.** The filter sits at the top of the
   per-block iteration, before shaping, before height computation, before any fragment is
   produced — the same position as the run-tier precedent, `push_styled_runs`
   (`crates/casual-doc-layout/src/flow.rs:6527`, returning at `:6539` on
   `effective.hidden == Some(true)`). *(That corrects `154` §5.3, which cites `flow.rs:6500`.)*
   A filter that measured and then clipped would pay the whole cost of the content it hides,
   which is the opposite of the point, and would be invisible in any correctness test —
   which is exactly how an O(n²) shipped here before.
5. **Reflow, not blanking.** A hidden block contributes **no fragments and no height**, so
   pagination closes up and the page count falls. A filter that merely skipped *painting*
   would leave Word's behaviour unreached and leave a blank band where the content was. This
   is the one wrong answer with a plausible shape, and it is the one ONLYOFFICE's
   architecture would force.
6. **A hidden block's page break is hidden with it; a visible heading's is not.** A
   `w:pageBreakBefore` on a block inside the folded range contributes nothing, because the
   break belongs to the block and an unlaid block lays nothing out — otherwise folding a
   section leaves a blank page, which is visible evidence of invisible content and worse than
   either alternative. A `w:pageBreakBefore` on the **collapsed heading itself** is honoured
   normally: the heading is visible and folding must never move it.
7. **Folding filters CONTENT, never DOCUMENT STRUCTURE.** This is one rule with several
   consequences and it is the subtle half of the design. A hidden block still
   **closes its section** if its `properties.section_break` says so — dropping it would change
   the page geometry, columns and header/footer of *visible* pages that precede the fold,
   which is a change to content the user did not fold. By the same rule hidden blocks still
   advance list counters, footnote/endnote numbering, bookmark and field state. The guard:
   fold a range containing a section break and assert every visible page's geometry and
   running content are unchanged; fold a range inside a numbered list and assert the visible
   items keep their numbers.
   **The one thing that does change is page numbers**, necessarily — collapsed content
   occupies no pages, so the on-screen page count is lower than the printed one. Word has the
   same property. Print, PDF and DOCX export are always fully expanded (below), so the
   *printed* numbers are the true ones; telling the reader that on screen is the chrome
   lane's item, listed below, and it is related to `151` §6.5's refusal to print a tile index
   as a page number.
8. **Complexity, stated honestly rather than optimistically.** The state machine is **O(1)
   extra per visited block**, so a full pass stays O(blocks) and a window stays
   O(blocks in the window) — *plus the hidden blocks it must step over to find the next
   visible one*. Stepping is cheap (one outline-level read, no shaping, no allocation) but it
   is **O(hidden blocks)**, so a single fold over a million paragraphs is a million cheap
   visits and the pass is **not** O(viewport) in that pathological case. **Do not claim
   O(viewport); claim O(viewport + hidden blocks stepped).** If that proves too slow the
   escape hatch is the textbook one and should be taken deliberately rather than discovered:
   a **skip list over fold boundaries**, rebuilt on a fold toggle (a user gesture, which may
   be O(document) off the main thread) rather than on a keystroke. The guard is a doubling
   test, not a millisecond threshold: fold a document of *n* and *2n* headings and assert the
   laid-out block count and the page count fall proportionally.

**Print, PDF and DOCX export are always fully expanded**, and the rule is enforced in the
wrapper rather than by each caller: `print.mjs`'s `withPagedLayout` already forces `Paged`
with the restore in a `finally`, and forcing "unfolded" belongs beside it, because a rule
enforced next to the thing it is a rule about cannot be forgotten by the next caller.

### What `webapp/` owes

Not built, and not to be built by the layout lane. The chrome lane owns it.

- **`outline_panel.mjs` becomes a real tree.** Today it is a flat list of buttons with
  `lvl-1`…`lvl-6` classes and `aria-current="location"`, with no disclosure, no
  `aria-expanded`, no `role="tree"`. It needs `role="tree"` / `role="treeitem"` /
  `aria-expanded` / `aria-level`. **ONLYOFFICE is the floor here and not the ceiling:** their
  `TreeView` sets `aria-expanded` and `aria-level`
  (`web-apps/apps/common/main/lib/component/TreeView.js:243`, `:253`) but `role="tree"` has
  **zero hits** in `web-apps/apps`, so matching them is not enough.
- **Keyboard model**, which is theirs plus the part they are missing. On a tree row:
  Right expands, Left collapses (theirs, `TreeView.js:345-350`), Up/Down move, Home/End to
  ends, Enter/Space navigates to the heading, `*` expands all under the focused row. The
  disclosure must be a real focusable control — their caret is a bare `<div
  class="tree-caret …">` (`TreeView.js:181`) with no role and no `tabindex`, which is the
  mistake to avoid, and this ADR's own `aria-expanded` requirement cannot be met by a `div`.
- **An in-body disclosure chevron** at the heading, which is Word's affordance and which
  ONLYOFFICE does not have at all, revealed on hover and on keyboard focus of the heading.
- **One state, two surfaces:** the panel's disclosure and the body chevron drive the **same**
  `FoldSet`.
- **Command ids**, in the register's existing `view.*` shape (beside `view.outline`,
  `view.reflow`): `view.fold.toggle` (fold/unfold the heading at the caret),
  `view.fold.all`, `view.fold.none`, and `view.fold.level` (a level picker). Each must be
  reachable from ≥2 surfaces, and until the layout tier lands each must ship **disabled with
  a reason** rather than as a control that does nothing (`SKILL` §10: never a dead control).
- **The live per-viewer fold state is the ENGINE's**, read back through `foldState()`, and a
  viewer's toggling is never written into the file. Not `prefs.mjs`, which is what this said
  until 2026-10-04 — see the two-tiers item above for why a persisted set is unsound.
- **`a11y_mirror.mjs` is filtered by the same set**, and a collapsed heading announces how
  much is hidden.
- **Say that the on-screen page count is not the printed one while anything is folded**
  (layout item 7).

**Consequences.**

- ~~**Add a `.docx` fixture carrying `w15:collapsed` before anything else here.**~~
  **Discharged 2026-10-02**: `fixtures/generated/collapsed-headings.docx`. It converted the
  unknown into a measurement, and the answer was the better of the two — the drop was
  **already reported** (`OmittedNotRetained`, 3 occurrences) rather than silent, through
  `body.rs`'s generic `w:pPr` long-tail arm. `154` §3.4 recorded this as unknowable for want
  of a fixture; it is now known. A reported loss is still a loss, so the state is now
  modelled and round-tripped, and the loss gate is armed for the element family — proven by
  driving `source_element_coverage` red with a writer that drops it.
- A `FoldSet` is a third view parameter on one mechanism. If it ever needs a second flow
  path, the abstraction is wrong and the design should stop.
- **Open:** whether `view.fold.level` and `view.fold.all` / `view.fold.none` ship in the
  first version (VS Code has them; neither Word nor Docs exposes a level control), and what
  the keyboard binding is — no competitor documents one, and ONLYOFFICE's Left/Right is a
  tree-row binding rather than a document one.
- **Open, and it is the layout lane's to answer first:** whether the windowed paginator
  already carries a per-window continuation state that `suppress_above_level` can join
  (specification item 3). If it does not, folding must not invent a parallel one.

## ADR-050 — A chart is a typed read projection of a retained part; the cache is the data, and the curve primitive is the shape lane's

**Status:** Accepted for implementation, 2026-10-01. Specified in
`155-DRAWINGML-CHART-MODEL-RENDERING-AND-AUTHORING-DESIGN.md`. **Closes `106` §9 Q3**
— in the opposite direction to the recommendation on file, which was
preserve-and-disclose for the v1 claim. Rows `105` FID-R-08 and `105` OO-014; depends on
`105` FID-L-04 for one slice.

**Decision.** Four parts. The first two are what keep a document product from becoming two
products; the fourth is the one that sets the delivery order.

1. **A chart is a typed READ PROJECTION of a retained part, never a replacement for it.**
   `word/charts/chart1.xml` and everything reachable from it are already retained
   byte-for-byte with a ledger record (`casual-doc-import/src/opaque.rs`,
   `casual-doc-import/src/lib.rs:512-644`), and export already re-emits them verbatim. The
   projection is a derived read index over those bytes — the same cache-plus-source-of-truth
   shape as `ExportMode::ExactIfUnchanged` and as `34` §4's provenance map. Export copies the
   part while the chart is clean; it regenerates only when the chart is dirty; and a chart
   whose projection is incomplete **cannot be made dirty** — the operation is refused rather
   than allowed to silently drop a trendline. A chart we fail to project behaves exactly as it
   does today. Adding the projection changes no output byte, and a guard asserts it.
2. **The cached data table is the data. The embedded workbook is never opened.**
   `c:numCache`/`c:strCache` are read into the model; `c:f` is carried verbatim as an opaque
   string and never parsed, resolved or evaluated; `c:externalData`'s workbook is a typed
   *pointer* (`EmbeddedPart` — a part name and a relationship id, per `45` I4) to a part that
   stays opaque bytes. Reading `word/embeddings/*.xlsx` would require a SpreadsheetML reader,
   a cell model, a reference resolver and then a formula evaluator and a dependency graph —
   that is `opencalc`, a separate product. **ONLYOFFICE's source settles the rendering half
   outright and demonstrates the cost of the other half.** Their chart rendering never consults
   a workbook — `recalculateReferences` begins `if (!oThis.worksheet) return;`
   (`ChartSpace.js:5631-5632`) and `worksheet` is set only from `cell/` code, never from
   `word/` — so in their document editor a chart draws purely from the caches, exactly as this
   decision does. But every place they *touch* the workbook is a place a spreadsheet had to be
   brought in: at save time `getXLSXFromCache` (`ChartSpace.js:2143-2167`) constructs
   `new AscCommonExcel.Workbook(...)` and serialises it with `BinaryFileWriter`, so their
   document bundle links the spreadsheet engine; and Edit data launches a **complete second
   editor** in an iframe (`DocsAPI.DocEditor({documentType:'cell', …})`,
   `ExternalDiagramEditor.js:63-92`, fed through `CFrameDiagramBinaryLoader`,
   `sdkjs/common/frameManager.js:849`, reached from `word/api.js:9611`), which then parses
   `c:f` to rebuild a workbook when the chart carried none (`fillWorkbookFromDiagramCache`,
   `frameManager.js:488-497`). That is the accidental growth this part refuses, in the code of
   the product we are an alternative to. The cost is confined to one interaction
   and is paid by provenance: data editing is allowed on a chart **we** authored, where we own
   both the cache and the workbook, and on an **imported** chart it ships **disabled with a
   reason** rather than silently desynchronising the two.
3. **Scope is the families that appear in documents, and combo is free.** Tier 1 is
   bar/column, line, area, scatter, pie and doughnut. The plot area is modelled as a *list of
   chart groups* and the axes as a *list*, so a combo chart and a secondary axis are the
   absence of a restriction rather than two later features — putting a chart-type enum on the
   chart instead of on the group is the mistake that makes both of them separate work. 3-D,
   surface, stock, radar, bubble, pie-of-pie, trendlines, error bars, data tables,
   `chartUserShapes` and all of `chartex` are out, each `omitted`-or-`degraded` +
   `preserved` and reported per `35`.
4. **`PaintItem::Path` is required before pie and doughnut, and it belongs to the shapes
   lane, not to charts.** Bar, column, line, area and straight-line scatter need only the
   `Rect`, `Line` and `Polygon` primitives the display list already has. A pie sector is an
   arc, and there is no arc, quadratic or cubic anywhere in `PaintItem`
   (`casual-doc-layout/src/display.rs:189`, `:114-152`) — `105` FID-L-04 is correct. A
   many-sided polygon fan is **rejected**: `119` §6 already named per-shape vertex lists as
   the wrong axis, `SKILL` §8 forbids a second mechanism for one rule, and ONLYOFFICE's own
   pie is an `arcTo` between two radii (`drawPieChart::_calculateArc`,
   `ChartsDrawer.js:12448-12467`; the doughnut's annulus is two arcs and two radii at
   `:14284-14314`) with no polygonisation anywhere. They share **one rasteriser, one curve
   vocabulary and one arc flattener** with preset shapes (`CShapeDrawer`, `Path2`, and
   `ArcToCurvers`/`EllipseArc3` at `ArcTo.js:216`, which `Geometry.js:1556` also calls) while
   building chart geometry in their own arena — so the layer they share is precisely the layer
   `PaintItem::Path` is. The primitive is therefore built once for its four consumers (~165
   presets, `a:custGeom` curves, SmartArt, charts), flattening arcs in the backend rather than
   once per caller, and delivery splits: **tier 1A** (bar, column, line, area, scatter and all
   furniture) ships on today's primitives; **tier 1B** is **pie and doughnut only** and keeps
   today's reported placeholder until the primitive lands.
   `c:smooth` is deliberately **not** in 1B: a smooth series is a curve *sampled into a
   polyline*, not a shape outline, which is what their shipping code does — ten straight `lnTo`
   segments per interval (`calculateSplineLine`, `ChartsDrawer.js:5435-5468`), with the
   true-Bézier variants present but switched off behind `//TODO … draws incorrectly. check!`
   (`:9365`, `:15237`).

**Why this reopens Q3 at all.** The recommendation on file was preserve-and-disclose, on the
grounds that it is honest and cheap. It is honest; it is not what is shipping. `155` §1
measures the disclosure against the code: `preview: None` is hard-coded for both
`EmbeddedKind::Chart` and `EmbeddedKind::Diagram` (`casual-doc-import/src/body.rs:5472`,
`:5498`), and the only code that ever sets `preview` reads `v:imagedata` inside a `w:object`
— legacy OLE. So "an embedded preview if the file supplies one, else a placeholder" is false
for charts: a chart is **always** the literal text `[chart]`, in an unstyled run, reflowing
the paragraph (`casual-doc-layout/src/flow.rs:3717-3755`). That is a weaker fallback than the
one `w:altChunk` already gets, which is a labelled dashed box. Preserve-and-disclose was
costed against a fallback that does not exist.

**Consequences.**
- `casual-doc-model` gains a `charts: DefinitionMap<ChartId, Chart>` table on `Definitions`,
  each `Chart` anchored to its `EmbeddedObject` by `NodeId` (`45` I3) — **not** a field on
  `EmbeddedObject`, both because a projection is derived data (I4) and because `SKILL` §5a
  shape 1 makes a new field on that struct a cross-lane break of every literal in five
  crates other lanes hold. `Definitions` is the established additive seam.
- **No float may enter the model.** `Definitions` derives `Eq` and the v1 model contains zero
  `f64`/`f32`. A cached number is therefore stored in its **verbatim lexical form** and parsed
  by the consumer — which a byte-faithful rewrite needs anyway, since `"4.30"` and `"4.3"` are
  the same number and different documents.
- The compatibility report gains per-construct chart findings, so a chart with a trendline
  stops reporting one line about a whole part and starts naming what was not understood. That
  is a `degraded` + `preserved` row where today there is only `omitted` + `preserved`.
- `a:graphicData@uri` is captured and never read (`body.rs:2776-2778`); routing is by the
  local name `chart` alone (`:2780-2782`), so any element named `chart` with an `id` in any
  namespace is taken as a DrawingML chart, and `EmbeddedKind::Other` is unreachable from
  import as a result. Verifying the uri is a precondition of this decision and ships with it.
- **No published grade moves and no `105` row closes until a chart actually draws** (`SKILL`
  §9 rule 4). `155` §10 records which increments are user-reachable and which are not, and
  records that `fidelity.js`'s `modeled: "full"` for Charts is itself an overstatement while
  no chart data model exists.
- Deliberately not decided here: whether an *imported* chart's data becomes editable behind
  an explicit replace-the-workbook confirmation (`155` Q-A — it destroys producer-authored
  content, so it is the owner's call), whether `PaintItem::Path` carries an explicit `ArcTo`
  (`155` Q-B — recommended yes, on their evidence), and floating `wp:anchor` charts, which are
  a pre-existing `EmbeddedObject` limit shared with SmartArt and OLE rather than a chart one
  (`155` Q-C).

## ADR-051 — An operation carries the identity *space* it mints in, and `apply` holds no generator

- **Status:** Accepted, implemented.
- **Date:** 2026-10-01.
- **Context:** `150` §9.3 / `152` §10 Q8. `casual_doc_edit::apply` does not only move bytes:
  some operations **create nodes**. A `FormatText` whose range starts mid-run splits that run
  and the tail half is a new `Run`; so do `DeleteText`, `SplitParagraph`, `CreateBookmark`,
  `InsertField`, `InsertInlineObject`, `InsertFieldRange` and `InsertNote`. Those identities
  came from the *applier's* generator, so two replicas produced the same document under
  **different names for the same run**. Nothing broke at once, because no operation in the set
  addresses a run — but `25`'s deterministic snapshot was then not byte-identical across
  replicas, which defeats *"a snapshot can be verified rather than trusted"*; every convergence
  assertion in `casual-doc-transaction` had to normalise run ids away before comparing; and it
  is the reason `152` §9 refused to freeze a byte codec.
- **Decision:** An operation travels with a **`Mint`** — a private, aligned run of `NodeId`
  counters — and `apply(document, mint, operation)` derives every identity it creates from it.
  `apply` takes **no** `RunIds`, so it cannot name a node the operation did not pay for, and it
  is a pure function of `(document, mint, operation)`: replayable, and therefore verifiable.
- **Prior art, named:** deterministic identity derived from the creating operation — Yjs's
  `(client, clock)`, Automerge's `(actor, counter)`. `casual_doc_model::IdSpace` (ADR-047)
  partitions the namespace half of a `NodeId` per participant; this partitions the counter half
  per operation. They compose: an id minted in a lane is still in its author's `IdSpace`, so
  `WireOperation::localise` checks an arriving mint with the same rule it already applied to a
  declared id.
- **Rejected: enumeration.** `150` §9.3 proposed the operation carrying the new run's id, as
  `SplitParagraph` carries `new_id`. It cannot work. The *number* of identities an operation
  mints depends on the document it lands on, and an operation is **transformed** before a remote
  replica applies it, so the state it lands on there is not the state its author saw. An
  enumeration is a count fixed at authoring time; the truth is discovered at application time. A
  *space* is the only declaration that survives a transform. A consequence worth stating: **no
  `Operation` variant changed**, so the codec's remaining blockers are `150` §9.1 and §9.2 alone.
- **Rejected: re-mapping on receipt.** Rewriting an arriving id into a local one is what the
  sibling does for an interned *value*. Here the id **is** the identity, so the two replicas
  would disagree about the name of one logical node permanently — the very thing this closes.
- **Shape.** Three nested levels, all bit fields rather than hashes, so distinctness is by
  construction:

  | Level | Width | Separates |
  | --- | --- | --- |
  | namespace (`IdSpace`) | 2⁶⁴ | participants, plus the document's own and offline spaces |
  | block | 1024 counters | one operation from the next |
  | lane | 128 counters | an operation from its inverse, and a `Rebase::KeepMany`'s pieces from each other |

  `Mint::inverse` flips one bit and is therefore an **involution**: undo followed by redo
  re-mints exactly the identities the original edit created, so an anchor into a destroyed run
  finds that run again when the redo restores it. A lane is a **bound**: an operation wanting
  more than 128 identities — about thirty times the worst case in the set — earns
  `EditError::IdExhausted` rather than wrapping into the next lane.
- **Consequences.**
  - `Transaction` and `Commit` carry one `Mint` per operation; `Transaction::reserve` is the one
    place an author's generator is touched on the edit path, and it is inside the facade's single
    choke point, pinned by `every_document_mutation_is_a_transaction`. `RevisionLog::apply` lost
    its `ids` parameter: **the log cannot mint.**
  - `WireOperation::of` takes the mint, and `localise` refuses a mint outside the sender's space.
  - A rebased commit declares the spaces the **replay** used, not the template's — proved by
    mutation, because declaring the template's is exactly the plausible mistake.
  - `Document::node_ids` is public, because "what identities does this document hold" stopped
    being inferable from "what did the allocator advance past" — a reserved block is 1024
    counters and an operation spends two or three. Two wasm guards were reading the allocator
    and now read the document, which is the guarantee they were always about.
  - Unchanged: the TP1 diamond still compares modulo run ids, correctly — its two sides apply
    *different* operation sequences. Convergence *with* identity is a protocol property and is
    where it is now asserted, with no quotient.
- **Mutation proofs.** (1) Receiver mints from its own generator instead of the carried mint →
  `a_run_an_edit_split_off_carries_the_same_identity_on_both_replicas` fails with the two split
  ids differing while the three pre-existing ids match. (2) A rebased commit stores
  `template.mints` → `a_replay_over_a_remote_edit_leaves_both_replicas_naming_every_node_alike`
  fails with Ada naming Grace's replayed runs one block away from Grace's own.
- **Not decided here:** identities inside a carried subtree (`152` §10 Q2) are still covered by
  the id-space rule at the mint and by `Document::validate`, not by an enumerating walk.

## ADR-052 — Document protection is enforced at the operation, by projection equality, and it is policy rather than security

- **Status:** Accepted, implemented for `readOnly`, `comments` and `trackedChanges`.
- **Date:** 2026-10-01.
- **Context:** `107` 6.7. The model has carried `DocumentProtection` with all four editing
  levels since Layer 1, and import and export round-trip `w:documentProtection`'s three policy
  attributes. Exactly one level was ever *enforced*: `w:edit="forms"`, at the facade's mutation
  choke point. `readOnly`, `comments` and `trackedChanges` were modelled, exported, and
  **ignored** — so a document a reader could see was protected was fully editable. That is the
  "modeled is not shipped" failure the working contract calls this repository's most expensive
  recurring pattern. (`153` reports the whole feature as missing, which overstates it in the
  other direction.)
- **Decision:** Enforce at the **operation**, in `casual-doc-edit`, not in the facade.
  - An operation is the only thing that can be judged; a toolbar says what a host offered.
  - The relay will need the same judgement on an *arriving* operation (`152` §10 Q5), and a
    rule written in the facade could not be reused by it.
  - `Operation` lives in that crate, so the classification is an **exhaustive match**: a 59th
    operation is a compile error rather than a permission hole. A `_ =>` arm is exactly how one
    would arrive exempt from every restriction.
  - `w:edit="forms"` stays in the facade, alone, because its answer needs state the engine
    cannot see — whether the reader is inside an enabled text form field *right now*.
    `protection::forms` decides "is a restriction in force" so that half is not duplicated.
- **The hard part: `comments` and `trackedChanges` are not variant-level questions.**
  Commenting, suggesting a change, and accepting one **all travel as
  `Operation::UpdateReviewState`**, whose payload is a replacement inline list per paragraph
  plus an optional comments table. No rule that looks at *which* operation arrived can tell
  them apart, so it would have to allow or forbid all three.
- **Answered by projection equality — exact, with no heuristic and no tolerance:**
  - **`comments`**: strip every comment marker from the current inlines and from the
    replacement; allow it if the remainders are equal. A comment anchor adds markers and
    nothing else; a character left behind in the remainder is refused.
  - **`trackedChanges`**: project both to *the text as it stood before the change* — content
    outside a revision, plus content inside a `Deletion`/`MoveFrom` (still present, struck
    through), and not content inside an `Insertion`/`MoveTo` — and require equality **plus**
    that no revision now in the document is missing from the replacement.

    The second condition is what catches a **rejection**, and it is not redundant: rejecting an
    insertion restores the before-state, so the projection alone is satisfied, and only the
    vanished revision distinguishes it from nothing having happened. A mutation removing that
    condition reddens exactly the reject case and nothing else. Accepting, in either direction,
    changes the projection; so does untracked typing. Four review decisions and one untracked
    edit, refused by two conditions rather than five special cases.
  - Word's tracked-changes restriction permits comments, so that level is a **superset** of the
    comments level and the projection drops comment markers too.
- **A batch is judged whole**, on its worst operation and not its first — the sibling's rule,
  so a permitted comment cannot carry an edit in behind it.
- **An operation naming a paragraph that is not there is refused**, not waved through: the rule
  the forms check already followed, because an unplaceable write into a locked document is
  exactly where guessing is wrong.
- **Complexity:** O(the inlines the operation names). `readOnly` resolves nothing, and an
  **unprotected** document costs no document walk at all — asserted with `block_visits`, because
  this runs on the keystroke path and `107` §4 B1 forbids a document walk there.
- **This is policy, not security, and that must be said out loud.** Nothing here authenticates
  anybody. It stops a *host* and a *user* doing what the document asks not to be done; it does
  not stop a program that edits the model directly, and it never could — a local-first engine
  hands the reader the bytes. Access control against an untrusted client belongs to the relay
  and to a host-signed grant (`152` §10 Q4/Q5), and this ADR is not it.
- **Password material: deliberately not modelled, and the decision is recorded rather than
  deferred silently.** `w:documentProtection` can carry `w:cryptProviderType`, `w:hash`,
  `w:salt`, `w:cryptAlgorithmSid` and `w:cryptSpinCount`. Verifying one would let a host offer
  "unprotect", and would invite the claim that the restriction is enforced against an
  adversary. It is not: the legacy hash is trivially removable by editing one attribute in the
  XML, and Word documents it as a deterrent. So opendoc **will not** implement password
  verification as a security boundary. What it may implement is the *shape* Word has — prompt,
  compare, lift — labelled as a deterrent, and only once a host asks for it.
- **A loss to report, in another lane's crates.** Derived from the code rather than from a
  fixture run: `casual-doc-import`'s settings parser builds `DocumentProtection` from `w:edit`,
  `w:enforcement` and `w:formatting` and returns *handled*, so the five crypto attributes are
  neither modelled nor reported; `casual-doc-export`'s semantic writer emits those same three.
  A **semantic** round trip therefore returns a password-protected restriction as a
  password-less one — the protection survives and becomes liftable in Word with no password,
  silently. Retention mode keeps the original part bytes, so the loss is semantic-mode only.
  That is `AGENTS.md`'s no-silent-data-loss rule, and it belongs to the import/export lane.
- **Update (2026-10-04): that loss is now reported, and the group is larger than this ADR
  said.** `casual-doc-import`'s settings parser raises a named `Degraded` attribute finding
  per password attribute it sees — `documentProtection/@hashValue`, `writeProtection/@salt`
  and so on — through the same `report_attribute` seam `numbering.rs` uses, so the loss
  reaches the host's compatibility report instead of vanishing. Two corrections to the
  paragraph above: the group is **sixteen** attributes, not five — `AG_Password`'s twelve
  (`w:hash`, `w:salt`, `w:cryptProviderType`, `w:cryptAlgorithmClass`,
  `w:cryptAlgorithmType`, `w:cryptAlgorithmSid`, `w:cryptSpinCount`, `w:cryptProvider`,
  `w:algIdExt`, `w:algIdExtSource`, `w:cryptProviderTypeExt`,
  `w:cryptProviderTypeExtSource`) plus `AG_TransitionalPassword`'s four
  (`w:algorithmName`, `w:hashValue`, `w:saltValue`, `w:spinCount`), which is the ISO
  verifier form Office writes *instead* of the legacy one when `UseIsoPasswordVerifier` is
  set, so a modern Word file's password material may be entirely in attributes this
  register did not name. And the loss is not confined to `w:documentProtection`:
  `CT_WriteProtection` carries both groups too and lost them the same way. The decision
  **not** to verify or re-emit password material stands, untouched and still open; what
  changed is only that it stopped being silent. `docs/160` §7 item 5 asked for exactly
  this, as "report at minimum".
- **A second document-safety defect in the same element, found with it and fixed with it:
  `w:enforcement` has three states and was read as two.** MS-OI29500 Part 1 §17.15.1.29:
  "Word enforces protection when this attribute is missing." The importer treated an
  **absent** attribute as off, so a document Word opens read-only opened here fully
  editable; the exporter **omitted** the attribute when the restriction was off, so a
  restriction an author deliberately switched off was written in the one form Word then
  enforces. One reading silently loosened incoming documents and the other silently
  tightened outgoing ones. Import now resolves absent to **enforced** and export always
  writes the attribute explicitly, `"0"` included. The `w:enforcement="0"` handling this
  module already had was right and is unchanged — that is the state Word writes when an
  author sets a restriction up and then stops it, and `protection.rs` depends on it.
- **Also not built here:** there is no operation that *sets* protection, so a reader cannot
  restrict or unrestrict editing from the product at all — protection can only arrive from a
  file. Closing that means a new definitions operation (ADR-030 I2), which is a decision and
  not an oversight.
- **Mutation proofs.** (1) `readOnly` falls through with `none` — the behaviour before this
  module — and the read-only guard fails on `InsertText`. (2) The revision-id condition is
  removed and **only** the reject case fails, proving it is load-bearing. (3) Only the first
  operation of a batch is judged and the batch guard fails. (4) A `Revision`'s children are
  cloned instead of projected and the container-set equivalence guard fails.

## ADR-053 — O(1) paragraph resolution is a session-owned identity index maintained at the choke point, not a cache on the document

- **Status:** **Proposed.** The decision is recorded; the work is not done here.
- **Date:** 2026-10-01.
- **Context:** `107` §4 **B1** requires per-keystroke work to be O(1) in document size, and
  §4.1 records that **no editing path meets it** — `blocks_owning_mut` and
  `find_paragraph_mut` walk the surfaces to find a paragraph by id, so every keystroke is
  O(document). HF-111 was the *review-specific* part of that general fact. Measured: an
  ordinary keystroke on 200 paragraphs costs 400 block visits, i.e. two full walks, so
  resolution is essentially the whole per-keystroke document cost.
- **How large the masked cost is, stated rather than implied.** Editing is refused above
  `MAX_WHOLE_LAYOUT_BLOCKS` = 262,144 top-level blocks, because a windowed body cannot
  re-paginate after a mutation. So the worst editable document today costs about **524,000
  block visits per character**, and the 1.3-million-paragraph case is masked by a refusal
  rather than served. **That inverts the priority order**: windowed *editing* (`113`) cannot
  be built on O(document) resolution, so this is a prerequisite for it and not an
  optimisation of it.
- **Named prior art, and why four of five candidates are rejected.**

  | Candidate | Verdict |
  | --- | --- |
  | **Positional index** — store `(Surface, path of child indices)` per id | **Rejected.** Every block insert or delete shifts its later siblings' indices, so the index needs the band arithmetic that `150` §2 says `NodeId` anchors were chosen to escape. It reintroduces coordinate fragility to fix an identity lookup. |
  | **Rebuild-on-demand cache with a dirty flag** | **Rejected, and worth recording because it is the obvious idea.** A keystroke *is* a mutation, so it invalidates the cache it was about to use: the hit rate on the typing path is zero. |
  | **Reference index on the document** (`ParagraphIndex` stored beside the paragraphs) | **Rejected.** It borrows the paragraphs it points at, so storing it in the document is self-referential. This is the obstacle §4.1 already names. |
  | **Generational arena plus handle map** — the model's nodes move into an arena, the tree holds handles (Yjs's `StructStore`, and every ECS) | **Right end state, wrong increment.** It is the textbook answer and it makes staleness structurally impossible, because a handle is not a position. It also re-founds `casual-doc-model`, touches import, export, layout and render, and is not a change one lane can make safely. |
  | **Session-owned id→location index, maintained at the mutation choke point** | **Chosen.** |

- **Decision.** The index is owned by the **editing session**, not by the document, and it is
  **maintained incrementally at the one mutation choke point** rather than rebuilt.
  - The self-reference disappears: the index lives beside the document, not inside it.
  - Staleness is structurally bounded by ADR-030 **I1**: every mutation goes through
    `casual_doc_edit::apply`, so an index updated there cannot miss a change without a
    compile-visible change to the choke point.
  - The deltas it needs are **already produced**. `apply` returns the inverse and the envelope
    already derives `MappingStep`/`PositionMap` from the pair to move *positions* across a
    commit; an id→location index consumes the same structural facts. One mechanism extended,
    not a second one added — which is the rule the windowed paginator followed.
  - It carries a **verification mode**: rebuild with `ParagraphIndex` and compare. A stale
    index is a correctness bug where a rebuilt one is merely O(n), so the guard has to be able
    to see disagreement rather than trust the maintenance.
- **What it does not fix.** Resolution becomes O(1); the *rest* of a keystroke does not. §4.1's
  remaining 2× on a suggested keystroke (the facade resolves the paragraph to build the review
  projection, then the edit crate resolves it again) is a separate item, and it becomes
  cheaper rather than moot.
- **Owner decision needed:** whether to take this increment at all before windowed editing is
  wanted. B1 is an owner constraint and is currently met by nothing, so the alternative is to
  restate B1 as "O(1) above the windowing threshold, linear below it" — which is a weaker
  promise honestly kept, and is the only other coherent position.

  **Addendum, 2026-10-02 — ONLYOFFICE's source answers this, and it adds a third option. The
  status stays Proposed; the false choice is removed.** Full evidence and citations in
  `157-ONLYOFFICE-LARGE-DOCUMENT-AND-FOLDING-SOURCE-FINDINGS.md` §2, read from
  `reference/sdkjs` at `72b0421` and re-verified rather than taken from a report.

  - **They have no document-size ceiling in the editor at all.** `MAX_PARAGRAPH_COUNT`,
    `MaxParagraphs`, `MAX_ELEMENTS`, `c_oAscMaxLength` → **zero hits** in `sdkjs`; no
    page-count cap outside spreadsheet printing; no "document too large" string. The only hard
    refusal is **server-side at open** on file size
    (`c_oAscServerError.ConvertLIMITS` → `ConvertationOpenLimitError`, threshold not in the
    tree), and the only in-editor valve is on accumulated *edit volume*
    (`baseEditorsApi.prototype.checkChangesSize`, `common/apiBase.js:1776`), **disabled by
    default** (`maxChangesSize = 0`). During editing, nothing happens at any size: no refusal,
    no warning, no mode change. **So this ADR's `MAX_WHOLE_LAYOUT_BLOCKS` = 262,144 editing
    refusal has no analogue in the product we are replacing**, and it cannot be defended as
    normal for the category.
  - **Their per-keystroke cost is O(current paragraph), with no term in document size — and
    they get there WITHOUT an identity index.** Resolution on the typing path is a cached
    integer per nesting level: `CDocument.CurPos.ContentPos` dereferenced directly
    (`word/Editor/Document.js:20023`, `var Item = this.Content[nContentPos];`), then
    `Paragraph.CurPos.ContentPos` (`Paragraph.js:4614`), then `ParaRun.State.ContentPos`
    (`Run.js:860`). The reverse direction is a cached `Index` on each block, repaired by a
    **lazy suffix reindex** armed only by structural edits
    (`Update_ContentIndexing`, `DocumentContentBase.js:118`; `private_ReindexContent`,
    `:345`). Typing a character leaves `ReindexStartPos === -1` and the repair costs nothing.
    They *do* have an id→object map (`CTableId`, `common/TableId.js:95`) — **the shape this
    ADR proposes** — and it serves serialisation and co-editing, not resolution.
  - **Recalculation is three tiers** — run-range, whole paragraph, then a page-at-a-time sweep
    time-sliced at 10 ms / 50 pages (`Document.js:2881-2890`, `IsContinueRecalculateOnTimer`
    at `:4335`, `GetCalculateTimeLimit` in `word/Editor/Layout/Base.js:189`). An ordinary
    keystroke takes tier 1 and repaints one page.
  - **Virtualised painting with real eviction (`m_lDrawingFirst`/`m_lDrawingEnd`,
    `StopRenderingPage`, `CCacheManager`); virtualised LAYOUT nowhere** — geometry is computed
    for the whole document and never discarded, and loading is eager and synchronous. So
    `113`'s windowed layout is ahead of them, not catching up.

  **What this changes.**

  1. **It argues against the second alternative.** Weakening B1 to "O(1) above the windowing
     threshold, linear below it" would document a limitation as a design, on a constraint the
     competitor meets in its strong form. Keep B1 as written.
  2. **The inversion of priority in this ADR stands**, unchanged: windowed editing cannot be
     built on O(document) resolution, whichever mechanism replaces it.
  3. **The mechanism is open again — and this ADR's own rejection of a positional index is the
     reason to look again rather than to switch.** That rejection ("every block insert or
     delete shifts its later siblings' indices") is correct and is not withdrawn. What the
     competitor shows is that the shift is affordable **when it is a lazy suffix repair armed
     only by structural operations** — O(n − insertion point) on an Enter, nothing on a
     character. It was weighed here as a *replacement* for identity anchors, which it must not
     be: `150` §2 chose `NodeId` for OT and that is not reopened. The narrower question, for
     the owner:

     > Is per-keystroke resolution better removed by **caching the caret's location beside the
     > session** (a chain of child indices, invalidated by the structural operations that
     > already pass through the one choke point), by **this ADR's id→location index**, or by
     > both — the cache for the caret, the index for everything that arrives by id (a remote
     > operation, a comment anchor, a find hit)?

     `157` poses that question and deliberately does not answer it: it is an architecture
     decision about our own mutation path with ADR-030 I1/I3 in it, and the measurement that
     would settle it — how often a real session takes a structural operation versus a
     character — has not been made. **Unverified:** whether a cached chain can be maintained at
     `casual_doc_edit::apply` as cheaply as the index can.
  4. **One caution against over-reading the comparison.** Absence of a ceiling in their source
     is **not** evidence that they survive documents at the scale of our refusal. Nothing was
     executed, and their own 10 ms time-slicing exists precisely because a large document is
     slow.

## ADR-054 — A tracked keystroke must be a granular operation, and that — not the session — is what unblocks suggesting mode

- **Status:** **Proposed.** It changes the operation set, which ADR-030 I2 reserves to the
  owner.
- **Date:** 2026-10-01.
- **Context:** `150` §6 and `152` §11 record a blocker: a `Coalesce::ContinueKeepingFirstInverse`
  commit records **no inverse**, so `Commit::changes()` is `None`, so it cannot be rolled back
  — and `152` §5.3 concludes that **suggesting mode cannot take part in a session at all**,
  because the rollback-and-replay driver needs every unacknowledged commit's inverse.
- **The finding: this is a symptom, not an independent problem.** Review typing is expressed as
  `Operation::UpdateReviewState` — a **whole-paragraph rewrite**. Its inverse is therefore a
  whole-paragraph snapshot, one per character, which is the only reason the coalescing mode
  that *drops* inverses exists. And `107` §4 **B4** already forbids exactly this: *"No
  operation on the typing path rewrites a paragraph. `SetInlines` is a paragraph-rewrite
  vehicle and must stay off that path."* Review typing violates B4, and the session blocker is
  that violation's consequence.
- **Decision (proposed).** Express a tracked keystroke granularly — text inserted *inside* a
  revision wrapper — so its inverse is a `DeleteText`-sized delta rather than a paragraph
  snapshot. Then:
  - every commit can afford to keep its inverse, so `ContinueKeepingFirstInverse` is not needed
    on the typing path and the rollback driver has what it needs;
  - **`152` §5.3's blocker closes without any change to the session**, which is the strongest
    argument that this is the right place to fix it;
  - B4 holds for the review path as it already does for the plain one;
  - the transform gets a granular subject to rebase instead of a paragraph rewrite, which
    `150` §5 refuses against a concurrent split or join — so it also removes refusals.
- **Rejected alternative: group-granular rollback.** Roll the whole undo group back using its
  first commit's snapshot. It restores the paragraph correctly, but the driver needs the state
  at *each* commit's own base to compute the arrival's image there, and a group-granular
  rollback cannot produce those intermediate states. It would trade a refusal for a
  divergence.
- **Rejected alternative: keep the snapshots.** Correct, and it costs one whole-paragraph copy
  per character — the memory the coalescing mode was invented to avoid. On a long paragraph
  that is the quadratic-feeling cost `147` records the flat history stacks having had.
- **The second reason suggesting mode cannot join is separate and still open**: `w:id`
  collisions between replicas (`152` §10 Q7). Two replicas minting revision `w:id`s
  independently produce colliding opaque ids on export. ADR-051's identity partition covers
  `NodeId`s and **not** `w:id`, which is a producer string. Recorded here so the two reasons are
  not mistaken for one.
- **Owner decision needed:** whether to add the operation. Until then suggesting mode stays
  out of a session, with the refusal code `152` §5.3 gives it.

## ADR-056 — `150` §9.1 and §9.2 are answered on the envelope, and the closed operation set does not change

- **Status:** Accepted, implemented.
- **Date:** 2026-10-02.
- **Context:** `152` §9 named exactly two remaining blockers for the byte codec, and said
  freezing bytes over them is *"the one thing a compatibility surface must not do"*:
  - **`150` §9.1** — `InsertBlocks`/`DeleteBlocks`/`InsertTable`/`InsertFieldRange` name a
    sibling **index**, which is why the `BlockPlacement` seam exists and why refusals U2 and
    U6 exist;
  - **`150` §9.2** — `Pos` carries no **affinity**, which is refusal U5.

  Each was recorded as needing a change to the closed operation set, which ADR-030 I2 reserves
  to the owner.
- **Decision: neither change is made, and neither is needed.** Both facts travel on the
  **envelope**, beside the operation, as an `Intent`. **No `Operation` variant changed and
  `Pos` is untouched.** `transform` gains `transform_declared`, which reads the declaration;
  `casual_doc_transaction::resolve_anchor` re-derives a slot's index from its anchor.
- **Why the envelope is the right place, and not a convenience.** Neither fact changes what
  `apply` does. An index is what `apply` consumes and it is already exact on the author's own
  replica; an affinity is not consumed by `apply` at all, because
  `casual_doc_edit::insert_text`'s attachment rule is fixed and changing it would break
  convergence (`150` §5.4). Both are **authoring** facts — true of the moment an operation was
  written, needed only by a *transform*, and needed only on a replica that did not write it.
  A receiver cannot reconstruct one, which is why it must travel; `apply` must not read one,
  which is why it must not be in the operation.
- **The precedent, named:** ADR-051, one increment earlier, for identity. `150` §9.3 proposed
  that an operation enumerate the ids it mints; that could not work, and the answer was to
  carry the *space* on the envelope. This is the same move twice more, and the consequence is
  the same one: the two findings that were about to move the **existing** shapes no longer do, so
  the codec is unblocked **without a waiver**. Not "the set is frozen", which would be an
  overstatement — ADR-054 and ADR-059 both propose an addition to it. ADR-057's format is additive
  under one by construction rather than by promise: an older decoder refuses an unknown variant by
  name rather than misparsing it.
- **Measured, not assumed.** Taking §9.2 in the op set was tried first: adding one field to
  `Pos` fails the build with two `E0063` in `casual-doc-wasm` (`lib.rs:2311`, `lib.rs:5712`) —
  a file another lane holds. That is precisely the "two green PRs can make `main` red" shape
  §5a of the working contract records, and it is what makes the op-set route **irreversible**
  where an envelope field is additive: a transaction that declares nothing behaves exactly as
  it did, so nothing has to be updated at once and a decoder meeting an unknown declaration
  skips it.
- **Shape.**

  | On the envelope | What it says | Read by |
  | --- | --- | --- |
  | `Intent::anchor` → `BlockAnchor::Before(NodeId)` / `AtEnd` | the block an index was counted to | `transform_declared`, `resolve_anchor` |
  | `Intent::affinity` → `Affinity::Before` / `After` | which side of a boundary the content belongs to | `transform_declared` |

  `Transaction::with_intents` is a **builder**, not a parameter on `new`/`reserve`, for the
  same reason the decision itself went this way: a parameter is a breaking change to every
  call site. A missing entry reads as `Intent::NONE`, which is what every caller declared
  before this existed. `Commit` retains them, because a commit becomes `against` for the next
  arrival; `WireOperation::declaring` carries them, because a receiver cannot recompute them.
- **An anchor is rebased by doing nothing.** A concurrent split carves out a new block and a
  concurrent join destroys one; neither *renames* the anchor, so the intention "put it between
  the same two neighbours" survives untouched and `transform` returns the slot unchanged.
  The cached index is re-derived **once**, by `resolve_anchor`, immediately before the
  operation is applied — which is the only moment the state it must be right about exists.
  That is also why resolution is not inside `transform`: an arrival is rebased over *every*
  commit since its base revision, and resolving against an intermediate state would have to be
  rebased again, reintroducing the arithmetic the anchor removes. `transform` stays pure.
- **`BlockTarget` is a second trait over one `BlockIndex`, deliberately.** `BlockPlacement`
  must describe the **base** state, which neither replica holds while it transforms;
  `BlockTarget` describes the state the caller is **about to mutate**, which every caller holds
  by definition. One is a promise that is hard to keep and the other is a fact in hand, and the
  names are what stop the wrong one being passed. One index answers both.
- **What this closes, stated without overstating.**
  - **U2 for insertion gaps** — `InsertBlocks`/`InsertTable`/`InsertFieldRange` over a
    concurrent split or join now need **no placement at all**.
  - **Half of U5** — a declared `Affinity::Before` is answered. `Affinity::After` is the
    commoner intent (typing at a paragraph start takes the following character's formatting in
    Word and in Docs) and remains **refused**, because no operation in the set attaches text to
    the run *after* a boundary. Declaring it does not make it expressible; it makes the refusal
    say the engine understood and cannot comply, which is a different and better statement than
    "nobody knows what you meant". Closing it properly needs either an operation that attaches
    on the following side or a run-property argument a pure transform cannot read.
  - **A destroyed anchor is a reportable loss**, never a fall-back to the stale index.
- **Still open, and not quietly treated as covered.** `DeleteBlocks` is *not* anchored: a
  removal band is not one anchor but a span of victims, and the node-addressed form of that is
  a list the operation does not carry. Refusal U6 therefore stands. `150` §9.1 is updated in
  place to say so.
- **Mutation proofs.** (1) Ignore a declared anchor in `rebase_coordinates` →
  `a_declared_anchor_answers_what_no_base_state_placement_could` fails with *"a declared anchor
  answers it with no placement: Unsupported { … reason: \"no block placement: neither operation
  says where the split paragraph sits\" }"*. (2) Treat `Affinity::After` as answerable →
  `a_declared_affinity_answers_the_insertion_a_concurrent_join_absorbed` fails with the
  transform returning `Ok(Keep(InsertText { … offset: 8 }))` where a refusal was required.
  (3) Drop the session's `resolve_anchor` call →
  `an_anchored_arrival_is_resolved_before_it_is_applied` fails with the paste landing at body
  index 2 instead of 3 (`left: 3, right: 4`). (4) Make a destroyed anchor resolve to the
  nearest index instead of reporting → `a_destroyed_anchor_is_a_loss_and_never_a_fallback_to_
  the_stale_index` fails with `left: Ok(()), right: Err(AnchorDestroyed { … })`.
- **Not decided here:** whether to *also* take the op-set change later. If one is ever taken,
  the envelope declaration becomes redundant rather than wrong, which is the reversibility this
  decision was chosen for.


## ADR-057 — The operation set derives `serde`, and the wire is a versioned frame around a self-describing payload

- **Status:** Accepted, implemented.
- **Date:** 2026-10-02.
- **Context:** `107` 6.4. `152` §9 would not freeze a byte codec while `150` §9.1 and §9.2 were
  still about to move the operation shapes — *"the one thing a compatibility surface must not
  do"*. ADR-056 answered both on the envelope, no `Operation` variant changed, and the shapes
  are final. 6.1 (durability) and 6.6 (the relay binary) were both waiting on this and nothing
  else.
- **Decision 1: the op set derives `serde` rather than being hand-encoded.** `casual-doc-edit`
  had no `serde` at all; it has it now. The model's values already derive it, so the operations'
  payloads and the document snapshot (`25`) share **one** value schema.
  - **Why not 58 hand-written encoders.** A hand-written encoder and the `apply` it must agree
    with are *two definitions of one schema*, and two definitions of one rule diverge. `150`
    §3.2 makes the same argument pointing the other way: an exhaustive match is right where
    each arm is a **decision** someone must take, and wrong where every arm is the same
    mechanical transcription. Nobody ever has to decide how a `u32` is encoded.
  - **What it costs, stated plainly:** `serde`'s derive is externally tagged and field-named, so
    **field names are the compatibility surface**. Adding a field is additive; renaming one is a
    break that no type error catches and no round-trip test can see, because both sides of a
    round trip move together. `GOLDEN_CHUNK` is the only guard that can, and it is why the
    golden vector is not optional decoration.
  - `Label` is `&'static str` and therefore **not** on the wire; `147` §7 Q2 already recorded
    that a persisted log needs a stable code instead. Nothing here needs one, because a relay's
    ordered entry carries no label (ADR-047 — it holds no document, so it holds only the order).
- **Decision 2: a 12-byte frame — magic, frame version, payload encoding, payload length.**
  - **The established pattern, named:** a versioned length-delimited frame around a
    self-describing payload. That is protobuf's and CBOR's shared property, and the only one
    that matters: a reader that meets something it does not understand can measure it and move
    on.
  - **`bincode`/`postcard` were rejected on that exact point.** They are smaller and are *not*
    self-describing: the schema is the reader's own struct definition, so a field a newer sender
    added is a silent misparse rather than a skipped one. For bytes two different versions of
    this software will read, that trade is the wrong way round. Neither is a new dependency
    either way — this adds **no new package**; `Cargo.lock` gained one dependency *edge*.
  - **The payload encoding is a field, not an assumption.** The register's pending list reserves
    *"canonical CBOR encoding profile and golden vectors"* for the **document** snapshot. Nothing
    here pre-empts it: swapping the payload encoding is a `PayloadEncoding` value, and a decoder
    meeting one it does not hold refuses by name. JSON is chosen for this increment because it is
    what the model's values are already round-tripped in. Its known cost — `152` §10 Q6's `Vec<u8>`
    as decimal numbers, about 4× — does **not** land on operations, because media travels as a
    *reference* and the bytes stay in the host's resource map. It would land on a snapshot, and
    that is exactly why the snapshot half of 6.1 must measure before reusing this encoding.
  - **Every bound is a refusal, and ordered so the bound comes first.** A codec is the only place
    in the crate that reads bytes it did not write, so `MAX_FRAME_BYTES` is checked from the
    **header**, before the declared length is used for anything at all — a field saying four
    gigabytes must not size an allocation on its way to being rejected. Trailing bytes are
    refused rather than ignored: they mean writer and reader disagree about the record, and
    guessing which is right is how a log loses its tail.
  - **The codec is a collaboration module, not an exemption from the rule that guards them.**
    `the_keystroke_path_runs_no_transform` was extended rather than relaxed: `codec.rs` joins
    `protocol.rs`/`wire.rs`/`session.rs`, *and* reaching `codec::` from anywhere else is now
    itself an offence, *and* `casual_doc_transaction::codec` joins what the live editor may not
    name. A single-user edit encodes nothing; a lone editor saves through `casual-doc-io`.
- **Mutation proofs.** (1) Check `MAX_FRAME_BYTES` *after* comparing the declared length with the
  bytes present → `every_refusal_names_what_was_expected_and_none_of_them_allocates_the_declared_
  length` fails with `Err(Truncated { declared: 4294967295, available: 0 })` where
  `Err(TooLarge { .. })` was required. (2) Add `#[serde(deny_unknown_fields)]` to `Submission` →
  `a_field_a_newer_sender_added_is_skipped_rather_than_refused` fails with *"unknown field
  `aFieldFromTheFuture`, expected one of `client`, `seq`, `base`, `operations`"*. (3) Rename
  `WireOperation::operation` to `op` on the wire → `the_golden_chunk_is_byte_for_byte_what_this_
  build_writes` fails, which is the rename no other guard can see. (4) Plant `codec::` in a
  keystroke-path module → `the_keystroke_path_runs_no_transform` reports *"v0.rs reaches
  `codec::`"*.
- **Not decided here:** the document snapshot's encoding (still the pending CBOR ADR); whether
  the client's own durable log (`112`) reuses this frame — if it does, that change has to argue
  with the collaboration-module list, which is the point of the list.


## ADR-058 — The relay is a workspace member under `server/`, and its durability is a checkpoint plus a write-ahead tail

- **Status:** Accepted, implemented. Transport is std-only by deliberate choice, below.
- **Date:** 2026-10-02.
- **Context:** `107` 6.1 (durability) and 6.6 (the relay binary). Both were blocked on the byte
  codec, which was blocked on the operation shapes; ADR-056 and ADR-057 cleared both.
- **Decision 1: `server/`, not `crates/`.** **No mandatory server** is a structural property of
  this project rather than a preference (`10`, `AGENTS.md`), and a relay that `crates/` *could*
  depend on is a relay that becomes required by accident, one `use` at a time. So the direction
  of the dependency is a build-level fact: `server/` sees the engine and the engine cannot see
  `server/`. `nothing_under_crates_depends_on_the_relay` reads the **manifests**, because a
  `path` dependency is how it would actually happen, and it asserts it read at least ten of them
  so it cannot pass by looking at nothing.
- **Decision 2: durability is a checkpoint plus a write-ahead tail.** Named before built: this is
  what every database does, and the alternative — rewrite the whole state per change — is what
  makes a relay O(history) per edit. Compaction renames a freshly written file over the old one,
  because a compaction that truncates in place is the operation that loses the data.
- **What is durable is remarkably little, and that follows from ADR-047.** A dumb relay holds no
  document, so the only state it can lose is **the order it imposed** plus the dedupe table that
  stops a retried chunk landing twice. A relay that persisted a document would be a relay that
  could disagree with its clients about one.
- **Decision 3: recovery verifies rather than trusts.** A record carries the submission *and the
  revision the relay ordered it at*, and recovery hands the submission back to
  `ServerSession::commit` and **checks the answer matches**. Replaying the *outcome* instead —
  writing the ordered entry straight into the history — would make every recovery "succeed",
  including one where the ordering rule had changed under the file; and a relay whose order is
  not the order its clients were acknowledged against has diverged everybody silently. Same
  argument as ADR-051's: verifiable, not trusted.
- **A torn tail is expected; a torn middle is not.** A process can die between appends, so the
  last frame may be partial: that is discarded **and counted**, because the count is the only
  evidence the previous run did not shut down cleanly. A frame that fails anywhere else is
  refused, because the records after it belong to an order the file can no longer describe.
- **Decision 4: journal first, answer second.** A client drops a chunk from its outstanding set
  the moment it is acknowledged, so an acknowledgement that outlives its record loses the work
  outright, while an un-acknowledged chunk is simply retried. The asymmetry is why `Room::commit`
  returns the journal's **error** rather than the relay's answer when the write fails.
- **Decision 5: the transport is `std` only, thread-per-connection — and that is a limit, not a
  claim.** An async runtime is a dependency decision (`tokio` is a tree, and
  `dependency-policy` runs `cargo deny`, a gate this lane cannot run), so it is left to the
  owner. Thread-per-connection costs a thread per participant and suits tens per room, not
  thousands. It is also the right shape for a *dumb* relay, whose per-message work is "append,
  number, write to N sockets" and never a document: there is no computation to overlap, only i/o.
  Moving to async is an optimisation of a working mechanism.
  **This adds no new package at all** — `std::net`, `std::thread`, and the codec's own frame.
- **`Frames` exists because a stream has no record boundaries.** Reading one message is: read
  until the frame its own header measures is complete, decode, keep the remainder. Assuming one
  `read` is one message, or scanning for a delimiter a payload may contain, is the bug every
  hand-rolled framing has, so it is written once.
- **Mutation proofs.** (1) Answer before journalling → `a_chunk_is_durable_before_the_room_says_
  it_is_ordered` fails `left: 0, right: 1`. (2) Recovery accepts any ordered replay → `recovery_
  checks_the_decision_and_refuses_a_journal_that_disagrees` fails, returning `Ok` with a
  recovered relay. (3) Skip a corrupt frame instead of refusing → `a_corrupt_frame_in_the_middle_
  is_refused_rather_than_skipped` fails with `Err(DecisionDiffers { logged: Revision(2),
  replayed: None })` — the damage surfacing later and in the wrong vocabulary, which is the point.
  (4) Add `opendoc-relay` to `casual-doc-edit`'s dev-dependencies →
  `nothing_under_crates_depends_on_the_relay` names the manifest and both strings.
- **A defect found by arithmetic, and the bound it needed.** A checkpoint embeds the relay's whole
  retained history — `DEFAULT_RETAINED_REVISIONS` (400) entries of up to `CHUNK_BUDGET_BYTES`
  (3 MB), about **1.2 GB** — while the codec's `MAX_FRAME_BYTES` is **30 MB**, because that one is a
  backstop against a hostile *socket*. Reading the journal with the socket's bound meant a busy
  relay writing a checkpoint it could never read back: `compact` succeeds and the next `open`
  refuses its own file, taking the order with it. No test would ever have seen it, because no test
  writes a 30 MB checkpoint — it was found by reading two constants against each other. The journal
  therefore reads with its own derived bound, and
  `the_journal_s_bound_covers_the_largest_checkpoint_the_relay_can_hold` guards the **arithmetic**,
  which is the only thing that can be guarded here. Raising the codec's constant instead would have
  been the wrong fix — a socket is not a file — and the guard asserts that too.
  **The underlying shape is still wrong and is recorded rather than hidden:** a frame is only that
  large because a checkpoint *embeds* the history. Writing the history as one bounded frame per
  entry, which is what every other record already is, removes the special bound entirely; it needs
  `ServerSession` to be reconstructible from its state plus a replay of its entries, an API it does
  not have, so it is the next increment.

  **Amended 2026-10-02: the next increment happened, and the special bound is deleted rather than
  documented.** `Record::Checkpoint` now carries a `SessionState` — the session *without* its
  retained history — followed by one `Record::Retained` frame per entry.
  `ServerSession::{checkpoint, restored}` is the API the paragraph above said was missing, and the
  pair is deliberately not one serializable value: handing a caller something it could encode in
  one frame would restore the defect. `MAX_JOURNAL_FRAME_BYTES` is **gone**, the journal reads with
  the codec's own `MAX_FRAME_BYTES`, and `the_journal_s_bound_covers_the_largest_checkpoint_the_
  relay_can_hold` is replaced by `every_journal_record_fits_the_codec_s_own_frame_bound`, which
  measures the **bytes of every frame in a real file** rather than arithmetic between two
  constants — a room with a small `retain` is driven past it so the checkpoint really carries a
  full window.

  Two things said plainly about the replacement. Its width assertion still **cannot** be driven
  red without allocating 1.2 GB, which is why the frame-*count* assertion (`frames >= 2`) is what
  carries it; that one does go red. And what is left unbounded is now the **participant tables**:
  `accepted`, `resumes` and `granted` hold one small entry per participant ever admitted and
  nothing prunes a departed one. That is a pre-existing slow leak, and the honest statement is that
  it is now the *only* term in a checkpoint's size — at roughly 100 bytes an entry the 30 MB bound
  is reached at some 300,000 lifetime participants, and reaching it is a loud refusal rather than
  silence. Pruning is `152` §10.
- **Decision 7, added 2026-10-02: a join is journalled, because not journalling one was a live
  defect and not a tidiness question.** `Record::Admitted(Admission)` is written **before the join
  is answered**, exactly as an ordered chunk is written before its acknowledgement, and
  `Room::join` returns `Result` for the same reason `Room::commit` does. `ServerSession::join` now
  hands back an `Admission` alongside its answer rather than offering a setter, so there is no way
  to admit a participant without being handed the record of it — the same compile-error discipline
  that makes ADR-060's `granted` a required argument.

  **What was wrong, measured rather than deduced.** `probe_what_a_crash_actually_does_to_an_
  admission` drove two participants joining after a checkpoint, one ordered-and-journalled chunk,
  and a restart. It printed:

  ```
  BEFORE  ada=ClientId(0) grace=ClientId(1) head=Revision(1)
  AFTER   head=Revision(1) replayed=1 has_assigned(ada)=false has_assigned(grace)=false
  RESUME  ada presenting a known key after the crash -> Welcome { client: ClientId(0), ... }
  SUBMIT  the holder of number 0, seq 1 -> Duplicate { revision: Revision(1) }
  ```

  Three distinct harms, and the handover's description of the second was **wrong in the direction
  that matters**. It expected `TooFarBehind`; the measurement says `Welcome`, which is worse —
  `TooFarBehind` is *announced* loss with an `ODC-7006` behind it, while a `Welcome` plus a
  snapshot discards the unacknowledged work a resume exists to preserve (`152` §5.5) and says
  nothing at all. Silent loss is the one failure class this project's priority order puts first.

  1. `next_client` regressed to the last checkpoint while the dedupe table was rebuilt by replay,
     so a participant number was **handed out twice** — and the next holder's first chunk came back
     `Duplicate` and vanished. That is the harm ADR-060's forged-submission fix closed, reachable
     through a crash instead of a forgery, and because a participant number *is* an `IdSpace`
     (ADR-051) two live replicas would also have been minting colliding `NodeId`s.
  2. The resume table regressed, so a client whose work was acknowledged could not resume.
  3. `has_assigned` regressed, so the relay's own boundary check refused chunks from a participant
     the order had acknowledged.

  **And it let `commit` take the admission check back**, which ADR-060 wanted and could not have:
  `granted` is now state the journal records, so recovery replays admissions ahead of the chunks
  that depend on them, and the check is *exact* membership inside the state machine instead of a
  range check at the boundary. `has_assigned` is deleted. The relay keeps **no second grant table**
  — one durable table, overwritten by every accepted join, which is what makes a revocation take
  effect on the reconnect; `a_narrowed_grant_on_rejoin_beats_the_one_the_journal_remembers` is the
  guard, and it is the one that **passed on its first writing** and had to be rewritten to create
  its condition (ada needs a *resume* key, or she is handed a new number and the two grants never
  meet at one map key).

  `Relay::disconnected` no longer forgets a grant, and that is a consequence rather than an
  oversight: the hazard it guarded — the next holder of this number inheriting an unverified
  capability — has no next holder now that numbers are never re-issued.
  `a_grant_dies_with_the_connection` pinned that *mechanism* and is replaced by
  `a_departed_participants_number_is_never_handed_to_anybody_else`, which asserts the guarantee in
  both halves.

  **Nine mutation proofs, all of which compile and all of which redden.** The floor on
  `next_client` removed → "the restart handed out `ClientId(0)` again". The resume entry not
  restored → the silent-`Welcome` message above, verbatim. `commit`'s membership check removed →
  `left: Ordered { revision: Revision(1) }, right: Refused { reason: NotAuthorised }`. The
  admission not journalled → `left: 0, right: 1` readmitted. A checkpoint that drops its retained
  window → the window comes back `[]`. An orphan `Retained` accepted → the rebuilt history holds
  `Revision(1)` **twice**. An append that also writes state → growth `[328, 330, 332, 334, 336,
  338]`, visibly O(history). `insert` weakened to `or_insert` → the journal's wider grant survives
  a narrowed rejoin. The relay's connection check removed → a forged submission is `Ack`ed.

  **A trap in the mutation harness itself, recorded because it would have produced a false
  green.** Restoring the file with `shutil.move` gives it an mtime *older* than the mutated version
  cargo last compiled, so cargo compares mtimes, decides nothing changed, and reuses the **mutated
  artifact** for the following run. One "unmutated" run was therefore a lie, and was chased as a
  production bug before the cause was found. The harness now bumps the mtime on restore, and every
  mutation above was re-run under it.
- **Stated rather than implied:** the durability guard proves the **ordering** of the write
  against the answer, not the `fsync`. A second `open` in the same process reads the page cache,
  so removing `sync_data` leaves it green. Durability against power loss is not observable from a
  unit test in one process, so that line is reviewed rather than tested — recorded here because a
  guard claimed to prove more than it does is how this repository has been bitten before.
- **Fan-out is built, because a relay that does not fan out is not a relay.** `Participants` is
  generic over the writer so its two rules are testable against a `Vec<u8>` rather than a port — a
  rule that needs a socket to exercise is a rule that gets exercised by hand, once. The rules:
  **the author is excluded** (a client that received its own chunk back would apply its own edit
  twice; what it is waiting for is the acknowledgement, a different message on a different path),
  and **a failed write is returned rather than swallowed** (that participant is now *behind the
  order*, and `152` §5.5's resume is how it catches up — which only happens if somebody noticed).
  A **duplicate** is acknowledged and deliberately **not** fanned out again. The room and the
  participant set sit behind **one** lock, so decide → journal → answer → fan out cannot
  interleave and two participants cannot be told about the order in two different orders. A
  connection that ends for any reason leaves the set, or every future chunk is written to a dead
  socket and reported as failed forever.
  It is **not** a queue: a slow participant blocks the fan-out for its own write, because
  per-participant buffering needs back-pressure and a policy for a reader that never drains, and
  both are designs rather than details.
- **Presence fan-out is built too, and is the one place the identity direction is visible.** A
  client sends `ClientMessage::Presence`, which has **no identity field**; the relay fans out
  `ServerMessage::Awareness { client, update }`, where the identity is **the one the relay
  attached**. That is what makes a forged identity *unexpressible* rather than merely rejected, and
  it is ONLYOFFICE's own shape. Three rules follow and each has a guard: a **stale** update (an
  older or equal clock) is dropped rather than fanned, because sending it would move a caret
  *backwards* on every other screen; presence from a connection that has **not joined** is dropped,
  because there is nobody to attribute it to and inventing one is what the no-identity-field design
  exists to prevent; and a **departure is announced**, because a caret that outlives its owner is
  worse than no caret — the reader believes somebody is there. Presence failures are deliberately
  **not** reported as "behind", unlike an ordered chunk: presence is not ordered, so a participant
  that missed one is not behind anything and the next update corrects it.
- **The message handling moved out of the binary, and that was a correction rather than tidying.**
  It was written in `main.rs` first, where every decision above sat behind a `TcpStream` and **no
  test could reach any of them**. `relay::Relay<W>` is generic over the writer, so each one is
  exercised against a `Vec<u8>`; the binary is now sockets and threads and nothing else. The part
  that is hard to get right is the part a test can reach.
- **Not built here:** a *typed* cursor payload (presence carries an opaque one and will until
  `107` P-4 lands), and the host-signed grant (`152` §10 Q4) that an access level would be read
  from.
- **Five more mutation proofs, for fan-out and the relay's decisions.** (5) Drop the
  author-exclusion → `the_author_receives_nothing_and_the_others_receive_the_bytes` fails
  `left: [(0,3),(1,3),(2,3)], right: [(0,3),(1,0),(2,3)]`. (6) Swallow a failed write →
  `a_participant_whose_write_failed_is_reported_rather_than_swallowed` fails `left: [], right:
  [ClientId(1)]`. (7) Fan a **duplicate** out again →
  `a_duplicate_chunk_is_acknowledged_and_not_fanned_out_a_second_time` fails `left: 2, right: 1`.
  (8) Fan a **stale** presence update → `a_presence_update_that_says_nothing_new_is_not_fanned_out`
  fails `left: 2, right: 1`. (9) Announce a departure unconditionally →
  `a_connection_that_never_joined_can_disconnect_without_announcing_anything` fails with *"nobody
  may be told that a participant who never arrived has left"*.


## ADR-059 — `SetDocumentProtection`: the one operation that would make protection reachable

- **Status:** **Accepted and implemented**, 2026-10-02 — the operation, the ordering rule, and
  the facade surface. The reachability half (Review ▸ Restrict Editing plus a command-palette
  entry, `105` UX-004) is `webapp/`'s and is **still open**; see "the reachability half" below.
  It was proposed rather than implemented because it adds a variant to the closed operation set,
  which ADR-030 I2 reserves to the owner, and because the two wasm match sites measured below sat
  in a file another lane held.
- **Date:** 2026-10-02 (proposed and implemented the same day).
- **Context:** ADR-052 made `w:documentProtection` **enforced** at the operation for `readOnly`,
  `comments` and `trackedChanges`. It is now the sharpest instance of this repository's most
  expensive recurring pattern, pointing the other way: the capability is *enforced* and
  *unreachable*. A reader cannot restrict editing, and — worse — **cannot unrestrict it**, so a
  document that arrives protected is permanently read-only in this editor while Word and
  ONLYOFFICE both offer Review ▸ Restrict Editing. Import and export already round-trip the
  policy faithfully, so nothing is lost on save; it simply cannot be *changed*.
- **Decision (proposed).** One variant, in exactly the shape of the three definition operations
  that landed with ADR-005:

  ```rust
  /// Install or remove the document's editing restriction (`w:documentProtection`).
  ///
  /// Document-global, not node-scoped, and its own inverse carrying the previous value
  /// (`None` when there was none) — the same `Some`/`None` shape as
  /// `SetStyleDefinition`, for the same reason: it inverts in both directions.
  SetDocumentProtection {
      protection: Option<DocumentProtection>,
  },
  ```

  `DocumentProtection` is `Copy` and three fields wide, so it needs no `Box`. The whole of
  `apply` is a swap of `definitions_mut().settings.document_protection`, returning the previous
  value as the inverse.
- **Why one variant and not a field on something existing.** The register's own precedent
  (`InsertFieldRange`) states it: I2 is about keeping the set **closed and additive**, and a new
  variant is additive while widening an existing one changes that operation's shape for every
  caller. The three definition operations ADR-005 added are the template.
- **The ordering trap, which is the only subtle part.** `refuse_if_protected` judges a batch on
  its worst operation, so an operation that *lifts* a restriction would be refused by the
  restriction it is lifting. The rule has to be that `SetDocumentProtection` is judged against
  the protection in force **before** the batch and is itself exempt from `ReadOnly` — otherwise
  `readOnly` is a one-way door and the feature is worse than absent. That is a decision about
  authority, not about mechanics, and the honest form of it needs the session's access level
  (`152` §10 Q4, still open): *the document* may say "do not edit me", and only a *participant
  grant* can say who may overrule it. Until the grant exists, the local reader is the only
  authority there is, which is exactly what Word does with an unpassworded restriction.
- **Measured blast radius — 13 match sites, and 2 of them are not this lane's.** Adding the
  variant was probed with a throwaway `ProbeVariant` and the compiler enumerated every site:
  - **11 in this lane's own files**: `casual-doc-edit`'s `apply` dispatch and `protection.rs`'s
    two exhaustive matches, and 9 in `casual-doc-transaction` (`transform/classify.rs` ×3,
    `transform.rs` ×3, `transform/effect.rs`, `wire.rs` ×2).
  - **2 in `crates/casual-doc-wasm/src/lib.rs`**, both exhaustive with no wildcard: the
    `HistoryKind` classification (~line 799) and `caret_after` (~line 25779). A live lane holds
    that 26,000-line file, and the working contract's §5a records precisely this shape as how two
    green branches make `main` red.

  So the change is **not blocked by design or by effort** — it is blocked by file ownership, and
  it is small: the two wasm arms are `HistoryKind::DocumentProperties` (a protection change is a
  document-wide policy edit, and Word labels it that way) and `Pos::new(doc_id, 0)` beside the
  other document-global operations, which route through `apply_action_caret` with the caller's
  own caret anyway.
- **The reachability half is not optional.** `105` UX-004 requires every capability on **two**
  surfaces, so landing the operation without Review ▸ Restrict Editing *and* a command-palette
  entry would leave the same defect one layer up. That half is `webapp/`'s and is named here so
  the operation is not mistaken for the feature.
- **How it was implemented, and the two things that were decided rather than assumed.**

  1. **The ordering trap is one predicate, not an inline rule.**
     `casual_doc_edit::protection::exempt_from_protection` is public and is called from **both**
     choke points — the engine's `refuse_if_protected` and, which this ADR did not anticipate,
     the facade's `w:edit="forms"` loop. **The forms level had the identical trap and it was
     already shipped**: a document-global operation is never "inside a form field", so
     `op_is_inside_a_form_field` refused it, and the owner's forms-protected loan agreement
     could never have been unprotected. One rule in one place rather than two copies, because
     two implementations of one exemption diverge. The exemption is a `matches!` and not an
     exhaustive match on purpose: the positive list *is* the exemption list, so a 60th operation
     defaults to **governed**, which is the safe answer, where the module's other two matches
     have the opposite default and therefore may not carry a `_` arm.
  2. **It is still judged against the protection in force *before* the batch**, which falls out
     of `refuse_if_protected` reading the level once above the loop. So a batch may lift a
     restriction and a batch may edit, but a batch may not lift a restriction and then edit
     under the lift: `[SetDocumentProtection(None), InsertText]` is refused on the `InsertText`,
     in either order. That keeps "a batch is judged whole" (ADR-052) true, and it closes the
     obvious **wrong** fix — waving a whole batch through because one operation in it is exempt,
     which would make a lift a passkey for everything travelling beside it.
  3. **Authority: the local reader, and nothing else is checked.** `152` §10 Q4's host-signed
     participant grant does not exist, so there is no session access level to consult. Anyone
     who can open the document can lift its restriction — which is exactly what Word does with
     an **unpassworded** restriction, and it is said in the operation's doc comment, in
     `exempt_from_protection`, and on the facade method rather than left to be inferred.
     **No password material is modelled, requested, or verified**, and that is a decision and
     not an omission: ADR-052 records that this is policy rather than security, and verifying
     `w:hash`/`w:salt` would advertise a boundary that does not exist, since the legacy hash is
     removable by editing one attribute in the XML. When the grant lands, the honest rule is
     *the document says "do not edit me" and only a grant says who may overrule it*, and this
     predicate is the single place that changes.
  4. **The 13 match sites were exactly as measured** — 2 in `casual-doc-edit`, 9 in
     `casual-doc-transaction`, 2 in `crates/casual-doc-wasm/src/lib.rs` — and the two wasm arms
     are the ones named here. The transaction crate's answers: `Tier::DocumentScope`,
     `Coordinates::None`, `Effect::Inert`, no node anchor and no registry key (so no tombstone
     is possible), the fixed-size payload group, no introduced identity, and
     `(Target::Settings, Aspects::DOCUMENT_PROTECTION)` — the same settings record as
     `SetEvenAndOddHeaders` and a **different** aspect, so a concurrent even/odd-header change
     and a concurrent restriction do not destroy each other while two concurrent restriction
     changes do contend and the later wins.
  5. **No model change was needed.** `DocumentProtection` is `Copy` and three fields wide, as
     predicted, so the variant carries it unboxed.
- **The facade surface the host calls:** `setDocumentProtection(edit, enforcement, formatting)`
  where `edit` is `none` | `readOnly` | `comments` | `trackedChanges` | `forms`, or `null` for
  Word's "Stop Protection"; `documentProtection()` reads it back as JSON. An unknown level is
  refused with a sentence rather than defaulted, because defaulting would silently apply a
  restriction the host did not ask for. `enforcement: false` stays distinguishable from "no
  restriction at all", because Word writes that state and it must survive a save.
- **Mutation proofs.** (1) The `exempt_from_protection` early-`continue` is dropped from
  `refuse_if_protected` and the ordering-trap guard reddens with `left: Err(ReadOnly)` on the
  operation that lifts the restriction — if that guard cannot fail, the trap is still there.
  (2) The whole batch is waved through when any operation in it is exempt, and the
  lift-then-edit guard reddens — the exemption is not a hole. (3) The exemption is dropped from
  the facade's forms loop and the forms-protected fixture refuses its own unprotection.
  (4) The same engine mutation, seen end to end through `setDocumentProtection`. (5) The inverse
  drops the previous value and undoing "Stop Protection" no longer restores the restriction.

## ADR-055 — A second document class is additive: new crates, a second surface, and seams published in place

**Status:** Accepted for the shared-core work only, 2026-10-01. Specified in
`156-PRESENTATION-SUPPORT-AND-SHARED-DRAWING-CORE-DESIGN.md`. **Does not decide whether
presentations are built**, which `106` §1 still answers "a future sibling"; it decides the shape
any such work must take, so that the question can be answered late and cheaply. Charts and
SmartArt are excluded and belong to ADR-050 / `155`, which landed while this branch was in
flight.

**Decision.** Five parts. The first is the one that makes the rest reversible.

1. **`v1::Document` is never modified to accommodate another document class.** It keeps its exact
   shape and its byte-identical serialization — the `skip_serializing_if` attributes on
   `properties` and `background` exist for precisely that reason. A presentation is a **sibling
   type in a new crate**, and the class discriminator lives at the **io boundary**
   (`ImportArtifact`/`ExportRequest`, `casual-doc-io/src/artifact.rs`), which is already the one
   place in the dispatch layer that names a concrete model type.
2. **Shared capability is reached by publishing seams in place, not by extracting shared crates.**
   The seams already exist: `flow_anchored_text_box` is a page-free "lay out a text body in a
   rect" function (`casual-doc-layout/src/flow.rs`), `resolve_anchor_rect` is a pure anchor→rect
   resolver, `GroupMapper` is a correct DrawingML child-space affine, and
   `casual-doc-render::render` takes a display list and knows nothing of pages. A big-bang
   extraction is the highest-risk, lowest-payoff move available; it waits for a **third**
   consumer, which is also when `opencalc`'s independently reimplemented OPC layer becomes worth
   paying down.
3. **A second editor surface, not a modified one.** `webapp/src/main.js` is 16,190 lines with zero
   exports and 333 import-time DOM bindings (HF-109 open), so a slide shell is its own page with
   its own facade, embedded through the same `<opendoc-editor>` iframe with a different `src`.
   The DOCX editor therefore carries **no** risk from presentation work. The cost is shell
   duplication, and that cost is the argument for landing HF-085/HF-109 first rather than a
   reason to defer them.
4. **The shared-core corrections land as DOCX work, on their own merits.** Every row of `155` §6
   is a defect in the shipped DOCX product — 22 of ~187 presets with the rest painting as
   bounding rectangles, a rotated group that paints unrotated, `a:effectLst` unimplemented at
   every layer, an autofit scale applied but never re-solved, a theme style matrix retained as an
   opaque string. These are sequenced against the existing FID-R and OO-014 rows and require **no
   decision about presentations**. If presentations are never built, none of it is wasted.
5. **No AGPL code, structure, or transcription enters this repository.** ONLYOFFICE is read for
   behaviour and scale only. The licence asymmetry is the wedge (`106` §2); importing their code
   would end it.

**Why additive rather than a unified model.** A `DocumentClass` discriminator inside `v1::Document`
would put a presentation's shape tree in the same type the DOCX editor, exporter, 58 edit
operations and five exhaustive `transform` matches all pattern-match on — so every presentation
increment would be a breaking change to the shipped product. It would also be the **one-way**
choice: a sibling type can move to another repository if `106` §1's "future sibling" answer stands,
whereas a modified `Document` cannot be un-modified.

**What makes this enforceable rather than aspirational.** Three guards already exist and were not
added for this:

- `casual-doc-model/tests/model_consumer_ledger.rs` caps typed-but-unreachable model rows with
  `UNCONSUMED_CEILING = 17` as a ratchet, so a presentation model type **cannot land ahead of its
  consumer**. That is the guard against `105`'s "modeled is not shipped", and it forbids exactly
  how this kind of effort normally rots.
- The oracle geometry gate (committed references under `fixtures/oracle/`, armed by
  `the_oracle_gate_is_armed`) plus `geometry_snapshot.golden` catch an unintended geometry change
  in part 4's work. A golden that moves must move intentionally, with the diff explained.
- Adding a variant to `ShapeGeometry`, `Fill` or `ShapePathCommand` breaks every exhaustive match
  and struct literal in the workspace — `E0063`, the §5a shape-1 failure. The absent `_` arms are
  deliberate (`preset_token`'s match has no wildcard so a new preset *must* be handled) and are
  kept. The mitigation is `cargo check --workspace --all-targets --all-features`, not crate tests.

**What choosing this costs, stated plainly.** Shell duplication until HF-109 lands, and a second
facade is a second large export surface with the same god-file risk `casual-doc-wasm` already
carries. Transitionally, two EMU→twip rounding rules coexist in `casual-doc-layout::units` —
collected into one module and pinned by a test that fails when they are converged, because
converging them is a geometry change that moves goldens. This partially answers the standing
pending question "whether layout uses fixed-point units internally": layout stays in twips, and
EMU is a boundary unit with its rounding named at the boundary.

## ADR-060 — A participant's access level is a host-signed grant: the host signs, the boundary verifies, the engine enforces

- **Status:** **Accepted and implemented**, 2026-10-02 — the capability vocabulary, the engine's
  enforcement point, the wire surface, and the relay's boundary check. **No signature profile is
  chosen**, deliberately; see "what is deliberately not decided". The chrome half (routing four
  refusal codes, and disabling Review ▸ Restrict Editing with a reason when `manageProtection` is
  absent) is `webapp/`'s and is **open**.
- **Date:** 2026-10-02.
- **Closes:** `152` §10 Q4 (the host-signed grant) and §10 Q5 (access enforcement at the
  operation). Implements `107` 6.7's open half and `143` §10's second enforcement layer.
- **Relates to:** ADR-052 (the *document's* protection, enforced at the operation), ADR-059 (the
  operation that changes it, and `exempt_from_protection` as the one place that would have to
  learn the difference), ADR-047 (the relay holds no document, which is what bounds what it can
  judge), ADR-057 (the wire), ADR-058 (the relay and its durability).

### Context: two authorities were collapsed into one, and the narrower one was missing

ADR-052 made `w:documentProtection` **enforced at the operation**, by projection equality. That
answers *what the document asks of everyone*. It does not answer *what this participant may do*,
and nothing did: a room's read-only guest could lift the document's own restriction and then edit
freely, because `exempt_from_protection` knew only one kind of caller. `152` §10 Q5 recorded the
gap in exactly those words and ADR-059 named the function that would have to close it.

Two facts, not one: **a document that asks not to be edited** and **a participant who is not
allowed to edit it**. Collapsing them has a concrete cost in both directions — a standalone
reader must still be able to lift an unpassworded restriction (that is what Word does, and
ADR-052's "policy, not security" depends on it), while a room guest must not.

### The established pattern, named before any code

A **least-privilege capability grant**. The host signs a short-lived token binding a subject to a
document and a capability set; the boundary verifies it; the engine enforces what came back. Four
independent precedents for the same shape: OAuth2/JWT's audience-subject-scope-expiry, WOPI's
access token, ONLYOFFICE's own JWT, and Fluid's permission-bound container. `143` §10 had already
written it down as five enforcement layers. Nothing here is invented; what is new is that the
layer this repository owns exists.

### Decision

**1. One vocabulary, in `casual_doc_edit::access`.** `Capabilities` — `comment`, `suggest`,
`edit`, `manage_protection` — with private fields, named presets (`viewer`, `commenter`,
`suggester`, `editor`, `owner`) and `narrowed_to` as the only composition, because a narrowing
operation that can widen is not one. It lives beside `Operation` because that is the only place an
exhaustive judgement over the operation set can be a **compile error**.

Named `bool` fields and not a bitmask: the wire is JSON (ADR-057), so each field carries
`#[serde(default)]` and both drift directions fail towards *less* access — an unknown field is
ignored, a missing one reads `false`. A bitmask makes an unknown bit invisible and a width change
silent.

**2. Reading is not a capability.** Admission *is* the view right. A `view` flag would be a flag
nothing could ever be false for, so `143` §10's `view` class is the floor rather than a bit.

**3. `review`, `history.read`, `history.restore` and `share.admin` are deliberately absent**, and
this is the part most likely to look like an omission. None has an enforcement point in this
crate, and a capability nothing enforces is the "modeled is not shipped" failure the working
contract names. `review` cannot get an *exact* one: `is_tracked_only` is a **negative** test — it
says an operation only *added* tracked marks — so accepting, rejecting and untracked typing all
fail it alike, and ADR-052's rule is that only projections that can be made exact are built. A
reviewer is therefore granted `editor()` today, which is **wider than the role**, and that is
recorded rather than hidden.

**4. One enforcement rule for two boundaries that hold different information.**
`refuse_if_not_permitted(document: Option<&Document>, operations, capabilities)`:

- **With a document** — every honest replica, at the facade's choke point. The `comment` and
  `suggest` classes are decided by ADR-052's projection equality, exactly and with no heuristic.
- **Without one** — the relay. ADR-047 makes it hold no document, so it can judge only what an
  operation's *variant* admits. That answer is **strictly weaker and never refuses something a
  replica would allow**, which is a property and not a hope:
  `the_relay_s_document_free_answer_never_refuses_what_a_replica_allows` pins it.

So the line the relay holds against a **rewritten client** is write-versus-no-write, plus "that
was definitely not a comment". The finer classes are enforced by every replica and reflected by
the chrome; against an adversary who rewrote their own client they are policy, in exactly the
sense ADR-052 uses the word. **Saying so is the point**: a boundary claimed and not held is worse
than one never claimed.

**5. The grant is the outer gate; the document's policy is the inner one.** The access check runs
first, the protection check second. A participant with no write capability is refused on an
*unprotected* document too, and reversing the two would let an unprotected document admit a
read-only guest's edit.

**6. `exempt_from_protection` now distinguishes a local reader from a room guest** — which is
ADR-059's prediction, met. Exemption from *the document's* policy is not exemption from a
*participant's* access level: `SetDocumentProtection` stays exempt from the restriction it
changes (or `readOnly` would be a one-way door), and is subject to `manage_protection` like any
other operation.

**7. `Capabilities` never travels from a client.** `152` §2b made a forged identity
*unexpressible* for presence by giving the message no field to put one in. The same asymmetry
here: `Join` carries an opaque `GrantToken`, and `capabilities` appears on `Welcome` and
`Resumed` only — server to client. A capability a client could assert is a capability a client
can forge. What a client may do with its copy is **disable a control and say why**; the authority
is the relay's own copy.

**8. `PROTOCOL_VERSION` is not bumped**, and the reasoning is in the constant rather than here:
`Join.grant` and `capabilities` are both *added optional fields*, and no enum variant changed. A
grantless client meets a loud `NotAuthorised` / `ODC-7003` rather than the silent disagreement a
version bump exists to prevent.

**9. The verification seam is `server::access`, and it has no default.** `Access::Open(caps)` is a
real, reachable policy — a read-only broadcast room, a comment-only review link — that cannot tell
two participants apart, so a grant presented to such a room is **ignored** rather than honoured
(the conservative direction: an open room's ceiling cannot be raised by a token).
`Access::Granted(Box<dyn GrantVerifier>)` requires one. `Relay::new` takes an `Access` with **no
`impl Default`**, and the binary requires a role on its command line, because a permission policy
nobody configured must fail at the call site as a missing argument rather than at runtime as a
room where everybody is an owner. That is "never a dead control" applied to a security boundary.

`GrantVerifier::verify` returns `Result<Capabilities, GrantRefusal>` and not `Option`, so "I could
not tell" has somewhere to go other than a capability set. A verifier that falls back to *allow*
on a parse failure turns every malformed byte into an owner, and it is the single most likely way
this seam gets implemented wrongly — so the doc comment says so in those words.

### What is deliberately not decided

**No signature profile.** `143` §16 Q5 leaves JWT, PASETO and an opaque provider token open, and
picking one adds a cryptographic dependency to this workspace — a supply-chain and `cargo deny`
decision, not a coding one. What ships is the seam plus two explicit policies. This is **not** the
same as "nothing is enforced": an `Open(commenter())` room refuses every participant's
`InsertText` at the relay with `ODC-7004`, whatever their client offered.

**No durable capability as authority.** A grant is re-verified on **every** join, resumed or not,
so reconnecting cannot restore a right the room revoked (`143` §10). A capability replayed out of
a journal would do exactly that, which is why the relay's live grant map is connection-lifetime
state and why ADR-058's `Admission` record carries the capability as **evidence for an operator**
rather than as authority.

### Two security defects found while building this, both closed, both proven red

Neither was theoretical and neither needed a stolen credential.

1. **A connected participant could submit as somebody else.** `Relay::handle` passed `submission`
   to `commit` without comparing `submission.client` to the identity its own socket joined as.
   Worse than misattribution: it writes the **victim's** `(client, seq)` dedupe entry, so the
   victim's own next chunk at that seq returns `Duplicate` and vanishes — precisely the harm
   `ResumeKey`'s doc comment describes for a stolen resume key, reachable without one. With the
   check removed the forgery is answered `Ack { through: Seq(1), revision: Revision(2) }`.
2. **A connection that had never joined could submit at all.** `commit` keyed everything on the
   dedupe table and the base, so a `Base::Revision(head)` submission could name **any**
   `ClientId`. The range check is `ServerSession::has_assigned`; the exact check is the connection
   comparison above, and it is at the boundary because a connection is not a thing a pure state
   machine has.

**Why the check could not go in `commit`, measured rather than reasoned about.** ADR-058's
recovery hands a logged submission back to `commit` and checks the answer, so `commit` may depend
only on state the journal records. `ServerSession::join` was not journalled, so `next_client`,
`resumes` and any grant table were all advanced after the last checkpoint and gone on restart —
and adding a check against any of them made the relay refuse **its own file**:
`a_chunk_is_durable_before_the_room_says_it_is_ordered` failed with
`DecisionDiffers { logged: 1, replayed: None }` the moment one was added. `has_assigned` is
therefore derived from `next_client` — an inequality, a pure function of checkpointed state —
rather than from a table. ADR-058's `Admission` record is what later made the stronger check
durable.

### Mutation proofs

Nine, each of a different production line. The two above, plus: dropping the access check from the
facade's choke point (a viewer's `InsertText` is applied); making `admitted_by` answer
`Capabilities::viewer` for an unknown operation (a new operation arrives ungoverned); letting
`narrowed_to` union instead of intersect (a viewer narrowed by an owner becomes an owner); giving
`SetDocumentProtection` an unconditional exemption (a read-only guest lifts the restriction);
removing the "grant is re-verified on every join" overwrite (a revoked right survives a
reconnect); and dropping the relay's document-free weakening (the relay refuses `AddComment`,
which a replica allows — proving the weaker answer is not *weaker*).

One of them **passed on its first writing** and had to be strengthened, which is the shape
`SKILL` §4 warns about: the forged-submission guard used `Base::Chained`, which is refused as
`Malformed` for an unrelated reason (a client with nothing accepted cannot chain), so it passed
against a relay with no identity check at all. Rewritten to **create the condition** — an accepted
chunk first, so the chain resolves — it reddens.

## ADR-061 — A comparison is expressed as tracked changes, not as a second markup mechanism

- **Status:** **Accepted and shipped** (`applyDiffAsRevisions`, Review ▸ Compare). This line
  read "Proposed … not started" after the implementation had merged. Since **ADR-065** it is the
  SECOND step of Compare — "Keep as tracked changes" — after the comparison has been shown on
  the canvas as a read-only redline; the mechanism this ADR chose (tracked changes, never a
  second markup layer) is what the redline is built from.
- **Date:** 2026-10-04.
- **Design doc:** `158` — ONLYOFFICE source findings plus Word and Google Docs behaviour.
- **Closes:** `105` OO-007 ("pre-existing tracked changes accepted on compare… result saved as a
  new version") as a *design* question. Answers `153` `review.compare-documents`' "merged third
  document… is not started" with a decision about what to build.
- **Relates to:** ADR-052 (protection enforced at the operation — a comparison that writes
  revisions is a mutation and is subject to it), ADR-033 / `107` §459 (which already records
  "emit the result as tracked changes into the existing revision model" as the intended answer),
  `140` invariant 5 (diff is read-only derived data, and creates tracked changes *only* through
  an explicit Compare action — this ADR is that action).
- **Corrects:** `webapp/src/compare_documents.mjs`'s header, which records the change-list panel
  as the decided shape of this feature. It was the right call for what was reachable; it is not
  the right destination, and the file says so as though it were.

### Context

The product owner's report, 2026-10-04: *"show changes and compare is fucked up .. i cant even
see what is being changed… it should be diff on canvas .. check how onlyoffice or google docs
does it"*. The panel reports `Compared with <date> / Differences: 1 / Text edits: 1 / Removed`.

`158` establishes that **no reference product presents a comparison as a count**. Word emits a
merged third document of tracked changes; ONLYOFFICE mutates the open document, setting
`reviewtype_Add` / `reviewtype_Remove` on runs with an author and a timestamp
(`sdkjs/word/Editor/Comparison.js:3864`, `:179`), and its Compare button sits *on the Review
band* beside Accept, Reject, Previous, Next and the four-way display-mode picker; Google Docs
produces a third document whose differences are suggestions. In all three, a comparison **is**
review markup, and the reader's primitives are the review primitives.

`158` §7 then measured our side. The `Revision` model exists with settable `author`/`date`
(`casual-doc-model/src/v1/body.rs:2409`); it is painted as author-coloured underline for an
insertion and strikethrough for a deletion by `apply_revision_markup`
(`casual-doc-layout/src/flow.rs:159`), structurally the same function as ONLYOFFICE's
`addLines`; and `decideRevision`, `decideMovePair`, `decideRevisionGroup`,
`decideAllRevisions`, `setShowChanges` and the chrome's `review.next` / `review.previous` are
all shipped. **The canvas rendering of a diff has been built the whole time.** Two things are
missing, and together they are why the panel is a list:

1. **A revision cannot be injected from JavaScript.** The entire authoring surface is ten
   `suggest*` methods, each meaning "perform *this* edit and record it". From Rust the seam
   exists and is the right one — `Operation::UpdateReviewState`
   (`casual-doc-edit/src/lib.rs:747`), whose paragraph entries (`:301`) each carry a node id
   plus an arbitrary inline list, so one of them can be a revision with any author and date.
   Every `suggest*` method is already built on it.
2. **The sidecar's anchor does not address the live document.** `casual-doc-wasm/src/diff.rs:228-244`
   imports *both* sides freshly, and the chrome's right-hand side is
   `comparableBytes(doc, …)` — a re-export of the live document. Ids restart per import, so
   `right.node` belongs to a throwaway parse. Only `right.path` survives, and there is no
   `path → NodeId` resolver: `blockIndexOf` is the inverse, `documentOutline` covers only
   headings, and `accessibilityTreeWindow` projects no node id at all.

### Decision

**A document comparison is applied to the open document as tracked changes, through the
revision model that already exists, and is read with the review surface that already exists.**

One function is added to the engine, `applyDiffAsRevisions(sidecar, author, date)` — exact
signature in `158` §7.4 — which turns each `DiffChange` into an `InlineNode::Revision` and
applies the lot as **one** `Operation::UpdateReviewState` under `HistoryKind::Review`: a single
undo step, and every downstream surface inherits it with no further change —
`listRevisions`, the canvas markup, `setShowChanges`, accept/reject per change and in bulk,
next/previous, the review gutter, and DOCX `w:ins`/`w:del` on export.

Three consequences are accepted deliberately:

- **The open document becomes the merged document.** This is ONLYOFFICE's answer rather than
  Word's and Google's, and it is why no third-document construction is needed. The result is
  then an ordinary edited document: saving it is an ordinary save, which is how `105` OO-007's
  "result saved as a new version" falls out for free rather than needing its own mechanism.
- **A comparison refuses on a document that already carries revisions**, with a reason, rather
  than merging the two. One `reviewType` field cannot carry both "a person suggested this" and
  "a comparison computed this" without the two accepting and rejecting each other. ONLYOFFICE
  resolves it by accepting all existing changes first, on consent
  (`Comparison.js:3910-3921`); we refuse instead, because silently destroying a reviewer's
  suggestions to run a comparison is the kind of data loss §12 puts first. The consent dialog
  is a later option, not a default.
- **Authorship is a synthetic author**, so the author colour means something and the changes are
  distinguishable from a human's. `setActiveAuthor` is the existing seam; the name comes from
  the compared document.

### Alternatives rejected

- **Render the sidecar as its own canvas overlay, in the chrome, beside the review overlay.**
  Rejected: it is a second mechanism painting the same thing, which `SKILL` §8 forbids and which
  this repository has been burnt by — two implementations of one rule diverge. It would also
  duplicate the author palette (already mirrored in two places), would need its own
  accept/reject or would have none, and **cannot be built anyway** without a `path → NodeId`
  resolver, so it buys a worse design at a comparable cost.
- **Keep the change list and add click-to-scroll.** Rejected as the *destination* — though it is
  the natural fallback if the engine half is deferred. It leaves the reader reading a list of
  the document instead of reading the document, which is the owner's complaint restated, and
  every reference product rejected it.
- **Produce a merged third document, as Word and Google Docs do.** Rejected for now, not
  forever: it is a document construction with its own correctness questions, and mutating the
  open document delivers the same reader experience with no new machinery. Recorded as `158`
  §8 open question 4 rather than closed.
- **Replay the diff as `suggest*` edits in Suggesting mode**, to avoid an engine change.
  Rejected: it routes a comparison through the typing path, so undo granularity, caret
  side-effects and move handling all become wrong in ways a sidecar application does not, and it
  would be O(changes) round trips across the wasm boundary instead of one operation.
- **Four display modes (Word's and ONLYOFFICE's "Display for Review").** Not rejected —
  deliberately unresolved. `ReviewProjection` already declares `Original` and `Final`
  (`body.rs:2347`, `:2349`) and neither is constructed outside tests; `93` §91 already lists
  them as a later extension. A comparison makes them more valuable, not less, and `158` §8 Q2
  carries it.

### What the chrome owes, once the engine half exists

Small, and all in `compare_documents.mjs` and the review chrome: route the comparison's result
into `applyDiffAsRevisions` instead of into `renderResult`; keep the panel as the **index** (the
Google Docs role — an entry scrolls the canvas to its change) rather than as the report; make
each entry **name its object** the way `Deleted: <text>` does
(`web-apps/.../controller/ReviewChanges.js:1192-1196`), because `Removed` with no object is the
same defect as a refusal with no reason; and turn `setShowChanges` on when a comparison
produces changes.

### What is deliberately not decided

Whether the comparison runs in a worker. It must not block the tab, and today it does not — the
existing driver slices, reports progress and cancels at a boundary, which is already better than
ONLYOFFICE's `sync_StartAction(… BlockInteraction, SlowOperation)` blocking wrap. Applying the
sidecar is O(changes), not O(document), so it does not change that picture. The worker question
stays where `compare_documents.mjs` already records it: blocked on COOP/COEP headers GitHub
Pages cannot send.
## ADR-062 — A control's surface follows what the control is: command, preference, or selection property

- **Status:** **Proposed**, 2026-10-04, with **corollary C1 implemented** in the same branch —
  `webapp/src/surface_reveal.mjs`, the measurement-unit reveal, and one `showSettings()`. C1 was
  taken first because its fix is fully determined by the rule, is the smallest, and repairs a
  reachable defect the owner reported. **C2 and C3 are not implemented**: collapsing two ribbon
  toggles into a command plus labelled switches, and five contextual bars into one, are behaviour
  changes to shipped controls and wait on this ADR being accepted. The four defect classes are
  inventoried in `159`.
- **Date:** 2026-10-04.
- **Closes:** nothing yet. **Occasioned by** two owner defect reports: the Measurement-unit row
  opening Settings with no measurement parameter visible, and spell check and grammar check
  sharing one icon.
- **Relates to:** **ADR-061** most directly — a comparison expressed as tracked changes rather
  than as a second markup mechanism is corollary C3 reached independently, and its panel-as-index
  (navigating to changes rather than reporting counts) is this rule's "selection property" row
  rather than an exception to it. Also `SKILL` §10 (every capability reachable from ≥2 surfaces;
  never a dead control), `SKILL` §8 (name the known pattern; quote the competitive standard for
  interaction design), ADR-030 (the extensibility seams, for the same reason: one choke point
  beats two paths), `docs/84` (the context-menu and command registry), `docs/104` (where the
  resulting rows go).
- **Evidence:** `159`, including source-verified ONLYOFFICE behaviour at
  `reference/web-apps` `9c0ca538c3b211052347df09d2a4d6781f023403`.

### Context: the surface was being chosen by available space

`SKILL` §10 already requires that every capability be reachable from at least two surfaces, and
that nothing ship as a dead control. Both rules are about *reach*. Neither says which surface a
control **belongs** on, and so that question has been answered, repeatedly, by where there was
room in the band — with the ribbon as the default because it is the most visible.

Four consequences, all live, all in `159`: a ribbon row that defers to a control in Settings and
never reveals it (the `.focus()` call is dead code and has never had an observable effect); two
as-you-type **preferences** rendered as icon-only ribbon toggles, distinguished by a badge
borrowed from the table domain, duplicating labelled switches that already read correctly in
Settings; the same contextual action bar implemented five times with five positioners, three of
them claiming `role="dialog"`; and an icon vocabulary that nothing owns, so neither "two commands
share a glyph" nor "this glyph is missing from the subset" is checkable.

The common factor is not carelessness. It is that **the question was never posed**, so there was
no answer to be inconsistent with.

### The established pattern, named before any code

This is not a new problem and the answer is not ours. It is the **command/preference/property**
split that every mature editor's UI guidelines state in some form, and it is the same distinction
the platform accessibility model already draws: a `button` does a thing, a `checkbox` or
`switch` carries a state, and a property sheet edits the current object. `aria-pressed` on a
`button` exists precisely because people kept building the middle case out of the first, and it is
a *mitigation* for screen readers, not a licence — it tells a screen reader a state that a sighted
reader still cannot see.

The competitor independently lands on the same three homes (`159` §7): the measurement unit is a
labelled dropdown on a settings page (`cmbUnit`,
`reference/web-apps/apps/documenteditor/main/app/view/FileMenuPanels.js:747`); the spelling
preferences are labelled checkboxes on that same page (`chSpell`, `:534-536`), one of which
**disables its dependents when off** — a relationship an icon cannot express; the as-you-type
toggle is in the **status bar**, never the ribbon (`ReviewChanges.js:895-910`, rendered only into
`#btn-doc-spell` at `Statusbar.js:112-113`; `Toolbar.js` has zero `spell` hits); and they ship
**no grammar check at all**, so the question of a second glyph never arises for them.

### Decision

**A control's surface is a consequence of what the control is, not of where there was room.**
Three kinds, each with one home:

| Kind | What it is | Home |
| --- | --- | --- |
| **Command** | does something once, when invoked | ribbon / menu / palette / context menu — icon **and** label; ≥2 surfaces per `SKILL` §10 |
| **Preference** | a persistent choice that outlives the selection | a **labelled** control in Settings, state readable in the label. A fast path may be a **menu row with a checkmark** or a status-bar control — never an icon-only ribbon button |
| **Selection property** | changes with what is selected | contextual bar / properties dialog |

**Corollary C1 — a pointer must point.** A control that defers to a control on another surface
must *reveal* it: open or select the owning surface, scroll the control into view, and place focus
on it **after** that surface has finished its own focus management. Opening the container is not
revealing the control. Where the owning surface defers its own initial focus by a microtask, the
reveal uses a `requestAnimationFrame` — microtasks always drain before the next animation frame,
so this is an ordering guarantee rather than a race won by being later.

**Corollary C2 — an icon is a name in a shared vocabulary.** Two *different* commands may not
wear one ligature, and no command may name a ligature the bundled subset lacks. The same ligature
on several surfaces for the **same** command is correct and required — so the rule forbids
collision, not repetition.

**Corollary C3 — one job, one mechanism.** Where two surfaces do the same kind of job they go
through the same seam. A second implementation of one rule is evidence the abstraction is wrong.

### What this rule predicts

- The measurement row stays where it is — a preference, correctly homed in Settings, with a
  pointer to it — and the pointer is fixed to actually reveal (C1). **Done in this branch.**
  The reveal became its own module because inlining it broke `main.js`'s line ratchet, which
  is the ratchet buying a seam rather than just a smaller file: with the frame scheduler
  injected, the ordering guarantee is checkable in node. Predicting the fix was not enough on
  its own, though — the rule also predicted a pane-guard defect that **turned out not to be
  reachable**, and only running the guard found that out (`159` §3.2). A rule that predicts
  well still does not excuse a guard from being driven.
- Spell check and grammar check stop being icon-only ribbon toggles. The labelled switches
  already in Settings are their home; a checkmarked menu row is the fast path; and the
  `spellcheck` icon goes to the one thing that is genuinely a command — running a proofing pass.
  The icon collision dissolves without touching the font subset, and the borrowed
  `.table-command-badge` goes with it.
- The five contextual bars collapse onto one `createContextualBar` composing
  `popover_position.mjs`, `popover_manager.mjs` and `ribbon_nav.mjs` (C3). Scoped as its own
  lane: it is a behaviour change to five live surfaces.
- The icon vocabulary acquires an owner, and with it the two guards C2 needs — no ligature
  serving two distinct commands, and no ligature missing from the subset.

### What is deliberately not decided

- **Whether a document-wide proofing pass exists as an invokable command.** The fix above assumes
  one. Ten proofing modules exist in `webapp/src/`; which is reachable, and whether a pass is
  O(document) and therefore owed off-main-thread, cancellable, progress-reporting treatment
  (`SKILL` §8), is open — `159` §9 Q2.
- **The replacement glyphs** for the `format_list_numbered` and `format_indent_increase`
  collisions. Icon choices touch the deliberate design tokens, so they are the owner's
  (`SKILL` §11).
- **Whether our status bar should carry fast paths at all.** The competitor's pattern is
  attractive but our status bar is hand-written markup with no registration seam; adopting it
  needs the seam first.
- **Word and Google Docs are not cited as support.** Neither is checked out, so `159` §7.3 is
  fenced as unsourced recollection. The preference/command distinction rests on ONLYOFFICE,
  which is sufficient for it; the *combined* spelling-and-grammar pass is not yet established
  from any source.

### How this will be verified

Three guards, each to be driven red before it is trusted (`SKILL` §4):

1. **A reveal reveals** — **built and driven red.** After invoking a deferring control, the named
   control is visible and is `document.activeElement`. Two layers: the ordering property in node
   with the frame scheduler injected (`surface_reveal.test.mjs`, which creates the condition by
   queueing a microtask that steals focus), and the guarantee in a browser over both reader
   routes (`e2e/file-page-panes.spec.mjs`). Mutations: a synchronous callback fails the first
   with `+ "the surface's own initial focus"`; restoring `…?.focus()` fails the second with
   `Expected: focused / Received: inactive` — the reported defect, reproduced. It asserts the
   guarantee, not the frame or the pane branch, which is why it survived §3.2's withdrawal
   intact.
2. **No ligature serves two commands** — built on the runtime registry rather than a regex over
   `editor.html`, because a regex sees 38 of 165 icon buttons. Written to permit one ligature on
   many surfaces for one command. It lands with the fixes, since it must report the three known
   collisions to be meaningful.
3. **Every ligature the chrome uses exists in the subset** — parsed from the woff2's ligature
   table. **Proposed, not promised**: whether a parser is available in this toolchain is untested.

`chrome_fonts.test.mjs` today asserts self-hosting, the `wOF2` magic and the licences, and
nothing about ligature coverage — so a glyph absent from the subset renders as a blank button with
no error. Guard 3 is the one that closes that, and it also gives the open `pilcrow` glyph question
a mechanical answer.

## Pending ADRs

- shaping stack: HarfBuzz wrapper versus platform-native shaping;
- native renderer: Skia, Vello, tiny-skia, wgpu custom, or hybrid;
- internal text storage: rope, piece tree, or chunked sequence;
- ~~collaboration operation model: OT vs CRDT~~ — superseded by **ADR-033** (proposed; owner decision 2026-09-15; see doc 107);
- ~~PDF generation backend~~ — superseded by **ADR-031** (Phase 0 accepted and implemented; see doc 98);
- PDF semantic reconstruction and browser OCR — **ADR-034 proposed experimental**;
- document assistance, local semantic retrieval, and MCP adapter — **ADR-035 proposed experimental**;
- schema format: canonical CBOR encoding profile and golden vectors;
- plugin ABI stability;
- proofing context pack: which source corpus is redistributable — **ADR-042** leaves it open;
- whether layout uses fixed-point units internally.

## ADR-063 — The relay's WebSocket is a framing adaptor, not an async runtime: `opendoc-relay` stays in the workspace and in the wasm gate

- **Status:** **Accepted**, 2026-10-04, **implemented** in the same branch —
  `server/src/websocket.rs` (RFC 6455 as a `Read`/`Write` adaptor), `server/src/serve.rs` (the
  per-connection loop, moved into the library), and five end-to-end guards in
  `server/src/serve_tests.rs` that drive real `ClientSession`s over real sockets.
- **Date:** 2026-10-04.
- **Closes:** the `107` 6.6 transport question — how a browser reaches the relay — and the
  dependency question in front of it.
- **Relates to:** **ADR-047** (a dumb relay: it orders chunks and holds no document), **ADR-057**
  (the `ODC1` byte codec this carries), **ADR-058** (durability; the journal is what makes the
  resume guard meaningful), **ADR-060** (the host-signed grant that arrives on `Welcome`),
  `docs/152` §2c (back-pressure), `docs/107` §4 B1 (the per-interaction budget), and `SKILL` §8
  (name the established pattern before designing).
- **Evidence:** measured in this branch. The relay's 58 tests, the five transport guards, and the
  mutation output recorded below.

### Context: the decision that was framed, and why it was the wrong frame

The question arrived as a choice between two options. The client is a browser, so the relay has
to speak WebSocket; the obvious library is `tokio-tungstenite`; `tokio` fails the `wasm` CI job,
because that job runs `cargo check --workspace --target wasm32-unknown-unknown` and
`opendoc-relay` is a workspace member, so `mio` refuses the target outright —

```text
This wasm target is unsupported by mio. If using Tokio, disable the net feature.
```

— and a configuration with no `net` and no multi-thread runtime is not a transport. So: **exclude
`opendoc-relay` from the workspace wasm gate, or move it out of the workspace.**

Both options cost something real, and both were costed before either was taken:

| Option | What it costs |
| --- | --- |
| Exclude the member from the wasm gate (`cargo check --workspace --exclude opendoc-relay`) | The gate stops being "the workspace compiles for the browser" and becomes a list somebody maintains. Every future member must be remembered, and the failure of forgetting is a crate that silently leaves the browser build — which is exactly the class `nothing_under_crates_depends_on_the_relay` exists to catch, now half-enforced. |
| Move `server/` out of the workspace | A second lockfile, a second `target/`, a second CI matrix entry, and the `path` dependency on `casual-doc-transaction` becomes a cross-workspace path — so a breaking change in the engine is no longer caught by the engine's own `cargo check --workspace --all-targets`, which `SKILL` §5a names as the main defence against two green PRs making a red `main`. On a machine whose disk has filled eight times in a day, a second `target/` is not a neutral cost either. |

### The decision: neither, because the premise is removable

**WebSocket is a framing layer, not a concurrency model.** RFC 6455 is a handshake (one HTTP
`Upgrade`, one SHA-1, one base64) and a length-prefixed binary frame with a mask. None of it
needs an async runtime, and the relay already has a concurrency model — thread-per-connection
`std`, documented in `opendoc_relay::transport` as suited to tens of participants per room, which
is what a dumb relay is for. `tokio` was never required by WebSocket; it was required by the
*library* that was reached for first.

So the relay stays a workspace member, stays in the wasm gate unmodified, and gains ~450 lines
implementing the specification directly, shaped as a **framing adaptor** — the decorator pattern
`std::io::BufReader` has:

- `websocket::Unframed<R>` is a `Read` yielding the payloads of the data messages it receives;
- `websocket::Framed<W>` is a `Write` turning each `write` call into exactly one binary message.

`Relay`, `transport::Frames` and `fanout::Participants` compile over them **unchanged**, so every
test those types already had still exercises the real thing, and the layer above does not know
the adaptor is there. The alternative shape — a `Message` enum a caller matches on, which is what
a WebSocket library exposes — would have given the relay a second record layer on top of the
`ODC1` frames it already has, and two mechanisms for one rule is what `SKILL` §8 says to avoid.

This decision is **not** a claim that async is unnecessary at scale, and it does not reopen that
question. It is narrower: the relay's per-message work is "append, assign a number, write to N
sockets", there is no computation to overlap, and moving to an async runtime is an optimisation
of a working mechanism with a dependency decision in front of it. When that decision is taken,
the table above is the cost of the two options and this ADR is superseded, not contradicted.

### What is paid for it, stated rather than glossed

Two primitives are implemented here: **SHA-1** and **base64**. SHA-1 is not used as a hash
function in the security sense — RFC 6455 §1.3 specifies it as a fixed, publicly-known
transformation of a nonce the client sends in the clear, and what it buys is that a proxy cannot
be fed a crafted HTTP request that looks like a cacheable response. Both are checked against
**published vectors** (FIPS 180-4's four examples, RFC 4648 §10's seven, and RFC 6455 §1.3's
worked `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=`) rather than against themselves, because a hand-written
SHA-1 that agrees with its own test is a hand-written SHA-1. Mutating one index in the message
schedule (`words[index - 8]` to `words[index - 7]`) reddens all four vectors and the handshake
value: `left: "a12b3c21c2f1c3ec6da081d6967099fbe7a6636b"`, `right:
"da39a3ee5e6b4b0d3255bfef95601890afd80709"`.

The relay also now speaks **WebSocket and nothing else**. There was no raw-framing client in the
tree when this landed — the only client that exists is a browser — so a sniffing dual path would
have been a second mechanism serving nobody. A deployment that wants raw frames has
`transport::Frames` over whatever stream it likes; what it does not get is this binary doing two
things.

### Which side owns which transport

The browser's transport is **the host's `WebSocket`**, not a Rust socket. `websocket::connect` —
the client half of the handshake — exists for tests and for a native client, and takes its nonce
as an argument rather than generating one, for the same reason nothing in the engine reads a
clock or mints an identity. Nothing in `casual-doc-wasm` opens a socket; `cargo check --workspace
--target wasm32-unknown-unknown` stays green with no exclusions.

### Three defects this surfaced, each fixed in the same branch

Moving the per-connection loop into the library — `opendoc_relay::serve`, for the reason
`opendoc_relay::relay`'s own header already gives about `main.rs` being untestable — made three
things visible that had been true and unreachable:

1. **The answer to a sender was written outside the room lock.** `relay`'s header states the
   contract as "decide → journal → answer → fan out, all under one lock held by the caller", and
   the binary wrote the answer after releasing it. So this connection's thread wrote the answer to
   its own socket while another participant's thread wrote the fan-out to the same socket through
   a different descriptor, and nothing but luck stopped them interleaving. A torn frame is not a
   dropped message: a self-describing length framing has no delimiter to resynchronise on, so the
   peer can never recover. The answer is now written under the lock.
2. **A journal failure skipped the cleanup its own comment called mandatory.** The loop used `?`
   on `Relay::handle`, which returned from the function past the `disconnected` call — and
   "a writer left behind is a socket every future chunk is written to and reported as failed
   forever" is that comment. A failure to journal is exactly when a relay must not also leak a
   participant. Every path now reaches the bottom.
3. **`Join::resume`'s doc comment said "when this is a reconnect".** A key is recorded only by
   the join that *presented* it, so a client following that sentence — withholding the key on its
   first connection — can never be recognised on its second: it is handed a fresh `Welcome` and a
   new `ClientId`, and whatever it had not had acknowledged is gone with no refusal naming the
   loss. The comment now says what the code does. This cost this lane a debugging cycle and would
   cost an integrator more.

### Two mechanisms remove a dead participant, and that was measured rather than assumed

`docs/152` §2c's back-pressure policy is eviction on a failed write. Over a real socket there is
a second route to the same outcome, and the guard asserts the **outcome**: with nothing mutated,
a dropped connection is noticed by its own reader and `Relay::disconnected` removes the
participant, producing no eviction notice at all; with `disconnected` neutered the guard stays
green, which is the evidence that `evict_unreachable` is genuinely reachable over a socket and
not only over a failing `Vec`. Pinning which mechanism won would be a guard on the circumstance
rather than the guarantee (`SKILL` §4), so the mutation that reddens it is the one line both
paths share — `Participants::left` returning `contains_key` instead of removing — measured at
`left: 2, right: 1`.

The client's half of eviction is the transport's: from a client, an eviction is indistinguishable
from any other dropped connection, because eviction *is* a failed write and there is no socket
left to send a refusal down. That is why no `Refusal` variant was added for it and the protocol
version was not bumped: the reader-facing state is a transport state, routed the way
`session.grant-unreadable` already is in `webapp/src/session_access.mjs` — a code in the chrome's
one table that the engine never produces, because the engine is never handed the condition.

### Addendum, 2026-10-04 — the browser half, and the one thing it changed about this decision

Built in the same branch: `casual-doc-wasm/src/collab.rs` (the session, sans-I/O) and
`webapp/src/collab_transport.mjs` (the socket, with its opener, timers and randomness injected).
Both are **not yet reachable from the product** — `webapp/src/main.js` has no call site — and
that is stated here rather than implied, because a capability recorded as built and not
reachable is the most expensive recurring pattern in this repository (`SKILL` §9.4, `109` RM-16).

The division above survived contact. One thing about it did not, and it is worth recording
because the obvious implementation of the browser half is wrong:

> **`ClientSession::flush` advances its mark AS IT HANDS A CHUNK OVER, and no path rewinds it
> except a `Refused` the client actually received.** `flush` sets `self.flushed = upto` and
> pushes to `self.sent` before returning; `resumed` replaces the capabilities and sets
> `awaiting`, and rewinds neither.

So the reasonable-sounding design — "the engine holds the unacknowledged work, therefore the
browser needs no queue, and anything queued may be dropped on a reconnect" — loses data. A
chunk the browser takes from `collabNextChunk()` and fails to write is offered by nothing ever
again. `152` §5.4 says "the log is the pending queue" and that remains true of what has *not
been flushed*; it is not true of what has. The transport therefore holds **custody**, not a
cache: it never polls while the socket is shut (a poll is a handover), it keeps a frame until
`send` returns, and it replays the queue in order ahead of anything new on the next connection.
Replaying a chunk the relay may already hold is the designed recovery rather than a hazard —
`(client, seq)` suppresses the duplicate, or `ODC-7009 StaleBase` comes back and
`ClientSession::refused` rewinds the mark so the engine re-offers a rebased chunk.

Two consequences follow for anyone writing a second client:

- the browser must not treat `collabNextChunk()` as idempotent or cheap to discard;
- a `Join` is the exception — it is rebuilt per connection from `collabState.revision`, so a
  join whose write failed must **not** be held, or the next connection sends two.

The reader-facing state is three values with a sentence each (`collab.connected`,
`collab.reconnecting`, `collab.stopped`) plus the chrome-only `session.connection-lost` this
ADR's previous section specifies, and the backoff is full jitter per AWS's "Exponential Backoff
And Jitter", guarded as a doubling ratio rather than against a clock (`107` §4). Twenty-one
mutations were run across the two new guard files and every one reddened its guard; two guards
had to be rewritten first, because the mutation exposed the guard rather than the code — one
asserted a join ordering a queue-everything implementation also satisfies, and one drove only
the phases whichever of two redundant gates happened to check first. The second of those is now
**one** gate in `connect`, for the reason `SKILL` §8 gives about two mechanisms for one rule: a
gate split in two is a gate whose halves cannot both be driven red.

## ADR-064 — Version history's Show changes is a read-only projection against the predecessor; ADR-061's mutation stays with Review ▸ Compare

**Status:** accepted; **its presentation is superseded by ADR-065** — the read-only projection
against the predecessor stands, and is now painted on the canvas as a redline rather than listed
as a unified diff in a panel. **Supersedes ADR-061 for one of its two routes.** `docs/139` §9.4
and `docs/140` §11.4/§11.6 are corrected in the same change.

### The decision

Two entry points reach one comparison engine, and they are now two decisions rather than one:

| | Review ▸ Compare | Version history ▸ Show changes |
| --- | --- | --- |
| The other side | a document the reader chose off the disk | the selected version's **predecessor** |
| This side | the open document, exported | the selected version's checkpoint |
| What happens to the open document | tracked changes written into it, one undo step (**ADR-061, unchanged**) | **nothing** |
| Where the differences are read | the canvas, through the review surface | a read-only **unified diff** in the panel |
| Reference this matches | ONLYOFFICE | Word, Google Docs, and ONLYOFFICE's own history |

### Why one ADR could not cover both

ADR-061 asked "how is a comparison expressed?" and answered "as tracked changes, not as a
second markup mechanism". That answer is right for Review ▸ Compare, and it was applied to a
route whose question is different: *what changed in this version?* Three measured facts, not
three opinions:

1. **It compared a version against itself, and could only ever say "No differences".**
   Clicking a row runs `void openPreview(row.versionId)` — "a click is a decision already
   made" — and `showVersionPreview` assigns `doc = previewDoc`, so the module-level live
   document *is* the historical one while a preview is up. The panel's own side is
   `comparableBytes(doc, …)`; a freshly parsed preview has `revision == 0`; and
   `ExportMode::ExactIfUnchanged` returns the retained original bytes **verbatim**. So the
   bytes handed in as "mine" were byte-identical to the checkpoint handed in as "theirs".
   The ⋮ is a SIBLING of the row's own click target (`item.append(entry, actionCell)`), so a
   test that pressed only the ⋮ never opened a preview and never saw this — which is why the
   spec covering this route stayed green for as long as the feature was broken.
2. **No competitor routes history through a mutation.** Word and Google each produce a third
   document and leave the sources untouched; ONLYOFFICE mutates only from Review ▸ Compare and
   its history UI never mentions comparison at all (`docs/164` §5, established by five
   independent greps of the vendored checkout). Writing tracked changes into a reader's current
   document because they asked a question about the past is a behaviour nobody has — so the
   behaviour the owner was unhappy with was not a weak version of a competitor's feature.
3. **`docs/139` §9.4 already specified the read-only answer**, and ADR-061 reversed it for both
   routes when it needed to reverse it for one. `docs/164` §7.2 found the two documents
   publishing the opposite of the code, and noted that the superseded requirement was the one
   matching the majority of the references.

### Why the predecessor rather than the live document

Google's model — a stored version against the current document — is a legitimate answer and is
what was intended here. It is also the answer that cannot survive this product's own preview:
the preview *is* the live document while it is up, so one of the two sides disappears exactly
when a reader uses the feature. Comparing against the predecessor answers the question the
panel is opened to ask, is stable regardless of what is on screen, and needs no live-document
state at all — which is what makes the read-only guarantee mechanical rather than a promise.

Two consequences, both deliberate:

- **The head row is live now.** It used to refuse with "comparing it with itself", which was
  true of a comparison against the document on screen. Against its predecessor the head is the
  most useful row in the panel: what changed in the latest save?
- **The earliest version kept refuses instead**, because there is nothing before it. Disabled
  with that reason, and the same sentence also exists as a status message, because the command
  is reachable from more than one surface and a refusal that exists only as a disabled control
  is not a refusal on the others.

### What "read-only" means mechanically, so it is checkable and not a claim

Both sides are checkpoint byte arrays. The live document is not read, not exported and not
written; `applyDiffAsRevisions` is not called; `io.landed` is not called, so
`setShowingChanges` never fires and no `Operation::UpdateReviewState` is built. There is
nothing to undo because nothing happened. The panel renders `[data-compare-read-only]` and
never `[data-compare-marked]`, which is the marker `compare-on-canvas.spec.mjs` asserts
**visible** for Review ▸ Compare — so one attribute distinguishes the two routes, and a guard
on either side fails if they are ever confused.

The route is also deliberately **not** behind the Viewing-mode mutation gate. A comparison that
writes revisions is a mutation and goes through that gate; this one writes nothing, and a
preview is read-only by definition — so gating it there would refuse the feature in precisely
the state a reader reaches it from.

### The presentation, and the prior art it is named after

A **unified diff** over blocks: changed regions as added/removed lines, each hunk with a few
unchanged blocks of context and a control that pulls more. Not side-by-side, which needs two
synchronised document renders when our body is one canvas, and which Word's own documentation
concedes "is not the best tool for making changes to your document". Not a summary count,
which is what the panel used to be.

Three established patterns carry it, named before any code was written (`SKILL` §8):

- **hunks with expandable elisions** — the diff's own shape, with a BLOCK as the unit because
  the engine's projection is a block forest, which is the honest unit rather than an
  approximation of a text line;
- **windowed (virtual) scrolling** — a flat fixed-height row array, a sizer of
  `rows × height`, and only the visible slice in the DOM, so a scroll tick is O(window) and
  not O(changes) (`docs/107` §4). The previous renderer built one `<li>` per change in one
  synchronous uncapped loop. Rows are `nowrap` with their own horizontal overflow, which is
  what a GitHub diff line does and is what keeps every row exactly one row tall — the
  virtualizer's correctness depends on it, so JS owns the height and writes it to CSS as
  `--diff-row-h` rather than the two repeating one number;
- **lazy pull for context** — the unchanged blocks are in neither side's sidecar, because a
  change record names only what changed. They are read from the comparison's two parsed sides,
  O(depth) each, on the press that asks for them. That is why `WasmVersionDiff` now keeps its
  parsed sides until the handle is freed instead of dropping them on completion: the peak
  memory is unchanged (both were resident for the whole comparison), the duration is bounded by
  the panel being open, and the release points are the panel closing, the next comparison, and
  a result with no differences.

### Two engine exports this needed, and why the host could not do either itself

- **`WasmDocument.nodeAtStoryPath(story, path)`** over
  `casual_doc_diff::projection::block_at_path` — a resolver that was merged and not exposed.
  `DiffAnchor` carries a `node`, and it is a trap: both sides of a comparison are parsed by the
  diff facade and ids are minted per import, so that id addresses a throwaway parse whose
  counter restarted at 1. `navigateToReviewAnchor` takes `{node, start, end}`, a byte-for-byte
  match for `DiffAnchor`, so handing it one scrolls silently to an unrelated paragraph with the
  same ordinal — wrong destination, no error, nothing to tell the reader from. The path is the
  coordinate that survives, and resolving it is not a walk a host should write: the projection's
  block sequence is not the model's block list (a table contributes rows and cells, which are
  not `BlockNode`s; a block-level content control contributes itself *and* is descended into).
  Returns `None` — never a guess — for a path that names a row or a cell, leaves the shape,
  names an absent story, or names a non-paragraph block.
- **`WasmVersionDiff.blockTextAt(side, story, path)`** for the context, with `None` at the end
  of a sibling list, which is how an expand control learns the document stops there rather than
  needing a second "how many siblings" call.

Navigation is offered only where a path can resolve, which is Review ▸ Compare: there the
right-hand side **is** the open document. In version history's read-only route neither compared
state is on screen, so there is nowhere to scroll to — and the unified diff is itself the
reading surface, which is why no row pretends otherwise.

### The summary beside the counts

Word's Reviewing Pane is the sourced spec: "the total number of changes and the number of
insertions, deletions, moves, formatting changes, and comments". Those five ship, from the
engine's own `kindCounts` rather than recomputed in the host, so the breakdown and the total
cannot drift apart. Three places the mapping is not one-to-one, each decided rather than
fudged: a move is counted **once**, at its destination — the engine reports both halves, and
adding them prints "2 moves" for one block that moved; comments are a **family** here and a
category there, so that number comes from `familyCounts`; and `property` is ours and Word has
no word for it, so it is published as a sixth row rather than folded into formatting (which
would overstate a formatting count) or dropped (absence from a published breakdown is an
overstatement by omission, `SKILL` §9.3). Every row is published even at zero: "0 deletions"
is a fact a redline reader wants, and a surface whose rows come and go cannot be read at a
glance or asserted by a guard.

### What this does not decide

- Whether a merged **third** document is ever built (`docs/164` §9 question 6). §5 gives it
  competitive weight — two of three references — and not a decision.
- Whether two **arbitrary** stored versions can be compared. No reference does it; if we build
  it we are first, and the risk is ours (`docs/164` §5).
- Compare's **options**. Word documents ten toggles plus granularity and destination;
  ONLYOFFICE exposes one; we still expose zero. `docs/164` §8 row 7 queues word/character
  granularity first, and this change deliberately does not pre-empt it.
- The tracked-changes **refusal dead end** (`docs/164` §8 row 6). Untouched, and it only ever
  applied to the route that mutates — so one effect of this split is that version history can
  no longer meet it at all.
- **Retention** (`docs/164` §8 row 8). Unchanged here, and worth noting that it now bears on
  this feature: pruning the middle of a lineage changes what "the predecessor" is, and
  `predecessorOf` deliberately answers "the one before it in what is still kept", which is
  also what the reader sees on screen.

## ADR-065 — A comparison is read as a redline on the canvas: version history and Compare both show changes on the page

**Status:** accepted. **Supersedes ADR-064's presentation** (the unified block diff in a side
panel) for version history, and **ADR-061's first step** for Review ▸ Compare: a comparison is
now shown before it is written. ADR-061's mechanism — a comparison is expressed as tracked
changes, never as a second markup layer — is kept, and is what makes this possible. Asked for by
the owner: "a diff canvas for version diff, to see the changes on that version — what anyone has
removed or added or changed in position — just like Google Docs."

### The decision

Both routes put the same picture on the canvas: a **redline**, read-only.

| | Version history (click a version) | Review ▸ Compare (pick a file) |
| --- | --- | --- |
| Older side | the version's predecessor | the other document (or this one, after **Swap order**) |
| Newer side | the version | this document (or the other one) |
| On the canvas | a throwaway copy of the newer side, every change painted as a tracked change in its author's colour, markup view on | the same |
| Who the changes are by | the version's recorded actor | the other document's name |
| Navigation | "3 of 12", previous/next, a key (Added / Removed / Moved / Reformatted) on the preview bar | the same, plus a list of changes in the panel |
| What happens to the reader's document | nothing | nothing, until **Keep as tracked changes** (ADR-061's `applyDiffAsRevisions`, one undo step, its refusal unchanged) |

### What the engine does

`WasmDocument.showComparison(job, author, date)` paints a finished `WasmVersionDiff` into the
handle it is called on, which must be the comparison's newer side — checked by content digest,
not assumed. Every change becomes an ordinary `InlineNode::Revision` through the existing
operations, so the page, the author colours, `listRevisions` and the review layer read it with no
change:

- insertions and a move's destination are marked where they are (`classify_change`, shared with
  `applyDiffAsRevisions` through one `comparison_review_operation`);
- removed text inside a paragraph is put back struck at its offset, read **in full** from the
  older side the job still holds — not limited to the record's 160-byte excerpt;
- **a whole paragraph that is gone — deleted, or moved away — is put back struck where it stood**,
  through `Operation::InsertBlocks`. This is the case a tracked change could not express and the
  reason ADR-064 retreated to a text list. `casual-doc-diff` now records where removed content
  stood: `DiffChange::place`, an insertion point in the newer document, computed during alignment
  (just past the nearest pairing before it, so the old text reads before its replacement);
- a paragraph whose exact text is unique on both sides is never paired positionally as an
  *edit* of whatever replaced it: it goes to move detection. Without this, a move beside an
  unrelated removal was word-diffed into one interleaved line ("Removed~~We~~ line…");
- moves are drawn with the **double** form of each mark — double strikethrough where text left,
  double underline where it arrived — which is Word's convention and the only cue that struck
  text is elsewhere rather than gone. Colour cannot carry it: colour is the author's.

A restored paragraph keeps its text, tabs, breaks, direct formatting and its styles **matched by
name** (a `StyleId` is minted per import and names nothing across two parses). Pictures and other
objects in it, and list numbering, are not reconstructed and that is reported
(`removedObject`), never silent. Formatting, table-structure, section and definition changes have
no mark: they are listed and, where they sit on a paragraph, navigable.

### Why this and not the alternatives

- **Not a second paint mechanism** (ADR-061's rejection stands). The redline is a document; the
  review layer paints it.
- **Not side-by-side** (ADR-064's reason stands: one canvas, one paginator). A redline needs one
  column, which is what Word's "compare into a new document" and Google's history both show.
- **Not a mutation of the reader's document on first click.** No reference writes a comparison
  into the open document before the reader asks; ADR-061 did, and refused any document with
  suggestions in it as a consequence. The redline view refuses nothing: nothing on it is ever
  decided, so a version's own suggestions and the comparison's marks cannot decide each other.

### What it costs

O(both documents) to parse and compare, in slices with progress and a Cancel (`runComparison`),
then one O(newer) open and an O(changes) paint, once per view. One O(document) content digest
proves the handle is the newer side. Never O(document) per interaction: stepping to a change is a
selection and a scroll.

### What this does not decide

- Per-change attribution inside a version (`docs/139` VH-016). A version records one actor, so a
  version's changes carry one colour. Google attributes each edit to its editor; that needs an
  author on the transaction log, which does not exist.
- Paint for formatting changes (`PropChange` is not drawn by the markup layout). They are listed.
- Removed tables, rows and headers/footers are still listed rather than painted (body
  paragraphs only, for `classify_change`'s reason).
- A worker. Still the main thread in slices, for the reason `diff.rs` gives.

## ADR-066 — HTML export is resolved by the renderer's resolver: one cascade, one palette, a normalized stylesheet

**Status:** accepted. Asked for by the owner: "HTML export fidelity is way too weak", then "fix
and embed header, footer, drawings and TOC". Design and measurement: `docs/167`. Tracker: `109`
HF-286, HF-284, HF-285.

### The decision

The single-file HTML exporter (`casual-doc-io` `html`) no longer reads direct formatting alone.
Every paragraph, run and table cell is resolved through `casual_doc_layout`'s `StyleCascade` —
document defaults, table style and its conditional regions, paragraph style chain, character
style, direct — and every colour, fill and border through the renderer's palette and
border-conflict rules, exposed read-only as `casual_doc_layout::paint_values`. Each function
there **delegates** to the one the flow engine calls (`cell_shading_rgba` was extracted from
`flow.rs` so both call it); none re-implements a rule. The page and the export therefore cannot
disagree about what a document looks like, and a fix to one is a fix to both.

The stylesheet is **normalized**: one class per paragraph style with its resolved declarations,
each element carrying only its delta (`initial` where it undoes what its class sets). Vertical
spacing is Word's additive arithmetic, not CSS margin collapsing. Lists are nested `<ul>`/`<ol>`
whose items carry the label the page prints; tables carry grid widths, resolved borders, fills
and padding, and a `<thead>` only for rows the document marks as repeating headers; footnotes
and endnotes are written after the body and linked both ways; `@page` carries the first
section's size and margins.

Drawings follow the same rule. A shape is the geometry the anchor engine evaluates
(`paint_values::shape_content` → `geometry_content`, with the theme's fill and line), a chart
with no stored picture is what the page's own `compose_chart` composes
(`paint_values::chart_drawing`), and both are written as inline SVG, so a drawing the page fixes
is a drawing the export fixes. The chart's labels are carried beside its primitives as strings
and written as SVG text, the one place the export measures for itself (an average advance;
`docs/167` §2). A drawing in front of or behind the text is placed at its offsets in its
paragraph's box; one placed only approximately is reported. Headers and footers are written
once, above and below the text. A tab goes to its stop on the two lines tabs are used for — a
contents line and a header line — and keeps the default grid elsewhere, reported.

### Alternatives rejected

- **A second resolver inside the exporter.** It is how the import, the model validator and the
  renderer came to answer one numbering question three ways (`docs/142` LST-10/LST-34). One
  resolver, read through a narrow public door, is the fix for that class.
- **Inline every resolved declaration on every element.** Exact, but each paragraph would repeat
  its style's dozen declarations; the class-plus-delta form is the same picture at a fraction of
  the size and is what an editor of the HTML expects to find.
- **Absolute positioning from the paginated layout** (one `<div>` per line at its page
  coordinate). Pixel-faithful and useless as a web page: no reflow, no selection order, no
  accessibility. HTML is the flowed format; PDF is the paginated one.
- **Reporting headers and footers instead of writing them** — this ADR's first position, on the
  ground that a "Page 1 of 14" footer on a page with no pages is a wrong fact. Reversed at the
  owner's request: a reader expects the letterhead and the footer's text, and a page number in
  them is its field's saved result, as every field in the export is. Written once, and that
  much is reported (`html.header_footer_once`).
- **Drawings as raster pictures**, rendered by the engine and embedded as PNG. Exact to the
  pixel and wrong as a web page: a grouped text box's words could not be selected, searched or
  read aloud, and the file grows with every shape. SVG keeps each shape a shape and each word
  text.
- **Chart labels as glyph outlines** (the page's shaped glyphs as paths). Exact placement, but
  the labels stop being text; the estimate is a few percent off and the text stays text.
