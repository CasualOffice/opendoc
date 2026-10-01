# 156 — Presentations (PPTX): the shared drawing core, and how to build it without disturbing DOCX

**Status:** **Design. Nothing here is implemented.** Tier 0 (§6) is DOCX work that
stands on its own and is the only part this branch begins.
**Opened:** 2026-10-01. **Decision:** [ADR-055](08-ADR-REGISTER.md).
**Advances:** a scope question `106` §1 and `135` §2 both currently answer "out of
scope" — presentations as "a future sibling". This document does not overturn that
answer; it prices it, and it separates the part that needs no decision from the part
that does.
**Depends on:** nothing. **Explicitly excludes:** charts and SmartArt authoring, which
landed on `main` as `155` / ADR-050 while this branch was in flight. Do not design them
here.

> **Read this first.** §6 is the load-bearing section. Every row in it is a **DOCX
> defect today**, whose correct fix happens to be the shared implementation a deck
> would need. That is why this document exists at the engine layer rather than as a
> product proposal: the first ten items require no commitment to presentations at
> all, and if presentations are never built, none of the work is wasted.
>
> §4.3 (Google Slides) and §4.4 (PowerPoint) are now measured, and they moved three
> decisions: animation authoring is deferrable because **neither competitor has it**
> (Slides has no animation model at all; PowerPoint for the web ships 37 of ~150
> effects), a master/layout **editor** is not v1 scope because **neither web client can
> edit masters**, and §6 row 0.7's autofit requirement was **wrong in a way that would
> have reduced fidelity** — §4.4 rank 4 records the correction.

---

## 1. The question

Can this repository accommodate PPTX and PowerPoint, and at what cost?

The honest answer has three parts, and only the third is a judgement call:

1. **The engine transfers further than expected**, because the seams already exist.
2. **The blockers are not the ones the format suggests.** Pagination is not the
   problem. Four specific gaps are (§5).
3. **Roughly two thirds of what makes a deck renderable is DOCX work that is
   currently done wrong** (§6). That part is not a presentation decision.

## 2. Measured, not estimated

Two provenances, kept separate throughout:

- **opendoc**, this working tree at `feat/shared-drawing-core`, branched from
  `6c4909aa`.
- **ONLYOFFICE v9.4.0**, read-only, at `sdkjs` `72b0421` and `web-apps` `9c0ca538`.
  Behavioural and scale observation only — **no AGPL code was copied or
  transcribed into this repository, and none may be.** The licence asymmetry is the
  whole wedge (`106` §2); importing their code would end it.

Where a number below is an estimate it says **ESTIMATE**. Per `105` EV-001 a
published number is derived from a committed artifact or it is not published, so
nothing here is destined for the site without a generator.

## 3. What transfers, with evidence

### 3.1 Already paradigm-neutral

| Asset | Evidence | Why it transfers |
|---|---|---|
| Rasteriser | `casual-doc-render/src/lib.rs:135` — `render(list: &DisplayList, surface, dpi, fonts, media)` | Knows nothing of pages or documents. Clips, layers with transform + blend, gradients, glyph outlines, media |
| Display list | `casual-doc-layout/src/display.rs:189` `PaintItem`, `:352` `DisplayList` | Glyphs / Rect / Ellipse / RoundedRect / Polygon / Image / Line / Shape / PushClip / PushLayer — a slide's paint vocabulary |
| Shaping | `casual-doc-layout/src/text.rs:712` `LineShaper`, `:653` `LineConstraints` | Ten scalar fields, zero page/section/column references |
| OPC substrate | `casual-doc-package` (1,034 lines); `casual-doc-ooxml/src/relationships.rs:79` `is_office_document_type` | Root part discovered by **relationship type**, not path. Proven reusable: `casual-doc-odf/src/package.rs:206` opens ODT through the same `BoundedPackage` |
| Format dispatch | `casual-doc-io/src/registry.rs:69` `FormatImporter` / `:77` `FormatExporter`; `FormatId` is a validated string newtype, not an enum | A new format registers with no core change |
| Loss vocabulary | `casual-doc-io/src/report.rs`; `casual-doc-import/src/coverage.rs` `meaningful_markup` | Reusable unchanged. `noop.rs`'s DrawingML half transfers directly |

### 3.2 The text-body-in-a-rect primitive already exists

This is the single most important finding, and it was not expected:

```rust
// crates/casual-doc-layout/src/flow.rs:4116
pub(crate) fn flow_anchored_text_box(
    document: &Document, blocks: &[BlockNode], shaper: &dyn LineShaper,
    outer_size: Size, properties: &TextBoxBodyProperties,
) -> FlowedTextBox
```

It builds a **fresh** flow context with `sections: &[]`, `line_grid: None`,
`paragraph_float_exclusions: None` (`flow.rs:851-864`) — no page, no `PageConfig`,
no `PaginatedLayout`. It is called four times today for exactly the "shape at an
absolute rect containing a text body" case (`anchor.rs:176/440/731/895`), and
`finish_text_box` (`flow.rs:4209`) already resolves insets, `TextBoxVerticalAnchor`,
`spAutoFit` growth and per-axis overflow clipping.

Publishing it is a four-line change. Alongside it:
`resolve_anchor_rect` (`anchor.rs:2002`) is a pure anchor→rect resolver;
`GroupMapper` (`anchor.rs:1606`) is a correct DrawingML child-space affine in EMU
with composing nested groups; `PlacedAnchor` (`page.rs:245`) is already "shape at an
absolute rect, with a z-order and a selectable node id".

### 3.3 Addressing needs no new scheme

`Pos { node: NodeId, offset: u32 }` (`casual-doc-edit/src/lib.rs:238`) is
**node**-addressed, not path-addressed, and `Surface`
(`casual-doc-edit/src/lib.rs:4710`) is *derived from the id on demand* rather than
threaded through op signatures — a deliberate design choice recorded at
`lib.rs:4700-4708`. "Shape on slide 7, run 3" is a new `Surface` arm, not a new
address space.

Consistent with this, all ten object-geometry operations are already classified
`Tier::NodeAddressed` in `casual-doc-transaction/src/transform/classify.rs` —
liveness and tombstoning, no positional arithmetic. Per `107` §3.1's own accounting
that is the cheap half of OT.

Note the op count: `Operation` has **58** variants
(`casual-doc-edit/src/lib.rs`, counted from the enum). `107` §3.1 says 47 and
§3.1's correction says 55. Both are stale; `107` should be corrected separately.

## 4. Competitive measurement

### 4.1 ONLYOFFICE: only 10.5% of a deck engine is presentation-specific

From their own build manifest `sdkjs/configs/slide.json`, resolved and counted:
**383 unique `.js` entries, 912,142 LOC.**

| Source area | LOC | Share |
|---|---|---|
| `word/` — the WordprocessingML engine | 372,693 | 40.9% |
| `common/Drawings` | 149,582 | 16.4% |
| `common/` root | 119,928 | 13.1% |
| **`slide/` — presentation-specific** | **95,983** | **10.5%** |
| `cell/` — pulled in because chart data is an embedded workbook | 87,670 | 9.6% |
| `common/Charts` + `Shapes` + `SmartArts` | 58,588 | 6.4% |

Their presentation-specific *specialisation* of the shared drawing layer is six
prototype-patch files totalling **1,966 lines**. The conclusion is blunt: **given a
word-processing text engine and a DrawingML layer, PPTX is an increment. Without
them it is not a PPTX project at all.**

### 4.2 DrawingML text costs ~1,800 lines, not a new engine

`common/Drawings/Format/TextBody.js` is only 652 lines because
`CDrawingDocContent` **derives from `CDocumentContent`**, the word-processing
content object (`DrawingContent.js:44-51`, verified by reading the prototype
chain). The nine list levels are word-processing paragraph-property objects in an
`Array(10)` (`Format.js:13370`). The placeholder text-style inheritance chain is
**~120 lines** resolving a style hierarchy (`Shape.js:2890`).

So they did not build a DrawingML text engine. They mapped `a:pPr`/`a:rPr`/
`a:lstStyle` onto the WordprocessingML property objects they already had and reused
the paragraph layout engine wholesale. **Marginal cost given a word engine ≈ 1,800
lines; without one ≈ 71,600.**

This corrects an earlier assessment on this branch that called `a:txBody` "the
largest single item with no partial version". It is a **property mapping plus a
style cascade**, and the cascade is the same resolution problem `cascade.rs`
already solves for `w:styles`. Build one cascade engine, not two.

### 4.3 Google Slides: the market's number two has no animation model at all

