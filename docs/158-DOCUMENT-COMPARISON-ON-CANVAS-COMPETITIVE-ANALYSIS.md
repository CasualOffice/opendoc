# 158 — Document comparison on the canvas: how Word, ONLYOFFICE and Google Docs present a diff

**Status:** Source findings, 2026-10-04. The decision it feeds is **ADR-061**. It changes no
behaviour on its own: the implementation it argues for is an engine addition
(`crates/casual-doc-wasm`) outside the lane that wrote this, and §7.4 gives its exact
signature so it can be routed.
**Corrects:** `crates/casual-doc-diff/src/lib.rs:47-50` and `record.rs`'s `DiffAnchor.node`
doc comment, both of which claim a host can use the anchor to scroll **a live document** to a
change. It can scroll a *preview session* that opened the same bytes; the live document's ids
come from a different import. §7.3, measured.
**Opened:** 2026-10-04.
**Evidence base:** `/Users/sachin/Desktop/melp/reference/sdkjs` at
`72b0421c0bbf9d01eed9cf14834ae47eb2df1b50` and
`/Users/sachin/Desktop/melp/reference/web-apps` at
`9c0ca538c3b211052347df09d2a4d6781f023403` — both "Merge branch release/v9.4.0 into master",
2026-05-19, the same checkouts `157` cites. **AGPL-3.0: behaviour and structure only, no code
taken.** Every ONLYOFFICE claim below is tied to a path and an identifier and was read
directly, not accepted from a report (`SKILL` §6, §9).
**Occasioned by:** an owner report that Compare and Show changes are unusable — the panel
reports `Differences: 1 / Text edits: 1 / Removed` and the reader cannot see *what* changed or
*where*.
**Relates to:** `153` `review.compare-documents`; `140` §8-13; `139` §18 (version capture);
`104` (defect queue).

> **What this concludes, before the evidence.**
>
> 1. **ONLYOFFICE's Compare does not produce a summary. It produces tracked changes in the
>    document on screen.** `CompareBinary` loads the other document into a throwaway
>    `CDocument` and runs `CDocumentComparison.compare()`, which mutates the *open* document,
>    setting `reviewtype_Add` / `reviewtype_Remove` on runs with a `reviewInfo` carrying an
>    author and a timestamp. There is no change-count surface anywhere in the feature. §2.
> 2. **The canvas rendering is therefore not a second mechanism — it is the review markup they
>    already had.** Insertion is an underline in the author's colour, deletion a strikethrough
>    in the author's colour, and a move is the double-line variant of each — one function,
>    `ParagraphLineDrawState.addLines`, decides all of it from the run's review type. §3.
> 3. **Compare lives *on the Review band*, between Accept/Reject and the display-mode
>    control.** That is not cosmetic. It is the admission that a comparison's output is review
>    markup, so every review gesture already applies to it: accept, reject, previous, next, and
>    four display modes. §4.
> 4. **Their change entry names the object; ours does not.** Theirs reads
>    `"<b>Deleted:</b> <the actual removed text>"`. The owner's report quotes ours as
>    `Removed`. That is the same defect class as a refusal with no reason (`SKILL` §10). §5.
> 5. **Authorship and time are shown in a balloon anchored at the change on the canvas**, not
>    in a header line — the engine pushes `asc_onShowRevisionsChange(sdkchange, isShow)` with
>    document coordinates and the chrome positions a popover there. §4.3.
> 6. **Nobody in this comparison draws a change bar in the margin except Word.** ONLYOFFICE
>    has no per-paragraph change-bar drawing at all. We already have a review gutter, so this
>    is a place we are *ahead*, and §6 says not to spend it. §3.3.
> 7. **Their Compare refuses in co-editing and demands existing changes be resolved first** —
>    two preconditions our design will meet or must consciously decline. §2.4.
> 8. **Our half of it is already built except for one function.** The `Revision` model,
>    author-coloured underline/strikethrough painting, accept/reject, next/previous and a
>    markup toggle all ship today. What is missing is the ability to *inject* a revision from
>    outside the engine — and, separately, the sidecar's anchor does not address the live
>    document at all, which is why the present panel is the most the chrome can currently say.
>    §7, measured in this branch.

