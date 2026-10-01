# 150 — `transform`: rebasing one operation over a concurrent one

**Status:** Accepted for implementation (this design ships with the change it describes).
**Opened:** 2026-09-30.
**Implements:** `107` §3 and §6.3 (the T1/T2/T3 transform), ADR-033.
**Decides:** ADR-045.
**Relates to:** `147`/ADR-043 (the envelope this is written against), `45` invariants I1–I4,
`59-V1-EDITING-OP-SET.md`, `24-TRANSACTION-SEMANTICS.md`, `26-SELECTION-FOUNDATION.md`,
`35-DISPOSITION-TAXONOMY.md`, `106` Phase 6.

**Explicitly not in this document:** any network, transport, relay, server, session state
machine, presence or cursors; any change to the operation set; anything in `webapp/`. This
lane writes the rule and proves it converges. Everything that *calls* the rule is `107` 6.6.

> **The caller now exists.** `152` / ADR-047 builds the protocol, the two session state
> machines and the rollback/replay driver that invokes `transform`. It corrects §6's last row
> and §10 Q1/Q2 in place, and adds §10 Q2a. Read it with §6 and §10 below.

**The sibling is the reference.** `opencalc` (`/services/opencalc`) accepted the same
decision in its ADR-011 / `docs/56` and has a working `transform` over a closed op set. The
shape, the vocabulary (`Side`, `subject`/`against`, refuse-rather-than-guess, TP1 as a
property over generated pairs) and the proof strategy are taken from it deliberately. §9
lists what a document forced differently and why.

---

## 1. The established pattern, named before any code

Operational transformation in the **server-ordered** formulation: Jupiter / Google Wave,
which is what Google Docs, Google Sheets and Univer run on. Not a CRDT, not peer-to-peer.
ADR-033 decided this here; `opencalc`'s ADR-011 decided it again independently, and its
`docs/56` states the property that matters most to this project:

> **Single-user needs no server and never has.** OT is dormant at one editor: nothing is
> contacted, no revision log exists, no transform runs, and no per-cell metadata is carried.
> Collaboration is something a document session *acquires*, not a mode the engine is built in.

That is binding here too, because *no mandatory server* is one of this project's three
structural advantages (`106` §3 A1, SKILL §1). §7 states how it is guarded.

The single rule, from the sibling's module doc:

> `transform(a, b)` answers one question: *`a` was written against a state where `b` had not
> happened; what is `a` on a state where it has?*

---

## 2. What makes a document different from a spreadsheet

Both are closed op sets over an addressable state, so most of the sibling's structure
transfers. Four differences change the design.

**1. `NodeId` anchors, not coordinates (ADR-030 I3).** A spreadsheet operation names a cell
by `(row, col)`, so *every* structural edit moves *every* address below it — which is why the
sibling's transform is dominated by band arithmetic and why its refusal surface is about
bands. A document operation names a `NodeId`. Inserting a paragraph moves no other
paragraph's identity, so an operation addressed to a node survives a neighbour's insertion
with no arithmetic at all. **Most of the 55×55 matrix is therefore the identity function, and
that is a property of the address space rather than an optimisation.**

**2. Offsets are paragraph-local.** The only positional collisions are *inside one paragraph*,
between operations that both name that paragraph by id. "Do these two interact positionally?"
is decidable from the two operations alone — no document, no context. This is the hot path
(two people typing in one paragraph) and it is fully answered here.

**3. Every operation carries its inverse (I2).** `casual_doc_edit::apply` returns the inverse
of every operation and `147`'s `Commit` retains it. That is not only an undo mechanism: it is
what makes a **pure** transform possible over operations whose effect depends on state.
`DeleteBlocks { container, index, count }` does not say *which nodes* died; its inverse
`InsertBlocks { .., blocks }` names every one of them. `JoinParagraphs { first, second }` does
not say where the join boundary fell; its inverse `SplitParagraph { at, .. }` does. So
`transform` is defined over a **`Change`** — the operation *and* the inverse `apply` returned
for it — rather than over a bare operation. This is the central design decision.

> The sibling reached the same wall and answered it differently: it passes `SheetNames` and a
> `FormulaTable` in, on the rule *"a pure function is allowed to take more arguments; it is
> not allowed to go and look something up."* We need far less of that, because invertibility
> already carries almost everything. §4 is the one place a document still needs a fact
> neither side carries.

**4. Half the block-structure operations are index-addressed, not node-addressed.**
`InsertBlocks`/`DeleteBlocks`/`InsertTable`/`InsertRow`/`InsertColumn`/`InsertObjectNode`/
`InsertFieldRange` name a *position among siblings*, and `SplitParagraph`/`JoinParagraphs`
name a *paragraph*. Neither can tell where the other sits. **This is the one place the op set
is weaker than I3 promises**, and it is what §4's placement seam exists for. It is recorded as
a finding, not worked around by widening the op set (ADR-030 I2 forbids that).

---

## 3. The shape

```rust
pub enum Side { Earlier, Later }

pub struct Change<'a> {
    pub operation: &'a Operation,   // what was applied
    pub inverse:   &'a Operation,   // what `apply` returned for it
}

pub enum Rebase {
    Keep(Operation),                 // the operation, in the new coordinates
    KeepMany(Vec<Operation>),        // §5.3, §5.8: one intention, several operations
    Satisfied,                       // nothing left to do; nothing lost
    Tombstoned(Tombstone),           // the anchor is gone; MUST be reported
}

pub fn transform(subject: &Operation, against: Change<'_>, side: Side)
    -> Result<Rebase, TransformError>;
pub fn transform_placed(.., placement: &dyn BlockPlacement) -> Result<Rebase, TransformError>;
```