Google publishes no format specification, but the Slides REST and Apps Script
references are an exhaustive enumeration of the model, so an absence there is
strong evidence of an absence in the product. Sourced from
`developers.google.com/workspace/slides/api` and
`developers.google.com/apps-script/reference/slides`.

| Finding | Consequence for this plan |
|---|---|
| `Shape.Type` has 143 members: 141 concrete, 140 named presets, of which **135 carry an explicit "Corresponds to ECMA-376 `ST_ShapeType`" mapping**. Only **5** geometries are Google-proprietary (`ARROW_EAST`, `ARROW_NORTH_EAST`, `ARROW_NORTH`, `SPEECH`, `STARBURST`) | **Implementing the ECMA preset table buys shape parity with Slides outright.** Strongest possible validation of Tier 0.1 |
| **Zero occurrences of "transition" or "animat" across the entire Slides Apps Script reference** | Slides has no animation or transition model. **Deferring animation authoring is parity with the number-two product, not a compromise.** Resolves Q4 |
| `Presentation` has `getLayouts`/`getMasters` but **no insert/create counterpart**; `Layout` and `Master` expose `remove()` only | Master and layout *editing* is not table stakes. A layout **picker** is |
| `ColorScheme` exposes only `getConcreteColor`/`getThemeColors`/`setConcreteColor` | "Theme" in Slides means the 12-colour map and nothing else — **no `fontScheme`, no `fmtScheme` equivalent**. Tier 0.2 is still required, but for PowerPoint fidelity, not Slides parity |
| Presentations export to exactly **4** MIME types (pptx, odp, pdf, txt). PNG/JPEG/SVG appear only under Drawings, never Presentations | Per-slide image export is a differentiator, not a catch-up item |
| The whole REST surface is **2 resources, 5 methods**; `create` makes a blank deck only. WordArt is read-only (`getRenderedText`, no `setText`) | — |
| **No edit, change or selection-change trigger, and no webhook for Slides anywhere.** The installable Open trigger exists for Docs, Forms and Sheets but **not Slides**. Editor add-ons are desktop-only, Apps Script-only, and **do not run on `.pptx` at all** | **Google has no mechanism by which a third party can participate in a Slides editing session.** This is the sharpest unserved need in the analysis and it is exactly what `106` §2 already identifies as the wedge |
| Google publishes **no** PPTX-fidelity or unsupported-features page for Slides | We cannot cite a competitor's own disclosure; our loss reporting (A2) has no counterpart to compare against, which is an advantage to publish |

**The net effect on §8:** Tier 4 is not a gap against Slides. The bar that actually
requires animations is PowerPoint (§4.4), and `p:timing` retention must be verbatim
and proven either way — a deck that silently loses its animations on save is a
fidelity defect regardless of whether anyone authors them here.

### 4.4 PowerPoint: the fidelity bar, and what its own web client cannot do

Sourced from `support.microsoft.com`, `learn.microsoft.com`, the Microsoft Open
Specifications and ECMA-376/ISO 29500 normative text.

**The organising insight, and it reorders §6.** PresentationML defers almost
everything visual to a resolution chain. A renderer that gets the chain wrong does
not render one thing wrong — it renders *every slide* wrong, consistently, in a way
that looks like file corruption. A placeholder's slide XML is frequently nearly empty
(`<p:ph idx="1"/>`, an empty `<p:spPr/>`, and the text); essentially all of its
appearance comes from layout → master → `p:txStyles` → `defaultTextStyle` → theme.

The fidelity bar, ranked by how fast a normal user notices rather than by difficulty.
The top four are all resolution-chain problems:

| Rank | What breaks | Maps to |
|---|---|---|
| 1 | **Placeholder inheritance chain** — the single highest-leverage correctness investment in the product | Tier 2 cascade |
| 2 | **Theme colour resolution** — `schemeClr` + `clrMap`/`clrMapOvr` + ordered `lumMod`/`lumOff`, the `dk1`/`lt1` map bypass, and `phClr` resolving from the *referencing* colour. Almost nothing in a real deck uses literal RGB | **§6 row 0.2** |
| 3 | **Theme fonts and font metrics** — wrong typeface changes line breaking, so text overflows and the whole deck reflows | existing font stack |
| 4 | **Autofit** — see the correction below | **§6 row 0.7** |
| 5 | **187 preset geometries + adjust values + `p:style` theme refs** — "a bounded, mechanical job, so there is no excuse for partial coverage" | **§6 rows 0.1, 0.2** |
| 6 | Text layout: line breaking, `spcPct`, `bodyPr` insets/anchor/wrap, bullet `marL`/`indent`, tab stops — cumulative, and where clean-room implementations usually lose | existing flow engine |
| 7 | Group transforms with `chOff`/`chExt` | **already correct here** (`anchor.rs` `GroupMapper`) |
| 8 | Pictures: `srcRect` crop, `stretch`/`tile`, **and EMF/WMF** — dropping EMF/WMF alone makes a large fraction of real corporate decks look broken | **§6 row 0.3**, plus a decoder decision |
| 9 | **Table styles** — `tableStyles.xml` in a real file often contains only a `def` GUID with *no definitions*, so the built-in gallery must be supplied by the implementation. There is no way around this; the definitions are not in the file | Tier 2 `a:tbl` |
| 10 | Slide size and the EMU→px mapping | cheap, catastrophic if wrong |

**Correction to §6 row 0.7, from rank 4.** This document previously said an editor
"must re-solve shrink-to-fit on every keystroke". That is wrong and would *reduce*
fidelity. The correct behaviour is to **honour the persisted `fontScale` /
`lnSpcReduction` on load and never recompute them**, because PowerPoint computed them
with its own font metrics and recomputing with ours gives a different answer on every
overflowing slide — which in real corporate decks is most of them. A solver is needed
only when the user *edits* the text, and it must then match PowerPoint's
quantisation. So the existing behaviour (apply the authored scale) is right for
viewing and round-trip; the gap is narrower than stated and is an *editing* gap only.

**`p:timing` and `p:transition`, measured.** Both live in one schema file,
`pml-animationInfo.xsd`: 107 element declarations, 59 complexTypes, 31 simpleTypes,
**≈197 named schema components**. The time tree is uniformly recursive with unbounded
depth (`CT_TimeNodeList` is the same 13-member choice for `tnLst`, `childTnLst` and
`subTnLst`), `p:cTn` carries 24 attributes, and several PowerPoint-specific
deviations are not in the standard at all (`repeatCount` 1000 meaning one iteration,
`evtFilter="cancelBubble"`, four distinct readings of `fill`, the `(spid, grpId)`
join between `p:bldLst` and the time tree). The conventional tree shape PowerPoint
writes is **convention, not schema**.

**Deferring is viable, and the measurement says so twice over.** Microsoft's own web
client ships **37 of ~150 animation effects and 8 of ~53 transitions** — PowerPoint
for the web is itself a partial implementation. Combined with §4.3 (Slides has no
animation model at all), rendering no animations in v1 is defensible.

**But there is a hard condition, and it is a data-safety condition, not a feature
one.** `p:timing`, `p:transition`, `p:bldLst` and their `mc:AlternateContent`
wrappers must round-trip **byte-faithfully from day one**, with a non-destructive
affordance saying the editor does not yet play them. The failure that ends an
evaluation is not "the animation didn't play here" — it is "I opened my deck in your
editor, saved, reopened it in PowerPoint, and my animations were gone." That is
silent data loss, which `AGENTS.md` already forbids. Preservation cost is trivial:
one self-contained subtree per slide whose only outward reference is `spid`s, which
must be remapped on shape delete and copy.

**What PowerPoint *for the web* cannot do — the realistic bar for a browser
competitor.** Masters and layouts **cannot be edited** (preserved and rendered only);
themes cannot be modified; gradients, effects, eyedropper and styles are desktop-only;
there is no Outline, Slide Master, Notes Page or Presenter view; **charts are
view-only** — they cannot be created or edited; WordArt cannot be inserted; Merge
Shapes, 3-D insert, picture effects and ink insert are all unsupported; table cell
merge/split is desktop-only.

Two scope conclusions follow directly, and they agree with §4.3:

- **A master/layout *editor* is not v1 scope.** Neither Google Slides nor PowerPoint
  for the web can edit masters or layouts. A layout **picker** is table stakes; a
  master editor is not.
- **Charts being a read projection over a retained part is the correct position**, not
  a limitation — it is what PowerPoint for the web itself does. That independently
  supports ADR-050's framing.

### 4.5 Animation and transitions are ~30% of their slide engine

