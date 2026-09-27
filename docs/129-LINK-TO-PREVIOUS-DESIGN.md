# 129 — Link to Previous: design, and why it is not shipped yet

Status: **design; the control ships read-only.** This is the document
`webapp/src/header_footer_settings.mjs` and `webapp/editor.html` cite as the reason the
"Link to previous" checkbox in Header and footer settings is disabled rather than absent.

The semantics were decided long ago and are not reopened here: **`docs/85` §8.4 and §8d
Q7** adopt Word's model — linkage is `HeaderFooterRef` presence or absence, header and
footer link independently, and each variant (default / first / even) links separately.
This document is about the part §8.4 left as one clause: *"create a new `HeaderFooter`
body (copying the inherited content)"*. That clause is the whole cost, and an audit of
what it actually requires found two things that are not true of the code today.

## 1. What already exists

| Piece | Where | State |
| --- | --- | --- |
| The resolution walk — "which body does this section's variant actually show" | `casual-doc-wasm` `effective_running_ref` | done, and shared with layout |
| The truth reported to the host, per page and per band | `casual-doc-wasm` `running_bands` → `headerLinked` / `footerLinked` | done |
| Point a section's variant at a body, or remove the reference | `Operation::SetSectionRunningRef` (`reference: None` **is** the linked form) | done, self-inverse |
| Mint a body from blocks | `Operation::CreateHeaderFooterBody { region, id, blocks }` | done |
| Remove a body (the inverse vehicle) | `Operation::RemoveHeaderFooterBody` | done |
| **A faithful deep copy of the inherited blocks with fresh node ids** | — | **missing** |

So re-linking (Link to Previous back **on**) needs no new machinery at all: it is one
`SetSectionRunningRef { reference: None }`. **Unlinking is the half that is blocked**, and
it is blocked on one helper, not on the op set. That matters for the extensibility
invariants (`ADR-030` I2, closed op set): shipping this adds **no operation**.

## 2. The first finding: the only deep-copy in the tree is lossy, and silently

There is exactly one "clone a block tree with fresh ids" in the codebase:
`Interop::fresh_block` in `crates/casual-doc-wasm/src/lib.rs`. It is the **structured
clipboard's** reconstructor, and it routes every paragraph's inlines through
`sanitize_inlines`, which is a `match` over four variants with a `_ => {}` arm:

```rust
InlineNode::Run(..) | InlineNode::Tab(..) | InlineNode::Break(..) | InlineNode::Hyperlink(..)
_ => {}          // everything else is DROPPED
```