---

## 1. Why this document exists

The owner's report is three defects, and only the first is a design question:

1. the diff is a **count**, not a diff — the reader cannot see what changed or where;
2. a **version is saved even when nothing changed**;
3. the panel's wording (`Removed`, with no object) tells the reader nothing actionable.

(2) and (3) are small and have no competitive content worth arguing about — (2) is a guard, (3)
is §5 below. (1) is the one that needs a reference, and the owner named the references:
*"check how onlyoffice or google docs does it"*. So this document answers that first, before
any approach is proposed, per `SKILL` §8 — *name the known pattern before inventing an
approach*.

The named pattern turns out to be the strongest possible one: **there is no new pattern.** A
document comparison is a tracked-change set, and every editor in this comparison renders it
with the review machinery it already shipped for human suggestions. The interesting finding is
not how to draw a diff; it is that drawing a diff is not a thing any of them do.

---

## 2. ONLYOFFICE: Compare emits review markup into the open document

### 2.1 The entry points

Four, all converging on one function:

| Entry | Path |
| --- | --- |
| `asc_CompareDocumentUrl(sUrl, oOptions, token)` | `sdkjs/word/api.js:13031` |
| `asc_CompareDocumentFile(oOptions)` | `sdkjs/word/api.js:13036` |
| `asc_CompareDocumentUrl_local` / `..._File_local` (desktop) | `sdkjs/word/api.js:13056`, `:13120` |
| `window["onDocumentCompare"]` (desktop callback) | `sdkjs/word/api.js:13273` |

All of them reach `asc_docs_api.prototype._CompareDocument` (`sdkjs/word/api.js:13256`), which
converts the other document to their binary intermediate and calls
`AscCommonWord.CompareBinary(stream, oOptions)`. Note the shape: the *other* document arrives
as **bytes**, and the open document is taken from the live session. That is the same split our
own facade already has.

### 2.2 What `CompareBinary` does — the load-bearing finding

`sdkjs/word/Editor/Comparison.js:3864`, `function CompareBinary(sBinary2, oOptions,
bForceApplyChanges)`:

1. `const oDoc1 = Asc.editor.WordControl.m_oLogicDocument` — **the document on screen**.
2. Reads the other side into a throwaway document: `new CDocument(...)` plus
   `new AscCommonWord.BinaryFileReader(oTempDocument, openParams)` with
   `openParams = {disableRevisions: true, noSendComments: true, noGenerateSmartArts: true}`.
   `disableRevisions: true` is worth noticing — the revised side's *own* tracked changes are
   discarded on the way in, so the comparison starts from a clean right-hand side.
3. Constructs `AscCommonWord.CDocumentComparison(oDoc1, oDoc2, oOptions)` and calls
   `compare()` on it.

`CDocumentComparison.prototype.compare` (`Comparison.js:2642`) then **mutates `oDoc1`**: it
opens a history action `AscDFH.historydescription_Document_CompareDocuments`
(`Comparison.js:2666`), enters `Start_SilentMode()`, merges the revised document's numbering
into the original, and walks `compareRoots(oOriginalDocument, oRevisedDocument)` and
`compareSectPr(...)`.

**There is no result object.** `compare()` returns nothing a caller could render. The output of
a comparison *is the document*.

### 2.3 The changes it writes are review changes, with an author and a date

`CReviewChange` (`Comparison.js:157`) is the unit. `CReviewChange.prototype.applyStart`
(`Comparison.js:179`) reads:

- `const oReviewInfo = oPartner.reviewInfo;`
- `const sReviewUserName = oReviewInfo.GetUserName();`
- `const sReviewDate = oReviewInfo.GetDateTime();`

and accumulates runs into `oNeedReviewWithUser[sReviewDate][sReviewUserName].reviewTypes[...]`,
keyed by `reviewtype_Add` and `reviewtype_Remove` (`Comparison.js:209-212`). Those two
constants are the ordinary tracked-change types: `sdkjs/word/Editor/Run.js:48` defines
`var reviewtype_Add = 0x02;` — the same constant the typing path uses when track-changes is on
(`Run.js:218`, `NewRun.SetReviewType(reviewtype_Add)`; `Run.js:882`,
`var DstReviewType = true === TrackRevisions ? reviewtype_Add : reviewtype_Common;`).

So a comparison difference and a human suggestion are **the same object**, carrying the same
author and timestamp fields. A move is the same with `moveReviewType` set to
`Asc.c_oAscRevisionsMove.MoveTo` / `MoveFrom` and a mark name
(`CReviewChange.prototype.setMoveReviewType`, `Comparison.js:262`).

Three sibling collectors run alongside, which is the evidence that comparison covers more than
text: `CTextPrChangeCollector` (`Comparison.js:672`) emits formatting changes,
`CBookmarkChangesCollector` (`Comparison.js:326`) bookmarks, `CCommentChangesCollector`
(`Comparison.js:353`) comments.

### 2.4 Two preconditions they enforce, and we should decide about deliberately

Both are in `CompareBinary`:

- **Refused during co-editing.** `Comparison.js:3869-3877`: if
  `oCollaborativeEditing && !oCollaborativeEditing.Is_SingleUser()` it sends
  `Asc.c_oAscError.ID.CannotCompareInCoEditing` and returns. Comparison rewrites the whole
  document under one history action; that cannot be interleaved with other people's edits.
- **Existing tracked changes must be accepted first.** `Comparison.js:3910-3921`: if
  `oDoc1.TrackRevisionsManager.Have_Changes()` or the revised side had revisions, it raises
  `asc_onAcceptChangesBeforeCompare` and only proceeds on consent — and `compare()` then calls
  `LogicDoc.AcceptRevisionChanges(undefined, true)` for every logic document holding changes
  (`Comparison.js:2670-2678`). The reason is structural: the comparison's output occupies the
  same `reviewType` field, so a pre-existing suggestion and a computed difference would be
  indistinguishable and would accept/reject each other.

The second one is the sharpest constraint on any design that routes a diff through the
tracked-change model. It is not a detail; it is the cost of using one mechanism for two
meanings. §6 returns to it.

Also noted, because it bears on where comparison may run: `sync_StartAction(...
BlockInteraction, SlowOperation)` wraps the whole of `CompareBinary`
(`Comparison.js:3878`). **ONLYOFFICE blocks the UI for the duration of a comparison.** They
do not have a cancellable, progress-reporting comparison. That is a bar we are already above
and must not fall to (`SKILL` §8 — anything O(document) off the main thread, with progress, and
cancellable).

### 2.5 The options surface

`function ComparisonOptions()` (`Comparison.js:2103`) carries `textBoxes`, `tables`, `words`,
`headersAndFooters`, `footNotes`, `caseChanges`, `formatting`, `whiteSpace`, `comments`,
`insertionsAndDeletions`, with `moves` and `fields` commented out. The only one the UI exposes
is word-vs-character granularity: `controller/ReviewChanges.js:699`,
`this._state.compareSettings.putWords(!Common.localStorage.getBool("de-compare-char"))`.

So their shipped comparison is **word-granular by default, character-granular on a
preference** — and `moves` is not shipped at all despite the engine carrying move review types.

---

## 3. How the markup is painted — one function, and it is the review path

### 3.1 The decision

`sdkjs/word/Editor/Paragraph/draw/line-draw-state.js`, `ParagraphLineDrawState.prototype.addLines`
(from `:660`):

