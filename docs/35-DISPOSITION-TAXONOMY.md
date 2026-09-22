# Import Disposition Taxonomy

**Status:** Accepted — 2026-07-24 (decision D3, `36-ADR-027-ACCEPTANCE-RECORD.md`)
**Tracker:** P1A-004
**Applies to:** `38-SCHEMA-V1-DESIGN-REFERENCE.md` (import architecture),
`34-OOXML-FIDELITY-ARCHITECTURE.md`
**Reconciles:** the three divergent disposition enums previously stated in the
Phase 1A import design (now `38-…#import-architecture`) and doc 34.

## Why this document exists

Phase 1A previously described a construct's fate with a single mutually
exclusive value drawn from three different enumerations:

- doc 32 compatibility report: `preserved`, `degraded`, `omitted`, `blocked`,
  `rejected`;
- doc 34 diagnostic-fidelity row: `unsupported`, `degraded`, `blocked`,
  `omitted`, `rejected`;
- doc 34 source-snapshot dispositions: `rejected`, `blocked`, `omitted`,
  `non-retained`.

A single value per entry cannot express the outcome the fidelity architecture
exists to serve: a construct can be **partially mapped into the model** *and*
have its unconsumed remainder **either retained in the preservation ledger or
lost**. "Degraded, remainder preserved" and "degraded, remainder not retained"
are different fidelity facts, and a future writer depends on telling them apart.

This document defines one normative taxonomy on two orthogonal axes. Docs 32 and
34 reference this taxonomy instead of restating an enum.

## Two orthogonal axes

Every dispositioned construct carries exactly one value on **each** axis.

### Axis A — model outcome

What the normalized OpenDoc model captured from the construct.

| Value | Meaning |
| --- | --- |
| `mapped` | Fully represented by normalized model semantics; no meaning left unconsumed. |
| `degraded` | Partially represented; some source meaning was not carried into the model. |
| `omitted` | Not represented in the model at all. |

### Axis B — retention outcome

What happened to the source detail that the model did **not** consume
(the "unconsumed remainder"). For a `mapped` construct with no remainder, the
retention outcome is `not-applicable`.

| Value | Meaning |
| --- | --- |
| `preserved` | Unconsumed detail is retained in a validated preservation-ledger or source-snapshot record. |
| `not-retained` | Unconsumed detail was intentionally and reportably dropped within policy (no validated record). |
| `blocked` | Retention was refused by security or resource policy; nothing is trusted or stored. |
| `rejected` | The construct was structurally invalid or over-limit; it is neither modeled nor retained, and the failure is reported. |
| `not-applicable` | There is no unconsumed remainder (`mapped` with nothing left over). |

## Legal combinations

Not every pair is meaningful. The normative combinations are:

| Model outcome | Retention outcome | Fidelity meaning |
| --- | --- | --- |
| `mapped` | `not-applicable` | Fully understood; nothing left to retain. |
| `mapped` | `preserved` | Fully understood; incidental source detail also kept for exact save. |
| `degraded` | `preserved` | Partially understood; remainder kept in the ledger. |
| `degraded` | `not-retained` | Partially understood; remainder reportably dropped. |
| `degraded` | `blocked` | Partially understood; remainder refused by policy. |
| `omitted` | `preserved` | Not modeled, but retained verbatim for save/inspection. |
| `omitted` | `not-retained` | Not modeled and reportably dropped. |
| `omitted` | `blocked` | Not modeled; retention refused by policy. |
| `omitted` | `rejected` | Structurally invalid or over-limit; reported, not modeled, not retained. |

`rejected` and `blocked` are retention-axis outcomes only; the model outcome of
a refused or invalid construct is `omitted` (or `degraded` if some content was
mapped before the refusal). There is no `rejected` model outcome — Axis A is
exactly `mapped`, `degraded`, `omitted`. Any pairing not listed above is an
internal error and must fail import, not be reported.

The prohibited-silent-loss rule: `not-retained` is only legal when a
compatibility-report entry records the drop. A `preserved` retention outcome is
legal **only** when the report references a validated ledger or source-snapshot
record; emitting a warning without retaining the declared content is
`not-retained`, never `preserved`.

