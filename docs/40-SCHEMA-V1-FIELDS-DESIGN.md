# Normalized Schema v1: Fields Design

**Status:** Accepted — 2026-07-25 (repository owner directive: proceed and complete
the semantic model)
**Tracker:** P1A-019 (schema v1 semantic extension), fields slice
**Decision basis:** ADR-027, schema v1 design (`38-…`), tables design (`39-…`),
disposition taxonomy (`35-…`)
**Supersedes for fields:** `38-…` §"Out of scope for v1" listed fields as
report+ledger only. This document promotes fields to an additive inline v1 value,
following the established additive pattern (drawings, hyperlinks P1A-021; tables
P1A-022). Retention round-trip is unchanged.

## Why

A WordprocessingML field is dynamic content (page number, date, cross-reference,
TOC entry, form calculation) written as a **field instruction** plus a **cached
result** (the last-computed value the producer stored). Today the importer reports
`w:fldSimple`/`w:fldChar`/`w:instrText` as unmapped and keeps their cached-result
runs as ordinary text. That loses the fact that the text is a field result and
discards the instruction, so the content cannot be recomputed or edited as a
field. Fields are inline, so they slot into the same inline model as runs,
drawings, and hyperlinks.

This slice models the field **instruction** and its **cached-result inlines**. It
does not evaluate fields (no recomputation — that is a layout/runtime concern) and
does not parse the instruction grammar; the instruction is retained as an opaque,
bounded string so its meaning survives for a future evaluator and for round-trip.

## Two source forms

WordprocessingML writes fields two ways; both map to one model node:

1. **Simple** — `<w:fldSimple w:instr=" PAGE ">` wrapping the cached-result runs
   as child content. The instruction is the `w:instr` attribute.
2. **Complex** — a run sequence delimited by field characters:
   `<w:r><w:fldChar w:fldCharType="begin"/></w:r>`,
   then one or more `<w:r><w:instrText> … </w:instrText></w:r>` (the instruction,
   concatenated in order),
   then `<w:r><w:fldChar w:fldCharType="separate"/></w:r>`,
   then the cached-result runs,
   then `<w:r><w:fldChar w:fldCharType="end"/></w:r>`.
   A field with no `separate` has no cached result (empty result inlines).

## Model

New inline variant on the existing `v1::InlineNode` (tagged `"type"`,
snake_case → `{"type":"field",…}` — additive; existing snapshots unaffected):

```text
InlineNode {
  Run(Run)             // unchanged
  Tab(Tab)
  Break(Break)
  Drawing(Drawing)
  Hyperlink(Hyperlink)
  Field(Field)         // new
}

Field {
  id: NodeId,
  instruction: String,        // the field code, opaque; non-empty, <= 4096 bytes
  inlines: Vec<InlineNode>,   // cached result (MAY be empty); leaf inlines only
}
```

`Field.inlines` is the cached result — most often a single run. Unlike
`Hyperlink.inlines`, it may be empty (a field with no `separate`/no result).

### Wrapper nesting rule (bounds inline recursion)

`Hyperlink` and `Field` are the two inline **wrappers**. To keep inline recursion
bounded and validation simple, a wrapper may contain only **leaf** inlines
(`Run`/`Tab`/`Break`/`Drawing`) — never another wrapper. This generalizes the
existing "no nested hyperlink" rule to "no wrapper inside a wrapper", so maximum
inline nesting stays at one wrapper level. Consequences:

- a hyperlink cannot contain a field, and a field cannot contain a hyperlink or a
  field. The importer captures a field only at paragraph top level; a wrapper
  encountered inside another wrapper is reported and its runs flatten into the
  enclosing wrapper (no silent loss).
- HYPERLINK-instruction complex fields (a hyperlink expressed as a field) are
  modeled as a `Field` with the URL in the instruction and the display runs as the
  cached result — they are not converted to `Hyperlink` nodes in this slice.

## Constants and domains

- `instruction` ∈ non-empty, ≤ 4096 bytes → else
  `PropertyValueOutOfDomain{"field.instruction"}`.
- `Field.inlines` may be empty; when non-empty, each child is a leaf inline.
- A `Field` (or `Hyperlink`) inside any wrapper → `NestedField(id)` /
  `NestedHyperlink(id)`.

