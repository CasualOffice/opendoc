# 60 — Fidelity Corpus Rendering and Pagination Audit

**Status:** Implemented fidelity pass with verified residuals.
**Audit date:** 2026-07-27
**Code baseline:** `main@cde11ff`; implementation branch
`agent/core-fidelity-corpus-fixes`.
**Primary scope:** DOCX rendering, pagination, and core layout output.

## Purpose

This audit validates the current engine against the five real documents selected
for the fidelity pass:

- `Class notes.docx`;
- the medical information form;
- the Chinese SDS;
- `Sample Document.docx`;
- `demo.docx`.

It supplements the code-first register in doc 55 with fixture-backed failures
and an executable work order. Import/model coverage, flow geometry, pagination,
page output, and paint are assessed separately. A matching page count alone is
not a fidelity pass.

## Fixture handling and evidence policy

These files are probe inputs, not repository-owned corpus assets. Their exact
bytes were read from repository history after they had been deliberately
untracked. They were extracted only to a temporary directory and were not
restored to `fixtures/`.

Do not recommit these files or derived page images without the rights review and
manifest records required by doc 23. Future CI coverage must use generated,
licensed, or explicitly approved equivalents.

| Probe | SHA-256 |
| --- | --- |
| Class notes | `c4387f07cb5b281a127b47938bb8a4ed35ae4fa8f1b9a52f71543374ade5e6c7` |
| Medical form | `570a906cd660d44d562b8c5e8582f3ee0c3d578f25212b0fde0c42f31bbe5b09` |
| Chinese SDS | `f377b09fa7dc649fac2b8f6df05eee619c33816b2fab49af9d0056e1f44a28d8` |
| Sample Document | `1c8241d1d276000c681b01d8bc0ed339a162ebf9525ae2a7e4712cc7adce4f1d` |
| demo | `269329fc7ae54b3f289b3ac52efde387edc2e566ef9a48d637e841022c7e0eab` |

## Method

The pass used:

1. the exact DOCX bytes identified above;
2. OpenDoc's `paginate_document` and native raster path;
3. LibreOffice PDF output as the available visual oracle;
4. page-by-page visual comparison at 96 DPI;
5. direct OOXML element counts for implicated features;
6. source tracing from importer/model through flow, page output, composition,
   and paint.

The text-fidelity harness remains useful for missing text, but its word-multiset
score is not a visual metric. It does not validate page geometry, headers,
leaders, line boxes, images, or table placement.

## Baseline and post-fix result

| Probe | Initial OpenDoc | Post-fix OpenDoc | LibreOffice | Current result |
| --- | ---: | ---: | ---: | --- |
| Class notes | 1 | 1 | 1 | Strong one-page control remains stable |
| Medical form | 3 | 3 | 4 | Styled checkbox glyphs and row containment fixed; row/pagination parity remains |
| Chinese SDS | 17 | 18 | 16 | Overpaint/box escape fixed; conservative readable line containment exposes residual balancing drift |
| Sample Document | 26 | 26 | 26 | Mixed geometry, TOC, furniture, inline images, and square wrap verified |
| demo | 8 | 8 | 8 | Count remains stable; mixed-object behavior remains incomplete |

The last measured text-only matches were 100% Class notes, 89% medical, 99%
Chinese SDS, 97% Sample Document, and 94% demo. These are diagnostic numbers,
not release claims. Sample Document has the correct page count while its TOC,
later-section furniture, mixed surface, inline images, and float wrapping are
now verified. Equations remain visibly absent; notes now have bounded reference,
footnote-band, continuation, and endnote-append support, with separator and full
section-policy fidelity still open.

## Fixture feature inventory

Counts are elements in `word/document.xml`, not package-wide counts.

