# 157 — ONLYOFFICE on large documents and on folding: source findings, and what they decide

**Status:** Source findings, 2026-10-02. The folding half is **acted on in the same branch**
— ADR-049 moves to **Accepted** and the `w15:collapsed` persistence tier is built. The
large-document half is **evidence for an open decision** and changes nothing on its own:
ADR-053 stays Proposed, with a third option it did not have.
**Opened:** 2026-10-02.
**Evidence base:** `/Users/sachin/Desktop/melp/reference/sdkjs` at
`72b0421c0bbf9d01eed9cf14834ae47eb2df1b50` and
`/Users/sachin/Desktop/melp/reference/web-apps` at
`9c0ca538c3b211052347df09d2a4d6781f023403` — both "Merge branch release/v9.4.0 into
master", 2026-05-19. **AGPL-3.0: behaviour and structure only, no code taken.** Every claim
below is tied to a path and an identifier, and the load-bearing ones were re-read directly
rather than accepted from a report (`SKILL` §6).
**Answers:** ADR-053's "owner decision needed" (§2) and ADR-049's Proposed status (§3, §4).
**Corrects:** `154` §2.3's enumeration of `collapsed` hits, and `154` §5.3's `flow.rs`
citation. Both marked in place.
**Closes:** `154` §7 open question 14 (whether their converter round-trips `w15:collapsed`)
for the editor path.

> **What this concludes, before the evidence.**
>
> 1. **They have no document-size ceiling in the editor.** No paragraph, element, page or
>    content-length limit exists in `sdkjs/word`. The only hard refusal is server-side at
>    open (file size, threshold not in these trees), and the only in-editor valve is on
>    accumulated *edit volume*, off by default. Our 262,144-block editing refusal has no
>    analogue. §2.1.
> 2. **Their per-keystroke cost is O(current paragraph), with no term in document size — and
>    they get there WITHOUT an identity index.** Position resolution on the typing path is a
>    cached integer per nesting level (`CurPos.ContentPos`), not a lookup. The reverse
>    direction is a cached `Index` field on each block repaired by a lazy **suffix**
>    reindex, armed only by structural edits. §2.2.
> 3. **That gives ADR-053 a third option it did not have, and it argues against weakening
>    B1.** ADR-053's two alternatives were "take the session-owned id→location index now" or
>    "restate B1 as O(1) above the windowing threshold, linear below it". The competitor
>    meets the strong form of B1 with a cheaper mechanism. §2.5.
> 4. **Virtualised painting, yes, with a real eviction policy. Virtualised layout, no.**
>    Geometry is computed for the whole document and never discarded; the mitigation for a
>    large file is a 10 ms time-sliced recalculation, not a window. So nobody in this
>    comparison has windowed editing, and `113` is not catching up to anyone. §2.4.
> 5. **Folding: they have none, measured three ways, and the attribute has no slot in their
>    model.** `grep -rinI "collaps" sdkjs/word` → **0 hits** across the entire 232-file Word
>    engine. `w15:collapsed` cannot survive an edit round-trip through their editor, and the
>    mechanism of the loss is literally `ReadUnknown`. §3.
> 6. **Word is therefore the standard for folding, and Word reflows** — collapsed content
>    occupies no pages. So folding is an **engine** change, not a panel change, and ADR-049's
>    "orthogonal to `LayoutView`" was right while its implied cheapness was not. §4.
> 7. **The fold range, verified in their arithmetic even though they do not fold:**
>    `[heading + 1, next_sibling_or_higher − 1]`. The heading's own paragraph stays
>    visible. §3.4.

---

## 1. How to read this document

`154` §2.3 already audited ONLYOFFICE's *reading view* from the same trees and got it
substantially right. This document answers two different questions the owner put, and it is
written to the same rule: a claim about their code names the file and the identifier, a
grep that found nothing is reported as a finding with its command, and anything inferred is
marked **inferred**.

Two notes on method, because this repository has published false claims twice in both
directions (`105` EV-007):

- **A zero-hit grep is evidence only when the search space is stated.** "No hits for
  `collapsed`" is worthless without "in `sdkjs/word`, 232 `.js` files". Both are given.
- **An absence in a closed list is stronger than an absence in a search.** Where the
  question is "does their model carry this property", the answer is a constructor's complete
  field list or an enum's complete value set, quoted to its last entry — not a grep.

---

## 2. Large documents: ceiling, resolution, recalculation, windowing

### 2.1 There is no document-size ceiling in the editor

Searched and not found, with commands run from `/Users/sachin/Desktop/melp/reference`:

| Search | Result |
| --- | --- |
| `grep -rnE "MAX_PARAGRAPH_COUNT\|MaxParagraphs\|MAX_ELEMENTS\|c_oAscMaxLength" sdkjs` | **0 hits** |
| `grep -rn --include='*.js' -oE "MAX_[A-Za-z0-9_]+" sdkjs/word sdkjs/common` | only `MAX_MM_VALUE`, `MAX_LEN`, `MAX_FONT_SIZE`, `MAX_POINTS_COUNT`, `MAX_PARAGRAPHS`, `MAX_ERRORS`, `MAX_ACTION_TIME`, … — none a document-size limit; the two paragraph/error ones are the spell checker's per-timer batch |
| `grep -rn -E "nMaxPages\|MAX_PAGE_COUNT\|maxPageCount" sdkjs/word sdkjs/common` | 2 hits, both `PrintMaxPagesCount` in `common/errorCodes.js`, raised only by the **spreadsheet** editor; `grep -rn "PrintMaxPagesCount" web-apps/apps/documenteditor` → **0 hits** |
| `grep -rniI -E "document (is )?too (big\|large)" sdkjs/word sdkjs/common` | **0 hits** |
| `grep -rniE "virtuali[sz]\|windowing\|lazy ?layout" sdkjs/word sdkjs/common` | **0 hits** |

The id registry is uncapped and unpruned: `CTableId` (`common/TableId.js:44`) is a bare
`this.m_aPairs = {}` whose `Add` is unconditional, and `g_oTableId.Delete(` has **zero call
sites** in `sdkjs/word` and `sdkjs/common` — entries go only on `Clear()`. That is a memory
fact, not a refusal.

**What does exist, in three kinds, none of them a document-size ceiling:**

1. **A hard refusal at open, imposed by the server.**
   `common/editorscommon.js:1050` maps `c_oAscServerError.ConvertLIMITS` to
   `Asc.c_oAscError.ID.ConvertationOpenLimitError`; the web client turns that into
   `errorFileSizeExceed` (`web-apps/apps/documenteditor/main/app/controller/Main.js:2337`)
   and escalates it to a terminal state at `Main.js:2450` with
   `this.api.asc_coAuthoringDisconnect()`. The user-facing string
   (`main/locale/en.json:1430`) is *"The file size exceeds the limitation set for your
   server. Please contact your Document Server administrator for details."* The document
   never opens. **The threshold is not in either tree** — it lives in Document Server's
   conversion configuration.
2. **An optional valve on accumulated EDIT volume, off by default.**
   `baseEditorsApi.prototype.checkChangesSize` (`common/apiBase.js:1776`) compares
   local + server change size against `api.maxChangesSize`, which is `0` — disabled — unless
   the licence response supplies it (`common/apiBase.js:244`, `:1990`). When it fires the
   user gets a forced choice between undo and a disconnected local mode
   (`web-apps/.../Main.js:3448`, `locale/en.json:1400`). It is called from `FinalizeAction`
   on every finished action, so when enabled it adds an O(unsaved-edits) term to each
   keystroke via `CHistory.prototype.GetLocalChangesSize` (`word/Editor/History.js:1041`).
3. **One silent degradation with no message at all:** spell check stops entirely past 2000
   errors. `word/Editor/SpellChecker/DocumentSpellChecker.js:40-41` defines
   `DOCUMENT_SPELLING_MAX_PARAGRAPHS = 50` and `DOCUMENT_SPELLING_MAX_ERRORS = 2000`, and
   `:147` gates the whole timer on `if (!this.IsOn() || this.IsErrorsExceed()) return;`.

**So the answer to "refusal, degraded mode, or nothing" is: nothing, during editing.** No
refusal, no warning, no mode change. A large document degrades continuously through
time-slicing (§2.3) and silently loses spell check.

### 2.2 Position resolution is a cached cursor chain — not a walk, and not an index

This is the finding that matters most, and it is a **third mechanism** that neither of
ADR-053's framings contains.

**Forward (position → object): a stored integer at each nesting level, dereferenced
directly.** `CDocument.prototype.controller_AddToParagraph`
(`word/Editor/Document.js:20023`) — re-read at that exact line:

```js
var nContentPos = this.CurPos.ContentPos;

var Item     = this.Content[nContentPos];
```

The same shape one tier down in `Paragraph.prototype.Add` (`word/Editor/Paragraph.js:4614`,
`this.Content[this.CurPos.ContentPos].Add(Item)`) and again in `ParaRun.prototype.Add`
(`word/Editor/Run.js:860`, `oRun.private_AddItemToRun(oRun.State.ContentPos, oItem)`). A
*stored* position is the same idea serialised: `CDocument.prototype.GetContentPosition`
(`Document.js:16323`) pushes `{Class, Position}` pairs down the tree and
`SetContentPosition` (`:16338`) walks them back. **There is no search anywhere on this
path, because the cursor is never lost in the first place.**

**Reverse (object → position): a cached `Index` on each block, repaired lazily.**
`CDocumentContentElementBase`'s constructor
(`word/Editor/DocumentContentElementBase.js:52`) carries the authors' own warning that the
field needs `Update_ContentIndexing()` first, and that function
(`word/Editor/DocumentContentBase.js:118`) is a **suffix** repair — re-read at that line:

```js
CDocumentContentBase.prototype.Update_ContentIndexing = function()
{
	if (-1 !== this.ReindexStartPos)
	{
		for (var Index = this.ReindexStartPos, Count = this.Content.length; Index < Count; Index++)
		{
			this.Content[Index].Index = Index;
		}
```

