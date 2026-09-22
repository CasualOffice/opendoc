# 120 — Grouped text and form checkboxes in the accessibility mirror

**Status:** accepted, implemented in `fix/a11y-grouped-text-and-form-checkbox`.
**Opened:** 2026-09-23. **Rows:** `109` HF-169, HF-177, HF-178. **Source:** `118` §3
rows 2 and 3. **Related:** `117` (grouped-object selection), `67` row 9 (the
mirror itself).

## 1. What this is

Two gaps in the same projection, both measured on the owner's
`Medical-In...orm.docx`, both about the same thing: **content that is on the
page and operable is not reachable by a screen reader.**

| | gap | `118` |
| --- | --- | --- |
| A | text inside a grouped shape never reaches assistive technology | §3 row 3 (HF-169) |
| B | a form checkbox reaches it as a raw private-use code point | §3 row 2 |

They are one document because they are one walk. The mirror
(`#a11yDocument`, built by `webapp/src/a11y_mirror.mjs` from the engine's
`accessibilityTree()`) asks each paragraph two questions — what text does it
carry, and what else does it carry — and both gaps were in the second answer.

## 2. What we did now, measured

**A — grouped text.** `collect_a11y_group_images` descended
`GroupChild::Group` and emitted `GroupChild::Picture`, and its last arm was
literally `GroupChild::TextBox(_) | GroupChild::Shape(_) => {}`. Reverting to
that arm and re-running the new guard prints the whole of the projection for
`fixtures/generated/nested-group.docx`:

```
"Group child one" must reach the mirror:
[Paragraph { text: "Body before the group." },
 Paragraph { text: "Body after the nested group." }]
```

Three text boxes, one of them two levels down inside a `wpg:grpSp`, and not a
word of any of them. On the owner's form that is every label on every drawing.
Since #577 a user can select those shapes, and since #582 drag them; since
#574 Tab reaches them and Enter edits them. **So a user could change text that
assistive technology could not read back** — which is why
`group-child-traversal.spec.mjs` had to assert its edit through the Undo
button's label rather than through the mirror, and said so in a comment.

**B — the form checkbox.** The eight `w14:checkbox` content controls in that
form each hold `<w:sym w:font="Wingdings 2" w:char="F0A3"/>`. Word writes a
symbol-font glyph as a **private-use code point**, so the control's content is
`U+F0A3` unchecked / `U+F052` checked. `node_plain_text` carries it verbatim,
the mirror rendered it as text, and `118` §3 measured the result: no `role`
attribute anywhere in `#a11yDocument`. Letting the glyph back through the new
projection reproduces it exactly — the control's own name becomes the glyph:

```
no control named "Medication error" in [
  A11yCheckboxJson { node: "…05", checked: false, name: Some("\u{f0a3} Medication error") },
  A11yCheckboxJson { node: "…0c", checked: false, name: Some("\u{f0a3}") },
  …
```

A private-use code point has, by definition, no meaning outside the font that
draws it. A screen reader announces **nothing** for it. And since #578 the
control can be ticked by click or by Space — so this is not a reading gap any
more, it is an **operable control with no role, no state and no name**, which
is WCAG 2.2 SC 4.1.2 (Name, Role, Value, Level A) in its plainest form.

The form also carries **no `w:alias` and no `w:tag` at all** — `0` of each in
`word/document.xml` — and the visible label of every control sits in a
**different table cell**: the box alone in a 421-twip cell, "Medication
Error" / "Fall or Injury" / … in the next cell of the same row. That is the
fact that makes the naming question real rather than a lookup.

## 3. The constraint that shapes both fixes

`node_plain_text` is the **shaper's byte layout**, and a UTF-8 offset from
hit-testing indexes it. The `A11yBlockJson::Image` doc comment already records
this trap for alt text: "giving alt text bytes there would move every caret
past a figure." The same is true of a text box's content and of anything the
projection would like to say about a control.

So neither fix touches `node_plain_text`. Grouped text-box content reaches the
**mirror** through a separate walk, and the guard pins it from the other side:
the anchor paragraph's `node_plain_text` must still be exactly `""`.

## 4. The references

### ONLYOFFICE, read from source

`ONLYOFFICE/sdkjs@master`, read 2026-09-23 — `common/workerspeech.js`,
`word/Editor/StructuredDocumentTags/InlineLevel.js`,
`pdf/src/forms/base/base.js`, `word/Editor/Shortcuts.js`. AGPL-3.0; read for
**behaviour**, nothing copied.

**They have no structural accessibility projection at all.** Their whole
accessibility surface is a single live region: `CWorkerSpeech` creates one
`div#area_id_screen_reader` with `role="region"`, `aria-live="assertive"`,
`aria-atomic="true"`, and pushes strings into it. It is off by default and
toggled by a shortcut (`SpeechWorker`, Ctrl+Alt+Z).

