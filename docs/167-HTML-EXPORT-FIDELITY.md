# 167 — HTML export fidelity: the file is resolved the way the page is

**Status:** implemented (ADR-066, `109` HF-286, HF-284, HF-285). **Opened:** 2026-10-09.
**Asked for by the owner:** "check HTML export — its fidelity is way too weak — work on it and
improve it", then "fix and embed header, footer, drawings and TOC — it's important".

## 1. What was wrong, measured

Twelve documents — the five webapp samples and seven real-producer files from
`fixtures/corpus` — were opened in the editor, exported through **File ▸ Export as Web Page**
(the same `exportAs("text.html")` call), and the export's first screen was put beside the
engine's own page 1. Every one disagreed with the page the same way:

| What the page shows | What the export showed | Why |
| --- | --- | --- |
| Title, Subtitle and headings in their style's font, size, colour and weight | browser-default black serif `<h1>` or a plain `<p>` | the writer read **direct formatting only**; a style was reported as `html.paragraph_style` and otherwise ignored |
| the document's font (Calibri, Cambria…) | Georgia, for everything | the writer never emitted a family |
| Word's paragraph spacing and line spacing | the browser's 1em margins, collapsed | spacing was not read |
| a table in its style: fills, banded header, the resolved borders | a 1px grey grid, **first row always bold and centred `<th>`** | table formatting was reported as `html.table_formatting` and replaced by a fixed stylesheet |
| nested, numbered lists with their labels (`1.1.`, `a)`, `•`) | one flat `<ol>`/`<ul>` per run, browser numbering | the level and the format were not read |
| a picture at its size | the picture at its pixel size, or a dot | the extent was not read |
| footnotes at the page foot | **nothing**: the note text was dropped | references were reported as `html.note_reference` and the notes never written |
| headers and footers | nothing, **and nothing reported** | silent loss |
| a contents line: title, dot leader, page number at the right margin | "Introduction 3", the number one tab-width along | a custom tab stop advanced on the default grid |
| drawn shapes, groups, a group's text boxes; a chart Word stored no picture for | nothing (`html.group_shape`, `html.embedded_object`) | no drawing path at all |

A vertical merge under a horizontally merged cell also exported wrongly: the continuation was
matched by cell index instead of grid column.

## 2. The decision (ADR-066)

**The export is resolved by the renderer's resolver, not by a second one.** Every paragraph, run
and table cell goes through `casual_doc_layout::cascade::StyleCascade` — document defaults, the
table style and its conditional regions, the paragraph style chain, the character style, direct
formatting — and every colour, border and fill through the renderer's palette and conflict
rules. `casual_doc_layout::paint_values` is the read-only door to those decisions; each of its
functions delegates to the one the flow engine itself calls. A fix to how the page resolves a
cell's fill is therefore a fix to the export's fill too — the two cannot drift. Named pattern:
**one source of truth** for presentation, consumed by two outputs.

**The stylesheet is normalized** — the pattern Word's and LibreOffice's own HTML filters use.
One class per paragraph style holds that style's *resolved* declarations (`.s-heading-1{…}`);
each element carries only where it differs (`Declarations::delta`), and resets with `initial`
what its class sets and it does not. A plain run inside a heading writes no declarations at
all; a bold run in a body paragraph is `<strong>` and nothing else.

### What each construct becomes

