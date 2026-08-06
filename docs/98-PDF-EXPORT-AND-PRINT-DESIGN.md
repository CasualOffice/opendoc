# 98 — PDF Export and Print Design

**Status:** Proposed implementation design (not yet accepted). Effort estimates are planning-grade (±40%).
**Date:** 2026-08-05
**Depends on:** ADR-003 (backend-neutral display list); `casual-doc-layout::display::DisplayList`; `casual-doc-render` (tiny-skia backend, reference implementation); `paginate.rs` per-page display lists; `casual-doc-io` format registry; `40-FONT-MANAGEMENT-DESIGN.md`; `94-ORACLE-VISUAL-FIDELITY-HARNESS-DESIGN.md`; `21-PARSER-LIMITS.md`; `20-ERROR-CODE-REGISTRY.md`.
**Owner decision required before implementation:** ADR-031 (build-vs-buy for the PDF writer + font subsetter) and the Phase-2 scope gate (§10).

## Problem

The engine can render a document to screen (`DisplayList` → `casual-doc-render` CPU
raster) and paginate it against a LibreOffice oracle, but it cannot produce a
**PDF**. The support matrix (`18-SUPPORT-MATRIX.md`) lists *"PDF render/export —
Backend decision pending"*, the architecture doc names *"Headless raster/PDF"* as
the third paint target, and the ADR register lists *"PDF generation backend"* as a
pending decision. This document specifies that backend end to end.

Two hard requirements shape the whole design:

1. **A real, proper PDF — never a page of images.** Output must carry a genuine
   text layer (selectable, searchable, copyable), vector graphics, and embedded
   **subset** fonts. Text-as-outlines or rasterized pages are explicitly rejected.
2. **Two simultaneous parity targets:**
   - **Editor parity (structural):** the PDF must look identical to what the user
     sees in the editor. Any drift is a defect.
   - **Word feature parity (capability):** the PDF must expose the same *feature
     set* Microsoft Word's "Export → Create PDF" produces (outline/bookmarks,
     hyperlinks, metadata, tagged/accessible structure, PDF/A option). This is
     feature parity, **not** an independent rendering target — layout fidelity to
     Word is earned in the layout engine, not here.

## The parity model (the single most important design constraint)

### Editor parity is structural, not aspirational

The editor pipeline is `model → layout → DisplayList → tiny-skia`. The PDF exporter
consumes **the exact same `DisplayList`, produced by the same layout pass**, and
maps each `PaintItem` to PDF content-stream operators. Because both backends share
pagination, shaping, and glyph positions, editor↔PDF drift is **architecturally
impossible** — provided the exporter never does its own layout. This yields the
governing rule of the whole feature:

> **The PDF exporter performs zero layout, zero re-shaping, and zero metric
> adjustment. It is a deterministic transcription of the shared `DisplayList`. It
> is never permitted to be "smarter" than the editor.**

Any attempt to improve fidelity inside the exporter (nudging positions to look
"more like Word") would *create* the editor↔PDF drift the requirement forbids. The
exporter's only job is faithfulness to the display list.

### Word parity is a layout goal the PDF inherits — with one exception

Because the PDF must match both the editor and Word, the editor must match Word.
The exporter cannot add Word *rendering* fidelity the editor lacks; it inherits
exactly the layout fidelity the on-screen renderer already has (tracked separately
under the fidelity-corpus / pagination work, `60`/`94`/`P1F`). **Word *feature*
parity, by contrast, is this document's job** — see §6 (semantic features) and §10
(tagged PDF / PDF-A).

### The font parity trap (must-fix, not optional)

The one place editor↔PDF parity most easily breaks is fonts. The `system-fonts`
build lays out with real system faces (Arial/Times/Calibri); the deterministic /
WASM build uses bundled substitutes (Roboto). Different face → different metrics →
different line breaks → different pagination. Therefore:

> **The PDF must embed and emit the *exact same font faces the layout pass used* —
> the same `FontId`, the same bytes from `FontRegistry`, the same glyph indices at
> the same advances. The exporter must never re-resolve fonts independently.**

`GlyphRun` already carries glyph indices into those specific faces, so embedding the
same face and emitting the same GIDs at the same advances produces a PDF that is
glyph-for-glyph identical to the editor. This is enforced by a CI golden (§9).

