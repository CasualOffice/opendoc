// Data-grounded DOCX fidelity support matrix (rendered by fidelity.html).
//
// This array is the single source of truth for the public matrix, kept in sync
// with docs/18-SUPPORT-MATRIX.md, the docs/109 backlog (whose open rows each
// note cites as `109` ID), and the docs/55/60 fidelity audits. Each stage value
// must be honest: a construct is only "full" where it is truly implemented and
// exercised, only "editable" where the current editor can actually change it.
// When support advances, update the cell here, in the PR that ships it.
// Last re-verified against main at 0ba6d98d (2026-11-06).
//
// Stage vocabulary (see fidelity.html legend):
//   full        ● implemented and exercised
//   partial     ◐ common cases work; documented gaps remain
//   placeholder ▢ a visible stand-in, not the real content
//   preserved   ⊟ kept losslessly for export but not painted (retention floor)
//   none        ○ not implemented
const FIDELITY = [
  {
    family: "Paragraphs & text",
    note: "Typing, IME, selection, navigation, undo/redo, alignment, indentation, spacing (incl. line-rule atLeast/exact), keepNext/keepLines, widow/orphan and contextualSpacing all render and edit, and the caret on an empty centred or right-aligned paragraph sits where typing will land (HF-121, #912). The only remainders are niche/bounded: document-grid line pitch is applied but character-grid snapping (`w:charSpace`) is not (`109` FID-L-19), and before/after autospacing sizing is a font-size approximation.",
    modeled: "full", rendered: "full", editable: "full", roundtrips: "full",
  },
  {
    family: "Character / run formatting",
    note: "Bold, italic, underline, strike, color, highlight, size, font, super/subscript, small-caps, and run-level shading (w:shd) render and edit. Colored underlines and typed underline styles (double/thick/dotted/dashed/dot-dash/wavy/words-only) render and can be authored. The Font dialog's effects paint as well as round-trip: emphasis marks and a run border (`compose.rs` `compose_emphasis_marks`/`compose_run_border`), outline, shadow, emboss and imprint glyph effects (`casual-doc-render`, #835), all caps, small caps and double strike, and character spacing, scale, position, kerning and ligatures reach the shaper (`font_advanced.mjs`, `batches-3.spec.mjs`). A run's language (`w:lang`) is set from Review ▸ Language and the status bar and is what spell check reads (RM-42, #917, `text-language.spec.mjs`). Rendered stays partial because the Word 2010 text effects (glow, reflection, gradient or outlined `w14` text fill, apart from `w14:noFill`), run-level `w:fitText` and `w:eastAsianLayout` are not painted.",
    modeled: "full", rendered: "partial", editable: "full", roundtrips: "full",
  },
  {
    family: "Paragraph & named styles",
    note: "Apply a paragraph style from the ribbon gallery and reflect the caret\u2019s style; update a style to match the selection and create a new paragraph style from it (`updateStyleFromSelection`/`createStyleFromSelection`, Format \u25b8 Styles and the palette, `ribbon-home.spec.mjs`); and, from Manage styles, set a style\u2019s following-paragraph style, delete an unused style, or restore its saved definition. Editable is partial because there is no Modify Style dialog that edits a definition\u2019s font and paragraph properties directly, a style cannot be renamed or re-based, and only paragraph styles are created.",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
  },
  {
    family: "Tables",
    note: "Insert (by grid, by size, or nested inside a cell), row/column, insert and delete cells with a shift, merge/split cells, split the table (Ctrl/Cmd+Shift+Enter), convert text to a table and back, sort, formula, style and the six style options (`w:tblLook`), the Borders and shading dialog, sizing, Draw table, and cell text direction (#917, #918, `table-tools.spec.mjs`). Positioned tables (`w:tblpPr`) place on the float layer with text wrapping beside them, at the top level of the body, and their wrapping and position are authored from Table properties ▸ Positioning; a positioned table nested in a cell or in a header, and one split across pages, keep the inline behaviour (`109` FID-L-07b). Cell `noWrap` and cell `textDirection` (a quarter turn down or up, `cell_text_direction.rs`) reach layout, and so do a table style’s indent, cell margins, cell spacing, alignment and layout (HF-402). Not yet reaching layout: cell `fitText` and `hideMark`, style-provided row properties, and exact art/compound borders, which fall back to solid (`109` FID-L-13, RM-40).",
    modeled: "full", rendered: "partial", editable: "full", roundtrips: "full",
  },
  {
    family: "Lists & numbering",
    note: "Numbering is fully modeled: multiLevelType, per-level restart, level→pStyle links, numStyleLink/styleLink indirection, full per-instance level/start overrides, and the numFmt vocabulary (incl. spelled-out cardinalText/ordinalText) are typed and round-trip. Rendering now resolves per-instance level/start overrides, numStyleLink/styleLink indirection, and lvlRestart, and paints spelled-out formats — so multi-level and style-based lists label correctly. Picture bullets are typed too: `w:numPicBullet` definitions and `w:lvlPicBulletId` are read (VML and DrawingML; `casual-doc-import/src/picture_bullets.rs`, an unsupported crop or transform is reported and the level keeps its text marker), painted at the hanging indent, written back, and authored from List settings (`setListPictureBullet`, #827, `batches-6-7.spec.mjs` “a custom picture bullet paints its image”). Bullet, numbered and checklist lists are authored from the ribbon, the menus and the context menu, and a second numbered list starts at 1 rather than continuing the first (HF-226, #912). Editable is partial because multilevel authoring is three presets in the List settings dialog, not Word’s multilevel gallery, and the remaining list gaps are enumerated in `docs/142` (`109` HF-227, HF-228).",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
  },
  {
    family: "Images & inline drawings",
    note: "PNG, JPEG, GIF, BMP, TIFF, and WEBP decode and render as true in-flow boxes (with crop/scale) and round-trip. Modeled and round-trip are partial, not full: picture effects (`a:effectLst` shadow, glow, soft edges, reflection) have no field in the model, so import reports a non-empty effect list as a loss and a semantic save drops it (`109` UX-OB-01). SVG vector paths/shapes rasterize on the native build; SVG text — and all SVG on the browser (WASM) build — falls back to a placeholder, as do EMF/WMF metafiles and undecodable images. A picture can be inserted (from a file, the clipboard, a URL through Insert ▸ Pictures ▸ Picture from URL, or Paste as picture; #918), selected, resized, moved, wrapped, reordered, aligned and distributed (Arrange, RM-28), cropped by dragging its handles or with Crop to fill, Crop to fit, Reset crop and Reset to actual size (RM-29, #911), given a border with its colour, weight and dash, replaced in place keeping its size and position (Change picture, `replacePicture`, HF-252), saved out as its own image file (Save as picture), described (alt text), and deleted (`object-editing-gestures.spec.mjs`). Picture transparency (`a:alphaModFix`) renders at the authored alpha and round-trips, so a watermark stays faint instead of painting solid. ROTATION AND FLIP are authored \u2014 a drag handle on the selection, the object chip\u2019s rotate menu and a numeric field in the properties panel, all over the one transform operation that reads and writes a picture\u2019s own `rotation`/`flipH`/`flipV`. Editable stays partial: transparency is drawn but cannot be authored, picture effects (shadow, glow, reflection, soft edges) cannot be added, the alt-text title is not authorable, there is no lock-aspect-ratio control, and a picture inside a header, footer, note or text box cannot be selected as an object (`109` RM-29, UX-OB-01, UX-OB-07, UX-OB-08).",
    modeled: "partial", rendered: "partial", editable: "partial", roundtrips: "partial",
  },
  {
    family: "Text boxes & shapes",
    note: "Shape geometry, fill (solid and multi-stop gradient), outline (color/width/dash/arrowheads), rotation/flip, an outer shadow (`a:outerShdw`, #918), the object’s name and title, and the tight/through wrap contour are typed and round-trip. Geometry means all 187 ECMA-376 preset shapes with their adjustments, drawn from a table generated from the standard’s own definitions, and the whole custom-geometry (custGeom) grammar — guide formulas, arcs, several paths with their own fill and stroke — through the same engine, written back as authored. Semantic-mode round-trip stays partial for the other shape effects (glow, inner shadow, reflection, 3-D), which import reports by name, for linked boxes, and for the drawing ids, which are renumbered. Rendering paints every preset and custom shape with solid/gradient fills, the path’s own shading mode, outlines (dash + head/tail arrowheads on open paths), rotation/flip, blurred outer shadows, and picture-frame borders. A text box’s `bodyPr@vert` and `@rot`, and a group child’s own rotation, now turn its text as well as its frame — laid out, painted and hit-tested (`turned_text_box.rs`, docs/182 G-3, #899). Rendered stays partial: text is contained/clipped to the whole box, not yet the shape’s own text rectangle, so text in an ellipse or a callout runs into the corners (`109` FID-SH-01); linked boxes are not chained; glow and 3-D are not drawn; and an inline text box’s vertical text is not turned. Tight and through wrap follow the authored contour band by band rather than the bounding box. Editing: a text box or preset shape can be inserted, selected, moved, resized, wrapped, reordered, aligned and distributed, and deleted; box CONTENT edits with the full text pipeline (inline, floating, and inside a shape group); a shape inside a group is reached the way Word does it — the first click takes the group as a unit, the next click takes the shape under the pointer, nested groups included, and Escape climbs back out one level at a time. Shape fill (solid and Word’s gradient variations), outline colour, weight, dashes and arrows, Change shape (RM-30, #911), a shadow from Shape Effects ▸ Shadow or its options dialog (#918), ROTATION AND FLIP (a drag handle, the object chip’s rotate menu and a numeric field in the properties panel), and TEXT-BOX BODY PROPERTIES — the four internal margins, the vertical anchor, both overflow modes and autofit (`object-editing-gestures.spec.mjs` “a text box’s body settings apply and read back”) — are all authored. Edit Points drags a shape outline’s vertices (`setShapeEditPoints`, FID-SH-02). Editable stays partial: adjust-handle drags are not there, Edit Points cannot add or delete a point or change a segment, a text box’s vertical text and text rotation cannot be set, texture, pattern and picture fills and line caps/joins are not offered, and Insert ▸ Shapes offers a subset of the presets (`109` FID-SH-04, RM-30).",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "partial",
  },
  {
    family: "Headers & footers",
    note: "Render with per-section widths, first/even/default inheritance, nested blocks, tables, images, and page fields. Fully editable: double-click the band (or the hover marker) to enter, type and format with the same pipeline as the body — including comments and tracked changes — on a contextual Header & Footer ribbon tab with the body dimmed; create a header or footer where none exists, toggle the different-first-page and different-odd-even variants, Link to Previous per band and variant (disabled, with the reason, in the first section), Remove a header or footer, insert a page number at one of six positions or as Page X of Y, and set each section's header and footer distance (HF-192, RM-23, #910, #917, `header-footer-bands.spec.mjs`). Link-to-previous is reference absence, per Word’s model. One layout limit is named rather than hidden: a positioned table (`w:tblpPr`) in a header stays inline (`109` FID-L-07b).",
    modeled: "full", rendered: "full", editable: "full", roundtrips: "full",
  },
  {
    family: "Footnotes & endnotes",
    note: "Reference markers, page-bottom note bands, the separator rule (short for a fresh note, full-width for a continuation), and the in-body auto-number glyph all render — with space reservation, per-column bands, cross-page continuation, end-of-document and end-of-section endnote placement, and the authored number format, start value and restart (continuous, each section, each page; `note_numbering.rs`, `tests/note_options.rs`). A footnote or endnote can be inserted from the Insert tab, the menu or the palette, and its body edits like any other surface. From References ▸ Notes, the menu and the palette: Word’s Footnote and Endnote dialog (location, number format, start at, numbering, apply to this section or the whole document; `setNoteSettings`), Convert notes between footnotes and endnotes for the document or the selection as one undo step (`convertNotes`), and go to the next or previous footnote or endnote (RM-24, #917, `note-commands.spec.mjs`). The one gap is the separator: an authored `w:separator` or `w:continuationSeparator` note is not modeled — the stock rule is drawn in its place, a customised one is reported as a loss on import and is not written back by a semantic save, and nothing edits it — which is why modeled and round-trip are partial and editable is not full (`109` RM-24).",
    modeled: "partial", rendered: "partial", editable: "partial", roundtrips: "partial",
  },
  {
    family: "Sections, columns & page setup",
    note: "Multi-section geometry and columns render, and so do page borders (`w:pgBorders`, `page_border.rs`). Editable: page size, margins, orientation, columns (unequal widths too), paper and margin presets, mirrored margins, gutter, page colour, vertical alignment, header/footer distance and page-number format and start (`section-properties.spec.mjs`, RM-22); page borders from Word’s Page Border dialog, without art, shadow and 3-D (`setPageBorders`, HF-193, #912, `page-borders.spec.mjs`); and section breaks — next page, continuous, even and odd page — with page and column breaks and Blank Page, from Insert ▸ Breaks, the Layout band, the palette and Ctrl/Cmd+Enter (`insertSectionBreak`, HF-203, `breaks.spec.mjs` “a section break really creates a section”). Rendered stays partial: column balancing balances only the last page of a non-final section, a next-column break is treated as continuous (`109` FID-L-11), and a section’s own `w:textDirection` is not consumed. Editable stays partial: a document with no `w:sectPr` refuses page setup and section breaks (`109` HF-206), and the RM-22 remainders (top gutter, named unequal-column presets) are not offered.",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Fields",
    note: "PAGE / NUMPAGES recompute; every other field shows its cached result, which flows and wraps like text — only a paragraph holding a PAGE or NUMPAGES marker is shaped without soft wrap. Insert ▸ Field… inserts a page, page-count, date, time, file-name or author field, and the header and footer band bar inserts Page X of Y (`insertField`, `field-insert.spec.mjs`); the table of contents, captions and cross-references are their own family below. A legacy form field is operable: a FORMCHECKBOX (`w:ffData/w:checkBox`) paints its box from its own state and ticks on click or Space as one undo step, and text typed at a FORMTEXT lands INSIDE the field, so it is the field's result on save rather than a run beside it. A field's result is part of the paragraph's text for the caret, hit-testing and the accessibility mirror. Editable is partial: dropdown form fields (`w:ddList`) cannot be operated, there is no Update all fields, no field code can be shown or edited, and the field catalogue is the six kinds above (`109` RM-33).",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Table of contents, captions & cross-references",
    note: "A table of contents (a TOC field, usually in a `w:sdt` gallery wrapper), SEQ captions and REF/PAGEREF cross-references are fields, typed, drawn from their cached results and written back. Authored from the References band, the menu and the palette (RM-25, #917): insert a table of contents with Word's settings (levels, page numbers, right-aligned numbers, tab leader, format, hyperlinks), reopen the settings read back from the document, update page numbers only or the entire table, Remove, Add text (Do not show and nine levels) and a table of figures (`insertTableOfContentsWith`, `updateTableOfContents`, `table-of-contents.spec.mjs`); insert a caption with its label, numbering and position, add a label, and Update caption numbers (`insertCaption`, `updateCaptionNumbers`); insert a cross-reference to a heading, bookmark, footnote, endnote or caption as its text, its label and number, its page number or above/below, optionally as a link (`insertCrossReference`, `references-captions.spec.mjs` “a cross-reference to a caption inserts, and ONE undo removes it”). A hyperlinked contents entry plants or reuses a `_Toc` bookmark and a cross-reference a `_Ref` one. A page-number reference goes stale after a reflow until it is updated, as it does in Word. Editable is partial: Update fields refreshes the contents only, not every field (`109` RM-33), and the Numbered item target of Word's cross-reference dialog is not offered (`docs/153` “Cross-reference to a numbered item”, `109` RM-25).",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
  },
  {
    family: "Document protection & forms",
    note: "`w:documentProtection w:edit=\"forms\" w:enforcement=\"1\"` is enforced: every edit outside a form field is refused, and the refusal says the document is protected rather than blaming the selection. Filling the form \u2014 typing in a FORMTEXT, ticking a FORMCHECKBOX \u2014 is what protection exists to allow, so it still works; so does filling an unlocked content control that takes text, and a control whose content is locked refuses the edit (#918). Insert ▸ Controls ▸ Protect form writes this restriction, and Form roles limits each reader to the fields of their role while the form is protected — roles are kept in the browser, not in the .docx (`content-controls.spec.mjs` “Protect form leaves only the fields editable”, “a form role fills only its own fields”). The other `w:edit` modes are enforced too, at the operation and not in the toolbar (ADR-052): `readOnly` refuses every edit, `comments` allows an operation only if stripping comment markers from it leaves the text unchanged, and `trackedChanges` allows one only if the text as it stood before the change is unchanged and no revision has gone missing \u2014 so accepting, rejecting or untracked typing is refused while suggesting is not. `w:formatting` \u2014 the SECOND restriction `w:documentProtection` carries, Word's \"only allow formatting with unlocked styles\" \u2014 is enforced too, at the same operation: direct character, paragraph, table, list and shape formatting is refused, a style the document locked with `w:style/@w:locked` cannot be applied, a style definition cannot be rewritten (which would reformat every paragraph using it, so allowing it would make the restriction bypassable), and applying an unlocked style is allowed because that is the one change the restriction exists to permit. A style the style table does not define is graded by `w:latentStyles/@w:defLockedState`. The axes are independent, so a document whose `w:edit` restricts nothing and whose `w:formatting` restricts everything refuses a formatting gesture and takes a keystroke. Two operations are NOT covered and the gap is named rather than hidden: `SetInlines` and `ReplaceTable` are inverse vehicles, undo runs through the same choke point, and refusing them would refuse the undo of a content edit the restriction permits \u2014 so a caller that hand-builds one can still reformat, which no editing gesture in this build does. Page and section setup is deliberately not treated as formatting. What is still NOT enforced is Word's obligation that a `trackedChanges`-protected document cannot have tracking turned off: it does not force the editor into Suggesting on open as Word does, and the Track Changes control stays live. `w:permStart`/`w:permEnd` editable ranges are NOT modeled at all \u2014 they are reported as a loss and dropped, not round-tripped \u2014 and that one construct is why modeling and round-trip are both graded partial rather than full. The other two constructs that used to share that reason are now typed: `w:sectPr/w:formProt` is a per-section table (`Definitions::section_form_protection`, `109` FID-AT-06), so a section Word left open for free editing stays open and a save keeps it; and the sixteen `AG_Password` and Office-2010 `AG_TransitionalPassword` attributes (`w:hash`, `w:salt`, `w:cryptSpinCount`, `w:algorithmName`, `w:hashValue`, `w:saltValue`, `w:spinCount`, \u2026) on `w:documentProtection` and `w:writeProtection` are carried verbatim (`PasswordVerifier`, FID-AT-14), so an edited save keeps a Word password. The Restrict Editing dialog sets one too, as the SHA-512 verifier Word 2013+ writes, and changing or stopping a passworded restriction asks for the password and checks it (SHA-1, SHA-256, SHA-384 and SHA-512 verifiers in either attribute set); only a verifier this build cannot check is removed by a change, and the dialog says so before Apply. A password is a deterrent and not encryption: the verifier is plain XML next to the restriction, so protection here is an authoring contract, not a security boundary. Rendered is none because a restriction has nothing of its own to draw on the page; the editor states it in its chrome instead (the access badge and the refusal message). The open work — editable ranges, and Suggesting forced on a `trackedChanges`-protected document — is `109` RM-32.",
    modeled: "partial", rendered: "none", editable: "partial", roundtrips: "partial",
  },
  {
    family: "Math (OMML)",
    note: "The full OMML math element set is typed — rows/text, fractions, sub/superscripts and pre-scripts, radicals, delimiters, functions, n-ary operators, matrices, equation arrays, accents, bars, limits, group-characters, and box/border-box wrappers — and the raw OMML is preserved verbatim (rare constructs like phantom spacing stay raw-retained, still lossless). Rendering paints the common arms inline; box/border-box border rules and full Word-parity typesetting remain partial (`109` RM-38). Not editable: there is no equation editor (`109` OO-009, RM-39).",
    modeled: "full", rendered: "partial", editable: "none", roundtrips: "full",
  },
  {
    family: "Charts",
    note: "Six classic families are typed, PAINTED and written: `c:barChart` (bar and column), `c:lineChart` (including `c:smooth`, sampled as a polyline), `c:areaChart`, `c:pieChart`, `c:doughnutChart` and `c:scatterChart`. For those the cached data tables, series fill/outline, data labels, legend, axes, scaling and tick marks are modeled, drawn by the engine as a real chart — not a placeholder — and serialized back to a `word/charts/chartN.xml` part with its content-type override, its document relationship, and its own `_rels` when the chart names an embedded workbook the package carries. Every OTHER family declines projection and survives as a byte-preserved reference with a text placeholder in its place; enumerated rather than summarized, because an unlisted family is a claim by omission: `c:bar3DChart`, `c:line3DChart`, `c:area3DChart`, `c:pie3DChart`, `c:surface3DChart` (every `*3DChart`), `c:surfaceChart`, `c:stockChart`, `c:radarChart`, `c:bubbleChart` and `c:ofPieChart` (pie-of-pie and bar-of-pie). The `cx1`…`cx8` chartex vocabularies — treemap, sunburst, histogram, box-and-whisker, waterfall, funnel, map — are not read at all, so what is modeled for one of those is the classic `mc:Fallback` chart Word writes beside it. Round-trip is therefore partial, not full: the six typed families survive a semantic-mode save on their own, the rest survive only through retention, and a label range's number-format code has no spelling in a string cache. Editing is partial, and broad: the chart's data, its type and its elements are editable on the surfaces ONLYOFFICE gives a chart — a contextual Chart ribbon tab, a Chart settings panel, the chart's right-click menu and the command palette, all running one command tree. Insert ▸ Chart opens a picker of all seventeen types, each also a palette row (HF-258, #911), creates the chosen chart and opens the Chart Data window on it — Word's Edit Data, ONLYOFFICE's chart editor: a grid (rows are categories, columns are series) that applies as each cell is committed, one undo step per cell, grows and shrinks by row and by series, and takes a tab-separated block pasted from a spreadsheet. The type is one of seventeen: clustered, stacked and 100% stacked column, bar, line and area, a line with markers, pie, doughnut, and scatter with straight or smooth lines. The elements: the title (none, above, or overlaid), the legend (none or five positions, optionally over the plot), data labels with the positions the family admits, each axis shown or hidden, horizontal and vertical gridlines, and a value axis's minimum, maximum and reverse order. A style gallery recolours the series in the theme's colourful rotation or in one of six monochrome palettes, drawn in the document's own theme colours. Saving writes the grid back as the chart's cache AND as a values-only embedded workbook (`word/embeddings/*.xlsx`) the series name with `c:f` references, so Word's own Edit Data opens the numbers the chart shows. A chart Word wrote is editable: every construct the model does not hold — gridline colours, the style choice, a data point's own fill — is carried verbatim on the element it came from and written back in schema order when the chart is saved, so an edit loses none of it; replacing a workbook the chart named is said in the panel before the first change and named in the export report. Formatting is editable too, from the Chart elements menu and the settings panel's Series, Text and axis sections: one series' colour, line width and dash; the value axis's and each series' number format, drawn in tick and data labels; the font face, size, bold, italic and colour of the whole chart, its title, legend and each axis; axis titles, a vertical one drawn turned a quarter; combination charts of column, line and area series, any of them on a secondary value axis, with their data still editable; trendlines (linear, exponential, logarithmic, power, polynomial and moving average, with the equation and R²) and error bars (fixed, percentage, standard deviation and standard error) — each read from the file, drawn, and written back. A floating chart (`wp:anchor`) opens floating, wraps, is placed on the float layer, selects as a chart so the Chart tab opens on it, can be re-wrapped, and saves back as an anchor (HF-260, #917, `floating_chart_tests.rs`); converting an in-line chart to floating, and resize grips on a floating chart, are not there. A chart is read-only, with the reason, only when it carries something that cannot be put back where it came from, or mixes a scatter series with another type. What is NOT editable: data-label fonts, and custom per-point error-bar lengths, which are read, drawn and saved but shown read-only, and Save as picture does not cover a chart (`109` RM-35, RM-29). The engine's insert path refuses a family it cannot paint with a reason rather than inserting a label. No preview bitmap is ever used — the importer passes none, and Word writes none for a classic chart part.",
    modeled: "partial", rendered: "partial", editable: "partial", roundtrips: "partial",
  },
  {
    family: "SmartArt",
    note: "Preserved byte-for-byte on export as an opaque reference. NOT modeled: nothing under word/diagrams is parsed, so there is no typed diagram — only the relationship. Not rendered as a diagram: in line a `[diagram]` text placeholder stands in; a floating one keeps its `wp:anchor`, saves it back, and is placed as an empty selectable frame (#917). Neither can be inserted or edited (`109` RM-38, RM-39). No preview bitmap is ever used — the importer passes none.",
    modeled: "none", rendered: "preserved", editable: "none", roundtrips: "full",
  },
  {
    family: "VML pictures & shapes",
    note: "Legacy VML is normalised at import into the same DrawingML model the renderer draws (ADR-072), not given a renderer of its own. Drawn: `v:rect`, `v:roundrect` (with its `arcsize`), `v:oval`, `v:line` in its own direction, a `v:shape` from its own `path` or its `v:shapetype`'s, evaluated through `v:formulas` and `adj` (arcs and quadrants included), a `v:group` as ONE positioned object holding its shapes, pictures and text boxes, `v:textbox` content through the shared block pipeline, `v:imagedata` pictures, solid, gradient and picture fills, `v:stroke` colour, weight, dash and arrowheads, flips and rotation, and `w10:wrap` mode, side and anchor frames; an `mc:AlternateContent` draws its DrawingML choice and never the VML fallback too. Rendered is partial, enumerated: the `ae`/`al` angle-ellipse path forms and the `h*` extensions fall back to the shape's bounding-box outline; `v:polyline`/`v:curve`/`v:arc` point lists are not read (box outline); a pattern fill is drawn as its colour; `v:shadow`, `v:extrusion`, `v:skew` and compound strokes are not drawn; a body group or text box that wraps by the side, or is placed against the page, is not floated (its text boxes stay inline). The compatibility report lists drawn VML as shown approximately. Round trip is full: an unedited file saves byte-identical, and an edited one writes every untouched VML shape back as its own source, text-box content regenerated into it; a shape whose drawing was changed is saved as DrawingML and the save names it. Not editable as VML (`109` RM-39).",
    modeled: "full", rendered: "partial", editable: "none", roundtrips: "full",
  },
  {
    family: "Comments",
    note: "Editor sidebar with anchored highlights; add (over a range that may span paragraphs, `multi-paragraph-comment.spec.mjs`), reply, @mention, resolve/reopen, edit, delete; Open/Resolved/All filtering; valid thread ids on export. Comments can be added in any surface, including headers, footers, notes and text boxes. The Display for review choice governs their marks: No markup hides them, Simple markup draws them as overlay, All markup draws the engine’s highlight (`review_display.mjs`, #917). Not Word/Docs parity: there is no Resolve all, and the engine paints a comment highlight only in the All markup layout (`flow.rs`, `ReviewView::Markup`); in the default view the highlight and sidebar are host/DOM chrome, so comments contribute nothing to the display list or to printed page output (`109` RM-31).",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Tracked changes",
    note: "Inline markup with per-author color; suggesting mode, accept/reject single/group/all, a review sidebar with Open/Resolved/All filtering and next/previous navigation, and round-trip with numeric ids. Changes can be authored in any surface, including headers, footers, notes and text boxes. Display for review offers Word’s four views — Simple markup (change bars; a click on one switches to All markup), All markup, No markup, and Original, which is the document with every change rejected, shown read-only — from the Review band, the Review menu and the palette (RM-31, #917, `review-display.spec.mjs`). A commentor may suggest but not decide (HF-199). Paragraph-level changes from Word documents — an added or deleted paragraph break (`w:pPr/w:rPr/w:ins|w:del`) and a paragraph formatting change (`w:pPrChange`) — are listed, drawn (a pilcrow at the mark, a margin bar for formatting), reachable with next/previous, and decided with Word’s rule that a join keeps the following paragraph’s formatting. Suggesting mode authors structural changes too: Enter, joining paragraphs, deleting across a paragraph boundary, paragraph formatting, an underline style or colour (HF-255) and Convert text to table are recorded as tracked revisions rather than refused, and a table inserted or deleted whole is one review item, listed and decided (`table_review.rs`). Not Word/Docs parity: table STRUCTURE changes (insert/delete row or column, merge cells, insert a table) and list level from the Tab key are still refused while suggesting (`suggesting-mode-gate.spec.mjs`); and a table-property revision (`w:tblPrChange`, `w:trPrChange`, `w:tcPrChange`), a cell insertion, deletion or merge, and a table only some of whose rows are marked are imported but neither listed nor decidable (`109` RM-31).",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Bookmarks & hyperlinks",
    note: "Links render, activate, and drive TOC navigation; insert/edit/remove a link. Bookmarks have a manager surface — create, rename, delete, and go to. A hyperlink on a PICTURE or shape (`a:hlinkClick`) is modeled on every object kind that can carry one, round-trips with one relationship per distinct URL, and is offered through the same chip a linked word is — including on a linked child inside a group, which is how Word writes a linked group. Adding or editing a link on a picture is not built yet: an imported one is honoured, a new one cannot be authored (`109` RM-39). A hyperlinked table of contents plants or reuses `_Toc` bookmarks and a cross-reference `_Ref` ones, reusing an existing bookmark on the same target. The pointer does not change over a linked picture, matching ONLYOFFICE and Word.",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
  },
  {
    family: "Content controls (w:sdt)",
    note: "SDT wrappers model and round-trip; content flows and edits as ordinary paragraphs. Content controls are authored and operated (#918): Insert ▸ Controls inserts a rich-text, plain-text, picture, checkbox, combo-box, drop-down list or date control (`insertContentControl`); a control inserted here holds Word’s grey placeholder text, marked `w:showingPlcHdr`, until the first keystroke replaces it; Properties sets its title, tag, colour, locks, list entries, date format and placeholder; and a click or Alt+Down on a drop-down, combo, date or picture control opens its chooser, as does Choose a value on its right-click menu (`fillContentControl`, `content-controls.spec.mjs` “a drop-down list is configured in its properties and filled with Alt+Down”). Locks are enforced, and typing at the end of an inline control lands inside it. A checkbox control paints its checked/unchecked glyph and ticks on a click or Space, one undo step, swapping the declared `w14:checkedState`/`w14:uncheckedState` glyph. Rendered is partial because the control’s chrome — the boundary and title tab while the caret is inside, and the choosers themselves — is host chrome, not painted by the engine (`109` RM-34). Editable is partial: control properties and choosers are refused while Suggesting, and a break inside a body-level control is refused (`109` HF-205).",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Fonts, fallback & color glyphs",
    note: "24 Latin faces are embedded in the native engine (Roboto, Carlito, Caladea, and Liberation Sans, Serif and Mono, four styles each); the browser build embeds the 20 metric-compatible ones and serves Roboto, Noto Sans and Noto Serif itself. Both embed a monochrome Noto Emoji face and a Noto Sans Symbols 2 subset (arrows, technical and geometric symbols, ballot boxes and check marks; `fonts.rs`, `tests/emoji_clusters.rs`). The editor also serves, from its own origin, fallback faces for Arabic, Hebrew, Thai, Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu, Kannada, Malayalam and Sinhala (`web_fonts.mjs`); CJK faces and colour emoji are fetched from a CDN, hash-verified, rather than bundled (`109` FONT-03), and about thirty scripts still have no face (`109` FONT-02). Hosts can register more through the font registry (the desktop story is OS fonts, the browser story is network-fetched faces). Whole-face substitution is metric-compatible with LibreOffice's choices (Arial→Liberation Sans, Calibri→Carlito, Cambria→Caladea) and consults the font table's declared `w:family` class as well as the name; per-glyph coverage fallback runs through the face index. Color glyphs render: sbix/CBDT bitmap strikes and COLR v0/v1 paint graphs are tried before monochrome outlines. Fonts embedded in the document (`w:embedRegular` and its bold/italic slots, the obfuscated `.odttf` parts) are de-obfuscated and registered under the document’s own family name, so they outrank the metric substitute; a corrupt or missing embedded face falls back to substitution and is reported. A PDF's embedded TrueType programs are kept the same way when a PDF is opened, and saved into a DOCX as obfuscated `.odttf` parts (docs/182 G-1). One real gap: the modeled PANOSE/altName/signature hints are not consulted. Nothing in the editor embeds a font on save or uploads one (`109` RM-39).",
    modeled: "full", rendered: "full", editable: "none", roundtrips: "full",
  },
  {
    family: "Hyphenation",
    note: "Not implemented in layout: there is no hyphenator and no dictionary, and nothing consumes `w:autoHyphenation`, `w:hyphenationZone`, `w:consecutiveHyphenLimit` or `w:doNotHyphenateCaps`; only `w:suppressAutoHyphens` is cascaded, and it has nothing to suppress. A document authored with automatic hyphenation on will break its lines differently from Word and therefore paginate differently. The model side is complete: all four settings and `w:suppressAutoHyphens` are typed (`Definitions` hyphenation fields), and the semantic writer puts every one of them back (`casual-doc-export/src/semantic.rs`), so a save never loses them. Nothing in the editor sets them. Open: `109` FID-L-02 and RM-36 (a pattern hyphenator honouring zone, limit and caps; the Hyphenation control; optional hyphens shown and honoured).",
    modeled: "full", rendered: "none", editable: "none", roundtrips: "full",
  },
  {
    family: "Line numbering (w:lnNumType)",
    note: "Painted in the margin by a post-pagination pass (`line_number.rs`), honouring start, count-by, distance, restart per page / section / continuous, and `w:suppressLineNumbers` from a paragraph or its style. Numbers are page furniture, not text: a click in the margin lands in the body and copying a paragraph never copies its number. One deliberate gap: lines inside table cells are not numbered yet (Word numbers them), because that needs a row-wise ordering rule rather than a guess (`109` RM-38). Authorable from Layout \u25b8 Line numbers: off, continuous, restart each page or section, suppress for the current paragraph, and start, count-by and distance (`page_setup.mjs`, `line-numbers.spec.mjs`). Editable is partial rather than full because the change always applies to the caret's section: Word's Apply to (whole document, or this point forward) is not offered. Common in legal pleadings and contract redlines.",
    modeled: "full", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Watermarks & WordArt",
    note: "Watermarks are modeled and painted; WordArt is not. A watermark is a section property (`SectionBoundary.watermark`) rather than the header shape Word stores it as, and a post-pagination pass stamps it behind every page of its section — text or washed-out picture, diagonal or horizontal, semitransparent, at the authored font, size and colour. DOCX import recognises Word's own watermark (the `PowerPlusWaterMarkObject` / `WordPictureWatermark` shape ids, and a header-borne `#_x0000_t136` text path for other producers), reads the words out of `v:textpath@string`, and lifts the shape off the float layer so the stamp is not painted twice; a shape that looks like a watermark but cannot be lifted is reported and left as a float, never dropped. On save the semantic DOCX writer puts Word's own shape back: it synthesises the section's header part and writes `PowerPlusWaterMarkObject` as a `_x0000_t136` text path, or `WordPictureWatermark` for a picture (`casual-doc-export/src/semantic.rs` `watermark_plan`, `a_word_watermark_survives_the_round_trip`). Three known gaps keep the cells partial: a DrawingML watermark (`a:prstTxWarp`) is not recognised and stays an ordinary shape; a picture watermark's authored scale becomes Word's \"Auto\" because the natural size is not available at import, so it does not come back as authored; and only a text watermark can be authored \u2014 Layout \u25b8 Watermark sets the words, font, size, colour, layout and transparency (`watermark.spec.mjs`), while the picture option is disabled because the editor cannot register an image for it yet, which is why editable is partial. General WordArt text paths (arched, curved, waved presets) are neither typed nor painted (`109` RM-39).",
    modeled: "partial", rendered: "partial", editable: "partial", roundtrips: "partial",
  },
  {
    family: "Bidi, RTL & CJK grid",
    note: "Paragraph and selected-run LTR/RTL can now be authored in the paragraph panel (batch 5), with undo and DOCX round trip; CJK document-grid authoring remains unavailable, so editable stays partial. Paragraph base direction (`w:bidi`) drives the alignment edge, `w:bidiVisual` mirrors table grid/margin/border geometry, and the Unicode bidi algorithm runs per line through the shaper. Two known limits: the shaper exposes no way to force a paragraph's base level, so an RTL paragraph whose text carries no strong RTL character reorders LTR; and the resolved level is collapsed to a single RTL flag, discarding embedding depth. `w:docGrid` line pitch reaches layout with exact/paragraph/table precedence and is gated by `w:snapToGrid`; character-grid snapping (`w:charSpace`, linesAndChars) is not applied. `w:jc=\"distribute\"` currently collapses to ordinary justification, and there is no kashida or CJK inter-character distribution.",
    modeled: "partial", rendered: "partial", editable: "partial", roundtrips: "partial",
  },
  {
    family: "Vertical & rotated text",
    note: "Partly implemented, through quarter turns rather than a second writing mode. A table cell's `w:textDirection` (tbRl, btLr) lays its text out upright at the cell's height and paints it turned down or up, with the caret and clicks hit-testing through the turn (`flow.rs` `CellTurn`, `crates/casual-doc-layout/tests/cell_text_direction.rs`), and it is authored from the Table band's Text direction, the cell's right-click menu and the palette (`setCellTextDirection`, RM-37, #917, `table-tools.spec.mjs` “text direction is chosen from the Table band”). A floating text box's `bodyPr@vert` and `@rot`, and a group child's own rotation, turn its text the same way (`turned_text_box.rs`, docs/182 G-3, #899). Not implemented: a section's or paragraph's `w:textDirection` is never consumed, so there is no vertical page or body text; `eaVert` is a plain quarter turn, with no upright CJK glyphs; an inline text box's vertical text is not turned; and a text box's direction and rotation cannot be set in the editor. Values round-trip. Open: `109` RM-37, FID-L-08.",
    modeled: "partial", rendered: "partial", editable: "partial", roundtrips: "full",
  },
  {
    family: "Drop caps",
    note: "Implemented: a dropped cap becomes a real paragraph-float exclusion that following lines flow around, and margin-position drop caps offset instead. Gated to the Word-shaped case \u2014 a single-character drop-cap paragraph followed by a body paragraph. Authorable from Insert \u25b8 Drop cap (`drop_cap.mjs`): none, dropped or in the margin, and the number of lines. Editable is partial because Word's drop-cap font and distance from text are not offered (`109` OO-006, RM-39).",
    modeled: "full", rendered: "full", editable: "partial", roundtrips: "full",
  },
];

