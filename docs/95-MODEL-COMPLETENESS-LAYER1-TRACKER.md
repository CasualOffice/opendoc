# 95 — Model-Completeness (Layer 1) Tracker

**Status:** Active. The typed-model completion pass — Layer 1 of a deliberate layering.
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

| Done | Item | OOXML | Prev | Scope |
|---|---|---|---|---|
| ✅ | Abstract `multiLevelType` | `w:multiLevelType` | ubiquitous | S |
| ✅ | Level restart trigger | `w:lvlRestart` | common | S |
| ✅ | Run text color "auto" | `w:color@val=auto` | ubiquitous | S |
| | Numbering level → paragraph-style link | `w:lvl/w:pStyle` | ubiquitous | S |
| | Note-number mark in note body | `w:footnoteRef`/`w:endnoteRef` | ubiquitous | S |
| ✅ | do-not-proof | `w:noProof` | common | S |
| | Style tab stops *(partial)* | `w:pPr/w:tabs` in styles.xml | common | S |
| | Page gutter margin | `w:pgMar@gutter` | common | S |
| | Update-fields-on-open | `w:updateFields` | common | S |
| | Footnote/endnote numFmt → enum *(partial)* | `w:footnotePr/w:numFmt` | common | S |
| | Shape/picture flip | `a:xfrm@flipH/@flipV` | common | S |
| | Math func / accent / limits | `m:func`,`m:acc`,`m:limLow/Upp` | common | S |
| | Run theme color + tint/shade *(partial)* | `w:color@themeColor/@themeTint/@themeShade` | ubiquitous | M |
| | Percentage table/cell width (AutoFit) | `w:tblW`/`w:tcW@type=pct\|auto` | ubiquitous | M |
| | Reusable list-style linkage | `w:numStyleLink`,`w:styleLink` | common | M |
| | Shape/picture rotation | `a:xfrm@rot` | common | M |
| | Line dash + arrowheads *(partial)* | `a:prstDash`,`a:headEnd/tailEnd` | common | M |
| | Doc-default footnote/endnote props | `w:settings/w:footnotePr` | common | M |
| | Paragraph-mark revision | `pPr/rPr/w:ins\|w:del` | common | M |
| | Shading theme fill *(partial)* | `w:shd@themeFill/…` | common | M |
| | Math n-ary / matrix / eqArr | `m:nary`,`m:m`,`m:eqArr` | common | M |

### Tier 2 — ubiquitous/common L, or occasional S/M

Big-value L: typed **`FieldKind`** for `w:instrText` (PAGE/TOC/REF/DATE/SEQ/STYLEREF/HYPERLINK…); **`latentStyles`** table; **gradient fill** (Fill enum). Occasional S/M: paragraph `w:textDirection` *(partial)*; clear tab `w:tab@val=clear` *(partial)*; table-style band sizes; row `gridBefore/gridAfter`; picture border *(partial)*; hyperlink `@anchor` fragment *(partial)*; `pgNumType` fmt → enum *(partial)*; `displayBackgroundShape` *(partial)*; sdt gallery/category *(partial)*; math `bar`/`groupChr`; full `lvlOverride/lvl` *(partial)*; wrap polygon *(partial)*; `sectPrChange`; tracked table row/cell ins/del; auto-hyphenation settings.

### Tier 3 — none.

### Excluded as niche
- `w:fitText` — rare run-compression, only in unusual form/cell layouts.
- non-drop-cap `w:framePr` — legacy positioned text frame superseded by DrawingML text boxes.

## Working method

Batch the cheap `S` items into grouped PRs by area (numbering cluster, settings cluster, math-arms cluster, drawing flip/rotate cluster). Each construct: typed field/enum → import → export → a round-trip test. Then the `M`/`L` items individually. Rendering stays untouched until the model column is "full" for the common families.