## Strict validation (additive to `validate_inlines`)

`validate_inlines` gains an `in_wrapper` context (replacing the `in_hyperlink`
flag; `in_hyperlink || in_field`). For each inline:

- `Field`: reject if `in_wrapper` (`NestedField`); check `instruction` domain;
  validate `inlines` with `in_wrapper = true`; a field is a hard run-merge boundary
  (resets adjacent-run tracking, like a drawing/hyperlink). An empty `inlines` is
  allowed.
- `Hyperlink`: reject if `in_wrapper` (`NestedHyperlink`, unchanged meaning);
  otherwise as today, validating children with `in_wrapper = true`.

Id-uniqueness (`record_inline_ids`) and snapshot text/scalar limits
(`accumulate_inline_limits`) recurse into `Field.inlines` exactly as they do for
`Hyperlink.inlines`, so a field's cached-result ids join the global id set and its
text counts against the bounds. The field's `instruction` bytes count against a
bound too (`field.instruction` ≤ 4096), independent of text-run limits.

New `ModelError` variant: `NestedField(NodeId)`.

## v0 → v1 migration

Unchanged. v0 has no fields; migration still produces only paragraphs/runs and the
byte-exact golden is unaffected.

## Import (`casual-doc-import`)

A small field state machine in the body parser, reusing the segment pipeline:

- **Simple** — `<w:fldSimple>` opens a field accumulator seeded with the `w:instr`
  attribute; its child runs become the cached-result segments; `</w:fldSimple>`
  commits a `Field` segment. An empty/oversize instruction is reported and the
  field's runs flatten into the paragraph (no loss).
- **Complex** — a `fldChar` state machine over the run stream:
  `begin` opens a field accumulator (state = collecting instruction);
  `instrText` text appends to the instruction while in that state;
  `separate` switches to state = collecting result (subsequent segments are the
  cached result); `end` commits the `Field` segment. `fldChar`-carrying runs
  themselves emit no text. A `begin` with no `end` (malformed) is flushed at
  paragraph close so nothing is dropped. Nested complex fields (a `begin` before
  the previous `end`) are bounded by a depth counter; the inner field's content
  flattens into the outer and the nesting is reported (the model forbids
  wrapper-in-wrapper).
- Routing reuses `push_segment`: while a field is open, runs/tabs/breaks/drawings
  route into the field accumulator's cached result — including any that arrive
  before `separate`, which a well-formed field never has, so this only preserves
  malformed pre-`separate` display content rather than dropping it. A hyperlink
  encountered inside a field is reported and its runs flatten into the field; a
  field encountered inside a hyperlink is reported and flattens into the hyperlink.
- Instruction text (`w:instrText`) appends to the open field's instruction while
  it is collecting one; instruction text with no field collecting it (orphaned,
  or appearing after `separate`) is reported, never silently dropped.
- Ids are allocated in document order (the field id before its cached-result
  inline ids), exactly as hyperlinks do.

Everything not modeled (field `w:fldLock`/`w:dirty` flags, the instruction
grammar, form-field `w:ffData`) is reported and — in Retention — preserved.

## Round-trip and fidelity

- Retention is unchanged: a field document round-trips byte-for-byte.
- The fidelity harness recurses `Field.inlines` (the cached result) when
  extracting text, so a field's displayed value counts toward the text proxy —
  matching what LibreOffice renders for the field.

## Acceptance evidence

- Model: unit tests for a valid simple and complex field (JSON round-trip),
  empty-result field, nested-wrapper rejection (`NestedField`, field-in-hyperlink,
  hyperlink-in-field), instruction-domain rejection, and id-uniqueness/limits
  recursion into the cached result.
- Import: `w:fldSimple` and a full `begin/instrText/separate/end` complex field
  map to a `Field` with the expected instruction and cached-result text; a
  `begin`-without-`end` is flushed without loss; nested fields report and flatten.
- All workspace gates green; adversarial multi-agent review run, findings folded.

## Out of scope for this slice (still reported + Retention-preserved)

Field evaluation/recomputation; instruction grammar parsing; form fields
(`w:ffData`), `w:fldLock`/`w:dirty`; converting HYPERLINK fields to `Hyperlink`
nodes; wrapper-in-wrapper nesting as model structure.