`private_ReindexContent` (`DocumentContentBase.js:345`) keeps the minimum dirty position and
is armed only by `Internal_Content_Add`/`Internal_Content_Remove`
(`Document.js:11373`, `:11420`) — i.e. by Enter, paragraph delete, or paste. **Typing a
character into an existing paragraph leaves `ReindexStartPos === -1` and the repair costs
nothing.**

**They do have a persistent id→object map, and it is not on the typing path.**
`CTableId.prototype.Get_ById` (`common/TableId.js:95`) over `m_aPairs`, maintained by every
object constructor (`word/Editor/Paragraph.js:177`,
`AscCommon.g_oTableId.Add(this, this.Id)` — 51 such call sites across 39 files), with ids
from a monotonic counter (`common/editorscommon.js:4557`). Its readers are the binary
reader, history, and collaborative change replay. **This is the shape ADR-053 proposes, and
in their design it exists for serialisation and co-editing rather than for resolution.**

**Where a scan does happen, it is bounded by the page, not the document.**
`CDocument.prototype.Internal_GetContentPosByXY` (`Document.js:7460`) narrows to the page's
column first and then scans `Column.Pos .. Column.EndPos` only (`:7511-7514`), so
click-to-place-caret is O(blocks on one page).

### 2.3 Recalculation is three tiers, and the unit is run-range / paragraph / page

The dispatch, re-read at `word/Editor/Document.js:2881-2890`:

```js
    if (undefined === _RecalcData)
    {
    	var arrChanges = this.History.GetNonRecalculatedChanges();

    	if (this.private_RecalculateFastRunRange(arrChanges))
			return document_recalcresult_FastRange;

    	if (this.private_RecalculateFastParagraph(arrChanges))
    		return document_recalcresult_FastParagraph;
    }
```

| Tier | Entry point | Unit of work | When |
| --- | --- | --- | --- |
| 1 | `private_RecalculateFastRunRange` (`Document.js:3264`) → `Paragraph.prototype.RecalculateFastRunRange` (`Paragraph_Recalculate.js:218`) | three line-ranges (previous, current, next) + **one** page repaint | the ordinary keystroke, when every pending change is in one `ParaRun` |
| 2 | `private_RecalculateFastParagraph` (`Document.js:3328`) → `Recalculate_FastWholeParagraph` (`Paragraph_Recalculate.js:49`) | one paragraph, 1–2 pages | the paragraph occupies ≤2 pages and its bounds, `EndInfo`, break flags and widow counts are unchanged |
| 3 | the page sweep in `Recalculate_PageColumn` | **one page at a time**, from the earliest dirty block to the end of the document | anything structural: tabs, drawings, breaks, changed line metrics, headers/footers (fast paths are disabled there by design), tables |

Tier 3 yields to a timer: `IsContinueRecalculateOnTimer` (`Document.js:4335`) breaks out
after ~2 pages once it has exceeded `Layout.GetCalculateTimeLimit()` or 50 pages, and that
limit is **10 ms** in edit and print layout (`word/Editor/Layout/Base.js:189`,
`PrintView.js:133`) and 100 ms in read layout (`ReadView.js:152`). A newly arrived edit
preempts an in-flight sweep (`Document.js:3115`).

**Full recalculation from position 0** happens on load, on zoom/layout change
(`RecalculateFromStart`, `Document.js:14962`), on a hyphenation-setting change
(`historyitem_recalctype_FromStart` has exactly 2 hits in `sdkjs/word`), and indirectly on a
default-paragraph-style change (`CStyle.RecalculateRelatedParagraphs`, `Styles.js:6117`).
**Not on a keystroke.**

### 2.4 Virtualised painting with eviction; no virtualised layout

**Painting is windowed and evicted.** The visible range is `m_lDrawingFirst`/`m_lDrawingEnd`
on `CDrawingDocument` (`word/Drawing/DrawingDocument.js:1874-1875`, set during scroll in
`word/Drawing/HtmlPage.js`), and the paint loop explicitly releases everything outside it
(`HtmlPage.js:3009-3015`) via `StopRenderingPage`
(`DrawingDocument.js:4149`, which calls `drawingPage.UnLock(this.m_oCacheManager)`). The
cache is a canvas pool with an unused-counter eviction policy — `CCacheManager`
(`word/Drawing/cachemanager.js:45`), evicting in `CheckImagesForNeed` (`:53`).

**Layout is not.** The sweep terminates only when content is exhausted
(`Recalculate_PageColumn`, `Document.js:4243`), `CDocument.Pages[]` and every paragraph's
`Pages`/`Lines` are retained for the whole document, and loading is eager and synchronous
(`asc_docs_api.prototype.OpenDocumentFromBin`, `word/api.js:1488`, is one
`oBinaryFileReader.Read(gObject)` call). The only page-count bound is for an explicitly
synchronous print-preview recalculation (`RecalculateAllAtOnce`, `Document.js:2816`).

**So the distinction the question asked for lands clearly on (a): pixels are windowed,
geometry is not.** Nobody in this comparison has windowed *layout*. `113` is not catching
up to a competitor; it would be ahead of one.

### 2.5 What this decides for ADR-053 — and what it does not

