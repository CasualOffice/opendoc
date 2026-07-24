# Phase 1A Decision Research: MS Word, ONLYOFFICE, LibreOffice

**Status:** Proposed; not accepted
**Tracker:** P1A-004
**Purpose:** Ground the open Phase 1A acceptance decisions
(`36-ADR-027-ACCEPTANCE-RECORD.md`) in how the three dominant OOXML engines —
Microsoft Word / Open XML SDK, ONLYOFFICE, and LibreOffice — actually behave.
Builds on the source-architecture study in
`33-DOCX-ENGINE-COMPETITOR-RESEARCH.md`.

Each finding drives a resolution recorded against its decision in doc 36. Where
a claim could not be verified against a primary source it is tagged
`[model-knowledge, unverified]` and does not carry a load-bearing decision.

---

## R1 — Main-document discovery (relationship vs conventional path)

**Question.** Does the OOXML ecosystem require the main document at
`word/document.xml`, or discover it by relationship?

**Finding — convergent, high confidence.** All three engines locate the main
part by following the `officeDocument` package relationship in `_rels/.rels`,
not by assuming a path. OPC fixes only two well-known names: `[Content_Types].xml`
and the `_rels/.rels` convention; the start/main part is discovered via the
package relationship and bound by content type, so `word/document.xml` is purely
a producer convention.

- **LibreOffice** — `oox/source/core/relations.cxx`
  `Relations::getFragmentPathFromFirstTypeFromOfficeDoc` tries the transitional
  `officeDocument` type, falls back to the strict `purl.oclc.org` type, and
  derives the path from the relationship `Target` (never hardcoded).
  *(source-verified)*
- **MS / Open XML SDK** — the main part is located by
  `MainDocumentPart.RelationshipTypeConstant` (the `officeDocument` relationship
  type), not a fixed path.
- **ONLYOFFICE** — `OOXML/DocxFormat/Docx.cpp` reads `OOX::CRels(/_rels/.rels)`
  at the package root first and is relationship/content-type driven. *(root read
  confirmed; precise dispatch not load-bearing)*

**Resolution.** R1 = resolve-now: relax the admitter to require only
`[Content_Types].xml` + `_rels/.rels`; discover the main document post-admission
by relationship type (transitional **and** strict namespaces), fail-closed. See
doc 36 R1.

**Sources.**
- ECMA-376 Part 2 / ISO/IEC 29500-2 (OPC); MS-OI29500 implementer notes
  (learn.microsoft.com/openspecs/office_standards/ms-oi29500).
- learn.microsoft.com Open Packaging Conventions overview (package relationships
  identify the start part; `[Content_Types].xml` and `_rels/.rels` fixed names).
- LibreOffice `oox/source/core/relations.cxx`
  (docs.libreoffice.org relations_8cxx_source.html).
- Open XML SDK `WordprocessingDocument`
  (github.com/OfficeDev/Open-XML-SDK … `MainDocumentPart.RelationshipTypeConstant`).
- ONLYOFFICE `OOXML/DocxFormat/Docx.cpp`
  (raw.githubusercontent.com/ONLYOFFICE/core … `CRels(/_rels/.rels)`).
- `[model-knowledge, unverified]` MS Word always writes the main part at
  `word/document.xml` (supports the optional fast-path only).

---

## R4 — Tables and nested containers

**Question.** How to disposition tables and paragraphs nested in table cells,
text boxes, and SDTs, given the flat `BlockNode::Paragraph`-only body?

**Finding — convergent, high confidence.** Neither pure option is right; every
reference engine treats cell/box/SDT text as first-class and geometry as
separable. ECMA-376 shares the same `EG_BlockLevelElts` grammar across the body,
`w:tc`, `w:txbxContent`, and `w:sdtContent`, and **text exists only inside
`w:p`/`w:r`** — so flattening containers loses only geometry, while dropping a
container loses all its text.

- **MS Word / Open XML SDK** models `Table > TableRow > TableCell` recursively
  with the paragraph as the universal leaf; a cell is a `Range` and
  `Range.Text` includes cell text — never opaque.
- **ONLYOFFICE** — `CTableCell` holds a `CDocumentContent` (the body's own
  recursive, editable content container); round-trip of unknowns is handled at
  the package level (`origin.docx` side-channel), not by dropping cells.
