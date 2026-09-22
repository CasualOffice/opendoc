# 118 — What is still wrong with the owner's own documents

**Status:** audit complete, measured not recalled. **Opened:** 2026-09-22.
**Scope:** `Medical-In...orm.docx` and
`General_Loan_On_lend_and_loan_from_SMSF_Agreement.docx`, the two files the
owner has reported on most, plus the 16-document `~/Downloads` corpus.
**Related:** `117` (grouped-object selection), `115` (Styles control).

## 1. Why this document exists

The owner asked what else is broken, and asked for it *measured against the
competitors* rather than invented. Every row below is either something I
performed in the browser against the real file, or something I read out of the
real XML. Where a fix implies an interaction, the interaction is cited from
Word and from ONLYOFFICE's source, not designed here.

It also corrects three things I told the owner in #575 that were wrong. That
correction is the point of the section: a list of gaps that contains
non-gaps is worse than no list, because it spends their attention on nothing.

## 2. The headline: this form cannot be filled in

`Medical-In...orm.docx` is a **Medical Incident Report Form**. It carries
**eight `w14:checkbox` content controls** — 7 unchecked, 1 checked — in the
"Nature of the incident" table.

Measured in the browser, sweeping the checkbox column and trying each gesture
on every box:

```
boxes before: {"unchecked":7,"checked":1}
toggled by:   NOTHING — click, Space and double-click all do nothing
boxes after:  {"unchecked":7,"checked":1}
```

The control is fully modelled — `SdtCheckbox { checked, checked_state,
unchecked_state }` with `SdtCheckboxSymbol { val, font }`, imported from
`w14:checked` / `w14:checkedState` / `w14:uncheckedState`. Import, layout,
render and round-trip all carry it. **What is missing is the operation and the
gesture**: nothing in the engine flips `checked`, and nothing in the host
listens for a click on one.

So the document opens, renders correctly, and is read-only in the one way that
matters: it is a form, and the form cannot be completed.

### What the competitors do

**ONLYOFFICE** (`sdkjs@master`, `word/Editor/StructuredDocumentTags/InlineLevel.js`,
read 2026-09-22; AGPL, read for behaviour, nothing copied):

- `CInlineLevelSdt.prototype.ToggleCheckBox` flips `Pr.CheckBox.Checked`;
- `SetCheckBoxChecked` records an undo entry
  (`CChangesSdtPrCheckBoxChecked`) and calls `private_UpdateCheckBoxContent`,
  which **rewrites the control's content run** to `CheckedSymbol` /
  `UncheckedSymbol`. The glyph is content, not decoration;
- that rewrite is **revision-aware**: under track-changes it replaces the
  content as a tracked insert/delete pair rather than editing in place;
- a checked **radio** button does not untoggle itself; siblings sharing a form
  key are untoggled by the forms manager.

**Word**: clicking the control toggles it; Space toggles it when it is
selected; the change is one undo step.

The two agree on everything that matters, so there is nothing to design:
**click toggles, Space toggles, one undo step, the glyph swaps between the
declared symbols, and under Suggesting it is a tracked change.**

## 3. The rest, ranked, all verified

