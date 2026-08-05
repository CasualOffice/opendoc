# Changelog

All notable user-visible, integrator-visible, compatibility, security, and
migration changes are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
OpenDoc will use semantic versioning when its public package line begins.

## Unreleased

### OpenDocument Text (ODT) fidelity — 2026-08-05

The bounded ODT adapter (`casual-doc-odf`, `casual-doc-io::OdtAdapter`) gained
several import/export families, each landed as a reviewed increment and disclosed
against the profiles in `docs/95`/`docs/96`/`docs/97`. Still a bounded subset, not
a general ODT support claim.

- **Master-page header/footer**: `style:header`/`style:footer` (and the `-left`
  even-page variants) map to schema-v1 `HeaderFooter` definitions and section
  references and are re-emitted, as a byte + semantic fixed point.
- **Document style defaults**: `office:styles` `style:default-style` (paragraph +
  text families, including a paragraph default's run text-properties) map to the
  model's `DocumentDefaults` cascade base and are re-emitted.
- **List start-value overrides**: a `text:list-item` `text:start-value` maps to a
  per-instance numbering start override (out-of-range/invalid values degrade the
  item rather than failing the import).
- **Embedded images**: an inline `draw:frame`+`draw:image` maps to a `Drawing`
  node plus a reference-only `MediaReference` (no image bytes are decoded or held
  in the model). `xlink:href` is validated as a safe internal package part —
  external/linked URLs, traversal, absolute paths, drive letters, schemes, and
  over-long/control-char names are blocked without fetching. The manifest is
  authoritative for media type, and a missing image part is reported.
- **Edit-tolerant preservation** (`docs/97`): under `retain_source`, referenced
  image bytes are retained (bounded, opaque) in the source envelope; a
  `PreserveWhenSafe` export now re-emits `draw:frame` and repackages those bytes
  with deterministic manifest entries, so images survive a semantic edit and
  reopen as a byte + semantic fixed point. Safe *unknown* non-semantic parts
  (thumbnails, settings, configurations, unreferenced pictures) are also retained
  and carried verbatim through an edit. Reserved/active-content and orphaned
  parts are never repackaged. New public API: `OdfRetainedParts`, `RetainedPart`,
  `OdtPackage::retained_media_parts`, `write_odt_with_retained_parts`,
  `referenced_retained_parts`; new `OdfImportLimits` retained-part bounds; the ODT
  adapter now advertises `preserve_when_safe`.
- **Real-producer interoperability**: the bounded importer now admits authentic
  LibreOffice/Word output instead of failing closed on constructs outside the
  modeled subset. Character data outside a modeled paragraph (e.g. index-template
  titles), style-property children (`style:tab-stops`, drop-caps, background
  images inside `style:paragraph-properties`), and inline active content
  (`office:scripts`, `script:event-listener` in `content.xml`) are dropped with a
  finding rather than aborting the document; the active-content subtree is never
  modeled or re-emitted, so no macro/handler code survives (`office:scripts` as a
  *manifest part* is still refused at package open). Interop is fixed by real
  LibreOffice-converted ODT fixtures under `fixtures/corpus/` — the whole
  real-producer corpus (rich text, table merges, table/list, footnotes,
  hyperlinks, header/footer, a round-tripped LibreOffice doc, rich metadata) plus
  a full sample document, 9 in all — each verified to import, validate, and
  re-export to a byte-exact canonical fixed point. Published limitations
  (admitted/mapped, dropped-with-finding, preserved-opaque, not-yet-done, and the
  round-trip contract) are documented in `docs/95` §10. Relax NG schema
  validation is a documented out-of-scope decision — the admission bar is
  structural well-formedness plus the bounded fail-closed profile and security
  limits, not grammar conformance — so no schema-validator dependency is bundled.
  A dependency-free `#[ignore]`-gated timing harness records import/export cost
  across the corpus (import 3.5–42 ms, export 1–8 ms).
- **Fields — page number, page count, date, time (fidelity parity, T3-1)**:
  `text:page-number`, `text:page-count`, `text:date`, and `text:time` now import
  to typed `Field` nodes (`FieldKind::Page`/`NumPages`/`Date`/`Time`, with a
  synthesized authoritative instruction) and export back to the ODF field
  elements, round-tripping to a byte-exact fixed point. The computed display cache
  (and, for date/time, the ODF number/data style with no DOCX format-picture
  equivalent) is dropped; a renderer recomputes the value. A field inside a
  hyperlink is not modeled (the model forbids a field nested in an inline
  wrapper) and stays a degraded text projection.
- **Fields — references & sequence (fidelity parity, T3-1c/T3-1d)**: a
  `text:bookmark-ref` (text/page reference format) and a `text:sequence` now
  import to typed `Field` nodes (`FieldKind::Ref`/`PageRef`/`Seq`) and export
  back, round-tripping to a byte-exact fixed point. The sequence's own
  formula/format and the reference's cached display are dropped. Control
  characters in a target/name round-trip as XML numeric references. Remaining
  field kinds (TOC) keep the degraded projection.
- **Comments — `office:annotation` (fidelity parity, T6-1)**: an inline
  `office:annotation` now imports to a schema-v1 `CommentReference` plus a
  `Comment` definition — author (`dc:creator`), date (`dc:date`), and body text
  (`text:p`/`text:h`, flattened to a single plain-text paragraph) — and exports
  back to `office:annotation` (with the `dc` namespace declared inline on the
  element, so comment-free documents are byte-unchanged), round-tripping to a
  byte-exact fixed point. The paired `office:annotation-end` range marker is not
  modeled, so a commented span collapses to a point comment at the anchor;
  multi-paragraph bodies flatten to one paragraph; the sequence/thread metadata
  (`office:name`, reply structure) is dropped. Active content inside an
  annotation is dropped like anywhere else.
- **Inline text boxes (fidelity parity, T6-4)**: an inline
  `draw:frame`>`draw:text-box` now round-trips to the model's `TextBox` inline and
  back, a byte-exact fixed point. The box body is captured as a flattened
  plain-text paragraph (multi-paragraph bodies join with a line break); size,
  fill, border, and floating anchors are dropped with a finding. The embedded-image
  path (and its href security validation) is unchanged — an image still wins when
  a frame carries both.
- **Tracked changes — insertions (fidelity parity, T6-3)**: an ODF tracked
  insertion now round-trips to the model's inline `Revision` and back, a
  byte-exact fixed point. The leading `text:tracked-changes` registry is
  pre-parsed for author/date (`office:change-info`), the body
  `text:change-start`/`-end` markers pair by change-id, and the inserted span is
  captured as a `Revision` wrapping its runs (a merge barrier keeps the inserted
  text from fusing into the preceding run). Export re-declares each region and
  wraps the range in markers, minting a change-id when the model's is not a valid
  unique XML name. This first slice covers same-paragraph insertions; deletions,
  moves, format changes, and block-spanning ranges degrade with a finding.
- **Table of contents (fidelity parity, T6-2)**: a block-level
  `text:table-of-content` now round-trips to the model's block content control
  (`BlockNode::Sdt`, a "Table of Contents" building-block gallery) — the
  `text:index-body` entries become the control's block content and `text:name`
  its tag — and exports back to `text:table-of-content`, a byte-exact fixed
  point. The level-template source is dropped (a renderer regenerates it); an
  empty TOC is dropped; an unnamed TOC is given a document-unique name on export.
- **Table cell margins (fidelity parity, T2c-6)**: a cell's content padding
  (`fo:padding` shorthand and per-edge `fo:padding-top`/`-bottom`/`-left`/`-right`)
  now round-trips via the `table-cell` style family to the model's `CellMargins`
  (twips↔pt codec, domain-clamped `0..=31680`); four equal edges collapse to the
  `fo:padding` shorthand. **This completes the whole table family both
  directions** — structure, merges, column widths, cell shading/valign/borders/
  margins, row height, and table width/alignment all round-trip.
- **Table-level width & alignment (fidelity parity, T2c-5)**: a table's alignment
  (`table:align`) and width (`style:width` absolute / `style:rel-width` relative)
  now round-trip via a new `table` style family, completing table-level geometry.
  Auto/nil widths and other table properties are reported.
- **Table row height (fidelity parity, T2c-4)**: a row's height now round-trips
  via a new `table-row` style family — exact height ↔ `style:row-height`, minimum
  ↔ `style:min-row-height` (twips↔pt codec, domain-clamped `0..=31680`). Rows with
  an `auto` height stay byte-identical.
- **Table cell borders (fidelity parity, T2c-3)**: a cell's four edge borders
  (`fo:border` and per-edge `fo:border-top/-left/-bottom/-right`, each `<width>
  <style> <color>`) now round-trip via the `table-cell` style family. Widths use
  an exact eighth-point↔`pt` codec (bounded to the model's `0..=1024` domain);
  the style token is stored verbatim; four identical edges collapse to the
  `fo:border` shorthand. Inside-H/V borders and edges carrying text padding are
  reported (not cell-representable as `fo:border`).
- **Table cell shading & vertical alignment (fidelity parity, T2c-2)**: a table
  cell's background fill (`fo:background-color`) and vertical alignment
  (`style:vertical-align`) now round-trip via a new `table-cell` style family —
  import resolves each `table:table-cell`'s referenced style; export mints a
  deterministic `table-cell` automatic style per distinct (fill, valign) and
  references it. Property-less cells stay byte-identical. Cell borders/margins
  remain reported remainders.
- **Table column widths (fidelity parity, T2c)**: a table's grid column widths
  now round-trip. Import resolves each `table:table-column`'s referenced
  `table-column` style (`style:table-column-properties`/`style:column-width`) —
  `number-columns-repeated` expanded — into `GridColumn.width_twips`; export emits
  a deterministic `table-column` automatic style per distinct width and references
  it from the column grid, grouping equal consecutive widths. Width-less tables
  stay byte-identical to before. Introduces the `table-column` style family the
  remaining table properties (cell shading, borders) will build on.
- **Paragraph properties (fidelity parity, T2b)**: paragraph formatting beyond
  alignment now round-trips — indentation (`fo:margin-left`/`-right`,
  `fo:text-indent` incl. hanging), spacing (`fo:margin-top`/`-bottom`,
  `fo:line-height` percent), and keep-with-next / keep-together / break-before.
  The single-alignment automatic style (`P_center`, …) is generalized to a
  deterministic paragraph style that keeps its historical name when only
  alignment is set. Lengths use an exactly-reversible `pt` form; the importer
  also accepts `cm`/`mm`/`in` from real producers. Tab stops and border/shading
  remain reported remainders (nested-element sub-slices).
- **Broader run properties (fidelity parity, T2a)**: the run-property round trip
  now covers font family (`fo:font-family`), superscript/subscript
  (`style:text-position`), all-caps (`fo:text-transform`), and small-caps
  (`fo:font-variant`) on both import and export, beyond the prior
  bold/italic/underline/strike/color/size subset. A latent bug where
  style-inheritance resolution silently dropped run properties outside that
  original subset was fixed. Theme fonts, the complex/east-asian font slots, and
  highlight remain reported remainders.
- **Hyperlink & bookmark export (round-trip parity)**: hyperlinks and bookmarks
  were imported faithfully but dropped on export (a silent round-trip loss).
  Export now re-emits `text:a` wrappers (external URLs with fragments, and
  `#anchor` internal targets, plus `office:title` screen-tips) and
  `text:bookmark-start`/`-end` markers, reaching a byte-exact fixed point with the
  importer. `xmlns:xlink` is now declared in the semantic content header. The
  exporter mirrors the importer's hyperlink-scheme allowlist — a blocked scheme
  (`javascript:`, `data:`, `file:`, …) a non-ODT-origin document might carry is
  degraded to plain text rather than re-emitted as a live link — and a bookmark
  name, href, or tooltip carrying an unserializable character degrades that one
  marker/link with a finding instead of aborting the whole export. First slice of
  the ODT→DOCX fidelity-parity effort.
- **Metadata conformance fix**: `meta.xml` now writes the creation timestamp as
  `meta:creation-date` and the last modification as `dc:date` (the ODF-native
  elements the importer reads), instead of the non-ODF `dcterms:created`/
  `dcterms:modified`. This keeps document dates interoperable with LibreOffice/
  Word and makes the metadata round trip idempotent.

### Rendering fidelity — 2026-07-27

Layout and rendering now consume the data the importer already models. Driven by
the gap analysis in `docs/46-RENDERING-FIDELITY-GAP-ANALYSIS.md` and measured
against LibreOffice as the layout oracle, page counts on the sample corpus are
now exact on 3 of 5 documents and within ±1 on the other 2 (PRs #126–#135).

- Fidelity gap analysis and prioritized fix roadmap (`docs/46`) added (#126).
- **Effective-property style cascade** (F1+F2): layout resolves each run's and
  paragraph's effective properties through `docDefaults → styles/basedOn →
  direct` instead of reading direct properties only — correct font sizes,
  families, bold/italic, and theme colors, plus paragraph spacing and the
  `w:lineRule` line-spacing rules (auto/atLeast/exact) (#127).
- **Header/footer band nesting**: the header/footer bands nest inside the page
  margins using Word's `max(margin, dist + band_height)` geometry instead of
  subtracting band height on top of the full margins, fixing systematic
  over-pagination on every header/footer document (#128).
- **Table cell margins + vertical alignment**: `w:tcMar`/`tblCellMar` cell margins
  and `w:vAlign` (top/center/bottom) are applied during table flow and compose,
  fixing cramped cells and mis-placed labels (#129).
- **Block-level SDT flow**: `BlockNode::Sdt` content is now flowed instead of
  dropped at zero height, recovering wrapped tables of contents and form-control
  paragraphs (#130).
- **VML shape parser**: positioned VML (`v:rect`/`v:line`/`v:oval`/`v:shape` +
  `v:imagedata` + `v:textbox`) is parsed from each `w:pict`'s raw XML, so
  VML-primary documents no longer render empty of graphics (#131).
- **Floating-object layer with real z-order** (F4): a single z-ordered float layer
  over both the body and the header/footer bands replaces the binary `behindDoc`
  split — DrawingML groups (`wpg`), floating text boxes, per-shape/group
  `relativeHeight` z-order, and header/footer floats. Groups are also written back
  to OOXML on export (#132).
- **Page background color**: `w:background` is painted instead of always-white
  pages (#133).
- **VML shape paint**: parsed VML shapes are mapped onto the float layer and
  painted (rectangles/lines/ovals/images as floats, text boxes with their blocks
  flowed through the shared pipeline), including header/footer VML (#134).
- **VML text-box de-overlap**: VML text boxes render inline so their content no
  longer overlaps the surrounding body, while shape and image floats are retained
  (#135).

Known residual limitations after this work are documented in `README.md`
("Status & limitations") and `docs/46`: text wrapping around floats, slightly
tall CJK fallback line metrics, a couple of ±1 page-count gaps, footer
`PAGE`/`NUMPAGES` recompute edge cases, floating-text-box export dropping its
anchor, and unlaid-out footnote/endnote bodies, inline math, and multi-column
sections.

### Fixed

- The semantic DOCX writer now emits the structured paragraph properties it
  previously dropped: paragraph `w:spacing` (before/after and line percent, the
  latter round-tripped exactly through the importer's `line*100/240` rule),
  `w:pBdr` (all six edges incl. `between`/`bar`), `w:shd`, and `w:tabs` (position,
  alignment, leader). A paragraph using any of these previously lost it on write
  → reopen; now proven by a round-trip test. (Completes the paragraph-property
  writer alongside the earlier run-property completeness.)
- The semantic DOCX writer preserves a style's present-but-empty `w:pPr`/`w:rPr`:
  a style whose paragraph/run properties reduce to the model default is now
  emitted as an empty element so it re-imports as `Some(default)` rather than
  `None` (the importer keys on tag presence). Also emits the `w:widowControl`
  paragraph flag, previously dropped. (Found by adversarial review.)
- The semantic DOCX writer now emits **every** modeled direct run property, not
  just bold/italic/strike/underline/color/size/fonts: `w:caps`, `w:smallCaps`,
  `w:vanish`, `w:webHidden`, `w:dstrike`, `w:vertAlign`, `w:highlight`, `w:em`,
  `w:spacing` (character), `w:kern`, `w:position`, and `w:lang` (all three
  tags). A run using any of these previously lost it on write → reopen; the full
  set is now proven by a round-trip test. (`style_ref` awaits the styles-part
  writer; `Color::Theme` on a run is not import-reachable.)
- XML character references in attribute values are now unescaped on import, so
  an attribute-carried string (a field instruction, a hyperlink/relationship URL
  with `&` query separators, a bookmark name, a revision author, an `sdt` alias)
  round-trips symmetrically with the writer's escaping instead of gaining an
  `amp;` layer on every pass. Applies to both the WordprocessingML importer and
  the OPC package reader (relationship `Target`s).
- Property-change tracked revisions (`w:tcPrChange`, `w:tblPrChange`,
  `w:trPrChange`, `w:pPrChange`, `w:rPrChange`, ...) no longer let their nested
  *historical* (pre-edit) property container overwrite the current table/row/
  cell/paragraph/run properties; the whole change subtree is reported and
  skipped. Theme-based shading (`w:themeFill`/`w:themeColor`) is now reported
  rather than silently dropped.

### Changed

- Licensed the entire project under Apache License 2.0.

### Added

- Font management, Phase 1A.3 (embedded fonts): a `fontTable.xml` font's
  embedded faces (`w:embedRegular`/`Bold`/`Italic`/`BoldItalic`) are now modeled
  as `EmbeddedFace` (fontKey, subsetted, the `fontTable.xml.rels` relationship
  id and `.odttf` part name, all verbatim). The importer resolves the font
  part's own relationships (`/font`); the writer regenerates the `w:embed*`
  children, `fontTable.xml.rels`, the `obfuscatedFont` content-type, and the
  `.odttf` parts (bytes verbatim — no de-obfuscation, which is a rendering
  concern). Proven by a fixed-point round-trip. Additive; migration golden
  byte-identical. (The `settings.xml` embedding flags remain a separate slice.)
- Semantic DOCX writer now emits inline text boxes: a `w:txbxContent` holding
  block content is regenerated inside the minimal DrawingML shape scaffold the
  importer round-trips. Proven by a fixed-point round-trip.
- Semantic DOCX writer now emits inline drawings (embedded pictures): the
  `w:drawing`/`wp:inline`/`pic:pic` scaffold with `a:blip@r:embed`, the media
  part, its content-type `Default` (by extension), and the `/image` relationship
  are regenerated. Media `part_name`/`relationship_id` are emitted verbatim so
  `MediaReference` round-trips, and those relationship ids are reserved so
  hyperlink/part ids cannot collide with them. `write_document`'s media byte map
  is now used (an absent entry writes an empty part; the reference still
  round-trips). Proven by a fixed-point round-trip. Text boxes and embedded
  fonts remain follow-ups.
- Semantic DOCX writer now emits headers and footers: each `HeaderFooter`
  definition becomes a `word/headerN.xml`/`footerN.xml` part (with per-part
  hyperlink rels), and the body `w:sectPr` gains the `w:headerReference`/
  `w:footerReference` entries. The relationship id each reference uses is derived
  from the `HeaderFooterId`, and parts are emitted in id order so the importer
  (which keys headers/footers by relationship order) re-allocates matching ids.
  Proven by a fixed-point round-trip.
- Semantic DOCX writer now emits the body `w:sectPr` page geometry (page size,
  margins, column count) — a section was previously silently dropped on write.
  Proven by a fixed-point round-trip.
- Semantic DOCX writer now emits footnotes/endnotes and comments: the note and
  comment definition parts (`footnotes.xml`, `endnotes.xml`, `comments.xml`,
  the latter with author/initials/date) are serialized back with ids derived
  from the internal `NoteId`/`CommentId`, and the body's `w:footnoteReference`/
  `w:endnoteReference`/`w:commentReference` reference them (previously dropped).
  A hyperlink inside a note/comment correctly routes through that part's own
  relationships (`word/_rels/<part>.xml.rels`) via a per-part writer context.
  Proven by fixed-point round-trips (incl. a note-internal hyperlink).
- Semantic DOCX writer now emits `word/numbering.xml`: abstract definitions
  (levels with start values) and numbering instances (with their abstract link)
  are serialized back, and a body paragraph's `w:numPr` (`numId` + `ilvl`,
  previously not emitted) references the instance. The `abstractNumId`/`numId`
  strings derive from the internal ids so the num→abstract link and the body
  reference resolve back to the same ids. Proven by a fixed-point round-trip.
- Semantic DOCX writer now emits `word/styles.xml` (definition-part writer, first
  slice): style definitions (kind, `basedOn`, paragraph + run property overrides)
  are serialized back, with the `w:styleId` string derived from the internal
  `StyleId` so a body `w:pStyle`/`w:rStyle` (run/paragraph `style_ref`, previously
  dropped) and a style's `w:basedOn` reference the same string and re-import to
  the same style. Emitted with its content-type override and `/styles`
  relationship. Proven by a fixed-point round-trip. The writer's extra-part
  handling was generalized so parts scale uniformly.
- Font management, Phase 1A.2b (theme `fontScheme`): the theme font scheme
  (`theme1.xml` `a:fontScheme`) is now modeled as `Definitions::font_scheme` — a
  `FontScheme` of major/minor `FontCollection`s, each with latin/ea/cs
  `ThemeFontEntry`s (typeface + opaque panose/pitchFamily/charset) and per-script
  overrides — so the 8-value theme font slots resolve to concrete families. The
  importer resolves the `/theme` relationship and parses the font scheme by local
  name (ignoring the colour/format schemes, which round-trip via Retention); the
  writer regenerates `word/theme/theme1.xml` with its content-type override and
  relationship. The orphaned `ThemeReferences` type is removed. Proven by a
  fixed-point round-trip. Additive: existing snapshots and the migration golden
  are byte-identical.
- Font management, Phase 1A.2a (`fontTable.xml`): the font table is now modeled
  as first-class `FontDescriptor`s on `Definitions::font_table` — family name,
  `altName`, `panose1`, `charset`, `family`, `pitch`, the OS/2 `sig` coverage
  fields, and `notTrueType`, with panose/charset/sig retained verbatim (opaque).
  The importer resolves the `/fontTable` relationship and parses the part; the
  semantic writer regenerates `word/fontTable.xml` with its content-type override
  and relationship. Proven by a fixed-point round-trip. Additive: existing v1
  snapshots and the v0→v1 migration golden are byte-identical.
- Font management, Phase 1A.1 (run-level `w:rFonts` fidelity): the theme-font
  slot vocabulary is now the full 8-value set (`majorAscii`/`majorHAnsi`/
  `majorEastAsia`/`majorBidi` and the `minor*` counterparts) instead of a
  `major`/`minor` collapse, the `w:rFonts@hint` disambiguator is modeled as a
  first-class value (a recognized hint-only `rFonts` is no longer reported), and
  the semantic writer now emits `w:rFonts` (the four slots + hint) — proven by a
  fixed-point round-trip. Additive: existing v1 snapshots are byte-identical.
- Semantic DOCX writer now emits the self-contained inline constructs:
  hyperlinks (external via generated `document.xml.rels` relationships, internal
  via `w:anchor`, with tooltips), simple fields (`w:fldSimple`), bookmark ranges,
  tracked-change revisions (`w:ins`/`w:del`, deletions written as `w:delText`),
  and inline content controls (`w:sdt`). Proven by model-fixed-point round-trips
  for both the self-contained set and external hyperlinks (rels regenerated).
- Semantic DOCX writer now emits tables: the model's tables (grid, table/row/
  cell properties incl. borders, shading, margins, merges, and nested tables)
  are serialized back to `w:tbl`, proven by the model-fixed-point round-trip.
- Semantic DOCX writer (Phase 1B, first slice): `casual-doc-export::write_document`
  serializes the v1 `Document` model back to a valid, editable DOCX (paragraphs,
  runs, and the core run/paragraph properties). The import->write->reopen model
  round-trip (the "semantic fixed point") is proven, and output bytes are
  deterministic. The dual of the existing Retention byte-copy writer.
  (`39-PHASE-1B-SEMANTIC-DOCX-WRITER-DESIGN.md`)
- Paragraph borders (`w:pBdr`), background shading (`w:shd`), and custom tab
  stops (`w:tabs`) are now modeled on paragraphs (borders reuse the shared
  border-edge type; a `w:shd` on the paragraph mark's `w:rPr` stays a run
  property, reported). Additive; the migration golden is byte-identical.
- Run typographic metrics and language: character spacing (`w:spacing` in
  a run), kerning (`w:kern`), baseline position (`w:position`), and language
  tags (`w:lang` val/eastAsia/bidi) are now modeled on runs; out-of-range
  metrics are reported. Additive; the migration golden is byte-identical.
- Content controls (structured document tags) in schema v1: block-level
  (`w:sdt` around paragraphs/tables) and inline-level (`w:sdt` around runs)
  controls are modeled as additive `BlockNode::Sdt` / `InlineNode::Sdt` wrappers
  carrying typed properties (control kind, alias, tag, retained `w:id`) and
  wrapping their content, which continues to parse (no text loss). A block sdt
  restarts the table-depth budget (like a text box), so deep tables inside a
  control import cleanly. Row/cell-structural controls and the `w:sdtPr` long
  tail (locks, placeholders, data binding, per-type detail, and the
  `w:docPartObj` vs `w:docPartList` distinction) remain reported (not yet
  modeled). Strictly additive — existing snapshots and the v0→v1 migration
  golden are byte-identical. (`53-SCHEMA-V1-CONTENT-CONTROLS-DESIGN.md`)
- Paragraph structural flags (`w:keepNext`, `w:keepLines`,
  `w:pageBreakBefore`, `w:widowControl`, `w:contextualSpacing`,
  `w:suppressLineNumbers`) and outline level (`w:outlineLvl`, 0-9) are now
  modeled on paragraphs (in both direct formatting and styles). Additive;
  existing snapshots and the migration golden are byte-identical.
  (`50-SCHEMA-V1-PARAGRAPH-PROPERTIES-DESIGN.md`)
- Table, row, and cell formatting (attribute-based, first slice): tables model
  `w:tblPr` (alignment, dxa width, layout, look flags, background shading), rows
  model `w:trPr` (height + rule, cantSplit, header repeat), and cells gain
  `w:tcPr` shading, vertical alignment, noWrap, and text direction. Non-dxa
  widths, table justify, unknown enum tokens, and patterned shading are reported
  (not silently mapped). Borders and cell margins remain reported (a follow-up
  slice). Strictly additive — existing snapshots and the migration golden are
  byte-identical. (`51-SCHEMA-V1-TABLE-PROPERTIES-DESIGN.md`)
- Run-property long tail (first slice): runs now model the toggle marks
  (`w:caps`/`smallCaps`/`vanish`/`webHidden`/`dstrike`), fonts (`w:rFonts` — the
  ascii/hAnsi/cs/eastAsia named + theme slots, finally populating the existing
  `font_ref`), and the named vocabularies `w:vertAlign` (super/subscript),
  `w:highlight` (named color), and `w:em` (emphasis mark). Unmapped values are
  reported, not silently dropped. Strictly additive — existing snapshots and the
  migration golden are byte-identical. (`49-SCHEMA-V1-RUN-PROPERTIES-DESIGN.md`)
- Tracked changes (revisions) are now modeled: inserted (`w:ins`) and deleted
  (`w:del`) run ranges become an additive `InlineNode::Revision` wrapper carrying
  its kind (insertion/deletion) plus retained author/date/id metadata and wrapping
  its content inlines; deleted text (`w:delText`) is preserved verbatim in the
  wrapped runs. Revisions nest with hyperlinks in both directions and within
  themselves (`w:ins` around `w:del`). Paragraph-mark, property-change
  (`w:rPrChange`/`w:pPrChange`/…), and move revisions remain reported (not yet
  modeled). Strictly additive — existing snapshots and the migration golden are
  byte-identical. (`48-SCHEMA-V1-TRACKED-CHANGES-DESIGN.md`)
- Comments (`word/comments.xml`) are now modeled as first-class definitions:
  `Definitions::comments` (a `CommentId → Comment` map, empty-omitted), each
  `Comment` carrying recursive block content plus retained `author`/`initials`/
  `date` metadata, and an in-body `InlineNode::CommentReference` that resolves to
  it (dangling references reported). Comment-part images and external hyperlinks
  resolve through the part's own relationships. The `w:commentRangeStart`/`End`
  anchor markers are reported (modeling deferred); no comment body content is
  dropped. Strictly additive — existing snapshots and the migration golden are
  byte-identical. (`47-SCHEMA-V1-COMMENTS-DESIGN.md`)
- Ruby phonetic guides (`w:ruby`) now keep their base text in document
  order; the annotation (`w:rt`) is reported (its text was previously merged in
  front of the base). No model change.
- Images and external hyperlinks inside notes, headers, and footers are now
  modeled (previously reported): each extra part resolves its own image and
  hyperlink relationships, and the media table aggregates image relationships
  across the whole package, de-duplicated by image part so a shared image has one
  id. Each part's parser resolves its own (per-part) relationship ids.
- Legacy VML pictures (`w:pict` → `v:imagedata@r:id`) are now imported as a
  `Drawing` (referencing the same media table as DrawingML pictures), instead of
  being reported and dropped. No model change; VML CSS sizing is not captured.
- Semantic headers and footers in schema v1: the `word/header*.xml` /
  `word/footer*.xml` parts are parsed into additive `Definitions.headers` /
  `footers` definitions (block content), and each `w:sectPr`
  `w:headerReference`/`w:footerReference` becomes a `HeaderFooterRef` on the
  section boundary (by page type: default/first/even). Additive: existing
  snapshots and section boundaries serialize byte-identically.
- Semantic footnotes and endnotes in schema v1: the `word/footnotes.xml` /
  `word/endnotes.xml` parts are parsed into additive `Definitions.footnotes` /
  `endnotes` note definitions (block content), and the in-body
  `w:footnoteReference`/`w:endnoteReference` becomes an additive
  `InlineNode::NoteReference` resolving to the note — closing the audit's
  silent-drop of note body text. Additive: existing snapshots serialize
  byte-identically (empty note maps are omitted). Note-part images/external
  hyperlinks are reported (their modeling is a follow-up).
- Semantic text boxes in schema v1: an additive `InlineNode::TextBox` holding
  block content, imported from `w:txbxContent` (DrawingML `wps:txbx` or legacy
  VML `v:textbox`). Fixes an audit-confirmed data-corruption blocker where a text
  box's inner paragraph truncated the enclosing paragraph, mis-captured the boxed
  text, and silently dropped the enclosing drawing's image. `mc:AlternateContent`
  now selects a single branch (first `mc:Choice`, else `mc:Fallback`), so a
  drawing expressed in both DrawingML and VML is no longer duplicated. Per-part
  relationship resolution (`DocxPackage::part_relationships`) was added as the
  foundation for images and links inside header/footer/footnote parts.
- Semantic fields in schema v1: an additive `InlineNode::Field` (opaque
  instruction + cached-result inlines), imported from both `w:fldSimple` and
  complex `fldChar` begin/separate/end run sequences. Fields and hyperlinks are
  mutually-exclusive inline wrappers containing only leaf inlines; a
  wrapper-in-wrapper is reported and flattened without losing display text.
  Backward-compatible and additive; the v0→v1 migration is unchanged.
- Semantic tables in schema v1: an additive `BlockNode::Table` (shared column
  grid, rows, cells holding recursive block content, `gridSpan`/`vMerge` cell
  merge geometry, depth-bounded), imported from `w:tbl` instead of flattening
  cell text into the body, with unmapped table styling still reported and
  Retention-preserved. Backward-compatible: existing snapshots and the v0→v1
  migration are unchanged.
- Pinned-source architecture research for LibreOffice, ONLYOFFICE, Open XML
  SDK, and Apache POI, plus a proposed OOXML fidelity architecture covering
  source snapshots, provenance, typed preservation, mapping rules, and future
  save planning.
- Production project foundation, design process, tracker, security policy, and
  CI contract.
- Initial normalized model, atomic text-insertion transaction, position mapping,
  SDK snapshot facade, and stable error codes.
- Grapheme-range deletion, paragraph split/join, semantic inverses, complete
  operation mapping, and revisioned undo/redo.
- Strict bounded normalized schema v0 JSON loading, deterministic export,
  semantic resource limits, redacted failures, and imported-ID collision
  avoidance.
- Canonical directed session selection with revision validation, grapheme-safe
  endpoints, and atomic mapping through edits, undo, and redo.
- Bounded future-only runtime event subscriptions with stable sequencing,
  transaction/selection causes, independent cursors, and explicit lag gaps.
- Security-bounded DOCX ZIP admission, deterministic part metadata, cancellable
  on-demand reads, and repository-owned package fixtures.
- Reproducible package/model benchmark runner with typed reports,
  named-environment comparison, an initial Apple M4 baseline, and CI smoke.
- Mixed-Unicode and unknown-safe-part DOCX fixtures with byte-exact package
  coverage.
- Independently locked DOCX package fuzz target, required pull-request build,
  and bounded scheduled sanitizer campaign.
- Bookmarks in schema v1: `w:bookmarkStart`/`w:bookmarkEnd` are modeled as an
  additive `Bookmark{name}` definition table (`Definitions::bookmarks`, keyed by a
  new `BookmarkId`) plus a paired `InlineNode::BookmarkStart`/`BookmarkEnd` marker
  range delimiting the bookmark's extent in document flow. Marker→definition
  integrity is validated (`DanglingBookmarkRef`); internal-hyperlink anchor
  resolution remains lax (forward/cross-part/well-known targets). Block-level
  markers and column bookmarks (`w:colFirst`/`w:colLast`, span dropped) remain
  reported (not silently lost); strict anchor resolution is deferred. Additive:
  existing snapshots and the v0→v1 migration golden are byte-identical.
  (`38-SCHEMA-V1-DESIGN-REFERENCE.md#bookmarks`)
- Paragraph structural flags (`w:keepNext`, `w:keepLines`,
  `w:pageBreakBefore`, `w:widowControl`, `w:contextualSpacing`,
  `w:suppressLineNumbers`) and outline level (`w:outlineLvl`, 0-9) are now
  modeled on paragraphs (in both direct formatting and styles). Additive;
  existing snapshots and the migration golden are byte-identical.
  (`38-SCHEMA-V1-DESIGN-REFERENCE.md#paragraph-properties`)
- Table, row, and cell formatting (attribute-based, first slice): tables model
  `w:tblPr` (alignment, dxa width, layout, look flags, background shading), rows
  model `w:trPr` (height + rule, cantSplit, header repeat), and cells gain
  `w:tcPr` shading, vertical alignment, noWrap, and text direction. Non-dxa
  widths, table justify, unknown enum tokens, and patterned shading are reported
  (not silently mapped). Borders and cell margins remain reported (a follow-up
  slice). Strictly additive — existing snapshots and the migration golden are
  byte-identical. (`38-SCHEMA-V1-DESIGN-REFERENCE.md#table-properties`)
- Run-property long tail (first slice): runs now model the toggle marks
  (`w:caps`/`smallCaps`/`vanish`/`webHidden`/`dstrike`), fonts (`w:rFonts` — the
  ascii/hAnsi/cs/eastAsia named + theme slots, finally populating the existing
  `font_ref`), and the named vocabularies `w:vertAlign` (super/subscript),
  `w:highlight` (named color), and `w:em` (emphasis mark). Unmapped values are
  reported, not silently dropped. Strictly additive — existing snapshots and the
  migration golden are byte-identical. (`38-SCHEMA-V1-DESIGN-REFERENCE.md#run-properties`)
- Tracked changes (revisions) are now modeled: inserted (`w:ins`) and deleted
  (`w:del`) run ranges become an additive `InlineNode::Revision` wrapper carrying
  its kind (insertion/deletion) plus retained author/date/id metadata and wrapping
  its content inlines; deleted text (`w:delText`) is preserved verbatim in the
  wrapped runs. Revisions nest with hyperlinks in both directions and within
  themselves (`w:ins` around `w:del`). Paragraph-mark, property-change
  (`w:rPrChange`/`w:pPrChange`/…), and move revisions remain reported (not yet
  modeled). Strictly additive — existing snapshots and the migration golden are
  byte-identical. (`38-SCHEMA-V1-DESIGN-REFERENCE.md#tracked-changes`)
- Comments (`word/comments.xml`) are now modeled as first-class definitions:
  `Definitions::comments` (a `CommentId → Comment` map, empty-omitted), each
  `Comment` carrying recursive block content plus retained `author`/`initials`/
  `date` metadata, and an in-body `InlineNode::CommentReference` that resolves to
  it (dangling references reported). Comment-part images and external hyperlinks
  resolve through the part's own relationships. The `w:commentRangeStart`/`End`
  anchor markers are reported (modeling deferred); no comment body content is
  dropped. Strictly additive — existing snapshots and the migration golden are
  byte-identical. (`38-SCHEMA-V1-DESIGN-REFERENCE.md#comments`)
- Ruby phonetic guides (`w:ruby`) now keep their base text in document
  order; the annotation (`w:rt`) is reported (its text was previously merged in
  front of the base). No model change.
- Images and external hyperlinks inside notes, headers, and footers are now
  modeled (previously reported): each extra part resolves its own image and
  hyperlink relationships, and the media table aggregates image relationships
  across the whole package, de-duplicated by image part so a shared image has one
  id. Each part's parser resolves its own (per-part) relationship ids.
- Legacy VML pictures (`w:pict` → `v:imagedata@r:id`) are now imported as a
  `Drawing` (referencing the same media table as DrawingML pictures), instead of
  being reported and dropped. No model change; VML CSS sizing is not captured.
- Semantic headers and footers in schema v1: the `word/header*.xml` /
  `word/footer*.xml` parts are parsed into additive `Definitions.headers` /
  `footers` definitions (block content), and each `w:sectPr`
  `w:headerReference`/`w:footerReference` becomes a `HeaderFooterRef` on the
  section boundary (by page type: default/first/even). Additive: existing
  snapshots and section boundaries serialize byte-identically.
- Semantic footnotes and endnotes in schema v1: the `word/footnotes.xml` /
  `word/endnotes.xml` parts are parsed into additive `Definitions.footnotes` /
  `endnotes` note definitions (block content), and the in-body
  `w:footnoteReference`/`w:endnoteReference` becomes an additive
  `InlineNode::NoteReference` resolving to the note — closing the audit's
  silent-drop of note body text. Additive: existing snapshots serialize
  byte-identically (empty note maps are omitted). Note-part images/external
  hyperlinks are reported (their modeling is a follow-up).
- Semantic text boxes in schema v1: an additive `InlineNode::TextBox` holding
  block content, imported from `w:txbxContent` (DrawingML `wps:txbx` or legacy
  VML `v:textbox`). Fixes an audit-confirmed data-corruption blocker where a text
  box's inner paragraph truncated the enclosing paragraph, mis-captured the boxed
  text, and silently dropped the enclosing drawing's image. `mc:AlternateContent`
  now selects a single branch (first `mc:Choice`, else `mc:Fallback`), so a
  drawing expressed in both DrawingML and VML is no longer duplicated. Per-part
  relationship resolution (`DocxPackage::part_relationships`) was added as the
  foundation for images and links inside header/footer/footnote parts.
- Semantic fields in schema v1: an additive `InlineNode::Field` (opaque
  instruction + cached-result inlines), imported from both `w:fldSimple` and
  complex `fldChar` begin/separate/end run sequences. Fields and hyperlinks are
  mutually-exclusive inline wrappers containing only leaf inlines; a
  wrapper-in-wrapper is reported and flattened without losing display text.
  Backward-compatible and additive; the v0→v1 migration is unchanged.
- Semantic tables in schema v1: an additive `BlockNode::Table` (shared column
  grid, rows, cells holding recursive block content, `gridSpan`/`vMerge` cell
  merge geometry, depth-bounded), imported from `w:tbl` instead of flattening
  cell text into the body, with unmapped table styling still reported and
  Retention-preserved. Backward-compatible: existing snapshots and the v0→v1
  migration are unchanged.
- Pinned-source architecture research for LibreOffice, ONLYOFFICE, Open XML
  SDK, and Apache POI, plus a proposed OOXML fidelity architecture covering
  source snapshots, provenance, typed preservation, mapping rules, and future
  save planning.
- Production project foundation, design process, tracker, security policy, and
  CI contract.
- Initial normalized model, atomic text-insertion transaction, position mapping,
  SDK snapshot facade, and stable error codes.
- Grapheme-range deletion, paragraph split/join, semantic inverses, complete
  operation mapping, and revisioned undo/redo.
- Strict bounded normalized schema v0 JSON loading, deterministic export,
  semantic resource limits, redacted failures, and imported-ID collision
  avoidance.
- Canonical directed session selection with revision validation, grapheme-safe
  endpoints, and atomic mapping through edits, undo, and redo.
- Bounded future-only runtime event subscriptions with stable sequencing,
  transaction/selection causes, independent cursors, and explicit lag gaps.
- Security-bounded DOCX ZIP admission, deterministic part metadata, cancellable
  on-demand reads, and repository-owned package fixtures.
- Reproducible package/model benchmark runner with typed reports,
  named-environment comparison, an initial Apple M4 baseline, and CI smoke.
- Mixed-Unicode and unknown-safe-part DOCX fixtures with byte-exact package
  coverage.
- Independently locked DOCX package fuzz target, required pull-request build,
  and bounded scheduled sanitizer campaign.

### Security

- Dependency license/source/advisory checks.
- Bounded parser and resource-limit specification.
- ZIP entry, expansion, path, overlap, encryption, macro, and compression
  enforcement before DOCX package admission.
- Nightly libFuzzer coverage for arbitrary package admission and verified part
  reads without adding fuzz dependencies to the production workspace.
