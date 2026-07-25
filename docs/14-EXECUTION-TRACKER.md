# Execution Tracker

## Purpose

Records project execution state. Each entry is terse: id, title, status, and a
one-sentence outcome plus its design reference. Full detail lives in git history,
PRs, and the design docs — chiefly the consolidated `38-SCHEMA-V1-DESIGN-REFERENCE.md`
(referenced by anchor, e.g. `#tables`). Update this file when work begins,
changes scope, or finishes.

## Status Values

Not started · Researching · Designing · Finalizing · Ready · In progress ·
Blocked · In review · Done

## Foundation Tracker

| ID | Workstream | Status | Notes |
| --- | --- | --- | --- |
| F-001 | Repository bootstrap docs | Done | Root/agent/license/process docs added. |
| F-002 | Project glossary | Done | `17-GLOSSARY.md`. |
| F-003 | Support matrix | Done | `18-SUPPORT-MATRIX.md`. |
| F-004 | CI design | Done | `15-CI-AND-RELEASE-GATES.md`. |
| F-005 | Workspace scaffold design | Done | `19-WORKSPACE-SCAFFOLD-DESIGN.md`. |
| F-006 | Error code registry | Done | `20-ERROR-CODE-REGISTRY.md`. |
| F-007 | Parser limits spec | Done | `21-PARSER-LIMITS.md`. |
| F-008 | Normalized schema v0 design | Done | `22-NORMALIZED-SCHEMA-V0.md`. |
| F-009 | DOCX fixture corpus plan | Done | `23-DOCX-FIXTURE-CORPUS.md`. |
| F-010 | Competitive analysis pass 1 | Done | `12-COMPETITIVE-ANALYSIS.md`; sources checked 2026-07-24. |
| F-011 | Phase 1 capability decomposition | Done | Independent scope/exit gates per phase; ADR-025, `06-…`. |
| F-012 | Apache-2.0 license policy | Done | ADR-026. |
| P0-001 | Deterministic model transaction slice | Done | Blank doc, grapheme insertion, atomic transaction, snapshots; all gates pass. |
| P0-002 | Transaction semantics and history | Done | Insert/delete/split/join, mapping, inverse, history; 17 tests + doc test. |
| P0-003 | Normalized snapshot loading | Done | Strict schema v0 JSON load, validation, limits, round trip; 25 tests. |
| P0-004 | Selection foundation | Done | Caret/range invariants + position mapping; 31 tests. |
| P0-005 | Runtime event foundation | Done | Ordered transaction/selection events, safe subscription lifecycle; 36 tests. |
| P0-006 | DOCX package reader | Done | Bounded ZIP admission, metadata, part reads, fixtures; 44 tests. |
| P0-007 | Benchmark and baseline harness | Done | Reproducible timing, reports, regression thresholds; 50 tests. |
| P0-008 | Phase 0 corpus and evidence closure | Done | 7 fixtures, fuzz infra, linked exit evidence; `31-PHASE-0-EXIT-REPORT.md`. |

## Maintenance

| ID | Title | Owner | Status | Notes |
| --- | --- | --- | --- | --- |
| M-001 | Decompose god-files into modules | Claude Code | Done | Every crate root reduced to a ≤64-line wiring file (`model` v0/v1, `ooxml`, `sdk`, `import`); public APIs preserved via re-exports; gates green. |
| M-002 | LibreOffice differential fidelity harness | Claude Code | In review | `tools/opendoc-fidelity`: compares importer text vs `soffice --convert-to txt` by word-multiset match %; corpus 6/7 at 100%, footnotes now closed. Round-trip proxy until the Phase-2 writer; not a CI dep. |
| M-003 | Fix casual-doc-import review findings (13 confirmed) | Claude Code | In review | 12/13 fixed (basedOn cycle, styles-part reporting, char-level rPr spacing, sz bound, style-ref kind, nested depth counters, out-of-body reporting, CDATA, report cap); remaining minor deferred (degraded-attribute detail needs a Degraded disposition). Landed with the importer decomposition. |

## Active Work

Design references point at `38-SCHEMA-V1-DESIGN-REFERENCE.md` (anchor).

