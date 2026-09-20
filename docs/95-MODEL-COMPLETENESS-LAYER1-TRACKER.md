# 95 — Model-Completeness (Layer 1) Tracker

**Status:** ✅ **Layer 1 (typed model) COMPLETE — and 17 of its 40 rows are still unreachable from the product.** The typed-model completion pass (Layer 1 of a deliberate layering) is done — every ubiquitous/common/occasional construct is typed and round-trips; a completeness-critic sweep of all 139 import disposition sites confirmed no common-or-above construct remains dropped (the remainder is niche and retained verbatim by the floor). The `modeled` column of the fidelity matrix (doc 18 / `webapp/src/fidelity.js`) has been advanced accordingly. Next layer: rendering fidelity (Layer 3).
**Scope:** `casual-doc-model` (typed model) + `casual-doc-import` + `casual-doc-export` (round-trip). **No new rendering in this layer.**
**Relates to:** doc 18 (support matrix — the `modeled` column is the exit criterion), doc 35 (disposition taxonomy — dropped / preserved-opaquely / not-retained), the rendering-fidelity audit (the layer that follows).

## The layering

Fidelity is built in order, one layer completed before the next:

1. **Round-trip / retention** — *done.* Anything unmodeled is kept verbatim (opaque sidecar) and reported; nothing is silently lost.
2. **Typed model (this doc)** — turn everything **real documents actually use** from retained-verbatim/dropped into **typed** model fields, verified by round-trip. This is the stable foundation render and edit build on (and the anchor for the OT/CRDT · MCP · RAG seams).
3. **Rendering fidelity** — paint the now-complete model; the modeled-but-not-rendered backlog.
4. **Editability + UX/UI** — edit ops over the complete model, designed with real interaction/UX.

**Bounding rule:** only ubiquitous/common/occasional constructs a typical business/office/academic document uses. Niche/legacy/once-a-year is explicitly out (retention floor still catches it). Exit criterion: the `modeled` column in the fidelity matrix reads "full" for every family the corpus exercises.

## Backlog

Derived from a 14-agent, evidence-cited model-completeness sweep, prevalence-ranked, niche excluded. **8 items are already partially modeled** (typed struct exists — import just needs wiring), marked *(partial)*.

### Tier 1 — ubiquitous/common, S/M (do first)

| Done | Item | OOXML | Prev | Scope | Consumer |
|---|---|---|---|---|---|
| ✅ | Abstract `multiLevelType` | `w:multiLevelType` | ubiquitous | S | unconsumed `multi_level_type` (FID-P-04) |
| ✅ | Level restart trigger | `w:lvlRestart` | common | S | `lvl_restart` → `crates/casual-doc-layout/src/numbering.rs`#`fn resolve` |
| ✅ | Run text color "auto" | `w:color@val=auto` | ubiquitous | S | `Color::Auto` → `crates/casual-doc-layout/src/flow.rs`#`fn run_color` |
| ✅ | Numbering level → paragraph-style link | `w:lvl/w:pStyle` | ubiquitous | S | unconsumed `pstyle` (FID-P-04) |
| ✅ | Note-number mark in note body | `w:footnoteRef`/`w:endnoteRef` | ubiquitous | S | `NoteNumberMark` → `crates/casual-doc-layout/src/flow.rs`#`fn note_number_run` |
| ✅ | do-not-proof | `w:noProof` | common | S | unconsumed `no_proof` (FID-P-04) — no spell checker exists to read it |
| ✅ | Style tab stops *(partial)* | `w:pPr/w:tabs` in styles.xml | common | S | `tab_stops: &props.tabs` → `crates/casual-doc-layout/src/flow.rs`#`fn build_galley_cached_labeled` |
| ✅ | Page gutter margin | `w:pgMar@gutter` | common | S | `gutter_twips` → `crates/casual-doc-layout/src/document_layout.rs`#`fn section_page_config` |
| ✅ | Update-fields-on-open | `w:updateFields` | common | S | unconsumed `update_fields` (FID-P-04) |
| ✅ | Footnote/endnote numFmt → enum *(partial)* | `w:footnotePr/w:numFmt` | common | S | `number_format` → `crates/casual-doc-layout/src/note_numbering.rs`#`fn resolve_props` |
| ✅ | Shape/picture flip | `a:xfrm@flipH/@flipV` | common | S | `flip_h` → `crates/casual-doc-render/src/lib.rs`#`fn object_transform` |
| ✅ | Math func / accent / limits | `m:func`,`m:acc`,`m:limLow/Upp` | common | S | `MathExpression::Function` → `crates/casual-doc-layout/src/flow.rs`#`fn layout_math_expression` |
| ✅ | Run theme color + tint/shade *(partial)* | `w:color@themeColor/@themeTint/@themeShade` | ubiquitous | M | `apply_tint_shade` → `crates/casual-doc-layout/src/flow.rs`#`fn run_color` |
| ✅ | Percentage table/cell width (AutoFit) | `w:tblW`/`w:tcW@type=pct\|auto` | ubiquitous | M | unconsumed `width_type` (FID-P-04) — the solver reads `dxa_twips()` only, so `Pct`/`Auto` collapse to content sizing |
| ✅ | Reusable list-style linkage | `w:numStyleLink`,`w:styleLink` | common | M | `num_style_link` → `crates/casual-doc-layout/src/numbering.rs`#`fn resolve_abstract` |
| ✅ | Shape/picture rotation | `a:xfrm@rot` | common | M | `rotation` → `crates/casual-doc-render/src/lib.rs`#`fn object_transform` |
| ✅ | Line dash + arrowheads *(partial)* | `a:prstDash`,`a:headEnd/tailEnd` | common | M | `DashStyle` → `crates/casual-doc-render/src/lib.rs`#`fn dash_pattern` |
| ✅ | Doc-default footnote/endnote props | `w:settings/w:footnotePr` | common | M | `footnote_props` → `crates/casual-doc-layout/src/note_numbering.rs`#`fn resolve_props` |
| ✅ | Paragraph-mark revision | `pPr/rPr/w:ins\|w:del` | common | M | `mark_revision` → `crates/casual-doc-wasm/src/lib.rs`#`fn decide_paragraph_revision` |
| ✅ | Shading theme fill *(partial)* | `w:shd@themeFill/…` | common | M | `theme_fill` → `crates/casual-doc-layout/src/flow.rs`#`fn shading_rgba` |
| ✅ | Math n-ary / matrix / eqArr | `m:nary`,`m:m`,`m:eqArr` | common | M | `MathExpression::Nary` → `crates/casual-doc-layout/src/flow.rs`#`fn layout_math_expression` |

