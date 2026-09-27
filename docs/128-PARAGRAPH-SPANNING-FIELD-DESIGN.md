# 128 — The paragraph-spanning complex field

**Status:** **Implemented** for the representation layer. Design of record for `105`
OO-001 (table of contents / table of figures). **Opened:** 2026-09-27.
**Landed:** 2026-09-27 — model, validation, import, export, layout, wasm accessor.
Generation, the update command and the editing behaviour are **not** in it (§9).

**Follow-up landed 2026-09-27:** the two field-representation gaps §5a and §9 recorded
are closed. Every field — inline or range, imported or newly inserted — is now written
in the complex `w:fldChar` spelling by **one** writer, and `w:fldLock` / `w:dirty` are
modelled, imported, exported and round-tripped. §5a and §5b are the record.

**Companion:** `127` defines the field-result contract this builds on; every field
switch, picture switch and `PAGEREF` row this document carries is specified there.

Three claims in the first draft of this document were wrong, and each is corrected
below rather than quietly dropped: the import promotion cannot land a start marker
inside an open hyperlink (§4), the repair needed more than "an `end` at the
container's last paragraph" (§4a), and "verbatim" export understated what the inline
field still normalizes (§5). Each was found by driving a guard red, or by checking a
claim against the fixture instead of trusting it.

## 1. The problem, stated as a model constraint

`crates/casual-doc-model/src/v1/body.rs` — `Field` is an **`InlineNode`** whose
`inlines` are leaf inlines. So a field lives inside exactly one paragraph and its
cached result is a run of inline content.

A `TOC` field's cached result is **many paragraphs** — one per entry, each with its
own `TOC N` style, its own dot-leader tab stop and its own `PAGEREF`. There was
therefore no way to hold a real table of contents at all, and the fallback was to
emit a `BlockSdt` of ordinary paragraphs: updatable by us, and **not** a field, so
Word's own "Update Table" had nothing to update.

This document adds the representation. It does **not** generate a table of contents;
see §9.

## 2. The prior art, stated before any of our own design

In OOXML a **complex field is a range, not a container**. ECMA-376 Part 1
§17.16.18 (`CT_FldChar`) and §17.16.5.1: the field is delimited by `w:fldChar`
markers — `begin`, then `w:instrText` runs, then `separate`, then the cached result,
then `end` — and **every one of those markers is a run-level element**. Nothing
encloses the result. That is precisely why the markers can sit in *different
paragraphs*, and why Word can hold a table of contents as a field.

So the shape is **delimited range with stable anchors**, and this repository already
implements that shape three times:

| Existing range | Start / end | Shared payload |
| --- | --- | --- |
| Bookmark | `InlineNode::BookmarkStart` / `BookmarkEnd` | `Definitions::bookmarks`, keyed by `BookmarkId` |
| Comment anchor | `CommentRangeStart` / `CommentRangeEnd` | `Definitions::comments`, keyed by `CommentId` |
| Tracked move | `MoveRangeStart` / `MoveRangeEnd` | the markers' own correlating `move_id`/`name` |

A paragraph-spanning field is the **same** mechanism as a bookmark, not a new one.
ADR-030 invariant **I3** (`NodeId`/`ModelPos` anchors) exists for exactly this, and
**I4** keeps the shared payload out of the content tree.

### 2a. The representation chosen

**A range/anchor pair, modelled on the bookmark, with the instruction in
`Definitions`.**

```
InlineNode::FieldRangeStart(FieldRangeStart { id: NodeId, field: FieldRangeId })
InlineNode::FieldRangeEnd  (FieldRangeEnd   { id: NodeId, field: FieldRangeId })

Definitions::field_ranges: DefinitionMap<FieldRangeId, FieldRange>
FieldRange { instruction: String, kind: FieldKind }
```

Everything between the two markers — in document order, across paragraph and table
boundaries inside one container — is the field's **cached result**, and it is
ordinary block and inline content. Nothing about it is special: a `TOC 1` paragraph
is a paragraph, its `PAGEREF` is the existing inline `Field`, its dot leader is the
existing tab stop.

Three consequences follow from copying the bookmark rather than inventing:

