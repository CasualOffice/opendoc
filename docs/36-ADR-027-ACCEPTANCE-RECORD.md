# ADR-027 Acceptance Record

**Status:** Accepted — 2026-07-24 (repository owner)
**Decision:** ADR-027 (see `08-ADR-REGISTER.md` and
`34-OOXML-FIDELITY-ARCHITECTURE.md`)
**Tracker:** P1A-004
**Purpose:** Make ADR-027 auditable by resolving each named Acceptance Decision
to a chosen option, so that "accept the fidelity architecture" is a checkable
event rather than a deferral to ten unstated sub-decisions.

## How to use this record

Doc 34 lists ten Acceptance Decisions that "must be explicitly approved before
implementation." This record turns each into a numbered entry with a **chosen
option** and a **state** — doc 34's ten become D1, D2, and D4–D11, and D3 (the
disposition taxonomy) is inserted from the reconciliation, for eleven decisions
in total. ADR-027 cannot be marked accepted while any decision below is
`Pending`.

State values: `Pending` (no option chosen), `Proposed` (an option is
recommended here, awaiting owner sign-off), `Accepted` (owner-signed).

Acceptance of ADR-027 is **architecture-level only**. It does not green-light
importer code, which remains gated on the accepted normalized schema v1 and the
five artifact schemas (see `32-PHASE-1A-SEMANTIC-DOCX-IMPORT-DESIGN.md`).

## Decisions

### D1 — Normalized model is the runtime source of truth
**State:** Accepted.
**Option:** The normalized OpenDoc model remains the live editing and layout
source of truth; source artifacts are auxiliary. (Doc 34 core decision.)

### D2 — `ImportBundle` dual-representation direction
**State:** Accepted.
**Option:** Import produces one atomic bundle of model + source snapshot +
provenance + preservation ledger + compatibility report + profile + registry
version, sharing one import identity.

### D3 — Disposition taxonomy
**State:** Accepted.
**Option:** Adopt the dual-axis taxonomy in `35-DISPOSITION-TAXONOMY.md`
(model outcome × retention outcome), replacing the three divergent single-value
enums. Blocking: report and ledger schemas cannot be authored until this is
fixed.

### D4 — Source-package retention modes and byte ceilings
**State:** Accepted (competitive research, high confidence).
**Option:** Three coupled choices. (1) **Modes:** Phase 1A ships `Semantic` and
`Retention`. `Inspect` (original admitted bytes) is deferred behind explicit
host authorization under the separate stricter original-bytes ceiling in doc 34
§1; never on by default. (2) **Ceilings:** reuse `21-PARSER-LIMITS.md` rather
than inventing new ones — `Retention` ledger budget = preserved-unknown-bytes
64 MiB secure-default / 256 MiB hard; `Inspect` original-bytes budget = input
package 256 MiB / 1 GiB — enforced per-part **and** aggregate on expanded size,
wired into the preservation-ledger writer (not only the admitter). (3)
**Overflow:** fail-closed with a typed error (`ODC-1003`); no silent downgrade.
An opt-in `Retention`→`Semantic` graceful downgrade is deferred, not default.
Competitive basis (see `37-PHASE-1A-DECISION-RESEARCH.md`): the dominant pattern
is bounded retention with fail-closed refusal — Word hard-bounds (512 MB file /
32 MB text) and refuses over-limit files; ONLYOFFICE enforces uncompressed
`inputLimits` (DOCX 50 MB) and returns a size-limit refusal, never a silent
drop; LibreOffice's unbounded in-memory InteropGrabBag is the counter-example
this decision exists to avoid. Ceiling calibration against the real-producer
corpus is a follow-up (tracked with R4 fixtures).

### D5 — Provenance and preservation-ledger ownership and anchoring
**State:** Accepted (competitive research, high confidence; primary sources
verified).
**Option:** A two-tier anchor. Provenance is **never** anchored to mutable model
node IDs — the accepted split/join ops churn node IDs and the equal-mark
run-merge collapses N source `w:r` into one run, so a node ID is not a stable
anchor.