`Timing.js` 15,113 + `anim-pane.js` 3,203 = 18,316 (19.0%);
`Transitions.js` 5,599 + `TransitionsGL.js` 2,970 + `MorphTransition.js` 2,061 =
10,630 (11.1%). Combined **28,946 of 95,983 = 30.1%** — and it is the one part that
amortises across nothing else. This is the number that makes "preserve, do not
author" a real option rather than an excuse (§8, Tier 4).

### 4.6 Preset geometry: they paid full price, and we should not

`CreateGeometry.js` is 10,397 lines with **187 shape preset arms** plus 40
text-warp presets. `Geometry.js:159` `CalculateGuideValue` is a genuine
guide-formula evaluator implementing all **17** ECMA-376 `a:gd@fmla` opcodes — but
each of the 187 presets is **hand-transcribed** into imperative builder calls
rather than loaded from data.

**The evaluator is ~200 lines of real work; the presets are ~9,000 lines of
mechanical transcription.** ECMA-376 Part 1 ships
`presetShapeDefinitions.xml`. Generating the table from the specification is the
named prior art here (§10), and it is the difference between a bounded task and
nine thousand hand-written lines.

### 4.7 An open item against a published page

`106` §2 states the host-customisation API is "gated *in code*
(`LayoutManager._applyCustomization` early-returns when `!_licensed`)". The
early-return is real and verified at `LayoutManager.js:66-67`, and the presentation
editor initialises the same singleton identically.

**But** `sdkjs/common/Local/license.js` `_onEndPermissions` — the offline/local
fallback — unconditionally sets `Success`, `setCanBranding(true)`,
`setCustomization(true)` and `Rights.Edit`. So the gate is in the AGPL source while
the *flag* arrives via `asc_onGetEditorPermissions`, which the proprietary Document
Server drives. Run the AGPL build standalone and it is permissive.

`106` is a **published page** (`webapp/tools/build-doc-pages.mjs`), and under `105`
EV-003/EV-006 understating and overstating are both false. This is recorded as an
**open question for the owner (§11 Q1)**, not silently edited here.

## 5. The four gates, and why they are not pagination

| # | Gate | Evidence |
|---|---|---|
| G1 | **DrawingML text** — `grep` for `txBody`, `lstStyle`, `defRPr`, `buChar`, `buAutoNum` across `crates/` returns **zero**. Text in a shape arrives only via `w:txbxContent` (`import/body.rs:2318`) | Cheaper than it looks — see §4.2 |
| G2 | **Theme style matrix** — `a:fmtScheme` is captured as an opaque XML string (`import/theme.rs:123`, stored `model/v1/definitions.rs:1390`), `a:fillRef`/`lnRef`/`effectRef` are actively suppressed (`import/body.rs:3335`), and `phClr` resolves to `None` (`import/body.rs:8194`) | A typical deck would import **with no fill and no outline**. Word writes explicit `spPr`; PowerPoint does not |
| G3 | **Geometry** — 22 typed presets (`model/v1/body.rs:887`, `TYPED: [Self; 22]`, exhaustive by construction); everything else paints as its bounding rectangle (`anchor.rs:1458`). `ShapePathCommand` is `MoveTo`/`LineTo`/`Close` only, with curves "deliberately absent" (`model/v1/body.rs:997`), and there is no bezier anywhere in the display list | Also a live DOCX defect — §6.1 |
| G4 | **No mount seam** — `webapp/src/main.js` is 16,190 lines with **0** exports and 333 import-time `getElementById` bindings; HF-109 is open | A second surface today means a second non-module. `reflow_view.mjs` (260 lines + one engine setter) is the proof the extraction pays for itself |

## 6. Tier 0 — shared core, and DOCX needs every row today

**This is the part that needs no decision about presentations.** Each row is a
defect in the shipped DOCX product whose correct fix is the shared implementation a
deck requires.

