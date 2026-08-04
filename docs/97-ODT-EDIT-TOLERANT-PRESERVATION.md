# 97 — ODT Edit-Tolerant Preservation

**Status:** Proposed design (pre-implementation)
**Date:** 2026-08-05
**Tracker:** MFIO-006 (Slice E continuation) / MFIO-007 inputs
**Parents:** `94-MULTI-FORMAT-IMPORT-EXPORT-ARCHITECTURE.md`, `95-ODT-IMPORT-PROFILE.md`, `96-ODT-EXPORT-PROFILE.md`

## 1. Purpose

Define how an admitted ODT survives a *semantic edit and re-export* without
losing the source parts the semantic model does not fully own — primarily
embedded images, but also safe unknown/opaque parts. This is the ODT analogue of
the DOCX `RetainedParts` path (`casual-doc-io::docx`,
`casual_doc_export::write_document_with_retained_parts`) and is the missing piece
behind two current gaps:

- `Drawing`/`MediaReference` import (doc 95) references a package image part but
  holds **no bytes**, so semantic ODT→ODT export cannot reproduce the image and
  reports it as a loss.
- `OdtAdapter` advertises `preserve_when_safe: false`; `PreserveWhenSafe` today
  writes the same semantic package and reports the source envelope as *not
  retained*.

This is not a claim of full ODF preservation. It is a bounded, deterministic,
safety-first retention path scoped to parts we can carry verbatim.

## 2. Non-goals

- No editing *of* the retained opaque parts (they are carried byte-verbatim).
- No cross-format leakage: retained ODF-native parts are never copied into DOCX,
  JSON, or TXT export (doc 95 §6 holds).
- No decoding, re-encoding, or validation of image pixel data.
- No preservation of parts that fail the security profile (scripts, macros,
  encryption, active content) — those remain blocked (doc 95 §2).

## 3. Retention model

Extend the source envelope, mirroring `DocxSourceState`:

```text
OdtSourceState {
    original_bytes: Option<Vec<u8>>,   // already present (ExactIfUnchanged)
    version: String,                   // already present
    retained: OdfRetainedParts,        // NEW
}
```

`OdfRetainedParts` is produced by the ODF import layer under
`retain_source == true` and holds a bounded, deterministic map:

```text
OdfRetainedParts { parts: BTreeMap<String /*normalized part name*/, RetainedPart> }
RetainedPart { media_type: String, bytes: Vec<u8> }
```

Only parts that are (a) referenced by a mapped `MediaReference`, or (b) safe
unknown non-`META-INF/`, non-`content.xml`/`styles.xml`/`meta.xml` parts within
bounds, are retained. `mimetype`, the manifest, and the semantic XML parts are
**not** retained here — they are regenerated on export.

Retention is opt-in (`retain_source`) and bounded (§6), so the default path
allocates nothing.

## 4. Import changes (`casual-doc-odf` + `casual-doc-io`)

1. `casual-doc-odf` gains a bounded API to enumerate admitted parts with their
   manifest media type and bytes (the package already reads bounded parts). A
   new `import_document` sibling returns, alongside `OdtImport`, the retained
   part set filtered to referenced media plus safe unknown parts.
2. The image cross-check (doc 95, `package.rs`) already resolves each
   `MediaReference` to a manifest part and its authoritative media type; retention
   reuses that resolution to pull the referenced bytes into `OdfRetainedParts`.
3. `casual-doc-io::odt` stores the result in `OdtSourceState.retained` when
   `retain_source` is set.

The normalized model is unchanged: it still carries only references, never bytes.

## 5. Export changes

Add `write_odt_with_retained_parts(document, retained, limits)` to
`casual-doc-odf`, and route `PreserveWhenSafe` in `casual-doc-io::odt` to it when
a matching retained source is present.

Two coupled behaviors:

1. **Emit `draw:frame` for `Drawing` nodes.** Semantic export currently drops a
   `Drawing` to an alt-text projection (`write_alt`). The preserving writer
   instead emits a bounded inline `draw:frame`/`draw:image` referencing the
   resolved part name, with `svg:width`/`svg:height` from the `Extent` and
   `svg:title` from `descr`. A `Drawing` whose part is **not** in `retained` still
   degrades to the alt-text projection with a finding (no dangling reference).
