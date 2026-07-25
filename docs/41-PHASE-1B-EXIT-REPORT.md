# Phase 1B Exit Report — Semantic DOCX Writer

**Status:** Accepted
**Report date:** 2026-07-26
**Implementation revision:** `a733316`
**Tracker:** P1B-004 through P1B-006, P1B-FONT-1A3, P1B-R1/R4, and the P1B
property-coverage series
**Design:** `39-PHASE-1B-SEMANTIC-DOCX-WRITER-DESIGN.md`

## Decision

Phase 1B is complete and accepted. The semantic writer (`casual-doc-export`'s
`write_document`) re-emits an edited normalized model as a valid `.docx`
package, and the modeled surface round-trips under the semantic fixed point:

> `import(Semantic) → write_document → reopen == identical model`

This report applies ADR-023 as amended: semantic DOCX round-trip baselines begin
with this Phase 1B writer (not the originally-scheduled Phase 2). It makes no
layout, pagination, or rendering claim — those begin with Phase 1C onward.

## What the writer emits

Every construct family currently in the schema-v1 model is written and reopens
identically:

| Area | Coverage |
| --- | --- |
| Body flow | paragraphs, runs, text, tabs, breaks |
| Inline constructs | hyperlinks, fields, bookmarks, drawings + media, text boxes, note/comment references, tracked-change revisions, inline content controls |
| Block constructs | tables (grid, merges, borders, shading, margins, nested), block content controls |
| Run properties | ~85% of `CT_RPr` — styles, bold/italic/underline/strike, color, size, four font slots + hint, caps/smallCaps, hidden/webHidden, dstrike, vertAlign, highlight, emphasis, spacing/kerning/position, language, effects (outline/shadow/emboss/imprint), rtl, snapToGrid, specVanish, run border + shading |
| Paragraph properties | ~90% of `CT_PPr` — style, numbering, alignment, indentation, spacing, keep\*/pageBreakBefore/widowControl/contextualSpacing/suppressLineNumbers, outline level, borders, shading, tabs, section break, the bidi/wordWrap/kinsoku/snapToGrid/mirrorIndents/adjustRightInd/suppressAutoHyphens/overflowPunct/topLinePunct/autoSpace\* toggles, textAlignment, and the paragraph-mark `rPr` |
| Table properties | ~85% of `tblPr`/`trPr`/`tcPr` — alignment, width, layout, look, borders, shading, cell margins, indent, cell spacing, overlap, caption/description, row alignment/spacing, cell fitText/hideMark |
| Section properties | ~85% of `w:sectPr` — page size, margins, columns (count/space/separator), break type, page numbering, vertical alignment, title page, doc grid, header/footer references, per-paragraph section breaks (multi-section) |
| Definition parts | styles.xml, numbering.xml, fontTable.xml (incl. embedded `.odttf` fonts), theme1.xml font scheme, footnotes/endnotes, comments, headers/footers, settings.xml (font-embedding flags) |

## Deliverables

| Phase 1B deliverable | Status | Evidence |
| --- | --- | --- |
| Semantic writer core (body, tables, inline constructs) | Pass | `casual-doc-export/src/semantic.rs`; `mod semantic_tests` fixed-point suite. |
| Direct run/paragraph/table/section property writers | Pass | `write_run_properties`, `write_paragraph_properties`, `write_table_properties`/row/cell, `write_section_properties`. |
| Definition-part writers | Pass | styles/numbering/fontTable/theme/notes/comments/headers/footers/settings emitters + content-type overrides + relationships. |
| Multi-section documents | Pass | `ParagraphProperties.section_break`; per-paragraph `w:sectPr` in `w:pPr`; `multi_section_survives_the_semantic_round_trip`. |
| Embedded fonts + settings flags | Pass | `EmbeddedFontSet`; `.odttf` parts (Solution A); `word/settings.xml` `w:embedTrueTypeFonts` etc. |
| Interoperability fixes | Pass | P1B-R4 (`w:cstheme` data-loss on real Word files), P1B-R1 (invalid table `Justify`). |
| Writer fidelity harness (P1B-006) | Pass | Real-producer corpus semantic round-trip (7/7 byte-identical) + LibreOffice validity check. |

## Exit Gates

| Exit gate | Status | Evidence |
| --- | --- | --- |
| Semantic fixed point on hand-crafted fixtures | Pass | Every construct/property family has an `import → write → reopen == model` test. |
| Semantic fixed point on real producer output | Pass | `real_producer_corpus_survives_the_semantic_round_trip` — all 7 real Word/LibreOffice corpus documents byte-identical. |
| External validity | Pass | `writer_output_opens_in_libreoffice` (`#[ignore]`-gated) — the re-written packages convert cleanly in LibreOffice. |
| Additive model evolution | Pass | Every field uses `#[serde(default, skip_serializing_if = …)]`; the v0→v1 migration golden and all pre-existing snapshots remain byte-identical. |
| Format / lint / MSRV / platforms | Pass | `cargo +1.96.0 fmt`, clippy `-D warnings`, and the full CI matrix on every merged PR. |
| No silent loss | Pass | Constructs not yet in the semantic model are preserved verbatim in Retention mode (the tier-1 byte floor) and every unmapped construct is recorded in the bounded compatibility report. |

## The no-silent-loss guarantee

Phase 1B does **not** claim 100% semantic modeling of WordprocessingML. Semantic
model coverage is ~90% of real-world common constructs; the long tail (math/OMML,
VML shapes, OLE, field computation, and the rarer property vocabularies) is not
yet a first-class model value. That tail is **not lost**: in Retention mode the
original package bytes are preserved and reproduced, and in Semantic mode every
dropped construct is reported. The writer's fidelity is exact for everything the
model represents; Retention is the floor for everything it does not yet.

## Known follow-ups (tracked, non-blocking)

- Long-tail semantic modeling (math/OMML, VML/`pict` shapes, OLE, field results)
  — opportunistic; Retention covers them until modeled.
- `P1A-006` deterministic input-derived import id seed (order-independent;
  native/WASM golden equivalence) — matters before render snapshots, does not
  affect the writer fixed point.
- Font resolution/substitution/metrics (`40-FONT-MANAGEMENT-DESIGN.md`, accepted
  full scope) — the font *data* is modeled and round-tripped; resolution is
  Phase 1C+ work.

## Next phase

With the writer complete, the project pivots from OOXML fidelity to visual
layout and rendering. Phase 1C begins the display path — font resolution, text
shaping, line breaking, paragraph layout, page construction, a backend-neutral
display list, and canvas rendering — the desktop-first track toward the Tauri
host. Retention remains the no-silent-loss floor throughout.