## Goals

- A `casual-doc-pdf` backend that transcribes a per-page `DisplayList` into a valid,
  deterministic PDF (ISO 32000-1) with a real text layer and embedded subset fonts.
- Byte-deterministic output: identical input → identical bytes (no timestamps, no
  random file IDs; IDs derived from content). Same discipline as the rest of the repo.
- Word *feature* parity delivered in phases (§6, §10): outline/bookmarks, internal +
  external links, document metadata; then tagged/accessible PDF and PDF/A as a gated
  second phase.
- Print support that reuses the PDF pipeline (§8), with host-owned device policy.
- Integration through the existing `casual-doc-io` registry and the WASM/SDK
  `export_as` / `available_export_formats` seam.
- Security: untrusted font bytes and image bytes handled under the `21-PARSER-LIMITS`
  discipline; bounded, fail-closed, no panics on adversarial input.

## Non-Goals

- Any *rendering/layout* fidelity work — that lives in the layout engine, not here.
  This document adds **no** shaping, line-breaking, or pagination logic.
- Round-trip PDF *import*. This is export-only.
- Text-as-outlines or rasterized-page output (explicitly rejected).
- Editing PDFs, PDF forms (AcroForm) authoring, digital signatures, JavaScript, or
  embedded multimedia.
- Font *procurement*. Which faces are legally embeddable is a licensing decision
  (§7), out of engineering scope; this document only specifies the embedding
  mechanism and the fallback policy.

## Architecture

```
                    ┌─────────────────────── shared layout pass ───────────────────────┐
   model ─▶ layout ─┤  paginate.rs → Vec<DisplayList> (one per page)  +  FontRegistry   │
                    │  (NEW) StructureTree  — logical/semantic side-channel (§6)         │
                    └───────────────┬───────────────────────────┬──────────────────────┘
                                    │                           │
                       ┌───────────▼──────────┐     ┌──────────▼───────────┐
                       │ casual-doc-render     │     │ casual-doc-pdf (NEW) │
                       │ tiny-skia (screen)    │     │ DisplayList → PDF    │
                       │ REFERENCE traversal   │     │ ops + semantics      │
                       └───────────────────────┘     └──────────┬───────────┘
                                                                │
                                     ┌──────────────────────────┼───────────────────────┐
                                     │ pdf-writer (container) │ font subsetter │ image XObjects │
                                     └──────────────────────────┼───────────────────────┘
                                                                │
                                     casual-doc-io registry ◀───┘  format_id = "pdf"
                                     WASM/SDK export_as("pdf", …)  → bytes / print target (§8)
```

New crate **`casual-doc-pdf`** (naming per workspace convention). It depends on
`casual-doc-layout` (for `DisplayList`/`PaintItem`/`GlyphRun`/`FontRegistry`) and
`casual-doc-model` (for `Definitions.media`, colors, metadata). It must **not**
depend on `casual-doc-render` — it is a sibling backend consuming the same seam
(ADR-003). It reuses `casual-doc-render`'s traversal *shape* as the reference for
correctness, not as a dependency.

## Step-by-step design