| Construct | Class | Medical | Chinese SDS | Sample | demo |
| --- | ---: | ---: | ---: | ---: | ---: |
| `w:ptab` | 0 | 0 | 0 | 72 | 0 |
| `w:sdt` | 0 | 8 | 0 | 17 | 0 |
| `w14:checkbox` | 0 | 8 | 0 | 4 | 0 |
| `w:drawing` | 0 | 2 | 2 | 11 | 4 |
| `wp:inline` | 0 | 0 | 0 | 10 | 1 |
| `wp:anchor` | 0 | 2 | 2 | 1 | 3 |
| `v:textbox` | 0 | 5 | 3 | 0 | 0 |
| `v:group` | 0 | 3 | 3 | 0 | 0 |
| `m:oMath` | 0 | 0 | 0 | 3 | 0 |
| Footnote references | 0 | 0 | 0 | 3 | 1 |
| Endnote references | 0 | 0 | 0 | 2 | 1 |
| `w:tbl` | 0 | 10 | 2 | 15 | 6 |
| `w:trHeight` | 0 | 50 | 7 | 0 | 9 |
| Run character scale `w:w` | 0 | 0 | 3,672 | 0 | 0 |

The probes are complementary:

- Class notes is a stable simple-page control;
- the medical form stresses SDTs, checkboxes, form rows, tables, and legacy
  shapes;
- the Chinese SDS stresses CJK, run scaling, exact lines, VML groups/text boxes,
  columns, and pagination;
- Sample Document is the broad element and section test;
- demo supplies another image/table/note path.

## Verified findings

### 1. Sample page 23 has a different page size — implemented

Verified. LibreOffice reports:

- pages 22 and 24: 612 × 792 pt, portrait Letter;
- page 23: 792 × 612 pt, landscape Letter.

OpenDoc flows page 23 using the landscape section width, but the native
`render_gallery` example allocates every output surface from
`document_page_config(document)`, which returns the first section's portrait
size. `Page` carries a `SectionId` and `content_area`, but no `page_size`.
Consequently the right side of the landscape table is clipped by a portrait
surface.

Some document-owning façades can resolve the section id back to the source
document as a workaround. The immutable core layout output is still incomplete:
a renderer, PDF exporter, page cache, or serialized consumer should not need the
source document to discover a page's physical box.

`Page::page_size` now carries the owning section's physical box. Native render
consumers allocate from the selected page rather than the first section. The
incremental geometry key also includes page size, so a geometry change cannot
reuse incompatible pages.

**Acceptance:** Sample pages 22/23/24 report portrait/landscape/portrait sizes;
page 23's final table column is visible; paint, hit testing, and page stacking
use the same dimensions.

### 2. Sample TOC loses spacing, leaders, and right alignment — implemented

Verified on pages 2–3. Each of the 72 TOC rows contains:

```xml
<w:ptab w:alignment="right"
        w:relativeTo="margin"
        w:leader="dot"/>
```

Import correctly models these nodes. `flow.rs::collect_items` drops
`InlineNode::PositionalTab`, so the heading and cached page number concatenate,
the dots disappear, and page numbers are not aligned to the right margin.

Positional tabs are now first-class flow items. Margin/indent reference frames,
left/center/right placement, and the five leader forms are resolved through the
tab layer. Sample's 72 right/margin/dot entries render with aligned page numbers
and continuous dot leaders.

The same layer now keeps ordinary tab stops in their authored margin coordinate
space before translating them into the paragraph's indent-local line box. This
preserves negative hanging labels in SDS form rows. When a trailing tabbed value
is wider than the remaining measure, it soft-wraps at that value column rather
than painting through the page edge; the eye-protection row on SDS page 5 is the
acceptance case.

**Acceptance:** all TOC page numbers share the authored right stop, dotted
leaders fill only the gap, long entries remain bounded, and Sample remains 26
pages.

### 3. Sample has no headers or footers — implemented

Verified. The document has five sections. The cover section has no running
content; each of the four later sections has its own default header and footer.
The driver calls `build_running_content` using `defs.sections.first()` and
applies that one result to the whole document. Because the first section is
empty, all later furniture is absent.

This also explains why adding one global footer would be wrong: the two-column,
landscape, and final-summary sections have different source parts and text.

A section-keyed running-content plan now resolves inherited references, flows
each section at its own width, reserves that section's bands, and applies
section-local first-page state. Literal U+0009 tabs emitted inside `w:t` by some
producers are canonicalized to semantic tab nodes, which also fixes opposing-edge
header/footer alignment and avoids `.notdef` boxes.

**Acceptance:** the normal section shows `docx-editor.dev` and `Page X of 26`;
the multi-column, landscape, and final sections show their own authored
furniture; reservation changes are deterministic and reviewed.

### 4. Inline images are painted, but not laid out inline — implemented