const FIDELITY_STAGE = {
  full: { glyph: "●", label: "Full" }, // ●
  partial: { glyph: "◐", label: "Partial" }, // ◐
  placeholder: { glyph: "▢", label: "Placeholder" }, // ▢
  preserved: { glyph: "⊟", label: "Preserved" }, // ⊟
  none: { glyph: "○", label: "Not yet" }, // ○
};

/** Renders the FIDELITY data into the #fidelity-body table body. No-op outside
 *  a browser (so the data can be imported by a Node drift check). */
function renderFidelity() {
  const body = document.getElementById("fidelity-body");
  if (!body) return;
  const frag = document.createDocumentFragment();
  for (const row of FIDELITY) {
    const tr = document.createElement("tr");

    const th = document.createElement("th");
    th.setAttribute("scope", "row");
    th.textContent = row.family;
    if (row.note) {
      const small = document.createElement("small");
      small.textContent = row.note;
      th.appendChild(small);
    }
    tr.appendChild(th);

    for (const stage of ["modeled", "rendered", "editable", "roundtrips"]) {
      const td = document.createElement("td");
      td.className = "cell";
      const value = row[stage];
      const info = FIDELITY_STAGE[value] || FIDELITY_STAGE.none;
      const mark = document.createElement("span");
      mark.className = `fmark ${value}`;
      const glyph = document.createElement("span");
      glyph.className = "glyph";
      glyph.setAttribute("aria-hidden", "true");
      glyph.textContent = info.glyph;
      mark.append(glyph, document.createTextNode(info.label));
      td.appendChild(mark);
      tr.appendChild(td);
    }
    frag.appendChild(tr);
  }
  body.replaceChildren(frag);
}

