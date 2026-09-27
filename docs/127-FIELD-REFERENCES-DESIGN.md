# 127 — Captions, cross-references, and the field-result contract

**Status:** Design of record for `105` OO-005 (captions and cross-references).
**Opened:** 2026-09-27. **Scope:** `SEQ`, `REF`, `PAGEREF`, `STYLEREF`.
**Companion:** `128` covers the table of contents and table of figures (`105`
OO-001), which consume what this document defines.

## 1. The prior art, stated before any of our own design

None of this is new, and OOXML already specifies all of it. The design decision in
each case is *which established shape to reuse*, not what to invent.

| Thing | What the format already says | Our reading of it |
| --- | --- | --- |
| A caption | A paragraph styled `Caption` holding a literal label run, a numbered sequence field, and the author's own trailing text | Reused verbatim. The `SEQ` **is** the number; there is no second counter anywhere in our model |
| Its number | ` SEQ Figure \* ARABIC `, with `\* ROMAN`/`\* alphabetic` picture switches | Reused verbatim |
| Chapter numbering | ` STYLEREF N \s ` for the chapter, a literal separator, and `\s N` on the `SEQ` to restart it at that heading level | Reused verbatim. `\s` is the whole mechanism behind "2-3"; we add nothing |
| A cross-reference | A `REF` field naming a **bookmark**, with `\p` (above/below), `\r`/`\n`/`\w` (paragraph number), and `PAGEREF` for the page. `\h` makes it a hyperlink | Reused verbatim |
| The bookmark it names | Word writes `_Ref` plus a decimal | Reused, including the prefix, so a file we write and a file Word writes are indistinguishable to a reader |
| A field's value | A cached *result* between `fldChar separate` and `fldChar end`, regenerable from the instruction | Reused. §4 records where we deliberately differ |

Read off `sample.docx` — the one genuinely Microsoft-Word-produced file in this
repository — plus ONLYOFFICE's own vocabulary, which we match so a host that
speaks one speaks both:

- `CaptionDialog.js:472-493` — the control set and its labels.
- `CrossReferenceDialog.js:289-350` — which "insert reference to" entries are
  legal for which reference type.
- `CrossReferenceDialog.js:438-473` — the nine reference kinds.
- `apiBuilder.js:12272-12288` — `AddCaption(sAdditional, sLabel, bExcludeLabel,
  sNumberingFormat, bBefore, nHeadingLvl, sCaptionSep)`, and the note that the
  target "must be in the document (not in the footer/header)".
- `DocumentHolderExt.js:46` — Insert Caption on the picture, table and equation
  **context menu**, which is where a reader reaches for it.

## 2. Where the code lives, and why it is split that way

| Layer | File | Rule it obeys |
| --- | --- | --- |
| Markup construction | `crates/casual-doc-edit/src/references.rs` | Reads **no document**. Every function is O(1) in document size, so none of them can hide a scan, and all of them are unit-testable with no document at all |
| Document reads and commands | `crates/casual-doc-wasm/src/references.rs` | **One walk per command.** Owns the numbering, the target enumeration, and the composition of each undoable action |
| Layout | `crates/casual-doc-layout/src/flow.rs` (one predicate) | §5 |
| Host | `webapp/src/*.mjs` | Takes its vocabulary as input; no engine knowledge beyond the bindings |

The closed op set (ADR-030, invariant I2) stays closed: a caption is
`SetStyleDefinition` + `InsertBlocks` + `SetInlines`, and a cross-reference is
`CreateBookmark` + `InsertField`. **No new operation was added**, so undo/redo,
review tracking and the transaction log all work without knowing this feature
exists.

## 3. Complexity, stated per path

| Path | Cost | Note |
| --- | --- | --- |
| `captionLabels`, `captionEntries`, `staleCaptionNumbers` | O(document) time, **one** whole-document scan, O(captions + headings) memory | |
| `referenceTargets("heading"/a label)` | as above | |
| `referenceTargets("bookmark")` | O(document), one extra walk | Order comes from the walk's own counter; asking a table per bookmark would be O(bookmarks × paragraphs) |
| `referenceTargets("footnote"/"endnote")` | O(notes) | The note definitions are their own map; no document walk |
| `insertCaption` | O(document) — two walks — plus one operation per later caption of the same label | |
| `insertCrossReference` | O(document), plus **one** layout query for `pageNumber` only | |
| `updateCaptionNumbers` | O(document), one walk, one operation per caption that actually changed | |
| **A keystroke** | **One** whole-document scan, at any document size, with or without captions | An owner constraint (`107` §4), pinned by a guard at exactly one |

Two shapes were caught in the first cut of this module, and **neither is a scan** —
worth recording, because `document_scans` catches only one of the two:

1. The scan kept a `(NodeId, order)` row and a nine-word chapter snapshot for
   *every* paragraph so that two questions could be answered afterwards: about
   50 MB of tables on the owner's 1.3M-paragraph file, to look up two nodes. The
   questions are known before the walk starts, so `ScanRequest::interest` names
   them and the walk answers them as it passes.