1. **Ownership — sidecar, not embedded.** Provenance and the preservation ledger
   live in the separately versioned `ImportBundle` source artifacts (D2, D10),
   not in model node storage. The model keeps no source spellings (per
   `32-…-IMPORT-DESIGN.md`); artifacts reference the model, never the reverse.
2. **Tier 1 — byte floor.** An immutable whole-part source snapshot of the
   original OOXML part(s), keyed by OPC part name (the D4 `Retention` snapshot).
   It survives arbitrary edits and gives an exact no-edit return, but cannot
   reconcile edited regions. This is the ONLYOFFICE `origin.docx` model
   (`CopyOOXOrigin(… "origin.docx" …)`, source-verified this pass).
3. **Tier 2 — edit-tolerant ledger.** The anchor is a content-relative
   structural coordinate captured at import **before** run normalization:
   (source part; document-order structural path to the owning block; a
   character-offset span `(start, len)` over the owning paragraph's normalized
   text), **plus** the source `w:r` index — not a `w:r`/node ID. Source run
   boundaries and empty/property-only runs are recorded as offset spans over the
   normalized text, so the equal-mark run-merge does not destroy them: boundaries
   become sub-paragraph offset marks recomputed from text, independent of how
   many run nodes the model keeps. This resolves R2.
4. **Split/join invalidation.** Offset spans are remapped through the **same**
   position-mapping the edit layer already defines for split/join
   (`24-TRANSACTION-SEMANTICS.md` position-mapping table; `26-SELECTION-…`), not
   a bespoke rule: an edit strictly outside a span re-scopes the span; an edit
   that crosses a span boundary marks the entry `stale`/best-effort and, per D7,
   may trip its save-blocking condition. No competitor preserves content a user
   has edited through, so best-effort-on-cross is the honest convergent ceiling —
   it must never report `preserved` (per `35-DISPOSITION-TAXONOMY.md`) for a
   crossed span.
5. **Determinism.** Structural paths and offset spans derive from input document
   order and normalized text only, independent of ZIP/relationship enumeration
   order (ties into R3); the provenance golden is asserted byte-identical across
   reordered-ZIP and native/WASM twins.
6. **Offset unit — grapheme (accepted).** All Tier-2 character-offset spans are
   measured in **extended grapheme clusters** over the paragraph's normalized
   text, consistent with ADR-014 (grapheme-boundary runtime positions) and the
   existing `Position.grapheme_offset`. This makes provenance spans share the
   model's position space, so split/join remapping reuses the transaction layer
   directly and offsets are identical across native and WASM. Schema v1 pins the
   grapheme unit before importer implementation.

Competitive basis (see `37-PHASE-1A-DECISION-RESEARCH.md` D5): convergent — all
three engines preserve source data beside a normalized model, none guarantees a
byte-identical **edited** round-trip, and they diverge on anchor granularity.
ONLYOFFICE snapshots the whole package (`origin.docx`, decoupled from node
identity — survives any edit but cannot merge an edited node back). LibreOffice's
`InteropGrabBag` rides the semantic node at eight scopes
(`InteropGrabBag`/`Para…`/`Char…`/`Style…`/`Table…`/`Row…`/`Cell…`/`Frame…`,
source-verified), so it is directly exposed to the cited failure mode: one
`CharInteropGrabBag` slot for all runs a merge collapses, and split/join carries
the bag on only one side. MS Word / ECMA-376 Part 3 (ISO/IEC 29500-3) MCE
(`mc:Ignorable`, `mc:AlternateContent`, `mc:PreserveElements/Attributes`) is
positional and hint-based — anchored to XML-tree position relative to a
surrounding understood element, with no cross-edit stable identifier, and lossy
on save. OpenDoc therefore takes the whole-part snapshot for Tier 1 and moves the
Tier-2 anchor off node identity onto content-relative offset spans — keeping
LibreOffice's scoped/typed idea while avoiding its single-node-survivor loss.
This determines artifact golden bytes; these questions were previously mis-filed
as non-blocking. The edit-invalidation promise (what `stale`/best-effort means
for the fidelity claim) was the product-owner call at issue; it is **accepted**
at the best-effort-on-cross ceiling, and a crossed span is never reported
`preserved`.