ADR-053 named two coherent positions: take the session-owned id→location index now, or
restate `107` B1 as *"O(1) above the windowing threshold, linear below it"*. The evidence
adds a third and argues against the second.

**The honest statement of the comparison:**

| | ONLYOFFICE | us, today |
| --- | --- | --- |
| Editing refused above a size | **no** — nothing in `sdkjs/word` | **yes** — `MAX_WHOLE_LAYOUT_BLOCKS` = 262,144 (`casual-doc-wasm/src/lib.rs:309`) |
| Position → paragraph on the typing path | cached integer per level, O(1) | `blocks_owning_mut` / `find_paragraph_mut`, **O(document)** (`107` §4.1) |
| Paragraph → position | cached `Index` + lazy suffix reindex | not needed in the same way |
| Per-keystroke asymptote | **O(current paragraph)**, dominated by whole-paragraph text shaping which their own comment flags as a known inefficiency (`Paragraph_Recalculate.js:2652`) | ~524,000 block visits per character at the refusal ceiling (`107` §4.1) |
| Incremental relayout unit | run-range → paragraph → page | `incremental.rs` / `dirty_pages` exist (`107` B6) |
| Windowed layout | **no** | `113`, designed |

**Three things follow, and the third is the one that changes a decision.**

1. **Do not weaken B1.** The weaker promise would put us behind a competitor on the
   constraint, not level with one. There is no windowing threshold in their editing path at
   all, and their per-keystroke cost is bounded by the paragraph. B1's strong form is
   achievable — it is achieved — so restating it as a conditional would be documenting a
   limitation as a design.
2. **ADR-053's inversion of priority stands.** Windowed editing still cannot be built on
   O(document) resolution. That conclusion does not depend on which mechanism replaces it.
3. **The mechanism is open again, and ADR-053's own rejection table is the reason to take
   this seriously rather than to switch.** A cached cursor chain is close to the
   **positional index** ADR-053 rejected, and the rejection's reason is correct and
   unchanged: *"every block insert or delete shifts its later siblings' indices"*. What the
   competitor shows is that the shift is affordable **when it is a lazy suffix repair armed
   only by structural edits** — they pay O(n − insertion point) on an Enter and nothing on a
   character. ADR-053 weighed a positional index as a *replacement* for identity anchors,
   which it must not be: `150` §2 chose `NodeId` for OT and that is not reopened. The open
   question is narrower and is for the owner:

   > Is the per-keystroke resolution cost better removed by **caching the caret's location
   > beside the session** (a chain of child indices, invalidated by the structural
   > operations that already go through the one choke point), by **ADR-053's id→location
   > index**, or by both — the cache for the caret, the index for everything that arrives
   > by id (a remote operation, a comment anchor, a find hit)?

   **This document does not answer that**, and it would be overreach to: it is an
   architecture decision about our own mutation path, ADR-030 I1/I3 are in it, and the
   measurement that would settle it (how often a real editing session takes a structural
   operation versus a character) has not been made here. What it does is remove the
   false choice. **Unverified on our side:** whether a cached chain can be maintained at
   `casual_doc_edit::apply` as cheaply as the index can — the deltas ADR-053 relies on
   (`MappingStep`/`PositionMap`) are about positions, which is encouraging, but nothing was
   measured.

**One caution against over-reading their numbers.** Absence of a ceiling in their source is
not evidence that they survive documents at the scale of our refusal. Nothing was executed,
and their own time-slicing exists precisely because a large document is slow. §5 lists this
as undetermined.

---

## 3. Folding: they have none, and the attribute has no slot in their model

### 3.1 The measured negative

| Search (`grep -rinI`, from `reference/`) | sdkjs (all) | **sdkjs/word** | web-apps/apps |
| --- | ---: | ---: | ---: |
| `collaps` | 341 | **0** | 510 |
| `collapsed` | 264 | **0** | 22 |
| `w15:collapsed` | **0** | **0** | **0** |
| `expandHeading` / `CollapseHeading` / `toggleHeading` | **0** | **0** | **0** |

**`sdkjs/word` — the whole Word engine, 232 `.js` files — contains the substring "collaps"
exactly zero times.** Verified directly here, not taken from a report. The near-misses are
all benign and worth naming so the negative is clean: `BookFoldPrinting` (booklet printing,
`word/Editor/Serialize2.js:747`), `foldedCorner` (an autoshape preset,
`word/fromToJSON.js:18835`), `ApiRange.prototype.ExpandTo` (`word/apiBuilder.js:1821`),
`DoNotExpandShiftReturn` (a justification compatibility flag), and `"chevron"` as a shape
preset name. Their only real folding is **spreadsheet** row/column outline grouping
(`sdkjs/cell/model/WorkbookElems.js:7152 Row.prototype.setCollapsed`) — which is worth
noting because it proves the team knows how to build folding and simply never built it for
documents.

Their word engine also never uses "collapsed" for an empty selection — they say
`IsSelectionUse()` / `RemoveSelection()` — so that usual source of grep noise contributes
nothing and the zero is clean.

### 3.2 `w15:collapsed` has no slot, proven against three closed lists

`grep -rn "w15" sdkjs/word` → **4 hits, all the shadow preset `shdw15`**
(`word/fromToJSON.js:2400`). No `w15` namespace handling anywhere in the JS tree.

