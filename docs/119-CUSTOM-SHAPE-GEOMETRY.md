# 119 — Custom shape geometry (`a:custGeom`)

**Status:** accepted, implemented in `feat/custom-shape-geometry`.
**Opened:** 2026-09-23. **Row:** `109` FID-G-01, unblocking `109` FID-L-04.
**Source:** `118` §3 row 4 — **whose evidence this document corrects; see §1.**
**Related:** `117` (the same comparison-first shape), `118` §4 (the empty-element
false-loss class, which this row turns out to be a member of).

## 1. What `118` row 4 said, and what is actually in the file

The row reads:

> **Custom-path shapes are drawn as plain rectangles** — loan, 5 shapes —
> `a:custGeom` with `moveTo`/`lnTo`/`ahLst` arrowheads has no representation, so
> an arrow becomes a box.

Three of those clauses do not survive contact with the XML. All five
`a:custGeom` elements in `General_Loan_On_lend_and_loan_from_SMSF_Agreement.docx`
are **byte-identical**, 233 bytes each:

```xml
<a:custGeom><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/>
  <a:rect l="l" t="t" r="r" b="b"/>
  <a:pathLst>
    <a:path w="6660515">
      <a:moveTo><a:pt x="0" y="0"/></a:moveTo>
      <a:lnTo><a:pt x="6660057" y="0"/></a:lnTo>
    </a:path>
  </a:pathLst>
</a:custGeom>
```

| the row claimed | measured |
| --- | --- |
| `ahLst` arrowheads | **`a:ahLst` is not arrowheads.** It is the *adjust-handle* list (ECMA-376 §20.1.9.1) — the drag handles that edit a shape's guides. Arrowheads are `a:headEnd` / `a:tailEnd` inside `a:ln`, and **neither appears anywhere in this document**. `<a:ahLst/>` is also self-closing and empty in all five, as are `avLst`, `gdLst` and `cxnLst`: no guides, no adjust handles, no connection sites. Same shape as the `effectLst` / `sizeRelH` false losses in `118` §4 |
| "so an arrow becomes a box" | none of the five is an arrow. Each is an **open two-point horizontal rule** — a moveTo and one lnTo, no `a:close` — stroked `a:ln w="19050"` (1.5 pt) solid `#29D19E`, with no fill, in a box of `cx="6660515" cy="1270"` (524.45 pt × **0.1 pt**) |
| `a:custGeom` "is in the importer's `is_drawing_scaffolding` allowlist … so it is silently consumed" | the allowlist arm is **unreachable for these shapes**. `casual-doc-import/src/body.rs:2911` matches `b"custGeom" if self.pending_shape.is_some()` *before* the scaffolding catch-all at `:4075`, and it calls `self.reporter.report(b"custGeom")`. The loss is reported. `custGeom` reaches the allowlist only when no shape builder is open (a `pic:spPr` custom crop geometry) |

**And the visible defect on this document is roughly one sixth of a point.**
Measured, not recalled — `opendoc-render` at 110 DPI over the real file, scanning
for `#29D19E`:

```
p1  y=307..308  x=54..855   (802 px wide, 2 px tall)
p1  y=723..724  x=54..855
p3  y=1014..1015 x=54..855
```

Two device rows of solid teal spanning the text column: a horizontal rule. The
rectangle fallback *coincides* with the right answer here because the bounding
box is degenerate — a 0.1 pt-high rectangle stroked at 1.5 pt collapses its top
and bottom edges onto each other, so the painted band is 1.6 pt instead of
1.5 pt and the two 0.1 pt end caps are sub-pixel. The document renders within
0.1 pt of Word.

So the row is **real but mis-evidenced**. Custom geometry genuinely has no
representation; this file is simply the wrong exhibit for it, and left as the
justification it would have sent the next reader looking for a missing
arrowhead. The defect is visible the moment a custom path is not a degenerate
horizontal line — any polygon, any L, any chevron, any callout tail drawn as a
freeform paints as a filled, stroked box of the path's bounding extent.

### So why build it, on this evidence?

Stated plainly, because the honest answer to "does the corpus demand this?" is
**no**, and the work was still done. Three reasons, in order of weight:

1. **It is the primitive `109` FID-L-04 is explicitly blocked on.** That row —
   *"~180 DrawingML preset shapes collapse to bounding rectangles"*, P1 — already
   records its own unblocker in its notes: *"Needs a path/Bézier primitive
   first, then the presets table-drive."* This slice is the straight-line half
   of that primitive. It is not speculative work justified by one document; it
   is the piece a P1 row has been waiting on, and §3 shows it is also how the
   competitor is built.
