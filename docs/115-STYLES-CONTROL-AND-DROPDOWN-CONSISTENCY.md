# 115 — The Styles control, and one dropdown model for the chrome

Status: accepted, implemented in `fix/one-styles-control`.
Scope: `webapp/editor.html`, `webapp/src/main.js`, `webapp/src/style.css`. **No crate changes.**
Rule this exists to satisfy: the working contract's §1 — *every editing interaction is
designed from a Word/Google Docs comparison first*, not from what is convenient to bind.

This document is the comparison that was missing. The owner reported the same defect four
times; each earlier attempt changed the control without first establishing what the two
products it competes with actually put in front of a user. So the numbers come first.

**The decision is the owner's, and it is not a question this document reopens:** shorten the
list, limit what is *offered for authoring*, and keep everything supported *internally*. The
comparison below is the justification for the number, not a menu of options.

## 1. What was actually shipped before this change (measured, not recalled)

Measured in the built editor at a 1280 px viewport on the `?fixture=rich` demo document
(14 paragraph styles defined), by reading the live DOM:

| Control | Where | How many paragraph styles it offered | Chrome |
| --- | --- | --- | --- |
| `#paragraphStyle` | Home ▸ Styles, top row | **14** — every style in the document, flat, alphabetical | native `<select>`, OS popup |
| `#stylesGallery` quick strip | Home ▸ Styles, bottom row | **3** — the first three of a preferred list, then document order | product cards |
| `#stylesMorePanel` (the `▾`) | popover off the strip | **14** — every style in the document, again | product cards |
| `#paraPanelStyle` | Paragraph properties dialog | 14 | native `<select>` |

So the Styles *group* carried **three** controls for one job, two of which listed the
identical 14 entries, and the group was 234 px wide (select 227 px, gallery 227 px) on a
Home band with **10 px** of slack at 1280 px.

`ribbon-home.spec.mjs:222` already described `#paragraphStyle` as *"the hidden reflection
select"*. It was not hidden. It was the most prominent control in the group, and the only OS
`<select>` left anywhere in the ribbon band — sitting directly above a custom gallery doing
the same job. That is the inconsistency the owner kept reporting: two different dropdown
chromes, OS and product, for the same kind of choice.

## 2. Microsoft Word

Word splits the job across three surfaces, and the ribbon gets the smallest share.

- **Home ▸ Styles gallery.** A gallery of *Quick Styles*, not of styles. The default
  template marks ~16 styles as quick (Normal, No Spacing, Heading 1, Heading 2, Title,
  Subtitle, Subtle Emphasis, Emphasis, Intense Emphasis, Strong, Quote, Intense Quote,
  Subtle Reference, Intense Reference, Book Title, List Paragraph). At a 1280 px window the
  gallery renders roughly **6–8 of them at once**, with up/down scroll arrows and a *More*
  expander that drops the full quick set as a grid. The applied style is highlighted, and
  Word scrolls it into view. A style the document already uses is added to the gallery.
- **Styles pane** (`Alt+Ctrl+Shift+S`). The full list — the default template carries a few
  hundred latent definitions. It is a pane, never a ribbon dropdown.
- **Apply Styles** (`Ctrl+Shift+S`). A by-name box with autocomplete.

The load-bearing fact: **Word never puts the document's whole style list in the ribbon.**
The ribbon gets a curated, visually-previewed subset; completeness is the pane's job.

## 3. Google Docs

One toolbar dropdown, labelled with the currently applied style. It offers exactly **six**
named paragraph styles — Normal text, Title, Subtitle, Heading 1, Heading 2, Heading 3 —
each with a submenu (*Apply*, *Update ‘X’ to match*), plus an *Options* row. Every other
style concept lives elsewhere entirely. Docs' number is **6**, fixed, regardless of what the
document defines. A `.docx` opened in Docs with thirty custom styles still shows six.

## 4. The comparison, in one table

| | styles offered in the ribbon/toolbar | full list reachable from | by-name access |
| --- | --- | --- | --- |
| Word | ~6–8 visible of a curated 16, plus in-use styles | Styles pane | Apply Styles box |
| Docs | exactly 6 | — (six is all Docs offers) | — |
| opendoc, **before** | **3** cards **and** a flat **14**-entry OS select **and** a 14-card popover | — | none |
| opendoc, **after** | up to **6** — recommended ∩ document, plus the style in use | command palette, Paragraph properties | command palette search |

**Revision (same day).** The first implementation of "one control, six styles" was a
two-row **card gallery** sitting open on the band. Every guard in §7 passed against it,
and the owner rejected it on sight: *"i need a dropdown but native and fewer options
like our competitors like google docs."* The count was right and the shape was wrong,
which is the failure mode this document was supposed to prevent — §3 records that Docs
offers its six from **one toolbar dropdown labelled with the applied style**, and the
implementation took the number from Docs and the shape from Word.

So the control is now a dropdown: `#stylesTrigger`, labelled with the style at the
caret, opening `#stylesMenu` with the same six. Two things change as a consequence.

