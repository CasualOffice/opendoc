# OOXML Fidelity Architecture

**Status:** Proposed; not accepted
**Candidate decision:** ADR-027
**Tracker:** P1A-003
**Research basis:** `33-DOCX-ENGINE-COMPETITOR-RESEARCH.md`
**Implementation state:** Blocked pending design acceptance

## Decision Required

OpenDoc must decide what information crosses the DOCX import boundary before
normalized schema v1 or the importer is implemented.

The proposed decision is:

> The normalized OpenDoc model remains the live editing and layout source of
> truth. DOCX import additionally produces an immutable, bounded source package
> snapshot, source-to-model provenance, and a typed preservation ledger.
> Import and future export rules are owned by one versioned mapping registry.

Semantic JSON is a deterministic diagnostic and interchange encoding of the
normalized model. It is not a replacement for OOXML, not the renderer input
contract, and not by itself a round-trip fidelity mechanism.

## Goals

- represent document meaning in an editor-friendly, host-independent model;
- preserve safe source information that the model cannot yet express;
- make every unsupported construct visible and dispositioned;
- prevent a future writer from guessing how imported semantics map back;
- support deterministic native, WASM, and headless behavior;
- keep package and XML parsing bounded for untrusted input;
- evolve feature support without turning the public model into an OOXML DOM.

## Non-goals

- byte-identical reserialization after an edit;
- using OOXML elements as public runtime nodes;
- implementing DOCX save in Phase 1A;
- promising visual equivalence before typography, pagination, and rendering;
- preserving executable, external, malformed, or over-limit content;
- retaining arbitrary source bytes without ownership and invalidation rules;
- treating a compatibility report as proof of successful preservation.

An exact no-op return of original admitted package bytes may be offered in a
future inspection mode. That is package retention, not a claim that OpenDoc can
reconstruct the package or safely merge arbitrary edits into it.

## Fidelity Vocabulary

Every compatibility claim must name the fidelity dimension it measures.

| Dimension | Meaning | Earliest owner |
| --- | --- | --- |
| Package fidelity | Parts, content types, relationships, names, and safe retained bytes | Phase 1A |
| Structural fidelity | Source order, hierarchy, alternates, namespaces, and ownership | Phase 1A |
| Semantic fidelity | Meaning represented by the normalized OpenDoc model | Phase 1A |
| Preservation fidelity | Unsupported safe content retained with a future disposition | Phase 1A design; Phase 2 validation |
| Edit fidelity | Supported edits invalidate and regenerate only intended source regions | Phase 2 |
| Visual fidelity | Lines, pages, geometry, paint, and fonts compare within declared tolerances | Phases 1B-1D |
| Behavioral fidelity | Fields, revisions, controls, macros, links, and producer behavior | Feature-specific |
| Diagnostic fidelity | Every unsupported, degraded, blocked, omitted, or rejected construct is reported | Phase 1A |

"Lossless" is prohibited in documentation and release claims unless the exact
dimension, feature set, producer profile, edit class, and test corpus are named.

## Proposed Import Architecture

```text
DOCX bytes
  -> bounded package admission
  -> immutable source package snapshot
  -> OPC graph and markup-compatibility processing
  -> source-shaped typed decoder events
  -> mapping registry
       -> normalized model builder
       -> provenance map
       -> typed preservation ledger
       -> compatibility report
  -> atomic ImportBundle
```

The package snapshot is created only after the existing package reader accepts
the archive. A failure in XML, relationship, semantic, preservation, or model
validation returns no usable document session.

### Future save architecture

Phase 1A defines this contract but does not implement it:

```text
normalized model + dirty-region map
source package snapshot + provenance
preservation ledger + mapping registry
target compatibility profile
  -> export planner
       -> copy safe unchanged part
       -> regenerate supported owned region
       -> merge valid preserved content
       -> omit with report
       -> block save
  -> package and XML validators
  -> DOCX bytes + export compatibility report
```

