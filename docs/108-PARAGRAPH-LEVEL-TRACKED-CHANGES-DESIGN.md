# 108 — Paragraph-level tracked changes

**Status:** Phase 1 implemented (foundation and read side). Phase 2 (authoring) not started.
**Closes (across both phases):** `104` HF-130, HF-131. Also three defects found while
mapping, recorded as new `104` rows: lossy undo of a paragraph join, split duplicating an
imported mark revision, and paragraph-only tracked changes reading as a clean document.
**Related:** `86` (run-level revision-aware editing), `107` (OT tier table), ADR-033.

## Problem

The model has carried both paragraph-level revision kinds for a long time, and import and
export round-trip them:

- `ParagraphProperties.mark_revision` — `w:pPr/w:rPr/w:ins|w:del`, a tracked insertion or
  deletion of the **paragraph mark** itself, i.e. a tracked split or merge.
- `ParagraphProperties.prop_change` — `w:pPrChange`, a tracked paragraph formatting change
  holding the prior properties.

Nothing above the importer uses them. The review list, accept/reject, navigation and
painting all work on runs only, and Suggesting mode refuses Enter, every cross-paragraph
deletion and every paragraph formatting command (17 refusal sites). A Word document whose
reviewer split paragraphs or recentred a heading opens looking clean: no cards, and Accept
All reports "no tracked revisions" while the file still carries them.

## Word semantics (the contract)

