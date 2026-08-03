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

Until their dedicated import/export mappings land, wrappers and complex blocks
may emit a bounded visible-text projection only when that projection is safe.
Every such case is reported as degraded; content with no safe projection is
reported as omitted. Unsupported run/paragraph properties, definitions,
resources, table formatting, advanced lists, links, bookmarks, media, tracked
changes, fields, controls, math, drawings, and embedded objects may not silently
disappear from the report.

## 4. Export modes

- `Semantic` writes the implemented normalized subset and reports all loss.
- `PreserveWhenSafe` currently writes the same semantic package and reports a
  matching or foreign source envelope as not retained; edit-tolerant ODF
  preservation is a later checkpoint.
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
deterministic `meta.xml` part and registered in the manifest. Application name
is emitted as `meta:generator`; unsupported application/custom fields are
reported rather than silently represented as unrelated ODT metadata.

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