2. `bookmark_targets` asked that table for each bookmark's paragraph —
   O(bookmarks × paragraphs). **`document_scans` would not have caught it**: it is
   a linear search over a table, not a second scan of the document. Not every
   quadratic in this area is a scan, and a guard that counts scans says nothing
   about one that is not.

## 4. The field-result contract: dirty, not silently stale

Word caches a field's result and leaves it stale until the reader presses F9.
Nothing in the document says it is stale and nothing in the UI says so either.

**We diverge deliberately, and the divergence is recorded in the code** (it is a
doc comment on `staleCaptionNumbers`, per the rule that behaviour differing from
Word is said out loud where a reader of the code will find it):

- Inserting a caption **renumbers the later captions of the same label in the same
  undoable action**. So this engine never *creates* a stale sequence number. One
  undo reverses both the insertion and the renumbering.
- Two things still produce staleness and neither is ours to prevent: a caption
  paragraph deleted with Backspace — ordinary editing, which must **not** trigger a
  document-wide renumber, because that would make a keystroke O(document) — and a
  document another producer wrote, which arrives with whatever its last reader
  cached.
- Therefore staleness is **reportable, and fixable**: `staleCaptionNumbers` names
  each caption, the number it shows and the number that is right;
  `updateCaptionNumbers` puts them right in one action, and refuses with a reason
  when there is nothing to do rather than pushing an empty undo step.

The one result we leave stale like Word does is a **`PAGEREF` page number**. It is
layout-derived; recomputing it is O(document) and must not ride a keystroke, and a
windowed document cannot answer it from a five-page window at all. `128` is where
that has to be solved properly, because a table of contents is made of nothing
else.

`captionEntries` reports the number the document **shows** — the `SEQ` field's
cached result — not the number a recomputation would produce. Reporting the
recomputed number made the caption list structurally unable to disagree with the
model, so a build that cached the wrong number still listed the right one and every
numbering guard passed. That was found by mutating the numbering and watching the
guards stay green.

## 5. The layout change this required, and why it removed a path rather than adding one

A caption is a field plus a sentence, and **a paragraph containing a field did not
soft-wrap.** Any `FlowItem::Field` routed the whole paragraph to
`shape_fielded_paragraph`, which lays out one line per hard break by construction —
it was written for headers and footers, which are single-line in Word. Invisible
while fields lived only in running content; fatal the moment captions exist.
Measured before the change: **one** line for a caption whose text needs two.

The fix is not a second wrapping implementation. A field the post-pagination field
pass does **not** recompute — everything but `PAGE` and `NUMPAGES` — has a cached
result that is already final and that *is* the paragraph's model text
(`append_node_plain_text` already reports it). So it flows as ordinary inline
content through the same recursion a hyperlink's children use. That is one
mechanism where there were two, which is the rule `SKILL.md` §8 states: when a
design needs a parallel path, the abstraction is wrong.

Two further pre-existing defects fall out of the same change:

- a table-of-contents row — title, dot-leader tab, `PAGEREF` — now draws its leader
  dots. The fielded path read `stop.alignment` and `stop.position` and never
  `stop.leader`, so the dots were simply absent. `128` depends on this.
- a field whose cached result carried a symbol, break or drawing no longer loses
  it. The marker's value came from a helper that understood only runs and tabs —
  silent loss (`AGENTS.md`: none) in the one place a reader cannot tell.

`PAGE`/`NUMPAGES` keep their markers: their value is recomputed per page and
corresponds to no model characters, which is what `FieldAnchor::atomic` means.

## 6. Open questions, recorded rather than hidden

1. **`STYLEREF N \s` shows a heading's *ordinal*, not its list number.** We cache
   the count of headings at that level, which equals the number Word shows whenever
   the heading style carries simple decimal numbering — the overwhelmingly common
   case — and differs under a multilevel list. Because `STYLEREF` is a cached
   result, a reader that updates fields shows the true value; ours is a best-effort
   display until then. Resolving a real multilevel number from the model is
   `layout/numbering.rs` work and is not done.
2. **A caption is refused outside the document body**, matching Word and
   ONLYOFFICE. The uniform-flow rule says every container flows content through the
   same pipeline, and it does — this is a *command* restriction, not a flow one, and
   it is the competitor's behaviour. If the owner wants captions in a text box, the
   blocker is that `InsertBlocks` addresses only body containers.
3. **Word's Cross-reference dialog stays open after Insert.** Ours closes. Recorded
   as a deliberate choice in the dialog module.
4. **`\# ` numeric picture switches on `SEQ`** (e.g. ` SEQ Figure \# "00" `) are
   retained on round trip but not honoured when we render the cached number. No
   producer in the corpus writes one.

## 7. Reachability

Per `SKILL.md` §10, every capability is reachable from at least two surfaces and
there is never a dead control. The command ids are the unit of reachability:
`reference.caption`, `reference.crossReference`, `reference.updateCaptionNumbers`.
`webapp/tests/e2e/one-axis-navigation.spec.mjs` enforces the rule over the whole
registry; `layout-references-surface.spec.mjs` enforces ribbon↔palette agreement on
this exact band. Where a command cannot run — no target selected, Viewing mode —
it is **disabled with a reason**, never hidden and never inert.
