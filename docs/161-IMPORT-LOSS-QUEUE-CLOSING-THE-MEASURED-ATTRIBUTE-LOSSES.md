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
