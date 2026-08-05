# 96 — ODT Export Profile

**Status:** Accepted for incremental Slice E implementation
**Date:** 2026-08-04
**Tracker:** MFIO-006
**Parent:** `94-MULTI-FORMAT-IMPORT-EXPORT-ARCHITECTURE.md`

## 1. Purpose

Define a deterministic, bounded OpenDocument Text export profile without
claiming full ODT fidelity. The first checkpoint makes ODT a real internal
export target for the normalized model and retains exact unchanged ODT bytes
when explicitly requested. Generic WASM dispatch and capability-driven browser
Save are implemented, but this profile does not imply native SDK availability
or edit-tolerant preservation of opaque ODF data.

## 2. Package contract

Semantic output is an ODF 1.4 ZIP package with:

- `mimetype` as entry zero, stored, exact, and without a local-header extra
  field;
- bounded `content.xml` and `META-INF/manifest.xml` parts;
- a namespace-correct manifest whose root MIME and version agree with the
  package;
- deterministic entry order, compression options, XML declaration, namespace
  declarations, attribute order, and escaping;
- no scripts, macros, signatures, encryption declarations, external fetches,
  or executable embedded content.

The writer's own bytes must reopen through `OdtPackage` before the checkpoint is
considered complete.

## 3. Initial semantic surface

The first writer maps body paragraphs and headings, run text, spaces, tabs, and
line/page/column breaks. Space runs use `text:s` so XML whitespace handling
cannot change text. XML-illegal control characters fail closed.

The automatic-style checkpoint additionally maps direct paragraph alignment
and the normalized run subset for bold, italic, underline, strike-through,
explicit RGB color, and half-point font size. Stable style names are derived
from canonical property values, definitions are emitted in sorted order, and
explicit `false` toggles remain distinct from absent properties. Theme colors
and every property outside this bounded subset remain compatibility findings.

The list checkpoint maps normalized bullet, decimal, lowercase/uppercase
letter, and lowercase/uppercase Roman levels into canonical `text:list-style`
definitions. It emits the first paragraph of each list item, nested list trees,
per-instance level starts, and continuation markers for separated sequences.
Style names derive from canonical list semantics rather than model IDs, making
supported output stable across reopen and re-export. Unsupported number systems,
multi-placeholder labels, and unimplemented level formatting are reported;
labels that cannot be represented safely are projected as plain paragraphs.

The table checkpoint emits recursive tables with canonical grid-column counts,
leading header-row containers, implemented nested cell blocks, horizontal spans,
and rectangular vertical spans represented by covered cells. Supported table
geometry reopens to the same normalized document and re-exports byte-identically.
Row/cell formatting and non-default grid widths are explicit loss findings.
Vertical-merge continuations that are orphaned, span-mismatched, or carry
non-canonical content/properties are written as visible regular cells and receive
a merge-loss finding instead of becoming invalid or hiding their content.

The note checkpoint emits normalized footnote/endnote references as canonical
inline `text:note` containers. Transport IDs derive deterministically from the
model note ID; note bodies reuse recursive paragraph/list/table writing and form
a semantic and byte fixed point for the supported shape. A definition referenced
more than once is emitted visibly with a unique occurrence ID and a degraded
finding because ODT owns note content at the inline occurrence. Nested references
are explicitly omitted, and unreferenced definitions receive an omission finding.
Authored ODT citation labels are not present in schema v1, so semantic output uses
an empty canonical citation element.

Hyperlinks and bookmarks round-trip: an `InlineNode::Hyperlink` is written as a
`text:a` wrapper (`xlink:type="simple"`, `xlink:href` carrying the external URL
with any fragment, or `#anchor` for an internal target; the screen-tip becomes
`office:title`), and `BookmarkStart`/`BookmarkEnd` markers are written as
`text:bookmark-start`/`text:bookmark-end` with the registered bookmark name — the
exact forms the importer reads back, giving a byte-exact fixed point.

Comments round-trip as point annotations: a `CommentReference` is written as an
`office:annotation` with the `dc` namespace declared inline on the element (so
comment-free documents keep the unchanged content header), carrying the author as
`dc:creator`, the date as `dc:date`, and the comment's block content as the
annotation body. The paired `CommentRangeStart`/`CommentRangeEnd` markers are
omitted (the model comment resolves through its reference), so a range comment is
written as a point at its anchor. A non-representable author/date is dropped with
a finding; the annotation and its body are still emitted.

