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
| 2 | **The checkbox is invisible to assistive technology** | Medical form | the a11y mirror carries raw `U+F0A3` / `U+F052` — Wingdings 2 private-use code points, which a screen reader reads as nothing. No `role` attribute appears anywhere in `#a11yDocument` |
| 3 | **Text inside a grouped shape never reaches assistive technology** | Medical form, loan | HF-169, filed 2026-09-22: `collect_a11y_group_images` walks a group for pictures and drops `GroupChild::TextBox` |
| 4 | **Custom-path shapes are drawn as plain rectangles** | loan, 5 shapes | the model's `ShapeGeometry` has 7 presets plus `Other`, documented as "drawn as its bounding rectangle"; `a:custGeom` with `moveTo`/`lnTo`/`ahLst` arrowheads has no representation, so an arrow becomes a box |
| 5 | **Picture transparency is ignored** | loan, 5 pictures | `a:alphaModFix amt="20000"` — a 20% watermark logo — has no field in the model, so it paints solid. Model/import/layout wiring is on `feat/picture-transparency` |
| 6 | **Warped text is drawn flat** | loan, 20 | `a:prstTxWarp` unmodelled |

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

1. **The checkbox toggle** (§2). It is the difference between this file being
   a form and being a picture of one, the model already carries every field it
   needs, and both competitors agree on the interaction.
2. **Checkbox accessibility** (§3 row 2), which falls out of the same change:
   the control should reach the mirror as a checkbox with a checked state, not
   as a private-use code point.
3. **HF-169**, grouped text in the a11y mirror.
4. **The empty-element false-loss class** (§4), so the compatibility report
   stops burying real findings.
5. **Picture transparency** (§3 row 5) — already part-built.
6. **Custom geometry** (§3 row 4) and **text warp** (row 6), both real
   engine work.