- The **payload lives in `Definitions`**, not on the markers, because the two
  markers must agree about the instruction and a shared definition makes
  disagreement unrepresentable. This is why `Bookmark::name` is in `Definitions`
  and not on `BookmarkStart`.
- There is **no `separate` marker in the model.** The inline `Field` already
  models the instruction as a `String` and not as retained instruction runs;
  `separate` is the boundary between those two, so once the instruction is a string
  the boundary has nothing left to delimit. The range therefore has the *same*
  contract as the inline field — instruction out of band, result as content —
  which is the "one mechanism, not two" rule (`SKILL.md` §8). A field with no
  `separate` and hence no result is a start immediately followed by an end.
- `kind` is the same best-effort `FieldKind` projection the inline field carries,
  derived from the same `FieldKind::parse`. `instruction` stays authoritative for
  export.

### 2b. What was rejected, and the OOXML reason

**A `BlockNode::Field { instruction, result: Vec<BlockNode> }` container** —
rejected. It is simpler, and it is not what the format says. A container cannot
represent the field Word actually writes: one that **begins mid-paragraph and ends
mid-another** — the `begin` after some text in paragraph *n*, the `end` before some
text in paragraph *m*. That shape is legal OOXML (the markers are run-level and
independent) and a container would have to either refuse it or silently promote the
partial paragraphs, which is the silent-loss `AGENTS.md` forbids. It also puts the
block tree in the position of holding a *second* notion of field, so `TOC` and
`PAGE` would no longer be the same construct.

**Keeping the inline `Field` and adding a separate block-level field concept** —
rejected for the reason `SKILL.md` §8 gives: two mechanisms for one rule diverge.
Every consumer — validation, export, plain-text extraction, review projection,
the field-update pass — would need a second arm forever, and the two would drift.

**Widening the inline `Field` so its result may hold blocks** — rejected. It makes
every `PAGE` field in every header pay for a `Vec<BlockNode>`, and it still cannot
express a field that starts mid-paragraph, because the field would still be *inside*
one paragraph's inline list.

The range is the faithful option and the cheap one: two zero-width leaf markers, one
definition map entry, and content that is already content.

### 2c. The inline `Field` is unchanged, deliberately

A `PAGE` field inside a paragraph is the common case and its shape is correct. A
complex field whose `begin` and `end` fall in the **same paragraph** therefore stays
an inline `Field`, exactly as before; only a field whose markers fall in *different*
paragraphs becomes a range. The importer decides from the input (§4), and it is the
input that decides, not a policy.

This is one construct with two encodings, which needs a reason. The reason is that
the inline encoding carries the stronger invariant — the result is *contained*, so
it cannot be half-deleted, and every existing consumer already understands it — and
that invariant is available whenever the field fits in a paragraph, which is the
overwhelming majority. Promoting every complex field to a range would weaken a
guarantee for thousands of fields to accommodate the rare one. The rule for a
reader is a single sentence: **a field that fits in a paragraph is an inline
`Field`; one that does not is a range.**

## 3. Validation rules and their errors

A range is weaker than a container: it can be left unbalanced, and unbalanced
`fldChar` markers are not a cosmetic defect — Word treats everything after an
unmatched `begin` as field instruction text, so a single missing `end` can blank the
rest of the document on open. The model therefore refuses what the bookmark
validator tolerates, and the difference is deliberate.

Scope: markers are matched **within one block container** — the body, one header,
one footer, one note, one comment, one table cell, one text box. A range may span
paragraphs and tables inside its container; it may not cross out of it, because a
container is a separate part or a separate inline stream on export and the marker
would have nowhere to be written.

A **block content control (`BlockNode::Sdt`) is NOT a container**: its blocks belong
to the enclosing container's stream on export, so the balance walk descends into one
carrying the enclosing tally, and a range may legitimately open outside a control and
close inside it. This matters because a `TOC` wrapped in a `w:sdt`
(`docPartGallery = "Table of Contents"`) is the shape Word most often writes; there
the whole range is inside the control, but nothing about the rule depends on that.

