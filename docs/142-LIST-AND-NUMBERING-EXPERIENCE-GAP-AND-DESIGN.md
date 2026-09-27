# 142 — Lists and numbering experience: gap analysis and design

**Status:** Design, complete. **Opened:** 2026-09-28. **Owner:** unassigned.
**Scope:** the *interaction* quality of list and numbering editing in OpenDoc measured
against Google Docs and Word, plus the design for the three lanes that close most of it.
Docs-only: this document changes no code.

**Why this exists.** Third in the series the owner asked for after the ONLYOFFICE toolbar
gap (`docs/130`) and the table experience (`docs/141`). `docs/141` is the format; this
document follows it, including its rule about correcting its own numbers (§0.3).

**This is not a feature inventory.** We ship **nine** list controls on the Home band, a
bullet gallery of 6 glyphs and a number gallery of 5 formats behind split carets, Tab /
Shift+Tab level change, Enter-on-empty-item exits the list, Backspace-at-start outdents,
Restart numbering, Continue numbering with a real availability predicate, a checklist kind
neither Word nor ONLYOFFICE has, and a numbering engine that honours `w:numStyleLink`,
`w:lvlOverride`, `w:startOverride`, `w:lvlRestart`, `w:isLgl` and `w:suff` — with 22 unit
tests over it. Listing commands again would answer a question nobody asked.

**The finding, in one sentence.**

> A list, in Word and in Docs, is an **object**: it has an identity you can point at, nine
> levels you can address, a definition you can edit, and a start value you can set. In
> OpenDoc a list is a **property on one paragraph**. So every gesture that only needs a
> paragraph property is present and mostly correct — and every gesture that needs the list
> *itself* is missing, silently wrong, or built and unreachable. Where `docs/141` found
> commands without hit zones, this document finds gestures without an object.

The single measurement that shows it: the closed operation set has **53 variants** and
**zero** of them can touch `Definitions::numbering` or `Definitions::abstract_numbering`
(§0.4). Four facade methods mint numbering definitions by reaching around the op stream
through `definitions_mut()`, so a list definition **cannot be undone, cannot be created by
a command, and cannot be carried by OT**. The visible consequence is §1.1: because
`fn ensure_list` memoises exactly one numbered instance per document session, **every
numbered list a user creates in a session is the same list**, and the second one starts at
4.

---

## 0. Method — and why every number here is reproducible

### 0.1 Our side is cited by grep anchor, never by line number

`webapp/src/main.js` and `crates/casual-doc-wasm/src/lib.rs` are both enormous and both move
by hundreds of lines a day — `wc -l` them rather than quoting a figure from here. **This
document proved the point on itself:** its draft measured them at 16,579 and 39,405, and the
rebase onto `origin/main` immediately before the commit moved them to 16,574 and 39,559, so
the only two raw line counts it ever contained were stale before it landed. `docs/130` §0
records a first draft carrying ~30 `main.js:NNNN` citations that were all stale within 24
hours; `docs/141` §0.1 restates the rule. So every claim about our code names **a string to
grep** — a function name, a `js_name`, a CSS class, a DOM id, an error message — and the file
it is in.

Two stale line citations found while writing this are reported in §6 rather than fixed
here: `docs/104` HF-107 cites `lib.rs:4397` and `lib.rs:4159` for `set_list_format` and
`restart_list` (both have moved), and `docs/109` row 10 cites
`casual-doc-edit/src/lib.rs:5599` for `find_paragraph_mut`, which is now elsewhere.

### 0.2 Google Docs claims are KNOWLEDGE; ONLYOFFICE claims are SOURCE-VERIFIED

There is no Google Docs source in this environment and it cannot be run from here.
`/Users/sachin/Desktop/melp/reference/` holds exactly two checkouts, `sdkjs` and
`web-apps`, both ONLYOFFICE. **Every "Docs does X" statement below is the author's
knowledge of the product, not a citation, and is tagged `[K]`.** A confident wrong claim
about a competitor is the worst possible content in a document that will be quoted
(`SKILL.md` §9), so §0.2a lists the `[K]` rows whose confidence is low, marked
**UNVERIFIED** rather than asserted.

Rows tagged `[S]` carry a `file:line` into the pinned ONLYOFFICE checkout. Lists are the
one area where their source is unusually informative, because list behaviour is almost
entirely keyboard and model logic rather than pointer geometry — so `[S]` coverage here is
much better than `docs/141` achieved for tables. **Three of the load-bearing `[S]`
citations below were re-read verbatim by the author** after the research pass and are
marked `[S✓]`: the Tab condition, the Enter-on-empty-item block, and the autocorrect
bullet literals.

ONLYOFFICE is not the bar Docs is. But in several rows below **ONLYOFFICE is ahead of us
and Docs is ahead of both**, which is the most useful shape a row can have, and in five
rows (§3) we are ahead of both.

### 0.2a The `[K]` claims this document does NOT stand behind

Marked so no lane builds against them:

| Claim | Why unverified |
| --- | --- |
| Docs demotes on Tab from **anywhere** in a list item, not only at the start | Word's rule is start-of-item only and is `[S✓]` for ONLYOFFICE too. The author believes Docs indents the whole item from any caret position but cannot check. **§1.3 grades against the Word/ONLYOFFICE rule, which is verified, and records the Docs behaviour as unverified.** |
| Docs' default nested numbered scheme is `1.` / `a.` / `i.` | High confidence for Word (`hybridMultilevel` default). For Docs the author is confident of `1.` / `a.` but has not checked the third level |
| A list inside a Docs text box shares the body's counter | Genuinely unknown, and Word's own behaviour here is subtle. §1.12 states what **our** code does and refuses to compare |
| Docs offers a "revert to literal text" affordance after an autoformat | Word has the AutoCorrect Options smart tag; ONLYOFFICE has **none** (`[S]`, §1.5). For Docs the author recalls no smart tag but is not sure. D-2 therefore designs the one-step undo as the *guarantee* and the affordance as an owner question |

### 0.3 The numbers this document publishes — and the six of its own it had to correct

Every figure has a recipe in §8. Two corrections to figures *given to the author as
premises*, and four to figures the author derived wrongly on the first pass:

1. **The brief's "the op set is 51 variants after #649" is wrong; it is 53.** Derived two
   ways in §0.4 (a `{`-only match and a match admitting tuple and unit variants both give
   53, and the two name sets are identical). `docs/08-ADR-REGISTER.md` still says *"The
   closed set is 51 variants"* — the two beyond 51 are `SetShapeFill`/`SetShapeStroke` and
   `SetTextBoxBody`. `docs/141` §0.4 already published 53; the ADR is the stale one. **This
   matters for constraint compliance, not vanity: the constraint is "no new operation unless
   nothing existing composes", and §4 proposes zero, so the baseline has to be right.**
2. **The HF-184 premise in the brief ("a linear scan with 35 call sites") counts
   occurrences, not call sites, and none of them are the list path.** `grep -c` over
   `casual-doc-edit/src/lib.rs` gives 35 lines, of which one is the definition and one a doc
   comment → **33 call-site lines**, some of them the function's own recursion. The two hits
   in `casual-doc-wasm` are *doc comments promising not to add a call site*. So the list
   facade's O(document) cost is **not** HF-184; it is the sibling `paragraph_properties` and
   `ordered_paragraphs` walks (§1.15), which is a different row with a different fix.
3. **The author's own first count of "list commands" was 9 and is wrong twice over.**
   `paragraph.list` is a **submenu container** (`submenu:`, no `run`), and
   `paragraph.listFormat.*` is a **generated family**, one id per gallery cell, not a
   command. The honest figures are in §0.5.
4. **Three published recipes counted struct-literal initialisers as consumers, and were
   rewritten before this document shipped.** `grep -rn 'lvl_jc' crates/casual-doc-layout`
   returns 7 hits and `pstyle` returns 8 — every one of them is a `field: None` in a test
   fixture or, in `pstyle`'s case, an unrelated test *name*. The claim in §1.19 ("no layout
   consumer") is correct and the first recipe for it was not; §8 now publishes the filtered
   forms, which return 0. **A recipe that cannot distinguish a read from an initialiser is
   not a derivation.**
5. **The author's first recipe for "22 numbering unit tests" was `grep -cE '^    fn '` and
   returns 30**, because that also counts six test helpers and the module's own
   non-test functions. The figure 22 is right; `grep -c '#\[test\]'` is the recipe, and §8
   publishes that.

6. **§2.1's own class counts were hand-counted wrongly three times**, in three different ways,
   before the author replaced the hand count with an `awk` extraction of §2's `Class` column.
   The full account is in §2.1 and the command is in §8. This is the most embarrassing of the
   six and the most instructive: the table being counted is **in this file**, so there was no
   excuse of a moving codebase — just `SKILL.md` §8's rule, ignored for one table because it
   looked small enough to count.

`docs/141` §0.3 predicted two of its own numbers would be wrong and found two. This document
found six, and four of them were in *recipes and counting method* rather than in figures —
which is the more dangerous kind, because a wrong recipe outlives the document that published
it.

### 0.4 The engine op count, derived — and the hole in it

```sh
awk '/^pub enum Operation \{/,/^\}/' crates/casual-doc-edit/src/lib.rs \
  | grep -cE '^    [A-Z][A-Za-z0-9]*( \{|\(|,)'                       # => 53
awk '/^pub enum Operation \{/,/^\}/' crates/casual-doc-edit/src/lib.rs \
  | grep -oE '^    [A-Z][A-Za-z0-9]* \{' | tr -d ' {' | wc -l         # => 53 (same names)
```

**Not one of the 53 can create, modify or delete a numbering definition.** Recipe:

```sh
grep -rn 'abstract_numbering' crates/casual-doc-edit/src/   # => 0 hits
grep -rn 'numbering' crates/casual-doc-edit/src/            # => only page_numbering / line_numbering
```

The only ops that carry list state are `SetParagraphProperties` (which replaces the whole
`ParagraphProperties`, so `numbering: Option<NumberingRef>` rides along) and
`SetStyleDefinition` (style-level `w:numPr`). So a *reference* to a list is transactional
and a *list* is not.

Four facade methods mint definitions outside the op stream, by
`self.document.definitions_mut()`:

| Method | What it inserts | Bounded? |
| --- | --- | --- |
| `fn ensure_list` | one `AbstractNumbering` + one `NumberingInstance` per kind | yes — memoised on `self.bullet_list` / `self.numbered_list` |
| `fn ensure_checklist` | same, per checked state | yes — memoised on `self.checklist_checked` / `self.checklist_unchecked` |
| `pub fn restart_list` | one fresh `NumberingInstance` per invocation | **no** |
| `pub fn set_list_format` | one fresh abstract **and** one fresh instance per invocation | **no** |

`fn undo_inner` replays **inverse operations** (`self.apply_group(&entry.operations)`), so
an insertion that was never an operation has no inverse and undo cannot remove it. This is
tracked as **HF-107** (P3, wasm, Open, *"blocked on an owner decision: routing list
numbering through undo means adding to the closed op set (invariant I2, `docs/45`)"*), and
HF-107's own parenthetical *"(ensure_list/ensure_checklist are memoized and are not
affected.)"* is **half wrong** — they are bounded, which is what memoisation buys, but they
are equally outside the op boundary, so the *first* bullet toggle in a session still leaves
two definitions behind after undo. `fn ensure_list`'s doc comment states the deferral
deliberately: *"The definition is document infrastructure, not a body edit, so it is not
part of undo; an unreferenced numbering instance is valid and harmless."* That sentence is
true of the orphan and false of the consequence — see §1.1.

**This is also the ADR-033 blocker in its list instance.** `docs/107` carries OT over
transactions; a numbering definition that is not an operation cannot be transformed,
ordered or replayed, so *"user A changes the list marker"* has no wire representation at
all. `docs/109` row 109 (CQ-002) already says CQ-002 *"closes HF-107 on the way"*; §4.1.5
takes a position on how to sequence around it.

### 0.5 The command surface, derived

```sh
grep -oE '"paragraph\.(list|listFormat|indent|bullets|numbering|restart|continue)[A-Za-z.]*"' \
  webapp/src/main.js | sort | uniq -c
```

| Figure | Value |
| --- | --- |
| Invocable list commands | **5** — `paragraph.list.bullet`, `.numbered`, `.checklist`, `.restart`, `.continue` |
| Indent commands (list-overloaded, §1.9) | **2** — `paragraph.indent.increase`, `.decrease` |
| Submenu containers | **1** — `paragraph.list` ("List & indentation") |
| Generated marker-format ids | **11** — one `paragraph.listFormat.<spec>` per gallery cell: 6 bullet glyphs + 5 number formats |
| Divergent context-menu-only ids for the same four capabilities | **4** — `paragraph.bullets`, `paragraph.numbering`, `paragraph.restart`, `paragraph.continue` |
| Home-band list controls | **9** — `#bulletList`, `#bulletListMenuBtn`, `#numberedList`, `#numberedListMenuBtn`, `#checkList`, `#indentDec`, `#indentInc`, `#restartList`, `#continueList` |
| Keyboard chords | **3** — `⌘⇧L` bullet, `⌘M` / `⌘⇧M` indent (`webapp/src/keymap.mjs`). **No chord changes a list level.** |

The SURFACE table, which `docs/141` §0.3 could not build for tables because the Table band
carries no `data-command`. For lists it *can* be built, because `webapp/src/ribbon_faces.mjs`
declares `face("#bulletList", "paragraph.list.bullet")` and its siblings:

| Capability | Ribbon | App menu | Context menu | Palette | Compact | Chord | Surfaces |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Bullet list | `#bulletList` | Format | *`paragraph.bullets`* | yes | yes | `⌘⇧L` | 6 |
| Numbered list | `#numberedList` | Format | *`paragraph.numbering`* | yes | yes | — | 5 |
| Checklist | `#checkList` | Format | `paragraph.list.checklist` | yes | yes | — | 5 |
| Restart numbering | `#restartList` | Format | *`paragraph.restart`* | yes | — | — | 4 |
| Continue numbering | `#continueList` | Format | *`paragraph.continue`* | yes | — | — | 4 |
| Marker format | split carets | **none** | **none** | `paragraph.listFormat.*` | **none** | — | **2** |
| Increase / decrease indent | Home + Layout | Format | yes | yes | yes | `⌘M`/`⌘⇧M` | 6 |
| **Change list level** | **—** | **—** | **—** | **—** | **—** | Tab only | **1** |

Two findings fall straight out. Italicised cells are the divergent ids — `docs/130` §11.1
already records `paragraph.bullets` and `paragraph.numbering` as a defect; **`paragraph.restart`
and `paragraph.continue` are two more of the same shape that `docs/130` did not find**, so
the count in that section is 5 leaf capabilities and should be 7. And the last row is the
one genuine single-surface capability in the feature: changing a list level is reachable
**only** by pressing Tab, or by the Increase Indent button, which does not say that is what
it does (§1.9). That is `docs/105` UX-004's floor violated, and it is why a screen-reader
user searching the palette for "list level" finds nothing.

### 0.6 The ribbon budget, and the figures not to quote

Home already carries nine list controls, so any proposal to add a tenth has to say where
the width comes from. The budget is derived by
`webapp/tests/e2e/ribbon-width-budget.spec.mjs` at a fixed 1280×900: **~288px** of Home
growth headroom, Home fits down to a **1017px** viewport, and the single assertion is a
**120px floor** (`expect(headroom).toBeGreaterThanOrEqual(120)`), deliberately not pinned
at 288. The failure mode is a **horizontal scrollbar** on the band, not the `⋯` overflow —
that button tracks viewport width, not content width.

**Do not quote "~55px of slack"**: it is wrong five-fold and names the wrong failure mode,
and six in-tree comments still say it (HF-216). **Do not quote `docs/130` §8's "264px"
either** — that section's table gives 264 for Home while the spec's own comment gives 288;
the spec is the artifact and the doc is the copy. Reported in §6.

**No design in this document proposes a new ribbon control.** D-3 lands as extra rows inside
the two split-button menus that already exist — zero band width, which is also what
`docs/130` §8 row 12 concluded independently for the multilevel gallery.

### 0.7 The five grades

| Grade | Meaning |
| --- | --- |
| **UI-only** | the facade and engine already do it; only `webapp/**` changes |
| **facade+UI** | `crates/casual-doc-wasm` needs a new export or parameter; model and layout already suffice |
| **facade** | `crates/casual-doc-wasm` alone |
| **engine** | a model field, a layout consumer, an op, or an import mapping is genuinely missing |
| **reachability-only** | we ship it and it works, but not from where the user reaches for it |

**Check the engine before grading a row `engine`.** `docs/141` §0.4 records a page break
graded `engine` that needed none, and a table row whose `facade+UI` was too cheap. The
numbering engine here is unusually complete — `numbering.rs` follows `w:numStyleLink`,
applies a full `w:lvlOverride/w:lvl` redefinition, honours `w:startOverride`,
`w:lvlRestart` and `w:isLgl`, and substitutes `%1`..`%9` — so most rows below grade
`facade+UI` or lighter, and every `engine` grade names the missing field, consumer, op or
mapping.

---

## 1. The interactions, audited one at a time

### 1.1 List identity — the row everything else sits on

**Docs `[K]` / Word `[K]`.** Click Numbered list at the top of a document, type three
items. Move to the bottom, click Numbered list again, type three more. The second list
starts at **1**. Two lists are two lists; "Continue previous numbering" exists precisely
because joining them is the *unusual* request.

**Ours: they are one list, and the second starts at 4.**