2. **One line of this IS a data loss today, not a rendering approximation.**
   `semantic.rs:5596` maps `Other` to `prst="rect"`, so opening and saving any
   document with a freeform *rewrites the freeform out of the file*. That is a
   `no silent data loss` violation (`AGENTS.md`), it applies to the owner's loan
   agreement right now, and it cannot be fixed without modelling the path.
3. **The slice is small and closed.** ~250 lines across import, model, layout
   and export, one new optional field, one new boolean on an existing display
   primitive, and seven mutation-proved guards. Nothing in it is a stub.

What would NOT have justified it: "an arrow renders as a box." That was not
true, and §2 measures how close the current rendering of this document actually
is.

## 2. What we do now, in code

| stage | behaviour |
| --- | --- |
| import | `body.rs:2911` sets `geometry = ShapeGeometry::Other`, **clears** `preset` and `adjustments`, and reports `custGeom` as an omission. The `a:pathLst` subtree is then consumed by the scaffolding allowlist (`path`, `pt`, `moveTo`, `lnTo` are not listed — they report individually) |
| model | `ShapeGeometry` (`casual-doc-model/src/v1/body.rs:745`) is a closed 8-variant enum of preset primitives. Its own doc comment says `Other` is "drawn as its bounding rectangle" |
| layout | `anchor.rs:947` — `ShapeGeometry::Rectangle \| ShapeGeometry::Other => AnchorContent::Rectangle` |
| render | `AnchorContent::Rectangle` → `PaintItem::Shape { geometry: ShapeGeometry::Rect }` → `rect_path` |
| export | `semantic.rs:5596` — `ShapeGeometry::Other => "rect"`, written as `a:prstGeom prst="rect"`. The authored path is **not** round-tripped |

The export line is the one that actually loses data: a document opened and saved
comes back with its freeform rewritten as a rectangle preset.

### Measured again after the change

Same file, same tool, same DPI. Alpha coverage summed down a column through the
first rule on page 1 (`x = 400`), where the authored stroke is `a:ln w="19050"`
= 1.5 pt = **2.0 display px** (`compose.rs:197` converts twips at 96 px/inch):

| | painted band | vs the authored 2.0 px |
| --- | --- | --- |
| before | 3.00 px | **+50 %** — the box's top and bottom edges each stroked, 0.1 pt apart |
| after | 2.00 px | exact |

The import report moves with it: **3414 omitted findings → 3359**, and `ahLst×5`
— which was the *fifth* entry in the summary and the one the audit row
misread as arrowheads — is gone, along with `custGeom` and the eleven path
elements per shape. Fifty-five findings, five shapes, eleven each.

One thing this does **not** fix, so it is not claimed: the raster backend does
not rescale stroke widths with output DPI, so at 110 DPI a 1.5 pt line is still
painted 2.0 px wide rather than 2.29. That is a pre-existing gap in
`stroke_px`/`render_shape`, it affects every stroked object equally, and it is
out of scope here.

## 3. ONLYOFFICE, read from source

`ONLYOFFICE/sdkjs@master`, read 2026-09-23 — `common/Drawings/Format/Geometry.js`,
`Path.js`, `CreateGeometry.js`, `common/Shapes/SerializeWriter.js`. **AGPL-3.0;
read for BEHAVIOUR only, nothing copied.** No code, no data table and no
expression from sdkjs appears in this repository.

The load-bearing fact is structural, and it is the opposite of how this
repository is built:

**There is no preset-primitive path and no custom-geometry path. There is one
geometry engine, and a preset is an input to it.**

- `CreateGeometry.js` is a 10,397-line `switch (prst)` that *builds* each preset
  out of the same primitives a file supplies. `case 'rect'` is four `lnTo`s and
  a close; `case 'line'` is a `moveTo` and two `lnTo`s and **no** close. A
  preset is a recipe for a path list, not a primitive.
- `Geometry.prototype.Recalculate(w, h)` seeds a guide table `gdLst` with the
  box-derived names (`l t r b w h hc vc wd2 … ss ls`), evaluates the authored
  guides on top, and then asks each `Path` to resolve its commands against it.
  A file's `a:gdLst` and a preset's built-in guides land in the same table.