This prevents Phase 2 from treating export as a second, unrelated
model-to-XML project.

## Representation Boundaries

### 1. Source package snapshot

An immutable snapshot records admitted package facts required for inspection
and future save planning:

- original package bytes only when enabled and within a separate hard ceiling;
- canonical part names, content types, compression metadata, and hashes;
- internal and external relationships;
- retained safe part bytes under per-part and aggregate limits;
- producer and conformance observations when deterministically identifiable;
- rejected, blocked, omitted, or non-retained part dispositions.

The snapshot is not exposed as mutable model state. Macros, signatures,
external resources, encrypted content, malformed structures, and over-limit
payloads follow security policy; preservation never bypasses package admission.

### 2. Source-shaped decoder events

The decoder understands OPC, namespaces, markup compatibility, and the OOXML
structures required by registered rules. Events retain enough source location,
order, and lexical distinctions for mapping and preservation decisions.

This layer is internal. OpenDoc should not make a complete generated OOXML
object graph part of its public API or live editor state.

### 3. Normalized model

The model owns editor semantics:

- stable OpenDoc identities and positions;
- document structure and properties;
- commands, transactions, selection, history, and events;
- future layout and display-list inputs;
- deterministic schema-versioned snapshots.

Source relationship IDs, part paths, namespace prefixes, and equivalent OOXML
spellings do not become semantic identity.

### 4. Provenance map

Provenance connects normalized entities and properties to source regions
without making those locations public identity. A record includes:

- source part and stable structural path;
- source range or event span where available;
- mapping-rule ID and version;
- normalized owner and property path;
- inherited, synthesized, defaulted, or explicit origin;
- related preservation-entry IDs;
- source hash needed for conflict checks.

Provenance supports diagnostics, inspection, dirty-region calculation, and
future export planning. It must not contain document text in logs or public
error context.

### 5. Typed preservation ledger

Each retained unsupported or source-specific construct has an explicit record:

- stable preservation-entry ID;
- feature and mapping-rule ID;
- source part, structural location, and semantic owner;
- anchor relation: before, within, after, sibling, property, or whole part;
- source order and namespace context;
- typed payload or bounded canonical fragment;
- byte and node accounting;
- security classification;
- edit-invalidation scope;
- conflict and merge policy;
- planned save disposition;
- compatibility-report entry IDs.

The ledger is not an arbitrary key/value extension map. Entries with no
accepted owner, anchor, limits, invalidation rule, and export disposition are
rejected or omitted with diagnostics.

### 6. Compatibility report

The report says what OpenDoc understood and what it did with everything else.
It references semantic and preservation records but is not the retained
payload. A `preserved` disposition means a validated ledger or package-snapshot
record exists; it cannot mean "warning emitted."

### 7. Mapping registry

One versioned registry owns both import and future export knowledge. Every rule
defines:

- stable feature and rule IDs;
- OOXML vocabulary, profile, namespace, part, and context;
- source decoder input and precedence;
- normalized target and default/inheritance behavior;
- provenance requirements;
- preservation behavior for unconsumed source detail;
- reverse mapping and canonical output form;
- dirty and invalidation scope;
- conflict, omission, and save-blocking policy;
- security limits and external-resource policy;
- fixture families and required test oracles;
- implementation and acceptance status.

A feature is not "supported" merely because import can parse it. Support status
must distinguish decode, semantic mapping, edit, export, reopen, layout, render,
and behavior.

## Import Result

The internal concept is an atomic `ImportBundle`:

```text
ImportBundle
  model
  source_snapshot
  provenance
  preservation_ledger
  compatibility_report
  import_profile
  mapping_registry_version
```

The exact Rust and SDK types remain a Phase 1A API decision. The bundle's
components must share one import identity and cannot be combined across
documents or engine versions without explicit validation.

Potential operating modes:

| Mode | Retention | Intended use |
| --- | --- | --- |
| Semantic | Model, report, minimum provenance | Read-only extraction where save is impossible |
| Round-trip | Model, report, provenance, safe retained parts and ledger | Editing and future DOCX save |
| Inspect | Round-trip data plus original admitted bytes within stricter policy | Auditing and exact no-op return |