Until their dedicated import/export mappings land, wrappers and complex blocks
may emit a bounded visible-text projection only when that projection is safe.
Every such case is reported as degraded; content with no safe projection is
reported as omitted. Unsupported run/paragraph properties, definitions,
resources, table formatting, advanced lists, media, tracked changes, fields,
controls, math, drawings, and embedded objects may not silently disappear from
the report.

## 4. Export modes

- `Semantic` writes the implemented normalized subset and reports all loss.
- `PreserveWhenSafe` performs edit-tolerant preservation when a matching retained
  source is present: `write_odt_with_retained_parts` re-emits `draw:frame` for
  `Drawing` nodes whose source image bytes were retained and repackages those
  bytes (byte-verbatim, stored) with deterministic manifest entries, so images
  survive a semantic edit and reopen through the bounded admission layer as a byte
  fixed point. A `Drawing` whose bytes are not retained still degrades to the
  alt-text projection (no dangling reference). Without a matching retained source
  it falls back to the plain semantic package. See
  `97-ODT-EDIT-TOLERANT-PRESERVATION.md`; safe unknown-part carry is a later
  checkpoint.
- An `AnchoredDrawing` (floating image) with retained bytes re-emits a positioned
  `draw:frame` (first increment): `text:anchor-type` (from the reference edges,
  page/paragraph), `svg:x`/`svg:y` (offset placement), `draw:z-index`, extent, and
  the image, with the ODF-default `Square` wrap carried implicitly (no graphic
  style). Every model state outside that reversible core is reported and mapped to
  its nearest representable form so the output stays a fixed point: a non-`Square`
  wrap, `behind_doc`, exclusion distances, and a contour polygon are dropped; an
  alignment position and a negative offset collapse to a zero offset; a
  margin-strip/character/line reference degrades to the nearest page/paragraph
  anchor type; and crop/border/flip/rotation are dropped. Without retained bytes it
  degrades to alt text like an inline `Drawing`.
- `ExactIfUnchanged` returns retained original ODT bytes only when the source
  format matches and the caller asserts the document is unchanged.

The descriptor advertises semantic export and exact-unchanged support but does
not advertise edit-tolerant preservation until that path exists.

## 5. Bounds and atomicity

`OdfExportLimits` bounds content XML bytes, final package bytes, paragraph/block
visits, inline visits, table-row/cell visits, table columns, note occurrences, recursion depth, emitted text
bytes, and compatibility feature buckets. Limit, model-validation,
XML-character, serialization, or ZIP failure returns no partial artifact.

When present, supported `DocumentProperties` core fields are emitted in a
deterministic `meta.xml` part and registered in the manifest. Dates use the
ODF-native elements — the creation timestamp as `meta:creation-date` and the last
modification as `dc:date` (not `dcterms:*`, which is outside the ODF meta schema
and not read back by the importer) — so document dates stay interoperable and the
metadata round trip is idempotent. Application name is emitted as `meta:generator`;
unsupported application/custom fields are reported rather than silently
represented as unrelated ODT metadata.
Numeric page/word statistics and typed custom properties are emitted as their
corresponding ODT metadata elements.
Total editing time is emitted as a canonical `PT#H#M` duration.

When document defaults are present, the writer emits a deterministic
`styles.xml` `office:styles` block with `style:default-style` entries for the
paragraph and text families (alignment and the supported direct run subset),
reusing the automatic-style property serializer. Defaults are emitted even when
no section exists, and unsupported default detail is reported. Supported defaults
form a semantic and byte fixed point.

Named `StyleKind::Character` definitions are emitted into that same
`office:styles` block as `style:style style:family="text"` entries (in `StyleId`
order, with a `style:text-properties` child carrying the supported run subset),
and a run bearing a `RunProperties.style_ref` re-emits `text:style-name="X"`
naming that style instead of minting an automatic `T_` run style. The emitted
`style:name` reuses the definition's retained name when it is a valid NCName and
otherwise mints a stable `Char{n}`; the styles.xml block (and its manifest entry)
is now emitted whenever named character styles exist, even without defaults or a
section. A run carrying both a style ref and direct run properties keeps the
named style and reports the direct subset; a ref that does not resolve to a
Character definition, and any non-run style detail the projection cannot carry,
are reported. Supported named character styles form a semantic and byte fixed
point.