- **LibreOffice** — writerfilter `DomainMapper`/`TableManager` builds cells as
  paragraph containers (`endTable`/`resolveCurrentTable`/`GetTopTextAppend`).
- **Cautionary** — python-docx `document.paragraphs` excludes cell paragraphs
  (issues #650/#276), a reproducible flat-body footgun; pandoc drops
  `txbxContent` inside `mc:AlternateContent` entirely (issue #5394).

**Resolution.** R4 = resolve-now: hybrid flatten-then-preserve — cell/box/SDT
paragraphs become first-class body paragraphs in document order; the container
becomes one typed ledger entry holding geometry, anchored to those paragraph
IDs; depth capped, over-depth `omitted`+`rejected`. See doc 36 R4.

**Sources.**
- ECMA-376 WordML tables content model
  (webapp.docx4java.org/OnlineDemo/ecma376/WordML/Tables.html, fetched);
  `EG_BlockLevelElts` shared model `[model-knowledge]`.
- MS Learn: `TableCellProperties`; `SdtContentBlock`; `SdtBlock`.
- datypic OOXML: `w_txbxContent`, `w_sdt-2`, `w_sdtContent-1`; c-rex Part 4
  `sdtContent`.
- LibreOffice writerfilter `TableManagerState`, `DomainMapper_Impl.cxx`,
  `DomainMapper.cxx` (docs.libreoffice.org).
- ONLYOFFICE `CTableCell → CDocumentContent`
  (deepwiki.com/ONLYOFFICE/sdkjs 4.2 tables; community.onlyoffice.com/t/1659).
- python-docx issues #650/#276; python-mammoth (cell text first-class, borders
  ignored); pandoc issue #5394.

---

## D8 — Conformance profile (Strict vs Transitional)

**Question.** What conformance class should the first profile target?

**Finding — convergent, high confidence.** All three engines accept Strict on
import by normalizing it to the Transitional namespace family, and all emit
Transitional by default. Strict and Transitional differ by namespace family
(`purl.oclc.org/ooxml/*` vs `schemas.openxmlformats.org/*/2006/*`); Strict is a
subset of Transitional.

- **Open XML SDK** reads ISO Strict by mapping `purl.oclc.org` namespaces onto
  `schemas.openxmlformats.org` on load.
- **LibreOffice** detects both families in `oox/source/core/filterdetect.cxx`
  (`OOXMLVariant::ISO_Strict`), normalizes Strict→Transitional in
  writerfilter/DomainMapper, and has never shipped complete Strict export.
- **ONLYOFFICE** references the `purl.oclc.org` strict namespaces in its schema
  resources (accepts Strict) and defaults to Transitional on save.
- **MS Word** "has never produced Strict OOXML by default" (TDF, June 2026);
  Save As default remains "Word Document" (Transitional).

**Resolution.** D8 = Proposed: accept both classes on import; normalize
Strict→Transitional at decode via a fixed total deterministic table; unmapped
strict URIs are reported, never coerced; feature support stays a narrow declared
subset. See doc 36 D8.

**Sources.**
- learn.microsoft.com Open XML SDK (Strict→Transitional namespace mapping on
  load).
- LibreOffice `oox/source/core/filterdetect.cxx` (`OOXMLVariant::ISO_Strict`).
- The Document Foundation blog, 2026-06-02, "A standard in name only" (Word never
  defaults to Strict).
- ISO/IEC 29500-1:2016 (Strict) and Part 4 (Transitional migration features)
  (iso.org 71691 / 71692).
- ONLYOFFICE core DocxFormatCodeGen `wml.xsd` (references strict namespaces).
- LOC FDD DOCX Transitional (fdd000397); mmohrhard.wordpress.com 2014 LibreOffice
  strict-import note `[model-knowledge corroboration]`.

---

## D4 — Retention bounds and overflow policy

**Question.** How should preserved/round-trip data be bounded, and what happens
on overflow?

**Finding — convergent on bounded, fail-closed, high confidence.** The dominant
real-world pattern is bounded retention enforced on expanded size with
fail-closed refusal.

- **MS Word** imposes hard explicit bounds (512 MB file, 32 MB document text)
  and refuses to open over-limit files with a user-visible error rather than
  degrading. Its markup-compatibility default (`ProcessMode=NoProcess`)
  preserves all markup verbatim (bounded by the file limit).
- **ONLYOFFICE Document Server** — the closest bounded-store analogue — enforces
  per-format **uncompressed** `inputLimits` (DOCX 50 MB, XLSX 300 MB) plus an
  upload cap and returns "file size exceeds the limitation", never a silent drop.
- **LibreOffice** is the counter-example: `InteropGrabBag` preservation is
  effectively unbounded and in-memory, so its only bound is implicit
  (large-document crashes). This is the behavior OpenDoc's bounded ledger exists
  to avoid.

**Resolution.** D4 = Proposed: ship `Semantic` + `Retention` (defer `Inspect`
behind host auth); reuse doc 21 ceilings (Retention 64 MiB/256 MiB, Inspect
256 MiB/1 GiB) enforced per-part and aggregate on expanded size; fail-closed
with `ODC-1003` on overflow; opt-in downgrade deferred. See doc 36 D4.

**Sources.**
- learn.microsoft.com markup-compatibility (`MarkupCompatibilityProcessSettings`,
  default `NoProcess`; ECMA-376 Part 3 §3.13, fetched).
- learn.microsoft.com Word operating-parameter limits (32 MB text / 512 MB file);
  Word "file larger than 512 megabytes" refusal.
- ONLYOFFICE `server/Common/config/default.json` (`FileConverter.inputLimits`,
  fetched); issues #770 (~100 MB hard limit), #1138 (size-limit refusal).
- LibreOffice OOXML-interoperability talk (grab-bag preservation of unknowns);
  writerfilter `DomainMapper.cxx` / `SavedAlternateState` (InteropGrabBag);
  ask.libreoffice.org large-document crash report.
- Zip-bomb decompression-ratio bound (en.wikipedia.org/wiki/Zip_bomb); USENIX
  WOOT'20 "Office Document Security and Privacy".

---

## D5 — Provenance / round-trip anchoring

**Question.** What anchor identity for provenance/preservation survives the
accepted split/join edit ops (which churn node IDs) and the equal-mark run-merge
(which collapses N `w:r` into one), and how is source run granularity kept?

**Finding — convergent, high confidence (primary sources verified).** All three
engines preserve source data *beside* a normalized model, none guarantees a
byte-identical **edited** round-trip, and they diverge on anchor granularity —
which is exactly why a node-ID anchor is unsafe.

- **ONLYOFFICE** snapshots the **whole package**: `X2tConverter/src/lib/docx.h`
  calls `CopyOOXOrigin(…, "origin.docx", …)` to duplicate the original DOCX
  intact beside the internal model (verified at commit `3250a848`). Decoupled
  from node identity — survives any edit, but cannot merge an edited node back.
  → OpenDoc **Tier 1** (whole-part snapshot).
- **LibreOffice** `InteropGrabBag` rides the semantic node at **eight scopes**
  (`InteropGrabBag` / `Para…` / `Char…` / `Style…` / `Table…` / `Row…` /
  `Cell…` / `Frame…`, all verified in `sw/inc/unoprnms.hxx` at commit
  `bdb27b21`). This is directly exposed to the cited failure mode: one
  `CharInteropGrabBag` slot for all runs a merge collapses, and split/join
  carries the bag on only one side. → OpenDoc keeps the scoped/typed idea but
  moves the anchor **off** node identity.
- **MS Word / ECMA-376 Part 3 (ISO/IEC 29500-3) MCE** (`mc:Ignorable`,
  `mc:AlternateContent`, `mc:PreserveElements`/`Attributes`) is positional and
  hint-based — anchored to XML-tree position relative to a surrounding understood
  element, with no cross-edit stable identifier, and lossy on save (only
  post-preprocessing markup is written).

**Resolution.** D5 = Proposed (owner sign-off): two-tier anchor — Tier 1 =
whole-part OPC-keyed immutable snapshot (byte floor); Tier 2 = content-relative
offset-span ledger anchored on (source part + document-order block path +
character-offset span over normalized paragraph text + source `w:r` index),
captured **before** normalization; split/join handled by reusing the existing
transaction-layer position mapping with a cross-boundary `stale`/best-effort rule
(never falsely `preserved`). Provenance lives in the `ImportBundle` sidecar,
never in model node storage. Resolves R2. See doc 36 D5.

**Sources.**
- learn.microsoft.com "Introduction to markup compatibility"; c-rex OOXML Part 5
  MCE `Ignorable` / `PreserveElements`; ECMA-376 Part 3.
- LibreOffice `sw/inc/unoprnms.hxx` @ `bdb27b21` (eight InteropGrabBag scopes,
  verified); FOSDEM 2014 InteropGrabBag talk; igalia interoperability blog;
  writerfilter `DomainMapper.cxx`.
- ONLYOFFICE `X2tConverter/src/lib/docx.h` @ `3250a848` (`CopyOOXOrigin`
  `origin.docx`, verified); api.onlyoffice.com saving-file docs.
- LOC FDD OOXML/MCE (fdd000396).

---

## D9 — Special parts (macros, signatures, OLE, custom XML, external rels)

**Question.** Per-class import/re-save policy for VBA macros, digital
signatures, OLE/embedded objects, custom XML, and external relationships.

**Finding — convergent security-conservative behavior, high confidence.** None of
the three engines executes untrusted content on open, none fetches external
targets without consent, and none can keep a signature valid across an edit.

- **VBA macros (`vbaProject.bin`, MS-OVBA).** LibreOffice preserves but
  comments-out VBA and disables macros by default (a macro-on-load path was
  treated as the security bug CVE-2019-9853); ONLYOFFICE never runs VBA and does
  not safely round-trip `vbaProject.bin` (DocumentServer #3466 corrupts the
  project); Word runs VBA only in `.docm` under Trust Center + Mark-of-the-Web +
  Protected View. → **retain inert, never execute.**
- **Digital signatures (MS-OFFCRYPTO).** Microsoft documents that any byte change
  — even recalculating to the same value — breaks the signed hash, surfaced as
  the Trust Bar "contains invalid signatures"; LibreOffice re-serializes on save
  and warns modified-after-signing. There is no edit-preserving signature. →
  **report-and-invalidate, never `preserved`.**
- **OLE / embedded packages.** OPC/ECMA-376 defines nesting/addressing but does
  **not** bound decompression or recursion (42.zip nested-ZIP, deflate 1023:1),
  so implementers must impose inflate-size, ratio, and depth limits (USENIX
  WOOT'20). → **preserve opaque, never activate, bounded before retention.**
- **customXml / content-control data binding.** The i4i judgment removed Word's
  specific "pink-tag" markup on open but explicitly did **not** affect content
  controls or their XML data binding, and Microsoft stated the Open XML standards
  themselves are unaffected. → **data-binding parts preserved opaque; free markup
  fenced/reported.**
- **External relationships (`TargetMode=External`).** OPC defines External as
  addressing, not a mandate to dereference; no engine fetches External targets on
  open without user consent (LibreOffice link-update prompts, Word remote-template
  / DDE gating). → **never fetched; opaque metadata; `ODC-1005`; consent-gated.**

**Resolution.** D9 = Proposed. The security floor (no execute, no fetch,
invalidate-on-edit, bounded-before-retention) resolves now; three retention
sub-dispositions are owner calls: (a) macro bytes inert-retain vs
drop-and-report; (b) i4i-era customXml preserve-opaque vs drop-fence; (c) whether
invalidated-signature bytes are kept for inspection. See doc 36 D9.

**Sources.**
- learn.microsoft.com i4i / custom-XML impact; "document contains invalid
  signatures"; digital-signatures-and-certificates.
- help.libreoffice.org VBA support; LibreOffice security advisories; CVE-2019-9853
  (aws/alas, The Register).
- ONLYOFFICE DocumentServer #3466; api.onlyoffice.com VBA-macro conversion guide.
- USENIX WOOT'20 "Office Document Security and Privacy"; zip-bomb (Wikipedia);
  Google Cloud threat-intel "Detecting embedded content in OOXML".
- MS-OVBA / MS-DOCX; docx4j `CustomXmlDataStoragePart`.

---

## D9 — Special parts (macros, signatures, OLE, custom XML, external rels)

*Pending re-research (in progress). The first research pass returned an invalid
placeholder for this decision; a focused re-run with primary citations
(VBA `vbaProject.bin` handling, signature invalidation on edit, embedded-package
recursion limits, custom-XML data binding, external-relationship non-fetch) is
under way. The current working resolution in doc 36 D9 is grounded in
established convergent behavior and remains `needs-owner-signoff`.*