| Step | What happens | Anchor |
| --- | --- | --- |
| `toggleList(..., "numbered")` asks for an instance | `fn ensure_list(true)` returns the **cached** `self.numbered_list`, minting it only on first use | `fn ensure_list` in `crates/casual-doc-wasm/src/lib.rs`: `if let Some(id) = if numbered { self.numbered_list } else { self.bullet_list } { return Ok(id); }` |
| the paragraph is pointed at it | `p.numbering = Some(NumberingRef { instance, level: 0 })` | `pub fn toggle_list` |
| the counter is keyed by the instance | `counters[(NumberingInstanceId, u8)]`, advanced once per numbered paragraph in document order | `pub struct NumberingState` in `crates/casual-doc-layout/src/numbering.rs` |
| so a later, unrelated list continues | counters have no notion of adjacency; the module doc says so: *"two paragraphs sharing a `numId` continue one sequence (even across intervening non-list paragraphs — Word's 'continued list')"* | `numbering.rs` module header |

**This is deliberate and pinned by a guard, which is why it has survived.** The native test
whose body contains the assertion message `the bullet definition is reused, not duplicated`
checks that `definitions().abstract_numbering.len()` does not grow on a second toggle. Read
carefully, **that guard exercises the BULLET path only**, and reuse is harmless for bullets:
a bullet level has no counter, so two bullet lists sharing one instance are
indistinguishable from two instances. The guard is right about bullets and silent about the
case that hurts. This is `SKILL.md` §9.6's *"a guard can pin a lie"* — except here the guard
does not even have to move, because the correct fix (D-1) changes only the numbered path.

Corollaries of having no list identity, each verified separately:

- **No facade call returns a list's id.** `listStyleAt`, `listLevelAt`, `listFormatAt` and
  `checkboxStateAt` return a kind string, a level, a token and a tri-state int. Nothing
  returns a `NumberingInstanceId`, so the host cannot ask *"are these two paragraphs the
  same list?"* — which is exactly the question every gesture in §1.6, §1.8 and §1.14 needs.
  There is no `ListInfo` analogous to `TableInfo`'s 20 getters: `grep -rn 'ListInfo'
  crates/ webapp/` returns **zero**.
- **The instance cache is session state, not document state.** `self.numbered_list` is a
  field on `WasmDocument`. Save, close, reopen: the cache is empty, so the next toggle mints
  a *new* instance and the numbering the user already had is a different list from the one
  they are about to create. Behaviour that depends on how long the tab has been open is the
  definition of non-deterministic, and `AGENTS.md` puts deterministic behaviour second only
  to correctness.
- **Restart is the manual workaround, and it is the only one.** `restartList` exists and
  works; `#restartList` sits on the Home band. So the product ships a Home-band button whose
  main job is to undo a defect. **Grade: facade** — the fix is an adjacency rule inside
  `toggle_list`, no new op, no model change (D-1).

**ONLYOFFICE `[S]`: ahead of us, and their mechanism is the design.**
`CRunAutoCorrect.prototype.private_GetSuitableNumPr`
(`reference/sdkjs/word/Editor/Paragraph/Run/RunAutoCorrect.js:991`) decides join-or-mint by
looking at the **previous paragraph's** `NumPr` and the value the user typed: it joins when
the level `IsSimilar` and the typed value is within `+1..+2` of the computed one (`:1042`),
and creates a new list only when every parsed level value is 1 (`:1113-1120`).
`CDocument.prototype.ContinueNumbering`
(`reference/sdkjs/word/Editor/Document.js:25028`) is the explicit join, and it too is a
`NumId` re-assignment (`:25091 oPara.SetNumPr(oSimilarNumPr.NumId, oCurNumPr.Lvl);`) — the
same shape as our `continue_list`. They have list identity; we have one list.

### 1.2 The default nested scheme — what a user sees the first time they press Tab

**Word `[K]`, high confidence.** A new numbered list is `1.` at level 0, `a.` at level 1,
`i.` at level 2, and the indent steps **720 twips (0.5 in)** per level.
**Docs `[K]`, medium confidence** (§0.2a): `1.` / `a.` / `i.`, 0.5 in steps.

**Ours: `1.`, then `1.1.`, then `1.1.1.`, stepping 360 twips.** Verbatim, from
`fn list_level(numbered: bool, level: u8)` in `crates/casual-doc-wasm/src/lib.rs`:

```rust
let placeholders = (1..=level + 1).map(|part| format!("%{part}")).collect::<Vec<_>>().join(".");
(NumberFormat::Decimal, format!("{placeholders}."))
…
start_twips: Some(720 + i32::from(level) * 360),
hanging_twips: Some(360),
```

So **every level is `Decimal` and every level's `lvlText` concatenates all its ancestors**
— we ship an outline/legal scheme as the *default*, where both competitors ship
`decimal / lowerLetter / lowerRoman`. Pressing Tab once on item 2 of a list produces `1.1.`
where a user expects `a.`, and the sub-item sits 0.25 in in rather than 0.5 in.

Bullets fare better but not well:
`["\u{2022}", "\u{25e6}", "\u{25aa}", "\u{2013}"][usize::from(level) % 4]` — `•`, `◦`, `▪`,
`–`, then **repeating from `•` at level 4**, so levels 0 and 4 are indistinguishable. Word
cycles `•`, `o`, `▪`; Docs `•`, `○`, `■` `[K]`.

Nothing about this is engine-shaped: `numbering.rs` renders whatever `lvlText` and `numFmt`
say, and there is a test for every numeral system (`fn roman_numerals`,
`fn lower_and_upper_letter_wrap_past_z`). **Grade: facade**, and it is three constants in
one function — the highest value-per-line row in the document.

### 1.3 Tab / Shift+Tab to demote and promote

**The single most-used list gesture there is.** Our implementation is real and mostly
right, which makes the divergences worth stating precisely.

| Aspect | Word / ONLYOFFICE | Ours | Anchor |
| --- | --- | --- | --- |
| Tab demotes a list item | **only when the caret is at the start of the item** (or the selection starts there); anywhere else it inserts a literal tab | **from anywhere in the item**; a list item can never contain a tab character typed with Tab | `[S✓]` `reference/sdkjs/word/Editor/Document.js:8810`: `if (null != Paragraph && (true === Paragraph.IsCursorAtBegin() \|\| true === Paragraph.IsSelectionFromStart()) && (undefined != Paragraph.GetNumPr() \|\| …))`, else `:8829 this.AddToParagraph(new AscWord.CRunTab());`. Ours: the `key === "Tab"` branch in `webapp/src/main.js` tests only `doc.listStyleAt(selection.focus.node)` |
| Level clamp | 0..8 | 0..8 | `[S]` `reference/sdkjs/word/Editor/Paragraph.js:10362` `Math.min(8, oldLvl + 1)`; ours `(i32::from(numbering.level) + delta).clamp(0, 8)` in `pub fn adjust_list_level` |
| First item of a list | **does not change level** — it shifts the whole `CNum`'s indents instead | changes level like any other item | `[S]` `Paragraph.prototype.Shift_NumberingLvl`, `reference/sdkjs/word/Editor/Paragraph.js:5158` `if (0 === Lvl && NumInfo[Lvl] <= 1)` → `oNum.ShiftLeftInd(NewX)` |
| In a **table cell** | Tab moves to the next cell | Tab moves to the next cell | ours: `if (doc.inTable(selection.focus.node)) { doc.moveTableCell(...) }` runs **before** the list branch. **Parity** |
| …so is demote reachable in a cell at all? | via Increase Indent | **yes** — `adjustIndentCommand` → `adjustListLevel` → `fn apply_paragraph_props_as`, which does **not** filter table paragraphs (unlike `fn apply_indent_props`, which does) | `fn apply_paragraph_props_as` vs `fn apply_indent_props` (`"no body paragraph in selection"`) |
| In a **footnote / header / footer / text box** | works | **works** — `pub fn paragraph_properties` and `fn ordered_paragraphs` both go through `fn surface_block_lists`, which covers every surface | the uniform-pipeline rule honoured; §3 |
| In **Suggesting** mode | tracked | refused **with a stated reason**: *"Indent and list structure changes cannot be tracked yet; switch to Editing"* | the `reviewMode === "suggesting"` guard in the Tab branch. Honest, and ahead of a silent no-op |
| Announced to assistive tech | — | **yes** — `webapp/src/a11y_mirror.mjs` re-nests demoted items as real `ul`/`ol`, guarded by three cases in `a11y-list-nesting.spec.mjs` | §3 |

**The cost of the start-of-item divergence is small but real, and it goes the way that
loses data**: there is no keystroke that puts a tab inside a list item. `docs/141`'s lesson
applies — record deliberate divergence in the code rather than leaving it ambiguous
(`SKILL.md` §8); the Tab branch's comment says *"(and how lists are demoted/promoted)"* and
does not mention the caret-position rule it is not implementing. **Grade: UI-only.**

**Test coverage: none of it is driven by a test.** `a11y-list-nesting.spec.mjs` presses Tab
and asserts the a11y projection, which is the only Tab-in-a-list assertion in the product.
There is **no native test for `adjust_list_level` at all** — not the happy path, not the
clamp, not its refusal (§1.20).

### 1.4 Enter on an empty item ends the list; Backspace at the start outdents

**Both are implemented, and both are right.** Recorded in full because the brief expected
otherwise and because the shape is worth copying.

```js
// Enter:
if (listKind && doc.paragraphLength(focus.node) === 0) {
  const level = doc.listLevelAt?.(focus.node) ?? 0;
  await runToolbarEdit((a, b, c, d) =>
    level > 0 ? doc.adjustListLevel(a, b, c, d, -1) : doc.toggleList(a, b, c, d, listKind), …);
}
// Backspace, at offset 0:
if (listKind) { … the same two-branch rule … }
```

So Enter on an empty **nested** item promotes one level, and only at level 0 does it strip
the list — which is exactly ONLYOFFICE `[S✓]`
(`reference/sdkjs/word/Editor/Document.js:19451-19463`:
`if (numPr.Lvl <= 0) { Item.RemoveNumPr(); … } else { Item.SetNumPr(numPr.NumId, numPr.Lvl - 1); }`)
and, to the author's knowledge, Word and Docs `[K]`. Backspace matches their
`Paragraph.prototype.Remove` branch `[S]`
(`reference/sdkjs/word/Editor/Paragraph.js:4329-4341`), whose comment states the same
two-branch rule.

Three sub-gaps inside the parity:

- **Shift+Enter is handled first and deliberately** — a soft break inside an empty list item
  must not exit the list — and the comment says so. Good; recorded so it is not "fixed".
- **In Suggesting mode the rule does not run.** The `reviewMode === "suggesting"` branch
  precedes the empty-item rule, so Enter on an empty item produces **another bullet** via
  `suggestSplit`. Arguably unavoidable (the engine cannot represent "remove numbering" as a
  tracked change), but it is undocumented and it is the one mode where the most habitual
  list gesture silently does the opposite thing. **Grade: UI-only** for saying so.
- **Neither gesture is tested.** A search for an Enter-ends-list assertion in
  `webapp/tests/` comes up empty; `checklist.spec.mjs` tests *"Enter after a checked item
  starts an unchecked one"*, which is a different rule.

### 1.5 Typing `1. ` or `- ` — autoformat as you type

**Ours: absent, entirely.** Recipe:

```sh
grep -rn -iE 'auto(_|-)?(format|list|bullet|number)' webapp/src crates/*/src   # no list hit
grep -n 'autoformat\|autoCorrect' webapp/src/main.js                          # 4 hits, all comments
```