`Side` is, word for word, the sibling's:

> Where `subject` sits relative to `against` in the order the server settled on. … "Whoever is
> transforming" cannot decide it — that is the one fact the two sides disagree about. The final
> order is the shared fact, so it is what the tiebreak reads.

**`Satisfied` and `Tombstoned` are deliberately two outcomes, not one.** A delete whose text
somebody else already deleted has genuinely become nothing and nothing was lost. An operation
whose anchor was removed is a **loss**, and the no-silent-loss rule (AGENTS.md, `35`) says it
must reach the disposition taxonomy. Collapsing them into one "no-op" — which is what the
sibling's empty `Batch` does, because a spreadsheet cell is never *destroyed*, only emptied —
would have made every tombstone silent. `107` §3.1 already required this; the two-outcome
return type is how it is made unmissable rather than remembered.

`TransformError::Unsupported { subject, against, reason }` is the sibling's rule verbatim:

> Deliberately an error and not a best guess: an untransformed op applied to a state it was
> not written against diverges the replicas *quietly*.

### 3.1 The steps

```
transform(subject, against, side):
    1. rewrite × structural  — a whole-paragraph rewrite meeting a split/join     -> refuse
    2. liveness              — did `against` destroy what `subject` addresses?    -> Tombstoned
    3. rewrite × positional  — a whole-paragraph rewrite meeting an edit inside it
    4. positional            — rebase `subject`'s coordinates through `against`   -> Keep / KeepMany
    5. table payload         — the cells a row/column insertion carries
    6. contention            — same target, same aspect: the earlier yields
```

Step 1 is ahead of step 2 on purpose: a join makes the rewritten paragraph *vanish*, which
would otherwise read as an ordinary tombstone and diverge quietly instead of refusing loudly.

### 3.2 Exhaustiveness

**Seven** functions match on `Operation` with **no wildcard arm** — `tier`, `variant_name`,
`coordinates`, `anchors`, `anchor_key`, `footprint` and `effect_of` — plus `set_coordinates`,
the mirror that writes rebased coordinates back. Adding a 56th variant is a compile error in
each, and each is a decision someone has to take. That is `107` exit gate 2, enforced by the
type system rather than by a CI grep, and it is why the per-operation tier classification is a
table in the code rather than being spread across 55 doc comments in another crate where
nothing would check it.

The code is three files, following the sibling's own `transform/` factoring: `transform.rs`
holds the algorithm and its steps, `transform/classify.rs` the four matches that say what an
operation *is*, and `transform/effect.rs` what a committed change *did* — the module the
design turns on, because it is where the inverse is read. Each is under the ~2,000-line rule
(SKILL §10), and the source-scanning refusal guard reads all three, because a guard that
scanned only the parent went blind the moment the module was split.

---

## 4. The one fact two operations cannot tell each other

A `SplitParagraph` adds a block immediately after `at.node`. An `InsertBlocks { container,
index }` adds blocks at `index`. To rebase either over the other, one of them has to know
where `at.node` sits among `container`'s blocks — a fact about the tree that neither operation
carries and no inverse records.

Following the sibling's `SheetNames` precedent exactly, the fact is **handed in**, never
looked up:

```rust
pub trait BlockPlacement {
    /// The container holding `node` and `node`'s 0-based index within it, **in the state
    /// both operations were written against**.
    fn block_position(&self, node: NodeId) -> Option<(Option<NodeId>, u32)>;
}
```

- `transform` uses `NoPlacement`, which answers `None`, so these pairs are **refused**.
- `transform_placed(.., &placement)` answers them.
- `BlockIndex::of(&Document)` walks the body, table cells, block content controls and
  text-box bodies: O(document) to build, O(1) to query.