- `Path.prototype.recalculate` is where `a:path@w`/`@h` are honoured:
  `cw = gdLst["w"] / this.pathW` when `pathW` is set, and when it is **not**
  set, `cw = 1` with `dCustomPathCoeffW = 1/36000` — i.e. **an absent `w`/`h`
  means the coordinates are absolute EMU in the shape's own space and are not
  scaled to the box.** Our file has `w` and no `h`, so x scales and y does not.
- `Path.prototype.draw` walks the resolved commands into the canvas verbatim and
  finishes with `drawFillStroke(true, this.fill, this.stroke && …)`, where
  `fill` and `stroke` are the **per-path** `a:path@fill` / `@stroke`
  attributes. There is no implicit close: an open path is stroked open.
- `Geometry.prototype.draw` early-returns on `!this.isValid()`, and
  `Path.prototype.isValid` is "there is at least one command and the first one
  is a `moveTo`". **An unrenderable geometry paints nothing.** ONLYOFFICE never
  substitutes a bounding rectangle.
- Hit-testing follows the same path: `CShape.hitInPath` → `Geometry.hitInPath` →
  per-path canvas containment, with `hitInBoundingRect` only as the fallback
  when there is no geometry at all.
- `SerializeWriter.WriteGeometry` confirms the split is *storage only*: record 1
  when `geom.preset` is a non-empty string (name + adjustments), record 2
  otherwise (adjustments, guides, adjust handles, connection sites, **path
  list**, text rect). Presets are re-expanded to paths on load.

## 4. Microsoft Word

No source. But this document contains Word's own statement of what the geometry
means, because Word wrote a `mc:Fallback` for every one of the five shapes, and
a VML fallback is Word downgrading its own rendering for a reader that cannot do
DrawingML:

```
<v:shape id="Freeform: Shape 941202955" coordsize="6660515,1270"
         path="m,l6660057,e" filled="f" strokecolor="#29d19e" strokeweight="1.5pt">
```

In the VML path grammar `m` is moveto (empty coordinates mean `0,0`), `l
6660057,` is lineto, and the terminator is **`e` — end, an open path** (`x`
would be close). Plus `filled="f"`. So Word's own fallback for an `a:custGeom`
is *an open, unfilled, stroked two-point polyline* — not a rectangle. The same
document writes `<v:rect …>` as the fallback for a shape that really is a
rectangle, so the distinction is deliberate, not an artefact.

Behaviourally, Word: draws the authored path; scales it to the shape box through
`a:path@w`/`@h`; fills a path only when the shape carries a fill (and treats an
open path as implicitly closed for fill purposes only, the same rule any canvas
uses); exposes it as a **Freeform** with per-vertex "Edit Points"; and preserves
`a:custGeom` on save.

ECMA-376 Part 1 §20.1.9.8 (`custGeom`) and §20.1.9.15 (`path`) agree on the
attribute defaults used above: `@w`/`@h` default `0`, `@fill` defaults `norm`,
`@stroke` defaults `true`.

## 5. Google Docs

Not a counterpart, and recorded so the absence is not mistaken for an oversight.
Docs has no in-document DrawingML shape model; drawings are edited in a separate
drawing editor with a fixed shape gallery plus polyline/scribble tools, and a
DOCX freeform arrives there as an image. It contributes nothing to this
decision.

## 6. Decision

**Adopt the ONLYOFFICE structure — a path is the primitive and a preset is a
recipe — but land it as one bounded slice, not as the whole engine.**

The named prior art here (per the `SKILL.md` §8 rule) is an **interpreter with a
lookup table**, not a wider enum. The tell that the current design is on the
wrong axis is exactly the one that rule describes: `ShapeGeometry` has grown to
seven presets plus a catch-all, each with its own hand-written vertex list in
`anchor.rs`, and DrawingML has **187** presets. Adding an eighth primitive buys
one shape; adding a path primitive buys the class, and later lets the seven
presets collapse into table entries rather than code.

### In scope for this slice

1. A `ShapePath` on `GroupShape`: the authored `a:path@w`/`@h` coordinate space
   plus an ordered command list of `MoveTo`, `LineTo`, `Close`.
2. Import of `a:custGeom/a:pathLst/a:path` restricted to `a:moveTo`, `a:lnTo`
   and `a:close`, with **exactly one** `a:path` and **exactly one** leading
   `moveTo`.
3. Resolution to page-local points at layout, honouring the `@w`/`@h` rule from
   §3 (absent = absolute EMU, present = scale to the box).
4. Painting as an open or closed polyline: `AnchorContent::Polygon` and the
   display list's `ShapeGeometry::Polygon` gain a `closed` flag, so one
   primitive serves both instead of two. Raster and PDF backends both honour it.
