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

### F5 — VML shape layout + paint (High)
VML (`v:rect`/`v:line`/`v:oval`/`v:shape` + `v:imagedata` + `v:textbox`) is
modeled only as inline image/textbox; positioned VML shapes/lines are dropped
entirely. VML-primary docs (SDS: 32 `w:pict` + 8 `v:shape` — rules, callouts,
images) render empty of graphics. Model VML shapes as positioned objects (parse
`style` left/top/width/height/z-index + fill/stroke) and paint via the F4 layer.

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