## What the report enumerates (and what it does not)

Added 2026-09-16 with the implementation, because the earlier wording was read
two ways and the two readings disagree about every healthy document.

Every traversed construct **has** a disposition on both axes. The report
**enumerates** only those whose model outcome is not `mapped`, plus any whose
retention outcome is a refusal (`blocked` or `rejected`). A construct the model
captures in full has the disposition `mapped` + `not-applicable`; that pair is the
report's implicit default and is carried without being listed.

The consequence is intended and load-bearing: **an ordinary document produces an
empty report.** A report that fires on every healthy file is one that every caller
learns to filter out, and a filtered report is worse than none — it converts a
competitive advantage (loss is detected and named, which a converter pipeline
cannot do) into noise. Diagnostic completeness is therefore a property of the
*taxonomy*, not a demand that every element appear in the output.

Two corollaries follow, and both are asserted by tests rather than left implied:

- `mapped` is **unreachable in a report** by construction. A mapped construct has
  no unrecovered meaning, so there is nothing to enumerate. `mapped` +
  `preserved` remains a legal disposition — it describes a construct fully
  understood whose incidental source detail is also kept — but it is not a
  finding, so it is not listed either.
- `not-applicable` is likewise unreachable in a report, since it pairs only with
  `mapped`.

## Non-content markup: what is outside the taxonomy

A disposition describes the fate of *document meaning*. Some markup carries none,
and dispositioning it would fill every report with rows a reader cannot act on.
The exclusions are enumerated here, not decided per parser, so that "not
reported" is a stated policy with a guard rather than an oversight.

| Markup | Why it is not dispositioned |
| --- | --- |
| Pure OPC plumbing (`[Content_Types].xml`, `_rels/*`) | Regenerated deterministically from the model. Not data. |
| `mc:Ignorable` (and the other markup-compatibility processing attributes) | A directive naming which namespace prefixes a consumer may ignore. It carries no document content, and a writer emits its own correct value. Plumbing, in the same sense as the content-type manifest. |

### The no-op class — markup that says a feature is *absent*

Added 2026-09-23 with the implementation (`109` HF-174). The importer had been
treating **unrecognised** as **lost**, and the two are not the same thing. A
sweep over the owner's fifteen-document corpus measured **701 findings that
described losses which had not happened**, on documents whose real findings
numbered in the tens — which is exactly how the report becomes something a
caller filters out, the outcome the section above exists to prevent.

The rule is not "these tags are boring". It is: **an element whose value equals
the state the model already has carries no meaning to lose.** Where that depends
on the value, so does the arm. The class is `mapped` + `not-applicable`
(`MappedComplete`), and by the rule above that disposition is not enumerated.