| ID | Title | Owner | Status | Notes |
| --- | --- | --- | --- | --- |
| P1A-001 | Semantic DOCX import design | Codex | Accepted | Architecture-level import design accepted via ADR-027 (`38-…#import-architecture`); importer gated on schema v1. |
| P1A-002 | DOCX engine competitor source study | Codex | Done | Source-architecture study of DOCX import/edit/preservation/export; `33-DOCX-ENGINE-COMPETITOR-RESEARCH.md`, extended by `37-PHASE-1A-DECISION-RESEARCH.md`. |
| P1A-003 | OOXML fidelity architecture | Codex | Accepted | Dual-representation fidelity + save-planning contract; `34-OOXML-FIDELITY-ARCHITECTURE.md`; ADR-027 accepted 2026-07-24 (`36-ADR-027-ACCEPTANCE-RECORD.md`). |
| P1A-004 | Phase 1A design reconciliation | Claude Code | Designing | Design-readiness + adversarial verify; added `35-DISPOSITION-TAXONOMY.md`, `36-ADR-027-ACCEPTANCE-RECORD.md` (D1–D11, R1–R4), `37-PHASE-1A-DECISION-RESEARCH.md`; resolved R1/R2/R4, D4/D5/D8/D9. Pending owner sign-off on remaining Proposed decisions; open follow-ups R3 seed + R1/R2/R4 code. |
| P1A-005 | Read path: relationship-based main-doc discovery (R1) | Claude Code | In review | Admitter requires only `[Content_Types].xml` + `_rels/.rels`; bounded parse, `officeDocument` discovery, fail-closed typed errors, `main_document_part()`; 5 tests, gates green. |
| P1A-006 | Deterministic import ID/namespace seed (R3) | — | Not started | Input-derived, order-independent seed; reordered-ZIP + native/WASM golden. |
| P1A-007 | Content-types + main-document relationship graph | Claude Code | In review | Retain parsed `[Content_Types].xml`; resolve main-doc part-level relationships with base-relative resolution + root-escape rejection; external never fetched; 2 tests. |
| P1A-008 | Normalized schema v1 design + implementation | Claude Code | In review | `38-…#base-schema-v1` accepted 2026-07-24; additive `pub mod v1` (v0 untouched); types + strict serde + validation + total v0→v1 migration; 12 tests incl. byte-exact golden. |
| P1A-009 | Source package snapshot (Tier-1 provenance) | Claude Code | In review | Deterministic `SourcePackageSnapshot` (ordered part manifest + content types/sizes, main doc, relationship graph); no decompressed text; 1 test. |
| P1A-010 | Semantic WordprocessingML body import | Claude Code | In review | New `casual-doc-import` (ADR-011): bounded `quick-xml` parse → deterministic `v1::Document` (paragraphs/runs/text/tabs/breaks + direct run props, adjacent-run merge, R4 flatten); dual-axis compat report; 8 tests. |
| P1A-011 | Styles import + paragraph direct formatting | Claude Code | In review | Map `w:pPr` direct formatting + parse styles part into `Definitions.styles` with `basedOn` inheritance (dangling/kind-mismatch reported); 15 tests. |
| P1A-012 | Retention mode (round-trip tier-1 byte floor) | Claude Code | In review | `ImportMode::{Semantic,Retention}`; Retention keeps original main-doc bytes verbatim (`RetainedSource`, D5 tier-1), bounded + fail-closed; 2 tests. |
| P1A-013 | Package-level retention | Claude Code | In review | `import_package` in Retention retains every admitted part verbatim (full package byte floor); 1 test. |
| P1A-014 | Numbering import (numbering.xml + w:numPr) | Claude Code | In review | Parse numbering part into v1 abstract/instance definitions; resolve `w:numPr` (dangling reported); 2 tests. |
| P1A-015 | Body section geometry import (w:sectPr) | Claude Code | In review | Body-level `w:sectPr` → `SectionBoundary` (page size/margins/columns), defaulted + domain-clamped; per-paragraph breaks deferred; 1 test. |
| P1A-016 | Media reference import (image relationships) | Claude Code | In review | Main-doc `/image` relationships → `Definitions.media` (`MediaReference`; no bytes decoded), deterministic `MediaId`; 1 test. |
| P1A-017 | No-edit DOCX writer (round-trip reconstruction) | Claude Code | In review | New `casual-doc-export`: `write_package(&RetainedSource)` reconstructs a valid DOCX byte-identically; makes round-trip end-to-end verifiable; 3 tests. Semantic writer is Phase 2. |
| P1A-018 | Round-trip corpus expansion (real-producer families) | Claude Code | In review | 5 real-producer families (tables/lists, nested tables+image, ext/int hyperlinks, header/footer, footnotes), each a LibreOffice-valid `.docx` with an export round-trip test; 7 tests, checksums enforced. |
| P1A-019 | Schema v1 semantic extension design (model everything) | Claude Code | Designing | Umbrella: multi-agent blueprint to model the remaining WordprocessingML constructs as first-class additive v1 values while Retention round-trips unchanged; drives the per-construct slices below (`38-…`). |
| P1A-021 | Model inline drawings + hyperlinks | Claude Code | In review | First additive slices: `InlineNode::Drawing`/`Hyperlink` (external/internal target); import media index + `push_segment` router; 18 tests (`38-…#base-schema-v1`). |
| P1A-022 | Model semantic tables (structure + cell-merge geometry) | Claude Code | In review | Additive `BlockNode::Table` (grid/rows/recursive cells, `gridSpan`/`vMerge`, `MAX_TABLE_DEPTH=32`); `tables.rs` `TableStack`; ~24 tests; review-fixed over-depth corruption via `suppressed_tbl_depth` (`38-…#tables`). |
| P1A-023 | Model semantic fields (instruction + cached result) | Claude Code | In review | Additive `InlineNode::Field{instruction, inlines}`; `in_hyperlink`→`in_wrapper` leaf-only rule; simple + complex `fldChar` machine; 15 tests; 2 review defects fixed (`38-…#fields`). |
| P1A-024 | Model text boxes + AlternateContent selection | Claude Code | In review | Additive `InlineNode::TextBox{blocks}` via suspended `ContentFrame` (fixes enclosing-paragraph truncation + silent image drop); one `mc:Choice` branch selected; 13 tests (`38-…#text-boxes`). |
| P1A-025 | Importer no-skip audit | Claude Code | In review | Multi-agent audit found 16 silently-skipped/mis-captured constructs (text boxes, `mc:AlternateContent`, extra parts, ruby); drives P1A-024 and the extra-part/VML/ruby slices. |
| P1A-026 | Model footnotes/endnotes (extra-part bodies) | Claude Code | In review | Additive `Definitions.footnotes`/`endnotes` + `InlineNode::NoteReference`; note-container parse resolving own relationships; closes the last LibreOffice-visible text gap; 12 tests; 3 review fixes (`38-…#footnotes-and-endnotes`). |
| P1A-027 | Model headers/footers (extra-part bodies + section refs) | Claude Code | In review | Additive `Definitions.headers`/`footers` + `SectionBoundary` refs; single-container parse, `w:sectPr` reference resolution; 11 tests; 1 review fix (`38-…#headers-and-footers`). |
| P1A-028 | Model legacy VML pictures (`w:pict`) | Claude Code | In review | No model change — `w:pict`→`v:imagedata@r:id` reuses `InlineNode::Drawing` via the shared media index; 2 tests; 1 review fix (`pict_depth` reset) (`38-…#vml-pictures`). |
| P1A-029 | Model media/hyperlinks inside notes/headers/footers | Claude Code | In review | No model change — per-part media/hyperlink indices via `part_relationships`, one `MediaId` per relationship (main-doc byte-identical); 3 tests; 1 review fix (`38-…#extra-part-media`). |
| P1A-030 | Fix ruby phonetic guides (`w:ruby`) | Claude Code | In review | Import-only: `ruby_annotation_depth` suppresses `w:rt` text so the base reads in order (annotation reported); 2 tests; 1 review fix (bool→depth counter) (`38-…#ruby`). |
| P1A-031 | Model comments (extra-part bodies + reference + metadata) | Claude Code | In review | Additive `Definitions.comments` (blocks + author/initials/date) + `InlineNode::CommentReference`; reuses note-container machine; 10 tests; review-fixed EOF-truncated open-table strand via `TableStack::flush_open` (`38-…#comments`). |
| P1A-032 | Model tracked changes / revisions (`w:ins`,`w:del`,`w:delText`) | Claude Code | In review | Additive `InlineNode::Revision` wrapper (kind + author/date/id), `w:delText` verbatim; unified innermost-wins `wrapper_order` stack; 24 tests; review-fixed close-side desync via `suppressed_revision_depth` (`38-…#tracked-changes`). |
| P1A-033b | Model run metrics + language (`w:spacing`/`kern`/`position`/`lang`) | Claude Code | Done | Additive `RunProperties`: character_spacing_twips/kerning_half_points/position_half_points (bounded, out-of-range reported) + `Language{value,eastAsia,bidi}`; closes the run-property tail (`38-…#run-properties`). |
| P1A-033 | Model run-property long tail (toggles + fonts + vocabularies) | Claude Code | In review | Additive `RunProperties`: caps/smallCaps/hidden/webHidden/doubleStrike toggles, `rFonts` four slots, vertAlign/highlight/emphasis enums; default serializes `{}`; 7 tests. Deferred: metrics + `w:lang` (`38-…#run-properties`). |
| P1A-034b | Model paragraph borders + shading + tabs (wave 2) | Claude Code | Done | `ParagraphBorders` (reusing `BorderEdge`) via a `ParagraphBorders` edge scope, paragraph `w:shd` (a `mark_rpr_depth` counter distinguishes the pilcrow's `w:rPr` shd), and `w:tabs`→`TabStop{pos,alignment,leader}` (clear/unknown reported); closes the paragraph-property tail (`38-…#paragraph-properties`). |
| P1A-034 | Model paragraph-property long tail (flags + outline; wave 1) | Claude Code | In review | Wave 1 shipped keep/break/widow/contextualSpacing/suppressLineNumbers toggles + `outlineLvl`, via the shared `apply_paragraph_property` (body + styles); 5 tests. Waves 2+ (shading/borders/tabs) designed, pending impl (`38-…#paragraph-properties`). |
| P1A-035 | Model table-property long tail (attribute-based; wave 1) | Claude Code | In review | Wave 1 shipped attribute-based `TableProperties`/`TableRowProperties` + extended `TableCellProperties` (shading/vAlign/noWrap/textDirection); 5 tests. Wave 2 `P1A-035b` (borders + margins) designed, pending impl (`38-…#table-properties`). |
| P1A-036 | Model bookmarks (`w:bookmarkStart/End`) | Claude Code | Done | `Bookmark{name}` definition + paired `InlineNode::BookmarkStart`/`BookmarkEnd`; marker→def strict (`DanglingBookmarkRef`), anchor resolution lax; accumulated across all parts; review fixes folded; gates green (`38-…#bookmarks`). |
| P1A-037 | Model content controls (`w:sdt`) | Claude Code | Done | Block + inline `Sdt` wrappers carrying sdtPr (alias/tag/id/kind); generalized frames (`push_frame`/`enter_sdt_block`), table-budget restart, no-panic, docPart distinction reported; agent-built, merged (PR #29), adversarial review fix-forward pending (`38-…#content-controls`). |
| P1A-035b | Model table borders + margins (wave 2) | Claude Code | Done | `BorderEdge`/`TableBorders`/`CellMargins` on table+cell; edge-name collision (`top`/`start`/…) resolved by an `EdgeScope` scope + tblpr/tcpr level; `edge_scope` text-box-leak fix folded (PR #27) (`38-…#table-properties`). |
| P1A-0FX | Adversarial-review fix-forwards | Claude Code | Done | Per-slice reviews caught + fixed: rFonts theme fallthrough, revision desync, `*PrChange`/themeFill overwrite, edge_scope text-box leak, bookmark phantom-end. Each regression-tested. |
| P1A-0AA | Model-coverage audit + parallel design (multi-agent) | Claude Code | Done | Data-driven ranking of every reported-not-modeled construct + adversarially-reviewed designs for the run/paragraph/table property tails, bookmarks, and content controls; confirmed no silent loss anywhere; drove P1A-033..037. |

## Completed Work

| ID | Title | Completed | Notes |
| --- | --- | --- | --- |
| F-001 | Repository bootstrap docs | 2026-07-24 | Root/license/agent/process/CI docs, tracker, competitive analysis. |
| F-002-F-010 | Foundation design batch | 2026-07-24 | Glossary, support, CI, workspace, errors, limits, schema v0, fixtures, ADRs, competitive pass 1. |
| P0-001 | Deterministic model transaction slice | 2026-07-24 | Three-crate workspace, atomic grapheme insertion, snapshots/errors, 10 tests, CI/security. |
| P0-002 | Transaction semantics and history | 2026-07-24 | Delete/split/join, mapping, inverses, SDK undo/redo, atomicity coverage. |
| P0-003 | Normalized snapshot loading | 2026-07-24 | Strict bounded JSON v0 load/export, semantic limits, duplicate/unknown rejection. |
| P0-004 | Selection foundation | 2026-07-24 | Canonical directed selection, strict validation, atomic edit/history mapping. |
| P0-005 | Runtime event foundation | 2026-07-24 | Bounded future-only subscriptions, stable ordering, explicit lag gaps, atomic journal. |
| P0-006 | DOCX package reader | 2026-07-24 | Bounded ZIP preflight, path/codec policy, verified part reads, 5 fixtures, checksum CI. |
| P0-007 | Benchmark and baseline harness | 2026-07-24 | Four deterministic workloads, typed reports, regression comparison, M4 baseline, CI smoke. |
| P0-008 | Phase 0 corpus and evidence closure | 2026-07-24 | Two package fixtures, exact corpus policy, package-reader fuzzing, accepted exit report. |
| F-011 | Phase 1 capability decomposition | 2026-07-24 | Split import/typography/pagination/rendering into independently gated stages. |
| F-012 | Apache-2.0 license policy | 2026-07-24 | Adopted Apache-2.0 before external contributions. |

## Open Questions

- Should the crate family retain the `casual-doc-*` names if public package-name
  availability later requires a change?
- Which fixed font set should be used for deterministic layout baselines?

## Phase 1B — Semantic DOCX writer (model -> WordprocessingML)

| ID | Item | Owner | Status | Notes |
|---|---|---|---|---|
| P1B-001 | Semantic DOCX writer design | Claude Code | Designed | `39-PHASE-1B-SEMANTIC-DOCX-WRITER-DESIGN.md`: serialize the v1 Document to a valid editable DOCX (the dual of Retention export); semantic-fixed-point round-trip (import->write->reopen = equal model); media supplied via a hybrid `part_name->bytes` map. Slices P1B-002..006. |
| P1B-002 | Writer core body | Claude Code | Done | `casual-doc-export::write_document`: `[Content_Types].xml`/`_rels`/`document.xml`; body paragraphs, runs, mapped run+paragraph properties, tabs/breaks, section `sectPr`; deterministic ZIP (Stored, fixed DateTime). Fixed point proven (`core_body_survives_the_semantic_round_trip`, `writer_is_deterministic`). |
| P1B-003 | Writer tables | Claude Code | Done | `write_table`/tblGrid/tblPr/trPr/tcPr, borders/shading/margins, gridSpan/vMerge, nested tables. Fixed point proven (`tables_survive_the_semantic_round_trip`). PR #36. Adversarial review: all table/row/cell/grid fields round-trip; 2 findings recorded below. |
| P1B-004 | Writer inline constructs | Claude Code | Done | Threaded a write `Ctx` (bookmark-name source + rel accumulator) through the writer: hyperlinks (external via generated `document.xml.rels` + `xmlns:r`, internal via `w:anchor`, tooltips), fields (`w:fldSimple`), bookmark ranges (`w:id` from the shared `BookmarkId`, name from `Definitions`), revisions (`w:ins`/`w:del`, deletions emit `w:delText`), inline content controls (`w:sdt`/`w:sdtPr`/`w:sdtContent`). Media constructs (drawings/text-boxes) + note/comment refs deferred to P1B-005. Proven by `inline_constructs_survive_the_semantic_round_trip` + `external_hyperlink_survives_the_semantic_round_trip`. Closes P1B-R2. |
| P1B-005 | Writer definition parts | Claude Code | Planned | styles.xml, numbering.xml, footnotes/endnotes, headers/footers + section refs, comments.xml; drawings + media hybrid path; note/comment references. |
| P1B-006 | Writer fidelity + corpus | Claude Code | Planned | fidelity harness over the round-trip corpus, LibreOffice differential validity check, CI wiring. |
| P1B-R1 | Table alignment `Justify` writer edge | Claude Code | Open (design) | Writer emits `w:jc w:val="both"` for `Alignment::Justify`; table importer maps `both->None` (deliberate), so an *authored* model with table alignment `Justify` breaks the fixed point. Not import-reachable (the table importer never produces `Justify`), and `both` is not a valid `ST_JcTable` value. Resolution options: (a) model-level validation forbids table `Justify`; (b) symmetric import capture (`both`->`Justify`) matching paragraphs. Decide with owner before changing import semantics. |
| P1B-R2 | Inline constructs dropped in `write_inline` | Claude Code | Fixed by P1B-004 | `write_inline` catch-all silently dropped hyperlink/field/drawing/revision/bookmark/sdt/note/comment inline nodes (acknowledged later-slice gap; not import-reachable in the current core+tables corpus). Addressed by P1B-004 (self-contained inline constructs) + P1B-005 (media/definition-part-dependent ones). |
| P1B-FONT | Font management design | Claude Code | Designing (owner review) | `40-FONT-MANAGEMENT-DESIGN.md` drafted from multi-tool prior-art research (Word/OOXML, LibreOffice, ONLYOFFICE, CSS Fonts 4) + Rust-ecosystem eval + OpenDoc-current audit. Recommends: model + round-trip all OOXML font data first (fix the 2-value theme collapse -> full 8-value `ThemeFontRef` + `RunFonts` + `hint`; typed `fontTable.xml`; theme `fontScheme`; embedded fonts with in-core `.odttf` de-obfuscation preserving `fontKey`; settings flags) then Phase-1B resolution as a pure `fn(request,&FaceIndex)` mirroring CSS Fonts 4 §5.2 with a 3-tier FaceIndex; stack `ttf-parser` now, `fontdb`/`fontique`/`skrifa` later, `font-kit` native-only. 8 open questions for owner sign-off before implementing. |