Its whole vocabulary is **thirteen** message types, enumerated in
`SpeechWorkerType`: `Text`, `TextSelected`, `TextUnselected`,
`SlidesSelected`, `SlidesUnselected`, `DrawingSelected` (`{ altText }`),
`CellSelected`, four cell-range variants, `MultipleRangesSelected`,
`SheetSelected`. **None of them is a form control, a checkbox, a heading, a
list or a table.**

And the content control itself knows nothing about assistive technology:
`InlineLevel.js` is 4,269 lines and contains **zero** occurrences of `aria`,
`role` or `speech`. Neither does the PDF forms base. `CInlineLevelSdt` exposes
`GetAlias`, `GetFormKey`, `GetPlaceholder`, `GetFormRole` — all of them for
their own Forms UI, none of them wired to a screen reader.

**So ONLYOFFICE exposes a `w14:checkbox` to assistive technology as nothing.**
That is a real finding, not a gap in the reading: there is no behaviour here
to adopt, and `105` §4 already records that they ship no accessibility
checker. Two things are still worth taking from them:

- a drawing is announced by its **alt text** (`DrawingSelected { altText }`) —
  which is what our `A11yBlockJson::Image` already does; and
- their announcer is a **live region**, i.e. an event stream, where ours is a
  **structure**. Ours is the stronger shape: a structure can be browsed, and a
  live region can only be heard once.

### Microsoft Word

Word's own naming of a content control is its **Title** — `w:alias` — which is
what Developer ▸ Properties shows and the only author-supplied name the format
carries. `w:tag` is the machine-facing key. Word's Accessibility Checker has
no rule for content controls, so it will not complain about an unnamed one.

**What was not verified:** what Narrator/JAWS actually announce for a
`w14:checkbox` in Word. There is no Word here to measure and no source to
read, so this document does not claim it. The decision below therefore rests
on the ARIA and WCAG specifications, which are normative, rather than on an
unmeasured claim about a competitor.

### The ARIA and WCAG specifications

These are the actual reference for B, and they are unambiguous:

- **WAI-ARIA 1.2, role `checkbox`.** `aria-checked` is a **required** state.
  Accessible name required: **true**.
- **ARIA Authoring Practices, Checkbox pattern.** `role="checkbox"` plus
  `aria-checked="true|false"`; Space toggles.
- **WCAG 2.2 SC 4.1.2 Name, Role, Value (A).** Name and role programmatically
  determinable; states set by the user programmatically determinable. An
  operable control with no name fails this.
- **WCAG 2.2 SC 2.5.3 Label in Name (A).** Where a component has a label
  presented visually, the accessible name must **contain** that visible text.
  This is the rule that decides the precedence in §5 — it is why an invisible
  `w:alias` cannot outrank the words a sighted user reads.
- **Technique ARIA14** — `aria-label` where a visible label cannot be
  associated; **ARIA16** — `aria-labelledby` where it can.

## 5. Decision

### A — grouped text

**A text box's block content is projected through the same walk as the body,
wherever the text box is.** `GroupChild::TextBox` and `InlineNode::TextBox`
both recurse into `collect_a11y_blocks_at`, so a heading inside a text box is
a heading, a list inside one is a list, and a text box inside a nested group
is reached at any depth. This is the uniform-flow rule the engine already
applies to layout, applied to the projection.

It reads **where the drawing is anchored** — after its anchor paragraph's own
text, in the same slot as a figure — because that is where a sighted reader
meets it.

Nesting is bounded by `MAX_A11Y_NESTING = 16`. The model is a tree, so the
walk terminates on its own; the bound is an explicit statement that an absurd
document cannot drive a read-only projection into the stack.

### B — the form checkbox

**The control is a node, not text.** A `w14:checkbox` contributes no
characters to the announced text; it contributes an
`A11yBlockJson::Checkbox { node, checked, name }`, which the host renders as
`role="checkbox"` + `aria-checked` + `aria-label`. `checked` is read from the
model's `w14:checked` flag on **every rebuild**, so ticking the box on the
canvas moves what is announced.

A control inside a table cell reaches the host on that **cell**
(`A11yCellJson { text, checkboxes }`) rather than as a top-level node, so it
keeps the row and column it is in. This is why `Table.rows` changed from
`Vec<Vec<String>>` to `Vec<Vec<A11yCellJson>>`: a form puts its controls in
cells, and a string can carry a control's glyph but not its role, its state or
its name.

**Private-use code points are dropped from announced text**, checkbox or not.
A PUA code point is silence dressed up as text; dropping it is strictly better
than emitting it. This is the class fix behind the instance.

### The accessible NAME — the rule, stated

In precedence order, first non-empty wins:

1. **The text of the block the control sits in**, with the control's own glyph
   already removed — the paragraph for a body control, the cell for a table
   one. This is the nearest visible label.
2. **The nearest non-empty other cell in the same table row** — to the
   **right** first, then to the **left**. A form writes the box and then its
   label; a right-aligned form writes the label first.