| Markup | Condition | Why nothing is lost |
| --- | --- | --- |
| `w:proofErr` | always | Where Word's spell/grammar checker last drew a squiggle. A cache of a check, not a statement about the document. |
| `w:lastRenderedPageBreak` | always | Where Word's pagination last fell. This engine paginates for itself, and the cached value is wrong the moment a font or margin differs. |
| `w:nsid`, `w:tmpl` | always | Word's list-gallery identity for an abstract numbering definition. Nothing in the package joins on them; numbering resolves through `w:abstractNumId`/`w:numId`. |
| `wp14:sizeRelH`, `wp14:sizeRelV` | always | A wrapper carrying only `@relativeFrom`; the percentage inside decides whether relative sizing is on. |
| `wp14:pctWidth`, `wp14:pctHeight` | value is `0` | Zero means relative sizing is **off**, and Word writes the element anyway. A non-zero percentage is a size the model (which sizes in EMU) does not carry, and it is reported. |
| `wps:cNvSpPr`, `wps:cNvCnPr`, `wpg:cNvGrpSpPr`, `wps:txbx` | always | Non-visual property wrappers and the text-box wrapper, beside `pic:cNvPr`/`pic:cNvPicPr` which were already excluded. Their children (`a:spLocks`, `w:txbxContent`) are judged, or imported, on their own. |
| `a:effectLst` | self-closing | An empty effect list is DrawingML for "no effects". A populated one is a lost shadow or glow, and is reported. |
| `a:spLocks` | no attributes | Locks nothing. A lock that locks something is a restriction the document asked for and did not get, and is reported. |
| `a14:useLocalDpi` | `val` is false | Asks that a picture *not* be rescaled to the authoring DPI, which is what this engine does. |
| `a:prstTxWarp` | `prst="textNoShape"` | DrawingML's token for *no* warp; Word writes it into every `wps:bodyPr`. ONLYOFFICE special-cases the same token in four places. A real preset is unmodeled and is reported. |
| `w:clrSchemeMapping` | the default mapping | The permutation every consumer already assumes when the element is absent. A swapped slot inverts real colours and is reported. |
| `w:characterSpacingControl` | `val="doNotCompress"` | The schema default, and this engine's behaviour. `compressPunctuation` is real East Asian justification and is reported. |
| `w:tl2br`, `w:tr2bl` | `val` is `nil`/`none`/absent | "There is no diagonal here", written as part of a complete border set. A drawn diagonal is unmodeled geometry and is reported through its container. |
| `mc:Fallback` | a `mc:Choice` was selected | The alternative to a branch that was read *in full*; ECMA-376 Part 3 requires the branches to describe the same content. A skipped `mc:Choice` is the opposite case and stays reported. |
| A `separator`/`continuationSeparator` note, and the `w:footnote`/`w:endnote` reference to it in `w:footnotePr`/`w:endnotePr` | the note holds no text, drawing, object, field or table | Word's stock rule above the notes, which this engine's layout draws for itself. Word also lets a user *replace* that rule, and a separator note holding content of its own is reported. |

Four members of the sweep's list were checked and are **not** in the class,
recorded here so the decision is not re-litigated from the tag name:

- **`w:autoRedefine`** — "update this style automatically as I format". Its
  presence is not a default (the default is off), so it says something, and this
  engine drops it. Reported, like the other editor preferences
  (`w:savePreviewPicture`, `w:doNotAutoCompressPictures`) already are.
- **`wps:style`** — the shape's theme style reference. Its `a:lnRef`/`a:fillRef`/
  `a:effectRef`/`a:fontRef` children are *suppressed*, not imported: a shape
  styled only through the theme comes out with the wrong fill and stroke. The
  wrapper is the honest place to say so until the reference is resolved.
- **`a:miter`** — selects a miter line join. `@lim="800000"` being Word's default
  says nothing about the join itself, and this engine models no join at all, so
  "nothing was lost" is not established. Reported.
- **`w:docPartUnique`** — a building-block property the model does not carry.
  Empty is not the same as meaningless: the element's presence is the value.

**Open question, recorded rather than settled.** `w:nsid`/`w:tmpl` and `w:rsid*`
are both per-session bookkeeping, and they are treated differently: the first
pair is excluded outright, the second is reported once per document as a class
(below). The distinction drawn is that an rsid identifies *content* — which
editing session wrote this run, which Word's compare and merge apply to the
user's own text — while an nsid identifies where a *list template* came from,
and no feature of the document depends on it. If a consumer for `w:nsid` is ever
found, it belongs in the rsid class rather than in the table above.

### Revision-save IDs (`w:rsid*`) — reported once per document, as a class

`w:rsid*` attributes (`rsidR`, `rsidRPr`, `rsidRDefault`, `rsidP`, `rsidDel`,
`rsidTr`, `rsidSect`, …) and the `w:rsids`/`w:rsid`/`w:rsidRoot` table in
`settings.xml` are per-editing-session bookkeeping tokens. Word's compare and
merge heuristics are their only consumer; they have no effect on layout,
rendering, text, or reopen fidelity.