Each container kind has a **negative** test — an unbalanced range inside a header, a
footer, a footnote, an endnote, a comment, a table cell and a text box is refused.
The positive test (the same range id recurring in two containers) pins the *scope* of
R3 but cannot prove a container is walked at all: it asserts acceptance, so deleting
the header walk left it green. That is recorded here because it is the exact failure
mode this repository keeps hitting.

| Rule | Violation | `ModelError` |
| --- | --- | --- |
| R1 | A marker's `FieldRangeId` is absent from `Definitions::field_ranges` | `DanglingFieldRangeRef(marker.id)` |
| R2 | A start with no matching end in its container, or an end with no matching start | `UnbalancedFieldRange(marker.id)` |
| R3 | A second start for the same `FieldRangeId` in the same container, or two ends | `DuplicateFieldRangeMarker(marker.id)` |
| R4 | A start opened while another range is open (nesting), or two ranges that cross | `OverlappingFieldRanges(marker.id)` |

R2 catches the "end precedes start" case as well, and reports it at the *end*
marker: an end encountered with no range open is an unmatched end, whether the
matching start comes later in the container or not at all. Spelling it as a separate
"inverted" error would need a whole-container pre-pass to distinguish the two, and
the diagnosis a caller can act on is the same.

R4 refuses **nesting** as well as crossing, which the format permits (`{ IF { PAGE
} … }`) and this model already refuses for the inline field (`ModelError::NestedField`).
A nested field's result flattens into its parent on import and is reported, exactly
as it does today; the range inherits that policy rather than inventing a second one.

A definition with **no** markers is allowed — it is an unreferenced definition, like
an unused style, and not a corruption.

Each rule has a test in `crates/casual-doc-model/src/v1/tests.rs`, and each test was
driven red by mutating the check it guards (see the PR for the recorded output).

**No panic and no truncation anywhere.** Validation *reports*; import *repairs*
(§4) so that what it hands the model is always balanced.

## 4. Import: build the range instead of truncating it

Before this change, `BodyParser::finish_paragraph` drained the wrapper stack and
reset `field_depth` to zero, so a field still open at `</w:p>` was committed with
whatever result it had accumulated so far and the later `fldChar end` was dropped on
the floor. That is the truncation this replaces.

The parser now distinguishes the two cases at the one point where it can:

1. `fldChar begin` opens a `FieldAccumulator` as before. Nothing is decided yet.
2. `fldChar end` **in the same paragraph** commits an inline `Field`, unchanged.
3. `</w:p>` **with the field still open** *promotes* it: the parser registers a
   `FieldRange` from the accumulated instruction, pushes a `FieldRangeStart`
   segment where the field's content began, splices the already-accumulated result
   segments after it as ordinary paragraph content, and records the open range id
   on the parser so it survives the paragraph.
4. Every following paragraph is imported with no knowledge of the range at all.
5. `fldChar end` with an open range emits a `FieldRangeEnd`.

Promotion happens **inside the wrapper drain**, after the field's own wrapper marker
is popped, so the start marker and the spliced result route into the *enclosing*
wrapper rather than being hoisted out of it.

**Correction to the first draft.** That draft said this is how "a field promoted from
inside an open `w:hyperlink`" keeps its marker in the link. It cannot happen: every
inline wrapper — `w:hyperlink`, `w:ins`/`w:del`, an inline `w:sdt` — is
*element*-delimited and must close before `</w:p>`, and a close over an open field
drains it as an **inline** field (that is what `at_paragraph_end = false` is for). So
in well-formed XML the field is necessarily the **only** open wrapper at `</w:p>`, and
the start marker always lands at paragraph level. The pop-before-route order is still
correct and is kept as defence, but it is not reachable, and the test that claimed to
exercise it was asserting nothing — the fixture had to close the hyperlink to be
well-formed XML at all, which committed the field inline before promotion could run.

The reachable and realistic case is the **end** marker: producers do write the closing
`fldChar end` inside the last entry row's `w:hyperlink`, and the marker must then be
that link's child because that is the run position it occupies and what the export has
to write back. That is what is guarded.

### 4b. Four cases that are not promoted

Each falls back to the existing inline commit and is **reported**:

| Case | Why a range would be wrong |
| --- | --- |
| `w:fldSimple` | Element-delimited: its result is its children, so no `fldChar end` can ever close a range made from one. Unreachable from well-formed XML (an unclosed `w:fldSimple` is refused by the reader); reachable from a `w:p` nested inside the field, which is balanced XML the schema does not allow, and that is what the guard uses. |
| A legacy form field (`w:ffData`) | `FieldRange` carries no form configuration, so promoting one would drop the `ffData` block silently. |
| An empty or over-long instruction | The model refuses it either way; the cached result is flattened so no text is lost. |
| A field opened while a range is already open | Nesting is refused (R4), exactly as the inline field refuses it (`ModelError::NestedField`). One policy for one construct. |

### 4a. Repair, so the model never sees an unbalanced range

A container whose markup ends with a range still open (truncated input, a producer
bug) has an `end` synthesized and the loss reported — nothing is dropped and the
document validates.

**Correction to the first draft.** "An `end` at its last paragraph" is not a
specification. Where the marker can go depends on what is still open, and getting it
wrong produces a range that crosses a container boundary — which validation then
refuses, turning a weird document into a failed *import*. The rule as implemented:

1. If a paragraph is still open, the `end` is pushed as a **segment** into it, so it
   rides into the paragraph in document order with the ids assigned as usual.
2. Otherwise it is appended to the last paragraph already committed **in the range's
   own sink** — which is why the open-range slot records whether the range was
   promoted inside a table cell. A range promoted in a cell belongs to that cell; one
   promoted outside it belongs to the part. Reading `in_cell` at repair time instead
   would put a body-scoped range's `end` inside a cell left open by truncated markup.
3. Only if that sink holds no paragraph at all is a paragraph synthesized to carry the
   marker. An empty paragraph is a visible artifact, so this is the last resort rather
   than the mechanism — and stating it removes the "unreachable" claim that a
   panic-or-truncate fallback would need.

No content is dropped on any path: the field's result is ordinary paragraphs and stays
exactly where it is. What is lost is *where the field ended*, and that is reported.

The repair runs at four kinds of boundary:

- `BodyParser::finish_container`, which replaced the epilogue copy-pasted at **four**
  parse entry points (the body, a header/footer part, and — through `close_note` — a
  note and a comment). One mechanism where there were four copies; adding the balance
  step to four copies is how the fourth gets forgotten.
- `</w:tc>`, before the cell closes, because a cell is a container.
- `exit_frame`, for a **text box** frame only, before its content is committed — so a
  range cannot leak between the box and the body. `push_frame` suspends the open-range
  slot for a text-box frame and deliberately does **not** for a block content control,
  which is transparent (§3).
- `close_note`'s unconditional tail clears the slot, so a *skipped* note (a separator)
  cannot leak a range into the next note parsed by the same reused parser.

One consequence worth stating plainly: a complex field with `begin`/`separate` and
**no `end` at all**, entirely within one paragraph, used to commit as an inline field
and now becomes a range that the repair closes in the same paragraph. At `</w:p>` the
parser cannot know whether an `end` is still coming, so paragraph-bound is not a
decision it can make — and treating it as paragraph-bound is exactly what dropped the
`end` for every real table of contents. A weaker shape for malformed input, traded for
never truncating a valid one.

## 5. Export: the `fldChar` structure, written back

`FieldRangeStart` writes the three-marker prologue and `FieldRangeEnd` writes the
epilogue, each as its own run, in the run position the marker occupies:

```xml
<w:r><w:fldChar w:fldCharType="begin"/></w:r>
<w:r><w:instrText xml:space="preserve"> TOC \o "1-3" \h \z \u </w:instrText></w:r>
<w:r><w:fldChar w:fldCharType="separate"/></w:r>
   … the result paragraphs, written as the ordinary paragraphs they are …
<w:r><w:fldChar w:fldCharType="end"/></w:r>
```

Four runs rather than one, which is what the single field writer emits for the inline
field too (§5a) and what Word itself writes. **"Verbatim" here means structurally verbatim,
not byte-identical**: a producer is free to put all four `fldChar`/`instrText`
children inside a *single* `w:r` (`sample.docx` does — see §7), and we normalize
that to one run per marker. The field is the same field to any reader, including
Word; the bytes are not the same bytes, and this document says so rather than
claiming a fidelity we do not have.

