# Normalized Schema v1: Legacy VML Pictures Design

**Status:** Accepted — 2026-07-25 (repository owner directive: complete the model)
**Tracker:** P1A-019 (schema v1 semantic extension), VML-images slice
**Decision basis:** ADR-027, schema v1 (`38-…`), drawings (P1A-021), importer
no-skip audit (`P1A-025`)

## Why

The no-skip audit found that only DrawingML pictures (`w:drawing` → `a:blip@r:embed`)
are modeled; a legacy VML picture (`w:pict` → `v:imagedata@r:id`) had its image
reference reported but dropped. Older producers and pasted content still emit VML
pictures. This slice models them — with **no model change**: a VML picture is just
an image, so it maps to the existing `InlineNode::Drawing` referencing a `MediaId`.

## Model

None. A VML picture reuses `Drawing { id, media: MediaId, extent: Option<Extent> }`.
VML sizes shapes in CSS (`style="width:..pt"`), which the model does not capture,
so `extent` is always `None` for a VML picture.

## Import

- `w:pict` (inside a run) opens a picture context (`pict_depth`); its
  `v:imagedata@r:id` is resolved through the **same** media table/index as a
  DrawingML picture (the `r:id` is a main-document image relationship id →
  `MediaId`). On the closing `</w:pict>` a resolvable id becomes a `Drawing`
  segment; an unresolved id, or an image-less shape (a VML **text box**, which is
  handled separately by `w:txbxContent`), is reported.
- `pict_depth` and the pending image id are saved/restored across text-box
  frames, so a VML picture nested inside a text box (and vice versa) is counted
  independently and cannot corrupt the enclosing context.
- A VML picture in a header/footer/note part resolves against that part's media
  index — empty in the current slice — so it is reported (its modeling shares the
  extra-part-media follow-up), never silently dropped.

## Out of scope (still reported)

VML shape geometry, CSS sizing, wrap/anchor, OLE objects (`w:object`), and VML
fills/strokes. VML text boxes are modeled by the existing text-box slice.