They are **dispositioned once per document as one class**, under the stable
feature identifier `docx.rsid`, with an occurrence count. They are *not*
dispositioned per construct, and they are *not* declared out of scope. The
reasoning, with the measurements it rests on (a 14-document sample of real Word
output plus this repository's own `sample.docx`):

- **Volume.** Six of fifteen documents carry them, up to 1,055 attributes in one
  file, across seven attribute names on four element types (`w:p` carries five,
  `w:r` three, `w:tr` three, `w:sectPr` three).
- **Per-construct reporting would cost ~14 rows per document** — the distinct
  element-and-attribute pairs — against the 28-30 rows such a document's report
  contains in total. A ~50% inflation of every report, carrying nothing a reader
  can act on. That is how a report becomes something callers ignore.
- **One class row costs nothing and is in fact a net reduction.** The `w:rsids`
  table already raised two element findings (`rsid`, `rsids`) before the class
  existed; folding those plus every attribute into one row leaves one. Measured
  over the same fifteen documents, adding the whole attribute vocabulary changed
  the total row count by **+2 rows across all fifteen** (median 0), where
  per-construct rsid reporting would have added roughly a hundred.
- **Declaring them out of scope would be a step backwards**, not a simplification:
  the two element rows that exist today would have to be *removed*, so the
  repository would report less than it does now about something a semantic save
  genuinely drops. "No silent data loss" does not permit that.

The class entry's disposition is per-mode-honest, not constant: `omitted` +
`preserved` under a retention byte floor (the bytes really are kept), `omitted` +
`not-retained` on a semantic save (they really are dropped).

Durable identities are deliberately **not** in this class. `w14:paraId` and
`w14:textId` have real consumers — `commentsExtended.xml` and `commentsIds.xml`
join a comment through its anchor paragraph's `paraId`, and Word's co-authoring
recognizes a paragraph across saves by it — so they are dispositioned as located
attribute findings on the elements that carry them. The element is modeled, so
the model outcome is `degraded`.

## Relationship to the fidelity vocabulary

This taxonomy is per-construct disposition. It is distinct from, and feeds, the
eight fidelity **dimensions** in `34-OOXML-FIDELITY-ARCHITECTURE.md`:

- **Semantic fidelity** is measured by the distribution of the model-outcome
  axis (how much reached the model).
- **Preservation fidelity** is measured by the retention-outcome axis (how much
  unconsumed detail is recoverable).
- **Diagnostic fidelity** requires that every traversed construct *has* a value
  on both axes, and that every construct whose disposition is not the trivial
  `mapped` + `not-applicable` is enumerated in the compatibility report — see
  "What the report enumerates (and what it does not)" above. Completeness is a
  property of the taxonomy's coverage, not a demand that a healthy document
  produce a non-empty report.

A feature's support state (decode / semantic mapping / edit / export / reopen /
layout / render / behavior) is a registry concern and is not collapsed into this
per-construct taxonomy.

## Compatibility-report and ledger encoding

- Every compatibility-report entry carries both axis values. They are encoded as
  a single `disposition` naming one of the nine legal pairs, from which both axes
  are derived, so **the six illegal pairs are unrepresentable rather than
  validated after the fact**. There is no code path on which an import or an
  export can build `mapped` + `rejected`. Both axes remain readable
  individually (`model_outcome()`, `retention_outcome()`).
- The disposition is chosen **per construct**, at the site that knows what
  happened, never per import or export *mode*. A per-mode constant cannot express
  the distinction this document exists to draw: within one semantic import,
  different constructs are legitimately `not-retained`, `rejected`, `blocked`
  and `preserved`.
- A preservation-ledger record exists **iff** some construct's
  `retention_outcome` is `preserved`, and the record's ID is referenced by the
  report entry. Records name where the retained bytes live:
  - `source-snapshot` — the retention-mode byte floor, which reproduces the
    import input exactly;
  - `opaque-part` — an admitted package part carried verbatim through the
    semantic writer via the side-table;
  - `model-subtree` — a source subtree retained inside the model and re-emitted
    verbatim on save (an OMML equation with no typed projection, for example).
- **Validation fails the import.** A `preserved` entry with no record, a dangling
  record reference, a record that retains nothing, or a record reference on an
  entry that does not claim preservation is an internal error, and the import
  returns it as an error rather than reporting it. A `source-snapshot` or
  `model-subtree` record of zero bytes retains nothing; an `opaque-part` record
  is different, because its artifact is the part, which the writer re-emits with
  its name and content type even when it is zero-length.