2. **Repackage retained parts.** The written ODF package includes each retained
   part (byte-verbatim) with a manifest `file-entry` carrying its retained media
   type, in deterministic (sorted) entry order after the semantic parts.

`Semantic` mode is unchanged (references reported as loss). `ExactIfUnchanged` is
unchanged (returns `original_bytes`). Only `PreserveWhenSafe` merges retained
parts, and `OdtAdapter` then advertises `preserve_when_safe: true`.

## 6. Safety and bounds

New `OdfImportLimits`/`OdfExportLimits` fields (and therefore the fuzz const
literals must be updated in lockstep — see doc 95 §7 note):

- `max_retained_parts`, `max_retained_part_bytes` (per part), `max_retained_total_bytes`.

Rules:

- A part exceeding any bound is not retained; its drawing degrades with a finding
  — never a hard failure of an otherwise-valid document.
- Retained bytes are treated as opaque octets: never parsed as XML, never
  executed, never fetched. Only parts already admitted by the package profile are
  eligible, so active-content/encryption blocking (doc 95 §2) is inherited.
- Part names are validated as safe internal ZIP paths (the doc 95
  `is_safe_media_href` / `normalized_part_path` rules) before repackaging, so a
  crafted reference cannot write outside the package namespace.
- The final package still passes the bounded ODF admission layer on reopen.

## 7. Determinism

- Retained parts serialize in sorted normalized-name order; manifest entries in a
  fixed, sorted order; identical input → identical output bytes.
- `draw:frame` attribute order, unit formatting (EMU→cm), and escaping follow the
  existing deterministic writer conventions (doc 96 §2).
- Repackaging a retained image and re-importing yields the same `MediaReference`
  (part name + media type) — a semantic and byte fixed point for the preserved
  subset.

## 8. Acceptance gates

1. Import with `retain_source` captures referenced image bytes + safe unknown
   parts within bounds; over-bound parts are excluded with findings.
2. `PreserveWhenSafe` export of an *edited* document (e.g. a paragraph inserted)
   re-emits the image `draw:frame` and repackages the bytes; the result reopens
   through the bounded admission layer with the image intact.
3. Byte fixed point for the preserved subset: preserve → reopen → preserve is
   byte-identical.
4. Cross-format export (ODT→DOCX/JSON/TXT) still drops ODF-native retained parts
   with explicit findings.
5. Over-bound, unsafe-path, and missing-part cases degrade deterministically,
   never abort a valid document.
6. Workspace test, strict Clippy, rustdoc, MSRV, WASM, format, diff, and
   fuzz-build gates pass; fuzz const literals updated for the new limit fields.

## 9. Delivery checkpoints

1. **Retention plumbing**: `OdfRetainedParts` + import capture + `OdtSourceState`
   wiring; no export behavior change yet (parts retained but unused).
2. **`draw:frame` export**: preserving writer emits `draw:frame` for `Drawing`
   nodes and repackages retained image parts; `PreserveWhenSafe` routes to it;
   descriptor advertises the capability. Image round-trip fixed point.
3. **Safe unknown-part carry**: extend retention beyond referenced images to
   bounded safe unknown parts; report what is and is not carried.
4. Each checkpoint gets its own adversarial review (path safety, bounds,
   determinism, atomicity) before the next.

## 10. Open questions

- Anchored/block-level `draw:frame` (page/paragraph anchors) is out of the doc 95
  import subset; preserving writer emits only the inline subset until that lands.
- Whether to retain `styles.xml`-referenced media (backgrounds, list bullets as
  images) in checkpoint 2 or defer to checkpoint 3.
- Interaction with a future signature-preservation path (doc 95 §2): a semantic
  edit already invalidates signatures, so retained parts must not resurrect a
  signature as valid.

## 11. Normative references

- OASIS, [OpenDocument Version 1.4, Part 2: Packages](https://docs.oasis-open.org/office/OpenDocument/v1.4/os/part2-packages/OpenDocument-v1.4-os-part2-packages.html).
- `94-MULTI-FORMAT-IMPORT-EXPORT-ARCHITECTURE.md` (retention/preservation axes, tagged sidecar envelopes).
