# 155 — DrawingML charts: model, rendering, and authoring

**Status:** accepted (ADR-050), implementation in `feat/charts`.
**Opened:** 2026-10-01.
**Rows:** `105` FID-R-08 (charts preserved but never drawn), `105` OO-014
(nothing to author), `106` §9 **Q3** — which this document closes in the
opposite direction to the recommendation on file.
**Depends on:** `105` FID-L-04 (`PaintItem::Path`) for one slice only — see §7.
**Related:** `35` (the disposition taxonomy), `34` (the fidelity architecture and
the preservation ledger), `45` (invariants I3 and I4), `119` (custom shape
geometry — the same "no curves" wall), `153` (the ONLYOFFICE parity matrix).

---

## 1. What is actually in the tree today, measured

`105` FID-R-08 reads:

> **Charts, SmartArt and OLE are preserved but never drawn** — an embedded
> preview if the file supplies one, else a typed text placeholder.

The first clause is true. **The second is false for charts and for SmartArt**,
and the row, `153`, and `webapp/src/fidelity.js` all repeat it. The evidence:

| claim | measured |
| --- | --- |
| "an embedded preview if the file supplies one" | `casual-doc-import/src/body.rs:5472` passes **`preview: None`** for `EmbeddedKind::Chart`, and `:5498` does the same for `EmbeddedKind::Diagram`. The only code that ever populates `EmbeddedObject.preview` is `commit_object` (`body.rs:5577`), which reads `v:imagedata@r:id` captured at `body.rs:3439` — inside a `w:object`, i.e. **legacy OLE only**. A DrawingML chart cannot acquire a preview on any path. |
| "else a typed text placeholder" | true, and it is the *only* outcome. `casual-doc-layout/src/flow.rs:3717-3745` takes the `preview` arm never, so every chart reaches `embedded_object_label` (`flow.rs:3748-3755`) and renders the literal five-character string **`[chart]`** in an unstyled `RunProperties::default()` run — no box, no frame, no reservation of the authored `wp:extent`. |

So the disclosed state is better than the real state. Two consequences that shape
this design:

1. **There is no honest fallback to preserve.** A chart in a Word document is
   today a word of body text that reflows the paragraph around it. Compare the
   `w:altChunk` placeholder, which *is* a dashed, labelled, fixed-size box
   (`flow.rs:7500` `ALT_CHUNK_PLACEHOLDER_TEXT`, decoration at `:7552`). The
   chart path is the weaker of two placeholders we already ship.
2. **A cached bitmap is not waiting to be wired up.** Word writes no fallback
   raster for a classic `c:chart` graphic frame. It writes one only for the
   *chartex* families (waterfall, treemap, funnel, sunburst, histogram, pareto,
   box-and-whisker), inside `mc:AlternateContent`; and because `cx1`…`cx8` are
   deliberately absent from `SUPPORTED_MCE_NAMESPACES` (`body.rs:169-171`,
   `:178-208`), those already resolve to their `mc:Fallback` **picture** and
   render as an ordinary image — via the picture path, never as an
   `EmbeddedObject`. That asymmetry is correct and stays.

What *is* in the tree, and is genuinely strong:

- `EmbeddedObject` / `EmbeddedPart` / `EmbeddedKind` — `casual-doc-model/src/v1/body.rs:1474-1537`. A chart is a first-class typed **reference** to a package part, with its verbatim `r:id`.
- The opaque part side-table — `casual-doc-import/src/opaque.rs`. `chart1.xml` is retained byte-for-byte **with its own `_rels` companion**, and `colors1.xml`, `style1.xml` and `word/embeddings/*.xlsx` are each retained in their own right because retention enumerates every admitted, non-plumbing, unconsumed part (`casual-doc-import/src/lib.rs:512-644`) rather than walking a relationship graph.
- A ledger record per retained part, so the `preserved` claim is auditable rather than asserted — `report.rs:596-603`, `Disposition::OmittedPreserved`, validated at `lib.rs:495-497` (a `preserved` claim with no ledger record is a hard `ImportError::Disposition`).
- An end-to-end guard: `chart_drawing_round_trips_as_an_editable_reference` (`casual-doc-export/src/lib.rs:6261`) asserts that after import → write → reopen, `word/charts/chart1.xml` **and** `word/embeddings/Microsoft_Excel_Worksheet.xlsx` come back byte-equal, and that the chart relationship is emitted exactly once.

**Nothing inside `chart1.xml` is parsed anywhere in the repository.** There is no
chart namespace reader, no series type, no axis type, no `c:numCache` reader; the
chart content-type string appears only inside two test fixtures. So this is a
greenfield subsystem sitting on a correct preservation floor — which is the best
possible starting position, and the thing this design must not spend.

---

## 2. The three decisions, up front

| # | Decision |
| --- | --- |
| **D1 — scope** | Classic DrawingML charts (`c:chart` in a `chart1.xml` reached from an `a:graphicFrame`). **Tier 1: bar/column, line, area, pie, doughnut, scatter — and combo, which falls out for free.** Out: every 3-D family, surface, stock, radar, bubble, pie-of-pie/bar-of-pie, and all of `chartex`. §4. |
| **D2 — chart data** | **Read the cache; retain the embedded workbook opaquely; never open it.** The cached data table is the rendering truth. `c:f` formula strings are carried verbatim as opaque strings and never evaluated. Chart *data* editing is offered only on charts **we** authored, where we own both sides; on an imported chart it ships **disabled with a reason**. §5. |
| **D3 — retention** | The typed model is a **read projection of a retained part, not a replacement for it.** Export byte-copies `chart1.xml` unless the chart is dirty; a chart we cannot type fully keeps working exactly as it does today. §6. |

And the question the task asked to settle before any code:

> **D4 — is `PaintItem::Path` required first?** **For pie and doughnut, yes. For
> bar, column, line, area and scatter, no.** *(Settled: the primitive landed, and
> pie/doughnut ship on it without an `ArcTo` — §7.5.)* It is the same primitive FID-L-04
> needs for ~165 preset shapes and the same one a SmartArt renderer would need,
> so it is built **once, in the shapes lane, as a display-list primitive** — not
> invented inside a chart renderer. That splits delivery into tier 1A (ships on
> today's primitives) and tier 1B (gated). §7.

---

## 3. ONLYOFFICE, read from their source

Their engine is checked out locally and the three decisions above were checked
against it rather than guessed. Identifiers, paths and observed behaviour only —
their source is AGPL-3.0 and none of it is vendored here, which is the same
standard `153` holds.

### 3.1 Which families they actually draw

`sdkjs/common/Charts/ChartsDrawer.js:282-322` constructs one drawer per chart
group: `drawBarChart`, `drawLineChart`, `drawHBarChart`, `drawPieChart`,
`drawScatterChart`, `drawAreaChart`, `drawStockChart`, `drawDoughnutChart`,
`drawRadarChart`, `drawBubbleChart`, `drawSurfaceChart`. At `:230-245` a second
table covers the *chartex* families: `drawWaterfallChart`, `drawFunnelChart`,
`drawParetoChart`, `drawTreemapChart`, `drawBoxWhiskerChart`,
`drawSunburstChart`. 3-D is a real projection, not a flattening — there is a
dedicated `common/Charts/3DTransformation.js`.

**They draw strictly more families than tier 1.** Stating that plainly is the
point of this section: `153`'s four chart rows are a genuine capability gap and
this programme narrows it rather than closing it. What tier 1 buys is the set that
appears in documents; what it defers is stock, radar, bubble, surface, 3-D and
the six chartex families (§4.3), of which chartex already renders here as its
fallback picture and the rest render as a placeholder with the loss reported —
which is more than their pipeline can say about anything it drops.

### 3.1a What they draw is not the same as what they keep

Their family count overstates their fidelity, and the reader rewrites several
chart types on the way in. From `SerializeChart.js`'s `ReadCT_PlotArea`
(`:12920-13060`):

| source | what their model ends up holding |
| --- | --- |
| `c:ofPieChart` | **converted to a plain `c:pieChart`** (`COfPieChart::convertToPieChart`, `ChartFormat.js:12845-12856`); the second plot, `splitType`, `splitPos`, `serLines` and `secondPieSize` are gone. The writer branch that would emit `ofPieChart` back (`SerializeChart.js:5644-5648`) is unreachable after a load, so **pie-of-pie is permanently lost on round-trip.** |
| `c:bubbleChart` | **converted to a `CScatterChart`** (`ChartFormat.js:9789`); the bubble sizes are discarded — which also makes `drawBubbleChart` dead code for imported files |
| `c:area3DChart`, `c:surface3DChart` | read as their 2-D/plain equivalents, and the writer branches are **commented out** (`SerializeChart.js:5593-5598`, `:5682-5687`), so they are written back flattened |
| `c:dTable` | modelled (`ChartFormat.js:10427`), read, written and even themed — and **never drawn**; there is no `dTable` reference anywhere in `common/Charts/` |

And there is **no cached-preview fallback for a chart on their side either.** The
only `cachedImage` on a chart space is produced on *copy* for clipboard interop
(`ChartSpace.js:5022`), and `cachedCanvas` (`:10498-10556`) is their own render
memoised. An unsupported construct is degraded at import or silently not painted;
the chartEx region-map case is an explicit early `return` in the draw path
(`ChartSpace.js:10586-10591`, with a `// TODO` above it).

So the correct comparison is not "seventeen against six". It is: **they draw more
families and they lose several of them, permanently and silently; we draw fewer
and lose none.** That is the trade this programme is making, and §6 is what makes
our half of it true.

### 3.2 How they hold chart data — and the boundary they drew

`c:externalData` is a typed member of their chart model, not bytes:
`CChartSpace.prototype.setExternalData` (`common/Drawings/Format/ChartSpace.js:5470`),
with `m_autoUpdate` read at `:4969-4973`, duplicated at `:2042-2043`, and
serialised by `SerializeChart.js:6123`. So they model the *reference*, exactly as
§8.3's `external_data: Option<EmbeddedPart>` does.

The load-bearing finding is what happens on **Edit data**. `btnEditData`
(`web-apps/apps/documenteditor/main/app/view/ChartSettings.js:352`) calls
`setEditData` (`:732-735`), which is one line: `this.api.asc_editChartInFrameEditor()`
— guarded by `Common.Controllers.LaunchController.isScriptLoaded()`. That API
(`sdkjs/word/api.js:9611-9621`) calls `oLogicDocument.OpenChartEditor()`, which
reaches `DrawingObjectsController.prototype.openChartEditor` and does this:

> `new AscCommon.CFrameDiagramBinaryLoader(this.getChartObject())` then
> `.tryOpen()` — `sdkjs/slide/Editor/DrawingObjectsController.js:287-292`,
> loader at `sdkjs/common/frameManager.js:849-866`.

The document editor then sits in `isOpenedFrameEditor = true`
(`common/apiBase.js:2689-2696`) and receives the result back as a **chart binary**
through `asc_editChartDrawingObject(chartBinary)` → `FinalizeEditChart`
(`word/api.js:9628-9641`).

What is on the other side of that frame is a **complete spreadsheet editor**:
`ExternalDiagramEditor.js:63-92` instantiates
`new DocsAPI.DocEditor(…, { documentType: 'cell', document: { url: '_chart_' },
editorConfig: { mode: 'editdiagram' } })` in an iframe and hands the blob over
with `serviceCommand('setChartData', …)`. Inside it, `CDiagramCellFrameManager.preObtain`
(`frameManager.js:511-523`) opens the **real embedded workbook** when one came
with the chart, and when one did not it opens an empty workbook and reconstructs
one from the caches *plus the parsed `c:f` formulas* (`fillWorkbookFromDiagramCache`,
`:488-497`).

So the accurate statement — and it is the one that matters — is this:

> **Their chart RENDERING never consults a workbook.** `recalculateReferences`
> begins `if (!oThis.worksheet) return;` (`ChartSpace.js:5631-5632`), and
> `worksheet` is only ever set from `cell/` code, never from `word/`. In the
> document editor a chart draws purely from `numCache`/`strCache`.
> **Every place they touch the workbook is a place a spreadsheet had to be
> brought in.** Save time: `CChartSpace::getXLSXFromCache` (`ChartSpace.js:2143-2167`)
> constructs `new AscCommonExcel.Workbook(...)`, fills it from the caches and
> serialises it with `AscCommonExcel.BinaryFileWriter` — so the document editor's
> bundle links the spreadsheet engine. Edit time: a whole second editor in an
> iframe.

That is exactly the accidental growth D2 refuses, demonstrated in the code of the
product we are an alternative to — and it confirms the *rendering* half of D2
outright: **cache-only rendering is what the market leader does.** It also tells
us what our own values-only writer (§5.3) would be a smaller version of:
`getXLSXFromCache` is that function, built on a spreadsheet model because they had
one to hand.

One consequence of their save path is worth stating because it cuts the other way
and is a real cost of manufacturing a workbook. `WriteCT_ChartXLSX`
(`SerializeChart.js:1417-1459`) writes `oVal.XLSX` verbatim when the chart was not
edited — so an untouched workbook *is* byte-preserved at their engine's level,
fairly noted. But when the chart carried **no** embedded workbook, the same
function manufactures one from the cache and injects it, so **their output gains
an embedding the input did not have**; and after an edit-data session the xlsx is
re-serialised whole from their spreadsheet model (`frameManager.js:203-222`,
`GraphicObjects.js:1346-1360`), discarding anything their model does not
represent. Both are reasons §5.3 refuses to rewrite a workbook we did not author.

Two further observations worth recording:

- `asc_onOpenFrameEditor` early-returns on `this.isMobileVersion`
  (`common/apiBase.js:2690-2692`), so **their chart-data editing is unavailable on
  mobile**. Our support matrix commits to mobile, so a data-editing design that
  depends on a second framed editor would inherit that hole; the §5.3
  authored-chart path does not.
- `ChartSettings.js:144` swaps the button caption to *Select data* when
  `externalRef` is set, and `:151`/`:1075` disable it while a reference is
  updating. Even they treat an externally-referenced chart's data as a different,
  more restricted case than an embedded one — which is the same distinction §5.3
  makes between a chart we authored and a chart we imported.

### 3.3 How they draw — one curve vocabulary, two geometry builders

This is the question D4 turns on. Their answer is precise rather than simple, and
the precision is the useful part.

Every chart element goes through `ChartsDrawer.prototype.drawPath`
(`ChartsDrawer.js:3136-3155`), which takes a path id and then:

> `var cGeometry = new CGeometry2(); … cGeometry.AddPath(oPath);
> this.cShapeDrawer.fromShape2(new CColorObj(pen, brush, cGeometry), …);
> this.cShapeDrawer.draw(cGeometry);`

**Shared with preset shapes:** `AscCommon.CShapeDrawer` — the same rasteriser
`Shape.js:5710` and `Image.js:622` use; the command vocabulary on
`AscFormat.Path2` (`Path.js:2540`) — `moveTo:2606`, `lnTo:2610`, `arcTo:2614`,
`quadBezTo:2640`, `cubicBezTo:2645`, `close:2651`; the emission layer
(`Path2::draw`, `Path.js:2654-2711` → `_s/_m/_l/_c2/_c/_z`); and **the arc
flattener** — `arcTo` is expanded to per-quadrant cubic Béziers by
`AscFormat.ArcToCurvers` (`ArcTo.js:43-216`, `EllipseArc3` at `:216`), which is
the identical function preset geometry uses (`Geometry.js:1556`).

**Not shared:** the geometry *representation*. Charts do not build an
`AscFormat.Geometry` with `pathLst`/`gdLst`/`avLst` guides. They use a chart-only
trio — `CPathMemory`, a growable `Float64Array` command arena on the chart space
(`ChartSpace.js:572-630`); `Path2` as a flyweight re-pointed into it by
`GetPath(index)` (`:603-606`); and `CGeometry2`, a throwaway shim **defined inside
`ChartsDrawer.js`** (`:17886-17940`) whose only job is to satisfy
`CShapeDrawer.fromShape2`.

So, corrected from an earlier draft of this document that claimed one mechanism
outright: **one rasteriser and one curve vocabulary; two geometry builders.** And
that split is exactly the right one, because the layer they *share* is the
primitive-and-flattener layer — which is what `PaintItem::Path` is — and the layer
they *do not* share is chart layout maths (axis scales, gutters, 3-D projection),
which nothing else would want. Their architecture endorses adding the primitive
once and letting charts build their own geometry with it.

Pie and doughnut, concretely — and correcting a citation an earlier draft got
wrong (`ChartsDrawer.js:8032` is the chartEx **sunburst** ring, not the pie):

- **Pie** — `drawPieChart::_calculateArc`, `ChartsDrawer.js:12448-12467`:
  `moveTo(centre)`, `lnTo(rim at start angle)`, `arcTo(radius, radius, -stAng,
  -swAng)`, `lnTo(centre)`. A sector as one arc and two radii.
- **Doughnut** — `drawDoughnutChart::_calculateArc`, `:14284-14314`: `moveTo`,
  `lnTo` outward, `arcTo` forward along the outer radius, `lnTo` inward, `arcTo`
  backward along the inner radius. An annulus as **two arcs and two radii**.

There is no `for (i…) lnTo` polygonisation of a pie sector anywhere in their
renderer. Circular markers are the same single `arcTo` sweeping `2π` (`:3200`).

**`c:smooth` is the exception, and it changes our delivery split.**
`CChartsDrawer::calculateSplineLine` (`ChartsDrawer.js:5435-5468`) derives
Catmull-Rom control points (`calculate_Bezier`, `:3744-3795`) and then evaluates
them at `t = 0, 0.1, … 1.0`, stitching **ten straight `lnTo` segments per data
interval**. The true-Bézier versions exist and are *switched off* behind an
explicit comment — `_calculateSplineLine2` at `:9366` and `:15238`, both preceded
by `//TODO for now enabling _calculateSplineLine function. with
_calculateSplineLine2 draws incorrectly. check!`. So a smooth line in the shipping
product is a sampled polyline, not a curve.

Four conclusions, and they are the whole of §7:

1. **`ArcTo` belongs in the command vocabulary and the flattening belongs in the
   backend** — which is how they do it, in one shared place. That settles Q-B, and
   for a better reason than "arcs are exact": it keeps one flattener rather than
   one per caller.
2. **A polygon fan is not how a production chart renderer draws a pie.** Ours
   would be the only one, and it would be a second curve mechanism in a repository
   whose shape lane is already scheduled to build the first.
3. **The primitive is the shared layer; chart geometry is not.** `PaintItem::Path`
   is therefore the correct first increment for FID-L-04, `119`'s remaining curve
   work, SmartArt *and* charts — one addition, four consumers — while the chart
   *geometry builder* is legitimately chart-specific.
4. **A smooth line does not need the primitive.** It is a sampled function, not a
   shape outline, so a polyline of evaluated points is the honest representation
   and is what the market leader ships. `c:smooth` therefore moves from tier 1B
   into **tier 1A**, and becomes a real cubic when the primitive lands.

---

## 4. Scope (D1)

### 4.1 What Word writes, and what we read

A chart in a `.docx` is a `w:drawing` → `wp:inline`|`wp:anchor` →
`a:graphic` → `a:graphicData uri="…/drawingml/2006/chart"` →
`c:chart r:id="rIdN"`, resolving through `word/_rels/document.xml.rels` to a part
of content type
`application/vnd.openxmlformats-officedocument.drawingml.chart+xml`. That part is
a `c:chartSpace` carrying `c:chart` (title, plot area, legend), `c:spPr`, and
optionally `c:externalData r:id` naming an embedded workbook. Its own `_rels`
reach `colors1.xml`, `style1.xml`, `chartUserShapes.xml` and the workbook.

### 4.2 Tier 1 — typed, drawn, and reachable

| family | OOXML chart group | why it is tier 1 |
| --- | --- | --- |
| Column / bar | `c:barChart` (`c:barDir` col\|bar; `c:grouping` clustered\|stacked\|percentStacked) | the most common chart in documents, and axis-aligned rectangles need no new primitive |
| Line | `c:lineChart` | polylines and markers need no new primitive |
| Area | `c:areaChart` | closed polygons need no new primitive |
| Scatter | `c:scatterChart` (`c:scatterStyle` lineMarker\|marker\|line) | markers plus straight connectors need no new primitive |
| Pie | `c:pieChart` (`c:firstSliceAng`) | very common; tier 1B — **drawn**, via `arc::sector` (§7.5) |
| Doughnut | `c:doughnutChart` (`c:holeSize`) | same geometry as pie; tier 1B — **drawn**, the same `sector` call with an inner radius |
| **Combo** | two or more chart groups in one `c:plotArea` | **free**: the plot area is modelled as a *list* of chart groups, so a bar+line combo is not a feature, it is the absence of a restriction |
| Secondary axis | a second `c:valAx` with its own `c:axId`, referenced by a group's `c:axId` | **free** for the same reason: axes are a list and a group names its own axis ids |

Combo and secondary axis being free is the payoff of modelling the plot area as
ONLYOFFICE's own tree does rather than as a single typed chart — and it is worth
saying out loud, because a "chart type" enum on the chart rather than on the
group is the mistake that makes both of them separate features later.

Chart furniture in tier 1: `c:title` (cached rich text only), `c:autoTitleDeleted`,
`c:legend` (`c:legendPos`, `c:overlay`), `c:catAx`/`c:valAx`/`c:dateAx`
(`c:delete`, `c:majorGridlines`, `c:minorGridlines`, `c:majorTickMark`,
`c:minorTickMark`, `c:tickLblPos`, `c:numFmt`, `c:scaling/c:min|c:max|c:orientation`),
`c:dLbls` (`c:showVal`, `c:showCatName`, `c:showSerName`, `c:showPercent`,
`c:dLblPos`), `c:plotVisOnly`, `c:dispBlanksAs`, `c:gapWidth`, `c:overlap`,
`c:varyColors`, and per-series solid `c:spPr` fill and line.

### 4.3 Deliberately out of tier 1, and what happens instead

Every construct below keeps working **exactly as it does today**: the part is
retained verbatim, the chart is not drawn, and a disposition is reported (§6).
Nothing regresses; a gap stays a gap and stays disclosed.

| out of scope | why | outcome |
| --- | --- | --- |
| `c:view3D` and every `*3DChart` group | A 3-D chart is a projection-and-lighting problem, not a plotting problem. Word's own 3-D charts are a formatting curiosity in documents. | `omitted` + `preserved` |
| `c:surfaceChart` / `c:surface3DChart` | needs a mesh renderer | `omitted` + `preserved` |
| `c:stockChart`, `c:radarChart`, `c:bubbleChart` | rare in documents; each is a distinct plotting geometry with no shared machinery | `omitted` + `preserved` |
| `c:ofPieChart` (pie-of-pie, bar-of-pie) | needs split-series semantics and connector geometry | `omitted` + `preserved` |
| `c:trendline`, `c:errBars` | each is a computation over the series, not a rendering of it; and a trendline needs curve fitting we would then have to defend numerically | `degraded` + `preserved` |
| `c:dTable` (data table under the plot) | a table inside a chart inside a document; it is real work and it is rare | `degraded` + `preserved` |
| `chartex` (`cx:chart`: waterfall, treemap, funnel, sunburst, histogram, pareto, box-and-whisker) | a different part schema entirely. Already handled better than a placeholder: it resolves to its `mc:Fallback` picture (§1) and renders as an image. | unchanged |
| `chartUserShapes.xml` (shapes drawn on top of a chart) | it is the shape renderer's problem, gated on the same `PaintItem::Path` | `omitted` + `preserved` |
| Gradient / picture / pattern series fills, shadows, bevels, soft edges | tier-1 fills are solid; effects are the shape lane's `a:effectLst` work | `degraded` + `preserved` |
| Floating (`wp:anchor`) charts | `EmbeddedObject` carries no `DrawingAnchor` at all — a pre-existing limit (P1F-28), not a chart limit. Charts are laid out inline, as they are today. Fixing it is one row for charts, SmartArt and OLE together, and it is not this design's. | unchanged |

### 4.4 Where we differ from ONLYOFFICE, stated plainly

§3.1 is the measured comparison, and it does not flatter us: the drawer tables at
`ChartsDrawer.js:230-245` and `:282-322` name **seventeen chart drawers** (bar and
horizontal bar are two of them for one OOXML group), against tier 1's six
families. The honest summary of the intended end
state is therefore: we match them on the families that actually appear in
documents, and on combo and secondary axis; we do **not** attempt 3-D, surface,
stock, radar, bubble, pie-of-pie, trendlines, error bars or data tables in this
programme, and chartex renders as its fallback picture rather than as a chart.
`153`'s chart rows narrow; none of them closes on scope alone.

We are ahead of them on exactly one axis, and it is the one §6 exists to protect:
**a chart we cannot draw is still byte-preserved and the loss is named.** Their
`DOCX → Editor.bin → DOCX` pipeline has no such floor — whatever `Editor.bin`
does not model is gone — so for them a rendering gap and a fidelity loss are the
same event, and for us they are not. Spending that to ship a family faster would
be the worst trade available.

---

## 5. Where the chart's data lives (D2)

### 5.1 The two sources, and why they disagree

A `c:ser` names its data twice:

```xml
<c:val>
  <c:numRef>
    <c:f>Sheet1!$B$2:$B$5</c:f>          <!-- a reference into a workbook -->
    <c:numCache>                          <!-- and the values, cached -->
      <c:formatCode>General</c:formatCode>
      <c:ptCount val="4"/>
      <c:pt idx="0"><c:v>4.3</c:v></c:pt>
      …
    </c:numCache>
  </c:numRef>
</c:val>
```

`c:f` is a formula into the workbook named by `c:externalData`, which is a whole
`.xlsx` package embedded at `word/embeddings/*.xlsx`. The cache is Excel's own
snapshot of what that formula evaluated to when the chart was last saved.

### 5.2 The decision

**Read the cache. Retain the workbook opaquely. Never open it.**

- **The cache is the rendering truth.** It is what Word itself paints — a chart
  displays from its cache without touching the workbook, which is why the cache
  exists. It is what every consumer that is not Excel uses. Reading it requires
  an XML reader over one part, which is squarely a document-importer's job.
- **`c:f` is carried verbatim as an opaque string and never evaluated.** It is
  stored so a regenerated `chart1.xml` can re-emit it unchanged, and for no other
  purpose. Nothing in this design parses a cell reference, a sheet name, a range
  or an operator.
- **The embedded workbook stays an opaque retained part.** It is already retained
  byte-for-byte with a ledger record, and `chart_drawing_round_trips_…`
  (`casual-doc-export/src/lib.rs:6261`) already proves the bytes survive. We add
  a typed **pointer** to it (an `EmbeddedPart`, which is a part name and a
  relationship id — not bytes, per `45` invariant I4) and nothing else.

**This is the line that stops a document editor growing a spreadsheet.** The
moment we read `word/embeddings/*.xlsx` we need a SpreadsheetML reader, a cell
model, a reference resolver and — the instant anyone edits — a formula evaluator
and a dependency graph. That is `opencalc`, a separate product with its own
crates and its own tests. The cost of *not* crossing the line is bounded and
stated in §5.3; the cost of crossing it is unbounded and would be paid forever.

### 5.3 What this costs, exactly

The cost is confined to one interaction: **editing a chart's data**.

If a user changes a value, the cache and the workbook diverge. The chart would
then draw our number while Word's own *Edit Data in Excel* opened the workbook
and showed the old one — a silent, user-visible inconsistency, and precisely the
kind of thing `12` §"no silent data loss" forbids. So the rule is split by
provenance, because the two cases are genuinely different:

| the chart was | who owns the workbook | data editing |
| --- | --- | --- |
| **authored by us** (§9 increment 6) | we do. We write a values-only `c:externalData` workbook, or we write no `c:externalData` at all and ship a cache-only chart — which Word renders correctly and simply offers no *Edit Data*. | **allowed.** Both sides are ours; they cannot diverge. |
| **imported** | the producer does. Its sheets may hold formulas, other series, other sheets, formatting — content we did not author and cannot regenerate. | **refused, with a reason.** The command ships **disabled and explains why** (`SKILL` §10: never a dead control), not absent and not silently lossy. |

Writing a values-only workbook for a chart we authored is a serializer for a
rectangular array of numbers and strings — no formulas, no references, no
evaluation. It is not a spreadsheet engine and it is not on the opencalc side of
the line. Writing over a workbook we *imported* is destructive, so it does not
happen in this programme.

**Recorded as the open question this leaves (§12 Q-A):** whether an imported
chart's data should later become editable behind an explicit "this replaces the
embedded workbook" confirmation, with the replacement reported. The
recommendation on file is yes, after the values-only writer exists and after the
loss it causes can be named in the compatibility report — but it is a decision
about destroying a user's data, so it is the owner's, not this document's.

---

## 6. What "preserved" must keep meaning (D3)

### 6.1 The rule

> **The typed chart model is a read projection of a retained part, not a
> replacement for it. `chart1.xml` and every part reachable from it are re-emitted
> byte-for-byte unless an operation has semantically modified that chart. A chart
> we model partly is drawn to the extent we modelled it and reported for the
> rest; it is never rewritten to the extent we modelled it.**

This is the named pattern, not an invention: it is **cache-plus-source-of-truth**,
the same shape as the existing `ExportMode::ExactIfUnchanged`
(`casual-doc-io/src/docx.rs:200-258`) and the same shape as the provenance map in
`34` §4. The retained bytes are the source of truth; the typed model is a derived
read index over them, valid until invalidated.

Three consequences, each enforced rather than intended:

1. **A chart is `dirty` or it is not.** Not dirty → export copies the retained
   bytes, exactly as today, and the typed model is never serialised into OOXML.
   Dirty → export regenerates `chart1.xml` from the model **and must report every
   construct the model did not capture**, because those are now genuinely lost.
   A chart whose projection is `ChartCoverage::Partial` therefore **cannot be
   made dirty**: the operation is refused rather than allowed to silently drop a
   trendline. That is the whole rule in one sentence, and it is what keeps
   "preserved" honest while the model is incomplete.
2. **Adding the projection changes no output byte.** Enforced by extending the
   existing `chart_drawing_round_trips_as_an_editable_reference` guard, and by a
   new guard over a fixture whose chart carries tier-1 *and* out-of-scope
   constructs, asserting the written part is byte-equal to the source part.
3. **A projection failure is never a document failure.** A `chart1.xml` that is
   malformed, over-limit, or of an out-of-scope family yields no projection, and
   the chart behaves exactly as it does today. Import must not fail, and the
   bytes must still survive.

### 6.2 Dispositions, per `35`

`35` requires exactly one value on each axis. The chart part itself is today
`omitted` + `preserved` (`lib.rs:596-603`). With a projection it becomes:

| construct | model outcome | retention outcome | why |
| --- | --- | --- | --- |
| The `a:graphicFrame` + `c:chart` reference | `mapped` | `preserved` | unchanged from today |
| A `chart1.xml` whose every construct is tier 1 | `mapped` | `preserved` | fully understood; the source bytes are also kept, which `35` explicitly allows and which is what licences an exact re-save. Not enumerated in the report — `35` says a `mapped` construct is not a finding. |
| A `chart1.xml` mixing tier 1 with §4.3 constructs | `degraded` | `preserved` | partially understood, remainder in the ledger. **Enumerated**, naming the specific unconsumed elements rather than the part. |
| A `chart1.xml` of an out-of-scope family (3-D, surface, stock, radar, bubble, ofPie) | `omitted` | `preserved` | exactly today's disposition; the projection simply declines |
| `word/embeddings/*.xlsx`, `colors1.xml`, `style1.xml`, `chartUserShapes.xml` | `omitted` | `preserved` | unchanged. The workbook is deliberately `omitted`: §5 says we do not model it, and saying so is the point. |
| A chart part over the retention byte ceiling | `omitted` | `rejected` | unchanged (`lib.rs:586-590`) |

The `degraded` + `preserved` row is the one that is new, and it is the row that
makes this work a *fidelity* improvement rather than only a rendering one: today
a chart with a trendline reports one line about a whole part, and afterwards it
reports which construct inside the chart we did not understand.

### 6.3 One defect fixed on the way in

`a:graphicData@uri` is captured at `body.rs:2776-2778` and **never read again**;
routing is by the element local name `chart` alone (`body.rs:2780-2782`). So any
element locally named `chart` carrying an `id`, in any namespace, inside any
`w:drawing`, is taken as a DrawingML chart reference — and `EmbeddedKind::Other`
is unreachable from import as a result (confirmed by the export comment at
`casual-doc-export/src/semantic.rs:7327-7330`). Verifying the captured uri
against the chart namespace before routing is a small, self-contained correctness
fix in this lane's own crate, and it is the precondition for a projection that
claims to have read a *chart*. It ships with its own mutation proof.

---

## 7. The rendering path, and whether `PaintItem::Path` comes first (D4)

### 7.1 The measured constraint — **superseded 2026-10-04, kept for the record**

> **This measurement no longer describes the tree.** The shapes lane has since
> replaced the display list's vertex-list `Polygon` with
> `ShapeGeometry::Path { commands, closed }` carrying
> `PathCommand::{MoveTo, LineTo, CubicTo, QuadTo}`, and `casual-doc-render`'s
> `command_path` walks all four — starting a new subpath on a mid-list `MoveTo`,
> so multiple subpaths work too. `FID-L-04`'s first clause is **done**. What
> follows is the state as measured on 2026-10-01 and is retained because §7.3's
> verdict was reached against it; the live position is in §7.5.

`casual-doc-layout/src/display.rs` — `PaintItem` (`:189`) has exactly twelve
variants: `Glyphs`, `Rect`, `Ellipse`, `RoundedRect`, `Polygon`, `Image`, `Line`,
`Shape`, `PushClip`, `PopClip`, `PushLayer`, `PopLayer`. The display list's own
`ShapeGeometry` (`:114-152`) has exactly five: `Rect`, `Ellipse`, `RoundedRect`,
`Polygon { points, closed }`, `Line`. **There is no arc, no quadratic and no
cubic anywhere**, and the model rejects curves at the door —
`casual-doc-model/src/v1/body.rs:991` records that `a:cubicBezTo`, `a:quadBezTo`
and `a:arcTo` "are deliberately absent". `105` FID-L-04 is correct, and its
stated fix order — "add `PaintItem::Path` + `a:path` command evaluation, then
table-drive the presets" — is the right one.

### 7.2 What each tier-1 family actually needs

| family | geometry | available today? |
| --- | --- | --- |
| Column / bar, incl. stacked and 100% stacked | axis-aligned rectangles | **yes** — `Rect` |
| Gridlines, axis lines, tick marks, legend keys, the plot border | straight segments and small rects | **yes** — `Line`, `Rect` |
| Line, incl. markers | polyline + marker glyphs | **yes** — `Polygon { closed: false }`, `Ellipse`/`Rect` |
| Area, incl. stacked | closed polygons | **yes** — `Polygon { closed: true }` |
| Scatter, `lineMarker`/`marker`/`line` with straight connectors | markers + polyline | **yes** |
| Line or scatter with `c:smooth="1"` | Catmull-Rom **evaluated to a polyline** — a sampled function, not a shape outline, and what ONLYOFFICE ships (§3.3) | **yes** — `Polygon { closed: false }` |
| **Pie** | circular sectors | **no** — an `Ellipse` cannot be a sector |
| **Doughnut** | annular sectors | **no** |
| Category labels, value labels, title, legend text | shaped text | **yes** — `Glyphs` |

### 7.3 The verdict

**`PaintItem::Path` is required before pie and doughnut, and before nothing else
in tier 1.**

It is *not* required for `c:smooth` — an earlier draft of this document said it
was, and §3.3 corrects that from their source: a smooth series is a curve
**sampled into a polyline**, which is what ONLYOFFICE ships and what the honest
representation of a sampled function is. That distinction matters and is not a
loophole: a pie sector is a *shape outline*, where flattening in the layout layer
would be a second curve mechanism competing with the primitive; a smooth line is a
*function evaluated at points*, where the polyline is the answer and the primitive
would only make it tidier later.

The tempting shortcut is to approximate a sector as a many-sided
`Polygon`. It is rejected, for a reason that is about architecture rather than
pixels:

- `119` §6 already named this exact wrong axis for shapes — per-preset
  hand-written vertex lists, "the wrong axis", with the fix being a path
  primitive plus a lookup table. A polygon fan for a pie is the same mistake with
  a different caller.
- `SKILL` §8: *prefer one mechanism over two; when a design needs a parallel
  path, that is evidence the abstraction is wrong.* A curve flattener inside the
  chart renderer is a second curve mechanism, and the first one is already
  scheduled.
- The primitive has **three** consumers waiting — ~165 preset shapes (FID-L-04),
  `a:custGeom` curves (`119`, FID-G-02), and charts — so it is the single
  highest-leverage addition in the render lane, and building it for one of them
  in a way the other two cannot use is the expensive outcome.

`casual-doc-layout` and `casual-doc-render` are held by another lane, so this
design **does not add the primitive**; it specifies what charts need from it
(§7.4) and splits delivery so that nothing waits unnecessarily:

- **Tier 1A — bar, column, line (including `c:smooth`), area, scatter, plus all
  furniture, combo and secondary axis.** Ships on today's primitives. No
  render-lane dependency beyond a chart consumer in `flow.rs`.
- **Tier 1B — pie and doughnut only.** Gated on `PaintItem::Path`. Until it
  exists these two take the §6.2 `omitted` + `preserved` path — the placeholder,
  reported — which is honest, is what happens today, and is strictly better than a
  curve mechanism we would have to delete. It is also a smaller hole than the
  earlier draft implied, because smooth lines moved out of it.

**Both tiers have now shipped.** Tier 1A in #744; tier 1B on
`feat/charts-arcs-and-reach`, which did not wait for an `ArcTo` because it did
not need one — see §7.5. `chart::is_drawable` no longer refuses any
`ChartGroupKind`, so the §6.2 `omitted` path is no longer reached by any tier-1
family.

### 7.4 What charts needed from `PaintItem::Path` — the ask, as filed

Stated here so the primitive is designed once for all three consumers:

- A command list, not a vertex list: `MoveTo`, `LineTo`, `CubicTo(c1, c2, end)`,
  `ArcTo`, `Close`, with integer coordinates in the same device-scaled twips the
  rest of the display list uses (the list must stay `Eq` and serialisable for the
  golden harness).
- `fill: Option<Fill>` and `stroke: Option<ShapeOutline>` on the path, matching
  `PaintItem::Shape`, so a sector can be filled and stroked in one item.
- Multiple subpaths in one item, with a stated fill rule — a doughnut ring is
  naturally an outer arc and an inner arc, and every SmartArt glyph needs it too.
- An explicit `ArcTo` rather than Bézier-only, flattened **once** in the backend
  rather than once per caller — `ArcTo.js:216` is where they do it and
  `Geometry.js:1556` is the second caller that therefore did not need its own.
  If the primitive ends up Bézier-only, charts will use the standard
  four-segment-per-quadrant approximation and say so in code.

### 7.5 What was actually built (2026-10-04) — and why it is not a fifth primitive

The shapes lane delivered the first three bullets of §7.4 and **not** the fourth:
`ShapeGeometry::Path` is a command list, it carries `fill` and `stroke` like
`PaintItem::Shape`, and `command_path` opens a new subpath on a mid-list
`MoveTo`. It is **Bézier-only** — there is no `ArcTo` — which §7.4's last
sentence already pre-authorised, and that is the branch taken.

`casual-doc-layout/src/arc.rs` is therefore a **constructor over the existing
primitive, not an addition to it**. No display-list variant was added. The one
public function is

```rust
pub fn sector(
    center: Point,
    outer_radius: Twip,
    inner_radius: Twip,
    start: i32,   // ST_Angle: 60000ths of a degree, clockwise from 3 o'clock
    sweep: i32,
) -> Vec<PathCommand>
```

Four decisions worth recording, because they are the ones a reader will want to
re-litigate:

1. **Cubic approximation, one segment per <=90 degrees**, handle length
   `k = (4/3)·tan(Δ/4)·r` — the textbook result SVG, PostScript, cairo and Skia
   all use. Maximum radial error is `2.8e-4·r`, published as
   `arc::MAX_RADIAL_ERROR_RATIO` so the guard cites the same number the doc
   comment does. On a 3-inch pie that is under 0.6 twip, i.e. **finer than the
   1-twip resolution the display list stores points at**, so there is no
   tolerance to tune and none is exposed.
2. **An `ArcTo` command was still not added**, and the reason is narrower than
   §7.4 assumed: `PathCommand` is the display-list mirror of
   `casual_doc_model::v1::ShapePathCommand`, so a fifth variant on the display
   side alone diverges the mirror, and the model crate is held by another lane.
   The preference in §7.4 and Q-B stands on merit — if the model gains
   `a:arcTo`, `arc::append_arc` is the function that becomes its backend
   lowering unchanged, and it is already the single shared flattener that
   `ArcTo.js:216` is cited for.
3. **A doughnut is a pie with an inner radius, not a second function.** A sector
   is emitted as a *single closed contour* — outer arc forward, radial segment
   inward, inner arc backward, closing radial segment — so the hole falls outside
   the contour and is empty under **any** fill rule. This answers §7.4's third
   bullet differently and better: a ring does not need multiple subpaths or a
   stated fill rule after all. Only a **full-turn** ring, which has no radii to
   cut, uses a second reversed subpath and relies on nonzero winding (which
   `casual-doc-render` uses at every `fill_path` call site).
4. **Angles are `ST_Angle`, clockwise from three o'clock** — DrawingML's own
   convention, and the unit `ShapeTransform::rotation` already uses — so the
   preset table and the `a:custGeom` evaluator reuse `arc` without a second frame
   of reference. A chart's `c:firstSliceAng` is measured from twelve o'clock;
   that quarter-turn offset lives in `chart::draw_pie`, not in the geometry.

`ChartPrimitive::Path` changed from a vertex list to the same `Vec<PathCommand>`
in the same work, so there is one chart path primitive rather than a straight one
and a curved one — the consolidation `ShapeGeometry` had already made.

The other two consumers §7.3 names are **still waiting**: the ~165-preset table
(FID-L-04's second clause) and `a:custGeom` curves (`119`, FID-G-02). `arc` is
built for them, not only for charts — several presets (`pie`, `arc`, `blockArc`,
`chord`, `circularArrow`) are sectors, and `sector` is already the function they
need.

---

## 8. The typed model — exactly what `casual-doc-model` needs

**`casual-doc-model` is held by the co-editing lane. This lane does not touch
it.** The request below is precise enough to be implemented without further
design, and §9 sequences this lane's work so it is not blocked waiting.

### 8.1 Where it attaches, and why not on `EmbeddedObject`

A new definition table on `Definitions`, keyed by a new `ChartId`, with each
`Chart` naming the `NodeId` of the `EmbeddedObject` it projects:

```rust
// crates/casual-doc-model/src/v1/definitions.rs — inside `Definitions`
/// Typed chart projections by id. Additive: omitted when empty so existing
/// snapshots serialize byte-identically. `docs/155`.
#[serde(default, skip_serializing_if = "DefinitionMap::is_empty")]
pub charts: DefinitionMap<ChartId, Chart>,
```

Two reasons this is a side table and **not** a field on `EmbeddedObject`:

- **It is the anchoring invariant.** `45` I3 says anchors are `NodeId`/`ModelPos`;
  I4 says derived and opaque data does not live in the node model. A chart
  projection is derived data about a retained part — it belongs beside `media`,
  not inside the node.
- **It is the low-conflict choice, and that is not incidental.** `SKILL` §5a
  shape 1 is a field added to a struct on one branch breaking every struct
  literal on another, with nothing for git to conflict on. There are **15**
  `EmbeddedObject { … }` literals across six crates including
  `casual-doc-wasm/src/lib.rs`, `casual-doc-layout/src/flow.rs` and
  `casual-doc-edit/src/lib.rs` — every one of which is in a crate another lane
  holds tonight. `Definitions` derives `Default` and every field is
  `#[serde(default)]`; adding a table is the established additive pattern in that
  file (`footnotes`, `bookmarks`, `field_ranges`, `font_scheme` all carry the same
  "Additive: omitted when empty" comment). Counted from the merged tree at the
  time of writing; the point is the shape, not the number.

### 8.2 The hard constraint nobody may violate: no floats

`Definitions` derives `Eq` (`definitions.rs:1323`), and **the v1 model contains
zero `f64` or `f32` occurrences** — `body.rs` and `definitions.rs` each grep to 0.
A chart's cached values are decimal numbers, so this is a real collision, and the
resolution is deliberate:

```rust
/// One cached data point. A number is kept in its VERBATIM lexical form — the
/// text of `c:v` — and parsed to `f64` by the layout consumer, never stored as a
/// float. Two reasons, and the second is the load-bearing one: the v1 model
/// derives `Eq` and holds no floating-point anywhere; and a byte-faithful
/// rewrite of `chart1.xml` must re-emit the spelling the producer wrote, which a
/// parse-and-reformat round trip through `f64` does not preserve ("4.30" and
/// "4.3" are the same number and different documents).
pub enum ChartValue {
    Number(String),
    Text(String),
    Blank,
}
```

with a `ChartValue::as_f64(&self) -> Option<f64>` helper for consumers. The same
rule applies to axis `c:min`/`c:max` and to `c:holeSize`-style percentages that
OOXML spells as decimals.

### 8.3 The shape

A new module `crates/casual-doc-model/src/v1/chart.rs`, re-exported from
`v1::mod`, deriving what every other v1 type derives
(`Clone, Debug, Deserialize, Eq, PartialEq, Serialize`, `rename_all = "camelCase"`,
`deny_unknown_fields`):

```rust
pub struct Chart {
    pub id: ChartId,
    /// The `EmbeddedObject` this projects (`45` I3 — a NodeId anchor).
    pub object: NodeId,
    /// How much of the source part the projection captured. `Partial` forbids
    /// regeneration (§6.1 consequence 1).
    pub coverage: ChartCoverage,          // Complete | Partial
    pub title: Option<ChartTitle>,        // c:title cached text + c:overlay
    pub auto_title_deleted: bool,         // c:autoTitleDeleted
    pub plot_area: PlotArea,
    pub legend: Option<Legend>,           // c:legend
    pub plot_visible_only: bool,          // c:plotVisOnly
    pub display_blanks_as: DisplayBlanks, // c:dispBlanksAs: gap | zero | span
    pub vary_colors: bool,                // c:varyColors
    /// Pointer to the embedded workbook named by `c:externalData`, when present.
    /// A part name and a relationship id — never bytes (`45` I4, §5.2).
    pub external_data: Option<EmbeddedPart>,
}

pub struct PlotArea {
    /// One or more chart groups. More than one IS a combo chart (§4.2).
    pub groups: Vec<ChartGroup>,
    /// The axes, each with its own `c:axId`; a group names the ids it uses, so a
    /// secondary axis is just a second entry (§4.2).
    pub axes: Vec<Axis>,
}

pub struct ChartGroup {
    pub kind: ChartGroupKind,
    pub series: Vec<Series>,
    pub axis_ids: Vec<u32>,               // c:axId, in declaration order
}

pub enum ChartGroupKind {
    Bar { direction: BarDirection, grouping: BarGrouping, gap_width: u16, overlap: i16 },
    Line { grouping: Grouping, marker: bool },
    Area { grouping: Grouping },
    Pie { first_slice_angle: u16 },
    Doughnut { first_slice_angle: u16, hole_size: u8 },
    Scatter { style: ScatterStyle },
}

pub struct Series {
    pub index: u32,                       // c:idx
    pub order: u32,                       // c:order
    pub name: Option<ChartText>,          // c:tx (cached only)
    pub categories: Option<DataRange>,    // c:cat
    pub values: DataRange,                // c:val, or c:yVal for scatter
    pub x_values: Option<DataRange>,      // c:xVal (scatter)
    pub fill: Option<Color>,              // solid c:spPr fill only (§4.3)
    pub line: Option<ChartLine>,
    pub smooth: bool,                     // c:smooth — tier 1A, sampled (§7.3)
    pub data_labels: Option<DataLabels>,  // c:dLbls
}

/// A cached data range. THE CACHE IS THE DATA (§5.2).
pub struct DataRange {
    /// `c:f`, verbatim and opaque. Stored so a regenerated part re-emits it
    /// unchanged; never parsed, never evaluated, never resolved against a
    /// workbook. Reading it as a formula is how this product would grow a
    /// spreadsheet engine by accident (§5.2).
    pub formula: Option<String>,
    /// `c:ptCount` — the producer's declared length, which may exceed
    /// `points.len()` when the cache is sparse.
    pub point_count: u32,
    /// `c:pt`, in `idx` order, from `c:numCache`/`c:strCache`.
    pub points: Vec<(u32, ChartValue)>,
    /// `c:formatCode`.
    pub number_format: Option<String>,
}
```

plus `Axis`, `Legend`, `DataLabels`, `ChartTitle`, `ChartText`, `ChartLine`, and
the small enums (`BarDirection`, `BarGrouping`, `Grouping`, `ScatterStyle`,
`LegendPosition`, `TickMark`, `TickLabelPosition`, `DisplayBlanks`,
`ChartCoverage`).

### 8.4 Bounds and validation

Validation belongs beside the existing `EmbeddedObject` checks
(`casual-doc-model/src/v1/document.rs:1323-1345`), and a chart part is
attacker-controlled input, so every list is bounded:

| bound | value | why |
| --- | --- | --- |
| groups per plot area | 16 | a combo chart beyond this is not a document chart |
| series per group | 256 | ONLYOFFICE and Excel both stop far below this |
| points per `DataRange` | 32 768 | matches the spirit of the existing `MAX_SHAPE_PATH_COMMANDS` ceiling: generous, finite |
| axes per plot area | 8 | primary + secondary on both axes, with slack |
| `ChartValue::Number` / `Text` length | 64 / 1024 bytes | a cached value is a number or a label |
| `DataRange::formula` length | 2048 bytes | the same ceiling `EmbeddedKind::Other`'s uri already uses |
| `Chart::object` | must resolve to an `EmbeddedObject` of `kind == Chart` | a dangling projection is a `ModelError`, exactly as `DanglingMediaRef` is today |

Exceeding a bound yields **no projection** (and a `degraded`/`omitted` +
`preserved` report), never a failed import — §6.1 consequence 3.

### 8.5 The complexity contract

`SKILL` §8 requires a stated complexity on anything touching the document.
Reading a chart part is O(part bytes), once, at import. `Definitions::charts` is
a `BTreeMap`, so a lookup is O(log charts) and **layout must fetch a chart
projection once per chart and carry it**, never look one up inside a loop over
data points. Building a chart's display items is O(points in the chart) and
independent of document size, so it does not violate the O(1)-per-interaction
budget (`107` §4).

---

## 9. Delivery order

Each increment is a commit, each has its own guard driven red before it is
believed, and each states whether a user can reach it.

| # | Increment | Crates | Reachable by a user when it lands? |
| --- | --- | --- | --- |
| 1 | **This document + ADR-050.** | `docs/` | No — it is a design. Said plainly rather than implied. |
| 2 | **The chart-XML reader**, producing the typed projection, behind a feature-gated internal API in this lane's own crates; plus the §6.3 `graphicData@uri` verification; plus a synthetic chart fixture carrying tier-1 *and* out-of-scope constructs, manifested. | `casual-doc-ooxml` (fixture), `casual-doc-import` | No. A reader with no consumer is the §9-rule-4 trap, and it is declared as such. |
| 3 | **Disposition reporting per construct** — the §6.2 `degraded` + `preserved` rows, replacing one line about a part with named findings. | `casual-doc-import` | **Yes** — the compatibility report is a user-visible surface, and this is the first increment with a real answer to "what did you not understand about my chart?" |
| 4 | **The export writer + the never-a-byte-changed guard**: `chart1.xml` byte-copied while clean, regeneration refused while `coverage == Partial`, and the round-trip guard extended to a chart with out-of-scope constructs. | `casual-doc-export` | **Yes**, as a guarantee rather than a feature: saving a document with a chart provably does not touch it. |
| 5 | **The layout/render consumer — tier 1A.** *Belongs to the layout lane.* This lane supplies the projection, the fixture and the expected geometry; it does not edit `casual-doc-layout`. | (layout lane) | **Yes** — this is the increment where a chart stops reading `[chart]`. |
| 6 | **Tier 1B** (pie and doughnut) once `PaintItem::Path` exists. **Done** — `feat/charts-arcs-and-reach`, on a Bézier-only primitive via `arc::sector` (§7.5); no `ArcTo` was needed. | (render + layout lanes) | Yes. |
| 7 | **Authoring**: insert a chart, chart elements, type and style, and data editing on charts we authored (§5.3). | `webapp` + facade lanes | Yes — and not before, which is why `153`'s four chart rows stay open until then. |

Increments 2–4 are this lane's and are buildable now. Increments 5–7 are
specified here and handed over.

---

## 10. Reachability — the rule that governs this lane

`SKILL` §9 rule 4: *"modeled" is not "shipped" and "built" is not "reachable."*
`105` FID-R-08 is honest today precisely because it claims nothing but
preservation. This programme must not turn it into a modelled-but-unreachable
claim, so:

- **No grade in `webapp/src/fidelity.js` moves, and no `105` row closes, until
  increment 5 lands.** Increments 2–4 add a reader, a report and a guarantee;
  none of them draws a chart, and the public artifact must keep saying so.
- FID-R-08's text needs correcting *downward* first, per §1: charts and SmartArt
  never show a preview. That is one row's text in `105`, owned by whichever lane
  owns the tracker, and it is an overstatement being removed — not a gap being
  published.
- The Charts family in `fidelity.js` currently grades `modeled: "full"`. With no
  chart data model of any kind in the tree, that is an overstatement by the same
  §9 standard, and it should read `partial` (a reference is modelled; the chart is
  not) — `webapp/` is another lane's, so this is reported, not edited.

---

## 11. Guards, and the mutation each must survive

Every guard below is written, then driven red by mutating production code, with
the red output recorded in the commit (`SKILL` §4).

| guard | asserts the guarantee | mutation that must redden it |
| --- | --- | --- |
| `chart_projection_reads_cached_series` | the cache, not the formula, is what the projection carries | make the reader skip `c:numCache` |
| `chart_formula_is_carried_but_never_parsed` | `c:f` survives verbatim | have the reader normalise or split the formula string |
| `graphic_data_uri_must_be_the_chart_namespace` | §6.3 — an element named `chart` in a foreign namespace is **not** routed as a chart | restore the local-name-only match |
| `out_of_scope_chart_yields_no_projection_and_reports_it` | a 3-D or surface chart still preserves and still reports | make the reader project it partially and claim `Complete` |
| `a_partially_projected_chart_cannot_be_regenerated` | §6.1 consequence 1 | let the writer regenerate while `coverage == Partial` |
| `chart_part_bytes_are_unchanged_by_the_projection` | §6.1 consequence 2 — the whole retention rule | have export write the part from the model when clean |
| `a_malformed_chart_part_does_not_fail_the_import` | §6.1 consequence 3 | make the reader propagate its error |
| `every_out_of_scope_construct_is_enumerated` | `35`'s rule that `not-retained` and `degraded` are reportable, enumerated by **family** rather than by the successes we happened to have (`SKILL` §9 rule 3) | drop one family from the reporting table |

The last one is the one to be most careful with: it must assert the guarantee
(*every* out-of-scope family in §4.3 produces a finding) and not the circumstance
(*this fixture* produced three findings), or it reddens `main` the day a family
moves from out-of-scope to tier 1.

---

## 12. Open questions

| # | Question | Recommendation |
| --- | --- | --- |
| Q-A | Should an **imported** chart's data become editable behind an explicit "this replaces the embedded workbook" confirmation? (§5.3) | **Answered by building it, §16** — after the owner's "its static, how can i add data". The values-only writer exists, the replacement is named in the report, and the panel says it before the first change. Reversible in one line (`chart_authoring_refusal`) if the owner wants imported workbooks untouchable. |
| Q-B | Does `PaintItem::Path` get an explicit `ArcTo`, or Bézier-only? (§7.4) | **Shipped Bézier-only**; the preference below still stands for when the model mirror can carry `a:arcTo` (§7.5 point 2). `ArcTo`, flattened in the backend. Their renderer keeps `arcTo` in the command vocabulary and expands it to per-quadrant cubics in **one** shared function (`ArcTo.js:216`), which is what stops every caller growing its own flattener (§3.3). |
| Q-C | Floating (`wp:anchor`) charts: fix `EmbeddedObject`'s missing `DrawingAnchor` in this programme or as one row for charts + SmartArt + OLE together? | Together, separately. It is not a chart limit and scoping it here would hide it. |
| Q-D | Do chart **theme** colours resolve against `Definitions::color_scheme`, or does a chart carry its own `colors1.xml` palette? | Resolve against the document theme, which already exists; treat `colors1.xml` as out of scope and preserved. Revisit if real files disagree. |

## 13. What this design deliberately does not do

- It does not read `word/embeddings/*.xlsx`. §5.
- It does not evaluate a `c:f` formula, resolve a cell reference, or model a sheet.
- It does not add a curve primitive, and it does not approximate one. §7.3.
- It does not draw 3-D, surface, stock, radar, bubble or pie-of-pie charts, and
  it does not draw `chartUserShapes.xml`. §4.3.
- It does not change any published fidelity grade or close any `105` row before a
  chart actually draws. §10.
- It does not touch `casual-doc-model`, `casual-doc-layout`, `casual-doc-render`
  or `webapp/`, which other lanes hold. §8, §9.

---

## 14. What increments 2-4 actually landed (2026-10-02)

Recorded here rather than in a tracker because it is a correction to this
document's own §8 and §9, and because the one thing a reader of this design needs
to know is whether a user can see a chart. **They cannot.** Nothing in this
increment draws anything; `105` FID-R-08 stays open and no grade moves (§10).

### 14.1 The model, and three deviations from §8

`crates/casual-doc-model/src/v1/chart.rs`, `ChartId` in `v1/ids.rs`,
`Definitions::charts` as the side table §8.1 specifies, validation in
`v1/document.rs` (`validate_charts`, `check_chart`), and `ChartId` added to
`Document::visit_node_ids` — the one walk of the document's identities, so a chart
id cannot collide with a body node and cannot be reissued by a reopened
snapshot's generator.

Three things differ from §8.3 and each is deliberate:

1. **`Chart` carries no `id` field.** §8.3 sketched one. No other v1 definition
   value carries its own id — `MediaReference`'s doc comment states the rule, and
   the `FieldRangeId` comment in `v1::ids` is the same argument — so the map key
   is the identity and an id that disagrees with its key is unrepresentable rather
   than merely invalid.
2. **`ChartValue` is adjacently tagged** (`tag = "type", content = "value"`), not
   internally tagged like `Color`. Serde cannot internally tag a newtype variant
   whose payload is a `String`, and it fails at **serialization** time rather than
   at compile time: every chart projection would have made `Document::to_json`
   return `SnapshotError::Serialization`. The snapshot round-trip guard caught it
   on arrival, which is the clearest argument available for writing that guard.
3. **`ChartGroup` gained `vary_colors`.** `c:varyColors` is a child of the chart
   *group* in OOXML, not only of the chart space; §8.3 put it only on `Chart`.
   Both are read.

### 14.2 The reader

`crates/casual-doc-import/src/chart.rs`. The caches are the data; `c:f` is carried
verbatim and the module contains no reference, sheet, range or operator handling
at all; `c:externalData` becomes an `EmbeddedPart` pointer and no workbook is
opened on any path. Declining is a first-class outcome — `read_chart_part` returns
no `Result` — so a malformed, over-bound, out-of-scope or empty part yields no
projection and no import failure.

**The §6.3 `graphicData@uri` fix was already in the tree** when this lane started
(`PendingGraphic::declares`), with two guards in `casual-doc-import/src/tests.rs`.
Its mutation proof is recorded below with the rest rather than skipped.

### 14.3 Per-construct reporting, and what it does to the part row

Built, because with a projection in place the constructs it captures are genuinely
`mapped` and the remainder genuinely `degraded` — the precondition §9 increment 3
sets. Three outcomes, which together replace one line about a part:

| the projection | the report |
| --- | --- |
| captured everything (`Complete`) | **nothing.** `35` makes `mapped` unreachable in a report, so the whole-part `omitted` + `preserved` row is *suppressed* rather than restated. The opaque-part ledger record still exists, so the preservation claim stays auditable. |
| captured some of it (`Partial`) | one `degraded` + `preserved` finding per unmodelled construct, feature `chart.<local name>`, charged to the chart part, each citing the part's own ledger record. The whole-part row is suppressed in favour of them. |
| declined | today's whole-part `omitted` + `preserved` row, unchanged — **plus** one `omitted` + `preserved` finding naming the out-of-scope family, because "we do not model `bar3DChart`" is information the part row cannot carry. Malformed/over-bound/empty add nothing, so they add no row. |

The feature name is qualified (`chart.spPr`, not `spPr`) because `spPr` and
`marker` name constructs in the WordprocessingML drawing vocabulary too, and one
aggregated finding would describe neither. It is a class prefix in the same shape
as `docx.rsid` and `docx.watermark`, not a namespace prefix.

One consequence worth stating plainly: a real Word chart writes `c:spPr` and
`c:txPr` almost everywhere, so **nearly every imported chart will be `Partial`**,
and therefore nearly every imported chart refuses regeneration. That is the
intended conservative answer, not a gap — but it does mean `ChartCoverage::Complete`
will be rare until the projection covers chart formatting.

### 14.4 The export half, and the writer that was deliberately not built

What landed is the **guarantee**, which is what §9 increment 4 calls user-reachable:
`crates/casual-doc-export/tests/chart_retention.rs` now proves that the fixture's
chart *projects* (two groups, three axes, `4.30` verbatim, `Partial`) **and** that
every part of its closure still comes back byte-identical — a byte check alone
passes vacuously once the reader declines, and a projection check alone says
nothing about the output. A second guard proves a chart whose projection is
`Complete` is **still** byte-copied, which is the only input on which a writer
could legitimately have decided to rewrite the part.

**No chart-XML regenerator was written, and this is a decision rather than an
omission.** Three reasons:

1. **Nothing can mark a chart dirty.** There is no dirty bit in the model, and the
   only paths that could set one live in `casual-doc-edit` and `casual-doc-wasm`,
   which other lanes hold. A regenerator would be unreachable by construction —
   `SKILL` §9 rule 4's trap, built on purpose.
2. **§6.1 consequence 1 means only a `Complete` projection could ever be
   regenerated**, and by §14.3 almost no real chart is `Complete`. The function
   would be dead on real files even with a dirty bit.
3. It would be verifiable only by its own inverse (project -> write -> project),
   which is a real property but not evidence that anything works for a user.

The gate it would have to pass already exists and is guarded:
`ChartCoverage::permits_regeneration`.

### 14.5 Still open

- **Increment 5 (the `flow.rs` consumer) is untouched and is the whole of
  reachability.** `FID-R-08` stays open; a chart still renders as the literal text
  `[chart]`.
- `webapp/src/fidelity.js` grades Charts `modeled: "full"`, which was an
  overstatement when no chart model existed and is still an overstatement now that
  a partial one does. §10 says it should read `partial`. `webapp/` is another
  lane's, so this is reported rather than edited.
- A chart inside a header, footer, footnote or comment is **not projected**: those
  part parsers are handed no embedded-relationship index, so a chart there is not
  modelled as a chart node in the first place. The model-side anchor walk already
  covers every container, so projecting one is an extension of
  `chart::build_charts`' walk and nothing else.
- Chart formatting (`c:spPr`, `c:txPr` and the chart-space fill) is reported, not
  modelled, which is what keeps coverage `Partial` on real files.

---

## 15. Increment 7, the engine half (2026-10-04)

§9's increment 7 is *"insert a chart, chart elements, type and style, and data
editing"*, listed against `webapp` + facade lanes. The facade half landed here;
the host half did not, and the reason is recorded rather than implied.

### 15.1 The capability trap that had to be fixed first

Tier 1A and 1B made seven families paint, and **nothing in the product inserted,
selected or edited one**. Worse than nothing: the one affordance a chart did have
advertised itself as live and then errored. Verified by reading, and all four
facts measured in the same session:

| claim the engine published | what the command did |
| --- | --- |
| `canDelete: true` | `NodeNotFound`. `is_object_node` listed `Drawing`, `AnchoredDrawing`, `TextBox` and `Group` and not `EmbeddedObject`, and that predicate is the whole of `DeleteObject`'s target test. |
| `canAltText: true` | `NodeNotFound`. `EmbeddedObject` has no `descr` field, so `SetObjectDescr` can never resolve one. |
| `canCrop: true` | `NodeNotFound`. It has no `srcRect` either. |
| `canResize: false` | …and `SetExtent` had an `EmbeddedObject` arm all along. `object_resize_handles_in_inlines` had no arm for the kind, so the defaulted frame published zero handles — the one operation that worked was the one hidden. |

All four came from one cause: a preview-bearing `EmbeddedObject` is pushed into
the image list by `collect_para_objects`, so it inherited
`ObjectCapabilities::inline_image`. It now gets
`ObjectCapabilities::inline_embedded_object` — resize and delete true, everything
else false because the model carries no field for it.

**A chart that DRAWS was not selectable at all**, which is §9 rule 4 the other way
round: making charts paint moved their painted box from `line.images` to
`line.charts`, and the placement correlation in `resolve_object_boxes` had never
looked at `line.charts`. So the only chart with selection handles was one whose
projection did *not* draw. Correlated now, as `kind: "chart"`.

A note on the old guard `a_chart_with_a_cached_preview_can_be_selected`: it
pinned a state the importer **cannot produce**. `commit_embedded_graphic` passes
`preview: None` for every `EmbeddedKind::Chart` and every `Diagram`, and only
`commit_object` — a `w:object` OLE embedding — ever resolves a preview. The guard
therefore blessed three capabilities, all of which failed, on a document that does
not exist. It is replaced by `an_ole_preview_is_not_offered_a_pictures_capabilities`
over the producible case.

### 15.2 `SetChartDefinition`, and the ordering rule it has to obey

No operation could write `Definitions::charts`, so insertion was never one command
short of working — it had no data path at all. `Operation::SetChartDefinition`
is the retained-value definition write (`SetMediaReference`'s shape), and the
chart table's sidecar keying imposes an order that is the model's rule and not the
operation's:

- a projection is installed **after** its object node exists;
- an object is removed **after** its projection is removed.

`insertChart(node, offset, kind)` therefore applies two operations in one
transaction, and `kind` is checked against **what paints** rather than what the
model represents — a family that is typed but undrawable is refused with a marked
sentence (`chart.unpainted-family`), because inserting one would put the word
`[chart]` on the page and call it success.

The removal half is composed at the `apply_group` choke point rather than inside
`deleteObject`, so every facade path that emits the operation gets it. It covers
the removals that **name** the node they remove; the positional ones do not
cascade and leave the document invalid, which is `109` HF-257 with the measurement
in its Notes cell.

### 15.3 What this deliberately did not do

- **No DOCX chart part is written** (`109` HF-256). Export emits the `c:chart`
  relationship from the retained-parts side table, and an inserted chart has no
  retained bytes, so saving writes a reference to a part the package does not
  contain. The projection is marked `ChartCoverage::Complete`, which is exactly
  what `permits_regeneration` reads, so the writer has its gate waiting for it.
  **Until it lands, no host surface may offer the insert.**
- **No host surface** (`109` HF-258). §10's rule applies to this increment too:
  `insertChart` is reachable from the facade and from nothing a user can touch.
  The chrome owes an `INSERT_SURFACE` entry, a button in `editor.html`'s
  `illustrations` group and a palette row — a seven-item gallery rather than one
  button, because a single Insert-Chart control would have to pick a family for
  the author.
- **No floating charts** (`109` HF-260, and Q-C's answer unchanged). `AnchorContent`
  has no `Chart` variant and `EmbeddedObject` no `anchor` field, so the work starts
  in `casual-doc-model` — the 15-literal field addition that broke `main` twice —
  and the float layer's paint-kind match uses `_ => continue`, so a floating chart
  would be placed, painted and reported by nothing. Decided against for this
  increment: an inline chart that inserts, draws, resizes and deletes is worth more
  than a floating one that cannot be saved.
- **No data editing, no title, no chart-element gestures.** The sample projection
  carries Word's own Insert-Chart numbers and no title, because there is no
  title-editing gesture and two unchangeable words are worse than none.

---

## 16. Increment 8: chart data authoring, end to end (2026-10-08)

The owner, on the inserted chart: *"its static how can i add data … what does
adding static chart achieve"*. §15.3 had recorded "no data editing" as a
deliberate gap; it is closed here across all four layers, because closing any
one of them alone ships the same complaint one step later.

### 16.1 The interaction, from the competitive standard

Word answers "how do I put data in a chart" with *Chart Design ▸ Edit Data* (also
on the right-click menu): a spreadsheet grid, row 1 series names, column A
categories, edits applied as they are made. Docs' chart editor is a right-hand
panel with a type gallery, the data range, the title and the legend. The panel
(`webapp/src/chart_data.mjs`) is both: a right-hand side panel holding the type
gallery, title, legend and Word's grid. Insert ▸ Chart opens it on the new chart
with the first value focused, which is Word's sequence. It is reachable from the
chart's chip, the object right-click menu (and therefore the palette) and a
double-click — four surfaces, `SKILL` §10.

Grid behaviour a spreadsheet user expects and gets: Enter commits and moves
down (and adds a row on the last one), Tab across, arrows at the text edge,
Escape abandons a pending entry and on an untouched cell closes the panel, and a
tab-separated block pasted from Excel or Sheets fills from the cell it lands in,
grows the grid to fit within the model's ceilings, undoes a displayed grouping
comma (`1,200`) and never guesses at a decimal comma.

### 16.2 One write, one undo step

`setChartData` (`casual-doc-wasm/src/chart.rs`) is the only write. It carries the
whole grid, the family, the title and the legend as one `SetChartDefinition`
through `apply_group`, so protection, review mode and the windowed-layout gates
apply and one undo takes one committed cell back. The patch is applied on top
of the existing projection: fills, lines, data labels, axis bounds and number
formats the grid does not show are carried across, not reset.

### 16.3 `Chart::dirty` — §6.1's bit, finally settable

§14.4 point 1 said nothing could mark a chart dirty. The data write now does,
and the exporter honours it: a dirty projection is regenerated even when its
part was retained, and the retained bytes it replaces are superseded — not
written beside it (two ZIP entries or two `Override`s for one part are a package
no two readers agree about; `an_edited_imported_chart_is_regenerated_and_supersedes_its_source_bytes`
was mutation-checked against both). Without this, a chart authored here, saved
and reopened came back as an import and was read-only: the "static chart" defect
one save later. The authoring refusal therefore no longer asks where a chart
came from; it refuses only a `Partial` projection and a combo chart.

### 16.4 The values-only workbook (§5.3)

`casual-doc-export/src/chart_workbook.rs` writes, for every chart the exporter
regenerates, an embedded `.xlsx` holding exactly the cache in Word's layout, and
binds the series to it with `c:f` references (`Sheet1!$B$2:$B$5`). A chart that
already named a workbook keeps the part name and relationship id, so it is
replaced in place and nothing is left pointing at the old one; replacing a
RETAINED workbook is reported as `docx.export.chart.workbook_replaced`
(`DegradedNotRetained`), because the producer's workbook may have held more than
the chart showed. `bind` declines — leaving the chart cache-only exactly as
before — for a combo chart, a range that already names a foreign formula, series
that do not share one category column, or a name collision.

### 16.5 What this still does not do

Series colours and line styles, data labels, axis options and bounds, number
formats, gridlines, chart styles, combo charts, secondary axes, trendlines and
error bars are kept, drawn and saved, and not editable. Most charts Word writes
projected `Partial` at the time of this increment, so they opened read-only —
superseded by §17, which makes them editable. The undo label is passed to `apply_group`
directly ("Chart data") rather than through a `HistoryKind` variant.

---

## 17. Verbatim carry: a Word chart is editable without losing what it holds (2026-10-08)

Measured first: a chart exactly as Word 2016 writes one by default projected
`Partial` on eleven constructs none of which its data depends on — the axis,
legend, plot-area and chart-space `c:spPr`/`c:txPr`, `c:lang`, the style's
`mc:AlternateContent`, `c:crosses`, `c:crossBetween`, `c:auto`, `c:lblAlgn`,
`c:lblOffset`, a group-level `c:dLbls` and an empty `a:effectLst`. `Partial`
forbids regeneration (§6.1), so every chart Word wrote opened read-only.

**The named pattern is round-tripping unknown content**, the reason
`Definitions::format_scheme_xml` exists. `v1::ChartXml { name, xml }` carries an
element the projection does not model verbatim on the container it came from —
`Chart::space_retained`, `chart_retained`, `PlotArea`, `ChartGroup`, `Series`,
`Axis`, `Legend` and `ChartTitle` each have a `retained` list — and the root's
namespace declarations travel in `Chart::namespaces`. Three rules keep it honest:

1. **Schema order lives in the model.** `chart_child_order(ChartContainer)` is
   the ECMA-376 child sequence of each container, per family and per axis kind.
   The importer carries an element only when its container's sequence admits it
   (a bar series' `c:explosion` has no place and stays a loss), and the writer's
   `Carry` puts each fragment back at its rank. An independent copy of the
   sequences in `chart_part_writer.rs` checks the written part.
2. **Shadows.** Where the model holds an element's MEANING but not its
   formatting — a series' `c:spPr` and `c:dLbls`, an axis's gridlines, a title's
   rich text — the verbatim copy is kept only if something inside it went
   unmodelled, and it replaces the generated element while the model says the
   element is present. An edit to the modelled value drops it (a palette drops
   the series `c:spPr`, a new title text drops `c:tx`), and turning gridlines off
   removes them whatever was carried.
3. **What cannot be put back is still a loss.** An element naming a relationship
   (`c:userShapes`) would point at nothing in a regenerated part; an element no
   sequence admits has no position. Both stay `Partial`. A fragment that reaches
   the writer malformed (only possible by snapshot) is dropped and reported
   (`docx.export.chart.fragment_dropped`); the model bounds the total at
   `MAX_CHART_RETAINED_BYTES`.

The import report is unchanged: a carried construct is still named, because it
is still not drawn — "not modelled" and "lost" are now two facts, and only the
second decides coverage. Mutation-checked: fragments written at the wrong rank, a
shadow outliving its model value, no validation, and an importer that carries
nothing each turn a guard red.

## 18. The surfaces, from ONLYOFFICE (2026-10-08)

ONLYOFFICE's document editor gives a selected chart a contextual Chart tab
(`common/main/lib/view/ChartTab.js`: Chart Elements, Edit Data, chart type,
styles, Advanced Settings), a right-hand chart panel
(`documenteditor/main/app/template/ChartSettings.template`) and a data editor in
its own window. This build has the same three, plus the right-click menu and the
palette, all running ONE command tree (`chart_commands.mjs`):

| Surface | Module | What it holds |
| --- | --- | --- |
| Chart tab (contextual, after Table) | `chart_surface.mjs`, `editor.html` `#panelChart` | Edit data, Type ▾, Elements ▾, the style gallery, Settings |
| Chart settings panel | `chart_panel.mjs` | Type, Style, Elements (title, legend, labels, axes, gridlines), axis bounds and order, a data summary |
| Chart Data window | `chart_data.mjs` | the grid; opens on Insert ▸ Chart and on double-click |
| Right-click menu, palette | `object_context_menu.mjs` | the same tree |

The engine side (`casual-doc-wasm/src/chart.rs`): seventeen type tokens
(`CHART_GALLERY` — clustered, stacked and 100% stacked column, bar, line and
area; line with markers; pie; doughnut; scatter, straight and smooth), and an
optional `format` on the one write — title and legend overlay, data labels with
the positions the family admits, each axis's visibility, bounds, order and
gridlines, and a palette. The renderer gained what those controls need so none
is dead: category-axis (vertical) gridlines, and label positions honoured. A
document with no theme now draws a chart's theme colours in Word's default Office
theme instead of black, and the panel's swatches show the document's own theme
colours, resolved by the engine.

Not yet, and named: an individual series' colour and line style, number formats,
chart text fonts, axis titles, combination charts, secondary axes, trendlines and
error bars — all kept, drawn where drawn, and saved.