| # | Defect today | What DOCX loses now | The fix, with its prior art |
|---|---|---|---|
| 0.1 | 22 typed presets; ~165 paint as bounding rectangles. No curve commands; multi-subpath `custGeom` rejected outright (`import/body.rs:5157`) | Every Word arrow, callout, banner, flowchart shape and curved connector renders as a rectangle | **Interpreter over a data table**: a guide-formula evaluator (ECMA-376 §20.1.9) + presets generated from `presetShapeDefinitions.xml`; add `CubicBezTo`/`QuadBezTo`/`ArcTo` and a path primitive to the display list. *Not* 165 hand-written arms |
| 0.2 | `fmtScheme` opaque; `*Ref` suppressed; `phClr` → `None` | Word's built-in shape styles lose their themed appearance | Parse the style matrix; resolve `phClr` by substitution; honour `*Ref@idx` |
| 0.3 | `Fill` has two variants. No shape `blipFill`, no `pattFill`/`grpFill`/`tile`. No line cap/join/compound, no `custDash`. Radial gradients collapsed to concentric | Picture-filled and patterned Word shapes lose their fill | Complete the fill/line model |
| 0.4 | `a:effectLst`: **zero** implementation at any layer; a populated list is reported as loss (`import/noop.rs:134-137`) | Word shape shadows and glows dropped on semantic save | Outer shadow + glow + soft edge is most of the value; 3-D is not. ONLYOFFICE round-trips glow/reflection/soft-edge with **no authoring UI in any editor** — parity here is cheap |
| 0.5 | Group `rotation`/`flip_h`/`flip_v` are modelled (`model/v1/body.rs:1064-1075`) and round-tripped, but `GroupMapper` (`anchor.rs:1605-1611`) carries only `scale_x/scale_y/tx/ty` | **A rotated Word group paints unrotated** — a live render bug | Add rotation/flip to the group affine |
| 0.6 | Three duplicated truncating `emu_to_twip` helpers (`flow.rs:4443`, `anchor.rs:2059`, `document_layout.rs:2070`), plus `emu_to_twip_f` (`anchor.rs:2064`) which **rounds**, and `emu_to_twip_signed` (`anchor.rs:2073`) which truncates toward zero | The two rules disagree by up to a whole twip (1000 EMU truncates to 1, rounds to 2), so a group child and a directly anchored sibling authored at the same EMU land a twip apart. Truncation also shrinks magnitudes, and an edge is `offset + extent`, so a right edge can fall two twips short and shapes authored to abut can seam. (Truncation toward zero *is* mirror-symmetric — an earlier draft of this row claimed otherwise and was wrong) | One `Emu` type in `units.rs`, one documented rounding rule, three copies deleted. Prerequisite for any EMU-native round-trip |
| 0.7 | `normAutofit` `fontScale` is read and applied but **never re-solved on edit** — three references in the whole layout crate (`flow.rs:4195`, `:4197`, and a test) | Typing in a Word text box with autofit leaves a stale scale — a live **editing** defect. Honouring the authored scale on load is *correct* and must not change (§4.4 rank 4) | A shrink-to-fit solver over `finish_text_box` + `block_intrinsic`, invoked **only on edit**, matching the producer's quantisation. Named prior art: bisection on a monotone predicate |
| 0.8 | `bodyPr@vert`/`@rot` unmodeled; `anchor.rs:927` states rotated text-box content is deferred | Word text boxes with vertical text render axis-aligned | Rotated/vertical text body |
| 0.9 | Group children carry `node: None` — "not individually selectable yet" (`page.rs:245-250`) | A UX gap in the shipped DOCX editor | Per-child identity |
| 0.10 | `Disposition`/ledger/`validate` live inside `casual-doc-import`; `casual-doc-io` sees a lossy projection (`io/src/docx.rs:448`) | **ODT already cannot make ledger-validated preservation claims** | Lift the taxonomy into a shared crate |

Rows 0.5 and 0.6 are the smallest and are where this branch starts.

## 7. Tier 1 — seams, still DOCX-first

Three of these are already on the owner's delivery order (`106` §6.0).

| # | Work | Existing row |
|---|---|---|
| 1.1 | Exports + `mount(el, config)` out of `main.js` | **HF-085 / HF-109 — cluster 3, "embeddability is the wedge"** |
| 1.2 | Publish the page-free layout seams: `flow_anchored_text_box`, an `AnchorRefs::in_frame` constructor, `GroupMapper`, `compose_blocks`, the preset-geometry functions | Behaviour-preserving; no DOCX risk |
| 1.3 | A document-class seam at the io boundary — `ImportArtifact.document` / `ExportRequest.document` hard-code `v1::Document` (`io/src/artifact.rs:115`); `FormatDescriptor` lacks its `135` §5 fields | `135` gaps that exist for DOCX/ODT today |
| 1.4 | `casual-doc-sdk`: `Engine::open`/`export` over `casual-doc-io` — it does not depend on that crate at all today; retire the frozen v0 path | **`135` Slice E, already "pending"** |
| 1.5 | "Absolute placement in a container coordinate space" beside `DrawingAnchor`'s wrap semantics. `SetGroupGeometry` + `GroupTransform` already *are* that abstraction | Prefer one mechanism over two |

