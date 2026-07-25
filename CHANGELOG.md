# Changelog

All notable user-visible, integrator-visible, compatibility, security, and
migration changes are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
OpenDoc will use semantic versioning when its public package line begins.

## Unreleased

### Fixed

- Property-change tracked revisions (`w:tcPrChange`, `w:tblPrChange`,
  `w:trPrChange`, `w:pPrChange`, `w:rPrChange`, ...) no longer let their nested
  *historical* (pre-edit) property container overwrite the current table/row/
  cell/paragraph/run properties; the whole change subtree is reported and
  skipped. Theme-based shading (`w:themeFill`/`w:themeColor`) is now reported
  rather than silently dropped.

### Changed

- Licensed the entire project under Apache License 2.0.

### Added

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

### Security

- Dependency license/source/advisory checks.
- Bounded parser and resource-limit specification.
- ZIP entry, expansion, path, overlap, encryption, macro, and compression
  enforcement before DOCX package admission.
- Nightly libFuzzer coverage for arbitrary package admission and verified part
  reads without adding fuzz dependencies to the production workspace.