1. **The `CParaPr` constructor**, `word/Editor/Styles.js:16232`. Re-read here; the complete
   field list is `Bidi, ContextualSpacing, Ind, Jc, KeepLines, KeepNext, PageBreakBefore,
   Spacing, Shd, Brd{First,Last,Between,Bottom,Left,Right,Top}, WidowControl, Tabs, NumPr,
   PStyle, FramePr, OutlineLvl, DefaultRunPr, Bullet, Lvl, DefaultTab, LnSpcReduction,
   PrChange, ReviewInfo, SuppressLineNumbers`. The only outline field carries their own
   comment: `this.OutlineLvl = undefined; // Для TableOfContents` — *"for
   TableOfContents"*. **No `Collapsed`.**
2. **The `Write_ToBinary` flag bitmap** (`Styles.js` ~16840–17010) runs contiguously from
   `Flags |= 1` to `Flags |= 16777216` = `SuppressLineNumbers`; the matching
   `Read_FromBinary` ends at `Styles.js:17147`. There is no 25th bit.
3. **The x2t↔sdkjs binary interchange enum**, `word/Editor/Serialize2.js:243
   var c_oSerProp_pPrType = {`. Values 0–49, ending `SuppressLineNumbers:44, CnfStyle:45,
   SnapToGrid:46, Bidi:47, Spacing_AfterLines:48, Spacing_BeforeLines:49`. Checked here with
   `sed -n '243,300p' word/Editor/Serialize2.js | grep -ci collapsed` → **0**. The only
   outline code is `outlineLvl: 34`.

Because (3) is the contract between their native converter and their editor, an unallocated
record would reach `Serialize2.js:9411 default: res = c_oSerConstants.ReadUnknown;` and be
discarded, and the writer can only emit allocated codes. **So `w15:collapsed` cannot survive
an edit round-trip through ONLYOFFICE, and the mechanism of the loss is literally
`ReadUnknown`.** That closes `154` §7 open question 14 for the editor path. The
x2t→binary leg is **inferred**, because `core`/`x2t` is in neither tree (`find
/Users/sachin/Desktop/melp/reference -maxdepth 2 -iname "*x2t*"` → 0 hits).

### 3.3 Their outline panel is a tree, and it is inert

Engine side: `CDocumentOutline` (`word/Editor/DocumentOutline.js`). Its elements are
`{Paragraph, Lvl}` with **no per-element fold flag**, and the complete exported API
(`DocumentOutline.js:501-512`, re-read here) is `get_ElementsCount, get_Text, get_Level,
get_CurrentPosition, goto, promote, demote, insertHeader, insertSubHeader,
isFirstItemNotHeader, selectContent, isEmptyItem`. **No Collapse, no Expand.** The UI has
nothing to call.

UI side: `Common.UI.TreeView`
(`web-apps/apps/common/main/lib/component/TreeView.js`), the same generic widget used by
version history (`apps/common/main/lib/view/History.js`), the hyperlink dialog, and the
spreadsheet autofilter. Collapsing flips `isVisible` on **list rows**
(`TreeView.js:95 collapseSubItems`). The settling evidence is the controller, re-read at
`web-apps/apps/documenteditor/main/app/controller/Navigation.js:314` — the menu item's
siblings go to the engine and expand/collapse go only to the view:

```js
            } else if (item.value == 'select') {
                this._navigationObject.selectContent(index);
            } else if (item.value == 'expand') {
                this.panelNavigation.viewNavigationList.expandAll();
```

and `grep -c "item:expand" .../controller/Navigation.js` → **0**: the controller does not
even subscribe to the expand event. **No code path hides a paragraph in the body.**

Accessibility, for the chrome lane: their caret is a bare `<div class="tree-caret …">`
(`TreeView.js:181`) with no role and no `tabindex`; the keyboard path is Left/Right on the
**row** (`TreeView.js:345-350`); `aria-expanded` and `aria-level` are set on the row
(`TreeView.js:243`, `:253`) but `role="tree"` has **0 hits** in `web-apps/apps`, and
`aria-expanded` has **0 hits** in `sdkjs`. So they ship an ARIA-attributed list, not a
declared tree, and nothing in the document body.

### 3.4 The fold range, from their own arithmetic

They do not fold, but they compute exactly the range a fold needs — for *selection*.
`CDocumentOutline.prototype.private_GetNextSiblingOrHigher`
(`word/Editor/DocumentOutline.js:400`), re-read here:

```js
	var nLevel = this.GetLevel(nIndex);
	var nPos = nIndex + 1;
	while (nPos < this.Elements.length)
	{
		if (nLevel >= this.GetLevel(nPos))
			return nPos;

		nPos++;
	}
```

Consumed by `SelectContent` (`DocumentOutline.js:421-440`), which selects from the heading
**inclusive**.

**The rule for us, stated exactly.** The heading's own paragraph **stays visible**; the
hidden range is every block from `heading_index + 1` through
`next_sibling_or_higher − 1` inclusive — all following content up to but excluding the next
heading whose outline-level *number* is the same or lower, transitively including
deeper-level headings inside it. A fold with no following sibling runs to the end of its
container. Note the one-index difference from their selection: `SelectContent` starts **at**
the heading, a fold starts **after** it.

