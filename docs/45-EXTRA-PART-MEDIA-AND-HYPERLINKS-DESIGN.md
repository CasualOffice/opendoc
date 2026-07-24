# Extra-Part Media and Hyperlinks Design

**Status:** Accepted — 2026-07-25 (repository owner directive: complete the model)
**Tracker:** P1A-019 (schema v1 semantic extension), extra-part media/links slice
**Decision basis:** ADR-027, media references (P1A-016), notes (`42-…`),
headers/footers (`43-…`), `DocxPackage::part_relationships`

## Why

Notes, headers, and footers were modeled, but images and external hyperlinks
*inside* them were only reported (their `r:embed`/`r:id` resolved against an empty
index). Those relationships live in each part's own `_rels` (`part_relationships`,
already landed), so this slice resolves them and makes the images/links
first-class — the same `Drawing`/`Hyperlink` nodes as in the main body.

## Model

None. Reuses `MediaReference` (media table), `InlineNode::Drawing`, and
`InlineNode::Hyperlink`.

## Import

- The media table is built into **one shared table** across the whole package.
  The main document's images are added first (identical to before), then each
  extra part's images are added just before that part is parsed. `media::build_into`
  allocates **one `MediaId` per relationship** (no de-duplication), so a
  main-document-only file's media table and id sequence are byte-identical to
  before this change, and returns that part's `relationship_id → MediaId` index.
  A genuinely shared image (referenced from two parts) yields two references to
  the same part — harmless, and the deterministic price of preserving exact
  backward-compat.
- Relationship ids are **per-part**, so each part's parser receives its **own**
  media index and hyperlink map. A header's `rId1` and the main document's `rId1`
  resolve independently. `import_package` builds a `PartSources { xml, images,
  hyperlinks }` per extra part via `part_relationships`.
- Deterministic id order: document → styles → numbering → media (all, deduped) →
  footnotes → endnotes → headers → footers → body.
- `MediaReference.relationship_id` records the first source that introduced an
  image part (main document preferred, as it is aggregated first).
- An image/hyperlink relationship that does not resolve (out of domain, or a
  target part that is not admitted) is reported, never silently dropped. Internal
  hyperlinks (`w:anchor`) need no relationship and already worked.

## Backward-compat

Additive. A document with no extra-part images builds the identical media table
and id sequence as before (dedup only collapses a genuinely-shared image part,
which for a main-document-only file changes nothing unless two relationships
already pointed at the same image — a case the deduped model represents more
faithfully).

## Out of scope

VML images inside extra parts still depend on that part's media index (now
populated); OLE objects; linked (non-embedded) images.