Verified on Sample page 13. Four colored `wp:inline` squares should sit inside
one sentence. OpenDoc emits each as its own line, with surrounding words stacked
above and below.

The behavior is explicit in `shape_paragraph_items`: a `FlowItem::Image` becomes
`image_line`, while text before and after it is shaped as separate chunks. This
prevents text/image co-measurement and distorts paragraph height, cell height,
wrapping, and pagination.

Parley 0.11 exposes in-flow inline boxes. The design should use that facility or
an equivalent unified atom stream rather than positioning images after unrelated
text shaping.

The line-shaper seam accepts image boxes at paragraph byte boundaries and the
Parley implementation uses native in-flow boxes. Sample page 13's four colored
squares now share the sentence line; tall boxes contribute line height and
wrapping. The uncommon field/tab-plus-image combination retains its conservative
standalone fallback.

**Acceptance:** Sample page 13's colored squares share the authored line and
baseline; an image at a wrap boundary moves with adjacent content; a tall image
grows its line; inline images in table cells affect row height and intrinsic
width.

### 5. Floating-image text exclusion is incomplete — bounded square support implemented

Sample page 13 also has one `wp:anchor` using square wrapping. OpenDoc paints the
float, but square/tight/through exclusion is not applied to surrounding lines.
This permits text/image intersection in Sample and other image-heavy documents.

This is separate from true inline-image layout. It needs the line-interval
exclusion design already identified in docs 51 and 55.

Paragraph/line-relative, margin/column-aligned left and right
square/tight/through anchors now create non-painting inline float markers. The
resumable line breaker narrows only intersecting lines by the object extent plus
wrap distances and restores full measure below it. Page-relative,
cross-paragraph, centered, and contour-accurate wrapping remain explicit
residuals.

### 6. Chinese SDS content exceeds its authored box — containment implemented; pagination residual

Verified on page 1 in the hazard-description VML group. The long CJK sentence
extends beyond its text-box boundary. The document applies run character scale
`w:w` extensively—3,672 occurrences—but `RunProperties` does not model it, so
the shaper uses unscaled advances and paint.

The document also has 301 `lineRule="exact"` paragraphs. The engine stores the
exact line-box height but deliberately allows glyphs to extend outside the box
without a line clip. Tight source metrics can therefore become visible
text-to-text or row-to-row overpaint.

Run character scale `w:w` is now modeled, validated, imported, cascaded,
line-broken, painted with matching horizontal outline scale, and exported.
Exact-height lines clip in composition and, critically, their glyph baselines
are re-anchored into the authored box on ordinary, tabbed, and fielded paths;
this removes the former clipped-half-line and successor-row overpaint. Dense
CJK auto spacing keeps at least one em of vertical pitch.

The page-1 hazard text is contained and later SDS pages no longer show text
fragments crossing rows. Hanging tab rows keep their label/value geometry and
long values remain inside the right page margin. The conservative readable
result is 18 pages versus LibreOffice's 16, so final balancing/font/grid parity
remains open and must not be represented as complete pagination fidelity.

### 7. Character and row collisions — local gates implemented; corpus validator remains

Sample page 21 intentionally exercises more than ten adjacent runs, including
large/small text, super/subscript, highlighting, underline, strike, and multiple
fonts. The source itself has deliberately tight adjacency, so comparison must
distinguish source-authored contact from an engine collision. Other reported
collisions can arise from exact line boxes, missing baseline-shift ink extents,
inline boxes excluded from row measurement, or independent floats.

Current tests assert many individual heights, but there is no corpus-level
validator for:

- monotonically stacked paragraph line boxes;
- glyph ink escaping an unclipped line;
- non-floating table rows intersecting a successor row;
- inline boxes omitted from line and row occupied height;
- wrapped text intersecting a float exclusion.

Synthetic regressions now cover exact-line clipping and baseline containment,
exact-row clipping, dense scaled-run pitch, inline image occupied height, and
square-float line exclusion. A general corpus diagnostic that reports ink and
box intersections by page/node is still required.

### 8. Other Sample and form gaps remain visible

The exact Sample fixture confirms:

- all three OMML equations are invisible;
- note reference marks and common footnote/endnote bodies now render through the
  bounded note-pagination path; separator customization and full per-section note
  policy remain open;