- `if (this.reviewRem)` → `this.Strikeout.Add(startX, endX, this.reviewColor, ...)`, or
  `this.DStrikeout.Add(...)` when `this.reviewMove`;
- `if (this.reviewAdd)` → `this.Underline.Add(startX, endX, this.reviewColor, ...)`, or
  `this.DUnderline.Add(...)` when `this.reviewMove`;
- and a deletion of something previously inserted gets **both**: the strikethrough plus an
  underline in the earlier author's colour (`this.reviewRemAdd`, fed from
  `run.GetReviewInfo().GetPrevAdded()`, `line-draw-state.js:643-649`).

The flags come straight off the run (`line-draw-state.js:636-642`):
`let reviewType = run.GetReviewType(); if (reviewType !== reviewtype_Common) { this.reviewAdd =
reviewtype_Add === reviewType; this.reviewRem = reviewtype_Remove === reviewType; this.reviewColor
= run.GetReviewColor(); ... }`.

**So the whole of "render a diff" is: set a field on a run.** Nothing in the painter knows a
comparison happened.

### 3.2 Author colour

`ParaRun.prototype.GetReviewColor` (`Run.js:10875`) returns `this.ReviewInfo.Get_Color()`,
falling back to `REVIEW_COLOR`, defined at `sdkjs/word/Editor/Paragraph.js:59` as
`new AscCommon.CColor(255, 0, 0, 255)` — red. Per-author colour comes from
`word/Editor/revisions/review-info.js:170`.

### 3.3 No change bar in the margin

Searched for a per-paragraph change-bar draw: `ReviewChangesLine`, `changesLine`,
`ChangesVerticalLine`, and `HaveRevisionChanges` in the drawing files. `HaveRevisionChanges`
occurs only in `word/api.js` and `word/Editor/Document.js` — never in `Paragraph.js`,
`Paragraph_Recalculate.js` or anything under `word/Editor/Paragraph/draw/`. There is no
vertical change bar.

Word has one (behaviour, not source). ONLYOFFICE substitutes the balloon of §4.3 and the side
pane of §4.2. **We already ship a review gutter** — `webapp/tests/e2e/review-margin.spec.mjs`,
`review-margin-affordance.spec.mjs`, `review-gutter-fit.spec.mjs` — so on this one axis we are
ahead of ONLYOFFICE and level with Word.

---

## 4. What the reader DOES with it — the gestures, enumerated

Compare is a control **on the Review band**:
`web-apps/apps/common/main/lib/view/ReviewChanges.js:107` renders
`'<span id="slot-btn-compare" class="btn-slot text x-huge"></span>'`, and `:314` constructs it
with `action: 'compare-document'`, `iconCls: 'toolbar__icon btn-compare'`. It is gated by
`config.canFeatureComparison` (`:764`, `:821`). Its siblings on that band are the entire review
vocabulary, which is the point:

### 4.1 Per-change and whole-document accept/reject, and navigation

| Gesture | Engine call | Citation |
| --- | --- | --- |
| Accept the change under the caret, move to next | `asc_AcceptChangesBySelection(true)` | `api.js:10033`; `controller/ReviewChanges.js:603` |
| Reject under caret, move to next | `asc_RejectChangesBySelection(true)` | `api.js:10044`; `controller/ReviewChanges.js:618` |
| Accept / reject a specific change | `asc_AcceptChanges(oChange)` / `asc_RejectChanges(oChange)` | `api.js:10019`, `:10026` |
| Previous change | `asc_GetPrevRevisionsChange()` | `api.js:10069`; `controller/ReviewChanges.js:590` |
| Next change | `asc_GetNextRevisionsChange()` | `api.js:10065`; `controller/ReviewChanges.js:591` |
| Are there any? | `asc_HaveRevisionsChanges(isCheckOwnChanges)` | `api.js:10055` |
| Jump to the other end of a move | `asc_FollowRevisionMove(change)` | `controller/ReviewChanges.js:634-637` |
| Turn tracking on/off | `asc_SetTrackRevisions(bTrack)` / `asc_IsTrackRevisions()` | `api.js:9950`, `:9954` |

