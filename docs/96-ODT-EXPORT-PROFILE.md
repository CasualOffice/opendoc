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

Until their dedicated import/export mappings land, wrappers and complex blocks
may emit a bounded visible-text projection only when that projection is safe.
Every such case is reported as degraded; content with no safe projection is
reported as omitted. Run/paragraph properties, definitions, resources, tables,
lists, links, notes, bookmarks, media, tracked changes, fields, controls, math,
drawings, and embedded objects may not silently disappear from the report.

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
visits, inline visits, recursion depth, emitted text bytes, and compatibility
feature buckets. Limit, model-validation, XML-character, serialization, or ZIP
failure returns no partial artifact.

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

Edit-tolerant source preservation, broader semantic writing, stable native SDK
surfaces, Relax NG validation, interoperability fixtures, and production claims
remain pending.
