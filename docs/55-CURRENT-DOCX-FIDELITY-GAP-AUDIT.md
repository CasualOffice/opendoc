# 55 — Current DOCX Fidelity Gap Audit

**Status:** Current-state audit; implementation designs remain separate.
**Audit date:** 2026-07-27
**Code baseline:** `main@cde11ff` (through merged PR #169)

The exact real-document validation and updated core-engine ordering are recorded
in `60-FIDELITY-CORPUS-RENDERING-AUDIT.md`. That pass adds mixed page-surface
geometry, true inline boxes, run character scale, and collision containment to
the register below.

## Purpose

This document reconciles the DOCX fidelity register against the current code
after the rendering work recorded in docs 46–54. It answers four questions:

1. what is now rendered with meaningful fidelity;
2. what is modeled or preserved but still invisible or visually degraded;
3. which limitations also affect headers, footers, table cells, text boxes, and
   other nested content;
4. which bounded pull requests should close the remaining gaps first.

This is a current implementation audit, not a claim of complete ECMA-376
coverage. Docs 44 and 46 retain the historical diagnosis and rationale. When
their old status snapshots disagree with this document, this document is the
newer baseline.

## Method and terminology

The audit traced the current paths from:

`package admission → import → model → semantic export → layout → display list → raster`

The evidence is the implementation and its explicit deferrals, chiefly:

- `crates/casual-doc-import/src/body.rs`, `properties.rs`, `vml.rs`, and
  `numbering.rs`;
- `crates/casual-doc-model/src/v1/`;
- `crates/casual-doc-layout/src/flow.rs`, `cascade.rs`, `columns.rs`,
  `document_layout.rs`, `running.rs`, `anchor.rs`, and `paginate.rs`;
- `crates/casual-doc-render/src/lib.rs`;
- `crates/casual-doc-import/src/retain.rs` and `opaque.rs`;
- `crates/casual-doc-export/src/semantic.rs`.

The following terms are deliberately distinct:

- **modeled** — represented in the normalized document;
- **preserved** — retained safely for a later save, possibly as opaque bytes;
- **laid out** — contributes geometry, wrapping, or pagination;
- **painted** — produces visible display-list and backend output;
- **semantically fixed-pointed** — import → write → reopen reproduces the modeled
  value, which does not imply byte-exact source XML;
- **render omission** — the data survives but contributes no visible output;
- **semantic degradation** — source detail is intentionally flattened or
  regenerated at lower fidelity.

## Current capability floor

The following are materially implemented and should not be listed as open gaps:

- bounded OPC admission, relationship-based main-document discovery, parser
  limits, deterministic compatibility reporting, and no-edit byte retention;
- semantic import/export fixed points for the broad modeled surface, plus an
  opaque side table for admitted unconsumed package parts;
- document defaults, paragraph/character style chains, direct paragraph/run
  formatting, common spacing and indentation, tabs, breaks, list markers,
  visibility, caps, super/subscript, highlight, and common decorations;
- line shaping, pagination, keep/widow/orphan controls, incremental page reuse,
  hit testing, and CPU glyph paint;
- PNG/JPEG picture paint, floating-object z-order, alt text on floats, group
  transforms, nested float discovery, and true same-line inline-picture boxes;
- body, table-cell, header, footer, text-box, and content-control block flow
  through shared recursive paths;
- local paragraph/line-relative `topAndBottom` float barriers, including nested
  table cells and repeated running bands, plus bounded left/right square-family
  line exclusion for explicit paragraph/line-relative floats;
- table grid/width solving, spans, common cell margins, vertical alignment,
  shading, common segmented borders, row splitting, repeated page headers,
  vertical merges, and exact-height clipping;
- multi-section and multi-column first-cut pagination, explicit unequal column
  geometry, separator rules, page-local physical size, and per-section running
  content selection/reservation;
- DrawingML text-box extent, fill, outline, insets, vertical anchor, overflow
  policy, shape autofit, and normal-autofit scaling;
- VML pictures, common shapes, lines, groups, horizontal rules, positioning
  metadata, wrap distances, text-box insets/vertical anchor/autofit, and the
  bounded safe body-float policy from doc 54;
- `PAGE` and `NUMPAGES` recomputation in body content, headers/footers, inline
  text boxes, and anchored text boxes;
- positional tabs (including TOC dot leaders), literal tabs in `w:t`, formatted
  symbols, run character scale, and exact-line paint containment;
- theme color resolution, system/host font fallback, and CJK metric/advance
  corrections on the supported host path;
- page background paint when the host creates the surface with the document
  background.

These capabilities are a strong base, but several remaining gaps are structural:
the model is broader than layout consumption, and some pagination facilities
still use bounded approximations.

## Priority summary

| Priority | Gap | User-visible consequence | Primary evidence |
| --- | --- | --- | --- |
| P0 | Modeled inline leaves are omitted by layout | Notes, object previews, math fallback, and special hyphens can be invisible | `flow.rs::collect_items` catch-all |
| P0 | `altChunk` occupies no layout space | Referenced HTML/RTF/text/sub-document content is preserved but absent from render | `flow.rs` `BlockNode::AltChunk(_) => {}` |
| P0 | Footnote/endnote placement is absent | Reference marks and note bodies do not render or reserve page space | `paginate.rs::build_page`; `Page::footnotes` |
| P1 | Square-family exclusion is only local/bounded | Cross-paragraph, page-relative, contour, and overlapping-float cases can still diverge | `flow.rs::shape_with_float_exclusions` |
| P1 | Table style cascade and advanced table geometry are not consumed | Styled tables lose conditional fills/borders/fonts; floating/bidi/spaced tables become inline approximations | `cascade.rs`; `flow.rs::flow_table` |
| P1 | Paragraph base direction and per-script run slots are not selected | Arabic/Hebrew/mixed-script layout and East Asian/complex-script typography can use wrong direction, face, weight, or size | `flow.rs::line_constraints`, `requested_family` |
| P1 | Inline VML images discard their CSS size | A valid inline VML image can disappear because its model extent is `None` | `body.rs::vml_segment`; `flow.rs::image_item` |
| P2 | Shape geometry is reduced at paint | Ellipse/round-rect/preset/path shapes can paint as rectangles | `anchor.rs::place_group_children` |
| P2 | Review semantics have no view policy | Insertions and deletions both render; comments have no visible anchor/UI | `flow.rs::collect_items` |
| P2 | Section/page-furniture long tail is not consumed | Page borders, line numbers, note policy, section text direction, and parity starts diverge | model `SectionBoundary`; layout search |
| P2 | Field evaluation is limited | Non-page fields rely on cached results and long fielded paragraphs may overflow | `flow.rs::field_kind`, `shape_fielded_paragraph` |

## Detailed findings

### 1. Modeled content that still becomes invisible

`flow.rs::collect_items` handles runs, tabs, symbols, breaks, pictures, floats,
fields, horizontal rules, text boxes, groups, and recursive wrappers. Its
catch-all silently ignores several valid `InlineNode` variants:

- `EmbeddedObject` — charts, SmartArt, and OLE objects retain their package
  references and optional preview, but layout paints neither the object nor the
  preview;
- `Math` — the retained OMML and best-effort `m:t` text fallback round-trip, but
  neither is shown;
- `NoteReference` — the reference glyph is absent, before considering note-body
  pagination;
- `NoBreakHyphen` and `SoftHyphen` — visible/break semantics are absent;
- comment, bookmark, move, and range markers — zero-width markers may correctly
  remain non-painting, but comments additionally lack any visible review
  affordance.

The same omission occurs inside hyperlinks, revisions, and inline content
controls because those wrappers recurse into the same function. The same
function is used from body paragraphs, table cells, text boxes, headers, and
footers, so the gap is not body-only.

`BlockNode::AltChunk` is likewise an explicit no-op in fresh flow, cached flow,
intrinsic sizing, and float discovery. Its referenced part is preserved and
relinked, but it contributes zero height and no fallback representation.

**Required direction:** add small leaf-specific slices, never a generic
“stringify every unknown node” fallback. At minimum:

1. special hyphens;
2. math text fallback with a compatibility marker;
3. embedded-object preview through the existing image pipeline, with a visible
   bounded placeholder when no preview is available;
4. explicit `altChunk` placeholder or a host-provided conversion seam.

Each slice needs body, header, footer, nested-cell, and text-box tests where the
node is valid in that context.

### 2. Footnotes and endnotes are modeled but not paginated

The model stores footnote/endnote definitions and inline references. `Page`
already has a `footnotes` band, but `paginate.rs::build_page` initializes it to
an empty vector and there is no later placement pass. `NoteReference` is also
dropped by inline collection.

This requires more than painting the reference number. A correct implementation
must:

- resolve the reference mark and note definition;
- flow note content through the shared block pipeline;
- reserve a bottom band on the page containing the reference;
- repaginate when the reservation moves body content and therefore moves the
  reference;
- handle split/continued notes within explicit bounds;
- place endnotes at the configured section/document boundary;
- honor per-section numbering, restart, format, and placement policy.

This is a fixed-point pagination feature and should not be mixed into the small
inline-leaf PR.

### 3. Float wrapping remains bounded and paragraph-local

The model carries `None`, `Square`, `Tight`, `Through`, and `TopAndBottom` plus
four wrap distances. `TopAndBottom` inserts a local vertical barrier.
Paragraph/line-relative left/right `Square`, `Tight`, and `Through` floats with
explicit geometry use a resumable line breaker that narrows only intersecting
lines and restores the full measure below the object. Tight/through currently
use the square bounding box rather than a contour.

Consequences:

- exclusions do not continue into later paragraphs;
- tight/through wrapping does not use an authored contour;
- page/margin/alignment-relative objects do not push later paragraphs;
- multiple overlapping side floats have no exclusion union;
- the current VML body safety policy must keep unsupported side/page cases
  inline to avoid overprint.

This affects body content, nested table cells, text boxes, headers, and footers;
the shared flow path propagates both the existing local support and the
remaining limitation.

**Next bounded design:** extend the existing line-interval seam with
cross-paragraph lifetime and a deterministic page-coupled convergence rule
before enabling page-relative body VML floats. Multiple-float interval unions
and tight/through contours follow.

### 4. Headers and footers are section-scoped

The document driver now builds a section-keyed running-content plan. Default,
first, and even references inherit across sections as WordprocessingML
specifies; each section is measured at its own content width, reserves its own
header/footer bands, and selects `titlePg` from section-local first-page state
while retaining global page parity.

Remaining approximations:

- a selected band taller than its reservation overflows; Word grows/reserves the
  band;
- continuous sections sharing a physical page use one page owner for running
  furniture; more exact mid-page ownership is still a compatibility choice.

### 5. Sections and columns retain known approximations

`columns.rs` explicitly documents:

- unequal columns are positioned at authored widths but share one galley flowed
  at the widest column, so content can overrun a narrow column;
- the final column page is not balanced;
- column breaks collapse to page breaks;
- table header rows do not repeat across column transitions.

Additional section gaps:

- `evenPage` and `oddPage` start a fresh page but do not insert a parity blank
  when required;
- `nextColumn` is treated like a continuous section rather than advancing to the
  next available column;
- incremental repagination remains the single-column path;
- per-section header/footer band growth remains the separate approximation
  above.

### 6. Table layout consumes only a direct-property subset

The table model and importer are substantially richer than layout. The style
cascade explicitly implements:

`docDefaults → paragraph style → character style → direct`

and defers table-style and numbering layers. `flow.rs::flow_table` reads common
direct table, row, and cell properties, but it does not resolve:

- `TableProperties::style_ref`, table style `basedOn`, `tblLook`, or
  `tblStylePr` conditional regions;
- table/row conditional selectors (`cnfStyle`);
- table or row alignment;
- `tbl_bidi_visual`;
- table/row cell spacing;
- floating-table position and overlap;
- cell `no_wrap`, `text_direction`, `fit_text`, or `hide_mark`.

Accordingly, an authored table style can lose its region-specific fonts,
paragraph formatting, fills, borders, and margins even though those values are
modeled. Floating tables stay in normal block flow. RTL visual column order and
separate-cell spacing are absent.

Border support is intentionally bounded: common solid/double/dotted/dashed
families and segmented spans paint, while art/compound tokens use bounded
fallbacks. Conflict ranking covers common cases, not every Word tie-break.

Intrinsic table sizing has another cross-feature gap. `block_intrinsic` measures
paragraphs through `collect_runs`, so inline images, text boxes, fields, object
previews, and other inline boxes do not contribute their natural width. This can
mis-size image/table-heavy documents even when the objects later paint.

**PR order:** table-style cascade and conditional regions first; intrinsic inline
box sizing second; cell spacing/bidi/alignment third; floating tables only after
the float-exclusion architecture can support them safely.

### 7. RTL and per-script typography are partial

The shaper performs Unicode BiDi analysis and records per-run bidi levels, so
ordinary mixed-direction text is not wholly unsupported. Layout still sets
`LineConstraints.rtl` to `false`, however, and does not derive paragraph base
direction from `w:bidi` or run direction from `w:rtl`.

Font selection is also ASCII-slot-only:
`flow.rs::requested_family` reads `RunProperties::font_ref` but not the modeled
`font_ref_h_ansi`, `font_ref_east_asia`, `font_ref_cs`, or `font_hint`.
Likewise, shaped weight/style/size use the Latin `bold`, `italic`, and
`size_half_points`, not `bold_complex`, `italic_complex`, and
`size_complex_half_points`.

Run character scaling (`w:w`) is modeled, round-tripped, and consumed by shaping
and paint. Exact-height lines clip escaped ink and re-anchor each glyph baseline
inside the authored box. For CJK/scaled runs, sub-single `auto` spacing retains
at least a one-em pitch to prevent the observed line-on-line overprint. These
are containment rules, not a claim that every producer's CJK grid and line
metrics are reproduced exactly; doc 60 records the remaining SDS pagination
delta.

This means system fallback may find a glyph-covering face while still ignoring
the producer's intended per-script face and complex-script metrics.

Other modeled run effects that are not painted include double strike, emphasis
marks, kerning threshold policy, outline, shadow, emboss, imprint, run border,
and run shading. Import currently flattens underline style/color to a boolean,
and run theme tint/shade factors are not modeled.

Paragraph properties such as mirror indents, East Asian auto-spacing, kinsoku,
punctuation overflow, grid snapping, and vertical/text alignment require
separate model-or-consumption review before Word-grade CJK/RTL can be claimed.

### 8. Images have size, format, and transform gaps

The normal DrawingML inline-image path paints positive-extent PNG/JPEG content
as a true in-flow box in the Parley stream. Text and images share advance,
wrapping, ascent/descent, line height, paint position, and hit-test geometry,
including table-row height. A conservative split path remains for unsupported
mixed field/tab/object combinations, and intrinsic table sizing still needs to
account for all atomic boxes.

The normal inline-image path requires a media entry and a positive `Extent`.
`image_item` returns `None` otherwise. The VML importer currently maps a genuinely
inline `v:imagedata` to `Segment::Drawing { extent: None }` even when the parsed
VML CSS box contains width and height. Such an image survives import but renders
nothing.

The CPU backend deliberately enables only PNG and JPEG codecs. EMF/WMF, SVG,
GIF, BMP, and TIFF are currently unsupported/undecodable and therefore paint
nothing. A production document engine needs an explicit safe policy per format:
native decode, bounded vector conversion, host-provided rasterization, or a
visible unsupported-media placeholder.

The inline picture model does not yet carry the full DrawingML appearance
surface, including crop/source rectangle, rotation/flip, and picture effects.
Anchors preserve useful position/wrap metadata but still defer simple-position
override, percentage offsets, exact character glyph anchoring, and complete
inside/outside parity behavior.

### 9. Shapes and text boxes remain approximate

`ShapeGeometry` distinguishes rectangle, round rectangle, ellipse, and line.
The float painter maps only `Line` specially; every other geometry becomes
`AnchorContent::Rectangle`. Thus an imported ellipse or rounded rectangle paints
as a rectangular fill/stroke.

Generic VML paths are retained by the parser but rendered as bounded-box
approximations. Gradients, exact preset/custom paths, per-side strokes,
rotation, vertical writing, and exact diagonal orientation remain open.

Text-box support is comparatively strong: authored extent, fill/outline,
independent insets, vertical anchor, overflow, shape autofit, normal-autofit,
nested blocks/tables/images, and repeated header/footer use are implemented.
Remaining gaps include:

- text rotation and vertical writing;
- `anchorCtr`, internal columns, and preset text warp;
- exact ellipsis synthesis (`ellipsis` currently clips);
- linked text-box chains;
- percentage positioning and exact mirrored parity;
- side/page-coupled float wrapping;
- legacy VML re-emission rather than normalization to DrawingML on semantic
  export.

### 10. Field display is cache-dependent beyond page numbers

Only `PAGE` and `NUMPAGES` are evaluated after pagination. Every other field kind
is a passthrough that displays the producer's cached result; a missing cache
therefore displays nothing. This is safe as a rendering fallback but is not a
field engine.

Fielded paragraphs are still laid out as one line per hard-break block. Tabbed
rows remain single-line when they fit, while an overflowing trailing value now
soft-wraps at its resolved tab column with the continuation held to that hanging
column. Long field results and mixed field/text paragraphs can still overrun the
available width.

Future evaluation must be policy-driven and deterministic. Dates, links,
references, formulas, document properties, and sequence fields should not read
ambient host state without an explicit host-supplied evaluation context.

### 11. Revisions, comments, and content controls have no presentation policy

Revision wrappers recurse transparently without inspecting `RevisionKind`.
Insertions, deletions, move-from, and move-to content therefore all appear in
the normal render. There is no final/original/markup view selection.

Comment ranges/references are modeled but produce no page marker, highlight,
callout, or review-pane data. Content-control children render transparently, but
there is no control chrome, placeholder state, lock/policy presentation, or
interactive binding behavior.

The render API needs an explicit immutable view policy rather than hard-coding
one review mode into flow. Final view should suppress deletions/move-from;
original view should suppress insertions/move-to; markup view needs stable
annotations without mutating the model.

### 12. Section furniture and numbering have a modeled long tail

`SectionBoundary` carries page borders, line numbering, per-section footnote and
endnote properties, text direction, bidi, page vertical alignment, paper source,
and orientation. Layout consumes page dimensions/margins and the implemented
column subset, but not most of that furniture.

Specific effects still absent include:

- page-border paint and its offset/z-order rules;
- line-number generation and restart behavior;
- vertical page alignment;
- section-wide text direction/bidi;
- footnote/endnote placement and numbering policy;
- printer paper-source semantics, which should likely remain host metadata.

Numbering layout supports common formats, level text, suffixes, indentation,
counters, and model-provided start overrides. Remaining explicit deferrals are:

- `w:lvlRestart` is not modeled;
- `cardinalText`, `ordinalText`, and unknown formats fall back to decimal;
- the importer does not populate per-instance `lvlOverride`/`startOverride`
  even though the model/layout can consume an override.

### 13. Preservation is stronger than visual fidelity, but not byte-exact after edits

The two save expectations must remain explicit:

- **Retention/no edit:** `RetainedSource` keeps the original admitted parts for a
  byte floor.
- **Semantic edit/save:** the writer regenerates modeled consumed parts and
  re-emits admitted unconsumed parts through the opaque side table.

This prevents broad package-part loss, but semantic save is not byte-exact for
consumed XML. Unsupported markup inside a consumed part can be reported or
flattened rather than preserved in its original source position.

Known deliberate or bounded degradations include:

- ruby base text survives while phonetic annotation text is reported and
  dropped;
- underline style/color is flattened to a boolean;
- some theme effects and color modifiers are flattened;
- `mc:AlternateContent` selects a supported branch rather than retaining the
  complete source branching structure semantically;
- VML is normalized into the shared drawing model rather than emitted in its
  exact legacy source form;
- digital signatures are dropped and reported on semantic edit because
  regenerated content invalidates them;
- macro project parts are rejected at package admission and never executed.

The compatibility report is therefore part of the fidelity contract, not only a
diagnostic convenience. Release behavior must surface it whenever semantic save
can degrade authored detail.

## Context coverage matrix

This matrix prevents a body-only implementation from being described as general
support.

| Capability | Body | Header/footer | Nested cell / SDT / text box | Current boundary |
| --- | --- | --- | --- | --- |
| Paragraph/run core flow | Yes | Yes | Yes | Same shared block/inline paths |
| Direct table layout | Yes | Yes | Yes | Table-style cascade and advanced table props absent |
| PNG/JPEG inline picture with extent | Yes | Yes | Yes | True in-flow box; other formats and extent-less inline VML omitted |
| Positioned float paint/z-order | Yes | Yes | Yes | Running-content source and geometry are section-scoped |
| `topAndBottom` local reflow | Yes | Yes | Yes | Paragraph/line-relative explicit-offset envelope only |
| Square/tight/through reflow | Safe subset | Safe subset | Safe subset | Local explicit paragraph/line-relative side exclusion; square bounds only |
| DrawingML text-box body properties | Yes | Yes | Yes | Rotation/vertical writing/ellipsis/anchorCtr absent |
| VML text-box position/body properties | Safe subset | Yes | Safe subset | Unsafe side/page body cases remain inline |
| `PAGE`/`NUMPAGES` | Yes | Yes | Yes, including anchored boxes | Other fields use cached result |
| Per-section running-content selection | N/A | Yes | N/A | Continuous-page ownership and band growth remain approximate |
| Notes | No | No | No | References and bodies not laid out |
| Embedded objects/math/special hyphens | No | No | No | Modeled/preserved but omitted by shared inline flow |
| Review markup presentation | No | No | No | Wrappers transparent; no view policy |

## Recommended implementation sequence

Each item is intentionally a separate reviewable design/PR unless its design
proves a smaller safe combination.

1. **Corpus pagination convergence**
   - implement deterministic final-page column balancing and true per-column
     reflow before tuning line metrics;
   - close table style/row-height gaps that account for the Medical form's
     remaining page-distribution delta;
   - keep per-document visual/page-placement evidence so a matching total alone
     is never accepted as proof.
2. **Inline visibility floor**
   - render no-break/soft hyphens;
   - render OMML text fallback and embedded-object previews/placeholders;
   - add explicit compatibility output for display fallbacks.
3. **Inline VML image extent**
   - bridge parsed CSS width/height into `Drawing::extent`;
   - cover body, header, footer, and nested-cell inline images.
4. **Extend square float exclusion**
   - add cross-paragraph lifetime, multiple-float interval union, and
     page-coupled convergence;
   - only then enable currently unsafe VML side/page body floats.
5. **Table style cascade**
   - table `basedOn`, look flags, conditional regions, and cell/paragraph/run
     overlays;
   - then cell spacing/bidi/alignment; floating tables last.
6. **Footnote/endnote pagination**
   - reference marks, note band, fixed point, continuation, endnote placement,
     and per-section policy.
7. **RTL/per-script typography**
    - paragraph base direction, per-script font slots, complex-script
      bold/italic/size, then language-specific line rules.
8. **Geometry and media**
    - ellipse/round-rect paint, picture crop/transform, safe additional raster
      formats, then vector metafile/SVG policy and generic VML paths.
9. **Review and field view policies**
    - immutable revision/comment view;
    - deterministic host-provided field evaluation.
10. **Page furniture and long tail**
    - parity section breaks, next-column semantics, balancing, page borders,
      line numbers, vertical page alignment, and remaining numbering rules.

## Required fidelity gates

Every behavior-changing fidelity PR should include:

- a model/import/export fixed-point test when the representation changes;
- a layout assertion against final geometry or display-list primitives, not only
  an importer assertion;
- body plus applicable header/footer and nested-content coverage;
- a malformed/bounded fallback test for parser or media changes;
- a real-document visual or page-placement comparison when pagination changes;
- explicit compatibility-report assertions for degradation;
- formatting, strict Clippy, all-feature workspace tests/doc tests, Rustdoc,
  WASM, MSRV, benchmark smoke, and `git diff --check` as required by doc 15.

The visual-regression CI gap remains open: deterministic golden fonts and a
curated rendered fixture set are needed before “Word-grade” can become a
release-enforced claim rather than a manual differential assessment.

## Completion rule

A row may be called fully supported only when:

1. source semantics are modeled or deliberately preserved;
2. semantic save is fixed-pointed or degradation is reported;
3. layout consumes the relevant values;
4. paint visibly reflects them;
5. body, header/footer, and nested contexts are covered where valid;
6. resource bounds and deterministic behavior are tested.

“Modeled,” “round-trips,” and “renders” are separate milestones and must remain
separate in tracker and support-matrix wording.