Sources: ISO/IEC 29500-1 §17.13.5.15, §17.13.5.20, §17.13.5.29; MS-OI29500 notes on the
same sections (which correct §17.13.5.20's copy-paste error); Word-saved fixtures from
Open-Xml-PowerTools `TestFiles/RP` (RP005, RP025, RP040, RP041, RP045, RP047, RP052); and
the published behaviour of LibreOffice and ONLYOFFICE, both built to match Word. ONLYOFFICE
is AGPL and was consulted as behaviour reference only.

**The paragraph mark ends the paragraph and owns its properties.** Every rule below follows
from that one fact.

| Revision on paragraph P | Accept | Reject |
|---|---|---|
| mark `ins` | Clear the revision; the split stays. | Join P into the next paragraph. The **next** paragraph's properties survive — that is the original mark. |
| mark `del` | Join P (and any run of consecutive deleted marks) into the next non-deleted paragraph, keeping **that** paragraph's properties. | Clear the revision; the paragraphs stay separate. |
| `pPrChange` | Drop the prior snapshot. | Restore the prior base properties exactly, keeping the mark's own run properties and any section properties. |

What Word writes, for phase 2:

| Edit while tracking | Written |
|---|---|
| Enter (middle or end) | The **leading** half's mark gets `ins`; the trailing half keeps the original mark. |
| Delete at end of A / Backspace at start of B | A's mark gets `del`; B is untouched. |
| Range from inside A to inside B | Covered runs wrapped as deletions; A's mark and every middle mark get `del`; B's mark does not. Paragraphs after A receive A's properties **plus** a `pPrChange` with their own prior. |
| Whole paragraph (start of A through its mark) | Runs and mark deleted; no `pPrChange` on the next paragraph. |
| Paragraph formatting | New properties plus `pPrChange` holding the full prior. |

## The operation layer

The split and join operations have to express these semantics **and** stay exact inverses.
Today they are not inverses:

- `JoinParagraphs` keeps the first paragraph's properties and drops the second's. Its
  inverse `SplitParagraph` clones the first's properties onto the restored second. So
  **Backspace a heading into the paragraph above, then undo, and it comes back as body
  text** — a data-loss defect in ordinary Editing mode, unrelated to review.
- `SplitParagraph` clones every property onto both halves, including `mark_revision`. After
  Enter inside an imported paragraph with a deleted mark, both halves carry the deletion,
  and accepting the leading one joins what the user just split.

### Decision 1: both operations carry an optional properties payload

```rust
SplitParagraph {
    at: Pos,
    new_id: NodeId,
    /// `None` is Enter. `Some` sets both halves exactly.
    properties: Option<Box<SplitProperties>>,
}
JoinParagraphs {
    first: NodeId,
    second: NodeId,
    /// `None` keeps the first paragraph's properties (Backspace). `Some` sets the merged
    /// paragraph exactly.
    properties: Option<Box<ParagraphProperties>>,
}
pub struct SplitProperties { pub leading: ParagraphProperties, pub trailing: ParagraphProperties }
```

- **Join** captures both paragraphs' properties before merging and returns
  `SplitParagraph { properties: Some({ leading: first_before, trailing: second_before }) }`.
  Undo is exact for any payload.
- **Split** captures the paragraph's properties before splitting and returns
  `JoinParagraphs { properties: Some(original) }`. Undo is exact for any payload.
- **Split with `None`** (Enter): the trailing half inherits the paragraph's properties,
  including `mark_revision`, because it holds the original mark; the leading half gets the
  same properties **without** `mark_revision`, because its mark is new. Word's `w:next`
  style rule still applies to the trailing half at the end of a paragraph. `prop_change` is
  kept on both halves: the formatting change applies to both, and whether Word duplicates
  the snapshot is unverified (see Open questions).
- **Join with `None`** keeps today's result — the merged paragraph looks like the first —
  which matches Word's Backspace and is pinned by
  `join_paragraphs_keeps_the_surviving_paragraphs_properties`.

A payload rather than a new operation: the operation count stays constant, so there is
nothing new to classify in the `107` tier table (both remain T1 positional), and every
review decision below is expressible as one split or join. The alternative — a dedicated
`RestoreSplit` variant — would still leave the join side without a way to keep the
**second** paragraph's properties, which accept-deletion requires.

### Decision 2: review decisions are compositions of split, join and SetParagraphProperties

| Decision | Operations |
|---|---|
| Accept `ins` on P | `SetParagraphProperties(P, props − mark_revision)` |
| Reject `ins` on P (next N) | `JoinParagraphs(P, N, Some(N.props))` |
| Accept `del` on P (next non-deleted N) | `JoinParagraphs(P, N, Some(N.props))`, repeated from the last deleted mark in the run back to P, so each join merges into a paragraph that still exists |
| Reject `del` on P | `SetParagraphProperties(P, props − mark_revision)` |
| Accept `pPrChange` on P | `SetParagraphProperties(P, props − prop_change)` |
| Reject `pPrChange` on P | `SetParagraphProperties(P, prior + current mark_run + current mark_revision + current section)` |

Every decision runs through `apply_action_caret_as(.., HistoryKind::Review)`, so it is one
undo step and rolls back atomically.

**A merge that cannot happen clears the revision instead.** A merging mark with no following
paragraph in the same container (the last paragraph of a cell or surface, or one before a
table) has nothing to merge into. Word does not write that shape; files that contain it came
from other producers. Deciding it clears the revision and deletes nothing. This was first
designed as a refusal and changed during implementation: a refusal would make Accept All fail
for the whole document over one malformed mark, and clearing loses no content the user can
see. Guard: `a_merging_mark_with_nothing_after_it_clears_instead_of_deleting`.

### Decision 3: review item identity

Neither `MarkRevision` nor `PropChange` has a node id, and one paragraph can carry both, so
the paragraph id is not a unique key. Review items get a kind-qualified id:
`<paragraph-id>:mark` and `<paragraph-id>:format`. `decideRevision` resolves that suffix
before the existing run lookups. Existing 32-hex ids are unaffected, and the JS card cache
key (`revision:${id}`) stays unique.

The `w:id` allocator seeds from paragraph `mark_revision.revision_id` and
`prop_change.revision_id` as well as inline revisions, so a new suggestion cannot reuse an
imported id.

### Decision 4: listing, labels, navigation, painting

- `collect_review_revisions` emits, per paragraph: a `paragraph_format` item before the
  paragraph's inline items, anchored over the paragraph; and a `paragraph_mark_insertion` /
  `paragraph_mark_deletion` item after them, anchored zero-width at the paragraph end, where
  the mark is.
- Labels follow Word's reviewing pane: "Inserted paragraph", "Deleted paragraph mark", and
  "Formatted: Centered, Indent: Left 0.5"" built from a paragraph formatting delta, the
  paragraph counterpart of `formatting_delta_json`.
- Next and Previous include both kinds. The current filter that skips any revision with
  empty text now exempts them.
- Painting: a pilcrow marker at the paragraph end for a mark revision, in the author's
  colour (inserted underlined, deleted struck through), and a margin change bar for a
  formatting change — Word's "changed lines" bar.

## Phases

**Phase 1 — foundation and read side.** The operation fix (Decision 1) with exact-inverse
tests; listing, identity, labels, navigation, accept/reject, Accept All and painting for
paragraph revisions that arrive in documents; the allocator fix. User-visible result: Word's
paragraph-level suggestions are visible and decidable, and undo after Backspace is exact.

**Phase 2 — write side (HF-130, HF-131).** Suggesting mode creates the Word shapes in the
second table above: tracked Enter, Delete/Backspace at a boundary, cross-paragraph deletion,
and paragraph formatting through the `apply_paragraph_props_as` choke point. Replaces the 17
refusals.

## Phase 1 as built

Beyond the design above, implementation found and fixed:

- **Caret decisions picked the wrong revision.** A formatting change spans its paragraph and
  lists first, so accepting at the caret inside an inline insertion resolved to the paragraph
  change. `reviewContextAt` now prefers the most specific revision: inline, then mark, then
  paragraph formatting.
- **Every ungrouped marker rendered active on open** (`104` HF-157). Each marker's list of ids it
  answers to held `null` for any revision that is not a move or a group, and `null` is also
  "nothing selected".
- **The formatting bar belongs in the margin.** Drawn beside the paragraph's own text, it sat
  mid-page for centred and right-aligned paragraphs. It anchors to the owning section's text
  column start.

## Test matrix

- Edit crate: split and join are exact inverses **for properties**, with and without
  payloads; Enter moves `mark_revision` to the trailing half; join with `None` keeps the
  first's properties (existing pin).
- Wasm: each row of the Decision 2 table, accept and reject, asserting text, paragraph count,
  surviving properties, a single undo step that restores the original exactly, and the
  exported XML shape; consecutive deleted marks; a deleted mark before a table fails closed;
  item ids are unique when one paragraph has both kinds; the allocator never reuses an
  imported id.
- E2E: a Word-shaped document with a paragraph-mark deletion and a `pPrChange` shows two
  cards with the right labels, Next reaches both, Accept All leaves no revisions, and undo
  restores them.
- Every new guard driven red by reverting the code it covers.

## Open questions (need a Word 365 probe before phase 2 locks)

1. A single-key join of two differently formatted paragraphs: does Word write a `pPrChange`
   on the second paragraph? (ONLYOFFICE does; unverified in Word.)
2. Enter at the end of a heading whose `w:next` is Normal: which mark is inserted, and is a
   `pPrChange` recorded?
3. A second tracked formatting change on a paragraph that already has one: does Word keep
   the original snapshot or overwrite it?
4. Enter inside a paragraph carrying a `pPrChange`: is the snapshot duplicated onto both
   halves? (Phase 1 keeps it on both.)