The start marker writes nothing when its definition does not resolve — the same
defence `BookmarkStart` has — while the **end marker writes unconditionally**. The
asymmetry is the point: an unmatched `begin` makes Word read the rest of the document
as instruction text, whereas a stray `end` is inert. So the export's obligation is to
never emit a `begin` it cannot complete. Validation refuses such a document first
(R1), so the path is defence in depth for a model arriving by snapshot load.

### 5a. The inline field's own normalization — closed (2026-09-27)

This was the correction to the first draft's implied claim, and it is now the record of
a gap that has been closed. It read: the **range** is written as `fldChar`, the
**inline** field is not — `write_inline` had always written an ordinary inline field as
`w:fldSimple w:instr="…"`, so a single-paragraph complex field (`sample.docx`'s `TOC`
among them) round-tripped `fldChar` → `fldSimple`. `w:fldSimple` is a real field that
Word updates, so nothing was *broken*; but Word itself writes the complex spelling, so
it was a fidelity gap, recorded here rather than hidden behind the word "verbatim".

**There is now one field writer, and it writes the complex spelling.**
`write_field_prologue` emits `fldChar begin` (carrying `w:ffData` when the field is a
legacy form field, and the update attributes of §5b), the `w:instrText` instruction and
`fldChar separate`; `write_field_epilogue` emits `fldChar end`. The inline
`InlineNode::Field` and the `FieldRangeStart` / `FieldRangeEnd` markers all go through
those two functions, so the two encodings **cannot** drift into different markup.

#### What decides simple vs. complex: nothing. There is no decision left.

`w:fldSimple` is **not written at all** any more, and the model does **not** record
which spelling a field arrived in. That is a deliberate choice, not an oversight, and
it is the one the brief asked to have argued:

* **A field imported as complex round-trips as complex.** That was the point.
* **A field imported as `w:fldSimple` also round-trips as complex.** The two spellings
  are the same field to any reader: the same instruction, the same cached result, the
  same `w:fldLock`/`w:dirty` state, the same behaviour under "Update Field".
  ECMA-376 Part 1 §17.16.19 defines `CT_SimpleField` as a shorthand for a field with no
  nested complexity; §17.16.18's `fldChar` form is the general one. Anywhere
  `w:fldSimple` is legal (`EG_PContent`) a `w:r` is legal too, so the complex spelling
  is always available and never less valid.
* **A field the user newly inserts is complex**, for the same reason — it goes through
  the same single writer. `casual-doc-edit`'s `CommonField::build` and `field_node`
  construct it with a default update state (neither locked nor dirty: the cached result
  was just computed, and freezing a field is a later author decision).
* **The model does not remember provenance**, because remembering it would require a
  second writer to act on it, and two writers for one rule is the shape that produced
  this gap in the first place. Nor is it preservation loss in the sense `docs/34` is
  about: `docs/34`'s sidecar exists for *document data* the model cannot express, and
  the choice between two encodings of an identical field is a producer's serialization
  habit, not data. `w:fldSimple` therefore stays **readable** on import — real
  documents use it, including inside `w:hyperlink` — and unreachable on export.

What is still **not** byte-identical, said plainly rather than left implied: a producer
may pack all four `fldChar`/`instrText` children into a *single* `w:r` (`sample.docx`
does — §7), and we write one run per marker. Word writes one run per marker too, so this
is normalization toward Word, but it is still normalization, and
`sample_docx_keeps_its_toc_field_as_an_updatable_field` says so in its own assertion
message.

### 5b. `w:fldLock` and `w:dirty` — modelled, imported, exported (2026-09-27)

§9 listed these as out of scope. They were dropped entirely — no handling anywhere in
import, export or the model — and that is worse than a byte difference, because both
attributes change what a **reader sees**:

| attribute | meaning | cost of dropping it |
| --- | --- | --- |
| `w:fldLock` | Word must not update this field | a deliberately frozen field (a dated letter, a quoted total) becomes one Word refreshes, so the document's *content* changes on the reader's machine |
| `w:dirty` | the cached result is stale; recalculate on open | our cached result is presented as current, when the producer had already marked it out of date |

