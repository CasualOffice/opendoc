# 161 — Closing the measured attribute losses: what each fix actually recovers

**Status:** Implementation record, with the measurement each item rests on restated and,
where it was wrong, corrected. **Opened:** 2026-10-04.
**Scope:** The ranked queue `160` §7 produced — border theme colour, `wrap_sides`
honouring `wrap_text`, `w:style@w:customStyle`, `w:hyperlink@w:history` and
`pic:cNvPr@descr`. Model, import, export and the layout consumers each needs.
Tracker rows: `109` FID-R-09 and the rows beside it.

`160` is the measurement and lands separately; this document is what the fixes found, and
it exists because three of the five items turned out to be a different size or a different
shape than the measurement predicted. A fix whose write-up only repeats the finding that
prompted it has not been checked.

**The corpus is confidential.** Every number below is a count of constructs over the
owner's twenty private `.docx` files. No file, and nothing derived from one, is committed;
nothing here names a document or quotes content. The scans are small scripts run from a
scratch directory and are described rather than committed, because committing a scanner
whose only input is private data invites the input into the tree — which has already
happened once (`160` §6).

## 1. Border theme colour: what was lost, measured rather than assumed

`160` §4 found five of nineteen documents losing a border theme colour and ranked it first
of seven. The natural way to state that — "a themed border paints the wrong colour" — is
**too strong**, and the fix is worth less and more than it claims.

Measured over the nineteen importable documents, counting border-edge elements
(`w:top`/`w:bottom`/`w:left`/`w:right`/`w:start`/`w:end`/`w:insideH`/`w:insideV`/`w:bar`/
`w:between`) that carry `@w:themeColor`:

| Fact | Count |
| --- | ---: |
| Documents with at least one themed border edge | 9 of 19 |
| Themed border edges | 8,449 |
| …that **also** carry a concrete `@w:color` | 8,449 — every one |
| …that carry `@w:themeTint` or `@w:themeShade` | 2,057 |

So the fallback was always there. Word writes the resolved sRGB beside the reference for a
consumer with no theme, and `BorderEdge::color` was reading it. The page was therefore
**not** uniformly wrong, and saying it was would have been the overstatement `99` §9 rule 3
is about.

What *was* certainly lost is the **reference**. A theme colour's whole purpose is that
changing the document's theme repaints it; with the reference dropped, every themed border
in the document froze at whatever colour the last save happened to resolve, and the
round-trip wrote a document Word no longer regards as themed at all.

Of the 2,057 edges carrying a tint or shade, comparing `@w:color` against the slot colour
from the document's own `a:clrScheme` with ECMA-376's tint/shade arithmetic applied:

| What `@w:color` holds | Count |
| --- | ---: |
| The already-tinted colour, so `@w:color` alone was right | 1,309 |
| The **raw** slot colour, so the tint was lost on the page | 94 |
| Neither | 654 |

The 94 are the cases where the old reader painted a visibly wrong colour: full-strength
accent where the document asked for a 60%-tinted one.

The 654 are **not explained by this scan** and are recorded as such rather than guessed at.
The obvious suspect is `w:clrSchemeMapping`, which remaps Word's twelve colour slots onto
the theme's and which this engine reports rather than models. It is not the cause here:
**zero of the nineteen documents sets any slot to anything but the default**, and five
carry no `w:clrSchemeMapping` at all. A stale fallback is the likelier explanation — a
producer that changed the theme without rewriting every `@w:color` beside it — and it is an
argument for honouring the reference either way, which is what the fix now does. The
scanner also resolved only the first theme part it found in each package, which a document
with more than one theme would defeat; that is a limit of the scan, not a finding.

### 1.1 The model shape, and why it is additive

`BorderEdge::theme_color: Option<ThemeColor>`, **beside** `color`, not a widening of it.

The alternative — giving `color` the `Color` type that runs already use, whose `Theme`
variant carries exactly this — was considered and rejected on two grounds, the second of
which is the real one:

1. `BorderEdge` is constructed by literal in **40 places across eleven crates**, two of
   them in `casual-doc-wasm`, which another lane owns. Rust has no source-compatible way to
   change a struct's field type or add a field (`SKILL` §5a), so either shape breaks every
   literal; this one breaks them identically, so that argument is weaker than it looks and
   is not what decided it.
2. The two are **not alternatives in the source.** Word writes the concrete `@w:color` as a
   *fallback* next to the reference, and the measurement above is what establishes that:
   8,449 of 8,449. Collapsing them into one field would throw the fallback away, and the
   fallback is what a consumer with no `a:clrScheme` has to paint — including this engine,
   on the five corpus documents that declare none.

This is the same shape `Shading` already uses: `fill: Option<RgbColor>` beside
`theme_fill: Option<ThemeColor>`, for the same stated reason. Preferring the existing
precedent over a new one is `SKILL` §8's "prefer one mechanism over two", applied to the
model rather than to code.

No ADR, deliberately. The decision follows a precedent already in the tree rather than
setting one, and §8 reserves ADRs for decisions; the reasoning lives on the field's own doc
comment, where the next person to touch it will actually read it.