The public API must not silently downgrade a requested mode when a retention
limit is exceeded.

## Markup Compatibility

Markup compatibility processing is not equivalent to ignoring unknown
namespaces. The importer must:

- recognize the applicable conformance and namespace profile;
- process `mc:Ignorable`, `mc:ProcessContent`, `mc:PreserveElements`,
  `mc:PreserveAttributes`, and `mc:AlternateContent` under explicit rules;
- record selected and non-selected alternate branches when retention is safe;
- preserve namespace context required to serialize retained fragments;
- report branch selection and unsupported required semantics;
- reject ambiguity, malformed namespace use, or expansion beyond limits.

Rules must be based on ECMA-376 and applicable producer specifications, then
verified against rights-reviewed fixtures.

## Edit Invalidation and Save Planning

Every model mutation identifies dirty semantic owners and property paths. The
export planner uses those scopes to classify source regions:

- `unchanged-copy`: source part or region can be retained safely;
- `regenerate`: OpenDoc owns the supported region and writes canonical OOXML;
- `merge`: validated preservation entries can be reattached around regenerated
  content;
- `omit`: policy permits removal and the report records it;
- `block`: a safe, deterministic edited save cannot be produced.

Examples:

- changing bold should invalidate the corresponding run-property mapping, not
  every unrelated package part;
- deleting a paragraph invalidates preservation entries anchored inside it;
- editing a section property may require regeneration of its enclosing
  paragraph properties and relationship updates;
- editing through an unsupported structured object may block save rather than
  silently flatten or corrupt it.

Conflict resolution must be deterministic and feature-specific. Generic
"model wins" or "source wins" policies are insufficient.

## Compatibility Profiles

Import and export behavior is selected by a versioned profile containing:

- Strict, Transitional, and producer-extension handling;
- accepted and emitted namespace versions;
- feature support levels;
- security and preservation limits;
- canonicalization rules;
- warning escalation and save-blocking policy;
- target producer/version when a compatibility mode is offered.

Profiles affect behavior and therefore appear in deterministic snapshots,
reports, baselines, and cache keys.

## JSON Contract

Normalized JSON remains valuable for:

- deterministic semantic golden files;
- SDK and model diagnostics;
- schema migration tests;
- cross-platform comparisons;
- debugging without requiring a renderer.

It must not contain arbitrary package XML merely to claim fidelity. Source
snapshot, provenance, and preservation artifacts are independently versioned,
bounded, and access-controlled. A JSON snapshot cannot be used to promise that
the original DOCX can be recreated.

## Security Requirements

- Existing package admission succeeds before any retained data is trusted.
- XML is namespace-aware, depth-limited, count-limited, cancellable, and has no
  DTD, custom-entity, external-entity, or network resolution.
- Every source, model, provenance, preservation, diagnostic, and registry count
  has a secure default and a non-bypassable hard ceiling.
- Retained XML is data, never reparsed under weaker settings.
- External relationships are metadata only and are never fetched implicitly.
- Macros, OLE objects, ActiveX, signatures, custom XML, and embedded packages
  have explicit policy entries before retention or export.
- Reports and errors omit document text, credentials, host paths, and unbounded
  source fragments.
- Inspection APIs require explicit host authorization for retained raw bytes.

## Test Architecture

### Phase 1A gates

- package graph golden assertions;
- normalized semantic JSON;
- compatibility JSON;
- provenance and preservation-ledger snapshots;
- equivalent ZIP-order determinism;
- Strict, Transitional, namespace, and markup-compatibility cases;
- malformed and every-limit boundary;
- native, WASM, pinned-toolchain, and MSRV equivalence;
- fuzzing of package, XML, mapping, and preservation boundaries.

### Phase 2 writer gates