Both are now `FieldUpdateState { locked, dirty }` — **one type**, carried by the inline
`Field` and by the definitions-side `FieldRange`, so a field cannot change its update
semantics by changing its encoding. Two independent flags rather than one tri-state,
because Word can produce a field that declares both (and `fldLock` wins there).

Import reads them from `w:fldSimple` (`CT_SimpleField`) and from **every** `w:fldChar`
marker (`CT_FldChar` allows them on any marker, not only the `begin` Word writes them
on) and **merges** rather than overwrites, since an absent attribute is the schema
default and not an assertion of `false`. A flag on the `end` marker of a field that was
promoted to a range lands on the registered `FieldRange`; a flag on a marker with no
field at all is *reported*, not swallowed. Export writes them on the `begin` marker.

**A recorded near-miss.** The first implementation read the attributes with `is_true`,
which answers the `w:val` question — where a *missing* value means `true`, as in
`<w:b/>` — so every field in every document came back locked **and** dirty. It was
caught by the existing `a_paragraph_spanning_toc_field_is_written_back_as_fld_chars`
guard, which searches for the exact bytes `<w:fldChar w:fldCharType="begin"/>` and found
`<w:fldChar w:fldCharType="begin" w:fldLock="true" w:dirty="true"/>`. Absence is now
checked before the value is interpreted, and every guard for this feature carries a
field that declares **neither** attribute, so the same mistake cannot pass again.

**One deliberate difference from Word.** Inside a tracked deletion Word writes
`w:delInstrText`; we write `w:instrText`. `CT_R` admits both, so the output is
schema-valid either way, and `w:instrText` is chosen because the importer reads it and
does not read `w:delInstrText` — emitting Word's spelling would lose a deleted field's
instruction on reopen. Recorded here and in `write_field_prologue`'s doc comment rather
than left ambiguous; reading `w:delInstrText` on import is the change that would let this
flip.

## 6. Layout: no new path

The two markers are **zero-width inert leaves**, like the bookmark markers. The
inline dispatch in `crates/casual-doc-layout/src/flow.rs` already ends in a
catch-all, so the markers cost nothing and need no arm; the result paragraphs are
ordinary paragraphs and flow through the ordinary pipeline. A `TOC 1` row's dot
leader and its `PAGEREF` work because `127` §5 already made a paragraph containing a
field soft-wrap and read `stop.leader` — which is why that fix is a dependency of
this one and not a coincidence.

Nothing in layout learns what a table of contents is.

## 7. Round-trip evidence, and a provenance correction

`sample.docx` carries one real, unpopulated `TOC` field:

```xml
<w:p><w:r>
  <w:fldChar w:fldCharType="begin"/>
  <w:instrText xml:space="preserve"> TOC \o "1-3" \h \z \u </w:instrText>
  <w:fldChar w:fldCharType="separate"/>
  <w:t>Update this field in Word to generate the table of contents.</w:t>
  <w:fldChar w:fldCharType="end"/>
</w:r></w:p>
```

Two facts about this fixture are worth recording, because both have been stated the
other way round in this repository. **Both were re-verified against the file itself,
not carried over from the draft.**

1. **It is not Word output.** `docProps/core.xml` says
   `<dc:description>generated by python-docx</dc:description>`; `docProps/app.xml`
   reads `Microsoft Macintosh Word` / `AppVersion 14.0000` because that is what
   python-docx's bundled `Normal.dotm` template carries, not because Word wrote the
   file. (Its XML declarations are single-quoted `lxml` output, too.) So it is **not**
   the stronger provenance for field markup, and its single-`w:r` field is in fact a
   shape Word never writes. It is still a valuable fixture — a real `TOC` instruction
   with a real cached result — but the claim that it is Word-produced is false and
   should not be repeated.
2. **This field does not exercise the range at all.** The whole document contains
   exactly three `fldChar` markers, all inside one `w:r` in one `w:p`. By §2c it is,
   correctly, an inline `Field` — before this change and after it. It is the guard
   that the inline path did not regress, not the guard for the new one.

### 7a. What is actually asserted