| # | gap | where | verified how |
| --- | --- | --- | --- |
| 1 | **A form checkbox cannot be ticked** | Medical form, 8 controls | browser: click / Space / double-click all no-ops |
| 2 | ~~The checkbox is invisible to assistive technology~~ | Medical form | **Fixed (#585, HF-177)**: the mirror carried raw `U+F0A3` / `U+F052` — Wingdings 2 private-use code points, which a screen reader reads as nothing — and no `role` attribute appeared anywhere in `#a11yDocument`. Now `role="checkbox"` + `aria-checked` + a name taken from the visible label beside it. Design and naming rule in `docs/120` |
| 3 | ~~Text inside a grouped shape never reaches assistive technology~~ | Medical form, loan | **Fixed (#585, HF-169)**: `collect_a11y_group_images` walked a group for pictures and dropped `GroupChild::TextBox`. A text box now flows through the same projection as the body, at any nesting depth, without moving a caret offset. `docs/120` |
| 4 | **Custom-path shapes fall back to their bounding rectangle** | loan, 5 shapes | `ShapeGeometry` has 7 presets plus `Other`, "drawn as its bounding rectangle"; `a:custGeom` has no representation. **Re-measured 2026-09-23 and this row was overstated** — see §6 |
| 5 | ~~Picture transparency is ignored~~ | loan, 5 pictures | **Fixed (#579)**: `a:alphaModFix` now renders, exports and round-trips, in the raster backend and in PDF |
| 6 | ~~Warped text is drawn flat~~ | loan, 20 | **NOT A GAP** — see §6 |

## 4. Not gaps — corrections to what I told the owner in #575

I listed seven "real losses" in #575. **Three of them were not losses at all**,
and I found that only by checking each against the XML afterwards, which is
what I should have done before sending the list.

| I claimed | actually |
| --- | --- |
| `sizeRelH` ×34 / `pctHeight` ×31 — "percentage-sized objects lost", ranked **first** | every one of the 63 values is **`0`**. Zero means relative sizing is **off**; Word writes the element anyway as a default. Nothing is lost |
| whole `drawing` ×1/×2/×3 — "whole drawings dropped" | `drawing` never meant a dropped drawing. It means the drawing was imported and some *detail* was not — and the detail was an empty `descr=""`. Fixed in #576; the drawings were always there |
| `tab` ×28 — "tab stops in a style dropped" | the NDA's 24 bare `<w:tab/>` are **tab characters** inside runs, not tab stops, and the importer handles them on a separate arm. The remaining count is unattributed. **Not a claim** until it is |

`effectLst` ×13 in the loan document is the same shape: all thirteen are
`<a:effectLst/>`, self-closing — "no effects". Reported as a loss; loses
nothing. It is the same class as the `w:shd` false loss (#575) and the empty
`descr` (#576), and it is worth fixing the class rather than the tag: an
element inside a drawing that is *empty* is, for almost all of DrawingML, a
statement that the feature is absent.

Also worth recording: **`40mb.docx` is not a DOCX.** It is a 40 MB plain text
file from examplefile.com with a `.docx` extension — Python's `zipfile` refuses
it too. The tab freeze on it was never a DOCX problem; it is the
1.3M-paragraph plain-text open path (`116` §7).

## 5. Order

1. ~~**The checkbox toggle** (§2)~~ — **done (#578)**. It is the difference
   between this file being a form and being a picture of one, the model
   already carries every field it needs, and both competitors agree on the
   interaction.
2. ~~**Checkbox accessibility** (§3 row 2)~~ — **done (#585, HF-177)**, and it did
   NOT fall out of the same change: the control reaches the mirror as a
   checkbox with a role, a checked state and a name, which needed a projection
   change of its own. `docs/120`.
3. ~~**HF-169**, grouped text in the a11y mirror~~ — **done (#585)**.
4. **The empty-element false-loss class** (§4), so the compatibility report
   stops burying real findings.
5. **Picture transparency** (§3 row 5) — already part-built.
6. **Custom geometry** (§3 row 4) and **text warp** (row 6), both real
   engine work.


## 6. Re-measured 2026-09-23 — two of my own rows above were wrong

A second, wider sweep measured every element of the corpus against the source.
It corrected §3 twice, and both corrections are of the same kind I made in §4:
a feature *named* in a document is not a feature *used* by it.

**Row 6, warped text — not a gap at all.** All 22 `a:prstTxWarp` in the corpus
(20 in the loan, 2 in the Medical form) carry `prst="textNoShape"`, which is
DrawingML's value for *no warp*; Word writes it into every `wps:bodyPr`. There
is no warped text anywhere in these documents. ONLYOFFICE agrees so strongly
that it special-cases the same token in four places — its "is there a warp"
predicate is literally `prstTxWarp && preset !== "textNoShape"`, and it
normalises an *absent* element *to* `textNoShape` when reporting to its UI.
The row is closed as a non-gap for this corpus. Watermarks and real WordArt
remain genuinely unmodelled; nothing here exercises them.

**Row 4, custom geometry — mis-described, and near-harmless here.** The five
`a:custGeom` shapes in the loan agreement are byte-identical: a
`6660515 × 1270` EMU box — 1270 EMU is **0.1 pt** tall — holding one
`moveTo(0,0) → lnTo(6660057,0)` path with a solid `#29D19E` 1.5 pt stroke and
no fill. They are **thin green horizontal rules**, not arrows. The `a:ahLst` I
cited is the *adjust-handle* list and is empty; arrowheads are
`a:headEnd`/`a:tailEnd` and these shapes have neither. Painting the bounding
rectangle of a 0.1 pt box with a 1.5 pt stroke already produces approximately
the intended line. Custom geometry is still a real gap for the ~180 unmodelled
presets behind `ShapeGeometry::Other` (tracker FID-L-04) — but **this corpus
is not the justification for it**, and my "an arrow becomes a box" line was
invented from the element names rather than measured.

**And the `tab ×28` I left unattributed in §4 is now attributed, and real.**
The stops carry `w:val="num"`, a legal `ST_TabJc` value that `apply_tab_stop`
and `read_style_tab_stops` do not map, so the stop is dropped and reported —
46 of them across the NDA and both `fannest` files. List text sits at the
default 720-twip grid instead of its authored position. ONLYOFFICE keeps the
`num` token and treats it as a left tab whose position test is inclusive
rather than exclusive, which is the whole recipe.

### What the wider sweep found that this document missed entirely

Ranked; the full evidence is in the sweep, and the rows it recommends are
being filed into `109` separately.

| | gap | scale | why it matters |
| --- | --- | --- | --- |
| 1 | **621 findings that report a loss where nothing was lost** | 12 of 12 documents; 276 on the loan alone | the webapp shows the count to the reader. `w:proofErr` alone is 110. The §4 class is far larger than §4 knew |
| 2 | **CJK text has no bundled face on the browser build** | 2 documents, 9,576 characters — **53% of their text** | tofu, the worst possible outcome, and the largest single visible defect in the corpus |
| 3 | **Legacy `w:ffData` form fields cannot be filled** | the loan agreement: **92 FORMTEXT + 19 FORMCHECKBOX** | §2 fixed the `w14:checkbox` SDT. This is the *other* form mechanism, fourteen times bigger, and the loan agreement is exactly as unfillable as the Medical form was |
| 4 | **`w:documentProtection w:edit="forms"` is imported and never enforced** | the loan agreement | Word locks the body and Tab-cycles the fields; we let the user type over the contract |
| 5 | **A hyperlink on a picture or shape is dropped** | Medical form, 4 links | the logo is clickable in Word and inert here |

Items 3 and 4 belong together and supersede §5's ordering: the next form
work is legacy fields plus protection, not accessibility.