This agrees with Microsoft's Open Specifications wording for the attribute — *"immediately
subsequent paragraphs with a higher heading level number appear collapsed"* — which is why
the range is **derived from the outline and never stored**. Storing it would duplicate a
fact the heading tree already carries and let the two disagree after an edit.

---

## 4. What this decides about ADR-049, and what is built

### 4.1 ADR-049 → Accepted, and one of its implications was wrong

The Proposed status was waiting on a competitive answer. There is one, and it is a clean
negative: **Word yes, Google yes, ONLYOFFICE no.** Where a competitor's code has nothing,
the house rule is to take Word's behaviour as the standard, and Word's behaviour settles the
only branch that mattered.

**Everything in ADR-049's Decision survives, and three claims are upgraded from assertion to
verified fact:**

- *"ONLYOFFICE has none (zero `collapsed` hits anywhere in `sdkjs/word/`, and
  `CDocumentOutline` exposes no Collapse or Expand)"* — confirmed exactly, §3.1 and §3.3.
- *"everyone else ships panel-tree-collapse and body-folding as two unrelated features with
  two states (ONLYOFFICE ship only the first, over the same generic `TreeView` widget their
  version-history panel uses)"* — confirmed, §3.3, including that their panel state is
  provably inert.
- The fold range, which ADR-049 did not state at all, is now pinned by §3.4.

**What was wrong is an implication rather than a decision.** ADR-049 says folding is
*"orthogonal to `LayoutView`"* and *"NOT part of reflow"*, and both are right. But §5.3 of
`154` and the ADR together read as though the mechanism were cheap and nearly local — it
reuses a run-tier filter, a view parameter, and a byte-space projection, all of which exist.
**It is still an engine change, and the evidence makes that unambiguous.** Word reflows:
collapsed content occupies no pages, so the heading immediately precedes the next visible
heading. ONLYOFFICE's pagination loop (`word/Editor/Document.js:3707`,
`for (Index = StartIndex; Index < nEndIndex; ++Index) { var Element = this.Content[Index];`)
has **no visibility filter at the block tier at all** — every element consumes vertical
space — and their only conditional-display mechanism is one tier down and inside the engine
(`CComplexField.prototype.IsHidden`, `word/Editor/Paragraph/ComplexField.js:1908`, which
hides a `{ TOC \o "1-3" }` instruction while showing its result). **A panel-only filter
cannot produce Word's behaviour**, and neither can a `webapp/` change.

### 4.2 The three tiers, and who owns each

| Tier | What | Owner | State |
| --- | --- | --- | --- |
| **1. Persistence** | `w15:collapsed` read, modelled tri-state, written, round-tripped; the fixture that arms the loss gate | `model` / `import` / `export` / `ooxml` / `fixtures` | **BUILT** on this branch, §4.3 |
| **2. Behaviour** | the block-tier visibility filter, so a folded subtree contributes no fragments and the document reflows | `casual-doc-layout` | **NOT STARTED** — `grep -rin "foldset\|fold_set" crates webapp/src` → **0 hits**. §4.4 states what it owes |
| **3. Affordance** | the in-body disclosure chevron and the outline panel as a real `role="tree"`, both driving one `FoldSet` | `webapp/` | **NOT STARTED** — `webapp/src/outline_panel.mjs` is still a flat list of buttons with no `aria-expanded`. §4.5 states what it owes |

### 4.3 Tier 1, built — and what the fixture actually found

`fixtures/generated/collapsed-headings.docx` is the first `.docx` in the repository to carry
`w15:collapsed`. ADR-049 said adding it converts an unknown into *"either a report or a red
gate"*. **It turned out to be a report**: `body.rs`'s generic `w:pPr` long-tail arm already
sent the element to `reporter.report_element`, producing one finding keyed `collapsed`, with
disposition `OmittedNotRetained` and `occurrences = 3`, and the loss-coverage gate stayed
green. So `154` §3.4's *"whether that drop is reported is unknown"* is now measured, and the
answer is better than ADR-049 assumed.

**A reported loss is still a loss**, and ADR-049 requires the state modelled and
round-tripped because it is the document default a viewer's live fold state layers over. So:

- `ParagraphProperties::collapsed: Option<bool>` (`casual-doc-model/src/v1/properties.rs`)
  — tri-state, for the reason `contextual_spacing` documents at length. `Some(false)` is an
  explicit `w:val="0"` that **cancels** a fold inherited from a style chain; a plain `bool`
  makes that unrepresentable and silently re-folds a section the document deliberately
  unfolded.
- `apply_paragraph_property`'s `b"collapsed"` arm (`casual-doc-import/src/properties.rs`),
  matched on the **local** name like every other property. The namespace is pinned by the
  fixture rather than by the arm, because a local-name parser cannot tell `w:collapsed` from
  `w15:collapsed` — and this project's own notes had guessed the wrong one.