5. DOCX export re-emits `a:custGeom` with the authored path instead of
   rewriting the shape to `prst="rect"`.
6. Bounded: at most `MAX_SHAPE_PATH_COMMANDS` commands, coordinates clamped to
   `MAX_EMU`, validated in `Document::validate`.

### Explicitly out of scope, and filed rather than half-built

Each of these keeps today's behaviour — reported as an omission, painted as the
bounding rectangle — and is `109` FID-G-02:

- ~~**Curves and arcs**~~ — **`a:cubicBezTo` and `a:quadBezTo` have since landed**; see
  "Landed since" below. `a:arcTo` is still out, and still FID-G-02.
- **Guide formulas**: `a:gdLst` and the `*/ +- pin sin cos at2 …` formula
  language, and therefore any path whose coordinates are guide *names* rather
  than integers. This is the single largest remaining piece.
- **Adjust handles** (`a:ahLst`, `a:ahXY`, `a:ahPolar`) and **connection sites**
  (`a:cxnLst`), which are authoring and connector features, not rendering ones.
- **Multiple subpaths** — more than one `a:path`, or a second `moveTo` inside
  one — and the per-path `@fill` / `@stroke` / `@extrusionOk` attributes.
- **`a:rect`**, the custom text rectangle.
- **Editing**: no Edit-Points gesture, no vertex handles. The shape stays
  selectable and movable as a box.
- **ODF export**: still writes the bounding `draw:rect`. `draw:polyline` /
  `draw:polygon` is the right target and is part of FID-G-02.
- **Hit-testing stays rectangular.** ONLYOFFICE hits the path; we hit the
  bounding box. Deliberate: these rules are 0.1 pt high and a path-exact hit
  test would make them unclickable without a tolerance model we do not have.
  Recorded in the code, not left ambiguous.

### Landed since: cubic and quadratic curves (FID-G-02, first piece)

Dated 2026-10-02. The slice above stands as the record of FID-G-01; this records what
changed, because the "out of scope" list above is no longer wholly true.

1. **The display list's shape outline became a path, not a point list.** A
   `PathCommand` (MoveTo/LineTo/CubicTo/QuadTo) replaced the vertex-only `Polygon`
   geometry at both the layout and display layers, and both backends build real
   curves. Replacing rather than adding was checked first: the display list derives
   serde and carried a backward-compatibility default, but it has no persisted form
   — no committed golden, no wasm export, only a self round-trip test — so the
   choice was one mechanism instead of two. The preset vertex lists lift into
   commands through a single helper, which goes away when they become table entries
   (FID-L-04).
2. **`a:cubicBezTo` and `a:quadBezTo` are modelled, imported, resolved, painted and
   re-emitted.** The quadratic stays a quadratic in the model and on save; only PDF
   promotes it, exactly, because PDF has no quadratic operator.
3. **Arity is enforced, not tolerated.** DrawingML reads a curve's `a:pt` children
   positionally, so a `cubicBezTo` with two points is not a cubic missing a control
   — there is no way to know which position was meant. A wrong count, or a command
   interrupted before its count is met, refuses the whole geometry and reports it,
   which is the same answer an unmodeled command gets.
4. **One bug this uncovered, worth recording.** The usability check required at
   least one `a:lnTo`, so a path made only of a move and a curve was still rejected
   after curves were modelled — the import suite stayed green while the capability
   was unreachable. It now asks whether any command *draws*, through a method on the
   command rather than a `matches!` at the call site, so a future command kind
   cannot be forgotten there. The generated fixture's curve shape moved from the
   reported side to the drawn side, and its loss count went 2 to 1.

5. **The guide formula LANGUAGE now exists**, as `casual-doc-layout::shape_guide`:
   all 17 ECMA-376 `a:gd@fmla` opcodes over the built-in variable environment
   (`w`/`h`/`ss`/`ls`, the box edges and centres, the `wd<n>`/`hd<n>`/`ssd<n>`
   fraction families, and the angle constants), with guides resolving in order so a
   later one can name an earlier one. Lengths are in the caller's unit; angles are
   1/60000 degree.

   Its first consumer is **adjustment resolution**. `adjustment_value` matched a
   `val ` prefix and fell back to the preset default for anything else, so a shape
   whose `adj` was computed drew with proportions nobody authored — and invisibly,
   since the shape still looked like itself and nothing reported a skip. A literal
   resolves exactly as before; the committed goldens are byte-identical.

   Deliberately NOT done with it yet: a guide-NAMED path coordinate
   (`<a:pt x="wd2"/>`) still refuses the geometry. That needs `a:gdLst` in the model
   and a path coordinate that can hold a name rather than an integer, which is a
   model change, not an evaluator one. The language was the stated blocker; it is no
   longer the blocker.