### D6 — Mapping registry owns import and reverse mapping
**State:** Accepted.
**Option:** One versioned registry owns import + future export knowledge per
feature; supersedes generic extension bags for OOXML preservation (see ADR-007
amendment note in `08-ADR-REGISTER.md`).

### D7 — Preservation invalidation and save-block semantics
**State:** Accepted.
**Option:** Every mapping rule declares dirty scope, conflict policy, and a
save-blocking condition; generic "model wins"/"source wins" is insufficient.
Phase 1A records these; Phase 2 enforces them.

### D8 — Compatibility profile baseline
**State:** Accepted (competitive research, high confidence).
**Option:** The first profile **accepts both conformance classes on import** —
ISO/IEC 29500 Strict (`purl.oclc.org/ooxml/*`) and ECMA/Transitional
(`schemas.openxmlformats.org/*/2006/*`). Strict input is normalized to the
Transitional namespace family at decode via a fixed, total, deterministic
mapping table (wordprocessingml, drawingml, officeDocument relationships,
content-type strings, `.rels` relationship-type strings), so the mapping
registry only ever sees a single Transitional token set. Any `purl.oclc.org/*`
URI absent from the table is dispositioned (reported), never silently coerced —
else it is silent loss. Semantic **feature** support stays a narrow declared
subset (paragraphs, runs, basic properties, styles, numbering, sections, themes,
one media relationship); conformance-class acceptance is orthogonal to
feature-support level. The Strict/Transitional probe fixture asserts
byte-identical normalized output between the two twins. Phase 2 emit-default is
Transitional (forward-looking; not exercised in 1A). Competitive basis (see
`37-PHASE-1A-DECISION-RESEARCH.md`): Open XML SDK maps `purl.oclc.org` →
`schemas.openxmlformats.org` on load; LibreOffice `filterdetect.cxx`
(`OOXMLVariant::ISO_Strict`) normalizes Strict→Transitional and emits
Transitional; ONLYOFFICE accepts Strict schema and saves Transitional; Word has
never defaulted to Strict.

### D9 — Treatment of macros, signatures, embedded objects, custom XML, external relationships
**State:** Accepted (competitive research, high confidence; the three retention
sub-dispositions are accepted at the security-conservative defaults below).
**Option:** A security-conservative, per-class registry policy. Opened documents
are untrusted data: no code execution, no document-triggered network I/O,
bounded resource consumption, and integrity artifacts never survive edits.
Anti-zip-bomb and nested-package recursion limits are enforced **before** any
part is retained (reuse `21-PARSER-LIMITS.md` inflate-size and expansion-ratio
limits; the embedded-package recursion-depth guard is a new limit, see risks).
Disposition per `35-DISPOSITION-TAXONOMY.md`, per class:

1. **VBA macros (`vbaProject.bin`, [MS-OVBA]).** Never parse-to-execute, never
   auto-run. Retained verbatim as an opaque, byte-bounded ledger entry with
   security classification `inert-preserve` and a save disposition requiring
   explicit host trust to re-emit; MS-OVBA is never interpreted to run code.
   `omitted` + `preserved` within the Preserved-unknown-bytes ceiling, else
   `blocked`.
2. **Digital signatures (OPC signature parts, [MS-OFFCRYPTO]).**
   Report-and-invalidate; never `preserved`. A signature hash covers bytes, so
   the normalized-model round-trip mathematically cannot keep it valid; the
   compatibility report records that a signature was present and is invalidated,
   and a stale signature is never presented as valid. `omitted` +
   `not-retained` (reported).
3. **OLE / embedded objects & embedded OPC packages.** Preserve opaque, never
   activate. Enforce max inflated size, max expansion ratio, and a nested-package
   recursion-depth guard before retention. Within limits → `omitted` +
   `preserved`; refused by policy → `omitted` + `blocked`; structurally invalid
   or over-limit → `omitted` + `rejected` (typed error, never silent truncation).
