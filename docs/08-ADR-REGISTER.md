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
and diff contracts are refined by docs 139–140.

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
- **Prerequisite:** the live editing path currently references `casual-doc-transaction` zero
  times and applies `casual-doc-edit` operations directly, so ADR-005 is not honoured in
  practice and there are two parallel operation sets. Unifying them (`107` §2.1) precedes any
  OT work, and is debt owed regardless.
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
5. **An explicit Save always creates a version** (`docs/139` §18 question 3). It is a point a
   person recognises, and content addressing makes a no-change Save cost a row rather than a
   document.
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
- whether layout uses fixed-point units internally.