- generated part and relationship assertions;
- canonical XML assertions for owned regions;
- no-edit and targeted-edit package diffs;
- preservation reattachment and invalidation cases;
- save/reopen semantic comparison;
- explicit omit and save-block outcomes;
- Word and competitor reopen probes where licensing and automation permit.

### Phases 1B-1D gates

- fixed and versioned fonts;
- paragraph metrics and line fragmentation;
- page and section geometry;
- display-list assertions;
- image and pixel-difference baselines;
- caret and pointer-position agreement.

Test evidence must record fixture rights, source, generator, version, hash,
profile, engine revision, and expected fidelity dimensions.

## Alternatives Rejected

### Raw OOXML as the live model

Rejected because source syntax is poorly aligned with transactions, stable
positions, inheritance resolution, layout, and SDK evolution. It also exposes
producer-specific interchange details as runtime identity.

### Normalized semantic model only

Rejected because normalization discards source distinctions needed for
round-trip compatibility and cannot account for unsupported safe content.

### Generic extension maps

Rejected as the primary preservation design because they lack typed ownership,
ordering, invalidation, security, and reverse-mapping contracts. Existing v0
extension maps may remain for model-schema evolution but do not satisfy OOXML
preservation.

### Retain the original package only

Rejected as a complete strategy because a source copy does not explain how to
merge supported edits, relationship changes, or deleted semantic owners.

### Implement import first and design export later

Rejected because irreversible import normalization choices would become writer
constraints and create silent fidelity loss.

## Delivery Sequence

1. Accept or revise this architecture and ADR-027.
2. Define fidelity terminology and support-state vocabulary in the glossary and
   support matrix.
3. Specify mapping-registry schema and initial feature inventory.
4. Specify source snapshot, provenance, preservation-ledger, and report schemas
   with limits.
5. Define normalized schema v1 and deterministic v0 migration.
6. Select XML/namespace dependencies through a separate ADR and security review.
7. Implement package graph and markup-compatibility decoding.
8. Implement registered semantic slices with fixtures and all five Phase 1A
   artifacts.
9. Accept Phase 1A only after end-to-end evidence passes.
10. Implement writer planning and serialization in Phase 2 against the same
    registry.

## Acceptance Decisions

The following must be explicitly approved before implementation:

- normalized model remains the live source of truth;
- `ImportBundle` dual-representation direction;
- source-package retention modes and byte ceilings;
- provenance and preservation-ledger ownership;
- mapping registry owns import and reverse mapping;
- preservation invalidation and save-block semantics;
- compatibility profile baseline;
- treatment of macros, signatures, embedded objects, custom XML, and external
  relationships;
- artifact versioning and public SDK exposure;
- fidelity claim vocabulary.

## Open Questions

- Which original package bytes are retained by default in round-trip mode?
- Is provenance embedded in the internal model storage or held in a sidecar
  indexed by stable model IDs?
- What canonical structural-path format survives streaming decode without
  becoming a public contract?
- Which preserved fragments require lexical bytes versus canonical XML events?
- What is the first target profile: Transitional-only, Strict plus
  Transitional, or a narrower declared subset?
- Which unsupported owner edits block save in the first writer release?
- How are digital signatures reported and invalidated without implying they can
  be preserved after mutation?
- Which parts may be copied unchanged and which must always be regenerated?

## Candidate ADR-027

**Decision:** Use a normalized OpenDoc model as the runtime source of truth and
pair DOCX imports with a bounded immutable source snapshot, provenance map, and
typed preservation ledger. Own import and future export rules in one versioned
mapping registry.

**Why:** A WYSIWYG runtime needs editor-oriented semantics, while production
DOCX compatibility requires source distinctions and unsupported safe content
that normalization cannot represent. Designing both directions before import
prevents irreversible fidelity loss.

**Consequence:** Phase 1A produces more than semantic JSON and requires explicit
preservation limits, mapping rules, diagnostics, and future reverse mappings.
The Phase 2 writer must use the same registry and may block saves that cannot be
reconciled safely.