4. **customXml / content-control data binding.** Custom XML data-storage parts
   that back content-control data binding are legitimate ECMA-376 parts,
   unaffected by the i4i judgment (which bound Microsoft's specific markup
   feature, not the standard) — preserved opaque, `omitted` + `preserved`, never
   treated as trusted or executable. Free-floating custom-XML markup ("pink
   tags") is dropped/fenced with a report (`omitted` + `not-retained`), following
   the conservative post-KB974631 Word behavior.
5. **External relationships (`TargetMode=External`).** Never fetch, resolve, or
   dereference at import or edit time — no network I/O is ever triggered by
   opening a document (defends against remote-template injection, DDE, and
   tracking-callback leaks). The relationship record is retained as opaque
   metadata and reported via `ODC-1005`; any later user-initiated activation is
   gated behind explicit host consent.

Each class is a registry policy entry with an explicit save disposition before
any retention or export; none is loaded as live executable or active content.
Competitive basis (see `37-PHASE-1A-DECISION-RESEARCH.md`, D9): convergent
security-conservative behavior across all three engines — LibreOffice preserves
but comments-out VBA and disables macros by default (help.libreoffice.org
vbasupport; CVE-2019-9853 treated macro-on-load as a bug), ONLYOFFICE never runs
VBA and does not safely round-trip it (DocumentServer #3466), and Word runs VBA
only under Trust Center + Mark-of-the-Web + Protected View; Microsoft documents
that any byte change breaks the signed hash (Trust Bar "contains invalid
signatures") and LibreOffice warns modified-after-signing; the spec does not
bound nested-package decompression, so implementers must impose inflate/ratio/
depth limits (USENIX WOOT'20; zip-bomb 1023:1); the i4i judgment removed Word's
pink-tag markup on open but explicitly did NOT affect content controls or their
XML data binding, and "the Open XML standards are not affected"; and OPC defines
external addressing, not a mandate to dereference. The three retention
sub-dispositions that are product/legal judgments are **accepted** at these
security-conservative defaults: (a) macro bytes retained **inert** within the
Preserved-unknown-bytes ceiling (never executed); (b) customXml content-control
data-binding parts **preserved opaque**, free "pink-tag" markup fenced and
reported; (c) invalidated-signature bytes are **discarded** by default
(`not-retained`), retained only under the explicit host-authorized `Inspect`
mode. The security floor (no execute, no fetch, invalidate-on-edit, bounded
before retention) is invariant across all modes.

### D10 — Artifact versioning and public SDK exposure
**State:** Accepted.
**Option:** Each source artifact carries its **own** independent version line —
source snapshot, provenance map, preservation ledger, compatibility report, and
mapping registry version independently, plus the normalized schema v1 version —
so any one can evolve without forcing a lockstep bump. In Phase 1A all artifacts
except the compatibility report are **internal** to the `ImportBundle`; the SDK
exposes only the normalized snapshot and the compatibility report. A bounded,
host-authorized inspection view over the other artifacts is deferred to a later
SDK design (couples to the `Inspect` mode gate in D4).

### D11 — Fidelity claim vocabulary
**State:** Accepted.
**Option:** Adopt the eight fidelity dimensions in doc 34 and prohibit
"lossless" unless dimension, feature set, producer profile, edit class, and
corpus are named.

## Substrate reconciliations

These are shipped-code contradictions resolved as part of acceptance. R1, R2, and
R4 are resolved (below); their code changes are tracked as implementation tasks.
R3 is an open implementation task and does not block acceptance.

- **R1 — package admitter part-set. Resolved (resolve-now).** Relax the
  admitter to hard-require only `[Content_Types].xml` + `_rels/.rels` — the two
  names OPC actually fixes. Main-document identification moves to a
  post-admission discovery step that parses `_rels/.rels`, selects the
  relationship whose `Type` is the `officeDocument` type in either the
  transitional
  (`http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument`)
  or strict (`http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument`)
  namespace, resolves its `Target` relative to the package root (rejecting any
  escape beyond root), and verifies the target part exists and carries the
  WordprocessingML main-document content type. Discovery is fail-closed: missing,
  duplicate, broken, unresolvable, or wrong-content-type is a typed error
  (`MissingMainDocument` / `AmbiguousMainDocument`), never silent acceptance.
  `word/document.xml` is retained only as an optional fast-path heuristic, never
  an admission gate. Competitive basis (see `37-PHASE-1A-DECISION-RESEARCH.md`):
  OPC fixes only these two names; LibreOffice
  `getFragmentPathFromFirstTypeFromOfficeDoc` (source-verified), Open XML SDK
  `MainDocumentPart.RelationshipTypeConstant`, and ONLYOFFICE
  `CRels(/_rels/.rels)` root read all discover by relationship type. Follow-ups:
  amend `28-DOCX-PACKAGE-READER.md` required-parts; add non-conventional-path
  and Strict-namespace fixtures to `23-DOCX-FIXTURE-CORPUS.md`; the
  relationship-selection tie-break and reordered-ZIP determinism are tracked
  with R3. Code follow-up: `crates/casual-doc-ooxml/src/lib.rs:260` drops
  `DOCUMENT_PART` from the required set (demoted to a fast-path constant).
- **R2 — run-merge vs provenance granularity. Resolved (resolve-now) via D5.**
  The model's mandatory equal-mark run merge collapses N source `w:r` into one
  run; provenance/ledger anchors capture source run boundaries **before** the
  merge as character-offset spans `(start, len)` over the paragraph's normalized
  text plus the source `w:r` index (D5 tier 2), so the collapse cannot destroy
  them. Empty/property-only runs are recorded as zero-length or property-anchored
  spans at the same offsets.
- **R3 — deterministic import-namespace/documentId seed.** Derivation must be
  input-derived and independent of ZIP entry and relationship enumeration order,
  with a reordered-ZIP + native/WASM golden asserting ID and provenance
  byte-identity.
- **R4 — table / nested-container disposition. Resolved (resolve-now):
  hybrid flatten-then-preserve.** Every paragraph inside a table cell (`w:tc`),
  text box (`w:txbxContent`), and block-level SDT (`w:sdtContent`) is
  materialized as a first-class `BlockNode::Paragraph` appended to the flat body
  in document order — no new `BlockNode` variant in schema v1, so zero cell text
  is lost. The container itself is not placed in the body and is not stored
  opaquely; instead each table/box/SDT becomes exactly one typed
  preservation-ledger entry recording its geometry (`w:tblGrid`, rows, cell
  spans/merges, borders), unsupported properties, nesting depth, and parent,
  anchored to the contiguous body paragraph IDs it produced. Nesting recursion is
  capped by the streaming-parser depth limit; over-depth containers are
  dispositioned `omitted` + `rejected` (never silently truncated). Competitive
  basis (see `37-PHASE-1A-DECISION-RESEARCH.md`): ECMA-376 shares
  `EG_BlockLevelElts` across body/`w:tc`/`w:txbxContent`/`w:sdtContent` and text
  exists only in `w:p`/`w:r` — so flattening loses geometry, not text, while
  rejecting loses all cell text (and would make the real-producer fixture golden
  wrong); Word `Range.Text` includes cell text, ONLYOFFICE `CTableCell` holds a
  `CDocumentContent`, and LibreOffice writerfilter models cells as paragraph
  containers. Constraints: table-structural edits (insert row, merge cell) are
  NOT expressible in Phase 1A (deferred to Phase 1B+); every body traversal,
  extraction, selection, and command path MUST explicitly recurse cell/box/SDT
  paragraphs or their text silently disappears (the python-docx
  `document.paragraphs` footgun). Depends on D5 for stable anchor identity.

## Sign-off

**ADR-027 is Accepted.** All eleven decisions (D1–D11) are `Accepted` and R1, R2,
and R4 have recorded resolutions; R3 (deterministic import-namespace/documentId
seed) remains an open implementation task tracked in `14-EXECUTION-TRACKER.md`
and does not block architecture acceptance. Docs 32, 34, and 35 are flipped to
Accepted, the ADR register (`08-ADR-REGISTER.md`) records ADR-027 as accepted,
and the accepted doc-28 amendment (R1) is effective.

Acceptance is architecture-level. Importer code is unblocked to begin the
schema-v1 slice and the R1 read-path slice; it is **not** a green-light to skip
the schema-v1 and artifact-schema deliverables.

| Role | Name | Date |
| --- | --- | --- |
| Design owner | Sachin sarwa (repository owner) | 2026-07-24 |
| Reviewer | Sachin sarwa (repository owner) | 2026-07-24 |