**The precondition is load-bearing and is the thing most likely to be got wrong above this
layer.** "The state both operations were written against" is the *base*, which neither replica
holds at the moment it transforms: the left one has applied `a`, the right one has applied
`b`. The sibling hit this exact shape and refused the case it could not guarantee
(`rebase_across_rename`'s `old == *name` check). Here, a wrong placement does not error — it
silently mis-shifts an index. A session that cannot produce a base-state placement must pass
`NoPlacement` and take the refusal. Recorded as the first open question (§10).

An incomplete `BlockIndex` walk degrades to `None`, which degrades to a **refusal**, never to
a wrong answer. That is why the seam is safe to ship before every container kind is walked.

---

## 5. The rules, and what they mean

### 5.1 The tie-break, stated once

> At an exact boundary — the same byte offset, the same sibling index, the same insertion gap
> — the operation the server ordered **Later** moves, and the **Earlier** one holds the
> position it asked for.

One sentence, applied to text offsets, sibling indices and block insertion gaps alike. Both
replicas compute the same answer because both are told the same order.

**This is not the caret rule, and the two must not be confused.** `Affinity` (`147` §3.6)
decides where *your caret* lands at a boundary; `Side` decides where *content* lands. They
agree in the case that matters — your own insertion uses `Affinity::After`, so your caret ends
up after your own text — but they answer different questions, and `107` §3.3 read as though
one rule served both. Corrected there.

The distinction the sibling names as *"the classic OT bug"* is the same one, one level down:
where a **thing** goes is not where a **gap between things** goes. An element index and an
insertion position differ exactly at the tie.

### 5.2 Intention, per tier

| Tier | Operations | Rule | Intention |
| --- | --- | --- | --- |
| **T1 · paragraph-local** | `InsertText`, `DeleteText`, `SplitParagraph`, `JoinParagraphs`, `FormatText`, `ClearFormatting`, `SetHyperlink`, `InsertField`, `InsertInlineObject`, `InsertNote`, `CreateBookmark` | Full offset arithmetic within the named paragraph; relocation across a concurrent split or join | *Put my text where I meant it relative to the text around it* |
| **T2a · sibling-indexed** | `InsertRow`, `DeleteRow`, `InsertColumn`, `DeleteColumn`, `InsertTable`, `InsertBlocks`, `DeleteBlocks`, `InsertObjectNode`, `InsertFieldRange` | Index arithmetic inside the named container; `Satisfied` when the band already removed the target | *Put it between the same two neighbours* |
| **T2b · node-addressed** | every property/geometry write, `DeleteTable`, `DeleteObject`, `RemoveInlineObject`, `RemoveField`, `RemoveNote`, `DeleteBookmark`, `RemoveFieldRange`, `ReplaceTable`, `SetInlines`, `UpdateReviewState` | Liveness check, then aspect contention | *Change that object, if it still exists* |
| **T3 · document-scope** | `SetCoreProperties`, `SetStyleDefinition`, `SetEvenAndOddHeaders`, `SetSection*`, `SpliceSectionBoundary`, `CreateHeaderFooterBody`, `RemoveHeaderFooterBody`, `SetSectionRunningRef`, `RenameBookmark` | Last-writer-wins per registry key, in the settled order | *The last person to decide, decides* |

T1 is where nearly all the *traffic* is; T2b is where nearly all the *operations* are and it
needs no arithmetic. That asymmetry is `107` §3.1's tractability argument, and building it
confirmed it.

One correction to `107` §3.1 found by building: `InsertInlineObject`, `InsertNote` and
`CreateBookmark` carry positions but insert **zero bytes** — a drawing, a note reference and a
bookmark marker are all zero-width in the paragraph's projected text, by the edit crate's own
length rule. They are T1 for *anchoring* and inert for *offsets*.

### 5.3 A range is one paragraph, and a split makes it two

`DeleteText`, `FormatText` and `ClearFormatting` name a range **within one paragraph** — the
edit crate refuses `CrossParagraph`. A concurrent `SplitParagraph` inside that range leaves
half in the original paragraph and half in the new one, and no single operation expresses
that.

The sibling's answer to "the result is more than one operation" is `Operation::Batch`. **We
have no `Batch` and may not add one** (ADR-030 I2). We do not need one: a `Transaction`
already carries `Vec<Operation>`, so `transform` returns `Rebase::KeepMany` and the caller
appends them all. No new engine operation; the multiplicity lives in the envelope that already
had it.

`SetHyperlink` is the exception and is **refused** (U1): dividing a link needs two fresh
`NodeId`s and the operation carries one. Minting an id inside a pure function, or reusing one,
are both wrong.

### 5.4 A removal and a span read an insertion at their own boundary differently

This is the pair of rules the property test established rather than confirmed, and getting
either backwards is TP1 failing on the two commonest operations in the set.

- **A removal must not take text somebody typed inside it concurrently.** That is the other
  author's intention, and it is what `apply` produces in the other order. So a `DeleteText`
  whose range contains a concurrent insertion **divides in two** around it — and the *later*
  half is applied first, because removing the earlier half would move the later one's offsets
  out from under it.
- **A span must cover it.** A bold over `abc` while somebody types in the middle bolds the
  typed text too, because in the other order the typing lands inside an already-bold run and
  inherits its properties.

And the span's boundaries are decided by where `apply` **attaches** inserted text, which is
"the first run whose `[start, end]` contains the offset":

| insertion at | inside a span `[s, e)` in the other order? | so the span |
| --- | --- | --- |
| `p < s` | no — it joins the run before the span | shifts whole |
| `p == s`, `s > 0` | no — it joins the run *before* the span | shifts whole |
| `p == s == 0` | **yes** — there is no run before it, so it joins the span's first run | keeps its start, widens its end |
| `s < p < e` | yes | widens its end |
| `p == e` | yes — it joins the span's last run | widens its end |
| `p > e` | no | unchanged |

That table is not a preference. It is `casual_doc_edit::insert_text`'s attachment rule read
back out, and it is the sharpest instance of the sibling's warning that *"a transform that
models the move differently from the way `apply` performs it converges on paper and diverges
in the document."* A change to that attachment rule breaks convergence, which is why the
dependency is named in the code rather than left to be rediscovered.

### 5.5 `FormatText` over concurrently deleted text — `107` §8 Q1, answered

> **The format applies to the survivor.** The range is rebased through the delete; when
> nothing survives, the result is `Satisfied`, not `Tombstoned` — the text the user meant to
> embolden is gone, so there is nothing to report and nothing was lost by *us*.

The converse needs no rule: formatting changes no byte offsets, so a `DeleteText` is
unaffected by a concurrent `FormatText`. `107` §8 Q1 is closed and marked so there.

### 5.6 Whole-paragraph rewrites do not merge, and say so

`SetInlines` and `UpdateReviewState` replace a paragraph's entire inline tree; `ReplaceTable`
does the same for a table. They resolve two ways:

- Against **another rewrite of the same node**: last-writer-wins. The earlier one is
  `Tombstoned`, not `Satisfied` — content was destroyed and the loss is reportable.
- Against a **positional edit inside that paragraph**: **refused** (U8). Applying the rewrite
  first and the edit after puts the typed text into the replacement; applying the edit first
  and the rewrite after throws it away. Yielding either one converges on neither order,
  because the operation applied first in the diamond has already landed and cannot be
  withdrawn. The only convergent answer is a rewrite that *contains* the other edit, which
  means reconstructing its inline list — and a transform that rebuilds one operation's payload
  out of another's is not a rebase.
- Against a **split or join of that paragraph**: **refused** (U7), for the same reason plus
  one more — the rewrite has no way to remove the half the split carved off.
- Against a write to the paragraph's own **properties**: no interaction. They commute, and
  treating every operation that merely names the paragraph as contending would refuse pairs
  that have a perfectly good answer.

This is `107` B4 showing up in a second place. `SetInlines` is an inverse vehicle and a review
mechanism, and it is off the typing path precisely because a rewrite defeats OT granularity.
Under transform it is *also* the most destructive thing in the op set.

### 5.7 Aspects — the sibling's rule, transposed

`opencalc` resolves contention per *aspect* (`content` vs `style`) because "the earlier
operation yields" is too blunt: a concurrent bold and a concurrent typed value should not
destroy each other. A document has more aspects than a cell, so the mask is wider — extent,
group transform, anchor, crop, alt text, fill, stroke, text-box body, paragraph properties,
table properties, cell properties, core properties, style, bookmark name, and one per section
property — and the rule is the same: **the earlier operation yields exactly the aspects the
later one overwrites, and nothing else.** `SetGroupGeometry` claims `EXTENT` as well as
`GROUP_TRANSFORM`, which is why it collides with `SetExtent` and not with `SetAnchor`; that
asymmetry is the whole reason the mask exists rather than a single "same node" test.

Every retained-value write in the op set claims its aspects atomically — there is no operation
that writes "the extent but not the transform" of a group — so a partial yield is
inexpressible and an intersecting pair yields whole, to `Satisfied`.

### 5.8 Run properties are contended per field, over the overlap

Two `FormatText`s over overlapping text are the one place where **positional** operations also
**contend**, and the resolution has to be finer than "the earlier yields":

- Their field masks are compared, not their ranges alone. A concurrent bold and a concurrent
  italic over the same words must both survive; a concurrent bold and a concurrent un-bold
  must not.
- The earlier operation keeps its **full delta outside** the overlap and only the fields the
  later one leaves alone **inside** it. That is up to three operations, which is why
  `Rebase::KeepMany` carries a sequence rather than a pair.
- `ClearFormatting` writes every field and cannot be reduced, so its overlapping part is
  dropped instead.

Over-yielding — dropping the whole overlap — is *not* convergent, and the property test says
so: in the other order the earlier operation's non-conflicting fields survive on top of the
later one's, so the shrunken version has to preserve them.

The delta is reduced by destructuring `FormatDelta` field by field with no `..`, so a twelfth
formatting field is a compile error rather than a field that silently survives a concurrent
write.

### 5.9 The table grid is shared state on two axes — `107` §8 Q4, corrected

`107` called table-geometry races "the most likely place TP1 fails". They are a real hazard,
but not for the reason it gave. The **index arithmetic converges**: `InsertColumn`/
`DeleteColumn` are refused on irregular tables by `apply` itself, so the grid is regular
wherever they apply, and regular-grid arithmetic converges under §5.1's tie-break.

What does not converge is the **payload**. `InsertRow` carries one cell per grid column and
`InsertColumn` carries one cell per row, so a concurrent change to the *other* axis leaves the
operation the wrong shape — and `apply` only refuses an irregular table, so the wrong shape
lands as a table one replica has and the other does not.

- A concurrent **removal** on the other axis drops the matching cell. Exact.
- A concurrent **insertion** needs one more cell than the operation carries, and a cell needs
  fresh identities for itself and the paragraph inside it. A pure transform may not mint
  identities, so the pair is **refused** (U9).

### 5.10 The section-boundary list

`SpliceSectionBoundary { at: Option<SectionId>, boundary }` addresses a **gap** by naming the
boundary it goes before, which is the one index-free insertion in the op set — and it is the
shape the block operations should have had (§2.4). It needs no placement and no arithmetic:
the anchor is a node id, so it is rebased by liveness alone.

---

## 6. The refusal surface, in full

Refusing is a first-class answer. These are the pairs `transform` will not answer, each with
the reason no answer exists. `transform::REFUSAL_REASONS` holds the sentences — sixteen of
them for these ten causes, because U2, U3 and U4 each say something different depending on
which side of the pair asked, and ADR-056 added two more: U5 now says a different thing when the
envelope *declared* the side it meant, and a declared block anchor the target cannot resolve has
a refusal of its own. `the_refusal_surface_is_exactly_these_cases` fails the build two
ways: if a refusal is written as a bare string literal instead of a listed constant, and if a
refusal outside the list reaches a caller.

| # | Pair | Why there is no answer |
| --- | --- | --- |
| **U1** | `SetHyperlink` whose range straddles a concurrent `SplitParagraph` | Two wrappers are needed and the operation carries one fresh `NodeId`. |
| **U2** | A sibling-indexed operation meeting a concurrent `SplitParagraph`/`JoinParagraphs`, with no `BlockPlacement` | §4: neither operation says where the other sits. Answered when a placement is supplied — **and, since ADR-056, answered with no placement at all for an insertion *gap* whose envelope declares a `BlockAnchor`.** It still stands for a removal band, which is a span of victims rather than one anchor. |
| **U3** | `JoinParagraphs` whose two paragraphs a concurrent split or insertion separated | No operation joins non-adjacent paragraphs. |
| **U4** | An inline-index operation meeting a change to the same paragraph's inline membership expressed as a byte offset | The index the run splitting produced is on neither operation. |
| **U5** | `InsertText`/`InsertField` at offset 0 of a paragraph a concurrent join absorbed | The position lands exactly on the run boundary the join created. **Since ADR-056 the envelope can declare the side, and that splits this row in two:** `Affinity::Before` is *answered*; `Affinity::After` is refused because no operation attaches text to the run after a boundary, and an undeclared insertion is refused as before. §9.2, §9.5. |
| **U6** | A block removal whose span covers either paragraph of a concurrent join | After the join, `second`'s text lives inside `first`; removing `first` removes text the other order leaves standing, and no operation removes "the bytes that came from `second`". |
| **U7** | A whole-paragraph rewrite meeting a concurrent split or join of that paragraph | §5.6. |
| **U8** | A whole-paragraph rewrite meeting a concurrent edit inside that paragraph | §5.6. |
| **U9** | `InsertRow`/`InsertColumn` meeting a concurrent insertion on the other axis of the same table | §5.9 — it would need a cell the operation does not carry, and a transform may not mint identities. |
| — | A `Change` whose inverse does not describe what its operation did | `Coalesce::ContinueKeepingFirstInverse` records no inverse (`147` §3.4), so such a commit cannot say what it destroyed. **Corrected 2026-09-30 by `152` (ADR-047): this is a blocker, not an open question.** Such a commit also cannot be *rolled back*, so suggesting mode cannot take part in a session at all. §10 Q2. |

U2 and U6 are the document's analogue of the sibling's "two concurrent line moves whose source
bands overlap" — the refusals a user reaches by ordinary work. Naming that plainly is the
point of this table.

**A refusal is not the only way a merge can fail.** `transform` guarantees that the rebased
operation expresses the right intention, **not** that the model will accept it: the engine's
own invariants (a table keeps at least one row and column, a cell keeps at least one block)
can still refuse it. Two people deleting the two columns of a two-column table concurrently is
exactly that shape — each delete rebases correctly and the second is then refused. **A caller
must treat an `apply` refusal of a rebased operation exactly as it treats `Unsupported`.**

---

## 7. Complexity, and single-user staying free

| Path | Cost |
| --- | --- |
| A single-user keystroke | **unchanged, and provably so.** `transform` has no call site in the engine; `single_user_editing_does_not_call_transform` scans the crate's source and fails the build if one appears. |
| `transform(subject, against, side)` | **O(1)** in document size. The only unbounded term is the walk over `against.inverse`'s retained payload when the concurrent change removed content, which is O(nodes that change removed) — bounded by what the concurrent operation itself carried, never by the document. |
| Rebasing one arriving operation | O(operations since its base revision) transforms — `107` B2, with nothing hidden inside each one. |
| `BlockIndex::of(&Document)` | O(document blocks), **once per rebase**; a caller that maintains one incrementally pays nothing per operation. Building it per arriving operation would violate B2; §10 Q1. |

**Guarded by doubling, never by a clock** (SKILL §8).
`transform_cost_does_not_scale_with_the_document` builds documents of *n* and *2n* blocks,
transforms the same pair against each, and asserts the nodes the transform reads do not grow.
The same test then asserts that `BlockIndex::of` — which genuinely is O(document) — *does*
roughly double, because a measurement that cannot see growth is not a measurement.

---

## 8. The property that is proven, and the one that is not

**TP1 is established.** For concurrent `a`, `b` over a shared state `S`:

```
apply(apply(S, a), transform(b, a, Later)) == apply(apply(S, b), transform(a, b, Earlier))
```

Proven as a **property over generated concurrent histories**, not by example:
`tp1_holds_for_every_supported_pair` generates operations clustered on the same paragraphs,
offsets, containers and indices so that ties, containments and overlaps are hit on purpose
rather than by luck, and runs every ordered pair through the diamond. Current numbers:
**3,153 pairs answered and compared, 76 refused, 2 refused by a model invariant, 0
divergent.** The refusal count is capped relative to the answered count and the answered count
has a floor, so the property cannot pass by skipping.

**What the comparison quotients out, and nothing else.** `apply` mints identities when an edit
splits a run, so two orders mint different *values* for the same logical run and no transform
can fix that — the identity is not on the wire. The comparison therefore canonicalises **run**
identities by document-order rank and is exact on everything else: every block id, every
object id, every property, every byte of text, and the order of all of it. Nothing an
operation *carries* is quotiented, and a run relabelled into a different position still fails,
because the rank is the position. §9.3 records the finding.

**TP2 is not established, and is not needed.** TP2 is what peer-to-peer OT requires. A server
imposes the total order (ADR-033, `opencalc` ADR-011), so no operation is ever transformed
against two different histories and the obligation does not arise. This is stated rather than
assumed, and it is the trade that keeps the transform set small. **If peer-to-peer or
merge-after-long-divergence is ever required, this design is void and needs a new ADR**, not
an extension.

### 8.1 Every guard driven red

Per SKILL §4, each load-bearing guard was proven able to fail before it was trusted:

| Mutation | What went red |
| --- | --- |
| Invert the insertion tie-break (`Later` holds instead of moving) | `concurrent_insertions_at_one_boundary_order_by_the_settled_order` **and** TP1 |
| Make a removal swallow a concurrent insertion instead of dividing around it | TP1, on `InsertText@3` vs `DeleteText[2,6)` |
| Disable the liveness check | `a_removed_anchor_is_a_tombstone_and_not_a_silent_no_op`, `an_anchor_nested_inside_removed_content_is_also_a_tombstone` **and** TP1 |
| Write a refusal as a bare literal instead of a listed constant | `the_refusal_surface_is_exactly_these_cases` |
| Make the transaction engine call into `transform` | `single_user_editing_does_not_call_transform` |

---

## 9. What was taken from the sibling, and what a document forced differently

**Taken directly:** `Side`/`Earlier`/`Later` and its `flip`; `subject`/`against` argument order
and meaning; `TransformError::Unsupported` carrying both variant names; refuse rather than
guess; the refusal surface pinned by a test; contention resolved per aspect with the earlier
yielding only what the later overwrites; TP1 as a generated-pair property with a clustered
seed, a skip cap and an answered-count floor; and the rule that transform and `apply` must
share one normal form.

**Forced differently by a document:**

1. **`Change`, not `Operation`.** The sibling transforms bare operations and passes side tables
   for what they cannot say. Our deletes name victims by identity rather than by address, so
   the *inverse* is the side table, and it is already on every commit. §2.3.
2. **No `Batch`, so `KeepMany`.** §5.3.
3. **`Tombstoned` as a distinct outcome.** A cell is emptied; a node is destroyed. The
   sibling's single `noop()` would have made every anchor death silent here.
4. **The matrix is mostly identity**, because addresses are node identities rather than
   coordinates. The sibling's dominant case is our rare case. §2.1.
5. **A removal divides, a span widens.** The sibling has no operation that spans a contiguous
   range of *content* whose middle another operation can add to. §5.4.
6. **Run properties contend per field over an overlap** — three pieces where a spreadsheet has
   one cell. §5.8.
7. **The table grid is two axes with a carried payload**, where a spreadsheet's rows are
   interchangeable and empty. §5.9.

### 9.1–9.3 Three findings for the owner

These are consequences of the op set, not of the transform, and each would need an op-set
change (ADR-030 I2) to close. They are reported, not taken.

1. ~~**Block operations are index-addressed, not node-addressed.**~~ **Closed for insertion
   gaps 2026-10-02 — §9.5, ADR-056. Still open for removals.** `InsertBlocks`/`DeleteBlocks`/
   `InsertTable`/`InsertFieldRange` name a sibling index. That is the *only* reason §4's
   placement seam exists, and the only reason a paste concurrent with an Enter needs a fact
   about the tree. A node-anchored form ("before this block") would remove the seam entirely —
   and it does, carried on the **envelope** rather than in the operation, so no variant changed.
   **`DeleteBlocks` is deliberately not covered:** a removal band is not one anchor but a *span
   of victims*, and the node-addressed form of that is a list the operation does not carry.
   **Refusal U6 therefore stands**, and so does the placement seam for it.
2. ~~**`Pos` carries no affinity.**~~ **Half closed 2026-10-02 — §9.5, ADR-056.** `Affinity`
   exists on the envelope's `Position` and not on the operation, so an insertion at a run
   boundary cannot say which side of it the text belongs to. That is refusal U5. The declaration
   now travels on the envelope, so the *intent* is known — and one of its two values is
   expressible and one is not. `Affinity::Before` is answered; `Affinity::After`, which is the
   commoner intent, is still refused because **no operation in the set attaches text to the run
   after a boundary**. The remaining half is an op-set question again, and a narrower one.
   §5.4's table is **unchanged** and still reads `apply`'s attachment rule: content order is
   decided by that rule and the settled order, and a declared affinity does not and must not
   override it — it says what the author *meant*, which is a different question from what
   `apply` *does*.
3. ~~**Operations do not carry the identities they cause to be minted.**~~ **Closed** — see
   §9.4 and ADR-051. `apply` no longer holds an id generator at all; an operation travels with
   the *space* it mints in, and every replica applying it names the nodes it creates
   identically.

### 9.4 How §9.3 was closed, and why not the way §9.3 proposed

§9.3 proposed "the operation carrying the new run's id, as `SplitParagraph` already carries
`new_id`". **That form cannot work, and the reason is structural rather than a matter of
effort.**

The *number* of identities an operation mints depends on the document it lands on.
`ensure_run_boundary` mints one id, or none, according to whether a run straddles the offset;
`split_inlines` mints one per nested wrapper it has to divide. And an operation is
**transformed** before a remote replica applies it, so the state it lands on there is not the
state its author saw — §5.4 and §5.8 both change where a range begins. An enumeration is a
count fixed at authoring time; the truth is a count discovered at application time. There is
no enumeration an author could write that would still be right on the receiver.

**A space is the only declaration that survives the transform.** So the unit carried is a
`Mint`: a private, aligned run of `NodeId` counters. `apply(document, mint, op)` derives every
identity it creates from it, in order, and holds no generator — which makes "this function
cannot name a node the operation did not pay for" a **compile-time** property rather than a
review rule, and makes `apply` a pure function of `(document, mint, operation)`.

The established pattern is named in `casual-doc-edit`'s `mint` module: **deterministic
identity derived from the creating operation**, which is Yjs's `(client, clock)` and
Automerge's `(actor, counter)`. `casual_doc_model::IdSpace` already partitions the *namespace*
half of a `NodeId` per participant (§4.2 of `152`); this partitions the *counter* half per
operation, and the two compose — an id minted inside a lane is still in its author's
`IdSpace`, so `WireOperation::localise`'s space check extends to the mint with no new
vocabulary.

| Level | Width | What it separates |
| --- | --- | --- |
| namespace (`IdSpace`) | 2⁶⁴ | participants, and the document's own and offline spaces |
| block | 1024 counters | one operation from the next |
| lane | 128 counters | an operation from its inverse, and a `KeepMany`'s pieces from each other |

Three consequences worth stating plainly:

- **`Mint::inverse` is an involution**, so undo followed by redo re-mints exactly the
  identities the original edit created. A comment anchored to a run an undo destroyed finds
  *that* run when the redo brings it back, rather than a stranger in its place.
- **A lane is a bound, not a hope.** An operation wanting more than 128 identities — about
  thirty times the worst case in the set — earns `EditError::IdExhausted`. It does not wrap
  into the next lane.
- **Where an author reserves is now the same place a transaction is built**, and there is
  exactly one such place in the facade, pinned by
  `every_document_mutation_is_a_transaction`. Identity enters a document through the choke
  point or not at all.

**What this does *not* make true.** The TP1 diamond in `transform_tests` still compares
documents modulo run ids, and that is correct rather than a leftover: the two sides of a
diamond apply *different operation sequences* (one applies `a` whole, the other applies the
pieces `a` rebased into), so they have no reason to mint alike. Convergence with identity is a
property of the **protocol**, where every replica applies the same `(mint, operation)` pairs in
the relay's order — and that is where it is now asserted, with no quotient, by
`a_run_an_edit_split_off_carries_the_same_identity_on_both_replicas` and
`a_replay_over_a_remote_edit_leaves_both_replicas_naming_every_node_alike` in `152`'s session
suite.

   **Updated 2026-10-01 — narrowed, not closed.** This section had two causes tangled
   together, and one of them is gone. Until the identity partition landed (`152` §4.4,
   ADR-047) the live editor minted in a **document-derived** namespace, so two replicas did
   not merely assign different ids to the same run — they assigned the **same** id to two
   different nodes, from the first edit. That is fixed: every identity the editing path mints
   now comes from a per-participant `IdSpace`, so a collision is impossible rather than
   unlikely.
   What remains is exactly the sentence above, and it is now the *whole* blocker: an
   operation that mints without declaring what it minted leaves two replicas replaying one
   ordered log with different ids for the same run, so **snapshot verification by replay
   across replicas is still not possible**. `WireOperation::introduces` already enumerates
   the ids an operation declares in its own fields; what is missing is the undeclared mints
   inside `apply`. Recorded as `152` §10 Q8.

### 9.5 How §9.1 and §9.2 were closed, and why not the way they proposed

**ADR-056.** Both proposed an operation-set change and neither needed one, for the same reason
ADR-051 found one increment earlier: **neither fact changes what `apply` does.** An index is
what `apply` consumes and it is already exact on the author's own replica. An affinity is not
consumed by `apply` at all — the attachment rule in §5.4 is fixed, and changing it would break
convergence. Both are **authoring** facts: true of the moment the operation was written, needed
only by a *transform*, and needed only on a replica that did not write it. A receiver cannot
reconstruct one, so it must travel; `apply` must not read one, so it must not be in the
operation. That is exactly the shape of an envelope field.

So an `Intent` travels beside each operation, one per operation, in the same order `Mint`
already uses:

| Declaration | What it says | Read by |
| --- | --- | --- |
| `BlockAnchor::Before(NodeId)` / `AtEnd` | the block the index was counted to | `transform_declared`, `resolve_anchor` |
| `Affinity::Before` / `After` | which side of a boundary the content belongs to | `transform_declared` |

**An anchor is rebased by doing nothing.** A split carves out a new block and a join destroys
one; neither *renames* the anchor, so "put it between the same two neighbours" survives
untouched and `transform` returns the slot unchanged. The index is re-derived **once**, by
`resolve_anchor`, immediately before the operation is applied.

**Why the resolution is not inside `transform`, which is the part worth reading twice.** An
arrival is rebased over *every* commit since its base revision. A resolution performed inside
one transform step would be against an intermediate state and would then have to be rebased by
the next step — which is the arithmetic the anchor exists to remove. The anchor is invariant
across the whole sequence, so there is exactly one correct moment to resolve it, and it is the
last one. That is what keeps `transform` pure: two operations and a settled order, never a
document.

**And it inverts §4's hardest precondition.** `BlockPlacement` must describe the *base* state,
which §4 calls the thing most likely to be got wrong above this layer because **neither replica
holds it** while transforming. `BlockTarget` describes the state the caller is *about to
mutate*, which every caller holds by definition. Same shape, opposite precondition: one is a
promise that is hard to keep, the other is a fact in hand. They are two traits over one
`BlockIndex`, and the names are what stop the wrong one being passed.

**Measured before it was chosen.** Taking §9.2 in the op set was tried first. Adding one field
to `Pos` fails the build with two `E0063` in `casual-doc-wasm` (`lib.rs:2311`, `lib.rs:5712`) — a
file another lane held — which is the "two green PRs can make `main` red" shape the working
contract records. An envelope field is additive instead: a transaction that declares nothing
behaves exactly as it did, so the change is reversible, and if the op-set change is ever taken
the declaration becomes redundant rather than wrong.

**What this unblocks.** The op shapes are now final, which is what `152` §9 required before a
byte codec could be written — the only thing standing in front of 6.4, and therefore in front of
6.1 and 6.6.


---

## 10. Open questions — recorded, not hidden

1. ~~**Base-state placement.**~~ **Closed 2026-09-30 by `152` §5.3 (ADR-047).** The question
   was how a session supplies a placement resolved against the *base* state when "neither
   replica holds that state at the moment it transforms". A **rollback** driver does: rolling
   its unordered commits back to the horizon *is* arriving at that state, so
   `BlockIndex::of` is built there and nowhere else and the precondition is met by
   construction rather than by a caller's promise. The cost is one O(document) walk per
   **contended** arrival — never per keystroke, never on an uncontended one — and `152`
   §10 Q1 records maintaining it incrementally as the way to pay less. Callers that are not
   the driver still pass `NoPlacement` and take the refusal.
2. ~~**Commits without inverses.**~~ **Not an open question — a blocker, recorded 2026-09-30
   by `152` §5.3 (ADR-047).** `Coalesce::ContinueKeepingFirstInverse` (`147` §3.4) drops a
   commit's inverse operations to keep review typing bounded. This section said such a commit
   "cannot serve as `against`", which understates it: it also **cannot be rolled back**, and
   rollback is how a replica rebases its own unacknowledged work. So **suggesting mode cannot
   take part in a session at all** until `147`'s envelope retains inverses and drops them at
   undo-read time instead. The session refuses with a stable code
   (`SessionError::NotRollbackable` → `ODC-7001`) rather than diverging, and
   `an_unordered_commit_that_kept_no_inverse_cannot_be_rolled_back` is proven red against the
   envelope change that would close it.
2a. **Rebasing a sequence needs more than one inverse.** Found while building `152`: to rebase
   the *second* unordered commit, the arrival has to be seen as it would be after the first,
   and its inverse there is the composition `p₁'⁻¹ ∘ R⁻¹ ∘ p₁` — three operations, where
   `Change` carries one. Rollback supplies the inverse for one step, not for a sequence. The
   driver therefore **probes**: it applies the arrival's image at each base state the rollback
   reveals, keeps the inverses, and applies them straight back, which rests only on the
   invariant undo already rests on. `152` §5.3.
3. **Collaborative undo.** §11.
4. **Tracked changes.** `107` §8 Q3 — whether operations are transformed before or after
   revision wrapping — is untouched by this lane and still open. `transform` sees whatever
   operations the suggesting path produced; if that path wraps first, the wrapped operations
   are what converge, and authorship is preserved by construction. That needs stating in
   `86`/`133` before collaboration ships.
5. **Log format stability.** `107` §8 Q5 is unchanged, and now has a second surface: transform
   depends on the **inverse**, so the inverse is a compatibility surface too.
6. **Node-addressed mapping steps.** `107` P-4 remains open. `transform` does not need
   `MappingStep`s — it reads operations directly, which is strictly more information — but
   **selection** rebasing does, and a remote edit that deletes the node your caret sits in
   still has no defined behaviour.
7. **`SetHyperlink` against concurrent formatting.** Not exercised by the current generator,
   because the seed carries no hyperlinks. Its `apply` rebuilds a paragraph's inline tree, so
   it is the most likely remaining place a pair diverges. Extending the seed is the next
   increment.

---

## 11. What this transform commits us to about collaborative undo

`opencalc`'s `docs/69` settles collaborative undo for a spreadsheet and is the reference. Undo
is not implemented here, but transform constrains it, and these are the commitments:

1. **An undo is an ordinary operation.** `147` already made undo "apply the inverse as a new
   transaction", so an undo submits like an edit and is transformed like an edit. Nothing on
   the wire says "this is an undo". The sibling states the consequence exactly: *"the refusal
   is a local decision taken before submitting … the server deliberately does not [know]: it
   orders operations and does not interpret intent."* Our `Origin::Undo { group }` is local
   state and must stay local state.
2. **The sibling's split applies, with the line drawn differently.** `69` decides *cell edits
   clobber, structural edits refuse*. The document analogue of "clobber" is §5.7's aspect
   yield — undoing a property write you no longer own resolves last-writer-wins, and the peer
   has their own undo. The document analogue of "refuse" is an undo whose inverse would
   **remove a node that now holds other people's work**: the inverse of a paste is a delete,
   and undoing your paste after somebody typed into it destroys work no undo stack contains.
   That must be refused loudly, before submission, exactly as `69` says.
3. **Transform makes the check possible and does not perform it.** `transform` is pure and sees
   one pair; "is this band still empty of other people's work" is a question about the document
   and about authorship. It belongs in the session.
4. **`Tombstoned` is the primitive the policy needs.** An undo whose inverse tombstones is
   precisely the case `69` refuses. Having it as a distinct outcome rather than a silent no-op
   is what will let that policy be written without a second mechanism.
5. **Undo inherits the refusal surface.** An undo of a paste is a `DeleteBlocks`, so it meets
   U6 and U2 like any other removal; an undo of a formatting change is a `SetInlines`, so it
   meets U7 and U8. **Collaborative undo is therefore the heaviest user of the refusals in
   §6**, which is a reason to close §9.1 before building it rather than after.
6. **Redo is not the inverse of this.** `69`: *"A redo is a new intention … it travels as an
   ordinary operation and lands wherever the transform puts it."* `147`'s `Origin::Redo`
   already models it that way.

---

## 12. Corrections to `107`

Made in place, as `147` did.

| `107` said | Building it found | Where |
| --- | --- | --- |
| §3.1 tier table, 9 T1 operations over a 47-op set | 11 T1 operations over 55; three of them carry positions but insert **zero bytes** | §5.2 |
| §3.2: `MappingStep` must be generalised before transform can be written | `transform` does not read mapping steps at all — it reads the `(operation, inverse)` pair, which is strictly more information. P-4 is still needed for **selection** rebasing, a different consumer | §2.3, §10.6 |
| §3.3: "`Affinity` already decides caret behaviour at that boundary; the same rule must decide *content* order" | Two different questions. Content order is decided by the settled order (`Side`); `Affinity` decides the caret. They agree where it matters but are not the same rule — and the operation carries no affinity at all, which is finding §9.2 | §5.1, §9.2 |
| §8 Q1: formatting vs deletion "needs a stated rule" | Stated: the format applies to the survivor; an empty survivor is `Satisfied` | §5.5 |
| §8 Q4: table-geometry races are "the most likely place TP1 fails" | Half right. The index arithmetic converges; the **carried payload** does not, and that is a different fix | §5.9 |
| §3.1: a tombstoned operation is "dropped with a reported outcome" | Right, and it needs a return type that cannot be confused with "nothing to do". Two outcomes, not one | §3 |
| §9 exit gate 2: "every operation is tier-classified in its own doc comment" | Done as one exhaustive table in `transform.rs` instead. A doc comment in another crate is not checkable; an exhaustive match is a compile error | §3.2 |