- **The trigger names the current style with nothing open.** That is the one thing the
  deleted 14-entry select did that the gallery did not, and §5.3 had to argue the
  highlighted card away as an adequate substitute. It no longer has to.
- **The list is not a native `<select>`**, even though "native" is the word the owner
  used, and this is the one place this document departs from the literal request. An
  OS popup renders every row in the system font, and the entire reason the list is
  worth opening rather than reading is that each row is drawn *in the style it
  applies*. A native select would show six identical words. `each option is drawn in
  the style it applies` is the guard, and flattening the rows fails it. Everything
  else about the control is a dropdown: one trigger, a caret, a label, click-outside
  and Escape to close, and no second entry point on the band.

Neither product shows more than ~8 in the ribbon, neither uses a flat dropdown of every
style, and neither presents two controls for the choice. We were doing all three.

## 5. Decision

**One control. A short list. Everything still supported internally.**

1. **`#stylesTrigger` is the only Styles control in the band**, and the only one anywhere in
   the ribbon or compact chrome. `#paragraphStyle` (the native select) and `#stylesMoreBtn` /
   `#stylesMorePanel` (the `▾` and its full-list popover) are **deleted**, not hidden.
   It is a dropdown labelled with the caret's style, opening a six-row product popup.
2. **The offered list is short and capped at six.** `RECOMMENDED_STYLES` is the intersection
   of Word's Quick Styles and Docs' six, in priority order; the gallery offers those of them
   the open document actually defines. **Six**, because that is Docs' number exactly and the
   bottom of Word's visible range. Three was chosen against a width budget, not against a
   competitor; fourteen was never a decision at all.
3. **A style the document already uses stays offered.** The style at the caret always holds a
   slot, and every style the caret has visited or the user has applied this session stays in
   the set, so opening a file and clicking through it surfaces exactly the styles that file
   uses and lets the user re-apply them. Word does the same thing; the select's only unique
   contribution was telling you which style you were in, and the highlighted card now does
   that. Ordering is recommended-first, then in-use in first-seen order; when the set exceeds
   six the **in-use style at the caret is never the one dropped**.
   *Limitation, stated rather than hidden:* "in use" is what the chrome has observed, because
   there is no `stylesInUse()` on the engine and enumerating every paragraph in JS is O(n) in
   document size — forbidden by the editing budgets in `docs/107` §4. The exact version is a
   one-method crate addition and is deliberately **not** in this branch, which touches no
   crate.
4. **Nothing is dropped from the model.** This changes what is *offered for authoring*, not
   what is *supported*. Import, the style cascade, layout, render, round-trip and export are
   untouched: a document arriving with `Envelope Return` still resolves, still lays out, and
   still saves byte-for-byte as it did. The only change is that `Envelope Return` is not one
   of six buttons on the ribbon.
