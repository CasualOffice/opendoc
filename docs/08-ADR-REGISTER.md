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
export-only `application.pdf` adapter, so a Rust host reaches it through the same
registry seam as DOCX and ODT; the browser host is not wired to it yet, and doc 98 lists
exactly what that wiring is. The Phase-2 accessibility/archival gate remains a separate
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
`107-COLLABORATION-OT-SNAPSHOT-REPLAY-DESIGN.md`.

**Decision:** Collaborative editing uses **operational transformation**, carried by
`casual-doc-transaction` transactions over the closed `casual-doc-edit` operation set, with a
durable ordered operation log. **Versioning is snapshot-plus-replay** over that log: a
snapshot (`25`) plus the operations after it reconstitutes any revision. Operations are
transformed only against a **totally ordered** log supplied by an optional relay.

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

## Pending ADRs

- shaping stack: HarfBuzz wrapper versus platform-native shaping;
- native renderer: Skia, Vello, tiny-skia, wgpu custom, or hybrid;
- internal text storage: rope, piece tree, or chunked sequence;
- ~~collaboration operation model: OT vs CRDT~~ — superseded by **ADR-033** (proposed; owner decision 2026-09-15; see doc 107);
- ~~PDF generation backend~~ — superseded by **ADR-031** (Phase 0 accepted and implemented; see doc 98);
- schema format: canonical CBOR encoding profile and golden vectors;
- plugin ABI stability;
- whether layout uses fixed-point units internally.