## 8. Tiers 2–4 — presentation-specific

Scope is **not final** pending §4.3 and §4.4.

**Tier 2, engine (new crates, no DOCX surface touched).** `casual-pres-model`
(Presentation / Slide / SlideLayout / SlideMaster / ShapeTree / Placeholder) →
DrawingML text-body mapping (§4.2) → the inheritance cascade: slide → layout →
master → `defaultTextStyle`, plus `p:style` → `fmtScheme` and `clrMap`/`clrMapOvr`
→ `casual-pres-import`/`-export` over the parameterised OPC layer → slide layout
(small; consumes §3.2's published seams) → `a:tbl` + `tableStyles.xml`, **the only
genuinely PPTX-only item in this tier** → `casual-pres-wasm`.

**Tier 3, product.** `slides.html`, slide sorter, canvas, notes pane, layout picker,
master editor, command registry plus slide descriptors, locale keys.

**Tier 4, preserve-and-disclose.** Animations (`p:timing`), transitions,
presenter/slide-show runtime, media. Charts and SmartArt authoring belong to `155` /
ADR-050 and are **not designed here**.

Deferring animation *authoring* is settled (Q4): it removes ~30% of the
presentation-specific engine (§4.5) and is parity with both competitors (§4.3, §4.4).
What replaces it is **not optional and is not a Tier 4 item at all** — byte-faithful
retention of `p:timing`, `p:transition`, `p:bldLst` and their `mc:AlternateContent`
wrappers, with `spid` remapping on shape delete and copy, plus a non-destructive
affordance. That is a **Tier 2 import/export requirement**, because silent loss is
forbidden (`AGENTS.md`) and it is what would actually end an evaluation.

Two items leave v1 scope on the competitive evidence: a **master/layout editor**
(neither web client can edit masters — a layout *picker* is table stakes) and
**chart authoring** (PowerPoint for the web is itself view-only, which independently
supports ADR-050's read-projection framing).

## 9. Does this break DOCX, and how it is built so it does not

### 9.1 The three risks

1. **Tier 0 touches live DOCX render paths — deliberately.** The nets that catch a
   mistake: the oracle geometry gate (8 committed references under
   `fixtures/oracle/`, armed by a `the_oracle_gate_is_armed` test so it cannot be
   quietly removed), `casual-doc-layout/tests/geometry_snapshot.golden`,
   `casual-doc-export/tests/source_element_coverage.rs`, the import/export report
   coverage tests, and the 18-file corpus. A golden that moves must move
   **intentionally, with the diff explained** — never hand-edited to reach green.
2. **The combination risk is the real danger, not the change.** Adding
   `ShapePathCommand::CubicBezTo`, a `Fill` variant, or new `ShapeGeometry` presets
   breaks every exhaustive match and struct literal in the workspace — precisely
   §5a shape 1 of the skill, the `E0063` incident. Mitigation: `cargo check
   --workspace --all-targets --all-features`, not crate tests; rebase **last**;
   re-measure every ratchet from the merged file. The missing `_` arms are
   deliberate (`model/v1/body.rs:885-887`) — a new variant *should* fail to
   compile. Keep that, and do not reach for `Default::default()` to silence it.
3. **One guard constrains the plan, helpfully.**
   `casual-doc-model/tests/model_consumer_ledger.rs:74` sets
   `UNCONSUMED_CEILING = 17` as a ratchet: typing a construct that nothing in
   layout, render, wasm or the webapp reads **fails the build**. Presentation model
   types therefore cannot land ahead of their consumers. That is the guard against
   "modeled is not shipped" (`105`), and it forbids exactly how this kind of effort
   normally rots.

### 9.2 The no-disturbance rules (ADR-055)

1. **Additive model, never a modified one.** `v1::Document`
   (`model/v1/document.rs:42-57`) keeps its exact shape and byte-identical
   serialisation — its `skip_serializing_if` attributes exist for that reason. A
   presentation is a **sibling type in a new crate**; the document-class
   discriminator lives at the **io boundary**, not inside `Document`.
2. **New crates, not modified ones** for everything presentation-specific. Existing
   crates change only by (a) publishing functions that already exist, and (b) Tier 0
   corrections DOCX wants anyway.
3. **A second surface, not a modified one.** `slides.html` with its own wasm facade,
   embedded through the same `<opendoc-editor>` iframe with a different `src`.
   `main.js` is untouched, so the DOCX editor carries no risk. The cost is shell
   duplication — which is the argument for landing 1.1 first.
4. **Do not extract shared crates yet.** The seams already exist *inside* the
   current crates (§3). A big-bang extraction is the highest-risk, lowest-payoff
   move available. Publish in place; extract when a third consumer appears — which
   is also when `opencalc`'s duplication becomes worth paying down. (Measured:
   `opencalc`'s OPC layer is an independent reimplementation, not a copy.)
5. **Additive CI.** DOCX gates unchanged and green; presentation work brings its own
   corpus and goldens. Never arm a new gate on `main` — branch, prove it green with
   the output, then land.

## 10. Named prior art, per structural piece

Required by the skill before designing anything structural.

| Piece | The established solution |
|---|---|
| Preset geometry (0.1) | **Interpreter over a data table** — a guide-formula evaluator plus a generated preset table. ONLYOFFICE built the evaluator and then hand-transcribed 187 presets (§4.6); that is the mistake to skip |
| Theme style matrix (0.2) | **Indirection table with late substitution** — `phClr` is a formal parameter; `*Ref@idx` is the lookup |
| Placeholder inheritance (Tier 2) | **Style cascade**, already implemented once here for `w:styles` in `cascade.rs`. One engine, not two |
| Shape autofit (0.7) | **Iterative fixed-point on a monotone predicate** — bisection on the scale, not a re-layout loop per candidate |
| Unit handling (0.6) | **A single canonical unit with one documented rounding rule at the boundary** |
| Slide layout | **Absolute placement in a container coordinate space** — already present as `GroupTransform`; generalise rather than add a parallel path |
| Retention (Tier 4) | **Opaque part passthrough**, already implemented as `RetainedParts` (`import/src/opaque.rs`). Note its `RelationshipOwner` has only `Root` and `Document` variants and would need a presentation owner |

## 11. Open questions for the owner

- **Q1.** `106` §2's licence-gating claim needs the §4.7 refinement. Edit the
  published page, or record the nuance elsewhere? It is a competitive claim on a
  public page, so it is the owner's call, not this branch's.
- **Q2.** Does presentation support live in this repository or in a sibling, per
  `106` §1's "a future sibling"? §9.2 is written so the answer can change late: new
  crates and a second surface move between repositories cheaply, whereas a modified
  `v1::Document` would not.
- **Q3.** Tier 0 is DOCX work. Should it be sequenced against the existing FID-R and
  OO-014 rows in `105` and proceed independently of any presentation decision?
  This document's recommendation is yes.
- **Q4.** ~~Deferring animation authoring — survivable, or disqualifying?~~
  **CLOSED by §4.3 and §4.4: survivable.** Google Slides has no animation or
  transition model at all, and PowerPoint for the web ships only 37 of ~150 animation
  effects and 8 of ~53 transitions. Deferring *authoring* is therefore parity with
  both competitors, not a compromise. It converts into a **hard data-safety
  requirement** instead: `p:timing`, `p:transition`, `p:bldLst` and their
  `mc:AlternateContent` wrappers must round-trip byte-faithfully from day one, with a
  non-destructive affordance. Preservation is trivial; silent loss would end an
  evaluation and is already forbidden by `AGENTS.md`.
- **Q5.** `107` says the op set is 47 in one place and 55 in another; it is 58.
  Correct `107` separately.

## 12. What this document does not claim

- Nothing here is implemented. No capability is shipped, reachable, or measured in a
  product.
- **No LOC estimate for Tiers 2–4 is given.** §4.3 and §4.4 now bound the *scope*,
  but a line estimate for work with no prototype would be a fabricated number, and
  ONLYOFFICE's figures are their scale in JavaScript, not a prediction of ours. The
  first Tier 2 increment should be measured, then used to estimate the rest.
- The ONLYOFFICE figures are **their** scale, not a prediction of ours. Rust density,
  and the fact that their bundle carries an entire spreadsheet engine for chart data,
  make a direct ratio misleading.
- Charts and SmartArt are out of scope here by instruction; `155` / ADR-050 owns them.
