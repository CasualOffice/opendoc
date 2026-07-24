# Import Disposition Taxonomy

**Status:** Accepted — 2026-07-24 (decision D3, `36-ADR-027-ACCEPTANCE-RECORD.md`)
**Tracker:** P1A-004
**Applies to:** `32-PHASE-1A-SEMANTIC-DOCX-IMPORT-DESIGN.md`,
`34-OOXML-FIDELITY-ARCHITECTURE.md`
**Reconciles:** the three divergent disposition enums previously stated in
docs 32 and 34.

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

## Relationship to the fidelity vocabulary

This taxonomy is per-construct disposition. It is distinct from, and feeds, the
eight fidelity **dimensions** in `34-OOXML-FIDELITY-ARCHITECTURE.md`:

- **Semantic fidelity** is measured by the distribution of the model-outcome
  axis (how much reached the model).
- **Preservation fidelity** is measured by the retention-outcome axis (how much
  unconsumed detail is recoverable).
- **Diagnostic fidelity** requires that every traversed construct carries a
  value on *both* axes in the compatibility report — completeness.

A feature's support state (decode / semantic mapping / edit / export / reopen /
layout / render / behavior) is a registry concern and is not collapsed into this
per-construct taxonomy.

## Compatibility-report and ledger encoding

- Every compatibility-report entry carries a `model_outcome` field and a
  `retention_outcome` field (replacing the former single `disposition` field).
- A preservation-ledger entry exists **iff** some construct's
  `retention_outcome` is `preserved`; the ledger entry ID is referenced by the
  report entry.
- Report completeness means every admitted part and every traversed unsupported
  element, attribute, relationship, or markup-compatibility branch carries both
  axis values. Repeated equivalent findings may be aggregated only when counts
  and first bounded locations remain deterministic.

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

## Acceptance status

This taxonomy is **accepted** as decision D3 of the ADR-027 acceptance record
(`36-ADR-027-ACCEPTANCE-RECORD.md`), 2026-07-24. Docs 32 and 34 reference it as
the single source of truth for disposition wording.