### Tier 2 — ubiquitous/common L, or occasional S/M

| Done | Item | OOXML | Scope | Consumer |
|---|---|---|---|---|
| ✅ | Typed `FieldKind` for field instructions | `w:instrText` (PAGE/TOC/REF/DATE/SEQ/STYLEREF/HYPERLINK…) | L | unconsumed `FieldKind` (FID-P-04) — layout re-derives a coarser Page/NumPages/Passthrough split from the raw instruction |
| ✅ | `latentStyles` table | `w:latentStyles`/`w:lsdException` | L | unconsumed `latent_styles` (FID-P-04) |
| ✅ | Gradient fill (Fill enum) | `a:gradFill` | L | `Gradient` → `crates/casual-doc-layout/src/compose.rs`#`fn fill_to_display` |
| ✅ | Table-style band sizes | `w:tblStyleRowBandSize`/`w:tblStyleColBandSize` | S | unconsumed `row_band_size` (FID-P-04) — banding is driven by `w:cnfStyle` bits, never by the period |
| ✅ | Short-row skipped columns | `w:gridBefore`/`w:gridAfter`/`w:wBefore`/`w:wAfter` | S | unconsumed `grid_before` (FID-P-04) |
| ✅ | Picture frame border | `pic:spPr/a:ln` | S | `border` → `crates/casual-doc-layout/src/anchor.rs`#`fn shape_stroke` |
| ✅ | Hyperlink in-target fragment | `w:hyperlink@w:anchor` | S | unconsumed `ExternalTarget` (FID-P-04) — `hyperlink_href` builds the href from the url alone, dropping the fragment |
| ✅ | `displayBackgroundShape` | `w:settings/w:displayBackgroundShape` | S | unconsumed `display_background_shape` (FID-P-04) — the background paints regardless of the flag |
| ✅ | Sdt building-block gallery/category | `w:docPartObj/w:docPartGallery`/`w:docPartCategory` | S | unconsumed `gallery` (FID-P-04) |
| ✅ | Auto-hyphenation settings | `w:autoHyphenation`/`w:hyphenationZone`/`w:consecutiveHyphenLimit`/`w:doNotHyphenateCaps` | S | unconsumed `auto_hyphenation` (FID-P-04) — there is no hyphenator |
| ✅ | Math `bar`/`groupChr` | `m:bar`,`m:groupChr` | S | `MathExpression::Bar` → `crates/casual-doc-layout/src/flow.rs`#`fn layout_math_expression` |
| ✅ | Tracked table row/cell ins/del | `w:trPr/w:ins\|w:del`, `w:tcPr/w:cellIns\|w:cellDel` | M | unconsumed `row_revision` (FID-P-04) — run-level revisions are consumed, row/cell ones are not |
| ✅ | Tracked property-change markers | `w:pPrChange`/`w:rPrChange`/`w:tblPrChange`/`w:trPrChange`/`w:tcPrChange`/`w:tblGridChange` | M | `prop_change` → `crates/casual-doc-wasm/src/lib.rs`#`fn track_paragraph_change` — paragraph/run only; the four table-level variants have no consumer |
| ✅ | Paragraph text direction | `w:pPr/w:textDirection` | S | unconsumed `text_direction` (FID-P-04) — one writing-mode axis through flow, composition, hit-testing and caret |
| ✅ | Clear tab | `w:tab@val=clear` (`TabAlignment::Clear`) | S | `TabAlignment::Clear` → `crates/casual-doc-layout/src/tabs.rs`#`fn resolve_next_stop` |
| ✅ | Full per-instance level override | `w:lvlOverride/w:lvl` (full level, not just startOverride) | M | `definition` → `crates/casual-doc-layout/src/numbering.rs`#`fn effective_level` |
| ✅ | Page-number format → enum | `w:pgNumType@fmt`/`@start` | S | `page_numbering` → `crates/casual-doc-layout/src/paginate.rs`#`fn page_number_label_at` |
| ✅ | Section-properties revision | `w:sectPrChange` | M | unconsumed `section_change` (FID-P-04) |
| ✅ | Wrap polygon | `wp:wrapPolygon` | M | unconsumed `wrap_polygon` (FID-P-04) |