| Guard | Fixture | What it proves |
| --- | --- | --- |
| `a_paragraph_spanning_toc_field_is_written_back_as_fld_chars` | A four-paragraph `TOC`: `begin`/instruction/`separate`, three entry rows with leader tabs and `PAGEREF` fields, `end` in the last row | `begin` → `instrText` → `separate` → result → `end` in that order, each marker its own run, and **three `w:hyperlink` rows and three `w:p` between `separate` and `end`** — the assertion neither a container representation nor the `BlockSdt` fallback could satisfy |
| `a_paragraph_spanning_toc_field_survives_write_then_reopen` | the same | write → reopen produces an equal model, markers and definition included; and a **second save is byte-identical**, so the shape has a fixed point rather than drifting per save |
| `sample_docx_keeps_its_toc_field_as_an_updatable_field` | `sample.docx` | its field is **not** promoted, survives export → reopen as a field with the same instruction, and is written as the `w:fldSimple` §5a records |
| `a_field_range_marker_whose_definition_is_missing_writes_no_orphan_begin` | a hand-built model with a balanced pair and no definition | validation refuses it (R1) and the export emits no `begin` and no invented instruction |
| `field_range_markers_are_inert_and_their_result_flows_as_paragraphs` | two entry paragraphs bracketed by markers | the flowed galley is **equal** to the same body without the markers, and the entries really did flow (so it is not two empty galleys agreeing) |

**Word's own "Update Table" has not been exercised**, because that needs Word and there
is none here. What is established is that the export writes the `fldChar` structure
Word's field machinery reads, in the order the schema requires, with the instruction
intact. That is the necessary condition; it is not a test of Word, and this document
does not claim to be one.

## 8. Complexity, stated per path

| Path | Cost | Note |
| --- | --- | --- |
| Import of a range | O(1) per marker on top of the existing single-pass parse | Promotion moves the already-accumulated segments once: O(result segments in the first paragraph) |
| Import repair | O(1) per container boundary | One `Option` check, plus at most one look at the last block of one sink — never a walk |
| Validation, R1 (reference resolves) | O(1) per marker | A keyed `contains_key`, inside the existing `validate_inlines` walk. No lookup-by-id inside a loop over ids. |
| Validation, R2–R4 (balance) | O(inlines in the container) time, O(ranges in the container) memory | **A walk of its own**, corrected from the first draft — see below |
| Export of a marker | O(1) | Four `write_event`s |
| Layout | O(1) — a catch-all arm | The result paragraphs cost what paragraphs cost |
| `fieldRangeEntries` (wasm) | O(ranges) | Reads the definition map; **no document walk**, the same shape as `bookmarkEntries` |
| **A keystroke** | **unchanged** | See below |

**Correction to the first draft: balance is its own walk, not a fold.** The draft said
the balance check was "folded into the existing validation walk; no second pass". It is
not, and it cannot be cheaply: balance is a *container-scoped* question — "did this
start find its end before the container ran out?" — while `validate_inlines` sees one
inline list at a time with no mutable state threaded through the block recursion.
Folding a tally in would mean adding a `&mut` parameter to `validate_block`,
`validate_table`, `validate_inlines` and `validate_group` for one construct.

So `validate_field_ranges` is a second O(document) walk. That is acceptable, and the
reason is worth being explicit about rather than papering over: `Document::validate`
already walks the whole document more than once (`validate_unique_ids` and
`validate_body` are each their own pass), it runs **per mutation, not per keystroke**,
and its memory is O(ranges in the container) — never a per-paragraph row, which is what
`docs/127` §3 records the cost of on a 1.3M-paragraph document. What would not be
acceptable is a per-marker lookup-by-id inside the walk, and there is none: the tally is
two `BTreeSet`s of range ids and one `Option`.

**What keeps a keystroke O(1).** Nothing on the editing path consults a field range.
The markers are inert leaves; the result paragraphs are paragraphs; the definition map
is keyed and never scanned. No code asks "am I inside a field range?" — and none needs
to, since generation and update are not in this layer.