### 1.2 One builder, not two

The triple was dropped in `styles.rs::border_edge` **and** in `body.rs::build_border_edge`,
which were two copies of one mapping — and that is why it had to be dropped twice to be
dropped at all. Both are now thin aliases for one `properties.rs::parse_border_edge`, next
to the `parse_shading` and `theme_color_ref` it reuses. `160` noted the duplication; this
removes it, so the next attribute added to a border edge is added once.

### 1.3 The layout consumer

`casual-doc-layout::flow::resolve_edge` is the single choke point every border edge in the
engine passes through — table borders, cell borders, the separated-cell perimeter,
horizontal span segments, a paragraph's `w:pBdr`, a run's `w:bdr` and a section's
`w:pgBorders`. It now takes the document's `ResolvedPalette`, which already existed for run
colours and shading, and resolves a slot through the same `apply_tint_shade` and
`tint_shade_factor` a run's `w:themeColor` uses. One `edge_color` helper serves both the
painted colour and the conflict ranking.

The ranking matters more than it looks: `border_rank`'s last tie-break is colour darkness,
and it read `edge.color`, so an unresolved themed edge ranked as **black** — the darkest
possible — and won conflicts it should have lost. It now ranks by the resolved colour.

Two call sites outside `flow` build the palette for themselves, once per document and once
per window rather than per page: `document_layout.rs` and `windowed.rs`, both of which
already hold the `Document`.

### 1.4 The guards, and what drove them red

`crates/casual-doc-export/tests/border_theme_color.rs` — five tests over an in-test package
(no new fixture, so no manifest or checksum churn) carrying a themed paragraph border, a
themed cell border, two themed table-style borders, and an **unthemed control**. Every
question is asked through one walker over XML element and attribute names; the probe's body
text says `themeColor accent1 themeTint themeShade` on purpose, so a guard reaching for a
substring passes while proving nothing and is caught.

Mutations run, with the output recorded in the commit:

- **Import stops reading the triple** → three of the five go red, including
  `the_theme_attributes_are_w_qualified_as_written` with `left: {}`.
- **Export writes a bare `themeColor` instead of `w:themeColor`** → two go red. The
  namespace pin is load-bearing, which is the thing a prefix test usually is not.
- **`edge_color` stops consulting the palette** → both layout guards go red, one with
  `left: [255, 0, 0, 255]` (the deliberately-wrong fallback the probe carries) against
  `right: [64, 128, 192, 255]`.

Two differences between source and written package are **normalizations, not losses**, and
the round-trip guard forgives exactly those two in one named function rather than by
loosening the comparison: physical `w:left`/`w:right` edges are written as logical
`w:start`/`w:end`, and the mapped slot spellings `text1`/`background1`/`text2`/`background2`
are written as their canonical `dark1`/`light1`/`dark2`/`light2`. Both are pre-existing and
documented — the first in `160`'s own spelling class, the second on
`casual_doc_import::properties::theme_color_ref`.

## 2. `wrap_sides` ignored its argument, and the guard that could not see it

`casual-doc-layout::wrap_side::wrap_sides` took a `&DrawingAnchor` and returned
`WrapSides::BothSides` unconditionally, so `DrawingAnchor::wrap_text` was discarded at
layout for **every** float in every document — including the DrawingML path where #738 and
#739 had already landed import, export and the model field. The fix is one `From` impl and
a one-line body, and the three previously-unreachable `WrapSides` variants lost their
`dead_code` allow because they are constructed now.

The arithmetic that consumes the value needed no change. That is the part worth recording:

> `wrap_side.rs`'s own `authored_sides_override_the_geometry` hands `band_exclusion` a
> `WrapSides` directly and asserts it is honoured. It passes whether or not the engine ever
> supplies one.

A guard on the exact code path that was broken, which could not fail — `SKILL` §4's case,
in a module whose docs are otherwise unusually careful. The replacement asks the question at
the altitude where the invariant lives: lay the **same** mid-column float out twice, once
with `wrapText` absent and once with `right`, and assert the text keeps opposite gaps. The
float sits three fifths across the measure so the leading gap is the wider one; `bothSides`
keeps it, and an authored `right` therefore asks for the gap geometry would not have chosen.
Under the stub both cases read identically, which is what makes the two distinguishable.

**Still approximate, and still documented rather than hidden:** `bothSides` is resolved as
`largest`. Our line geometry is one measure with a leading and a trailing inset
(`text::InlineFloatSpec`), so a hole in the middle of a line is not representable and text
in the narrower gap is lost. Making it exact needs segmented lines — the same primitive a
real `wp:wrapPolygon` needs (`109` FID-L-12).

## 3. `pic:cNvPr@descr` — the verdict `160` gave it does not hold

`160` §3.2 graded this "Real. **Picture alt text** — an accessibility loss", two of
nineteen documents. Checked before changing anything, which is what the queue asked for, and
the grade is wrong.

The importer has two `cNvPr` arms. The first captures `@descr` for an open **group child**
shape. The second — a top-level picture or lone shape — carried a comment saying its
`@descr` "is taken from `wp:docPr` above it", i.e. the same field read from a different
element, deliberately. The question is therefore not whether the attribute is read but
whether `wp:docPr@descr` is reliably there.