5. **The full stylesheet stays reachable, off the ribbon** — two surfaces, satisfying §10:
   - the **command palette** already emits a `Style: <name>` row for every style
     `doc.listStyles()` returns (searchable by name — Word's *Apply Styles*);
   - **Paragraph properties ▸ Style** (`#paraPanelStyle`) is a full list in a dialog — the
     Styles-pane equivalent.
   Neither is in the band, which is exactly where the owner did not want them.
6. **Compact chrome adopts the same element.** `ADOPTED_CONTROL_IDS.style` points at
   `#stylesTrigger`; compact CSS narrows it and lets the label ellipsise. One element,
   one reflect path, two chromes — no clone that can drift.

### Trade-offs, stated

- **Cost:** the band no longer carries a *textual* readout of the applied style. Mitigated by
  (3): the applied style is always a highlighted card, and the gallery carries the name in
  `aria-label`. Word accepts the same trade.
- **Cost:** a style that is neither recommended nor yet visited takes two surfaces-worth of
  keystrokes (palette, or the Paragraph dialog) instead of one dropdown. That is the point of
  the change, and it is Docs' behaviour exactly.
- **Cost:** on a document defining few recommended styles the second row is partly empty
  (the demo document offers 5 of 6 — Normal, Body Text, Heading 1, List, Caption). Honest:
  Word's gallery is equally short on such a document, and the gallery's box is fixed either
  way, so the band never shifts as the caret moves.
- **Measured:** the group occupies **exactly** the footprint the old select-above-a-strip
  composition did — 227 × 62 px, so 234 px of band including padding, unchanged before and
  after, with the Home band's slack still 10 px at 1280 px and no horizontal scrollbar.
  Same space, twice the capacity.
- **Why wrapping flex and not a fixed-column grid:** equal columns are tidier, but
  `ribbon-legibility.spec.mjs` holds the harder and correct line — a card that ellipsises to
  "Bo…" defeats the only reason a gallery exists instead of a list of names. "Heading 1"
  renders at ~92 px at its preview size and equal thirds of 227 px cut it to the width of
  "List". Cards keep their natural width and the row wraps, exactly as the single-row strip
  sized itself; `.style-card`'s `max-width` is the backstop against a pathological name.
- **Rejected: Docs' single dropdown** (delete the gallery, keep one trigger). Less code, but
  it throws away the live per-style previews, which are the reason a gallery exists; and a
  ribbon's Styles *group* is a gallery in every product that has a ribbon.
- **Rejected: keeping a product trigger above the cards.** Visually that is what the owner
  reported — two stacked entry points — and it fails the one-control guard below on purpose.
- **Rejected: a scrolling gallery or a `▾` expander for the remainder.** Both re-admit the
  long list to the band through a side door. (The compact bar does scroll the SHORT list
  sideways, because it is one 30 px row rather than two — that is the same six cards in a
  narrower box, not a route to the other eight styles.)
- **Rejected: extending `#linkPlaceSelect` and the object inspector's five classless selects
  by hand.** They were found by the computed-style guard, not by reading, and the fix is the
  shared rule plus `.dialog-field select` so the next one is covered on arrival.

## 6. One dropdown model for the chrome

The band now has **zero** `<select>` elements. What remains is either a product popover or a
`<select>` inside a dialog or menu row — and those were themselves inconsistent: `.ctl select`
was restyled (`appearance: none` plus the product's own caret) while `.dialog-select`,
`.menu-select` and `.save-format` rendered raw OS chrome.

The rule adopted:

> A `<select>` is legitimate inside a **dialog or a menu row**, where a long list in a
> platform popup is the right affordance and the field is already a form field. It is not
> legitimate in the **ribbon band or the compact bar**, where every picker is a product
> popover. Wherever a `<select>` does appear, it wears the product's field and the product's
> caret.

Implemented as one shared selector list in `style.css` carrying `appearance: none` and the
caret image, applied to every `<select>` class in the chrome, and guarded two ways:

- `tests/select_chrome.test.mjs` — parses `editor.html` and `style.css`: no `<select>` inside
  a `.ribbon-panel`, and every `<select>` in the file is covered by the shared treatment or
  named on an exemption list with a reason. The exemption list is empty.
- `tests/e2e/styles-control.spec.mjs` — reads *computed* style in the browser: every
  `<select>` in the chrome computes `appearance: none`. A rule that is present but overridden
  still fails this.

### Known remaining OS-rendered list, deliberately not changed

`#fontSize` is an `<input type="number" list="fontSizeSuggestions">`. The `<datalist>` popup
is OS chrome. It is **not** converted here, for a stated reason: a product combobox needs a
caret affordance, which widens the Font group by ~16 px, and the Home band has ~10 px of
slack at 1280 px — it would exile a whole group into the `⋯` overflow. Word's own size box is
likewise an editable field rather than a pure picker, so this is the least-wrong place for the
remaining inconsistency. Recorded as open rather than left implicit. This change returns no
width to spend on it either — the Styles group deliberately lands on exactly the footprint it
already had, so the band's 10 px of slack is unchanged and remains margin, not budget.

## 7. The guards, and why the obvious ones are worthless

A guard asserting "the Styles control exists" passed through the entire three-control period.
The guards that can actually catch this class:

0. **The control is a dropdown that names the caret's style unopened.** Asserted on
   its own, because every other guard here passed against the card gallery. The band
   must carry exactly one `aria-haspopup` trigger and no option rows; the trigger's
   label must equal the reflected style; the menu must open, mark the current style,
   and close on an outside click. Putting the options back on the band fails it with
   "the options belong in the popup, not on the band"; labelling the trigger "Styles"
   instead of the style fails it too.
1. **Each row is drawn in the style it applies.** The justification for a product
   popup over an OS one, so it is guarded rather than asserted in prose: at least two
   distinct (weight, size) pairs across the rows. Flattening the previews fails it —
   "every row renders identically (Normal 13px/450, Body Text 13px/450, …)".
2. **One control per job.** Every element in the Home band that applies a paragraph style must
   be the single `#stylesTrigger`, and the band must contain no `<select>`.
   Re-adding `#paragraphStyle` — or any second style picker — fails it.
3. **The ribbon does not list every style.** The number of rows offered is at most 6 and
   strictly fewer than the styles the document defines. Uncapping the list fails it.
4. **The in-use style is always offered.** Put the caret in a style outside the recommended
   set; its card must be present and `aria-selected`, and stay offered after the caret leaves.
   Deleting the in-use step fails it.
5. **Reachability ≥ 2 surfaces** (§10): every style the document defines is reachable as a
   `Style: <name>` palette row **and** in Paragraph properties. Neither alone counts, and it
   is asked of *every* defined style rather than of one example.
6. **Select chrome**, as above, from computed style.
7. **The compact chrome borrows the same element.** Nothing covered compact mode before, and
   this change moved what it adopts from the deleted `#paragraphStyle` to the gallery — a
   broken adoption would have shipped as a Styles control that simply was not there. The
   guard asserts the adopted element is the gallery, that it offers the same set, that it
   still applies, and that leaving compact mode puts it **back** in the ribbon.

Each was driven red by mutating `main.js`/`style.css` before being accepted; the mutations and
their verbatim output are in the branch's commit message.
