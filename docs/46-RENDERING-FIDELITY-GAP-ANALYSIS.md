# 46 — Rendering Fidelity Gap Analysis & Roadmap

## Purpose

The CPU renders of real documents are far below Word-grade (owner assessment:
"<10%"). This document is the evidence-based diagnosis and the prioritized fix
roadmap. It is produced from five parallel analysis lanes, each rendering the
corpus in **both LibreOffice** (the layout **oracle** for pagination/placement —
NOT for borders/decorations, which LibreOffice over-draws) **and ours**, and
diagnosing from the code. Measurement of progress is **page-count + visual
convergence vs LibreOffice**, per document.

## The one finding behind almost everything

**Import is good; the layout layer doesn't consume the data it's given.** The
importer models and preserves the style tables, `basedOn` chains, `docDefaults`,
cell margins, `vAlign`, `lineRule`, drawing anchors, VML, etc. — but `flow.rs`
resolves only a run/paragraph's **direct** properties and skips whole rendering
paths. So the fixes are mostly in *consumption*, and localized, not in the model.

Two structural holes cause the bulk of the "<10%":
1. **No effective-property cascade.** Layout reads direct props only; it never
   walks `direct → char-style → paragraph-style(basedOn) → docDefaults`. Grep for
   `style_ref`/`based_on`/`doc_defaults` in `crates/casual-doc-layout` returns
   nothing. In `Class notes.docx` **100% of runs** get their size/font/color from
   styles → we render the whole document as flat 11pt Roboto black. Body text is
   only right *by luck* (these docs' `docDefaults` happen to be 11pt).
2. **Missing rendering paths + missing box-model axes.** No floating-object layer
   with real z-order (groups, floating text boxes), no VML layout, no cell
   margins/`vAlign`, no `lineRule`, no empty-paragraph height, no page background.

## Ranked gap inventory (evidence-backed)

Grouped by the fix that closes them. `file:line` in each lane's analysis.

### F1 — Effective-property cascade (P0, dominant)  [in progress]
Layout must compute effective run + paragraph properties via the cascade before
flowing. Closes at once: **font size** (Title 36pt→11pt today), **font family**
(→ Roboto instead of the doc's/theme-minor face), **bold/italic** (headings
non-bold), **color** (teal Title → black), **char spacing**, and paragraph
**spacing before/after**. Evidence: `Class notes` 100% / `demo` 65% / `Sample`
29% of runs have no direct `w:sz`. Files: `flow.rs` `styled_run:1699`,
`run_metrics:1728`, `resolve_font:1960`, `run_color:1844`, `box_metrics:2047`;
model already has `Style.based_on`, `DocumentDefaults`, `style_ref`.

### F2 — Line/paragraph box model (P0)  [in progress, with F1]
- **`w:lineRule`** (auto/atLeast/exact) not modeled — `Spacing` has only
  `line_percent`. The SDS has **301 `lineRule="exact"`** paragraphs, all dropped
  → we substitute tall CJK-fallback natural heights → over-paginate (20 vs 16).
- **Empty paragraphs collapse to 0 height** — Word gives a blank paragraph a full
  line at the paragraph-mark's font. Docs use blank lines for spacing → we
  under-paginate & look cramped.
- **contextualSpacing** modeled, never applied; **autospacing** not modeled;
  default line multiplier (1.08–1.15×) + 8pt-after not applied.

### F3 — Table cell margins + vertical alignment (P0)
- **Cell margins** (`w:tcMar`/`tblCellMar`, default L/R ~108 twips) never applied
  → text hugs the grid line and cell top ("cramped"). Also under-measures row
  height.
- **`w:vAlign`** (top/center/bottom) never applied — `CellFragment` has no vAlign
  field; 122 cells in the medical form specify it → labels float to row tops
  instead of sitting on the answer line. Data present; `flow_table`/`compose`
  drop it. Files: `flow.rs flow_table:469`, `block.rs CellFragment:119`,
  `compose.rs:196`.

### F4 — Floating-object layer with real z-order (P0/Critical)
Replace the binary `behindDoc` split with a single z-ordered float layer over
**body and header/footer**. Closes:
- **DrawingML groups (`wpg`)** — currently collapsed to the first image sized to
  the *group* extent (stretched logo); sibling text boxes shoved inline;
  rectangles/connectors dropped. Word paints children in doc order → the reported
  "one box behind the image, one in front."
- **Floating text boxes** — modeled inline-only (no anchor/extent/z/fill/border).
- **Z-order** — no `relativeHeight`, no per-shape/group z.
- **Anchored objects in headers/footers** — the placement pass walks the body
  only → header floats vanish (the SDS's live in `header1.xml`).
Files: `anchor.rs collect:53/place:138/resolve:232`, `flow.rs textbox_item:1012`,
`compose.rs z-order:135`, model `body.rs TextBox:667`, import `commit_drawing:3083`.

### F5 — VML shape layout + paint (High) — DONE (P1F-VML, `feat/vml-paint`)
VML (`v:rect`/`v:line`/`v:oval`/`v:shape` + `v:imagedata` + `v:textbox`) was
modeled only as inline image/textbox; positioned VML shapes/lines were dropped
entirely, so VML-primary docs (SDS: 32 `w:pict` + 8 `v:shape` — rules, callouts,
text boxes) rendered empty of graphics. **Resolved:** `commit_pict` re-parses each
`w:pict`'s raw XML via `parse_vml_pict` (P1F-VML) and maps every `VmlDrawing` onto
the F4 float layer — `v:rect`/`roundrect`/`oval` → anchored `GroupShape`
(group-of-one), `v:line` → `Line` float, `v:imagedata` → positioned
`AnchoredDrawing` (shared media index), `v:textbox` → floating `TextBox` (blocks
flowed through the shared block pipeline); `style` left/top/width/height →
EMU anchor offsets/extent, `z-index<0` → `behindDoc` + monotonic `relativeHeight`.
Header/footer VML rides F4's band walk. SDS page 0 now shows the horizon rules,
callout-box outline, right-positioned header date/title text boxes, and footer
bar (vs LibreOffice); 16 pages preserved. **Deferred:** exact `v:shape` path
geometry (approximated by its bounding-box OUTLINE), gradient fills,
anti-diagonal `v:line` orientation.

### F6a — Header/footer band nesting (P0, systematic over-pagination)
`PageConfig::content_area()` (`paginate.rs:63`) subtracts measured header + footer
height **on top of** the full top/bottom margins. Word *nests* the header inside
the top margin: `body_top = max(margin_top, header_dist + header_height)` (symmetric
for footer). We lose `header_height + footer_height` of body area on **every page
of every header/footer doc** → SDS over-paginates +4. Root also in model:
`PageMargins` (`definitions.rs:288`) never captures the `w:header`/`w:footer`
distances, so layout *can't* apply the rule. Fix: model the two distances, then
change `content_area()` to the `max()` nesting. Small, localized, corpus-wide.

### F6b — Lay out block-level SDT + AltChunk (P0, systematic under-pagination)
`BlockNode::Sdt => {}` and `BlockNode::AltChunk => {}` drop the entire subtree at
zero height (`flow.rs:253/446/816`). Sample wraps its **whole TOC** (`TOC \h \o`,
~35 entries) plus 12 form-control paragraphs in block SDTs → ≈ −2 pages dropped.
The node already carries its `blocks`; recurse through `flow_blocks`. Reserve or
lay out `AltChunk` similarly. NOTE: break primitives (widow/orphan, keepNext,
keepLines, cantSplit, header-row repeat) are all already implemented in
`paginate.rs` — divergence is not from missing break rules.

### F6c — Remaining content completeness (P1–P2)
Model-but-don't-lay-out (zero height → under-pagination + visible loss):
footnote/endnote **body** placement (`paginate.rs:1048` never filled; no note
pass) · inline OMML **math** (display eqns = full lines) · **symbols** `w:sym`
glyphs (checkboxes; Medical 8, Sample 4) · **EmbeddedObject** chart/SmartArt/OLE
(blank box) · complex field results · NoBreakHyphen/SoftHyphen/PositionalTab.
Page background (`w:background`) → white pages. **Multi-column sections**
(`SectionColumns` modeled at `definitions.rs:300`, layout flows full-width single
column — SDS is 2-col ×15): the top *content-on-which-page* spatial gap,
independent of page counts.

### F7 — Appearance details (P1–P2)
Per-script font slots (eastAsia/cs — CJK uses wrong font) · underline
style/color, double-strike, emphasis marks, outline/shadow · theme tint/shade on
runs · text wrapping around floats (square/tight/…) · header/footer PAGE fields
show cached values · non-metric-compatible fonts (Arial/Times → Roboto) · CJK
line-break quality · drop caps/framePr · cell spacing.

## Focused table, text-box, and object audit (2026-07-27)

This follow-up traces each feature through
`model → import/export → flow/anchor placement → composition`. It corrects the
coarser claims above: cell margins, vertical alignment, and the float layer have
landed, but several modeled properties are still not consumed.

### Table findings

| Surface | Current behavior | Status / next slice |
| --- | --- | --- |
| Width, grid span, fixed/autofit, indent | Consumed by the width solver. | Implemented for horizontal spans. |
| Cell margins and vertical alignment | Resolved in flow and applied in composition. | Implemented. |
| Table/cell shading | Cell fill is painted; table-level `w:shd` now supplies the fallback when a cell has no overriding fill. | Implemented by `P1F-TBL-TOPO`. |
| Horizontal border topology | Perimeter vs `insideH` is selected by row position; conflicts inspect abutting cells above/below, including differing grid-span partitions. | Implemented by `P1F-TBL-TOPO`; segmented span edges remain below. |
| Border conflict details | `nil` suppression and first-in-reading-order ties are handled by the topology slice, but the rank covers only a few styles and uses an approximate luminance tie-break. Non-zero cell spacing needs a different conflict mode. | Follow-up after topology; keep the limitation explicit. |
| Border appearance | Border color and width reach paint, but the style token does not: double/dashed/dotted/art borders are all painted as a single solid edge. | **High:** extend the resolved-edge/display-list representation before adding style-specific paint. |
| Vertical merges | The vertical-merge slice resolves conforming restart/continuation runs by exact grid range, gives the restart merged-height/content ownership, and keeps the group page-local. Malformed continuations remain visible ordinary cells. | Implemented by `P1F-TBL-VMERGE`; differently styled side segments remain part of the segmented-border follow-up. |
| Table style and conditional formatting | `style_ref`, `tblLook`, and row/cell `cnfStyle` reach the model but the layout cascade has no table-style layer. | **High:** resolve style defaults and conditional regions before sizing/paint. |
| Alignment, bidi, and row alignment | `w:jc`, `w:bidiVisual`, and row `w:jc` are modeled but ignored by flow. | **High:** define physical start/end mapping before implementation. |
| Cell spacing | Table/row spacing is modeled but neither cell geometry nor border conflict mode consumes it. | **High:** needs gap geometry plus separately visible table/cell borders. |
| Floating tables | `tblpPr`, overlap, and from-text distances are modeled but tables remain inline. | **High/design required:** integrate table boxes with float placement and wrapping. |
| Cell text behavior | `noWrap`, vertical `textDirection`, `fitText`, and `hideMark` are modeled but ignored. | Medium/high, split into bounded layout slices. |

`CellFragment` currently stores one border per whole edge. When a grid-spanning
cell abuts several cells whose borders differ, Word can paint independently
styled edge segments. Correct support therefore needs a segmented-edge
representation; choosing one winner for the full span is only an interim
topology fallback and must not be presented as complete fidelity.

### Text boxes, drawings, and embedded objects

| Surface | Current behavior | Status / next slice |
| --- | --- | --- |
| Inline pictures | Media and extent flow to an image paint item. | Implemented for supported raster media. |
| Floating pictures | Anchor position and z-order are placed; wrap polygons/modes do not reflow body text. | Partial. |
| DrawingML inline text boxes | Import captures shape fill/stroke/extent, then `commit_shape` discards them for the inline form. Flow substitutes a black 1px border, no fill, fixed inset, and content-derived size. | **Critical correctness:** retain and consume authored appearance/extent. |
| Floating/group text boxes | Fill and border color survive; stroke width is discarded and composition always paints 1px. `bodyPr` insets, vertical alignment, rotation, and autofit are not modeled. | **High:** preserve stroke width first; design the text-box box model separately. |
| Nested block content | Paragraphs, SDTs, nested tables, and inline images use the shared block flow. | Implemented, subject to the placement defects below. |
| Floats nested in table cells | Source traversal finds them; the nested-float slice adds fragment-tree paragraph lookup that mirrors composed cell geometry instead of falling back to page 0. | Implemented by `P1F-NESTED-FLOATS`. |
| Floats in header/footer tables | Selected running-content bands are recursively walked through table cells and nested blocks, so their floats repeat on the correct pages. | Implemented by `P1F-NESTED-FLOATS`. |
| Multi-section anchors | Placement uses one page geometry; later-section page/margin frames can be wrong. | High; requires per-page section geometry. |
| Shape geometry | Lines are distinct; ellipse, round-rectangle, and other presets collapse to a rectangular box. Standalone non-text shapes may be reported and dropped on import. | High; add first-class preset/path paint geometry. |
| Text-box overflow | Nested content is not clipped to an explicit text-box extent. | Medium; define overflow/autofit policy before clipping. |
| Embedded object previews | Chart/SmartArt/OLE metadata, preview media, and extent are modeled/imported/exported, but `collect_items` has no `EmbeddedObject` arm, so even safe previews are invisible. | **High, bounded:** render an available preview as a non-executable image; keep the object opaque. |
| Other inline leaves | Symbol, math fallback, no-break/soft hyphen, and positional tabs are modeled but absent or incomplete in flow. | Covered by F6c. |

### Controlled implementation order

1. `P1F-TBL-TOPO`: table perimeter/interior border topology, adjacent-row
   conflicts, and table-shading fallback.
2. Preserve inline/floating text-box appearance and stroke width without
   changing anchor semantics.
3. Render safe embedded-object previews through the existing image path.
4. Make anchor lookup recurse through table fragments in body and
   header/footer bands.
5. Design the larger table-style/spacing/bidi/floating-table and text-box
   `bodyPr` slices before implementation.

## Execution roadmap (controlled sequence, not parallel free-for-all)

Order chosen for maximum fidelity-per-fix and to avoid `flow.rs` merge churn:

1. **F1 cascade + F2 box model** — the single biggest jump (fonts/sizes/colors +
   correct line/para heights). *In progress.* Verify with a re-rendered
   `Class notes` (real typography) + SDS page count toward 16.
2. **F3 table cell margins + vAlign** — fixes the cramped tables.
3. **F6a header/footer band nesting + F6b block-SDT layout** — the two
   localized pagination fixes; directly pull SDS toward 16 and Sample toward 26.
4. **F3 table cell margins + vAlign** — fixes the cramped tables.
5. **F4 floating-object layer + z-order** — fixes logo/group/text-box placement
   and layering, header floats.
6. **F5 VML** — the SDS vanishing graphics.
7. **F6c content completeness** (page background + multi-column first — cheap;
   then footnotes, math, symbols).
8. **F7 appearance details.**

Each step lands with **before/after page counts vs LibreOffice** and a re-render
of the relevant corpus doc as the proof point — no adjectives.

## Corpus baseline (page count: LibreOffice = oracle, vs ours, 2026-07-26)

| Doc | LibreOffice | Ours | direction |
| --- | --- | --- | --- |
| Medical form | 4 | 3 | under (spacing/margins) |
| Chinese SDS | 16 | 20 | over (`lineRule=exact` dropped) |
| calibre demo | 8 | 5 | under (spacing + dropped content) |
| Sample Document | 26 | 22 | under (style spacing + TOC) |
| Class notes | 1 | 1 | ok (single page) |

Convergence of this table toward the LibreOffice column is the fidelity metric.