- native checked SDT checkboxes do not show their checked presentation;
- floating tables remain inline approximations;
- the two-column section and separator are already strong;
- the main SDT content flow is already strong.

The medical form confirms that block SDTs and common table content render.
`w:sym` now retains its owning run properties across import/model/layout/export,
so the unchecked form glyph uses the authored size and color. Vertical/table
geometry still under-paginates the four-page oracle into three pages.

These remain separate rows. An SDT container rendering correctly does not mean
every SDT control type is visually supported.

None of the five probes in this pass contain an EMF/WMF (or other
PNG/JPEG-unsupported) image, so this audit's page-by-page comparisons are
unaffected either way. Noted here for completeness: a later, separate PR
(`agent/fix-emf-wmf-placeholder`, after this audit's baseline) changed what
`casual-doc-render` does when an inline image's bytes are present but not
decodable by its PNG/JPEG-only path (doc 55 §8) — it now paints a visible
placeholder box in the image's rect instead of leaving a blank gap. Real
EMF/WMF vector-metafile decoding is still not implemented; that remains
`P1F-28`'s follow-up.

## Fixes completed in this pass

The combined implementation contains:

1. **Streamed XML references.** `quick-xml` emits entities such as `&amp;` and
   numeric references as `Event::GeneralRef`. Body text, field instructions,
   OMML fallback text, anchor-axis text, and document properties previously
   ignored those events. A strict shared decoder now preserves predefined and
   numeric XML references and rejects malformed/undeclared ones. Sample headings
   containing `&` now render correctly.
2. **Complex-script segmenter data.** The layout dependency now enables Parley's
   `complex-scripts` feature. This removed 2,150 repeated ICU missing-model
   warnings from one successful Chinese SDS render and six from Sample without
   changing page counts.
3. **Page-local geometry and section-local running content.**
4. **Positional tabs, literal-tab canonicalization, hanging-tab coordinates, and
   bounded trailing-value soft wrap.**
5. **True inline images and bounded square-family float exclusion.**
6. **Run character scaling and exact-line baseline/clip containment.**
7. **Run-formatted symbol fidelity for medical-form controls.**

The final gate counts and commands are recorded in the pull request. Restricted
fixture images remain local evidence and are not committed.

## Remaining work order

1. **Corpus collision diagnostics** — report glyph ink, line/row clips, and float
   intersections by page/node.
2. **Residual pagination** — medical form row/grid parity and SDS font/grid/final
   balancing without global margin or font-size tuning.
3. **Cross-paragraph float reflow** — page-relative exclusions and convergence.
4. **Contour wrapping** — tight/through contour intervals beyond square bounds.
5. **Visible inline-content floor** — note marks, OMML text fallback, special
   hyphens, then note-body fixed-point pagination.
6. **Control appearance** — native checked-state presentation beyond the
   preserved cached symbol.

Do not tune global margins or line height merely to force the oracle page count.
Every page-count change must be attributable to a modeled geometry rule and keep
page-local collision and containment invariants green.

## Required regression set

The restricted probes may continue as local evidence, but CI needs generated or
approved equivalents for:

- portrait → landscape → portrait sections;
- a two-page TOC using right/dot/margin `w:ptab`;
- an ordinary hanging-indent form row whose trailing tabbed value wraps;
- first-section-empty plus distinct later-section headers/footers;
- text–inline-image–text on one line, including a table cell;
- square-wrapped image with text above, beside, and below;
- CJK `w:w` character scaling inside a bounded text box;
- exact line spacing with CJK, super/subscript, and an explicit clip expectation;
- auto-height and exact-height table rows with inline images;
- checked and unchecked SDT controls;
- note separator/continuation-separator paint and full per-section note policy.

Each golden must record page dimensions, placed geometry, display-list
primitives, page count, font set, and a reviewed reference image.

## Decisions applied

1. Physical size is stored directly on immutable `Page`; section identity
   remains available for richer geometry lookup.
2. Each produced page records the section whose flow created it; continuous
   sections sharing a page retain per-fragment section geometry while the
   physical page keeps its creating section's box.
3. `lineRule="exact"` uses Word-like clipping with baselines re-anchored inside
   the exact box.
4. Parley's in-flow/custom-out-of-flow boxes are the primary seam for inline
   images and bounded paragraph-local float exclusion.