Buttons: `btnAccept`, `btnReject` (each with a menu for current-vs-all), `btnTurnOn`,
`btnReviewView` (`view/ReviewChanges.js:287`, `:300`, `:340`, `:383`). Accept and reject are
locked when the document is review-only and the change is not the current user's
(`view/ReviewChanges.js:667`; `controller/ReviewChanges.js` `editable:` at `:538`).

### 4.2 Four display modes — "Display for Review"

`asc_SetDisplayModeInReview(nMode)` (`api.js:11832`) over
`Asc.c_oAscDisplayModeInReview` (`sdkjs/common/commonDefines.js:3775`):

```
Edit: 0, Final: 1, Original: 2, Simple: 3
```

`Edit` is markup, `Simple` is markup without balloons, `Final` and `Original` show the two
sides with no markup at all. The menu wiring is `view/ReviewChanges.js:983-987`, mapping
`'markup' | 'simple' | 'final' | 'original'`.

This is the affordance that makes an on-canvas diff *readable* rather than oppressive: the
reader toggles between "show me what changed", "show me the result" and "show me what it was".
A diff with no such toggle is a document nobody can read.

### 4.3 Authorship and time: a balloon anchored at the change, plus a side list

The engine pushes the changes under the caret to the chrome:
`asc_registerCallback('asc_onShowRevisionsChange', ...)` and
`'asc_onUpdateRevisionsChangesPosition'` (`controller/ReviewChanges.js:153-154`).
`onApiShowChange(sdkchange, isShow)` (`:242`) takes **document coordinates off the change
itself** — `posX = sdkchange[0].get_X(), posY = sdkchange[0].get_Y()` (`:261-262`) — and does
`this.getPopover().setLeftTop(posX, posY); this.getPopover().showReview(animate, lock,
lockUser)` (`:272-275`). Suppressed in `Simple` display mode (`:259`).

The balloon's content per change (`controller/ReviewChanges.js:525-545`, building a
`Common.Models.ReviewChange`): `username`, `usercolor`, `initials`, `avatar`, `date`
(localised), `changetext`, `type`, `lock`, `lockuser`, `editable`, `goto`. The model's defaults
are at `web-apps/apps/common/main/lib/model/ReviewChange.js:55-69`.

So authorship is **three affordances at once**: the author's colour in the markup itself (§3.2),
the name and timestamp in a balloon at the change, and the same in the side list. There is also
a modal list, `dlgChanges` (`view/ReviewChanges.js:1131`), for narrow windows.

---

## 5. Their change entry names the object. Ours does not.

This is the sourced answer to the owner's third defect.

`controller/ReviewChanges.js` builds `changetext` per change type. For a deletion
(`:371-396`):

```
changetext = (movetype == Asc.c_oAscRevisionsMove.NoMove) ? me.textDeleted : ...
... changetext += (' ' + Common.Utils.String.htmlEncode(value));
```

and the strings are (`:1192-1196`):

```
textInserted:     '<b>Inserted:</b>',
textDeleted:      '<b>Deleted:</b>',
textParaInserted: '<b>Paragraph Inserted</b> ',
textParaDeleted:  '<b>Paragraph Deleted</b> ',
textFormatted:    'Formatted',
```

So an entry reads **`Deleted: <the actual removed text>`** — and when the removed thing is not
text, the label still names it: `'<' + me.textImage + '>'`, `textShape`, `textChart`,
`textEquation` (`:378-391`). A formatting change enumerates the properties that changed — bold,
italic, colour, highlight, font family, size, spacing, language — one by one
(`:404-437`); a paragraph-formatting change likewise, with real units
(`:446-...`). `TablePr`, `RowsAdd`, `RowsRem` have their own labels (`:512-521`).