Measured per `w:drawing` over the nineteen documents:

| Shape | Drawings |
| --- | ---: |
| Total drawings | 64 |
| `wp:docPr@descr` only | 12 |
| Both `wp:docPr@descr` and a non-empty `pic:cNvPr@descr` | 5 |
| …of which the two values are **identical** | 5 |
| …of which the two values **disagree** | 0 |
| `pic:cNvPr@descr` alone, no `wp:docPr@descr` | **0** |

So no alt text was lost: it arrived through the element the importer already read, and the
remaining three of the eight `pic:cNvPr@descr` attributes in the corpus are `descr=""`,
which is an absent alt text rather than a lost one (the same false-loss class the `wp:docPr`
arm already fixed).

What landed anyway is a **fallback**, not a model change and not a second source: the
picture-level `@descr` is taken only when the drawing-level one gave nothing. A producer
that writes only `pic:cNvPr@descr` is schema-valid, we had no reader for that shape, and the
whole cost is one condition. `wp:docPr` precedes `pic:nvPicPr` in both `CT_Inline` and
`CT_Anchor`, so the preferred source has always been read by the time the fallback runs —
which makes "only when nothing was captured" an ordering-safe test rather than a race. One
`capture_drawing_descr` now serves both elements, because two copies of the length and
emptiness rules are two places for them to diverge.

Three guards, each driven red by its own mutation: the fallback (`left: None` against
`Some("A picture-level logo")` with the arm disabled), the precedence of `wp:docPr` over the
duplicate (`left: Some("The picture-level duplicate")` with the captured-already test
disabled), and the no-false-loss rule for `descr=""`. The precedence guard pins an ordering
**nothing in the corpus exercises**, because the two values always agree there — which is
exactly when a precedence inverts without anybody noticing.

## 4. The two booleans: one modelled, one reported, and why they differ

`160` §3.2 listed `w:style@w:customStyle` (4 of 19) and `w:hyperlink@w:history` (6 of 19) as
"a boolean each; model or report". They do not get the same answer, and the reason is
structural rather than a preference.

Re-measured here, both higher than `160` recorded:

| Attribute | Occurrences | Documents | Values seen |
| --- | ---: | ---: | --- |
| `w:style@w:customStyle` | 274 of 1,923 `w:style` | 12 of 19 | `"1"` only |
| `w:hyperlink@w:history` | 54 of 74 `w:hyperlink` | 6 of 19 | `"1"` only |

**`@w:customStyle` is modelled** as `Style::custom_style`. Reporting 274 occurrences of a
construct this common is the report-noise class HF-174 is about, and the flag is not
cosmetic: it is how a consumer separates the author's style from a built-in whose id
collides, and how Word decides which styles a template re-attach may replace. Dropping it
changes what a later edit in Word does to the document.

**`@w:history` is reported.** Adding a field to `v1::Hyperlink` breaks every struct literal
of it — Rust has no source-compatible way to add one (`SKILL` §5a) — and there are 37 across
`casual-doc-edit`, `casual-doc-transaction` and `casual-doc-wasm`, three crates other lanes
own. So the silence is closed and the model half waits for a lane that owns those files.
This does not reintroduce report noise: `Reporter::report_attribute` keys a finding by
`(feature, kind)` and counts occurrences, so 54 occurrences are **one** entry reading
`hyperlink/@history`.

It is reported on **presence** rather than on a non-default value, which is a deliberate
over-report with its reason recorded: the `ST_OnOff` default for the attribute is not
verified from the specification here, every value measured is `"1"`, and the writer emits
the attribute in neither case — so one of the two values is genuinely lost whichever way the
default goes, and over-reporting by one feature entry is the cheaper error.

### 4.1 Two things the guard caught that review would not have

- **`is_true` is for an element, not an attribute.** `custom_style:
  is_true(attribute_value(element, b"customStyle"))` marked **every built-in style as the
  author's**, because `is_true(None)` is `true`: it is the CT_OnOff *element* helper, where
  the element's presence is the assertion. An absent attribute asserts nothing. The reader
  now goes through one `on_off_attr`, which `style_default_attr` was already doing by hand,
  and the guard failed on arrival with `{"Author Voice": true, "heading 1": true}`.
- **A guard keyed on `w:styleId` tests the id allocator, not the flag.** The exporter derives
  `w:styleId` from the model's internal id, so the written token is a number and never the
  source's string; the first draft failed with
  `left: [("18446744073709551618", Some("1")), …]`. The round-trip guard is keyed on the
  style's `w:name`, which survives. This is the "pinned to the circumstance rather than the
  guarantee" shape.

### 4.2 The open remainder

`casual-doc-wasm`'s create-style-from-selection path mints a `Style` with
`custom_style: false`. A style the user just authored **is** custom, so that value is wrong —
the `Default::default()` reflex `SKILL` §5a warns about, met in a crate this lane must not
touch. Recorded as the open half of `109` FID-R-11 rather than fixed from outside the lane
that owns the file. It is one line.
