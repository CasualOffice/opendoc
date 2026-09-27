# 128 — The paragraph-spanning complex field

**Status:** Design of record for `105` OO-001 (table of contents / table of figures),
**representation layer only**. **Opened:** 2026-09-27.
**Companion:** `127` defines the field-result contract this builds on; every field
switch, picture switch and `PAGEREF` row this document carries is specified there.

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

Promotion happens **inside the wrapper drain**, so a field promoted from inside an
open `w:hyperlink` puts its start marker and its result inside that hyperlink, which
is where the markup put them.

### 4a. Repair, so the model never sees an unbalanced range

A container whose markup ends with a range still open (truncated input, a producer
bug) has an `end` synthesized at its last paragraph and the loss reported — nothing
is dropped and the document validates. A text box is a container: `enter_frame`
saves the open-range slot alongside `field_depth` and `exit_frame` closes any range
the frame left open, so a range cannot leak out of a text box into the body.

The hook for this is a new `BodyParser::finish_container`, which replaced the
six-line epilogue that was **copy-pasted at five parse entry points** (body,
notes, header/footer, comments, and the text-box frame unwind). One mechanism where
there were five copies; adding the balance step to five copies is how the fifth gets
forgotten.

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

Four runs rather than one, which is what the existing inline-field writer already
emits and what Word itself writes. **"Verbatim" here means structurally verbatim,
not byte-identical**: a producer is free to put all four `fldChar`/`instrText`
children inside a *single* `w:r` (`sample.docx` does — see §7), and we normalize
that to one run per marker. The field is the same field to any reader, including
Word; the bytes are not the same bytes, and this document says so rather than
claiming a fidelity we do not have.

A start marker whose definition does not resolve writes nothing — the same
defensive behaviour `BookmarkStart` already has — but validation refuses that
document first (R1), so it is unreachable from a valid model.

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
other way round in this repository:

1. **It is not Word output.** `docProps/core.xml` says
   `<dc:description>generated by python-docx</dc:description>`; `docProps/app.xml`
   reads `Microsoft Macintosh Word` / `AppVersion 14.0000` because that is what
   python-docx's bundled `Normal.dotm` template carries, not because Word wrote the
   file. So it is **not** the stronger provenance for field markup, and its
   single-`w:r` field is in fact a shape Word never writes. It is still a valuable
   fixture — it is a real `TOC` instruction with a real cached result — but the
   claim that it is Word-produced is false and should not be repeated.
2. **This field does not exercise the range at all.** Its `begin` and `end` are in
   the same paragraph, so by §2c it is, correctly, an inline `Field` — before this
   change and after it. It is the guard that the inline path did not regress, not
   the guard for the new one.

The range is therefore exercised by a fixture that can actually reach it: a
paragraph-spanning `TOC` whose `begin` is in one paragraph, whose result is three
`TOC 1` paragraphs with dot leaders and `PAGEREF` fields, and whose `end` is in the
last of them. A fixture that cannot exercise the path under test is the specific
trap this area has already fallen into, so both fixtures are asserted on and the
paragraph-spanning one is asserted to *be* a range.

## 8. Complexity, stated per path

| Path | Cost | Note |
| --- | --- | --- |
| Import of a range | O(1) per marker on top of the existing single-pass parse | Promotion moves the already-accumulated segments once: O(result segments in the first paragraph) |
| Import repair (`finish_container`) | O(1) | One `Option` check per container, not a walk |
| Validation of a container | O(1) per marker, O(open ranges) memory | Folded into the existing validation walk; no second pass, no per-marker lookup-by-id inside a loop |
| Export of a marker | O(1) | Four `write_event`s |
| Layout | O(1) — a catch-all arm | The result paragraphs cost what paragraphs cost |
| `fieldRangeEntries` (wasm) | O(ranges) | Reads the definition map; **no document walk**, the same shape as `bookmarkEntries` |
| **A keystroke** | **unchanged** | See below |

**What keeps a keystroke O(1).** Nothing on the editing path consults a field range.
The markers are inert leaves; the result paragraphs are paragraphs; the definition
map is keyed and never scanned. A document containing a `TOC` range does exactly the
same per-keystroke work as the same document without one, because no code asks
"am I inside a field range?" — and no code needs to, since generation and update are
not in this layer. `document_scans` continues to pin the per-keystroke scan count at
one.

Validation is the one path that touches every marker, and it is already O(document)
by construction, running inside the walk it shares with every other rule rather than
adding a pass of its own. It runs per mutation, not per keystroke.

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
- `w:fldChar/@w:dirty` and `@w:fldLock`. Both are dropped today for the inline field
  too, so modelling them only on the range would create the two-mechanisms problem
  §2b rejects; they belong in one change that covers both encodings. Neither is
  needed for "Update Table", which is a reader action, not a document flag —
  `w:dirty` only asks a reader to update *automatically on open*. Recorded as a real
  fidelity gap, not as done.

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
