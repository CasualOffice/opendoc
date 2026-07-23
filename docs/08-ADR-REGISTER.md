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

**Decision:** Build the package reader on exactly `zip` 7.0.0 with default
features disabled and only stored/Deflate DOCX input enabled.

**Why:** the current `zip` release exceeds the project's Rust 1.85 MSRV, while
7.0.0 supports Rust 1.83; encryption and unrelated codecs increase dependency
and attack surface without helping DOCX compatibility.

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

## Pending ADRs

- shaping stack: HarfBuzz wrapper versus platform-native shaping;
- native renderer: Skia, Vello, tiny-skia, wgpu custom, or hybrid;
- internal text storage: rope, piece tree, or chunked sequence;
- collaboration operation model;
- PDF generation backend;
- schema format: canonical CBOR encoding profile and golden vectors;
- plugin ABI stability;
- whether layout uses fixed-point units internally.