**Ours says `Removed`.** The owner is right that this is unreadable, and it is the same defect
class as a control that refuses without a reason (`SKILL` §10: *able to say something when it
refuses*). The fix is not a wording tweak — it is that **the entry must name its object**, which
our engine already carries: `webapp/tests/e2e/compare.spec.mjs` asserts
`await expect(removed).toContainText("Gamma only in theirs")` for a *block* change, so the text
is available and the `text`-family entry is the one dropping it.

---

## 6. Google Docs — behaviour, not source

Labelled as behaviour throughout: Google Docs is not open source and no source was read. Stated
because `SKILL` §9 exists and this repository has published unsourced competitive claims twice.

1. **Two separate surfaces, two different models.**
   - **Suggesting mode** is inline tracked markup: an insertion is coloured and underlined, a
     deletion coloured and struck through, each in the suggester's colour, with a card in the
     right margin naming the author and time and carrying ✓/✗. Accept/reject per card, or all
     at once from a menu.
   - **Version history** (File ▸ Version history ▸ See version history) is a right-hand side
     list of timestamped entries grouped by day, with the authors of each. Selecting an entry
     renders *that version* in the canvas **with the changes from the previous version
     highlighted in the author's colour**, and there is a "Show changes" checkbox at the foot
     of the pane that turns that highlighting on and off.
2. **Compare documents** (Tools ▸ Compare documents) is the closest analogue to Word's
   Compare. It takes a second file from Drive and **produces a third document** — a new file
   whose differences are *suggestions* in the suggesting-mode sense, attributed to a named
   "comparison" author. So Google, like Word, answers "where do the differences live?" with "in
   a document, as tracked changes."
3. **The side list is a navigation surface, not a report.** Clicking an entry scrolls the canvas
   to it. The list never substitutes for the canvas rendering; it indexes it.

The structural agreement across all three references is the finding:

| | Compare's output | Where differences live | Reader's primitive |
| --- | --- | --- | --- |
| Word | a third, merged document | tracked changes in it | accept/reject, prev/next, change bars |
| ONLYOFFICE | the open document, mutated | `reviewtype_Add` / `_Remove` on runs | accept/reject, prev/next, 4 display modes |
| Google Docs | a third document (Compare), or the rendered version (history) | suggestions / author-coloured highlight | accept/reject cards; list scrolls canvas |
| **opendoc today** | **a JSON sidecar** | **a panel** | **reading a count** |

**Not one of them presents a comparison as a count.** The owner's complaint is not a polish
request; it is the observation that we implemented a different feature from the one the category
has.

---

## 7. What this document deliberately does not decide, and what would close it

`SKILL` §8 says *prefer one mechanism over two*, and §1-3 above make the candidate design
obvious: **express a comparison as the tracked-change model the editor already paints**, so the
canvas rendering, the accept/reject gestures, the author colours, the gutter and the review
sidebar are all reused rather than rebuilt, and Compare moves from a reporting surface to a
routing problem.

The three questions that decide its size were **measured**, and the answers are below. They were
read directly in this branch, not accepted from a report (`SKILL` §6).

### 7.1 The tracked-change model exists, renders, and is decidable — all of it

`crates/casual-doc-model/src/v1/body.rs:2409` `pub struct Revision { id, kind: RevisionKind,
author: Option<String>, date: Option<String>, revision_id, editor_group, inlines }`, with
`RevisionKind` (`:2325`) = `Insertion` / `Deletion` / `MoveFrom` / `MoveTo` — the same four
ONLYOFFICE carries. Author and date are **plain settable fields**, not derived from a session.