`InlineNode` has **27** variants at the time of writing. Twenty-three of them fall into that arm. (It is **29** since #637 added the paragraph-spanning field-range markers, so the drop was **25 of 29** by the time it was fixed.) Among them:

* `Field` — which is the **`PAGE` field**. A page number is the single most common thing
  in a header.
* `Drawing` / `AnchoredDrawing` / `EmbeddedObject` / `TextBox` / `Group` — the **logo**,
  which is the second most common thing in a header.
* `Symbol`, `Math`, `NoBreakHyphen`, `SoftHyphen`, `PositionalTab`, `HorizontalRule`.
* `BookmarkStart`/`End`, `CommentReference`, `CommentRangeStart`/`End`, `Revision`,
  `MoveRangeStart`/`End`, `Sdt`, `NoteReference`, `NoteNumberMark`.

`fresh_block` itself handles `Paragraph` and `Table` and **rejects** the third `BlockNode`
variant (`Sdt`) rather than cloning it with duplicate ids — correct for a clipboard, wrong
for this.

**CORRECTION, 2026-09-27 — this paragraph used to say the opposite, and it was wrong.**
It read: *"That is a sanitizer doing its job: a clipboard fragment deliberately arrives as
text, tabs, breaks and links."* It does not. Verified by driving the real methods: the
payload is the model **verbatim**. `copy_structured` filters at BLOCK level only —
`selected.iter().all(|b| matches!(b, Paragraph | Table))` — and then serialises every
inline inside those blocks as it stands, pictures and fields included. Worse, whole blocks
are captured even on a partial selection, so selecting *any part* of a table copies the
whole table with its logo. So `sanitize_inlines` was never a sanitizer: it was this lossy
deep copy already sitting in its first and most-used consumer, and **paste was already
broken** before Link to Previous ever asked for a copy. The genuinely sanitizing path is
the *external* one, `paste_external_structured` / `build_external_block`, which builds from
a small JS `ExternalFragment` contract that cannot express anything but runs, tables and
lists — and which never called `sanitize_inlines` at all.

Measured, in a Rust test over `copy_structured` → `paste_structured` with a table cell
holding a run, an inline `Drawing` and a `PAGE` field: the clipboard JSON carried
`"type":"drawing"` and `"type":"field"`, and the document went from 1 picture and 1 field
to **1 picture and 1 field** where it should have had 2 and 2. Fixed in #645, which closes
`109` HF-191.

The *conclusion* below was right all along, which is why the rest of this section stands.
Reused for Link to Previous the same helper is **silent data loss**, which `AGENTS.md`
lists as a hard rule and not a preference — and silent in the worst available way. The user
turns Link to Previous **off**, which in their mind is "give this section its own copy of
that header", and the copy arrives **with the page number and the logo gone**. Nothing
refuses, nothing reports, and the loss is only visible on the page.

The lesson worth keeping: this document reasoned from what the function's NAME implied
about its caller, and the name was the only evidence for it. A premise about a path nobody
drove is a guess wearing a citation.

**So the copy does not belong in the clipboard sanitizer, and must not be built by calling
it.** It belongs in `casual-doc-edit` beside the ops that consume it, as a structural clone
that preserves every variant and re-mints every `NodeId` — including the ids inside tables
(row / cell / each cell's blocks), inside hyperlinks, inside text boxes and groups, and
inside inline SDTs.

**And it must be total.** An exhaustive `match` with **no catch-all**, so that adding a
30th `InlineNode` variant is a compile error in this helper rather than a new silent drop.
That is the only form of this rule a build can enforce; a catch-all plus a comment is how
`sanitize_inlines` came to drop twenty-five kinds without anybody deciding to.

## 3. The second finding: `docs/85` §8.4's garbage-collection clause is not true

§8.4 says that on re-link "the now-orphaned body is garbage-collected on export". The
exporter does not do this. `casual-doc-export/src/semantic.rs` iterates
`definitions.headers` (and footers) and writes **every** entry as a `word/headerN.xml` part
with a relationship id, whether or not any section references it. There is no
reachability pass.

Consequences to design around rather than discover:

* Toggling Link to Previous off and on *n* times leaves *n* unreferenced header parts, and
  each save writes them all out again. That is unbounded growth driven by a checkbox.
* It is not a *fidelity* error — Word also leaves unreferenced header parts behind, which
  this exporter already relies on for the header-less watermark case — but "Word sometimes
  does" is not a licence for "grows without bound".

Two candidate answers:

1. **Re-link removes the body too**: `SetSectionRunningRef { reference: None }` plus
   `RemoveHeaderFooterBody`, as one action. Exact, and undo already works because
   `RemoveHeaderFooterBody` is documented to carry the removed blocks back out. The risk is
   that the body may be referenced by *another* section, so it needs a reachability check
   at edit time — the check the exporter is missing, moved to the wrong layer.
2. **A reachability pass in the exporter**, skipping bodies no section resolves to. One
   place, and it also cleans up bodies that were orphaned by import or by any future op.
   This is the better shape, and it is a change worth making whether or not Link to
   Previous ever ships.

Either way: **an unlink/re-link cycle must not leave the package larger than it started.**
That is the acceptance criterion.

### 3a. The answer, taken (2026-09-27): the reachability pass

**Answer 2.** `crates/casual-doc-export/src/semantic.rs` now computes
`referenced_running_bodies(defs)` — the union, over `Definitions::sections`, of each
section's `headers` and `footers` refs plus those of every prior snapshot in its
`section_change` chain — and the two part-writing loops skip any `headers` / `footers`
entry outside it. The drop is reported
(`docx.export.header.unreferenced_dropped`, `docx.export.footer.unreferenced_dropped`),
because the bytes leave the package even though no reader ever displayed them, and
`AGENTS.md` makes reported-not-silent a hard rule.

**Complexity: O(sections + parts).** The set is built from the section table with a
bounded number of refs per section (at most one per page type per region) plus the
change-chain depth, and the filter is one set lookup per part. Nothing walks the body,
which is what the constraint in the brief demanded and what answer 1 would also have
satisfied — so complexity did not decide this.

**Why answer 1 was rejected**, in the order the reasons matter:

* It needs the *same* reachability computation anyway. A body may be shared by two
  sections, so "re-link removes the body" is only safe after checking no other section
  resolves to it. Answer 1 is therefore answer 2's computation performed at edit time,
  once per op site, instead of once in the one place that can see the whole document.
  Two implementations of one rule diverge.
* It only covers the op that happens to call it. An orphan can arrive from **import** (a
  source package that already carried an unreferenced header part — Word leaves them), from
  a future op nobody has written yet, or from a snapshot load. Answer 1 leaves every one of
  those leaking; answer 2 holds the invariant regardless of provenance.
* It makes the model refuse a state OOXML permits. An unreferenced running body is legal,
  and a model that cannot hold one cannot represent a file it just opened.
* It couples two ops into one undo unit for a *storage* reason rather than a user-intent
  reason, which is the wrong axis for an undo boundary.

What answer 2 gives up, stated rather than glossed: the orphan stays in memory for the rest
of the session, so an unlink/re-link loop grows the *model*, not the file. That is bounded
by the session and by the ordinary block-count admission limits, and it is the price of
keeping the pass in one place. If it ever matters, the fix is a model-side collector, not a
second copy of the reachability rule in the ops.

**The watermark path, checked before the pass was written.** §3 above notes that this
exporter relies on Word's tolerance of unreferenced header parts for the header-less
watermark case, so a pass that dropped everything unreferenced could have broken it. It
does not, and by construction rather than by luck:

* `watermark_plan`'s `in_header` map is keyed by ids taken from `section.headers`, so a
  header part that carries a watermark is one the section **references** — it is inside the
  reachable set, not outside it.
* the parts invented for a section that has no header of a page type it uses
  (`plan.synthesized`) are **not entries of `Definitions::headers` at all**. They are built
  from `plan`, numbered after the real headers, and carry their own `rIdWm…` relationship
  which `write_section_properties` emits into the `w:sectPr`. The pass cannot see them, so it
  cannot drop them.

Both halves are guarded —
`crates/casual-doc-export/tests/field_and_running_part_fidelity.rs`,
`a_header_less_sections_watermark_still_gets_its_part_beside_an_orphan` and
`a_watermark_in_a_referenced_header_survives_beside_an_orphan` — each with an orphan in the
same document, so the guard fails if the pass either stops running or starts over-reaching.

One non-obvious member of the reachable set: a `w:sectPrChange` prior snapshot's own
`w:headerReference`. The writer emits it into the revision record, so the part it names has
to exist or the package advertises a relationship it does not contain (FID-R-06). Dropping
it was the first shape of this pass, and
`a_running_body_reached_only_from_a_sect_pr_change_snapshot_is_still_written` is the guard
that pins it.

`docs/85` §8.4 has been corrected: it claimed this garbage collection already happened.

## 4. The op sequence

Per band and per variant, one undoable action:

```
CreateHeaderFooterBody { region, id: <fresh>, blocks: <faithful copy of the inherited body> }
SetSectionRunningRef   { section, region, kind, reference: Some(id) }
```

Both already exist, and `CreateHeaderFooterBody`'s own doc comment already anticipates this
exact pairing. The blocks are freshened by the **caller**, so the op stays a data-only,
self-inverse record — the same reason the clipboard mints ids before building its ops.

Refusals, each explicit rather than a silent no-op:

* **First section.** There is nothing to inherit from, so the control is disabled with that
  reason. This is what ONLYOFFICE does — `Common.Utils.lockControls(Common.enumLock.linkToPrevious,
  this._state.SameAs === null, …)` at `apps/documenteditor/main/app/controller/HeaderFooterTab.js:247`,
  keyed on their core returning `null` rather than `false` for the first section — and it is
  what "never a dead control" (`SKILL.md` §10) asks for.
* **Already unlinked.** The section declares the variant itself; there is nothing to copy.
* **Nothing to inherit.** `effective_running_ref` returns `None` — no earlier section
  declares this variant either. Unlinking then means *creating an empty body*, not copying
  one, which is legitimate and must not be confused with a failure.

## 5. Where the live control belongs — and why not in this dialog

In Word and in ONLYOFFICE, Link to Previous lives in the **Header & Footer contextual
surface**, which exists only while a band is open:
`apps/documenteditor/main/app/view/HeaderFooterTab.js:219` (`chSameAs`), on the tab whose
layout is at L82-87 of the same file. That is not decoration. The control's scope is **one
band and one variant** — §8d Q7: header and footer link independently, and each variant
links separately — and "which band and which variant" only has an answer when the caret is
**in** a band.

Header and footer settings opens with the caret in the **body**. There, the question has no
answer, which is why the dialog holds three spinners and two switches that are all
section-scoped, and why the checkbox in it is an **indicator**: it reports
`headerLinked && footerLinked` for the page in view, and a section whose header is
inherited while its footer is its own is a real document that one tick cannot describe.

**Decision.** The live toggle ships in the header/footer editing chrome — the band overlay,
alongside the existing band controls, once per band — and Header and footer settings keeps a
read-only indicator with its reason. That keeps one answer to "which band", the same way
`pageSetupSections` and `sectionLayout` keep one answer to "which section". It also means
this work is gated behind `docs/85`'s band-editing surface and is properly a row there, not
a row here.

## 6. The guards this must arrive with

Stated now so the implementation cannot be finished without them, and every one of them is
about **ink or bytes**, not about a host flag reading back what was written:

1. **The copy keeps the page number.** A header carrying a `PAGE` field, on a two-section
   document; unlink section 2; the header on section 2's pages still inks a page number.
   *Mutation: route the copy through `sanitize_inlines`.* This is the guard that would have
   caught the defect in §2, and the one that must be written first.
2. **The copy keeps a picture.** Same shape, with a drawing.
3. **Editing the unlinked header does not touch the previous section's.** Type into
   section 2's header; section 1's header ink is unchanged. This is the `docs/104` T-01
   class, and it is the assertion that proves the ids were re-minted rather than shared.
4. **Round trip.** Unlink, export, reopen: section 2 declares its own `w:headerReference`
   and section 1 still declares its own. `sample.docx` is the provenance to prefer for what
   Word writes.
5. **One undo.** One press reverses the whole unlink — create and point are one action.
6. **An unlink/re-link cycle does not grow the package** (§3's acceptance criterion).
7. **Totality.** A compile-time guard, not a test: the exhaustive match of §2.

A guard asserting `runningBands().headerLinked === false` is **not** on this list and must
not be counted as one. Four of the five properties in the round that raised this question
were already readable and writable from the engine before any UI existed, so host state is
exactly the thing that proves nothing.

## 7. Sources

Every ONLYOFFICE claim here is from the source tree, not from their documentation, which
`SKILL.md` §1 records as unreliable:

* `apps/documenteditor/main/app/view/HeaderFooterTab.js:219` — `chSameAs`, the checkbox.
* `apps/documenteditor/main/app/view/HeaderFooterTab.js` L82-87 — where it sits in the tab,
  with no separator between it and the two variant switches.
* `apps/documenteditor/main/app/controller/HeaderFooterTab.js:247` — disabled, not hidden,
  when there is no previous section.

Word: Microsoft Support, "Link to previous" and "Configure headers and footers for
different sections of a Word document" — already cited in `docs/85` §8d and not re-derived
here.