// Per-format pipeline support (validation / import / export / browser host).
// Kept deliberately conservative so the fidelity story never overstates a format;
// a drift check asserts ODT stays "partial" until a family is genuinely complete.
const FORMAT_SUPPORT = [
  {
    format: "DOCX",
    note: "Bounded OOXML admission, semantic import/export, and the browser editor path are implemented and tested.",
    validation: "full",
    import: "full",
    export: "full",
    host: "full",
  },
  {
    format: "Normalized JSON",
    note: "Strict bounded schema-v1 validation and deterministic compact export are available through the generic WASM API and the browser Open/Save controls.",
    validation: "full",
    import: "full",
    export: "full",
    host: "full",
  },
  {
    format: "Plain text",
    note: "Bounded UTF-8 import, canonical LF export, exact retained unchanged bytes, and loss reporting are available through the generic WASM API and the browser Open/Save controls.",
    validation: "full",
    import: "full",
    export: "full",
    host: "full",
  },
  {
    format: "ODT",
    note: "ODF 1.2–1.4 admission plus generic WASM/browser Open and Save are implemented. Core text, safe links, bookmarks, bounded direct/named styles, nested bullet/number lists, recursive tables, typed footnotes/endnotes, and authored citation labels map through the normalized model and a matching writer to a byte-exact fixed point, and unsafe merge geometry is visibly projected and reported rather than dropped. Import/export and therefore host support remain partial: advanced list continuation/item overrides and label layout, broader style/table properties, media beyond references, and full conformance remain in progress.",
    validation: "full",
    import: "partial",
    export: "partial",
    host: "partial",
  },
  {
    format: "RTF",
    note: "Import only. A bounded RTF reader (`casual-doc-rtf`, its own limits and an `rtf_import` fuzz target) maps text, formatting, lists and tables into the model, and the editor opens a .rtf through File \u25b8 Open (`rtf-open.spec.mjs`). There is no RTF writer (`109` HF-160, RM-44), an imported heading loses its outline level (`109` FOLD-005), and double-byte CJK code pages lose most of their text (`109` HF-160).",
    validation: "full",
    import: "partial",
    export: "none",
    host: "partial",
  },
  {
    format: "PDF",
    note: "Open and save, pre-release (`docs/182`, RM-49). Admission is bounded (`casual-doc-pdf-read` limits, encrypted files refused, a `pdf_import` fuzz target). In the editor a PDF opens as a flowing document rebuilt from its layout \u2014 paragraphs, headings, lists, tables, header and footer, footnotes \u2014 through a second WebAssembly module loaded on first use; File \u25b8 PDF \u25b8 Keep exact page layout reopens it with every page as drawn, the only layout whose save goes back into the original (byte for byte when unedited, an incremental update when edited), and scanned pages are recognised by OCR in a worker when the host supplies models (`pdf-import.spec.mjs`, `pdf-layout-switch.spec.mjs`). Export is a real-text vector PDF with subset fonts (`docs/98`). Import is a reconstruction, so it is partial by nature; the not-built lists are `docs/182` \u00a716.4\u2013\u00a720.6. Export lacks tagged PDF, PDF/A, outlines and link annotations (`109` RM-44).",
    validation: "full",
    import: "partial",
    export: "partial",
    host: "partial",
  },
  {
    format: "HTML",
    note: "Open and save through the built-in registry. Import is a bounded tokenizer over the paste path\u2019s mapping (headings, lists, tables, emphasis, links, code, quotes); scripts, styles and embedded frames are skipped unparsed, nothing is fetched, and input over its size or nesting limits is refused. Save writes one self-contained file (`html-export-fidelity.spec.mjs`). Partial both ways: `rowspan` degrades, a remote picture is reported rather than fetched, and an HTML file is never saved back byte for byte (`109` RM-06).",
    validation: "full",
    import: "partial",
    export: "partial",
    host: "partial",
  },
  {
    format: "Markdown",
    note: "Open and save through the built-in registry. Import reads CommonMark with GFM tables, task lists, strikethrough and footnotes into real headings, lists, tables, code, quotes, checkbox content controls and footnotes (`casual-doc-io/src/markdown_import.rs`, `markdown_import_tests.rs`); Markdown \u2192 model \u2192 Markdown \u2192 model is a tested fixed point for what Markdown expresses. Partial both ways because most of a document has no Markdown spelling: raw HTML is reported and omitted, a remote picture is reported rather than fetched, and the writer cannot carry layout, styles or objects (`109` RM-06).",
    validation: "full",
    import: "partial",
    export: "partial",
    host: "partial",
  },
];

if (typeof document !== "undefined") {
  renderFidelity();
}

// Expose the data (not the DOM render) to a Node drift check.
if (typeof module !== "undefined" && module.exports) {
  module.exports = { FIDELITY, FIDELITY_STAGE, FORMAT_SUPPORT };
}