It renders: `crates/casual-doc-layout/src/flow.rs:159` `apply_revision_markup` sets
`run.color = review_author_color(author)` and then `strikethrough` for
`Deletion | MoveFrom`, `underline` otherwise — structurally identical to §3.1's
`addLines`, down to the double-line-for-moves distinction being the one thing we do *not* yet
draw. Ten author hues at `flow.rs:136`, mirrored byte-identically in the chrome at
`webapp/src/review_labels.mjs:168`.

It is decidable from JS today: `decideRevision(id, accept)`, `decideMovePair`,
`decideRevisionGroup`, `decideAllRevisions`, `setShowChanges(on)` / `showingChanges`, and the
chrome already has `review.next` / `review.previous` over them
(`webapp/src/main.js:11261` `navigateReview`). **§4.1's entire gesture table is already built.**

So the canvas rendering is not the missing half. It has been built the whole time.

### 7.2 A revision cannot be injected from JavaScript. This is the whole blocker.

The complete authoring surface is ten `suggest*` methods — `suggestInsert`, `suggestReplace`,
`suggestDelete`, `suggestSplit`, `suggestDeleteRange`, `suggestReplaceRange`, `suggestFormat`
and siblings — each of which means "perform *this* edit and record it as a revision". There is
**no** export that accepts a revision, a revision list, or an inline tree. From Rust there is
one, and it is the right one — `Operation::UpdateReviewState`
(`crates/casual-doc-edit/src/lib.rs:747`), whose `paragraphs` field is a list of
`ReviewParagraphState` (`:301`), each a node id plus an arbitrary inline list. Because that
list is arbitrary it accepts a tree containing `InlineNode::Revision(..)` with any author, date
and kind. **Every `suggest*` method is already built on exactly that operation.**

So routing a comparison through the review model is a small, well-shaped addition *in the
engine* — and the engine is outside this lane's file domain. §7.4 gives the signature.

### 7.3 The sidecar's anchor does NOT locate a change in the live document, and the crate says it does

This is the finding that killed the chrome-only design, and it is a documentation defect worth
fixing on its own.

`DiffAnchor` (`crates/casual-doc-diff/src/record.rs:113`) is
`{ story, path: Vec<PathSegment>, node: Option<String>, start: u32, end: u32 }`, and `start`/`end`
are UTF-8 byte offsets "the same byte space `ModelPos::offset` and the review anchors use"
(`record.rs:105-110`) — the two sides really are commensurable, and the diff reads
`ReviewProjection::FinalWithMarkup` (`crates/casual-doc-diff/src/projection.rs:59`), which is
what the editor shows. So far so good.

But `crates/casual-doc-wasm/src/diff.rs:228-244` imports **both** sides with
`import_for_diff(&bytes)`, a fresh parse, and ids are minted by a counter that restarts per
import (`lib.rs:33-58` says so explicitly, and `tests.rs` proves it with two real imports).
The chrome's right-hand side is `comparableBytes(doc, sourceFormat)` — i.e.
`doc.exportAs(...)`, a **re-export of the live document**, re-imported inside the facade. So
`right.node` is a `NodeId` of a throwaway parse and is **unrelated to the live document's id
space**.

`DiffAnchor.node`'s own doc comment and `casual-doc-diff/src/lib.rs:47-50` both say a host can
use it to "scroll a live document or a preview session to a change without re-deriving
anything". **That is true of a preview session and false of the live document**, and nothing
distinguishes the two. It is an overstatement of exactly the kind `SKILL` §9 exists for, and it
is the reason a reader might expect the present panel to be one small step from navigating.

That leaves `right.path` (story + block/row/cell indices) as the only live-document coordinate
— and **there is no `path → NodeId` resolver in the facade.** Checked: `blockIndexOf`
(`crates/casual-doc-wasm/src/lib.rs:12526`) is the inverse direction, `NodeId → index`;
`documentOutline` (`:12371`) emits node ids but only for *headings*; and
`accessibilityTreeWindow` (`:12495`), which does window by top-level block index, projects
`A11yBlockJson::Heading { level, text }` (`:12621`) and carries **no node id at all**.