Named `StyleKind::Paragraph` definitions are emitted the same way as
`style:style style:family="paragraph"` entries (with a `style:paragraph-properties`
child carrying the supported paragraph subset), and a paragraph bearing a
`ParagraphProperties.style_ref` re-emits `text:style-name="X"` instead of minting
an automatic `P…` paragraph style. Character and paragraph names are assigned from
independent id spaces. A retained name matching the automatic paragraph scheme
(`P`/`P_start`/`P_end`/`P_center`/`P_justify`, optionally with the property-hash
suffix) is re-minted `Para{n}` so it cannot collide with a direct-formatted
paragraph's automatic style. A paragraph carrying both a ref and direct
properties keeps the named style and reports the direct subset; an unresolvable
ref and non-paragraph style detail (run/table slots, inheritance, UI flags, an
outline level or numbering link on the style) are reported. Supported named
paragraph styles form a semantic and byte fixed point.

When schema-v1 sections are present, the writer emits a deterministic
`styles.xml` page-layout definition and manifest entry for page geometry.
Section column count, gap, and separator settings are emitted when present.
Supported section writing modes are emitted as `style:writing-mode`.
When the first section carries header/footer references, the writer emits a
`style:master-page` (bound to the page-layout) with the bounded plain-text
subset: `HeaderFooterKind::Default` maps to `style:header`/`style:footer` and
`Even` to `style:header-left`/`style:footer-left`. The `text` namespace and the
`office:master-styles` block are only added when a header/footer is present, so
geometry-only output stays byte-identical. Header/footer content reuses the body
block/paragraph/text writer so escaping, `text:s` spacing, and DoS counters are
shared; run/paragraph formatting, first-page (`First`) references, non-paragraph
blocks, and duplicate references are explicit loss findings. Supported
header/footer content is a semantic and byte fixed point.

## 6. Acceptance gates

The first checkpoint requires:

1. deterministic bytes for identical normalized input;
2. package reopen through the bounded ODF admission layer;
3. semantic write → reopen → import equality for the implemented core subset;
4. exact unchanged recovery only under matching retained-source authorization;
5. bounded, stable findings for unsupported model constructs and resources;
6. output/recursion/count/character limit tests;
7. workspace test, strict Clippy, rustdoc, MSRV, WASM, format, diff, web honesty,
   and fuzz-build gates.

## 7. Normative references

- OASIS, [OpenDocument Version 1.4, Part 2: Packages](https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part2-packages/OpenDocument-v1.4-os-part2-packages.html).
- OASIS, [OpenDocument Version 1.4, Part 3: OpenDocument Schema](https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part3-schema/OpenDocument-v1.4-os-part3-schema.html).

## 8. Implementation status

The first internal checkpoint is implemented on `feature/multi-format-io`:

- `casual-doc-odf::write_odt` writes deterministic bounded ODF 1.4 packages for
  the core paragraph/heading/text/space/tab/line-break subset;
- deterministic `office:automatic-styles` preserve direct paragraph alignment
  and the supported run formatting subset; unsupported property remainder is
  still reported instead of being silently discarded;
- output is reopened through the independent bounded package/import path and the
  supported subset is tested as a semantic fixed point;
- unsupported normalized constructs and resources receive deterministic loss
  findings, while invalid models, XML-illegal characters, and limit violations
  fail atomically;
- `casual-doc-io::OdtAdapter` exposes semantic export and exact retained
  unchanged bytes through the format registry;
- `casual-doc-wasm` routes auto/explicit open and explicit export through that
  registry, applies the viewer package limits consistently to DOCX and ODT, and
  exposes deterministic import/export compatibility reports as JSON;
- the browser populates its export target control from the WASM capabilities,
  attempts exact unchanged same-format recovery before preservation export,
  uses semantic export cross-format, and shows report occurrence counts.

The matching automatic-style import subset is implemented and tested as a
semantic fixed point. Named `styles.xml` resolution and same-family inheritance
also feed that normalized subset on import. Bounded list import/export covers
the label systems and nesting described in section 3, with deterministic
reopen/re-export tests. Recursive table import/export covers the geometry and
fallback rules described in section 3 with the same fixed-point tests. Typed
footnote/endnote import/export covers recursive note bodies, deterministic IDs,
occurrence bounds, and the non-one-to-one outcomes described in section 3. Style
defaults, broader style and table properties, advanced list continuation/item
overrides and label layout, edit-tolerant source preservation, broader semantic writing, stable
native SDK surfaces, Relax NG validation, interoperability fixtures, and
production claims remain pending.