Still out of scope and still FID-G-02: `a:arcTo`; `a:gdLst` in the model and
guide-named path coordinates (the language exists, the consumer does not); multiple
subpaths; `a:ahLst`/`a:cxnLst`; the `a:rect` text rectangle; ODF export; Edit Points;
and path-exact hit testing.

### Rejected

- **An eighth `ShapeGeometry` variant (`Freeform`) carrying the points.** It
  puts the path in the same enum as the presets and so keeps two mechanisms for
  one rule; the enum is also matched exhaustively in eight crates, so every
  future primitive costs eight edits. The path is data about a shape, not a kind
  of shape — it is an optional field, and `Other` keeps its meaning of "no typed
  primitive".
- **Paint nothing when the geometry is unsupported**, which is what ONLYOFFICE
  does (§3, `isValid`). It is arguably the more honest rendering — a wrong shape
  is worse than a missing one — but it would silently erase every unsupported
  freeform in every existing document, which is a far larger visible change than
  this row justifies, and it is not what Word does. Keeping the bounding
  rectangle is a deliberate divergence from ONLYOFFICE and is stated as such at
  the fallback site in `anchor.rs`.
- **Reusing `ShapeGeometry::Line` for a two-point path.** It would have rendered
  the loan document correctly and nothing else, it would have attached
  arrowheads that the file does not declare, and it would have made the
  three-point case a new special arm. A slice that fits exactly one file is the
  thing `118` §4 warns about.
- **Adding a separate `Polyline` display primitive** alongside `Polygon`. Two
  implementations of one rule diverge (`SKILL.md` §8); a `closed` flag with a
  `serde` default of `true` keeps old display lists deserializable.

## 7. Guards

Every one was driven red by mutating production code; the mutations and their
verbatim output are in the branch's commit message.

1. **Import builds the path.** A `wps:wsp` whose `a:custGeom` is the loan
   document's path imports with `path = Some(ShapePath { width_emu: 6660515,
   height_emu: 0, commands: [MoveTo(0,0), LineTo(6660057,0)] })`. Reverting the
   `custGeom` arm to `geometry = Other; preset = None` fails it.
2. **The loss stops being reported** for a path we now carry, and is **still**
   reported for one we do not. The second half is the one that matters: a guard
   that only checks the supported case would let the unsupported case fall
   silent, which is the `no silent data loss` rule.
3. **An unsupported path is refused, not half-imported.** A `custGeom` with an
   `a:cubicBezTo`, a second `a:path`, or a second `moveTo`, keeps
   `path = None`, keeps the `custGeom` omission, and keeps painting a rectangle.
4. **Layout resolves to a polyline, not a rectangle** — `AnchorContent::Polygon
   { closed: false, points: [left-edge, right-edge] }` at the box's top, with
   the `@w` scale applied. Restoring the `Other => Rectangle` arm fails it.
5. **Render paints the path and not the box.** An open path along the *top* edge
   of a tall box: the top band is stroked and the bottom band is blank. A
   rectangle fallback paints the bottom edge too, so the mutation is visible as
   a pixel count, not as an internal type.
6. **Closed paths still close.** A three-point closed path paints all three
   edges; dropping the `closed` flag through to the backends fails it.
7. **Export re-emits `a:custGeom`**, and import → export → import is
   path-identical. Restoring `prst="rect"` fails it.
8. **The model refuses an unbounded path** — more than `MAX_SHAPE_PATH_COMMANDS`
   commands, or a coordinate outside `±MAX_EMU`.
9. **The registered fixture** `fixtures/generated/custom-geometry.docx`
   reproduces the loan document's five shapes in 1.5 KB, so the real file (which
   is the owner's and stays local) is not the only evidence.

## 8. Open questions

- **Should the unsupported fallback become "paint nothing"?** §6 rejects it for
  now on blast radius. It should be revisited once guide formulas land, because
  at that point the set of geometries we cannot draw is small and weird rather
  than large and ordinary.
- **`a:path@fill="none"`** on a path inside a filled shape is currently
  unrepresentable; the first slice imports no path that carries the attribute at
  a non-default value only because it imports no multi-path geometry. When
  FID-G-02 adds subpaths this has to be answered, not inherited.
