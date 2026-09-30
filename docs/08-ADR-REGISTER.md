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
- **The document surface is a named exception to "no horizontal scroll", not a solved
  problem.** A 6.5in text column cannot be both 390px wide and readable; the answer is
  reflow, which both references ship (Google's Pageless, ONLYOFFICE's
  `api.ChangeReaderMode()`), and which is layout-engine work. `148` §6 and §9 carry it.
  The chrome is guarded; the paper is declared. **Superseded in part by ADR-045**,
  which specifies that engine work precisely; the exception itself stands until it
  is built.
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

**Status:** Accepted 2026-09-30; **engine half implemented**, shell half (`151` §6)
outstanding. Specified in `151-REFLOW-PAGELESS-LAYOUT-DESIGN.md`. Completes the
consequence ADR-044 left open.

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

**Where we differ from ONLYOFFICE, deliberately:** their reader mode sets
`SelectEnabled = false` and is **not editable**. A phone browser is our whole mobile
story, so a reading mode you must leave in order to type is not an answer.

**Consequences.**

- Entering or leaving reflow is O(document) — a full re-shape, because the galley
  cache is width-scoped. It is a *mode change*, not an interaction: it goes through
  the background/progress path, is cancellable, and is never driven straight off a
  resize event. Resize must be quantised and debounced, or it is an O(document) pass
  per animation frame on the slowest device we support.
- A `PAGE` field and the page counter resolve against **tiles**, which are not pages.
  The shell shows neither in reflow rather than printing a number that is wrong.
- Print and PDF export force `Paged` unconditionally.
- A table too wide for the reflow width keeps a horizontal scroller **of its own** —
  Google's arbitration. That is the one horizontal scroll that survives, and it tells
  the reader something true about a table rather than something false about the page.
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
minted. **The measurement that decides whether this was right is the refusal rate as
concurrent writers rise**, and nothing here is proven until it is run (`152` §10 Q3).

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
- **The live editor's minting namespace is derived from the document today**, so two replicas
  collide from the first edit. Until that is fixed in `casual-doc-wasm` — the next increment,
  a different lane's crate — a session refuses every arrival that introduces an id, loudly and
  with `ODC-7008`, because the alternative is a silent overwrite of a definition.
- `docs/20` gains `ODC-7002`…`ODC-7009`. `ODC-7001` is reused for `CannotMerge`.
- The two O(document) costs — one document clone and one `BlockIndex` build — are on the
  **contended** path only: a remote edit arriving while this replica has unacknowledged work.
  Never on a keystroke, never on an uncontended arrival.
- Deliberately not decided or built here: the byte codec, the relay binary, presence,
  collaborative undo, the host-signed grant, and durability. `152` §9 and §10 say why for each.

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