### The consumer ledger (FID-P-04)

Every row above carries a **Consumer** cell, and `crates/casual-doc-model/tests/model_consumer_ledger.rs`
fails the build if one is missing, malformed, or lying. A cell is one of:

| form | meaning | what the guard checks |
|---|---|---|
| `` `field` → `path`#`symbol` `` | something outside the model/import/export round-trip reads this field | the file exists, and contains **both** the field name and the symbol |
| ``unconsumed `field` (ROW-ID)`` | typed and round-tripped, but nothing reads it — the construct is invisible to a user | the field exists in `casual-doc-model`, and a tracker row is cited |
| `preservation-only: …` | deliberately never consumed; retention is the whole intent | the reason is stated |

This exists because **"modeled" was being counted as done while nothing consumed it** — `docs/105`
calls it the most expensive recurring pattern in this repository, and marking a row ✅ on the strength
of a DOCX round-trip is exactly how it happens. A round-trip proves the bytes survive. It proves
nothing about a user ever seeing the construct.

**The count is the deliverable: 17 of 40 Tier-1/Tier-2 rows are modelled and unreachable.**

- Tier 1 (5 of 21): `multiLevelType`, numbering level → `pStyle` link, `w:noProof`, `w:updateFields`,
  and percentage table/cell width — the last is the subtle one: a consumer *exists* but reads
  `dxa_twips()` only, so `Pct`/`Auto` silently degrade to content sizing.
- Tier 2 (12 of 19): typed `FieldKind`, `latentStyles`, table-style band sizes, short-row
  `gridBefore`/`gridAfter`, the hyperlink in-target fragment, `displayBackgroundShape`, sdt
  gallery/category, the four auto-hyphenation settings, tracked table row/cell ins/del, paragraph
  `textDirection`, `sectPrChange`, and `wrapPolygon`.
- Plus one partial that the ledger records in prose rather than in the count, because the row does
  have a consumer: of the six tracked property-change markers, `w:pPrChange` and `w:rPrChange` drive
  real accept/reject UI, while `w:tblPrChange`, `w:trPrChange`, `w:tcPrChange` and `w:tblGridChange`
  have no consumer at all.

None of these were reclassified as `preservation-only` to make the guard pass. They are open gaps,
and `UNCONSUMED_CEILING` in the guard is a **ratchet**, not an allowance: adding another unconsumed
row fails the build, and every row that gains a consumer should lower the number. It is a debt
ledger with a build-enforced maximum, not an approval.

Legend: ✅ merged · 🔄 in flight · ⬜ not started. **All Tier 1 complete (21/21). All Tier 2 complete (19/19).** Layer 1 is **done**.

### Completeness-critic sweep result (definitive)

A read-only sweep classified all 139 `reporter.report(...)` sites in `casual-doc-import` against the disposition taxonomy, then cross-checked each against the model on `main`. **No common-or-above construct remains unmodeled.** Every genuinely-unmodeled construct is occasional-or-niche and retained verbatim by the byte-floor:

- **Niche, retained-not-typed (do NOT model):** `w:ruby`/`w:rt` phonetic text and `w:eastAsianLayout` (CJK-only), `w:effect` text animation (legacy), `w:background@themeColor` (rare theme variant), `w:fitText`, non-drop-cap `w:framePr`, and the ~43 dynamic catch-all sites (shape-geometry internals like `custGeom`/`gd`, sdt `dataBinding`/`lock`, unmapped `docProps`/`settings` toggles).

### Tier 3 — none.

### Excluded as niche
- `w:fitText` — rare run-compression, only in unusual form/cell layouts.
- non-drop-cap `w:framePr` — legacy positioned text frame superseded by DrawingML text boxes.

## Working method

Batch the cheap `S` items into grouped PRs by area (numbering cluster, settings cluster, math-arms cluster, drawing flip/rotate cluster). Each construct: typed field/enum → import → export → a round-trip test. Then the `M`/`L` items individually. Rendering stays untouched until the model column is "full" for the common families.