`webapp/src/text_rules.mjs` exports `smartQuoteForTyped` and nothing else of this shape; the
only as-you-type transform in the product is smart quotes. Typing `1. ` leaves the literal
characters `1. ` in the paragraph. This is tracked as **`docs/109` OO-016** (*"AutoCorrect is
smart quotes only — no math codes, no autoformat list triggers"*, P3, M, Open), and **P3 is
the wrong grade**: for a large fraction of users typing `1.` **is** how you start a list, and
its absence is felt before any button is found. Reported in §6.

**ONLYOFFICE `[S]`, and they are well ahead — their implementation is the spec for D-2.**

| Aspect | Their behaviour | Citation |
| --- | --- | --- |
| Where | `CRunAutoCorrect`, flag `AUTOCORRECT_FLAGS_NUMBERING = 0x00000020`, tried last | `reference/sdkjs/word/Editor/Paragraph/Run/RunAutoCorrect.js:49`, `:173` |
| Triggered by | **Space or Tab only** — the numbering flag is on `CRunSpace` and `CRunTab` and on nothing else, so typing `1.` and pressing **Enter** does *not* make a list | `.../RunContent/Space.js:220-223`, `.../RunContent/Tab.js:192-195`; absent from `ParagraphMark.js:201-206` and `Break.js:346-350` |
| Bullet literals | exactly three: `*` → Symbol U+00B7, `-` → Arial U+2013, `>` → Wingdings U+00D8. **Not** `+`, `•`, `–` | `[S✓]` `RunAutoCorrect.js:1128-1155` |
| Number literals | ≥2 chars, terminator `.` or `)`, ASCII + Arabic-Indic + extended Arabic-Indic digits, A-Z → upper Roman/letter, a-z → lower, compound `1.2.3.` parsed level-by-level and capped at 9 | `RunAutoCorrect.js:1162-1349`, `_getDigit` `:1175-1185`, `private_ParseNextInt` `:1210`, cap `:1271` |
| Guard | only in a paragraph with no numbering yet, only with the cursor at the scan end | `RunAutoCorrect.js:911-914` |
| Join vs new list | joins the previous paragraph's list when the level is similar and the typed value is within `+1..+2`; mints a new list only when every level value is 1 | `RunAutoCorrect.js:991`, `:1042`, `:1113-1120` |
| Undo | **one step back to the literal text** — the whole conversion is one history point and the literal *and* the trigger space are removed inside it | `RunAutoCorrect.js:930` `StartAction(AscDFH.historydescription_Document_AutomaticListAsType)` … `:951 FinalizeAction()`, selection at `:938` |
| A "revert" affordance | **none** — no smart tag, and the description is deliberately absent from `private_IsPointDoAutoCorrect` | `reference/sdkjs/word/Editor/History.js:1545-1556` |
| Settable off | two checkboxes, default on | `reference/sdkjs/common/api/autoCorrectSettings.js:47`; UI `web-apps/apps/common/main/lib/view/AutoCorrectDialog.js:302-317`; strings `.../locale/en.json:766`, `:777` |

**Grade: UI-only** for the matching, **facade** for the one-undo guarantee (§4.2.2). Every
other piece exists: `doc.copyText(node, 0, node, offset)` reads the prefix, `toggleList` and
`setListFormat` set the list and its numeral system, and `runEdit` already gates on Viewing
and Suggesting. The work is the *design* — which literals, which trigger, and how the
one-step undo is guaranteed — which is D-2.

### 1.6 Continuing versus restarting numbering

**We are at or ahead of Docs here on reachability and behind ONLYOFFICE on depth.**

| | Docs `[K]` | ONLYOFFICE `[S]` | Ours | Anchor |
| --- | --- | --- | --- | --- |
| Restart at 1 | right-click, Restart numbering | context menu, *"Start new list"* (bullets: *"Separate list"*) | **Home band button, Format menu, context menu, palette** | `#restartList`; `pub fn restart_list` |
| Continue previous | right-click, Continue previous numbering | context menu, *"Continue numbering"* (bullets: *"Join to previous list"*) | same four surfaces, and **only enabled when it would work** | `js_name = canContinueList`; `continueListBtn.disabled = … \|\| !doc.canContinueList(...)` |
| Restart at an arbitrary **value** | no | **yes** — *"Set numbering value"* → `NumberingValueDialog` → `asc_RestartNumbering(value)` | **no** — hard-coded `start: Some(1)` | `[S]` `web-apps/.../DocumentHolderExt.js:2294`, `.../controller/DocumentHolderExt.js:2315-2329`, `reference/sdkjs/word/api.js:4366`. Ours: `NumberingOverride { level, start: Some(1), definition: None }` in `pub fn restart_list` |
| Mechanism | — | clone the whole `CNum` and re-point the tail; **never writes `w:startOverride`** | mint a fresh instance **with** `w:startOverride` | `[S]` `reference/sdkjs/word/Editor/Document.js:25192-25211`. Ours uses the OOXML construct they avoid — **a fidelity lead** (§3) |
| Bullets refused | — | refused (`:25162`) | refused, *"restart numbering requires a numbered list item"* | parity |

Three defects inside it:

- **`restart_list` breaks at the first deeper item.** Its carry loop is
  `if numbering.instance != current.instance || numbering.level != current.level { break; }`
  — so on `1. / 2. / 2.1 / 3.` restarting at item 1 re-instances item 1 **only**, and items
  2 and 3 keep the old instance whose level-0 counter now starts from 1 again. The visible
  result is two items numbered `1.`. Word restarts the level *and* its subtree `[K]`.
  **Grade: facade.**
- **`restartListBtn.disabled = !hasSel || listKind !== "numbered"` sets no `title`.** A
  disabled Restart on a bullet list presents as a grey button whose tooltip still reads
  *"Restart numbering at 1"*, while the **palette** row for the same command says *"Place the
  caret in a numbered list"* and the context-menu row hides itself. Three behaviours for one
  refusal; this is `docs/141` TBL-03's list twin. **Grade: UI-only.**
- **`listKind !== "numbered"` is the wrong predicate for a multilevel list.**
  `pub fn list_style_at` resolves the kind from the level the caret is on, so a numbered list
  whose level 3 is a bullet reports `"bullet"` and Restart goes grey on an item of a numbered
  list. Small, but it is the shape that gets reported as "the button doesn't work".

### 1.7 Multi-level list definitions

**Ours: there is no multilevel scheme picker, and no way to change any level but the
caret's.** `pub fn set_list_format`'s own doc comment is the admission:

> *"Only single-level marker formatting is handled here; authoring a full multilevel list is
> a separate follow-up."*

What the marker galleries do is good work, and precise: `pub fn set_list_format` clones the
current abstract, retargets **the caret level's** `w:numFmt`/`w:lvlText` through
`fn apply_marker_spec`, mints a fresh abstract **and** instance so other lists sharing the
editor's reusable definition are untouched, and repoints the contiguous run. Six bullet
glyphs and five number formats, `aria-checked` reflected from `listFormatAt`, blocked in
Viewing, one undo — `list-marker-gallery.spec.mjs` covers all three. That is a Docs-shaped
marker picker and it works.

What is absent, each verified:

| Capability | Evidence of absence |
| --- | --- |
| Pick a multilevel **scheme** (all nine levels at once) | no such control; `grep -oE 'id="[a-zA-Z]*([Ll]ist\|[Nn]umber)[a-zA-Z]*"' webapp/editor.html` lists no multilevel id |
| Change the marker of a level other than the caret's | `pub fn set_list_format` addresses `current.level` only |
| Set a level's `w:suff`, `w:lvlJc`, `w:isLgl`, `w:lvlRestart`, indent or tab position | no facade parameter for any of them |
| Bind a level to a paragraph style (`w:lvl/w:pStyle`) | `NumberingLevel::pstyle` exists in the model, is imported and exported, and **no layout code reads it** (§8's filtered recipe → 0) |
| `w:multiLevelType` | written as `None` at both `fn ensure_list` and `fn ensure_checklist`; `grep -n 'multi_level_type' crates/casual-doc-wasm/src/lib.rs` → 2 hits, both `None` |

This is tracked as **`docs/109` OO-021** (*multilevel list gallery*, P2, M, Open) and
`docs/130` §8 row 12 places it *"on Home — but as a third item in the existing list split
buttons' menus, not a new button. Zero width."* D-3 builds on that placement.

**ONLYOFFICE `[S]`, far ahead, and their dialog is the field list for D-3.** They have a
third split button `#id-toolbar-btn-multilevels` (`web-apps/.../Toolbar.js:845-847`), a
9-item **Change list level** submenu on each of the three buttons (`Toolbar.js:2691-2694`,
`:2703`, handler `controller/Toolbar.js:1806 this.api.asc_SetNumberingLvl(item.options.level);`),
**eight hardcoded multilevel presets spelling out all nine levels** (`Toolbar.js:3232-3241`,
e.g. `'{"Type":"number","Lvl":[{"lvlJc":"left","suff":"tab","numFmt":{"val":"decimal"},"lvlText":"%1.","pPr":{"ind":{"left":360,"firstLine":-360}}}, …]}'`),
more loaded from `resources/numbering/multilevel-lists.json` (`:3258`), and a
`ListSettingsDialog` whose multilevel variant edits, per level: `numFmt` (`:399`), bullet
char (`:405`), the `lvlText` string as free text (`:1155-1158`), an "include level number"
control that inserts `%n` (`:1088-1089`), `lvlJc` (`:441`), start-at (`:541`), restart
(`:649`), `suff` (`:627-635`), number position and text indent (`:592`, `:614`), tab stop
(`:659`), and the marker's font, bold, italic, size and colour (`:1214`, `:1226`, `:1235`,
`:476`, `:767`).

They do **not** have (searches that returned nothing, `[S]`): a "Define new multilevel list"
or list-style creation dialog (`grep -rn "DefineNewMultilevel\|defineNewList" web-apps/` →
empty; `grep -n "Define new" .../locale/en.json` → empty), an enumeration API for existing
definitions (`grep -rn "GetAllNumbering" sdkjs/word` → empty), and any authoring of
`w:isLgl` (`grep -rn "put_IsLgl" web-apps/apps` → empty; it is *read* only, to render the
preview, at `ListSettingsDialog.js:1040`).

**Grade: facade+UI.** The model carries every field, `numbering.rs` consumes almost all of
them, and the export writes them all back (§1.13). What is missing is one facade write path
and one panel.

### 1.8 Mixed selections

**This is the row where we silently do the wrong thing, and it is asymmetric.**

The branch is chosen in JS from the **focus** paragraph, and the engine then validates the
**start** paragraph:

```js
// webapp/src/main.js — adjustIndentCommand and the Tab branch, both:
const listKind = doc.listStyleAt(selection.focus.node);
listKind ? doc.adjustListLevel(a, b, c, d, ±1) : doc.adjustIndent(a, b, c, d, ±360)
```

```rust
// pub fn adjust_list_level, on the ORDERED start:
if paragraph_properties(&self.document, start.node).and_then(|p| p.numbering).is_none() {
    return Err(to_js("list level adjustment requires a list paragraph".into()));
}
```

| Selection | What the user expects | What happens | Why |
| --- | --- | --- | --- |
| list items only | all demote | all demote | correct |
| a plain paragraph **first**, list items after | Word demotes the items and indents the paragraph (`IsMixedSelection` → `IncreaseIndent`, `[S]` `reference/sdkjs/word/Editor/Document.js:8799-8804`) | **refused**, and the message says *"That tracked format is not supported for this selection yet"* | `listStyleAt(focus)` is truthy → `adjustListLevel` → the start check throws → `runToolbarEdit`'s hardcoded catch (§1.17) |
| list items **first**, a plain paragraph after | the same | **all get a 360-twip indent instead of a level change, silently** | `listStyleAt(focus)` is `""` → `adjustIndent` for the whole range, list items included |
| a list plus a following paragraph, then click **Numbered** | Word shows the button mixed and applies to all `[K]` | **the list is REMOVED** | `pub fn toggle_list` computes `already` from `start.node` only, so an already-numbered first paragraph means "turn it off for everything" |
| two lists at different levels | both demote by one | both demote by one | correct — `fn apply_paragraph_props_as` runs the closure per paragraph and the closure is `if let Some(mut numbering) = p.numbering` |
| the toolbar button's own state | `aria-pressed="mixed"` `[K]` | a definite on/off read from the **focus** paragraph | `bulletListBtn.setAttribute("aria-pressed", String(listKind === "bullet"))` — while `fmtButtons.underline` in the same function does implement `"mixed"` |

So three of six mixed-selection cases are wrong, two of them **silently**, and the sixth
misreports state. **Grade: UI-only** — decide the branch from the whole selection rather
than the focus, and add the mixed button state; the engine already tolerates mixed ranges.

### 1.9 Indent controls versus list level

**Increase Indent and Demote are the same command here, and nothing says so.**

```js
function adjustIndentCommand(delta) {
  const listKind = doc.listStyleAt(selection.focus.node);
  runToolbarEdit((a, b, c, d) =>
    listKind ? doc.adjustListLevel(a, b, c, d, delta > 0 ? 1 : -1)
             : doc.adjustIndent(a, b, c, d, delta), { paragraphLevel: true });
}
```

**ONLYOFFICE `[S]`: identical, deliberately.** `Paragraph.prototype.IncreaseDecreaseIndent`
opens `if (undefined !== this.GetNumPr()) { this.Shift_NumberingLvl(!bIncrease); }`
(`reference/sdkjs/word/Editor/Paragraph.js:5326-5330`), and their toolbar has only
*"Increase indent"* / *"Decrease indent"* (`web-apps/.../Toolbar.js:617`, `:631`; strings
`locale/en.json:4338`, `:4352`) with **no** promote/demote pair. Word behaves the same way
`[K]`. So **the overloading is parity, not a defect** — recorded so it is not "fixed".

What is a defect is everything around it:

- **The label lies about what it will do.** `#indentInc`'s title is *"Increase indent
  (Tab)"*. In a list it does not change the indent; it changes the level, which also changes
  the marker. ONLYOFFICE has the same overloading *and* an explicitly level-shaped **"Change
  list level"** nine-item picker (`Toolbar.js:2703`, string `locale/en.json:4198`) so a user
  can say what they mean. We have no such control on any surface (§0.5, last row).
- **Indenting a list item without changing its level is still possible, but only via the
  ruler or the paragraph panel** — `setLeftIndent` / `setFirstLineIndent`, whose
  `fn apply_indent_props` deliberately skips table-cell paragraphs. So the two paths have
  different table semantics for the same conceptual action, and neither is discoverable as
  "indent this list item".
- **There is no `Alt+Shift+→`/`←`** (Word's promote/demote chord `[K]`) and no chord of any
  kind for the level. `webapp/src/keymap.mjs` has `⌘M`/`⌘⇧M` for indent and `⌘⇧L` for bullets.

**Grade: UI-only** (a Change-list-level control plus honest titles); the overloading stays.

### 1.10 Numbering across interruptions

Verified by reading `NumberingState` and the flow, section and note paths. Four of the five
cases are right and the fifth is a real defect.

| Interruption | Behaviour | Anchor |
| --- | --- | --- |
| a plain paragraph between items | **continues** — correct. Counters are keyed `(instance, level)` with no positional rule, so a non-list paragraph simply never calls `resolve` | `numbering.rs` module doc: *"even across intervening non-list paragraphs — Word's 'continued list'"* |
| a table between items | **continues** — correct | the same mechanism |
| a paragraph **inside a table cell** | **participates in the body's sequence** — body item 1, cell item 2, body item 3, which is Word's behaviour `[K]` | `fn flow_blocks` recurses into cells with the *same* `ctx`: only `ctx.table_depth` and `ctx.table_style` are saved and restored, never `ctx.numbering` |
| a page break | **no effect** — correct. The galley tier is width-only; pagination runs afterwards and `grep -n numbering crates/casual-doc-layout/src/paginate.rs` hits only `page_numbering`/`line_numbering` | `crate::paginate` |
| a **section break** | **counters restart** — a list that spans a section break renumbers from 1. Word continues `[K]` | `fn push_section_run` (`crates/casual-doc-layout/src/document_layout.rs`, two call sites: the per-break loop and the trailing section) → `fn build_body_galley` → `pub(crate) fn build_galley_for_blocks_inner`, which passes `None` for `fn flow_body_into`'s `resume: Option<NumberingState>` → `numbering: resume.unwrap_or_default()` |

**Headers, footers and text boxes each get a fresh counter**, because `fn flow_running_blocks`
builds its own `FlowCtx` with `numbering: NumberingState::new()`. For a header that is
correct — each header is its own story and re-flows per section and parity `[K]`. For a
**text box** it means the same `numId` can print `1.` inside the box and `7.` in the body;
Word's behaviour here is genuinely subtle and the author will not guess (§0.2a).

**Footnotes and endnotes get a fresh counter per note**, via
`pub(crate) fn build_galley_for_note_blocks` → `build_galley_for_blocks_inner(..., None, ...)`.
Correct `[K]` — but an **endnote rendered as trailing body content** (`fn blocks_with_endnotes`,
test `fn endnote_body_is_appended_as_ordinary_body_content`) goes through the **body** galley
and therefore shares the body's counters. So the same endnote can be counted two ways
depending on which path renders it. **UNVERIFIED** whether both paths are live at once.

The section-break row's fix is small and local: thread the `MeasureResume`/`NumberingState`
that already exists for chunked measurement between `SectionRun`s.
`pub struct MeasureResume { numbering: NumberingState }` and
`FlowCtx::resume_snapshot: Option<NumberingState>` are already the mechanism, and
`fn flow_body_into` already ends `ctx.resume_snapshot.take().unwrap_or(ctx.numbering)`.
**Grade: engine** — a layout threading change, not a facade one, and the only `engine` grade
here that is not about the op set or an import mapping. No test covers it.

### 1.11 The marker's own formatting

**Docs `[K]`:** clicking a marker selects all the list's markers and formatting applies to
them. **Word `[K]`:** the same, and the classic use is a bold number with unbold text.

**Ours: the marker takes the LEVEL's `w:rPr` layered over the paragraph STYLE's run cascade,
and nothing in the product can write a level's `w:rPr`.** Verbatim, from
`fn prepare_list_marker` in `crates/casual-doc-layout/src/flow.rs`:

```rust
let level_rpr = resolved.run_properties.unwrap_or_default();
let mut effective = ctx.cascade.resolve_run_in_table(ctx.para_style, &level_rpr, ctx.table_style.as_ref());
```

Three consequences, each verified:

1. **The base is the paragraph *style*'s run properties, not the paragraph *mark*'s.**
   `props.mark_run` is never consulted in `prepare_list_marker`, and neither is
   `paragraph.inlines[0]`. So a document whose number takes its face or size from the
   paragraph mark renders wrong. ONLYOFFICE `[S]` gets this right and documents the one
   Word-compat exception: `Paragraph.prototype.GetNumberingTextPr`
   (`reference/sdkjs/word/Editor/Paragraph.js:1877-1920`) merges compiled TextPr → the
   paragraph mark's character style → the paragraph mark's `rPr` → `numLvl.GetTextPr()`
   (`:1906`), with underline from the paragraph mark deliberately dropped (`:1882-1884`,
   *"Word не рисует подчеркивание у символа списка…"*). **Grade: engine** — one more cascade
   layer in `prepare_list_marker`, no new field.
2. **Bolding the list item's text does not bold its number**, because direct run formatting
   is not in the marker's cascade at all. That is the "surprises people when missing" case
   the brief named, and it is a rendering-fidelity row as well as an authoring one.
3. **`NumberingLevel::run_properties` is written exactly once in the whole facade, as
   `None`** (`grep -c 'run_properties: None,' crates/casual-doc-wasm/src/lib.rs` → 1, inside
   `fn list_level`). The field is imported, exported, cascaded and **tested** —
   `fn a_numbered_paragraph_prepends_a_marker_glyph_run_using_the_level_rpr_size` — and
   unreachable from the product. `SKILL.md` §9.4 exactly.

ONLYOFFICE's authoring model is worth copying and worth its caveat: formatting a marker
writes to the **shared level**, so it changes every item at that level of that list
(`reference/sdkjs/word/Editor/Document.js:19990-20010` → `CNum.prototype.ApplyTextPr`,
`reference/sdkjs/word/Editor/Numbering/Num.js:436-456`), and typing any non-format content
while a numbering selection is live **deletes the list** instead
(`CDocument.prototype.RemoveSelectedNumberingOnTextAdd`,
`reference/sdkjs/word/Editor/Document.js:6392-6424`). Per-level, not per-item, is the right
scope for us too — it is what `w:rPr` on `w:lvl` means.

### 1.12 What a list does where it is not the body

Recorded because the owner's uniform-flow-pipeline rule makes this a first-class question
and because the answer is unusually good.

| Surface | Toggle a list? | Change level? | Counter |
| --- | --- | --- | --- |
| body | yes | yes | the document's |
| table cell | yes | yes (Increase Indent; Tab is cell nav — parity, §1.3) | the document's — correct `[K]` |
| header / footer | yes | yes | **its own, restarting per flow** — correct for a header `[K]` |
| footnote / endnote body | yes | yes | its own, per note — correct `[K]` |
| text box | yes | yes | its own — **not compared** (§0.2a) |
| **pasted into any of the above** | **no** — structured paste refuses outside a top-level body paragraph | — | — (§1.14) |

The reach comes from one decision: every list facade method resolves paragraphs through
`fn surface_block_lists`, which covers every surface, and `fn apply_paragraph_props_as`
applies to whatever `fn paragraphs_in_selection` returns. **`fn apply_indent_props` is the
exception** and deliberately skips table-cell paragraphs with a stated reason (*"A body-scale
indent applied to a narrow cell paragraph overflows the cell and mangles the table"*). So
list level works in a cell and ruler indent does not, which is defensible and undocumented.

### 1.13 Import, export, and one serious defect

**The round trip is very strong.** Import parses 22 numbering element names and export
re-emits every modelled field in `CT_Lvl` schema order (`fn numbering_xml`, `fn write_level`
in `crates/casual-doc-export/src/semantic.rs`), including `w:lvlOverride`,
`w:startOverride`, a full nested `w:lvl` redefinition, `w:isLgl`, `w:suff`, `w:lvlJc`,
`w:lvlRestart`, `w:pStyle`, level `w:pPr` and level `w:rPr` — and it preserves *presence* by
emitting a bare `<w:pPr/>` for a `Some(default)`. Six export round-trip tests and nine import
tests name numbering. `docs/44`'s row **P1F-6** (*"only `ilvl`+`start`;
`numFmt`/`lvlText`/`lvlJc`/`suff` dropped"*) is marked Done and is Done.

**But one import mapping silently strips lists from a whole class of real Word documents.**

`fn resolve` in `crates/casual-doc-import/src/numbering.rs` refuses a `w:numPr` whose `ilvl`
is not in a per-instance `valid_levels` set:

```rust
pub(crate) fn resolve(&self, num_id: &str, level: u8) -> Option<NumberingRef> {
    let instance = *self.by_num_id.get(num_id)?;
    if self.valid_levels.get(&instance)?.contains(&level) { Some(NumberingRef { instance, level }) } else { None }
}
```

and `valid_levels` is built from the abstract's **own `w:lvl` children only** — the
`defined: BTreeSet<u8>` accumulated in the `for raw in abstracts` loop and stored as
`abstract_by_key.insert(raw.id.clone(), (id, defined))`. It does **not** consider:

- a level supplied only by `w:lvlOverride/w:lvl` on the instance, and
- a level reachable only through `w:numStyleLink`.

Word's List Styles produce exactly the second shape `[K]`: one `w:abstractNum` carrying
`<w:numStyleLink w:val="…"/>` and **no `w:lvl` at all**, plus a second abstract carrying
`<w:styleLink w:val="…"/>` and the nine real levels. For such a document
`valid_levels[num] == {}`, so `resolve` returns `None` for **every level**, `body.rs`'s
`b"numPr"` close arm reports the feature, and **every paragraph of the list arrives as a
plain paragraph with no marker.**

The bitter part: `crates/casual-doc-layout/src/numbering.rs` has `fn resolve_abstract`, which
follows `w:numStyleLink` through the style to the defining abstract, bounded against cycles,
and it is **tested twice** — `fn num_style_link_resolves_through_to_the_defining_abstract`
and `fn num_style_link_cycle_terminates`. The renderer can do it and the importer never hands
it anything to do it with. `grep -c 'numStyleLink' crates/casual-doc-import/src/tests.rs`
returns **0**.

A second asymmetry in the same family: `fn resolve_numbering_level` in
`crates/casual-doc-model/src/v1/document.rs` validates a `NumberingRef` against
`abstract_num.levels` only, so the **model rejects** (`ModelError::NumberingLevelUndefined`)
a shape the **layout renders**. Nothing produces that shape today, because the importer drops
it first — but it means the fix to `fn resolve` must move the validator too, or importing a
fixed document will fail validation instead of losing a marker.

**Grade: engine** for the import mapping (it is model/import semantics, not facade), and it
is the most serious row in this document: silent-by-default fidelity loss on documents users
already have, against one of the three structural advantages `SKILL.md` §1 says to protect.

Other losses, for completeness, all verified:

| Loss | Where | Reported? |
| --- | --- | --- |
| `w:numPicBullet`, `w:lvlPicBulletId` | not modelled | **yes** — `reporter.report(b"numPicBullet")` then the subtree is skipped as one finding; `lvlPicBulletId` via the element catch-all |
| `w:legacy` / `w:legacyIndent` / `w:legacySpace` | not modelled anywhere (`grep -rn 'legacyIndent' crates/` → 0) | yes, via the catch-all |
| `w:tplc`, `w:tentative` | **attributes**, and `numbering.rs` never calls `report_attribute` (`grep -c report_attribute …/numbering.rs` → **0**) | **no — silently dropped**. A reporting hole: the catch-all sees elements only |
| `w:nsid`, `w:tmpl` | dropped by policy, `fn carries_no_meaning` | by policy, `docs/35` |
| numbering part byte-retention | `word/numbering.xml` is in the `consumed` set, so it is excluded from opaque retention and regenerated from the model in `ImportMode::Semantic` | — |
| ODF `text:continue-numbering` / `text:continue-list` | dropped; each top-level `text:list` gets a fresh instance | **yes** — `"odf.list.continuation"`, Degraded |
| ODF `text:display-levels` | no literal in the repo; multi-level labels like `1.2.3` collapse | yes, generically |
| ODF `text:list-header` | **not handled at all**; its child `text:p` reaches `fn list_numbering`, whose `open_list_items.last_mut().ok_or(OdfError::MalformedContent)?` may make a leading `text:list-header` a **hard import failure** | reported as an unknown element. **UNVERIFIED** (derived from reading, not executed) |
| ODF: the 2nd..nth paragraph of a `text:list-item` | loses list membership entirely — `fn list_numbering` returns early `if !item.first_paragraph` | **no finding at all** |
| DOCX→ODT: `lvl_restart`, `pstyle`, `multi_level_type`, `num_style_link`, `style_link`, `overrides[].definition` | `fn register_numbering`'s loss check omits all six | **no** |
| RTF `\pn` legacy lists | omitted | yes — `"rtf.list.legacy-pn"` |

### 1.14 Paste

**External paste is genuinely good, and it stops at the body.**
`webapp/src/clipboard.mjs`'s `fn collectListItems` walks `<ul>`/`<ol>`, emits one paragraph
block per `<li>` annotated `{ ordered, level }`, recurses into a nested list at `level + 1`,
and excludes an item's nested lists from its own text. So nesting depth maps to `w:ilvl` and
a Google-Docs or Word-HTML list pastes as a real list. `structured-paste.spec.mjs` and
`external-structured-paste.spec.mjs` cover it.

Four gaps:

| Gap | Anchor |
| --- | --- |
| **Pasting into a list joins it** rather than starting a new one, because `fn build_external_block` calls `self.ensure_list(list.ordered)` — the same cached instance as §1.1. Sometimes right, never a choice | `fn build_external_block` |
| **The marker format is not carried.** The contract is `{ ordered, level }`, so an `<ol type="i">` or a `list-style-type: lower-roman` pastes as decimal, and `<ol start="5">` loses its start | the payload built by `fn collectListItems` |
| **A list pasted anywhere but a top-level body paragraph loses its structure entirely.** `pub fn paste_external_structured` refuses with *"paste target is not in the document body"* / *"structured paste targets a body paragraph"*, and the webapp falls back to the flat rich-run path — so a list pasted into a table cell, a header, a footer or a footnote arrives as plain paragraphs | `pub fn paste_external_structured` |
| **No paste-option choice** ("merge with the list above" vs "keep as its own list") | `webapp/src/paste_loss.mjs` reports families; no list option exists |

**ONLYOFFICE `[S]` is worse on three of the four**, which is unusual in this document. Their
merge/don't-merge feature is **commented-out dead code** —
`reference/sdkjs/common/wordcopypaste.js:3505-3515`, with `uniteList: 18, doNotUniteList: 19`
declared in `commonDefines.js:2534-2535` and referenced nowhere in `web-apps` — and **pasting
into a list item forces text-only insertion** (`wordcopypaste.js:4301-4304`, `:4878-4883`,
beside a `//TODO … exception for pasting text into list` comment). They do fabricate fresh
`CNum`s for HTML paste rather than reusing one (`:9266-9320`), which is the §1.1 behaviour we
lack.

**Grade: facade+UI** for the marker format and the start value (one more field on the
external block contract); **facade** for widening the target beyond the body; **UI-only** for
a paste option, once §1.1 has a list identity to offer a choice about.

### 1.15 Per-interaction cost — the O(1) rule

`docs/107` §4 makes per-interaction work O(1) in document size an owner constraint. **Every
list facade call is O(document), and the composed gestures multiply.**

The two primitives, both full walks of every surface's block list:

- `pub fn paragraph_properties` (`crates/casual-doc-edit/src/lib.rs`) —
  `surface_block_lists(document).into_iter().find_map(|blocks| find_paragraph(blocks, node))`.
- `fn ordered_paragraphs` (`crates/casual-doc-wasm/src/lib.rs`) — a full walk that also
  materialises **every paragraph's plain text**; `fn order_endpoints` then runs two
  `position()` scans over the result.

`ParagraphIndex::build` is the existing mitigation (one walk, then hash lookups; 13 call
sites in the facade), and the list methods use it — but they each also call the primitives
around it. Walk counts, derived by reading the call chains:

| Call | Full-document walks |
| --- | --- |
| `listStyleAt`, `listLevelAt`, `listFormatAt`, `checkboxStateAt` | **1** each |
| `canContinueList` | **3** — and it runs from `updateToolbar` on **every selection change** |
| `restartList`, `continueList`, `setListFormat` | **3** each, plus `find_paragraph_mut` per emitted op |
| `toggleList`, `adjustListLevel` | **4–5** — `order_endpoints` runs twice, once in the method and once inside `apply_paragraph_props_as` |
| `adjustIndent`, `setLeftIndent`, `setRightIndent`, `setFirstLineIndent` | **2–3** |

Composed:

- **One Enter inside a list item costs ~7 full-document walks**: `listStyleAt` (1) +
  `paragraphLength` + `listLevelAt` (1) + `adjustListLevel`/`toggleList` (4–5). Enter is an
  interaction.
- **One caret move costs ~5–6**: `doc.listStyleAt` has **8** call sites in `main.js`,
  `updateToolbar` reaches several, and `canContinueList` adds 3.

And numbering makes the two performance tiers that exist give up:

- **Windowed layout**: `fn classify_resume` in `crates/casual-doc-layout/src/windowed.rs`
  returns `FlowResume::FromStart` when `paragraph.properties.numbering.is_some()`, and
  `FromStart`'s own doc says *"time is `O(document)` per window."* **One numbered paragraph
  anywhere in the body demotes every window in a 1.3M-paragraph document.**
- **Incremental galley reuse**: `struct RetainedBlock`'s `reusable` field is documented false
  for *"a numbered paragraph (flowing it advances the list counters the numbered paragraphs
  after it read)"*, so `fn reuse_block` refuses every numbered block.

Both are *correct* — the counter really is a cross-block dependency — and both are the
textbook case for a **prefix-sum / checkpoint index**: snapshot `NumberingState` every k
top-level blocks and let a window resume from the nearest checkpoint rather than from the
start. The mechanism already exists as `pub struct MeasureResume` and
`FlowCtx::resume_snapshot`; what is missing is storing more than one. Named, not designed,
because it is a windowed-layout row (`docs/113`) rather than a list-experience one.

**Grade: facade** for the walk counts; **engine** for the checkpoint index. **This blocks
nothing in §4** — none of D-1, D-2 or D-3 is hover-driven, which is the trap `docs/141` §1.13
found for tables, where one gesture cost 80 page-tree scans.

### 1.16 Touch

- **Nothing in the webapp knows what a touch is.** `grep -rn 'pointerType' webapp/src/ | wc -l`
  → **0**. The same figure `docs/141` §1.14 published for tables; it is a product-wide row.
- **The checklist marker is the only list gesture with a pointer target at all.**
  `js_name = checklistMarkers` returns each marker's engine rect and node, and
  `webapp/src/main.js` hit-tests a click against them to call `toggleChecklistItem`. The
  target is the **glyph's own box** — a `☐` at body size, well under WCAG 2.2's 24×24 CSS px
  (2.5.8) and 44×44 (2.5.5). Those are W3C figures, not a competitor claim.
- **The compact ("Docs-shaped", mobile) toolbar carries three list commands** —
  `paragraph.list.checklist`, `.bullet`, `.numbered` — plus both indent commands. Restart,
  Continue and the marker galleries are **not** there, so on a phone they are reachable only
  through the menu bar. Better than tables, where the compact toolbar has nothing
  (`docs/141` §1.14).
- **No long-press affordance** anywhere, so Docs' press-and-hold on a marker has no analogue.

**Grade: UI-only**, plus design; D-2 and D-3 each state their touch path.

### 1.17 Refusal behaviour — three vocabularies for one feature, and one from the wrong domain

`SKILL.md` §10: never a dead control, and a refusal must say something actionable. List paths
run through **three** different catch blocks, and the worst of them mentions tables.

| Helper | What it says on refusal | Which list gestures use it |
| --- | --- | --- |
| `runEdit` | `editRefusalMessage(err, …)` → *"That edit isn't supported for this selection yet"* (`GENERIC` in `webapp/src/edit_errors.mjs`) | the **Tab / Shift+Tab** branch |
| `runToolbarEdit` | a hardcoded `setStatus("That tracked format is not supported for this selection yet", "error")` — **it does not call `edit_errors.mjs` at all**, and "tracked" is wrong outside Suggesting mode | bullet / numbered / checklist buttons and commands, **Enter-ends-list**, **Backspace-outdents**, Increase/Decrease Indent |
| `runNodeEdit` | `setStatus(err?.message ?? "Table change could not be applied", "error")` — the raw engine string, untranslated, with a **table** fallback | **Restart**, **Continue**, the **marker galleries**, **checklist toggle** |

So restarting a bullet list puts the engine's own `"restart numbering requires a numbered
list item"` on the status line in English, in a status channel that is otherwise
`data-i18n`-driven; and if a list method ever threw without a message, a list user would be
told a *table* change failed. `docs/141` TBL-04 files `runNodeEdit`'s hole for tables — the
fix is shared, and this document adds the list instance and the second, worse offender
(`runToolbarEdit`).

`edit_errors.mjs` maps nothing list-specific, by design: it holds `GENERIC`, `HISTORY`,
`VIEWING` and a pass-through for engine strings already prefixed `refused: `, and its own
comment explains why a list of remembered sentences is the wrong shape. **So the engine side
has to carry the code**, not the webapp side.

**The facade's list refusals are free-form unlocalised English with no code.** Twenty-six
distinct strings, all constructed at the throw site and passed to `fn to_js(String)`:

```
list level adjustment requires a list paragraph
restart numbering requires a numbered list item        (thrown at two sites)
continue numbering requires a numbered list item       (thrown at two sites)
list formatting requires a list item
not a list item          /  not a checklist item
numbering definition not found  /  abstract numbering not found  /  numbering level not found
bullet marker requires a glyph  /  unsupported list format: {spec}
no contiguous numbered items to restart  /  no contiguous numbered items to continue
no preceding numbered list at this level to continue
this item is already part of a continuous numbered list
no list paragraphs to reformat  /  no paragraph in selection  /  no body paragraph in selection
paragraph not found  /  start node not found  /  end node not found
invalid node id: {node}  /  invalid start node  /  invalid end node
id space exhausted
```

Four consistency defects inside that list: *"list paragraph"* vs *"list item"* for the same
condition; `"paragraph not found"` and `"start node not found"` for the same failure;
`"numbering definition not found"` thrown from two methods with no way to tell which; and a
host cannot branch on any of them without string matching. **Grade: facade** — the same row
`docs/141` filed as TBL-24, and it should be filed once for the product, not twice.

Two refusals that are **good** and should be the model: the palette's
`disabledReason: "There is no earlier numbered list to continue"`, and the Tab branch's
Suggesting message, which names the mode to switch to.

**One silent refusal.** `pub fn set_list_format` accepts only six tokens
(`fn number_format_from_token`: `decimal`, `decimalZero`, `lowerLetter`, `upperLetter`,
`lowerRoman`, `upperRoman`) while `pub fn list_format_at` **emits** a wider set (`ordinal`,
`cardinalText`, `ordinalText`, `none`, and `Other(…)` verbatim). So on an imported
`w:numFmt="ordinal"` list the gallery shows **no cell checked** — `fn reflectListGallery`
compares `cell.dataset.spec === current` and nothing matches — and there is no way back to
`ordinal` after picking anything else. `list_format_at`'s own doc comment claims *"The same
tokens are accepted back by `set_list_format`"*, which is false.

### 1.18 `toggleList` destroys the level

`pub fn toggle_list` always writes `NumberingRef { instance, level: 0 }`. So switching a
nested item from bullet to numbered, or numbered to checklist, **flattens it to level 0** —
and doing it over a selection flattens every level in the selection. Word and Docs preserve
the level when a list's kind changes `[K]`. `pub fn toggle_checklist_item`, by contrast, does
preserve it (`let level = reference.level; … NumberingRef { instance, level }`), so the
correct pattern is already in the same file. **Grade: facade**, three lines.

### 1.19 Everything round-trip-complete and unauthorable

All `SKILL.md` §9.4 instances: the model has the field, the importer reads it, the exporter
writes it, the layout consumes it, and nothing in the product can set it.

| Field | Layout consumer | Guarded by | Writer | Grade |
| --- | --- | --- | --- | --- |
| `w:isLgl` (legal numbering) | `fn format_level_value`: `let fmt = if current.is_lgl { &NumberFormat::Decimal } else { … }` | `fn is_lgl_forces_decimal_for_all_substituted_levels` | **none** — the facade's only mention is `is_lgl: false` | facade+UI |
| `w:startOverride` at an arbitrary value | `instance.overrides…and_then(\|o\| o.start)` beats the level's `start` | `fn start_override_wins_over_override_definition_start` | **partial** — `restart_list` writes it, always as `1` | facade+UI |
| `w:lvlRestart` | `fn level_resets_on` (`None` reset, `Some(0)` never, `Some(k)` anchored) | `fn lvl_restart_zero_never_resets_deeper_level`, `fn lvl_restart_anchor_limits_which_level_resets` | **none** | facade+UI |
| `w:suff` (tab / space / nothing) | `fn prepare_list_marker` folds `Space`; `pub fn body_indent` handles `Tab`/`Nothing` | `fn body_indent_hanging_and_overflow`, `fn body_indent_suffix_tab_honors_explicit_tab_stops` | **none** — always `Some(LevelSuffix::Tab)` | facade+UI |
| level `w:rPr` | the marker cascade (§1.11) | `fn a_numbered_paragraph_prepends_a_marker_glyph_run_using_the_level_rpr_size` | **none** | facade+UI |
| level `w:pPr` indent + tabs | merged below the paragraph's own, per field | `fn level_indent_is_surfaced_for_merging`, `fn a_levels_authored_list_tab_reaches_the_resolved_marker_and_moves_the_body` | **none** | facade+UI |
| `w:lvlOverride/w:lvl` full redefinition | `fn effective_level` replaces the abstract level | `fn full_level_override_replaces_abstract_level`, `fn override_definition_applies_to_substituted_deeper_level` | **none** | facade+UI |
| `w:numStyleLink` | `fn resolve_abstract`, cycle-bounded | `fn num_style_link_resolves_through_to_the_defining_abstract`, `fn num_style_link_cycle_terminates` | **none**, and **unreachable from DOCX import** (§1.13) | engine |
| `w:lvlJc` (right-aligned numbers) | **none** — §8's filtered recipe → 0 | — | none | **engine** |
| `w:lvl/w:pStyle` | **none** — §8's filtered recipe → 0 | — | none | **engine** |
| `w:multiLevelType` | **none** | — | `None` always | facade+UI (D-3 needs it as metadata only) |
| the rest of a level's `w:pPr` (alignment, spacing, keeps) | **none** — `ResolvedMarker` carries only `level_indent` and `level_tabs` | — | none | engine |

`w:isLgl` is the cheapest row here and one of the cheapest in the document: one checkbox, one
facade field, and a layout consumer that already exists and is tested. Note ONLYOFFICE cannot
author it either (§1.7), so this is a lead over them as well as a Word-parity row.

### 1.20 Test coverage

| Area | Coverage |
| --- | --- |
| Marker galleries | **good** — `list-marker-gallery.spec.mjs`, 3 cases incl. the Viewing block and one-undo |
| Restart / Continue | **good** — `list-continue-numbering.spec.mjs`, 2 cases incl. a palette reach |
| Checklist | **good** — `checklist.spec.mjs`, 3 cases |
| Tab demote → a11y nesting | `a11y-list-nesting.spec.mjs`, 3 cases. **The only Tab-in-a-list assertion in the product** |
| The numbering engine | **excellent** — 22 unit tests in `crates/casual-doc-layout/src/numbering.rs` |
| Import / export round trip | **good** — 9 import + 6 export tests name numbering |
| `adjust_list_level` | **nothing.** No native test, no clamp test, no refusal test |
| `adjustIndent` / `setLeftIndent` / `setFirstLineIndent` / `paragraphIndent` | **nothing** |
| Enter-ends-list, Backspace-outdents | **nothing** |
| Mixed selections (§1.8) | **nothing** |
| Counters across a section break (§1.10) | **nothing** |
| `numStyleLink` on **import** (§1.13) | **nothing** — `grep -c numStyleLink crates/casual-doc-import/src/tests.rs` → 0 |
| `listFormatAt` → `setListFormat` token asymmetry (§1.17) | **nothing** |
| A level > 0 surviving a `toggleList` (§1.18) | **nothing** — and it does not survive |

---

## 2. The ranked gap table

**Row ids are `LST-nn` and are claimed by this document.** They are not tracker ids;
`docs/104`, `105`, `109` and `14` are deliberately untouched (§6 lists what to file).

**Ordering rule.** Ranked by *value removed per unit of cost*, so the table can be worked
straight down. "Value" is how much of the felt experience the row accounts for; "cost" is the
grade plus the surface area. Ties break towards the row that unblocks another row.

**Cost key.** XS = under a day, one call site. S = a few days, one module. M = a lane-week.
L = a lane-month or a new type in a shared crate.

| # | id | Gap | Evidence | Class | Cost | Unblocks |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | **LST-01** | Nested numbered levels are `1.1.` / `1.1.1.` and step 360 twips; Word and Docs give `a.` / `i.` stepping 720 (1.2) | `[K]` Word high / Docs medium | facade | XS | — |
| 2 | **LST-02** | `toggleList` writes `level: 0`, so switching a nested item's list kind flattens it (1.18) | `[K]` | facade | XS | — |
| 3 | **LST-03** | Three refusal vocabularies on list paths; one says *"Table change could not be applied"*, one says *"tracked"* outside Suggesting (1.17) | §10 rule | UI-only | XS | — |
| 4 | **LST-04** | Restart / Continue ship disabled on the ribbon with **no stated reason**, while the palette explains itself (1.6) | §10 rule | UI-only | XS | — |
| 5 | **LST-05** | `w:isLgl` is consumed, tested and unauthorable — one checkbox (1.19) | `[K]` Word; ONLYOFFICE lacks it too `[S]` | facade+UI | XS | — |
| 6 | **LST-06** | Every numbered list in a session is **one list**; the second starts at 4 (1.1) | `[K]`, and `[S]` for the join-or-mint rule | facade | S | LST-13, LST-21 |
| 7 | **LST-07** | Mixed selections: refused one way, silently indented the other, and the button reports a definite state (1.8) | `[S]` `Document.js:8799` | UI-only | S | — |
| 8 | **LST-08** | `listFormatAt` can return a token `setListFormat` rejects; an imported `ordinal` list shows no checked cell and cannot be restored (1.17) | — | facade | XS | — |
| 9 | **LST-09** | `restart_list` breaks at the first deeper item, so restarting a multilevel list leaves two items numbered `1.` (1.6) | `[K]` | facade | S | — |
| 10 | **LST-10** | **A `w:numStyleLink`-only abstract loses every marker on import**, although `fn resolve_abstract` follows the link and is tested twice (1.13) | `[K]` for Word's output shape | **engine** | S | — |
| 11 | **LST-11** | Changing a list level is reachable from **one** surface (Tab), or from Increase Indent, which does not say so. No command id, no chord, no level picker (0.5, 1.9) | `[S]` they have a nine-item picker | UI-only | S | — |
| 12 | **LST-12** | No autoformat: typing `1. ` or `- ` stays literal (1.5) | `[S]` in detail | UI-only + facade | M | — |
| 13 | **LST-13** | Restart is hard-coded to 1; no "set numbering value" (1.6) | `[S]` `api.js:4366` | facade+UI | S | — |
| 14 | **LST-14** | The marker's base run properties come from the paragraph **style**, not the paragraph **mark**, so a Word number's face can render wrong (1.11) | `[S]` `Paragraph.js:1877-1920` | **engine** | S | LST-15 |
| 15 | **LST-15** | No multilevel scheme picker and no per-level list settings: `suff`, `lvlJc`, `lvlRestart`, level `rPr`, level indent and per-level markers are all unauthorable (1.7, 1.19) | `[S]` `ListSettingsDialog.js` | facade+UI | M | — |
| 16 | **LST-16** | Numbering definitions are minted outside the op set: undo orphans them, `numbering.xml` grows monotonically, and OT has no wire form for a list (0.4) | HF-107, ADR-033 | **engine** | M | the collaboration lane |
| 17 | **LST-17** | Counters restart at a **section break**; Word continues (1.10) | `[K]` | **engine** | S | — |
| 18 | **LST-18** | One Enter in a list item costs ~7 full-document walks; `canContinueList` costs 3 on every selection change (1.15) | `docs/107` §4 | facade | M | — |
| 19 | **LST-19** | Any numbered paragraph forces `FlowResume::FromStart`, so every window is O(document), and no numbered block is ever reused incrementally (1.15) | `docs/107` §4, `docs/113` | **engine** | M | large documents |
| 20 | **LST-20** | A list pasted anywhere but a top-level body paragraph loses its structure (1.14) | uniform-pipeline rule | facade | S | — |
| 21 | **LST-21** | Pasted lists carry no marker format and no `<ol start>`, and pasting into a list silently joins it with no choice (1.14) | `[S]` theirs is worse | facade+UI | S | needs LST-06 |
| 22 | **LST-22** | In Suggesting mode Enter on an empty list item makes another bullet; the rule is bypassed and undocumented (1.4) | `[K]` | UI-only | XS | — |
| 23 | **LST-23** | Facade list refusals are 26 free-form unlocalised English strings with no code; four are inconsistent with each other (1.17) | — | facade | M | shared with `docs/141` TBL-24 |
| 24 | **LST-24** | Tab demotes from anywhere in the item, so no keystroke can put a tab inside a list item, and the divergence is not recorded in the code (1.3) | `[S✓]` `Document.js:8810` | UI-only | XS | — |
| 25 | **LST-25** | `w:lvlJc` (right-aligned numbers) has **no layout consumer** (1.19) | `[K]` legal documents rely on it | **engine** | S | quality of LST-15 |
| 26 | **LST-26** | `w:lvl/w:pStyle` has no layout consumer, and the rest of a level's `w:pPr` (alignment, spacing, keeps) is dropped (1.19) | — | **engine** | S | quality of LST-15 |
| 27 | **LST-27** | The context menu wires `paragraph.restart` and `paragraph.continue` under ids the registry does not answer — two more of `docs/130` §11.1's family, which counted 5 and should count 7 (0.5) | `docs/130` §11.1 | UI-only | XS | surface parity |
| 28 | **LST-28** | The bullet cycle is `• ◦ ▪ –` mod 4, so levels 0 and 4 are identical; Word `• o ▪`, Docs `• ○ ■` (1.2) | `[K]` | facade | XS | — |
| 29 | **LST-29** | A checklist marker's tap target is the glyph's own box, under WCAG 2.5.8's 24×24; `pointerType` has zero occurrences in `webapp/src` (1.16) | WCAG 2.2 | UI-only | S | — |
| 30 | **LST-30** | Restart, Continue and the marker galleries are absent from the compact (mobile) toolbar (1.16) | `[K]` | UI-only | XS | — |
| 31 | **LST-31** | `w:tplc` / `w:tentative` are dropped **silently**: `numbering.rs` never calls `report_attribute` (1.13) | no-silent-loss rule | **engine** | XS | — |
| 32 | **LST-32** | ODF: `text:list-header` unhandled and possibly a hard import failure; `text:display-levels` lost; the 2nd+ paragraph of a `text:list-item` loses membership with **no finding** (1.13) | — | **engine** | M | — |
| 33 | **LST-33** | DOCX→ODT silently drops `lvl_restart`, `pstyle`, `multi_level_type`, `num_style_link`, `style_link` and `overrides[].definition` (1.13) | no-silent-loss rule | facade | S | — |
| 34 | **LST-34** | The model's `resolve_numbering_level` is stricter than the layout's `effective_level`, so fixing LST-10 without moving the validator turns a dropped list into a rejected document (1.13) | — | **engine** | XS | gates LST-10 |
| 35 | **LST-35** | An endnote rendered as trailing body content shares the body's counters while the same endnote rendered as a note does not (1.10) | — | **engine** | S | **UNVERIFIED** |
| 36 | **LST-36** | No test drives Tab, Enter-ends-list, Backspace-outdents, a mixed selection, `adjust_list_level` at all, or any indent method (1.20) | test gap | UI-only | S | every row above |
| 37 | **LST-37** | `w:numPicBullet` and `w:legacy*` are not modelled at all — reported, so not silent, but a lossy round trip for legacy-authored lists (1.13) | — | **engine** | M | — |

### 2.1 The shape of the table

**These figures are extracted from the table above by a command, not counted by hand** — and
they are extracted because counting them by hand failed three times in a row (below). The
command is in §8; its output is:

| Class | Count | Rows |
| --- | --- | --- |
| **UI-only** | **10** | LST-03, 04, 07, 11, 22, 24, 27, 29, 30, 36 |
| **UI-only + facade** | **1** | LST-12 |
| **facade+UI** | **4** | LST-05, 13, 15, 21 |
| **facade alone** | **10** | LST-01, 02, 06, 08, 09, 18, 20, 23, 28, 33 |
| **engine** (each also needing facade or UI) | **12** | LST-10, 14, 16, 17, 19, 25, 26, 31, 32, 34, 35, 37 |

10 + 1 + 4 + 10 + 12 = **37**.

**Three hand counts of this one table were wrong, in three different ways**, and the fourth
attempt was to stop hand-counting. Pass 1 gave UI-only 11 by filing LST-12 there when it needs
a facade method for its undo guarantee (§4.2.2), and gave `engine` 13 while listing twelve
rows. Pass 2 fixed those and then dropped LST-23 from the `facade` list. Pass 3 caught LST-23
and dropped **LST-28**. Only the `awk` extraction agreed with the table. `docs/141` §0.3
predicted two wrong numbers and found two; this is the same failure mode three times in one
table, and the lesson is not "count more carefully" — it is `SKILL.md` §8's *counts in docs
must be derived, not hand-maintained*, applied to a table inside the document itself.

The distribution is the headline for planning, and it is the **opposite** of `docs/141`'s:

> The table experience was not blocked on the engine — 29 of 38 rows were UI or facade. The
> **list** experience is a third engine, and the engine rows are not decoration: LST-10 is
> silent fidelity loss on real documents, LST-16 is the collaboration blocker, LST-19 is the
> large-document blocker, and LST-14 is a rendering defect.

But the *cheap* rows are cheap in a way the table rows were not. **The five highest-value
rows are XS or S and none of them needs a new operation**: three constants (LST-01), three
lines (LST-02), one catch block (LST-03), one `title` assignment (LST-04), one checkbox
(LST-05). LST-06, the row the whole document turns on, is one adjacency rule.

---

## 3. Where we are ahead, with the same rigour

`docs/99` §9.6 and `SKILL.md` §9.6: understating our own position is also false. Every claim
here is anchored the same way as the gaps.

| We lead on | Evidence | Against |
| --- | --- | --- |
| **Checklists as a first-class list kind** | `toggleList(..., "checklist")`, `#checkList`, `paragraph.list.checklist` on five surfaces, `js_name = toggleChecklistItem`, `js_name = checklistMarkers` giving each marker an engine rect so the checkbox is clickable in the document, `fn ensure_checklist` round-tripping to DOCX as a checkbox bullet list, three e2e cases | ONLYOFFICE has **no** checklist kind at all — `docs/130` §5 records the absence from `sdkjs/word/Numbering/` and their `locale/en.json`. Docs has checklists `[K]`, so this is an ONLYOFFICE lead |
| **`w:startOverride` is the restart mechanism** | `pub fn restart_list` writes `NumberingOverride { level, start: Some(1), definition: None }` on a fresh instance — the OOXML construct for exactly this | ONLYOFFICE `[S]` **avoids** it: `RestartNumbering` clones the whole `CNum` and edits `Lvl.Start` (`reference/sdkjs/word/Editor/Document.js:25192-25199`), and `grep -rn "StartOverride\|LvlOverride" web-apps/apps` → nothing. Ours is the higher-fidelity representation |
| **Continue numbering is offered only when it would work** | `js_name = canContinueList` walks back to the nearest earlier item at the caret's level and returns whether it is a *different* instance; it drives `continueListBtn.disabled` and the palette row's `enabled` | ONLYOFFICE `[S]` shows *"Continue numbering"* whenever the caret is in a list (`web-apps/.../DocumentHolderExt.js:2526-2531`, `setVisible(in_list)`) and lets it fail. Docs `[K]` likewise offers it unconditionally |
| **Restart and Continue are on the ribbon, not only in a context menu** | `#restartList`, `#continueList` on the Home band, plus the Format menu, the context menu and the palette — four surfaces | ONLYOFFICE `[S]` has them **only** in the paragraph context menu (`DocumentHolderExt.js:2290-2302`). Docs `[K]` likewise right-click only |
| **A screen reader reads a demoted list as a nested list** | `webapp/src/a11y_mirror.mjs` projects list items into real `ul`/`ol` with checkbox markers as `role="checkbox"` + `aria-checked`, windowed so a million-block document costs the same as a small one; guarded by three cases in `a11y-list-nesting.spec.mjs` including *"promoting with Shift+Tab closes the nested list again"* | this is the class of defect that makes a list unreadable to assistive technology, and it is guarded at the projection layer rather than asserted about the canvas |
| **Lists work on every surface the pipeline reaches** | `fn surface_block_lists` under `paragraph_properties` and `ordered_paragraphs`, so toggle and level change work in a table cell, a header, a footer, a footnote, an endnote and a text box (§1.12) | the owner's uniform-flow rule honoured in a feature where it is easy not to. `docs/141` found tables **cannot** be inserted anywhere but the body (`container: None` hardcoded) |
| **The numbering engine's spec coverage** | `fn resolve_abstract` (numStyleLink, cycle-bounded), `fn effective_level` (full `w:lvlOverride/w:lvl`), `w:startOverride` precedence, `fn level_resets_on` (`w:lvlRestart` `None`/`0`/anchored), `is_lgl` forcing decimal across substituted levels, `%1`..`%9` with `%0` and a lone `%` as literals, `pub fn body_indent` for all three `w:suff` values, a bullet-scale correction so a sub-list `○` is a small ring, and the Wingdings/Symbol→Unicode `fn map_marker_glyphs` remap with the symbol face dropped so the mapped char does not render as tofu — **22 unit tests** | genuinely deep engine work, and it is why most of §2 grades `facade` or lighter. **It is also why §1.13 and §1.19 hurt: the engine is ahead of every path that could reach it** |
| **One numeral formatter for lists and notes** | `pub(crate) fn format_number` in `numbering.rs` is shared with `crate::note_numbering`, with `fn roman`, `fn letters` (wrapping past `z` to `aa`), `fn ordinal`, `fn cardinal_text`, `fn ordinal_text`, `fn chicago` | one mechanism, not two — `SKILL.md` §8's *"prefer one mechanism over two"*, in a place where two would silently diverge |
| **The marker contributes no caret stops** | every caret-stop builder in `crates/casual-doc-layout/src/hittest.rs` filters `!run.is_marker`; guarded by `fn a_list_markers_glyphs_contribute_no_caret_stops` | clicking a number puts the caret in the text, not "inside" the number. A small thing that is wrong in a lot of editors |

**One thing not to overclaim.** The engine's completeness does **not** transfer to the
experience, and it is why the owner's report can be true at the same time as 22 passing
numbering tests: **those tests are about rendering a definition, not about authoring one.**
`docs/76`, the only list design document in the repository, is seven lines long and covers one
gesture.

---

## 4. The designs

### 4.0 Rows 1–5 and 8 need no design beyond their own row

The six cheapest rows in §2 are each one or two call sites, and their §1 subsections already
contain everything a lane needs. Restated as one-line specs so the queue can be worked from
the top without reading back.

| id | The change |
| --- | --- |
| **LST-01** | In `fn list_level` (`crates/casual-doc-wasm/src/lib.rs`), replace the concatenated placeholder template with the level's own placeholder and a per-level format: `num_fmt` cycling `Decimal, LowerLetter, LowerRoman` by `level % 3`, `lvl_text` = `format!("%{}.", level + 1)`, and `start_twips: Some(720 + i32::from(level) * 720)` with `hanging_twips: Some(360)` unchanged. Bullets: `["\u{2022}", "\u{25cb}", "\u{25aa}"][usize::from(level) % 3]` — LST-28 in the same edit. **Mutation proof:** set the cycle length to 1 and assert a level-1 item's marker is `2.` not `b.`. No test pins the current values, so this change needs its own new guard, which is the point of writing one. |
| **LST-02** | In `pub fn toggle_list`, read the current level before writing: `let level = current.map_or(0, \|n\| n.level);` and write `NumberingRef { instance, level }` when turning a list **on over an existing list item**, `level: 0` when creating one from a plain paragraph. `pub fn toggle_checklist_item` in the same file is the pattern. **Mutation proof:** demote an item to level 2, switch bullet→numbered, assert `list_level_at` is still 2. |
| **LST-03** | Point both catches at the one helper: `runToolbarEdit`'s hardcoded `"That tracked format is not supported for this selection yet"` and `runNodeEdit`'s `err?.message ?? "Table change could not be applied"` both become `setStatus(editRefusalMessage(err, { editingUnavailableReason: readOnlyReason }), "error")` — the call `runEdit` already makes. Then add a unit guard that no `setStatus` in `main.js` passes a raw `err.message` and that no refusal fallback names a feature the caller is not in. `docs/141` TBL-04 is the same fix for `runNodeEdit`; **do it once.** |
| **LST-04** | In `updateToolbar`, wherever `restartListBtn.disabled` / `continueListBtn.disabled` is set, set `control.title` to the sentence the palette already uses (*"Place the caret in a numbered list"*, *"There is no earlier numbered list to continue"*) and restore `authoredTitle(control)` when enabled — the helper exists and `#tableStyleBtn` already uses it. Extend the guard `docs/141` TBL-03 proposes to cover the Home band's list controls rather than writing a second one. |
| **LST-05** | Add `legalNumbering` (`w:isLgl`) to the per-level write path D-3 introduces, and — so it ships before D-3 — one checkbox in the paragraph properties panel's list section, *"Number all levels with digits (legal style)"*, writing the caret level's `is_lgl`. The layout consumer and its test already exist. Reachability: panel + palette = two surfaces, no band width. |
| **LST-08** | Make the two token sets one. Either widen `fn number_format_from_token` to every token `fn number_format_token` emits — the cheap, correct direction, since `Ordinal`, `CardinalText`, `OrdinalText` and `None` are all renderable and tested — or have `pub fn list_format_at` return `""` for a token the setter cannot accept so the gallery is honestly blank. **Widen:** a format the engine renders and the picker cannot name is a capability we already paid for. Add the four gallery cells, and a guard asserting `number_format_from_token(number_format_token(f)) == Some(f)` for every `NumberFormat` but `Bullet` and `Other`. |

Also fix while in the area: **LST-27** — give the four context-menu rows the registry's own
ids (`paragraph.list.bullet`, `.numbered`, `.restart`, `.continue`) so a surface-parity guard
can see them, and delete the duplicate `doc.toggleList` call sites. `docs/130` §11.1 owns the
family; this adds two rows to it.

The three designs below are the top three rows a lane cannot build without design.

---

### 4.1 Design D-1 — list identity (LST-06, and LST-09, LST-13, LST-21 fall out of it)

**The named pattern, first.** This is **adjacency-based identity assignment** — the rule any
editor uses to decide whether a new list item continues the list above it, and the rule
ONLYOFFICE already implements: `private_GetSuitableNumPr`
(`reference/sdkjs/word/Editor/Paragraph/Run/RunAutoCorrect.js:991`) joins the previous
paragraph's list when it is compatible and mints a new one otherwise. Nothing here is novel;
the current behaviour is the *absence* of the rule, not a different rule.

The second pattern is **get-or-create keyed on a computed key** rather than on a session
field: `fn ensure_list`'s defect is that its cache key is "the kind" when it should be "the
kind **and** the list we are joining".

#### 4.1.1 The rule

On `toggle_list(kind)` turning a list **on**, for the first paragraph of the selection:

1. Look at the **immediately preceding paragraph in document order, within the same
   surface**. If it carries a `NumberingRef` whose instance classifies to the same *kind*
   (`pub fn list_style_at`'s classification), **join that instance** at that paragraph's
   level — Word's behaviour `[K]`, and the level rule LST-02 already needs.
2. Otherwise, if the **immediately following** paragraph is a compatible list item, join
   *it* — toggling a list on the paragraph above an existing list should extend it, which is
   what a user typing backwards expects.
3. Otherwise **mint a fresh instance**. For `"bullet"` and `"checklist"` this may reuse the
   cached instance (a bullet level has no counter, so sharing is unobservable and the existing
   guard stays green); for `"numbered"` it must be fresh.

`fn ensure_list` keeps its memo for bullets and checklists and gains a
`fn new_numbered_list()` sibling for step 3. **No new operation**: the definition insertion is
the same `definitions_mut()` write the method already performs — which is LST-16's problem,
not D-1's, and §4.1.5 says why D-1 must not wait for it.

#### 4.1.2 The one facade addition

```
listInfo(node: &str) -> ListInfo
```

A `TableInfo`-shaped getter, because four scalar calls that each walk the document is the
wrong shape (§1.15) and because every gesture in this section needs *the list*, not a property
of one paragraph:

| Getter | Type | Meaning |
| --- | --- | --- |
| `found` | `bool` | the paragraph is a list item |
| `kind` | `String` | `"bullet"` / `"numbered"` / `"checklist"` |
| `instance` | `String` | the hex `NumberingInstanceId` — **the identity that does not exist today** |
| `level` | `i32` | 0-based, `-1` when not a list |
| `format` | `String` | the caret level's marker token (today's `listFormatAt`) |
| `levels` | `u32` | how many levels the abstract defines |
| `value` | `u32` | the number this item actually prints, for a "Set numbering value" field's default |
| `firstInList` | `bool` | drives ONLYOFFICE's first-item rule `[S]` and Restart's enablement |
| `canContinue` | `bool` | today's `canContinueList`, folded in so it is not a second walk |
| `startOverride` | `i32` | the effective start, `-1` when none |

**Built from ONE `ParagraphIndex::build` plus one `ordered_paragraphs`**, so it costs 2 walks
where today's four getters plus `canContinueList` cost 6 — and `updateToolbar` calls it once.
**State the complexity in the doc comment** (`SKILL.md` §8): *"O(document), called once per
selection change; replaces five calls each costing O(document)."* That is not O(1) and does not
pretend to be; LST-18 is the row that fixes the walk itself, and D-1 must not make it worse.

`value` needs the counter, which lives in the layout. `LayoutSnapshot` is already how the
facade asks the layout geometric questions (`cell_rect`, `marker_rects`), so the marker *text*
comes back the same way — `fn marker_rects` already enumerates marker runs per page.

#### 4.1.3 The gestures this settles

| Gesture | Before | After | Drives |
| --- | --- | --- | --- |
| Numbered list at the top, then another at the bottom | one list; the second starts at 4 | two lists, both starting at 1 | `paragraph.list.numbered` (unchanged id) |
| Continue previous numbering | the only way to get one sequence | still the way, and now the *unusual* request it should be | `paragraph.list.continue` |
| Restart numbering on a multilevel list | re-instances a prefix; two items numbered `1.` | carries the whole subtree: the loop's `break` becomes "stop at a paragraph whose level is **less than or equal to** the caret's and whose instance differs", so deeper items follow their parent | `paragraph.list.restart` |
| **Set numbering value** (LST-13) | absent | `restartList(node, startValue)` — the same `w:startOverride` write with the value from a field, defaulting to `listInfo.value` | a new `paragraph.list.startAt` id |
| Paste a list into a list (LST-21) | silently joins | the paste block contract gains `instance: Option<String>`; the clipboard bridge passes `listInfo(target).instance` to join or `null` to mint | `insert.paste` |

#### 4.1.4 States, refusals, keyboard, touch

- **Set numbering value** is a small popover (`registerPopover`, not a modal — `docs/63`'s
  panel contract), one number field, range 1..32767 to match the model's validator, default
  `listInfo.value`, Enter applies, Escape dismisses. Reachability: context menu beside Restart,
  Format menu, palette — three surfaces, **no band width**.
- **Refusals**, written to be actionable:
  - not a list item → the control is disabled with *"Place the caret in a numbered list"* (the
    palette's existing sentence; LST-04 puts it on the ribbon too).
  - a bullet level → *"Bulleted lists have no numbers to restart"* — better than today's
    engine string, which says "requires a numbered list item" about a paragraph that is one.
  - out of range → **refuse, do not clamp.** `set_table_column_width_at`'s silent clamp is
    `docs/141` TBL-25; do not repeat it. *"Numbering can start between 1 and 32767."*
  - Suggesting mode → the existing *"…cannot be tracked yet; switch to Editing"*.
- **Keyboard**: every gesture is already a registry command, so the palette reaches all of them
  by name and the menu gives each a focus path. No new chord — `⌘⇧L` is taken and none of these
  is frequent enough to spend a chord on.
- **Touch**: nothing here is pointer-driven. The new popover gets the same
  `min(340px, calc(100vw - 42px))` fixed-drawer treatment under `@media (max-width: 900px)`
  that `.side-panel` already uses, and the number field gets `inputmode="numeric"`.

#### 4.1.5 Why D-1 must not wait for LST-16

LST-16 (definitions outside the op set) is the correct fix and it is blocked on an owner
decision about the closed op set, recorded on HF-107 since before this document. D-1 makes the
*existing* `definitions_mut()` write happen more often — once per new numbered list instead of
once per session — so it makes LST-16's orphan growth worse in exchange for correct numbering.
That trade is worth naming rather than hiding: **the orphans are invisible and the numbering is
not.** §4.1.6 bounds the cost with an assertion rather than a promise.

#### 4.1.6 The assertions D-1 carries

`docs/141` §4.4: `command-activation-contract`'s inert-control assertion cannot fail (HF-186 —
eight unrelated DOM mutations arrive within 3s of idle), so no design may be verified by it.
D-1's guarantees, each assertable and each able to go red:

1. Two numbered lists separated by a paragraph: the second item of the second list reads `2.`
   in the a11y mirror. **Mutation:** restore the memo → reads `5.`.
2. Toggling numbered on the paragraph directly below a numbered list extends it: the new item
   reads `4.`, not `1.`.
3. Restart on item 1 of `1. / 2. / 2.1 / 3.`: the items read `1. / 2. / 2.1 / 3.` with the
   first item's instance changed and **no two items numbered `1.`**.
4. Set numbering value to 7: the item reads `7.` and the next `8.`; values 0 and 40000 are both
   **refused with the sentence** and the document is unchanged.
5. **Orphan bound:** after 20 toggle-on/undo cycles, `definitions().numbering.len()` grows by
   at most 20 — asserting the growth is linear and bounded rather than pretending it is zero,
   which is the honest guard until LST-16 lands.

---

### 4.2 Design D-2 — autoformat as you type (LST-12)

**The named pattern, first.** **Input-triggered replacement with a single-history-point undo**
— the mechanism the product already has for smart quotes (`fn smartQuoteForTyped` in
`webapp/src/text_rules.mjs`, applied in the printable-character branch of the keydown handler)
and the mechanism ONLYOFFICE uses for lists (`CRunAutoCorrect`, one
`StartAction`/`FinalizeAction` pair around the whole conversion,
`reference/sdkjs/word/Editor/Paragraph/Run/RunAutoCorrect.js:930`, `:951`). The one sub-pattern
worth naming separately is **the trigger is the delimiter, not the pattern**: the conversion
fires on Space or Tab, never on Enter, so `1.` in prose survives.

Nothing novel. The design work is the trigger table, the undo guarantee, and the refusal.

#### 4.2.1 The trigger table

Fires only when **all** of: the caret is collapsed; the paragraph carries **no** numbering; the
caret is at the end of the paragraph's text; review mode is Editing; and the key is **Space**
or **Tab**. The last condition is ONLYOFFICE's `[S]` and it is right — a trigger on Enter would
convert `See item 1.` into a list.

| Typed prefix | Becomes | Drives |
| --- | --- | --- |
| `*` | bullet list, glyph `•` | `paragraph.list.bullet` |
| `-` | bullet list, glyph `–` (en dash) | `paragraph.list.bullet` + `paragraph.listFormat.bullet:–` |
| `>` | bullet list, glyph `»` (our gallery's arrowhead, not Wingdings) | `paragraph.list.bullet` + `paragraph.listFormat.bullet:»` |
| `1.` `1)` | numbered, `decimal` | `paragraph.list.numbered` + `paragraph.listFormat.decimal` |
| `a.` `a)` | numbered, `lowerLetter` | + `paragraph.listFormat.lowerLetter` |
| `A.` `A)` | numbered, `upperLetter` | + `paragraph.listFormat.upperLetter` |
| `i.` `i)` | numbered, `lowerRoman` | + `paragraph.listFormat.lowerRoman` |
| `I.` `I)` | numbered, `upperRoman` | + `paragraph.listFormat.upperRoman` |

**Deliberately narrower than ONLYOFFICE in two places and wider in one.** Narrower: no
Arabic-Indic digits in the first increment (a separate row, filed not designed — it belongs to
the i18n lane), and **no compound `1.2.3.` parsing**, which needs a multilevel definition to
land in and therefore waits for D-3. Wider: `a`/`A`/`i`/`I` are single characters, so the
`sText.length < 2` guard their parser opens with (`RunAutoCorrect.js:1164`) is replaced by "a
recognised marker token followed by `.` or `)`".

**Ambiguity rule, stated because it bites:** `i.` is both `lowerRoman` 1 and `lowerLetter` 9.
Word and ONLYOFFICE both resolve to Roman at value 1 `[S]` (`RunAutoCorrect.js:1311-1349`); so
do we. `a.` is `lowerLetter` because there is no Roman `a`.

**Join-or-mint** is D-1's rule, and this is why D-2 is ranked after it: the typed value should
decide. `1.` at the top of a document starts a list; `4.` directly under an item numbered `3.`
joins it. Until D-1 lands, D-2 mints per the cached instance and the numbering is whatever §1.1
gives — so **D-2 must not ship before D-1**, and that ordering is the design, not a preference.

#### 4.2.2 The conversion, as one undoable step

The guarantee is: **one Ctrl+Z restores the literal characters and the space, and leaves the
paragraph a plain paragraph.** That is the whole feature's acceptability — an autoformat you
cannot escape is worse than none.

There is no compound op and none is needed: `Operation::DeleteText` and
`Operation::SetParagraphProperties` already compose, and `fn apply_action_caret_as` pushes
**one** `HistoryEntry` per call whatever the op count. So the facade gains one method:

```
autoformatList(node: &str, prefixLen: u32, kind: &str, formatSpec: &str) -> EditResult
```

which emits, in one group: `DeleteText` over `[0, prefixLen)`, `SetParagraphProperties`
installing the `NumberingRef`, and — when `formatSpec` differs from the level's current marker
— the abstract clone `set_list_format` already performs. `HistoryKind::ListFormatting`, whose
`undo_label` is *"List formatting"*.

**Why a facade method rather than three JS calls:** three `runEdit` calls are three history
entries, and the guarantee is one. The alternative — a JS-side begin/end transaction — does not
exist in this facade, and inventing it for one feature is the "two mechanisms for one rule"
defect.

**Why the JS side does the *matching*:** the prefix is a string and the rule is a product
decision that has to be localisable and settable, so it belongs beside `smartQuoteForTyped` in
`webapp/src/text_rules.mjs`, as
`fn listAutoformatForTyped(key, { enabled, offset, readPrefix })` — the same signature, so the
two transforms are one shape. It is pure and therefore unit-testable without a browser, which
is how the trigger table gets eight cases cheaply.

#### 4.2.3 States and refusals

- **Never refuses.** If the prefix does not match, the Space is inserted and nothing else
  happens. There is no state in which the user is told a list was not created, because they did
  not ask for one.
- **Viewing / Suggesting**: the transform does not run at all (the guard reads review mode) and
  the Space is inserted normally. Saying *"autoformat is unavailable in Suggesting mode"* on a
  Space keypress would be noise; the tracked-change limitation is already announced when the
  user asks for a list explicitly.
- **A settings toggle**, because ONLYOFFICE has two `[S]`, Word has a page of them, and a user
  typing `1.` in a legal document does not want a list. One row in the existing Tools surface
  beside `tools.smartQuotes`, which is already a palette command with an `on`/`off` label —
  copy that exactly: `tools.autoformatLists`. **Default on**, matching both competitors.
- **The "revert to literal" affordance** is an **owner question**, not a decision this document
  takes: Word has the AutoCorrect Options smart tag, ONLYOFFICE has none `[S]`, and Docs is
  unverified `[K]` (§0.2a). The *guarantee* — one-step undo — is the same either way, so the
  affordance can be added later without changing anything below it.

#### 4.2.4 Keyboard and touch

- **Keyboard is the whole feature**; there is nothing to add.
- **Touch**: a soft keyboard's space bar produces the same `keydown`, so the transform works.
  But swipe-typing, dictation and platform autocorrect **emit no `keydown`** — the keydown
  handler's own comment records this — so on those input paths the transform silently does not
  fire. **State it rather than pretend:** the same limitation already applies to smart quotes,
  and fixing it is an input-path row (`beforeinput`), not a list row. Filed as such in §6.

#### 4.2.5 The assertions D-2 carries

Not `command-activation-contract` (§4.4). Each of these can be driven red:

1. Type `1. Alpha` Enter `Beta`: the a11y mirror shows an `ol` with two items and no literal
   `1.` in the text. **Mutation:** remove the Space trigger → the mirror shows a paragraph whose
   text begins `1. `.
2. Type `1. Alpha`, then **one** undo: the paragraph text is exactly `1. ` and there is no list.
   **Mutation:** split the facade method into two `runEdit` calls → two undos needed, red.
3. Type `See item 1.` and press **Enter**: no list. (The Enter-is-not-a-trigger rule.)
4. Type `1.` then Space in a paragraph that is **already** a list item: the literal stays and no
   second conversion happens.
5. Turn `tools.autoformatLists` off, type `1. `: the literal stays.
6. Eight unit cases over `listAutoformatForTyped` for the trigger table, including `i.` →
   `lowerRoman` and `a.` → `lowerLetter`.

---

### 4.3 Design D-3 — the list definition surface (LST-15, and LST-05, LST-25, LST-26 land in it)

**The named pattern, first.** This is a **live property inspector over a definition**, and this
repository has already built one and proved the shape: the table properties panel
(`#tablePropertiesPanel`) writes every field through **one** JSON patch method,
`js_name = applyTableProperties`, with `serde` `camelCase` + `deny_unknown_fields`, only present
fields written so defaults are not materialised, committed as a single operation — and
`docs/141` §1.6 records it as the row where we are **at or ahead of Docs**. D-3 is that shape
applied to `AbstractNumbering`.

The second pattern is the **gallery of presets over the same payload**: a multilevel scheme is
nine levels of the very same JSON, so the picker and the editor are one mechanism, not two.
ONLYOFFICE `[S]` does exactly this — their eight presets are literal `CNum` JSON
(`web-apps/apps/documenteditor/main/app/view/Toolbar.js:3232-3241`) fed to the same
`asc_AddNewNumbering` the dialog uses.

Nothing novel.

#### 4.3.1 The one facade addition

```
applyListDefinition(node: &str, definitionJson: &str) -> EditResult
```

`node` is any paragraph of the list. The payload mirrors `applyTableProperties`'s contract
exactly — `camelCase`, `deny_unknown_fields`, every field optional, only present fields
written:

```json
{ "multiLevelType": "multilevel",
  "levels": [
    { "level": 0, "numFmt": "decimal", "lvlText": "%1.", "start": 1,
      "suff": "tab", "lvlJc": "start", "isLgl": false, "lvlRestart": null,
      "startTwips": 720, "hangingTwips": 360, "tabTwips": 720,
      "runProperties": { "bold": true } },
    { "level": 1, "numFmt": "lowerLetter", "lvlText": "%2.", "startTwips": 1440 } ] }
```

It does what `pub fn set_list_format` already does — clone the abstract, apply, mint a fresh
abstract and instance, repoint the contiguous run — but across **any subset of levels** and
**every level field**, in one operation group. `set_list_format` becomes a one-line caller of
it, so there is one write path, not two. `fn apply_marker_spec` stays as the
token→`NumberFormat` mapping.

**The `runProperties` field is what closes LST-14's authoring half** — a bold number with
unbold text is `{"levels":[{"level":0,"runProperties":{"bold":true}}]}`. LST-14's *rendering*
half (the cascade base) is separate and is not in D-3.

**No new operation.** The definition write is the same `definitions_mut()` reach-around the
method already performs, and the paragraph repointing is `SetParagraphProperties`. LST-16
remains the row that fixes both, and D-3 makes its orphan growth one-per-apply rather than
one-per-marker-change — **better** than today, because a user adjusting five fields in the
panel produces one definition instead of five.

**Complexity, stated in the doc comment:** *"O(document): one `ParagraphIndex::build` and one
`ordered_paragraphs`; called on field commit, not on keystroke."* Field commit is a change
event, not an interaction in the `docs/107` §4 sense — the same standing
`applyTableProperties` has.

#### 4.3.2 Where it lives — and the width it costs

**Zero ribbon width**, which is also what `docs/130` §8 row 12 concluded independently.

- The **bullet** and **number** split-caret menus (`#bulletGalleryMenu`, `#numberGalleryMenu`)
  each gain a separator and one row: **`List settings…`**. They are already `role="menu"`
  popovers with `.list-gallery-cell` children; a `.menu-item` row below a separator is the
  existing pattern in `#tableMenu`.
- The **number** menu additionally gains a **Multilevel** section above that separator: five
  preset cells, each a `.list-gallery-cell` previewing three levels, with `data-spec` naming a
  preset id rather than a format token. Presets: `1. a. i.`, `1. 1.1 1.1.1`, `I. A. 1.`,
  `1) a) i)`, and `Article I. / Section 1.01 / (a)` — the last because legal outline numbering
  is the case `isLgl` and `lvlJc` exist for and the one a user cannot build by hand.
- A **`Change list level`** submenu, nine rows, one per level, `role="menuitemradio"` checked
  from `listInfo.level` — ONLYOFFICE's control `[S]` (`Toolbar.js:2691-2694`), and the thing
  that closes **LST-11** by giving the level a named, palette-reachable, screen-reader-reachable
  command: `paragraph.list.level.0` … `.8`, a generated family like `paragraph.listFormat.*`,
  declared in `host_contract.mjs` as `family("paragraph.list.level.", "mutate", "always")`.

So the Home band grows by **nothing**, and three capabilities gain surfaces.

#### 4.3.3 The panel

A **live inspector**, not an OK/Cancel modal — `docs/141` §1.6's finding was that our table
surface is already the better shape and Word's dialog is the worse one. Same contract:
`.side-panel` at `flex-basis: 320px`, the `@media (max-width: 900px)` fixed-drawer fallback,
**no Apply/Reset footer**, and a guard asserting neither exists (the table panel has exactly
that guard).

Left column: nine level rows, each previewing its own marker, the active one selected from
`listInfo.level`. Right: the fields for the selected level, grouped as the model groups them:

| Group | Fields | Model field |
| --- | --- | --- |
| Marker | format (the 5 number tokens + LST-08's four + bullet), bullet glyph picker, **number format text** (the `lvlText` string, with an "insert level number" control that appends `%n`) | `num_fmt`, `lvl_text` |
| Numbering | start at, restart after level (a level picker plus "never"), **legal style** | `start`, `lvl_restart`, `is_lgl` |
| Position | number alignment (start / center / end), number indent, text indent, follow number with (tab / space / nothing), tab position | `lvl_jc`, `paragraph_properties.indentation`, `suff`, `paragraph_properties.tabs` |
| Marker format | bold, italic, size, colour, font | `run_properties` |

That is ONLYOFFICE's field list `[S]` (§1.7) minus their "link level to style", which needs
`w:lvl/w:pStyle` to have a layout consumer (**LST-26**) before it would do anything —
`SKILL.md` §10: never a dead control, so it is not in the first increment.

**One field needs engine work before its control can ship**, and it ships **disabled with a
reason** until that lands, never as a control that does nothing: *number alignment* →
**LST-25**, `w:lvlJc` has no layout consumer, so it is disabled with *"Number alignment is not
applied yet"*. The rest of a level's `w:pPr` (alignment, spacing, keeps) is **not offered at
all**, so LST-26's other half raises no dead control.

#### 4.3.4 Refusals

- Not a list item → the `List settings…` row is **disabled with** *"Place the caret in a
  list"*, and the split caret still opens (the marker gallery is useful without it).
- `lvlText` with no placeholder on a numbered level → **allow it** (a literal-text level is
  legal OOXML and `fn format_marker` renders it verbatim) but warn inline: *"This level shows no
  number. Insert a level number to show one."* A refusal here would be wrong; a silent surprise
  would be worse.
- `start` out of 1..32767 → **refuse with the range**, do not clamp (§4.1.4).
- `%9` on level 0 → refuse: *"A level can only show numbers from itself and the levels above
  it."* `fn format_level_value` falls back to the target's `start` rather than failing, so
  without this the user gets a stable wrong number with no explanation.
- Viewing / Suggesting → the panel reflects and does not write, the existing gate.

#### 4.3.5 Keyboard and touch

- The nine level rows are a **roving tabindex** with arrows / Home / End / Escape — the pattern
  the insert-table grid picker already implements and which `docs/141` §3 records as a place we
  are **ahead of Docs**. Reuse it; do not write a second one.
- The preset cells are real `<button>` gridcells inside the existing `role="menu"`, so they are
  tab-reachable with no new mechanism.
- Every field is a native control, so the panel is operable and announced without ARIA beyond
  the group labels.
- **Touch**: the panel is the same bounded right drawer at ≤900px; the nine level rows are
  full-width rows at ≥44px, which is the WCAG 2.5.5 target §1.16 says the checklist marker
  misses; the preset cells get `min-width: 44px; min-height: 44px` at that breakpoint. Each
  preset is one tap, which is the point — **on a phone a preset is the only realistic way to
  author a multilevel list**, so the gallery is the touch path and the field editor is the
  desktop one.

#### 4.3.6 The assertions D-3 carries

Not `command-activation-contract` (§4.4):

1. Pick the `1. a. i.` preset on a three-level list: the a11y mirror reads `1.`, `a.`, `i.`.
   **Mutation:** drop the second level from the payload → reads `1.`, `1.1`, red.
2. Set level 0's `runProperties.bold`: the marker's glyph run is bold and **the item's text is
   not** — the assertion that names LST-14's authoring half.
3. Set `isLgl` on level 1 of a list whose level 0 is `upperRoman`: level 1's substituted `%1`
   renders as `1`, not `I`. The layout test for this already exists; this one proves the UI can
   reach it.
4. Set `start` to 5: the first item reads `5.`. Set it to 0: **refused with the sentence**, and
   the definition is unchanged.
5. `Change list level` → level 3 from the palette by name: the item's level is 3 and the marker
   is level 3's. **Mutation:** remove the generated family from `host_contract.mjs` → the
   host-contract spec goes red, which is what pins reachability.
6. Twelve fields changed one at a time, then twelve undos, each reverting exactly one field —
   the assertion the table inspector already carries, and the one that proves
   `applyListDefinition` is not a batch.
7. `#listSettingsApply` and `#listSettingsReset` both have count **0**.

### 4.4 Constraints from `main` that bind all three designs

**`HF-186` — no design here may be verified by `command-activation-contract`.** Its
inert-control assertion cannot fail: on a document with a caret, eight unrelated DOM mutations
arrive within 3s of idle (five `childList` on `.overlay`, plus `#statusToast`'s
`class`/`hidden`/`childList`), and it was proven by making the Table band's sort control a
complete no-op while the table sweep still passed. D-1, D-2 and D-3 therefore each carry their
own guarantee assertions (§4.1.6, §4.2.5, §4.3.6), which assert the **guarantee** — the second
list starts at 1, one undo restores the literal, the marker is bold and the text is not — not
"some DOM mutation happened". That is `SKILL.md` §10's assert-the-guarantee rule, and here it
is also the only assertion that *can* fail.

The related good news: **none of these designs adds overlay churn.** D-1 and D-3 paint no
canvas chrome at all, and D-2 paints none — which is why lists escape the trap `docs/141` §4.4
had to design around for tables.

**`HF-216` — six in-tree comments still say "~55px of slack".** `webapp/embed.html`,
`webapp/src/main.js`, `webapp/src/style.css` (twice), `layout-references-surface.spec.mjs` and
`ribbon-legibility.spec.mjs`, two of them citing `docs/63`/`docs/64` as the source. §0.6 is the
correct figure and `ribbon-width-budget.spec.mjs` is the artifact; the six comments are
HF-216's job. Noted so the conflict does not read as an error here.

**No new engine operation** (ADR-030 I2). Verified against all three: D-1 composes
`SetParagraphProperties`; D-2 composes `DeleteText` + `SetParagraphProperties`; D-3 composes
`SetParagraphProperties`. The `definitions_mut()` write each performs is **pre-existing**
behaviour, not a new operation, and LST-16 is the row that makes it one. **§4 proposes zero new
operations.**

**Per-interaction O(1)** (`docs/107` §4). D-2's transform reads the paragraph prefix with
`doc.copyText(node, 0, node, offset)` — O(paragraph), not O(document) — and its conversion runs
once per accepted trigger, not per keystroke. D-1's `listInfo` *reduces* the walk count on a
selection change from ~6 to 2 while remaining O(document); LST-18 is the row that makes it O(1)
and neither D-1 nor D-3 blocks on it. **Nothing in §4 is hover-driven**, which is the specific
trap `docs/141` §1.13 found.

---

## 5. What the existing list and numbering documents get wrong

There is **one** list design document in the repository and it is **seven lines long**:
`docs/76-LIST-LEVEL-DESIGN.md`. That is the finding. For comparison, tables have eleven
documents (`docs/49, 50, 70, 72, 73, 74, 75, 89, 90, 91, 92`).

**Nothing below is edited by this document.** §6 files them.

| Doc | The sentence that is now false or incomplete | What the code does |
| --- | --- | --- |
| **76** | *"Tab and Shift+Tab on list paragraphs now change the numbering level instead of only changing paragraph indentation."* | Accurate. **But the document stops there**, and four things it does not say are now load-bearing: Tab is taken by table-cell navigation first; it fires from anywhere in the item rather than only at the start (§1.3); the same code path is Increase/Decrease Indent (§1.9); and a mixed selection is refused or silently indented depending on which end the focus is at (§1.8). A seven-line document is not wrong; it is the reason §1.8 shipped |
| **76** | *"Levels are clamped to `0..=8`, preserve the existing numbering instance"* | Accurate for `adjust_list_level`. **`toggle_list` does not preserve the level** — it writes `level: 0` (§1.18), which is the same feature's other half and contradicts the sentence a reader would take away |
| **76** | *"apply to every selected paragraph in one undoable `SetParagraphProperties` transaction"* | Accurate, and worth keeping: it is the one place the transaction shape is written down |
| **76** | *"Non-list paragraphs retain the existing 360-twip indent behavior."* | **True of `adjust_list_level`'s closure and false of the gesture.** The closure is `if let Some(mut numbering) = p.numbering`, so a non-list paragraph in the range gets **nothing at all** — not a 360-twip indent. The 360-twip behaviour belongs to `adjust_indent`, which the JS reaches only when the *focus* paragraph is not a list. So the sentence describes a behaviour no code path produces |
| **104** HF-107 | *"(ensure_list/ensure_checklist are memoized and are not affected.)"* | Half wrong (§0.4). They are **bounded**, which is what memoisation buys, and they are equally outside the op boundary — so undoing the first bullet toggle in a session still leaves two definitions behind. The row's symptom (*"repeated use bloats the exported numbering.xml"*) is right about `set_list_format`/`restart_list` and silent about the fact that the memo is itself the cause of §1.1 |
| **104** HF-107 | locations `crates/casual-doc-wasm/src/lib.rs:4397` and `:4159` | Both stale. The anchors are `pub fn set_list_format` and `pub fn restart_list` |
| **109** row 10 (HF-184) | *"`find_paragraph_mut` (`casual-doc-edit/src/lib.rs:5599`)… 35 call sites"* | Stale line, and 35 is the raw occurrence count (33 call-site lines, some of them the function's own recursion). **And it is not the list path**: the two hits in `casual-doc-wasm` are doc comments promising *not* to add a call site (§0.3) |
| **109** OO-016 | *"AutoCorrect is smart quotes only — no math codes, no autoformat list triggers"*, **P3** | Accurate, wrongly ranked. Typing `1. ` is how a large fraction of users start a list; both competitors have it on by default. This document ranks it 12th of 37 (LST-12) |
| **109** OO-021 | the multilevel gallery row, **P2 / M** | Accurate and the right shape. D-3 sharpens it: the missing thing is not a gallery but a **per-level write path** (`applyListDefinition`), of which the gallery is one caller |
| **130** §8 row 12 | *"Multilevel list gallery → Home — but as a third item in the existing list split buttons' menus, not a new button. Zero width"* | **Accurate and adopted verbatim** by §4.3.2. The best-aimed row about lists in the repository |
| **130** §8 table header | *"Home (264px headroom)"* | Contradicts `ribbon-width-budget.spec.mjs`'s **288px** (§0.6). The spec is the artifact; the doc is the copy |
| **130** §11.1 | *"Five leaf capabilities are wired twice"* | **Seven.** `paragraph.restart` and `paragraph.continue` are two more of the same shape (§0.5, LST-27) |
| **44** row P1F-6 | *"Numbering — only `ilvl`+`start`; `numFmt`/`lvlText`/`lvlJc`/`suff` dropped"*, **Done** | Done and correct for import/export. **But `lvlJc` has no layout consumer**, so "not dropped" means "round-trips", not "renders" — `SKILL.md` §9.4's distinction, and worth a footnote on a closed row |
| **08** ADR register | *"The closed set is 51 variants."* | **53** (§0.4). `docs/141` §0.4 already published 53; this is the second document to derive it |
| **35** | the `w:nsid`/`w:tmpl` silent-drop policy row, with an explicit *"Open question, recorded rather than settled"* | Accurate, and the habit to copy. **But `w:tplc` and `w:tentative` are dropped silently and are *not* in the policy row** — `numbering.rs` never calls `report_attribute`, so they fall through an element-only catch-all (LST-31) |
| **110** RTF profile | `\*\pn` legacy lists unsupported; *"alternate names, level indents and follow characters, list templates"* dropped | Accurate, and it **reports** (`"rtf.list.legacy-pn"`). The best-behaved of the import profiles on lists |

**Accurate as written:** `docs/107` §4's budget (which this document uses as the constraint it
is), and `docs/130`'s "Ahead" rating for checklist / restart / continue, which §3 confirms
against source.

---

## 6. Rows to file — the owner applies these centrally

Per the brief, this document does not edit `docs/104`, `105`, `109` or `14`.

**New rows (interaction and fidelity):** LST-01 … LST-37 from §2. If they are filed as one
themed row rather than 37, **five must survive individually**:

- **LST-10** — the `w:numStyleLink` import hole. It is silent fidelity loss on real documents,
  it is the only row here that costs a user data they already have, and it makes a tested engine
  capability unreachable. **File it at P1 or above and file it alone.**
- **LST-06** — one list per session. It gates LST-13 and LST-21 and it is what a user meets
  first.
- **LST-01** — the `1.1.1.` default. Three constants; the single highest value-per-line row.
- **LST-16** — definitions outside the op set. It is HF-107, and the ADR-033 consequence belongs
  on HF-107 rather than as a new row.
- **LST-19** — numbering forces `FlowResume::FromStart`. It belongs to `docs/113`'s windowed
  layout and will not be found from a list row.

**Amend, do not close:**

- **HF-107** — correct the two stale line citations, correct the *"ensure_list … not affected"*
  parenthetical (§0.4), and add that the memo is the cause of LST-06 and that D-1 increases the
  orphan rate deliberately (§4.1.5).
- **`docs/109` row 10 (HF-184)** — correct the line and the count, and record that the list
  facade is **not** a caller (§0.3).
- **`docs/109` OO-016** — re-rank; add the ONLYOFFICE trigger table as the spec.
- **`docs/109` OO-021** — sharpen to "a per-level write path, of which the gallery is one
  caller".
- **`docs/44` P1F-6** — closed correctly; add the footnote that `lvlJc` round-trips and does not
  render.
- **`docs/08`** — *"51 variants"* → 53.
- **`docs/130`** §8's 264px → 288px, and §11.1's five → seven.
- **`docs/35`** — add `w:tplc`/`w:tentative` to the policy row, or file LST-31 against it.

**Doc corrections (§5):** `docs/76`'s last sentence describes a behaviour no code path produces,
and its level-preservation claim is contradicted by `toggle_list`. **`docs/76` is the one that
matters most**, because it is the only list document in the repository and it is what a lane
would read before starting any of §4. It should either grow into the design document it is
standing in for, or say plainly that it documents one gesture.

**Engine rows worth their own entry:** `Operation::SetNumberingDefinition` (or an
`InsertNumberingDefinition`/`RemoveNumberingDefinition` pair) — the op the closed set is
missing, which is HF-107's blocked decision; a `NumberingState` checkpoint index for the
windowed tier (LST-19); a layout consumer for `w:lvlJc` (LST-25) and for `w:lvl/w:pStyle`
(LST-26); and the marker cascade taking the paragraph mark as its base (LST-14).

**Input-path row, larger than lists:** the keydown transform does not fire for swipe-typing,
dictation or platform autocorrect, because those emit no `keydown` (§4.2.4). This already limits
smart quotes and would limit D-2 identically. It is a `beforeinput` row for whoever owns text
input.

**Test rows:** LST-36, broken out, because they are not one row — `adjust_list_level` has **no
native test at all**; no indent method has one; Tab, Enter-ends-list, Backspace-outdents and
every mixed selection are undriven; no import test constructs a `numStyleLink` abstract; nothing
asserts counters across a section break.

---

## 7. What this document deliberately leaves out

- **Any code change.** Every domain was occupied when this was written:
  `crates/casual-doc-edit|model|layout|import|export|odf` by a footer-field lane, `webapp/**` by
  the version-history UI, and PR #652 open across `main.js`, `editor.html`, `style.css` and
  `capabilities.mjs`. §4 is buildable as written; none of it was built here.
- **`docs/76`'s own rewrite.** It is the document most in need of one (§6), and editing it would
  have put this lane in the same file as the design it is proposing.
- **Outline numbering of headings.** A numbered heading is a paragraph style carrying `w:numPr`,
  and the cascade resolves it (`fn overlay_paragraph`:
  `if over.numbering.is_some() { base.numbering = over.numbering; }`), so it works — but
  *authoring* it is a **styles** row (`docs/115`), and the outline panel
  (`webapp/src/outline_panel.mjs`) is `docs/122`'s. `docs/109`'s "Navigation promote/demote" row
  is that document's, not this one's.
- **Table of contents.** `docs/109` OO-001, and it consumes numbering rather than authoring it.
- **Line numbering** (`w:lnNumType`) and **page numbering**, both of which grep as "numbering"
  and are neither. `docs/130` §4.4 records both as shipped; `#lineNumbersBtn` and
  `#pageNumberFormat` are their surfaces.
- **Footnote and endnote numbering.** It shares `pub(crate) fn format_number` with lists (§3)
  and is otherwise `crates/casual-doc-layout/src/note_numbering.rs`'s and `docs/62`'s.
- **Picture bullets** (`w:numPicBullet`). LST-37 files the model gap; designing an authoring
  surface for an image marker needs the media pipeline and is a much larger row than the rest of
  §4.
- **A localisation sweep.** Three unlocalised list strings were found in passing — the Tab
  branch's Suggesting message, `runToolbarEdit`'s hardcoded sentence, and every facade refusal
  (§1.17) — and are folded into LST-03 and LST-23. The product's wider `t()` coverage was not
  audited.
- **Touch text-selection grips.** `grep -rn 'selection-grip\|selectionHandle' webapp/src/` →
  zero, the same finding `docs/141` §7 records. A general selection row; none of §4 needs it.
- **A millisecond figure for anything.** §1.15 publishes call counts and complexity, both
  derivable from the functions cited. A timing number would have needed instrumentation, and
  `SKILL.md` §8 asks for a doubling guard rather than a threshold.
- **Google Docs' own list settings UI.** The author cannot open it, so D-3's field list is built
  from ONLYOFFICE's (source-verified) and Word's (`[K]`) rather than guessed from Docs. Recorded
  as a limit, not hidden.

---

## 8. Every published number, and how to re-derive it

| Number | Recipe |
| --- | --- |
| **53** `Operation` variants; **0** numbering-related | `awk '/^pub enum Operation \{/,/^\}/' crates/casual-doc-edit/src/lib.rs \| grep -cE '^    [A-Z][A-Za-z0-9]*( \{\|\(\|,)'`, and `grep -rn 'abstract_numbering' crates/casual-doc-edit/src/` → 0 |
| **4** facade sites minting definitions | `grep -n 'definitions_mut()' crates/casual-doc-wasm/src/lib.rs` → 13 lines; the numbering ones are inside `fn ensure_list`, `fn ensure_checklist`, `pub fn restart_list`, `pub fn set_list_format` |
| **12** `NumberFormat` variants | `awk '/^pub enum NumberFormat \{/,/^\}/' crates/casual-doc-model/src/v1/definitions.rs \| grep -cE '^    [A-Z][A-Za-z]*(\(\|,)'` |
| **6** tokens `set_list_format` accepts vs **11** `list_format_at` emits | read `fn number_format_from_token` and `fn number_format_token` in `crates/casual-doc-wasm/src/lib.rs` |
| **5** invocable list commands, **2** indent, **1** container, **4** divergent ids | `grep -oE '"paragraph\.(list\|listFormat\|indent\|bullets\|numbering\|restart\|continue)[A-Za-z.]*"' webapp/src/main.js \| sort \| uniq -c` |
| **11** generated `paragraph.listFormat.*` ids | `awk '/id="bulletGalleryMenu"/,/<\/div>/' webapp/editor.html \| grep -c data-spec` → 6, plus the same over `numberGalleryMenu` → 5 |
| **9** Home-band list controls | `grep -cE 'id="(bulletList\|bulletListMenuBtn\|numberedList\|numberedListMenuBtn\|checkList\|indentDec\|indentInc\|restartList\|continueList)"' webapp/editor.html` |
| **3** list-related keyboard chords | `grep -n chord webapp/src/keymap.mjs \| grep -iE 'list\|indent'` |
| **3** list commands in the compact toolbar | `grep -c 'paragraph.list' webapp/src/compact_toolbar.mjs` |
| **8** `doc.listStyleAt` call sites in `main.js` | `grep -c 'doc.listStyleAt' webapp/src/main.js` |
| **16** `runNodeEdit` call sites | `grep -c 'runNodeEdit(' webapp/src/main.js` |
| **0** `pointerType` reads in the webapp | `grep -rn 'pointerType' webapp/src/ \| wc -l` |
| **22** numbering unit tests | `grep -c '#\[test\]' crates/casual-doc-layout/src/numbering.rs`. **Not** `grep -cE '^    fn '`, which returns 30 because it also counts six test helpers and the module's own functions (§0.3, correction 5) |
| **26** facade list refusal strings | read `pub fn toggle_list`, `adjust_list_level`, `restart_list`, `continue_list_inner`, `set_list_format`, `toggle_checklist_item`, `fn apply_marker_spec`, `fn order_endpoints`, `fn apply_paragraph_props_as`, `fn apply_indent_props`, `fn node_id_msg` |
| **22** numbering element names parsed on import | `grep -oE '^\s+b"[a-zA-Z]+"' crates/casual-doc-import/src/numbering.rs \| tr -d ' ' \| sort -u \| wc -l` |
| **0** `report_attribute` calls in the numbering importer | `grep -c report_attribute crates/casual-doc-import/src/numbering.rs` |
| **0** import tests naming `numStyleLink` | `grep -c numStyleLink crates/casual-doc-import/src/tests.rs` |
| **0** layout consumers of `lvl_jc` | `grep -rn 'lvl_jc' crates/casual-doc-layout/src \| grep -v 'lvl_jc: None' \| wc -l`. **The unfiltered form returns 7 and every one is a `field: None` test-fixture initialiser** (§0.3, correction 4) |
| **0** layout consumers of `pstyle` | `grep -rn 'pstyle' crates/casual-doc-layout/src \| grep -v 'pstyle: None' \| grep -v 'explicit_pstyle' \| wc -l`. The unfiltered form returns 8: seven `pstyle: None` initialisers and one unrelated test *name* |
| **3** layout reads of `is_lgl` (a real consumer, for contrast) | `grep -rn 'is_lgl' crates/casual-doc-layout/src \| grep -v 'is_lgl: false' \| wc -l` |
| **1** write of `run_properties` in the facade, and it is `None` | `grep -c 'run_properties: None,' crates/casual-doc-wasm/src/lib.rs` |
| **2** writes of `multi_level_type`, both `None` | `grep -n 'multi_level_type' crates/casual-doc-wasm/src/lib.rs` |
| §2.1's class counts (10 / 1 / 4 / 10 / 12 = 37) | extract the `Class` column of §2 rather than counting it — three hand counts were wrong (§2.1): `awk '/^\| 1 \| \*\*LST-01/,/^\| 37 \| \*\*LST-37/' docs/142-LIST-AND-NUMBERING-EXPERIENCE-GAP-AND-DESIGN.md \| awk -F'\|' '{gsub(/^ +\| +$/,"",$6); print $6}' \| sort \| uniq -c \| sort -rn` |
| Walk counts per facade call (1, 3, 4–5) | read the call chains: `pub fn paragraph_properties` (`crates/casual-doc-edit/src/lib.rs`, `surface_block_lists(…).find_map(…)`), `fn ordered_paragraphs`, `fn order_endpoints`, `fn apply_paragraph_props_as`, `fn selected_properties`, `fn paragraphs_in_selection` — and note `order_endpoints` runs twice in `toggle_list`/`adjust_list_level` |
| `find_paragraph_mut`: **35** occurrences, **33** call-site lines, **0** in the facade | `grep -rn 'find_paragraph_mut' crates/ \| awk -F: '{print $1}' \| sort \| uniq -c`; the two `casual-doc-wasm` hits are doc comments |
| Level clamp 0..8 | `pub fn adjust_list_level`: `.clamp(0, 8)`; ONLYOFFICE `[S]` `reference/sdkjs/word/Editor/Paragraph.js:10362`, `reference/sdkjs/word/api.js:4512`, `reference/sdkjs/word/Editor/Numbering/Num.js:438` |
| Indent 720 + level×360 twips, hanging 360 | `fn list_level` in `crates/casual-doc-wasm/src/lib.rs` |
| Bullet cycle `• ◦ ▪ –` mod 4 | the same function: `["\u{2022}", "\u{25e6}", "\u{25aa}", "\u{2013}"][usize::from(level) % 4]` |
| `start` clamp 1..32767 | `fn build_level` in `crates/casual-doc-import/src/numbering.rs` (`min(32_767)`) and `fn validate_numbering_level` in `crates/casual-doc-model/src/v1/document.rs` |
| Ribbon: **~288px** Home headroom, **120px** floor, **1017px** minimum viewport | `webapp/tests/e2e/ribbon-width-budget.spec.mjs` — grow the last `.rgroup` with a pad in 4px steps, predicate = nothing exiled **and** no hscroll; logged as `RIBBON_HOME_HEADROOM_AT_1280`. **Do not quote `docs/130` §8's 264 or the six in-tree "~55px" comments (HF-216).** |
| WCAG target sizes 24×24 (2.5.8), 44×44 (2.5.5) | W3C WCAG 2.2, not a competitor claim |
| ONLYOFFICE: Tab demotes only at the item start | `[S✓]` `reference/sdkjs/word/Editor/Document.js:8810`, else `:8829` |
| ONLYOFFICE: Enter on an empty item promotes, then strips at level 0 | `[S✓]` `reference/sdkjs/word/Editor/Document.js:19451-19463` |
| ONLYOFFICE: Backspace at the item start does the same two-branch thing | `[S]` `reference/sdkjs/word/Editor/Paragraph.js:4329-4341` |
| ONLYOFFICE: three autoformat bullet literals `*`, `-`, `>` | `[S✓]` `reference/sdkjs/word/Editor/Paragraph/Run/RunAutoCorrect.js:1128-1155` |
| ONLYOFFICE: Space/Tab are the only autoformat triggers | `[S]` `reference/sdkjs/word/Editor/Paragraph/RunContent/Space.js:220-223`, `Tab.js:192-195`; absent from `ParagraphMark.js:201-206`, `Break.js:346-350` |
| ONLYOFFICE: autoformat is one history point | `[S]` `RunAutoCorrect.js:930`, `:951` |
| ONLYOFFICE: join-or-mint by the previous paragraph and the typed value | `[S]` `RunAutoCorrect.js:991`, `:1042`, `:1113-1120` |
| ONLYOFFICE: Restart clones the `CNum` and never writes `w:startOverride` | `[S]` `reference/sdkjs/word/Editor/Document.js:25192-25211`; `grep -rn "StartOverride\|LvlOverride" web-apps/apps` → nothing |
| ONLYOFFICE: `w:isLgl` read-only in the UI | `[S]` `web-apps/apps/documenteditor/main/app/view/ListSettingsDialog.js:1040`; `grep -rn "put_IsLgl" web-apps/apps` → nothing |
| ONLYOFFICE: multilevel presets are literal `CNum` JSON | `[S]` `web-apps/apps/documenteditor/main/app/view/Toolbar.js:3232-3241`, `:3258` |
| ONLYOFFICE: the per-level field list | `[S]` `ListSettingsDialog.js:399`, `:405`, `:441`, `:476`, `:541`, `:592`, `:614`, `:627-635`, `:649`, `:659`, `:767`, `:1088-1089`, `:1155-1158`, `:1214`, `:1226`, `:1235` |
| ONLYOFFICE: marker formatting is per level, not per item | `[S]` `reference/sdkjs/word/Editor/Document.js:19990-20010` → `reference/sdkjs/word/Editor/Numbering/Num.js:436-456` |
| ONLYOFFICE: the marker's effective run properties merge the paragraph mark below the level | `[S]` `reference/sdkjs/word/Editor/Paragraph.js:1877-1920`, the underline exception at `:1882-1884` |
| ONLYOFFICE: Increase Indent is overloaded to change level | `[S]` `reference/sdkjs/word/Editor/Paragraph.js:5326-5330`; toolbar `web-apps/apps/documenteditor/main/app/view/Toolbar.js:617`, `:631` |
| ONLYOFFICE: Restart/Continue are context-menu only | `[S]` `web-apps/apps/documenteditor/main/app/view/DocumentHolderExt.js:2290-2302`, `:2526-2531` |
| ONLYOFFICE: "Set numbering value" exists | `[S]` `web-apps/apps/documenteditor/main/app/controller/DocumentHolderExt.js:2315-2329`; `reference/sdkjs/word/api.js:4366` |
| ONLYOFFICE: list-aware paste is dead code | `[S]` `reference/sdkjs/common/wordcopypaste.js:3505-3515`, enum `reference/sdkjs/common/commonDefines.js:2534-2535`; `grep -rn "uniteList" web-apps/apps` → nothing |
| ONLYOFFICE: no multilevel-definition dialog, no numbering enumeration API | `[S]` `grep -rn "DefineNewMultilevel\|defineNewList" web-apps/` → nothing; `grep -rn "GetAllNumbering" sdkjs/word` → nothing |

**Every Google Docs behaviour in this document is tagged `[K]` and is knowledge, not a
citation** — §0.2, with the low-confidence ones enumerated in §0.2a. There is no Docs source in
this environment and it cannot be run from here. Where a `[K]` row turns out to be wrong, the row
is wrong; the anchors on our side, and every `[S]` line number into the pinned ONLYOFFICE
checkout, are not.