The work is ordered so each step lands as an adversarially-reviewed slice (the
repo's working pattern) and so nothing downstream is blocked by an undecided upstream.

### Step 0 — ADR-031: PDF-writer + subsetter build-vs-buy (BLOCKING)

Decide, against `deny.toml` and `unsafe_code = forbid`, whether to:

- **(A) Hand-roll** a minimal PDF writer and TrueType/CFF subsetter (max control,
  zero new supply-chain surface, deterministic by construction; higher eng cost —
  the subsetter is real work); or
- **(B) Vet crates** — e.g. a low-level PDF writer + a font-subsetting crate —
  against the dependency policy (lower eng cost; requires audit, `cargo-deny`
  clearance, determinism verification, and pinning).

This choice moves Phase 0 by ~1.5 sprints (§ effort). **No code starts before it is
recorded as ADR-031.** Recommendation: prefer (B) for the *writer* (well-trodden,
low risk) and evaluate (A) vs (B) for the *subsetter* specifically, since subsetting
is the correctness- and security-critical part.

### Step 1 — PDF container writer

- Object model: indirect objects, xref table (or cross-reference streams for
  compactness), document catalog, page tree, per-page resource dictionaries.
- Content streams: Flate-compressed (reuse the workspace `zip`/flate stack already
  present).
- **Determinism:** object numbering is assignment-order stable; the file `/ID` is
  derived from a content hash, not time/random; no `/CreationDate`/`/ModDate` unless
  sourced from `DocumentProperties` (and then normalized). This mirrors the repo's
  existing byte-fixed-point contracts.
- Page geometry comes straight from pagination (page size + margins already in twips;
  convert twips → PDF points, 1 pt = 20 twips, origin flip to PDF's bottom-left).

### Step 2 — Vector paint mapping (`DisplayList` → content operators)

Walk `PaintItem` in back-to-front order (identical order to `casual-doc-render`) and
emit operators. The mapping is mostly mechanical because the display primitives
already exist:

| `PaintItem` | PDF operators |
|---|---|
| `Rect { fill, stroke }` | `re` + `f`/`S`/`B` with `rg`/`RG` colors, `w` line width |
| `Ellipse` / `RoundedRect` | Bézier path approximation (same math as the raster backend) |
| `Polygon` | `m`/`l…`/`h` + fill/stroke |
| `Line { from, to, stroke }` | `m`/`l`/`S`; dash via `d` |
| `Shape { geometry, fill, stroke, head/tail_end, transform }` | path build + `sh` shading for gradients; `cm` for transform; arrowheads as explicit paths |
| `Image { media, rect, crop, transform }` | image XObject (Step 4), `cm` place, clip for crop |
| `Glyphs { run }` | text object (Step 3) |
| `PushClip`/`PopClip` | `q` + `re W n` … `Q` |

Gradients map to PDF axial/radial shadings (`ShadingType 2/3`); dashes to `d`;
`ShapeTransform` (rotate/flip about center) to a `cm` matrix. All geometry is device-
independent twips → points; **no** device scale is applied (PDF is resolution-free),
which is the one deliberate divergence from the raster backend's paint path.

### Step 3 — Text as a real, selectable layer

For each `GlyphRun`:

- Open a text object (`BT`/`ET`), set the embedded font + size (`Tf`), set fill color
  (`rg`), set the text matrix (`Tm`) from `run.origin` (+ `character_scale_percent`
  as a horizontal-scale `Tz`, matching how the raster backend scales outlines).
- Emit glyphs **by glyph index** into a `Type0`/CID font with `Identity-H` encoding
  and explicit positioning from `Glyph.advance` (`TJ` with advance adjustments), so
  the on-page positions equal the display list exactly.
- Decorations (`underline`/`strikethrough`/`double_strike`), `highlight`, and run
  `shading` are painted as rects behind/over the run — identical to the raster
  backend — not as font features.
- **`ToUnicode` CMap:** `Glyph.cluster` is the UTF-8 byte offset into the paragraph's
  source text. This is exactly the data needed to build the `ToUnicode` map (glyph →
  source scalar[s]), which makes selection, search, and copy work. This is what
  separates a "proper" PDF from an outline dump, and the mapping data already exists
  in the display list.
- List-marker runs (`is_marker`) are emitted as text too, but tagged as artifacts in
  the Phase-2 structure tree (§10) so they don't pollute the reading order.

### Step 4 — Image XObjects

- Resolve `PaintItem::Image.media` against `Definitions.media` (the exporter is given
  the media bytes by the host, exactly as the raster backend is).
- JPEG → `DCTDecode` passthrough (no re-encode). PNG/other → decode then `FlateDecode`
  (or `DCTDecode` for photographic content per the size/quality knob, §6).
- Honor `crop` (source sub-rect via clip + scaled placement) and `transform`
  (rotate/flip via `cm`).
- Reuse `casual-doc-render`'s decode limits verbatim (`MAX_DECODED_IMAGE_*` from
  `21-PARSER-LIMITS`): bounded dimensions/pixels/bytes, fail-closed with a finding.

### Step 5 — Font embedding + subsetting (the long pole)

For every `FontId` referenced by any emitted `GlyphRun`:

- Pull the exact face bytes from `FontRegistry` (`face_bytes`/`FontBytes`/`DynFace`)
  — the same face the shaper used (parity guarantee, §"font parity trap").
- **Subset:** retain only the referenced glyph indices. TrueType path: rebuild
  `glyf`/`loca`/`cmap`/`hmtx`/`maxp`/`head`/`hhea` for the kept set; CFF/OTF path:
  subset the CharStrings INDEX. `.ttc`/`.otc` collections: extract the single face by
  index (`DynFace` already carries the face index).
- Wrap as a `Type0` font with a CID-keyed descendant (`CIDFontType2` for TrueType /
  `CIDFontType0` for CFF), `Identity` `CIDToGIDMap`, a `W` array from the run advances
  /`hmtx`, a `FontDescriptor` (flags, bbox, ascent/descent/stemV), and the subset
  embedded (`FontFile2`/`FontFile3`). Subset-tag the PostScript name (`ABCDEF+Name`).
- **Untrusted-font hardening:** the face bytes may originate from an imported
  document. Subsetting parses untrusted tables → apply `21-PARSER-LIMITS` bounds
  (table sizes, glyph counts, offsets), reject malformed tables fail-closed with a
  reported finding, and never panic. This is the highest-risk security surface in the
  feature and gets its own fuzz target (§9).
- **Fallback policy (no images):** if a face genuinely cannot be embedded (unsupported
  outline format, or a licensing flag the host declines), the run is emitted with a
  substitute embeddable face and a **loss finding** — never rasterized, never dropped
  silently. Word offers a "bitmap text when fonts can't be saved" fallback; we do not
  (it violates requirement 1).

### Step 6 — Semantic features (Word feature parity, Phase 1)

A flat display list has no logical structure, so feature parity needs a **semantic
side-channel** carried from the model through layout to page positions. Introduce a
`StructureTree` (logical order: headings, paragraphs, lists, tables, links, figures)
with per-node page + rect resolved from the same layout pass. Every Phase-1 feature
hangs off this one bridge:

- **Document metadata** → `/Info` dict + XMP packet, from `DocumentProperties`
  (title/author/subject/keywords/lang). Nearly free.
- **External hyperlinks** → `Link` annotations with `URI` actions; rects come from the
  link runs (layout must expose link geometry — a small, additive layout change). The
  URI scheme allowlist mirrors the import/export security allowlist (no
  `javascript:`/unsafe `data:`), consistent with the ODT work.
- **Internal links / TOC / cross-refs** → `GoTo` actions to named destinations minted
  from bookmark anchors (model already has bookmarks; T1 ODT work modeled them).
- **Outline / bookmark tree** → `/Outlines` built from Heading-style paragraphs
  (named-style identity exists), i.e. Word's "Create bookmarks using Headings".
- **Page range / selection** → emit a subset of pages.
- **Image quality knob** (Standard vs Minimum size) → image re-encode/downsample policy
  in Step 4.

### Step 7 — Public surface + registry wiring

- Register a `"pdf"` export adapter in `casual-doc-io` (export-only descriptor).
- Route through the existing WASM/SDK `export_as(format_id, mode)` and advertise it in
  `available_export_formats`. `mode` carries options (page range, metadata inclusion,
  image quality, and — Phase 2 — tagged/PDF-A toggles).
- SDK facade method on `casual-doc-sdk` (consumers depend on the SDK, not the crate —
  ADR-004). Emit a compatibility/loss report (unembeddable fonts, dropped features)
  in the same shape as the other adapters' findings.

## Fonts and licensing (must be settled early)

Requirement 1 (real text) forces font **embedding**, which forces **redistribution
rights** for the embedded faces. Two independent concerns:

1. **Mechanism** (this doc, Step 5): subset + embed the exact layout face.
2. **Rights** (owner decision, out of eng scope): the bundled substitute faces must be
   embeddable (verify their licenses permit PDF embedding — most open faces do). To
   reach true *Word layout* parity you additionally need metric-compatible or genuine
   MS faces (Calibri/Cambria → Carlito/Caladea substitutes are the standard answer)
   **in the layout engine**, not here. Flag: the closer you want editor+PDF to match
   Word's line breaks, the more this licensing choice matters — but it is a layout-
   engine input, and this backend faithfully embeds whatever the layout used.

## Print

Print reuses the PDF pipeline; it is not a second renderer.

- **Web/WASM host:** "Print" = generate the PDF (same path) and hand the blob to the
  browser print dialog (`window.print()` on a PDF object/hidden frame). No new
  rendering code. ~1–2 eng-days.
- **Native host:** OS device policy (CUPS/Quartz, GDI/XPS) is **host-owned** (AGENTS.md
  — hosts own device/resource policy; the runtime must not). The contract: the runtime
  hands the host either the PDF or per-page `DisplayList`s; the host drives the printer.
  A thin host-side adapter, ~2–4 eng-days, not core work.

Net: budget print as a small addition to PDF, provided "print = PDF routed to a print
target" is accepted (the standard approach for an embeddable engine).

## Security

- Untrusted **font bytes** (Step 5) and **image bytes** (Step 4) are parsed under
  `21-PARSER-LIMITS`: bounded sizes/counts/offsets, fail-closed, no panics, reported
  findings. Dedicated fuzz targets for the subsetter and the image path.
- URI/link schemes mirror the existing import/export allowlist; blocked schemes degrade
  to a reported remainder, never emitted (a non-PDF-origin model must not smuggle a
  `javascript:` link into an annotation).
- No JavaScript, launch actions, embedded files, or auto-actions are ever written.
- Determinism is a security property here too (reproducible builds / no entropy leak):
  no time, no RNG, content-derived `/ID`.

## Testing and gates

Every slice must pass the standard gates: `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, WASM
check, plus:

- **Determinism golden:** export a fixture twice → byte-identical; export₂ == export₃
  fixed point (the repo's standard contract).
- **Editor↔PDF parity golden (the key invariant):** for each corpus page, assert the
  PDF text-run positions/advances/glyph-ids equal the shared `DisplayList` geometry
  (zero/epsilon). This converts "the PDF matches the editor" from a hope into a CI
  gate — if the exporter ever does something the display list didn't say, it fails.
- **Structural validity:** validate against a PDF parser (and, if PDF/A is in scope,
  a veraPDF-class check on the re-bless workflow, not per-CI).
- **Oracle extension:** extend the `docs/94` harness to diff the PDF's page geometry +
  text layer against the LibreOffice-PDF oracle (Word as tie-break authority per §"Word
  parity"). Reuses the existing pinned-`soffice` re-bless flow.
- **Fuzz:** font subsetter and image decode paths get `cargo fuzz` targets with the
  all-fields-explicit limit literals (note: adding a new `PdfExportLimits` field breaks
  those literals — update them or `cargo fuzz build` fails, same gotcha as ODT).
- **Text-layer proof:** automated `pdftotext` extraction of a known fixture equals the
  source text (selectable/searchable proof), and a copy-paste spot check.

Docs to update per slice: this doc, `18-SUPPORT-MATRIX.md`, `08-ADR-REGISTER.md`
(ADR-031 + any Phase-2 ADRs), `14-EXECUTION-TRACKER.md`, and a new
`99-PDF-EXPORT-PROFILE.md` (supported/loss/limits table, mirroring `95`/`96`).

## Phasing, slices, and effort

Estimates are planning-grade (±40%), for one experienced Rust engineer fluent in this
codebase, and **include** the per-slice adversarial review + all gates. Sprint = 2
weeks ≈ 8 productive eng-days after reviews. Biggest swing = ADR-031 (crate vs
hand-roll). Excluded: layout-engine Word-fidelity work and font procurement.

### Phase 0 — proper vector PDF (selectable text, embedded fonts, images)

| Slice | Work | Eng-days (crate) | Eng-days (hand-rolled) |
|---|---|---|---|
| PDF-PH0-0 | ADR-031 decision + writer skeleton (objects, xref, page tree, deterministic ID, Flate) | 2–4 | 4–7 |
| PDF-PH0-1 | Vector paint mapping (rects/lines/ellipse/roundrect/polygon/shapes/gradients/dashes/clips) | 4–6 | 4–6 |
| PDF-PH0-2 | **Font subsetting + embedding** (TrueType+CFF, Type0/CID, ToUnicode, `.ttc`, dynamic blobs, untrusted-font limits) | 5–9 | 12–20 |
| PDF-PH0-3 | Image XObjects (JPEG passthrough, PNG→Flate, crop/transform, decode limits) | 3–5 | 3–5 |
| PDF-PH0-4 | Registry/SDK/WASM wiring + determinism & parity goldens + oracle extension | 3–5 | 3–5 |
| **Phase 0 total** | | **~17–29** | **~26–43** |

→ ≈ **2–3.5 sprints** (crate) / **3.5–5.5 sprints** (hand-rolled). Font subsetting is
the long pole and the reason ADR-031 matters.

### Phase 1 — semantic features (Word-equivalent for ~95% of users)

| Slice | Work | Eng-days |
|---|---|---|
| PDF-PH1-0 | `StructureTree` semantic bridge (layout emits link rects + heading/anchor structure) | 3–5 |
| PDF-PH1-1 | Metadata (`/Info` + XMP) | 1–2 |
| PDF-PH1-2 | External hyperlink annotations (scheme allowlist) | 2–3 |
| PDF-PH1-3 | Internal links / TOC / cross-ref `GoTo` destinations | 3–5 |
| PDF-PH1-4 | Outline / bookmark tree from Heading styles | 2–4 |
| PDF-PH1-5 | Page range / selection + image-quality knob | 1–2 |
| **Phase 1 total** | | **~12–21** |

→ ≈ **1.5–2.5 sprints.**

### Print

| Slice | Work | Eng-days |
|---|---|---|
| PDF-PRINT-0 | Web: PDF → browser print handoff | 1–2 |
| PDF-PRINT-1 | Native host print adapter contract (host-side) | 2–4 |
| **Print total** | | **~3–6** |

→ ≈ **0.5 sprint.**

### Phase 2 — accessibility / archival (GATED — decide in/out up front, §10)

| Slice | Work | Eng-days |
|---|---|---|
| PDF-PH2-0 | **Tagged PDF** (structure tree → marked content, reading order, heading/list/**table** tags, alt text, `Lang`, artifacts) | 15–25 |
| PDF-PH2-1 | **PDF/A** conformance (ICC/OutputIntent, XMP+PDF/A id, embedding guarantees, validation; requires tagging) | 8–14 |
| PDF-PH2-2 | Encryption / password (AES + encrypt dict) | 3–5 |
| PDF-PH2-3 | Comments / markup export toggle | 3–5 |
| **Phase 2 total** | | **~29–49** |

→ ≈ **3.5–6 sprints.**

### Roll-up

| Target | Eng-days | Sprints (1 eng) | Calendar (1 eng) |
|---|---|---|---|
| **Ship-worthy: Phase 0 + Phase 1 + Print** (no tagging/PDF-A) | **~32–56** | **~4–7** | ~8–14 weeks |
| **Word feature-complete** (+ Phase 2) | **~61–105** | **~8–13** | ~16–26 weeks |

**Parallelism:** Phase 0's font work (PH0-2) and paint work (PH0-1) can run as two
concurrent tracks (2 engineers compress Phase 0 to ≈2 sprints). Phase 1 largely
serializes after the container exists.

## Open decisions (§10)

1. **ADR-031 — writer/subsetter build-vs-buy** (BLOCKING; §Step 0). Moves Phase 0 by
   ~1.5 sprints.
2. **Phase-2 in or out.** Tagged PDF + PDF/A roughly *doubles* the project. Decide up
   front, because the `StructureTree` (§6) should be designed with tagging in mind from
   day one rather than retrofitted. Drive by whether accessibility/archival compliance
   is an actual customer requirement.
3. **Font substitute licensing** (§7) — owner/legal, not engineering. Caps how close to
   Word's line breaks either the editor or the PDF can get.
4. **Oracle tie-break policy** — reaffirm §"Word parity": LibreOffice PDF = automatable
   CI oracle, Word = correctness authority on disagreement (already the repo's posture
   for on-screen rendering; PDF stays consistent with it).

## Recommendation

Adopt Phase 0 + Phase 1 + Print as the committed scope (**≈ 4–7 sprints, one
engineer**) — a proper vector PDF with selectable text, embedded fonts, links,
outline, metadata, and print: genuine Word-equivalence for what users notice, with
editor parity guaranteed structurally and by a CI golden. Gate Phase 2 (tagged
PDF / PDF-A, **≈ doubles the effort**) on a concrete accessibility/archival
requirement. Settle ADR-031 first; nothing else starts before it.