- `write_paragraph_properties` emits `w15:collapsed` beside `w:outlineLvl`
  (`casual-doc-export/src/semantic.rs`), and `declare_fold_namespaces` declares `w15` plus
  `mc:Ignorable="w15"` on each of the six part roots that can carry a `w:pPr` — the main
  document, header/footer, notes, comments, styles, numbering.

**The fixture is adversarial on purpose.** Four headings in all four `CT_OnOff` states
(explicit on; implied on, with no `w:val`, which `CT_OnOff` reads as ON; explicit off;
absent), each with a body paragraph under it so the subtree a fold would hide is real
content. It carries `mc:Ignorable="w15"` exactly as Word does — that directive is a licence
for an old consumer to ignore the element, and a guard exists precisely to show our reader
does not take it.

Six guards in `casual-doc-export/tests/collapsed_heading_state.rs`, each driven red by a
production mutation and recorded in the branch's commit message. One is worth repeating here
because it is the house rule working: the first draft asked
`xml_text(written).contains("collapsed")` and **failed**, because the fixture's fourth
heading reads *"Heading with no collapsed state"* — the written package held the substring in
a `w:t` and the guard claimed a round-trip that had not happened. A guard measuring the
fixture rather than the guarantee. The wording stays in the fixture, as an adversarial
property that will catch the next substring-based guard.

### 4.4 What `casual-doc-layout` owes — precisely

This is the hand-off. Nothing below was built, and nothing in `casual-doc-layout` or
`casual-doc-render` was touched on this branch.

1. **A third per-viewer view parameter.** A `FoldSet` of collapsed heading `NodeId`s,
   threaded exactly as `ReviewView` (`crates/casual-doc-layout/src/flow.rs:106`) and
   `LayoutView` (`crates/casual-doc-layout/src/document_layout.rs:162`) are: defaulting to
   today's behaviour, so every existing caller is byte-for-byte unchanged and
   `geometry_snapshot.golden` does not move.
2. **A block-tier visibility filter, not a second flow path.** The precedent is one tier
   down and already there: `push_styled_runs`
   (`crates/casual-doc-layout/src/flow.rs:6527`) returns early at `:6539` on
   `if effective.hidden == Some(true)`. Folding is that filter at the **block** tier. *(This
   corrects `154` §5.3, which cites `flow.rs:6500`.)*
3. **The scope rule of §3.4**, derived from outline levels at flow time and never stored:
   hide `[heading + 1, next_sibling_or_higher − 1]`, the heading itself staying visible.
   Nesting is transitive: a deeper heading inside a folded range is hidden with it whatever
   its own fold state says.
4. **Reflow, not blanking.** Folded blocks must contribute **no fragments and no height**,
   so pagination closes up and the page count falls. A filter that merely skipped painting
   would leave Word's behaviour unreached and is the one wrong answer with a plausible
   shape.
5. **The byte-space projection must give a folded range a single boundary position**, so a
   caret cannot land inside what the eye cannot see — reusing the collapsed-deleted-range
   projection (`casual-doc-wasm/src/lib.rs:9553`, guarded at `:32635`). It must **not** take
   `ReviewView::Markup`'s escape of being *"never fed to caret/selection/hit-test"*
   (`flow.rs:107-110`): the document stays editable while folded.
6. **Print, PDF and DOCX export are always fully expanded**, forced in the wrapper rather
   than by each caller — `print.mjs`'s `withPagedLayout` already forces `Paged` with the
   restore in a `finally`, and "unfolded" belongs beside it. Word reportedly prints only
   expanded content and Notion reportedly drops collapsed toggles from a PDF; both are
   rejected as silent loss in a printed artefact (`SKILL` §12).
7. **It is not a layout view, and must not become one.** If the design finds itself wanting
   a second paginator, the abstraction is wrong and it should stop (ADR-049's own
   consequence).
8. **A guard that doubles.** Fold a document of *n* and *2n* headings and assert the laid-out
   block count and page count fall proportionally — a complexity guard, not a millisecond
   threshold (`SKILL` §8).

### 4.5 What `webapp/` owes — precisely

Also not built here, and reported rather than attempted.

1. **`outline_panel.mjs` becomes a real tree.** It renders a flat list of buttons with
   `lvl-1`…`lvl-6` classes, `aria-current="location"`, **no disclosure control, no
   `aria-expanded`, no `role="tree"`**. It needs `role="tree"` / `role="treeitem"` /
   `aria-expanded` / `aria-level`, with Left/Right collapsing and expanding — ONLYOFFICE's
   `TreeView` is the floor here, not the ceiling: they have `aria-expanded` and `aria-level`
   but no declared `role="tree"` (§3.3), so matching them is not enough.
2. **An in-body disclosure chevron** at the heading, which is Word's affordance and which
   ONLYOFFICE does not have at all. It must be a real focusable control, not a bare `div` —
   their caret is a `div` with no role and no `tabindex`, which is the mistake to avoid.
3. **One state, two surfaces.** The panel's disclosure and the body chevron drive the **same**
   `FoldSet`. Everyone else ships these as two features with two states; having one is an
   improvement rather than parity, and it satisfies the ≥2-surfaces floor by construction.