- Every entry carries a **bounded location**: the package part it is charged to
  where the importer knows it, and the XML **local** names of the element and —
  when the finding is about an attribute rather than the element — of the
  attribute. Namespace *prefixes* are not recorded: a prefix is whatever the
  producer's `xmlns:` bound it to, so a prefix inside a stable feature identifier
  would make the identifier a property of the writer instead of the construct.
  A location is only populated where it is known; an invented location (a
  conventional part name a package need not use) is worse than none.
- **Known limitation.** The ledger is a format-adapter-level artifact: it travels
  with the DOCX importer's own report, and the format-neutral adapter report
  (`casual-doc-io`) does not carry it across the boundary yet. Validation
  therefore happens where the claim is made — an import whose claims do not
  resolve fails — but a host reading the neutral report cannot re-audit a
  `preserved` claim itself. Plumbing the ledger through the adapter layer is
  deliberately deferred, not overlooked.
- Repeated equivalent findings are aggregated by feature **and disposition**,
  keeping the count and the **first** bounded location. Two findings that share a
  feature name but differ in what happened to the construct are different
  fidelity facts and stay separate entries.

## Migration from the previous single-enum wording

For readers of earlier drafts, the previous single values map as:

| Previous single value | Model outcome | Retention outcome |
| --- | --- | --- |
| `preserved` | `mapped` or `degraded` or `omitted` (as applicable) | `preserved` |
| `degraded` | `degraded` | `preserved` or `not-retained` (must now be stated) |
| `omitted` / `non-retained` | `omitted` | `not-retained` |
| `unsupported` | `omitted` | `not-retained` or `preserved` (must now be stated) |
| `blocked` | `omitted` or `degraded` | `blocked` |
| `rejected` | `omitted` | `rejected` |

The ambiguous cases (`degraded`, `unsupported`) are exactly the ones the single
enum could not express; the dual axis forces an explicit retention decision.

## Implementation status

Landed 2026-09-16 (`105` rows FID-R-02 and FID-R-03). Where the vocabulary is
constructed today:

| Value | Constructed by |
| --- | --- |
| `omitted` | Any element the model does not represent — the common case. |
| `degraded` | An attribute of a modeled element whose meaning is not carried (`a:theme/@name`, `a:fontScheme/@name`, `w:p/@w14:paraId`, `w:p/@w14:textId`, `w:tr/@w14:paraId`), and a modeled value clamped to the model's bounds (`wp:anchor/@distT` and its siblings). |
| `mapped` | Nothing, by construction — see "What the report enumerates". |
| `preserved` | The retention byte floor (every finding, citing the source-snapshot record); an opaque side-table part (citing its own record); an OMML equation with no typed projection (citing its model-subtree record) — the last of these on the **semantic** path, which is the case a per-mode constant could not reach. |
| `not-retained` | A regenerated part's unmapped element on the semantic path. |
| `blocked` | A digital-signature part on the semantic path: retention is refused because a signature over regenerated content would assert an integrity nobody verified. "Nothing is trusted or stored" is the distinction from `not-retained`. |
| `rejected` | A structurally unusable construct: a `w15:commentEx` with no `paraId` to join on, a `w16cid:commentId` missing half its pair, a `w15:person` with no author, a `w15:presenceInfo` with no person. |
| `not-applicable` | Nothing, by construction — it pairs only with `mapped`. |

`degraded` + `blocked` is legal and currently unconstructed: refusal happens at
whole-part granularity in this implementation, and a whole part is never
partially mapped. It is named here so its absence is a recorded fact rather than
an assumed impossibility.

The vocabulary has one home, `casual-doc-import`'s `report` module, from which
`casual-doc-export` re-exports it. `casual-doc-io` deliberately keeps its own
format-neutral adapter vocabulary and converts at the boundary; that separation is
a layering decision, not drift.

## Acceptance status

This taxonomy is **accepted** as decision D3 of the ADR-027 acceptance record
(`36-ADR-027-ACCEPTANCE-RECORD.md`), 2026-07-24. Docs 32 and 34 reference it as
the single source of truth for disposition wording.