So: the chrome cannot place a diff on the canvas, cannot scroll to one, and cannot highlight
one, with any API that exists today. The present panel is not a lazy rendering of a
locatable change list; **it is the most the chrome can currently say.** That reframes the
owner's defect: it is an engine-reachability gap (`SKILL` §9 point 4 — built, and not
reachable), not a chrome oversight.

### 7.4 What the engine needs — one function, exact signature

```rust
/// Applies a comparison sidecar to this document as tracked changes.
///
/// `sidecar` is `VersionDiff`'s JSON (`casual-doc-diff` `DIFF_SCHEMA` 1) produced
/// by comparing `left` = the other document against `right` = THIS document's own
/// exported bytes — `beginVersionDiff`'s orientation, which is review's. Each
/// `DiffChange` becomes an `InlineNode::Revision` authored to `author`/`date`,
/// applied as ONE `Operation::UpdateReviewState` under `HistoryKind::Review`, so
/// it is a single undo step and every existing review surface
/// (`listRevisions`, `decideRevision`, `setShowChanges`, DOCX `w:ins`/`w:del`)
/// reads it with no further change.
///
/// Refuses, rather than silently merging, when the document already carries
/// revisions — see `docs/158` §2.4: one `reviewType` field cannot hold both "a
/// person suggested this" and "a comparison computed this" without the two
/// deciding each other.
///
/// Complexity: O(changes + blocks touched). Not O(document).
#[wasm_bindgen(js_name = applyDiffAsRevisions)]
pub fn apply_diff_as_revisions(
    &mut self,
    sidecar: &str,
    author: &str,
    date: Option<String>,
) -> Result<EditResult, JsValue>
```

Because the chrome's right-hand side is a re-export, the engine applying the sidecar is also the
only party that *can* resolve `right.path` against the live document — it holds both. That is a
second, independent reason the work belongs there rather than here.

Two further constraints are already settled by the evidence and belong in whatever design
follows:

- **§2.4's collision is real and must be answered explicitly.** One `reviewType` field cannot
  carry both "a person suggested this" and "a comparison computed this" without the two
  accepting each other. ONLYOFFICE answers it by *destroying* the first — accept all existing
  changes before comparing, on consent. That is a legitimate answer and it is the cheap one; an
  alternative is a distinct origin on the revision so the two can coexist and be filtered. The
  design must pick one and say which, in the panel, to the reader.
- **The comparison must not block the tab.** ONLYOFFICE's does (`sync_StartAction(...
  BlockInteraction, SlowOperation)`, §2.4). Our existing driver already slices, reports progress
  and cancels at a slice boundary, and `compare_documents.mjs`'s header records why it is on the
  main thread (no `SharedArrayBuffer`, so no shared-memory worker on GitHub Pages) and that a
  separate instance in a worker is the right home. Routing the output through the review model
  changes nothing about that and must not regress it.

Until (1) is measured, the honest status of the on-canvas diff is **designed-but-unsized**, and
that is what this document records.

## 8. Open questions

1. Whether a comparison-authored revision should be attributed to a synthetic author (Google's
   approach, and the one that makes author colour meaningful) or to the compared document's
   name. Not decided.
2. Whether we ship the four display modes of §4.2 or a subset. A diff with no "show me the
   result" toggle is hard to read, so at least `Edit` and `Final` look mandatory, but this is
   unmeasured against our own chrome.
3. Whether word-vs-character granularity (§2.5) is exposed. `casual-doc-diff` already does
   grapheme-cluster word diff; whether that is switchable is unread.
4. Whether a merged third document (Word's and Google's Compare answer) is ever built, or
   whether mutating the open document (ONLYOFFICE's answer) is our permanent position.
   `compare_documents.mjs`'s header currently records "no merged third document" as a known gap.