| Construct | HTML |
| --- | --- |
| paragraph style | a class: font stack, size, colour, weight, style, caps, letter-spacing, alignment, indents, line spacing, borders, fill, keep rules |
| heading (outline level or name) | `<h1>`–`<h6>`, its class overriding the browser's heading size |
| run | `<strong>`/`<em>` where bolder/more italic than its paragraph; `<sup>`/`<sub>` (neutral in the stylesheet; the size and shift are the page's two thirds and one third); a `<span>` with the delta, decorations (style and colour), highlight or shading, baseline position, border, `lang`, `dir`, `hidden` |
| font | `'Calibri','Carlito',sans-serif`: the authored family, then the metric-compatible face the page draws when the reader lacks it (`font_substitution::substitute`), then the generic class; the East Asian face after |
| vertical spacing | Word's: a paragraph's top margin is its space before **plus** the previous paragraph's space after; `w:contextualSpacing` removes both between paragraphs of one style; only the last block in a container writes a bottom margin. CSS margin collapsing (the larger of the two) is not Word's arithmetic |
| empty paragraph | `<p><br></p>`, so it keeps its line |
| list | nested `<ul class="list">`/`<ol class="list">` by level; each `<li>` takes the label the page prints (`NumberingState`, Symbol/Wingdings mapped) as its `list-style-type` string and the level's indent relative to the list containing it |
| numbered heading | the heading, with its label as a leading `<span class="marker">` |
| table | `<colgroup>` from the grid; width, fixed layout, indent, alignment, cell spacing; `<thead>`/`<th>` only for rows marked as repeating headers; each cell's resolved borders (`nil` as `hidden`), fill, padding, vertical alignment, width, text direction; merges by grid column |
| picture | its extent as `width` and `aspect-ratio`; crop as a clipping frame; rotation, flip, opacity; an anchored picture floats to its side for square/tight/through wrap and stands on its own line for top-and-bottom |
| text box | a box with its size (`width`, `min-height`), fill, outline and inner margins, its vertical anchoring, placed as an anchored drawing is (below); the paragraph holding it is a `<div>`, because a `<p>` cannot hold blocks and a browser would close it early |
| anchored drawing (picture, text box, group) | text wraps around it: floated to its side, wrap distances as margins; text above and below: on its own line; **in front of or behind the text: at its offsets in its paragraph's box** (`position:absolute`, `z-index` over or under), a page-edge offset taken from the text area's edge. A placement that is not the page's — a vertical offset from the page or margin, a float's vertical offset — is reported (`html.anchor_position`) |
| drawn shape, group | an inline `<svg class="drawing">` at the group's extent: each shape is the geometry the page evaluates (`paint_values::shape_content` → the anchor engine's `geometry_content`, with the theme's fill and line when it has none of its own) as `rect`, `ellipse`, `line` or `path`, filled solid or with its linear or radial gradient, outlined with its width and dash; a picture an `<image>`; a text box its frame, with its paragraphs as HTML in a `<foreignObject>` so they wrap and select as text; a nested group a `<g>` through its own child space; rotation and mirroring as transforms about each child's centre. Arrowheads are not drawn (`html.drawing_line_end`) |
| chart, SmartArt, OLE object | the picture Word stored for it, at the object's size (`html.embedded_object_as_picture`) |
| chart with no stored picture | drawn as the page draws it (`paint_values::chart_drawing` → the page's own `compose_chart`, palette and label size) as an inline `<svg class="chart" role="img">` named by the chart's title: bars, lines, markers, gridlines, axes and legend as SVG shapes, every label as SVG `<text>`. A SmartArt or OLE object with no picture is reported (`html.embedded_object`) |
| header and footer | the first section's header **once, above the text** (`<header class="page-header">`) — its first-page header when the section has a distinct first page — and its default footer **once, below everything** (`<footer class="page-footer">`), each resolved like the body. Reported as `html.header_footer_once`: a web page has one top and one bottom, and a page-number field shows the value it was saved with |
| tab stop | from the stops the cascade resolves for the paragraph, tab `k` to stop `k` (`html/tabs.rs`). A line whose last tab goes to a right or decimal stop — a contents line, "name … date" — is a flex row ending exactly at that stop: the text, a leader filler drawn dotted, dashed or ruled as the stop's leader says, and the number. A short line whose every tab reaches a stop — a header's "left · centre · right" — places each segment at its stop: from its left edge, centred on it, or ending at it. The inlines are cut at each tab through hyperlinks, fields, content controls and tracked changes, so a contents entry's link is a link on both sides of its leader. Anything else keeps the default grid (`tab-size`) and is reported (`html.tab_stop`) |
| footnote / endnote | a superscript link labelled as the page labels it (`note_numbering`); the notes written after the body, each linking back |
| page and section | `@page` with the first section's size and margins; the body's measure is that section's text column; a multi-column section is CSS columns; a section, page-break-before or page break becomes `break-before`/`break-after: page` |
| document | `lang` from the document's default language — none claimed when none is declared; its background colour |

### What is still not carried, and is reported

`html.header_footer_once` (written once, not per page; other sections' headers are not
written), `html.watermark`, `html.section_layout` (later sections of another page size),
`html.tab_stop` (a tab line neither layout describes, or a list item's tabs: the default grid),
`html.break_within_paragraph`, `html.anchor_position` (a placement that is not the page's),
`html.table_float_position`, `html.text_box` (Word's shrink-text-on-overflow), `html.group_shape`
(a group with no extent), `html.drawing_line_end` (arrowheads), `html.math_markup` (OMML, not
MathML), `html.embedded_object` (a SmartArt or OLE object with no stored picture),
`html.field_instruction` (the cached result is shown), `html.content_control`,
`html.symbol_font` (only when the symbol map has no Unicode for the glyph), `html.picture_*`,
`html.range_or_comment_marker`, `html.document_author`, `html.source_envelope`,
`html.dangling_*`, `html.alt_chunk`.

### Deliberate differences from Word

- **Line spacing.** Word's single spacing is the font's own ascent, descent and gap; CSS's
  `normal` is the same quantity, but CSS cannot multiply it. A multiple (`1.15`, `1.5`) is
  written against a single line of 1.2 em (`css::SINGLE_LINE`), which is what the faces
  documents name measure to within a few percent.
- **List markers** sit outside the item's content edge, so the item's text starts exactly where
  the page starts it and wrapped lines align beneath it; the marker itself ends at that edge
  rather than starting at the hanging indent.
- **Fonts are named, not embedded.** The file asks for the document's family and then its
  metric-compatible partner (`'Calibri','Carlito',sans-serif`); a reader whose machine has
  neither sees the generic class. Embedding the faces would make every export megabytes
  larger for the reader who already has them; it is an option, not the default.
- **Headers and footers** are written once — the header above the text, the footer below
  everything — because a web page has one top and one bottom. A page-number field in them
  shows the number it was saved with, which is the field's cached result like every other
  field in the export; the report says so (`html.header_footer_once`).
- **Tab `k` goes to stop `k`.** Word moves a tab to the first stop past the text before it,
  which needs that text's width; a web page has no width until it is laid out. On a contents
  line and a header line the two agree, which is why only those lines are laid out by stop. A
  placed segment does not wrap, so a line longer than 120 characters keeps the grid.
- **Chart labels are text, placed by an estimate.** The page shapes each label to centre it;
  the export sets the label as SVG `<text>` (selectable, searchable, read by a screen reader
  through the chart's name) and places it from an average advance of 0.55 em per character,
  which lands within a few percent of the page in the faces documents use.

## 3. Safety

Every declaration is re-serialized from a typed, resolved field. The two pieces of document
text that reach the `<style>` element — a style's font family and its name — are reduced to
characters that cannot end a string, a declaration, an attribute or the element
(`css::font_name`, `css::css_string`, `writer::class_slug`). Guarded by
`a_style_cannot_inject_markup_through_its_font_or_its_name`, which builds a package whose
style's name and font are both `x';}</style><script>…`.

## 4. Measured after

The same twelve documents, exported by the new writer through the app and read in a browser
beside the engine's page 1:

- **styled.docx** — both headings in the style's `#2f5496` at 16pt/13pt bold, the body in its
  near-black; previously black serif at the browser's heading sizes.
- **sample.docx** — Title 30pt bold `#102a43` with its rule, Subtitle italic grey with its
  letter-spacing, the PURPOSE callout as the shaded cell it is (previously a bordered, bold,
  centred `<th>`), the diagram at its 457.2pt width, Heading 1 blue Calibri, the suite table's
  header fill and padding, bulleted and numbered lists with the page's labels and indents.
- **real-producer-table-merges / -rich / -table-list** — merges, nested tables, double grey
  borders and the list labels as the page draws them.
- **real-producer-footnotes** — the note's text, numbered and linked both ways (previously
  dropped).
- **Loss reports** — across the twelve, what remains reported is `html.header_footer_once`
  (3 documents), `html.section_layout` (2), `html.field_instruction` (sample.docx's TOC field,
  whose saved result is a placeholder sentence), `html.anchor_position` (float.docx, whose
  picture is offset from the page) and `html.document_author` (10). `html.paragraph_style` and
  `html.table_formatting`, which every document used to report, no longer exist.
- **real-producer-header-footer / sample.docx** — the header above the text in its own
  style (sample.docx's right-aligned), the footer below the notes.
- **A contents line** — no fixture carries a generated TOC (sample.docx's field holds a
  placeholder), so a four-entry contents and a header line were built and opened in a browser:
  each title, a dotted leader, the page numbers ending together at the right stop, an indented
  entry keeping its indent, the entry's link on both sides of the leader; the header line's
  three parts at the left margin, the centre and the right margin.
- **Drawings** — `shapes`, `preset-shapes`, `custom-geometry`, `grouped-text-boxes`,
  `nested-group` and `chart` (no stored picture) exported and put beside the engine's page:
  the rectangle, ellipse and grouped caption at their offsets in their fills and outlines; the
  presets as the page's grid of rounded rectangles, snips, arrows and stars rather than a
  column; the grouped and nested text boxes at the page's offsets; the chart's bars, target
  line and markers, gridlines, axis labels and legend where the page puts them. Each of these
  was previously absent from the file.

The browser guard `html-export-fidelity.spec.mjs` reads the exported file's **computed**
styles. Run against the previous exporter's output for the same document it reads Title 16px
regular black with no rule, Heading 1 32px black, the header cell unfilled, no list item at
all, and Georgia — every assertion fails; against the new writer, every one holds.

## 5. Complexity

O(nodes) for the walk; a style's class is resolved once; a table's per-cell style layers and
border candidates once per table; a vertical merge looks down its own grid column only. A
paragraph's tab plan is O(inlines + stops); a drawing is O(children × geometry), a preset's
geometry compiled once per process; a chart is O(points + labels). The chart for an embedded
object is one B-tree lookup by its node id, indexed once. No lookup by id inside a loop.