4. **The live per-viewer fold state lives beside `docReflow` in `prefs.mjs`**, and a viewer's
   toggling is **never** written into the file. `w15:collapsed` is the saved default; this is
   Google's two-tier split and the only model that can honour a Word file.
5. **Find reveals by unfolding.** `findText` (`casual-doc-wasm/src/lib.rs:3503`) walks the
   model, so folded text is already searched by construction; the work is unfolding a match's
   ancestors.
6. **`a11y_mirror.mjs` is filtered by the same set**, or the fold lies to screen-reader
   users, and the collapsed heading announces how much is hidden.
7. **A fold-crossing selection takes the whole subtree, out loud** — one undoable operation
   that states its scope ("Deleted section and N paragraphs"), never a quiet cascade. No
   competitor documents this either way; it is decided from this repository's no-silent-loss
   rule.
8. **Never a dead control.** Until tier 2 lands, a disclosure that cannot actually hide
   anything must not ship as a button that does nothing: disabled with a reason, or not
   present.

**What a user can reach today:** nothing new. A `w15:collapsed` document now opens with its
fold state read, carried and written back instead of dropped, which is a fidelity fix and
invisible in the UI. Folding is not reachable until tiers 2 and 3 land, and this document
does not claim otherwise (`SKILL` §9.4: "built" is not "reachable").

---

## 5. What could not be determined — read this before quoting anything above

1. **The numeric value of ONLYOFFICE's server-side open ceiling.** `c_oAscServerError.
   ConvertLIMITS` is mapped, never defined with a threshold, in either tree. It lives in
   Document Server configuration, not checked out here.
2. **The numeric value of `maxChangesSize`.** Default `0` (disabled); the real value arrives
   from the licence response.
3. **Whether ONLYOFFICE survives documents at the scale of our 262,144-block refusal.** No
   limit and no benchmark exists in their source, and nothing was executed. **Absence of a
   ceiling is not evidence of adequate performance.**
4. **x2t's own handling of `w15:collapsed`.** §3.2 closes the editor path directly and the
   x2t→binary leg by inference from the closed enum. The converter is in neither tree.
5. **Whether ONLYOFFICE honours `w:vanish` at draw time.** All 37 `Vanish` hits in
   `sdkjs/word` are model, binary IO, JSON and one shaping comparison
   (`word/Editor/Paragraph/TextShaper.js:334`); zero in draw or recalculation paths and zero
   in `web-apps/apps`. So hidden text looks round-tripped but unhonoured with no UI toggle —
   **inferred**, not executed.
6. **Where Word places `w15:collapsed` among `w:pPr` children.** Flagged in our own code at
   the writer. This writer's child order is already not the `CT_PPr` schema sequence
   (`w:jc` precedes `w:keepNext`), so Word's exact position is neither matched nor
   regressed; the element is foreign content under `mc:Ignorable` and Word is tolerant of
   its position. Unverified either way.
7. **Whether Word prints collapsed content.** Second-party reporting only. ADR-049 rejects
   the behaviour on its merits regardless, so nothing is gated on it.
8. **Whether a cached cursor chain is maintainable at our mutation choke point as cheaply as
   an id→location index** (§2.5). Not measured. The question is posed, not answered.
9. **Table-internal and header/footer recalculation complexity in ONLYOFFICE.** Both fast
   tiers explicitly refuse tables and headers/footers
   (`Paragraph_Recalculate.js:42`, `:65-71`); `word/Editor/Table.js` (20,156 lines) was not
   read.
10. **Their incoming-collaborative-change path**
    (`common/applyDocumentChanges.js`, `CDocumentPositionsManager`). A different hot path
    from local typing, not analysed.
11. **ONLYOFFICE's mobile, embed and forms editors.** 0 `collaps` hits, so no folding there
    either, but their navigation UIs were not otherwise classified.

---

## 6. Corrections this document makes to `154`

| Where | What `154` says | What the source says |
| --- | --- | --- |
| §2.3, folding paragraph | the `collapsed` search *"returns hits only in `cell/` … and in the web-apps version-history UI"* | the sdkjs hits are **exclusively** under `cell/`; the `web-apps` hits are help text, locale strings, border-collapse CSS, ribbon/toolbar fold (`Mixtbar.js`), the macros dialog, and the panel `TreeView`. The *widget* is shared with `History.js`, but the literal term is not in it. Conclusion unchanged. |
| §2.3, folding paragraph | cites `DocumentOutline.js:42-488` and lists seven methods | the file is 512 lines; the export block at `:501-512` lists twelve. Still no Collapse or Expand. Conclusion unchanged. |
| §5.3, precedent 1 | cites `flow.rs:6500` for the `w:vanish` early return | `fn push_styled_runs` is at `flow.rs:6527` and the return at `:6539`. |
| §3.4 | *"whether that drop is reported is unknown, and the gate cannot tell us"* | measured: it **was** reported (`OmittedNotRetained`, 3 occurrences) and is now modelled and round-tripped. §4.3. |
| §7, open question 14 | whether their converter round-trips `w15:collapsed` | **closed for the editor path**: no slot in `c_oSerProp_pPrType`, unallocated records hit `ReadUnknown`. §3.2. |