That is a budget, not a sentence:
`a_keystroke_in_a_document_with_a_field_range_costs_no_extra_document_scan` asserts a
keystroke **inside a field's cached result** costs the same one `document_scans` as the
same document with no range, and that the count does not change when the document
doubles. It has its own range-bearing fixture because the existing
`a_keystroke_costs_a_bounded_number_of_document_scans` opens plain text and therefore
holds no range — a scan added on the range path would never execute there. Mutation:
walking the document once per range on the keystroke path takes it to 2 against a
budget of 1.

### 8a. The operation set stayed closed (ADR-030 I2)

**Zero new operations.** A range arrives by import and leaves by export; the markers
are `InlineNode`s, so the existing content operations move, copy and delete them like
any other inline, and the definition table is reached the same way
`Definitions::bookmarks` is. Undo/redo, review tracking and the transaction log
therefore work without knowing the feature exists — the same result the captions work
reached, and for the same reason.

The one place this shows as a *restriction* rather than a freedom is §10.1: an edit that
deletes one marker of a pair makes the document invalid and is refused. That is the
honest consequence of not adding an operation, and the fix is the editing work in §9,
not an operation bolted on here.

When generation and update land (§9) they are legitimately O(document): per ADR-005
and `107` §4 they must not run on the main thread, must show progress and must be
cancellable, and `webapp/src/background_measure.mjs` is the cooperative-yield path
they use — there is no worker.

## 9. Deliberately out of scope

- **Generating** a table of contents or a table of figures — walking the headings,
  building the `TOC N` paragraphs, computing the page numbers.
- The **update** command and its UI, including the F9 / "Update Table" affordance
  and the progress/cancel surface an O(document) rebuild requires.
- **Editing** a range: creating one, deleting one, or the selection behaviour of a
  caret inside a field result (Word selects the whole field on one click). Today a
  range arrives by import and leaves by export.
- ~~`w:fldChar/@w:dirty` and `@w:fldLock`.~~ **Done (2026-09-27) — see §5b.** They
  landed in the one change that covers both encodings, exactly as this bullet asked,
  together with the `w:fldSimple` normalization of §5a. Neither is needed for "Update
  Table", which is a reader action rather than a document flag — `w:dirty` only asks a
  reader to update *automatically on open* — but both are author intent about content,
  which is why they are no longer dropped.

## 10. Open questions, recorded rather than hidden

1. **An edit that deletes one marker is refused, with a reason, rather than
   repaired.** R2 makes a half-deleted range invalid, so an operation that removes
   the paragraph holding a `TOC` field's start fails validation and is rejected.
   That is the safe direction — the alternative is exporting markup that blanks the
   rest of the document in Word — but the *right* behaviour is for the delete to
   take the whole range, which needs the editing work in §9. Until then the refusal
   is the honest answer and not the final one.
2. **Nesting is refused (R4) where the format permits it.** Consistent with the
   inline field's existing `NestedField` refusal, and no producer in the corpus
   writes a nested `TOC`. If one appears, the fix is to allow a stack in validation,
   not to change the representation.
3. **A range may not cross a container boundary.** A field that begins in the body
   and ends inside a table cell is legal OOXML and is repaired to a body-scoped
   range with a reported loss. No producer in the corpus writes one.
4. **`Definitions::field_ranges` is not garbage-collected.** A definition whose
   markers are both deleted stays in the map (allowed by §3). Bookmarks behave the
   same way; a sweep, if wanted, belongs with the editing work.
5. **A range crossing into or out of a block content control is allowed by validation
   but repaired by import.** §3 makes a `BlockNode::Sdt` transparent, so a range that
   opens outside one and closes inside it is a *valid* model. Import cannot produce it,
   though: `push_frame` suspends the open-range slot for a text-box frame only, and a
   block control's frame leaves the slot live — which is right — but a range left open
   when the control's frame exits is not balanced there, so it reaches the container's
   own repair instead. No producer in the corpus writes one, and the asymmetry is
   recorded rather than resolved because resolving it means threading the slot through
   the frame for one kind and not the other twice over.
6. ~~**The inline field still exports as `w:fldSimple`** (§5a).~~ **Closed
   (2026-09-27).** It exports in the complex spelling, from the same writer the range
   markers use, in the same change that added `w:dirty`/`w:fldLock` — §5a and §5b. The
   model deliberately does not record which spelling a field arrived in, and §5a argues
   that choice rather than leaving it implicit.