3. **`w:alias`** (the control's Title, the one place a Word author can name
   it), then **`w:tag`**.
4. **Nothing.** The engine reports no name, and the host announces the generic
   `"Check box"`.

**Why visible text outranks `w:alias`.** SC 2.5.3: the accessible name must
contain the visible label. An alias is never displayed, so naming the control
`chk_fall` when the page says "Fall or Injury" breaks speech input ("click
Fall or Injury" would not match) and contradicts what the user is looking at.
The fixture carries exactly this disagreement so the precedence is observable
rather than asserted.

**Why a positional rule is allowed here at all.** It is not a guess about
meaning; it is the mechanical form of ARIA16 — point the name at the visible
text that is already labelling the control — for a layout where the label
lives in a neighbouring cell. The engine resolves the string rather than
emitting an `aria-labelledby` reference because only the engine knows the
model and only the host knows the DOM ids; resolving it once, in Rust, keeps
one mechanism and makes the rule testable by `cargo test` rather than only in
a browser.

**What bounds the wrongness.** A positional label is applied **only when the
cell holds exactly one control**. Two boxes in one cell beside one label are
both left unnamed and announced generically, because the label cannot say
which one it means. It never looks outside the row. And when there is nothing,
nothing is invented: **an operable control with no name is a WCAG failure, and
a wrong name is worse than a generic one**, so the floor is a generic name and
never a guess.

The generic name is a HOST string (`UNNAMED_CHECKBOX` in
`a11y_mirror.mjs`), not an engine string, for the same reason
`"Image without a description"` is: it is user-facing chrome text and belongs
where the localisation seam will be (`109` HF-081).

### Not adopted

- **ONLYOFFICE's live-region announcer.** We already have the stronger shape.
  A structural mirror can be browsed with a screen reader's own navigation; a
  `aria-live` region can only be heard as it happens. Adopting theirs would be
  a downgrade, and their vocabulary has no form control in it anyway.
- **Making the mirror's checkbox operable.** It is a read-only projection —
  `docs/67`'s Open Risks say "accessibility bridges cannot become hidden DOM
  editors", and the container's own label says read-only. Operating it also
  needs focus to survive the rebuild the edit itself triggers, which is real
  work rather than an attribute. Filed as **HF-178**; today the control is
  operated in the document by click or Space (#578), which is the same gesture
  Word and ONLYOFFICE use.
- **`aria-disabled` on the mirror's checkbox.** It would be a lie: the control
  is not disabled, it is operated somewhere else.
- **Mapping Wingdings code points to their Unicode equivalents** (`U+F0A3` →
  `U+2751`, `U+F052` → `U+2611`). A ~200-entry table per symbol font, and it
  would only buy a nicer *character* where what is needed is a *role*. Drop
  the glyph and expose the control instead.
- **Naming a control from outside its own table row** — a column header, a
  preceding heading. Cheap to write, unbounded in how wrong it can be.

## 6. Guards

Every one was driven red by reintroducing the defect; the output is in the
commit message.

Rust (`crates/casual-doc-wasm/src/lib.rs`):

1. `text_inside_a_grouped_shape_reaches_the_accessibility_projection` — all
   three text boxes of `nested-group.docx`, the nested one included, are in
   the projection, in anchor order; **and** the anchor paragraph's
   `node_plain_text` is still `""`. Restoring the dropped `GroupChild::TextBox`
   arm fails the first half; splicing the text into `node_plain_text` to "fix"
   it fails the second.
2. `a_form_checkbox_is_announced_with_a_role_a_state_and_a_visible_name` —
   seven controls projected; each naming rule exercised by its own fixture
   row; no private-use code point anywhere in the projection; and the
   announced state follows a real toggle and its undo.

Browser (`webapp/tests/e2e/`):

3. `form-checkbox.spec.mjs` — `#a11yDocument` carries seven
   `[role=checkbox]`, zero private-use code points, and the named controls;
   `aria-checked` flips on Space and flips back on undo. Its `boxes()` helper
   no longer counts glyphs, because there are none to count: the flag/glyph
   agreement it was protecting is pinned natively by
   `a_form_checkbox_ticks_and_unticks`.
4. `group-child-traversal.spec.mjs` — the spec that had to read the Undo label
   now reads the mirror, before and after the edit, and still asserts the two
   body paragraphs are untouched.

## 7. Still open

- **HF-178** — the mirror announces a form checkbox but cannot activate one.
- Table cells project text and checkboxes only: a **picture** inside a cell
  still does not reach the mirror, although one in a body paragraph does.
- Legacy `w:fldChar`/`w:ffData` FORMCHECKBOX and FORMTEXT fields are a
  different mechanism and are **not** covered here — that is HF-175, and its
  accessibility half belongs with it.
- A text box's content is projected but is not *navigable* as a region: a
  reader is not told where the drawing starts and ends.
