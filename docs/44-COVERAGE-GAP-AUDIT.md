# 44 — Coverage Gap Audit (2026-07-26)

## Purpose

A definitive, evidence-backed inventory of **what OpenDoc does not yet handle** — every
WordprocessingML construct family, package part, and layout/render capability that is
missing, partial, or lossy — so gaps are worked down deliberately instead of discovered
one at a time. Produced by three parallel read-only audits (WML content, package parts,
layout/rendering) cross-checked against ECMA-376 Part 1 and the code at `main` (through
PR #88). Every row cites `file:line` evidence.

This document is the **source of the P1F-* tracker rows**; keep the two in sync.

> [!IMPORTANT]
> ## Status: reconciled 2026-08-01 — this is now a historical evidence register
>
> The `file:line` evidence below was captured at `main` through PR #88 and is **not**
> re-verified against later code. Since then almost every Tier 0–3 row has been
> implemented (see the per-row **Status** column added to each table).
>
> **The current authoritative sources are:**
> - **`docs/14-EXECUTION-TRACKER.md`** — live per-item status (the `P1F-*` rows).
> - **`docs/55-CURRENT-DOCX-FIDELITY-GAP-AUDIT.md`** — current rendered/preserved/degraded
>   fidelity register (audit 2026-07-27, baseline `main@cde11ff`).
> - **`docs/60-FIDELITY-CORPUS-RENDERING-AUDIT.md`** — verified real-document rendering &
>   pagination residuals (audit 2026-07-27).
>
> Use this file for the **original gap analysis and evidence**, not for current status.
> The Status column records what closed each row (the `P1F-*` tracker item), what is still
> open, and what needs re-verification. When in doubt, `docs/14`/`55`/`60` win.

## Three structural findings (read these first)

Most individual gaps are instances of one of three systemic issues. Fixing the systemic
issue is higher-leverage than closing rows one by one.

### F1 — Model-rich, layout-poor (rendering)
The model faithfully captures ~35 run/paragraph properties, but the layout/flow layer reads
only ~8 of them and the renderer draws only three primitives (filled rect, stroked rect,
filled glyph outline). Everything else is parsed into the model and then **silently dropped
before paint**. Evidence: `flow.rs:723-734` (`collect_runs`), `flow.rs:739-766`
(`styled_run`), `flow.rs:858-869` (`box_metrics`), `render/src/lib.rs:70-117`. Consequence:
these gaps are **low-risk to close** — the data is already present and validated; a few lines
wire each property into geometry or a paint item.
**Status (2026-08-01): largely closed.** The Tier-2 rendering rows (P1F-11..18, 23, 24, 25)
are Done; the renderer now paints images, decorations, shading/highlight, headers/footers,
super/sub + caps, theme color, and per-script/symbol coverage. Remaining F1 residuals:
line-spacing `atLeast`/`exact` (P1F-20, needs verification) and footnote/endnote bottom-band
placement (P1F-22, in progress — the `P1F-NOTES-*` cluster).

### F2 — Silent data loss on the semantic edit→save path (fidelity)
OpenDoc has two writers. **Retention** (`import lib.rs:227-242` + `export lib.rs:49-68`) keeps
every admitted part byte-for-byte, so a **no-edit** round-trip is byte-perfect. **Semantic**
(`export/semantic.rs`) regenerates OOXML *from the model only* — it builds a fresh
`[Content_Types].xml` (`semantic.rs:447-502`) and a fresh root `_rels` pointing only at
`word/document.xml` (`semantic.rs:504-518`). The importer resolves only 13 relationship
types off the main document (`import/lib.rs:89-198`); every other part is **never opened**, so
it is dropped on the first semantic edit **with no compatibility-report entry**. This
violates the AGENTS.md "no silent data loss" contract for the edit path.
**Highest-leverage fixes:** (a) a *package-manifest disposition pass* that reports every
admitted part the semantic model does not consume; (b) an *opaque side-table* that carries
unmodeled admitted parts (with their content-types + rels) through the semantic writer,
converting a broad class of silent losses into edit-survivable pass-through.
**Status (2026-08-01): closed at the systemic level.** The disposition pass (P1F-1) and
opaque side-table (P1F-2) are Done, so every unconsumed admitted part is now reported and the
common parts pass through edit. The bulk of individual part/construct families (P1F-3..6,
8..10, 26..37, 39) are now modeled + round-tripped. Remaining F2 residuals: docProps
semantic-writer emission (P1F-7, in progress) and the Tier-4 long tail (opaque-preserved via
P1F-2 but not semantically modeled).

### F3 — Two active correctness bugs (not just missing features)
1. **Math (OMML) is mangled, not dropped.** The importer doesn't namespace-check, so `m:r`/
   `m:t` are matched by the `w:r`/`w:t` arms (`import/body.rs:863,881`) and equations flatten
   into malformed plain text interleaved into the paragraph. Both structure and text order are
   lost. Fix: guard the `m:` namespace *before* the run/text arms; model an opaque math node.
2. **Hidden text renders.** `w:vanish` is modeled (`properties.rs:757`) but ignored in
   `collect_runs`/`styled_run`, so hidden runs (index entries, hidden notes) are painted onto
   the visible page. Fix: skip vanished runs in `collect_runs`.
   (Related paint bug: `exact` row-height clipping is emitted by the paginator but
   `PushClip`/`PopClip` are no-ops in the renderer — `render/lib.rs:112-114` — so `w:trHeight`
   overflow is not actually clipped.)
**Status (2026-08-01): closed.** Both bugs fixed — OMML namespace guard + opaque `Math` node
(P1F-C1), `w:vanish` suppression (P1F-C2) — and the clip primitive is implemented (P1F-C3).

## Severity tiers & work order

Ranked so the list doubles as the implementation order. "In flight" marks work already
open as a PR at the time of writing. The **Status** column reflects the current tracker
(`docs/14`) as of 2026-08-01; the ID/Gap/Evidence/Fix columns are the original audit.

### Tier 0 — Correctness bugs (fix first; these are wrong, not just absent)
| ID | Gap | Evidence | Fix | Status (2026-08-01) |
| --- | --- | --- | --- | --- |
| P1F-C1 | OMML math flattens into garbled text | `import/body.rs:863,881` | Namespace-guard `m:`; opaque `Math` inline node retaining the OMML subtree + text | **Done** (P1F-C1) |
| P1F-C2 | Hidden `w:vanish` text is rendered | `properties.rs:757`, `flow.rs:739-766` | Skip vanished runs in `collect_runs` | **Done** (P1F-C2) |
| P1F-C3 | `exact` row clip ignored at paint | `render/lib.rs:112-114`, `compose.rs:74-77` | Implement `PushClip`/`PopClip` in the renderer | **Done** (P1F-C3; tiny-skia clip stack) |

### Tier 1 — Silent semantic-path data loss (fidelity; violates "no silent loss")
| ID | Gap | Evidence | Fix | Status (2026-08-01) |
| --- | --- | --- | --- | --- |
| P1F-1 | **Package-manifest disposition pass** — report every admitted part the semantic model drops | `import/lib.rs:227-242`, `semantic.rs:504-518` | Enumerate `package.entries()`; emit a compat entry (Preserved-in-Retention / Dropped-in-Semantic) for each unconsumed part | **Done** (P1F-1) |
| P1F-2 | **Opaque part side-table** — carry unmodeled parts through the semantic writer | `semantic.rs:447-518` | Preserve glossary, embeddings, charts, webSettings, thumbnail, stylesWithEffects, signatures, customXml verbatim; merge their content-types + rels | **Done** (P1F-2; signatures deliberately dropped+reported) |
| P1F-3 | Theme `clrScheme` (12 theme colors) + `fmtScheme` dropped → doc colors collapse on edit | `theme.rs:1-7`, `semantic.rs:855-864` | Model the 12 color slots; preserve `fmtScheme` opaque | **Done** (P1F-3) |
| P1F-4 | `styles.xml` `w:docDefaults` silently dropped → whole-doc base font/size shifts | field exists `definitions.rs:396-397`, never set `import/lib.rs:631-647`, `styles.rs:268` `_=>{}` | Parse `rPrDefault`/`pPrDefault` into the existing field; emit in writer | **Done** (P1F-4) |
| P1F-5 | `settings.xml` — only 3 of ~40 settings; parser has no reporter (fully silent) | `settings.rs:19,56-61`, `definitions.rs:335-346` | Model `evenAndOddHeaders`, `defaultTabStop`, `trackChanges`, `documentProtection`, `proofState`, core `w:compat`, `mathPr`; attach a `Reporter` | **Done** (P1F-5; expanded settings + reporter) |
| P1F-6 | Numbering — only `ilvl`+`start`; `numFmt`/`lvlText`/`lvlJc`/`suff` dropped → lists lose glyphs | `numbering.rs:238-259`, writer `semantic.rs:1010-1018` | Extend `NumberingLevel` with format/text/justify/suffix + per-level pPr/rPr | **Done** (P1F-6; bullet render P1F-6b/31b) |
| P1F-7 | docProps core/app/custom (title/author/dates/company/counts) | no refs; `import/lib.rs:89-96` | **In flight (#28).** Ensure semantic writer emits; cover dcterms `xsi:type`, `cp:revision`, app `HeadingPairs`/`TitlesOfParts` | **In progress** (P1F-7; read-only metadata bridge landed in Phase 1G, semantic-writer emission still open) |
| P1F-8 | customXml store + content-control `dataBinding` (enterprise templates break) | `ooxml/tests.rs:346`, SDT model `body.rs:279-298` | Preserve customXml parts (side-table) + model `dataBinding`/list entries on `SdtProperties` | **Done** (P1F-8; also closes P1F-33 SDT data) |
| P1F-9 | Styles per-style depth — `link`/`next`/`uiPriority`/`qFormat`/`semiHidden`; **table styles** (`tblStylePr`) dropped | `styles.rs:235-283`, `properties.rs:346-352` | Add `Table`/`Numbering` style kinds + conditional formatting; model style metadata | **Done** (P1F-9; `w:latentStyles` remains Tier-4) |
| P1F-10 | Comment companion parts — `paraId`/threading, resolved-state, `people.xml` identity | zero refs; `import/lib.rs:96` | Model `paraId`+parent on comment; parse commentsExtended/People | **Done** (P1F-10) |

### Tier 2 — High-visibility rendering gaps (F1: data present, unconsumed — low-risk)
| ID | Gap | Evidence | Fix | Status (2026-08-01) |
| --- | --- | --- | --- | --- |
| P1F-11 | Inline images/drawings never rasterized | `render/lib.rs:112-114`, `flow.rs:731`, `display.rs:81` | Emit `PaintItem::Image`; decode + blit in renderer | **Done** (P1F-11; EMF/WMF placeholder follow-up landed) |
| P1F-12 | Paragraph indents (left/right + first-line/hanging) computed but unused | `flow.rs:229,858-869`, `compose.rs:50-56` | Apply to `origin.x` + shaper `max_width`; first-line offset | **Done** (P1F-12) |
| P1F-13 | Tab stops (custom/default/leaders/alignment) ignored; `\t` collapses | `flow.rs:727`, `properties.rs:588` | Resolve tab stops post-shaping; advance to next stop; leaders | **Done** (P1F-13; RTL tab layout is a follow-up) |
| P1F-14 | Hard breaks `w:br` (line/page/column) + `w:cr` ignored → text merges | `flow.rs:731`, `text.rs:69` | Split runs at breaks; map page/column kinds to paginator | **Done** (P1F-14) |
| P1F-15 | Underline/strikethrough not drawn (glyphs only) | `render/lib.rs:120-161`, `text.rs:59` | Draw decoration lines from run metrics (data already on run) | **Done** (P1F-15; boolean decoration — typed underline style/color is P1F-38) |
| P1F-16 | Highlight + paragraph/run/cell shading + borders not painted | `flow.rs:739-766`, `compose.rs:48-84`, `properties.rs:580-585,769,807-812` | One shared "fill/stroke rect behind box" mechanism | **Done** (P1F-16; run `w:shd`/`w:bdr` deferred) |
| P1F-17 | Headers/footers (incl. first-page/even-odd) + PAGE/NUMPAGES fields | `page.rs:61-82`, `paginate.rs:71`, `flow.rs:731` | Header/footer bands on `PageConfig`/`Page`; compute PAGE at paginate | **Done** (P1F-17; multi-section running content via P1F-RUNNING-SECTIONS) |
| P1F-18 | Super/subscript + all-caps/small-caps not applied | `flow.rs:739-766`, `properties.rs:751,754,764,779` | Baseline shift + size scale / case-transform in `styled_run` | **Done** (P1F-18) |
| P1F-19 | Multi-section pagination + section-break types (next/even/odd/continuous) | `paginate.rs:71`, `properties.rs:594` | Drive paginator per-section; honor break types | **Done** (P1F-PAGINATION + P1F-COLUMNS + P1F-RUNNING-SECTIONS; even/odd section-start blank-page insertion: needs verification, see docs/60) |
| P1F-20 | Line-spacing `atLeast`/`exact` (model has only `line_percent`) | `properties.rs:514-524`, `shape.rs:152-156` | Extend model with line rule+value; map in shaper | **Needs verification** — no dedicated P1F row closed this at the layout layer; line-rule fidelity is being handled in editor row P1G-027 (In review). Confirm the `atLeast`/`exact` shaper mapping before closing. |
| P1F-21 | Columns (multi-column, separators, unequal) | `definitions.rs:110-118`, no flow logic | Column-aware content-area subdivision + balancing | **Done** (P1F-COLUMNS + P1F-COLUMNS-2) |
| P1F-22 | Footnotes/endnotes placement (bottom-of-page band) | `paginate.rs:963`, `flow.rs:731` | Reserve bottom band; flow note bodies | **In progress** — active `P1F-NOTES-*` cluster (P1F-NOTES-PAGINATION Designing; NOTES-BANDS/SPLITS/METADATA/MULTISECTION/CONTINUOUS/MULTICOLUMN In review). Design in `docs/62`. Not yet Done. |
| P1F-23 | Theme/`auto` run color → everything non-RGB renders black | `flow.rs:745-748` | Resolve theme/auto colors against theme + context | **Done** (P1F-23; wired to the real `clrScheme` palette from P1F-3) |
| P1F-24 | Per-script font slots (`cs`/`eastAsia`/`hAnsi`) — only ascii slot used | `flow.rs:797-802`, `properties.rs:738-744` | Per-codepoint slot selection | **Done** (P1F-24; system-font fallback registry) |
| P1F-25 | Cell vertical align + cell margins (`w:vAlign`/`w:tcMar`); vertical merge (`w:vMerge`) render | `flow.rs`, `compose.rs`, model `table.rs:24-31` | Implemented: resolved insets/vAlign plus exact-grid vertical-merge content ownership, merged height, paint suppression, and page-local grouping (`P1F-TBL-VMERGE`; docs 46/49). | **Done** (P1F-25 margins/vAlign + P1F-TBL-VMERGE merge render) |

### Tier 3 — Content families dropped on the semantic path (reported, not silent)
| ID | Gap | Evidence | Fix | Status (2026-08-01) |
| --- | --- | --- | --- | --- |
| P1F-26 | Charts + SmartArt vanish (only picture blips modeled) | `import/body.rs:1843-1846` | Object-reference node keyed to the chart/diagram part | **Done** (P1F-26; `EmbeddedObject` node, part un-orphaned) |
| P1F-27 | OLE/embedded objects (`w:object`) dropped | `import/body.rs:1602` | `EmbeddedObject` node (progId, part refs, preview media) | **Done** (P1F-27) |
| P1F-28 | Anchored/floating drawings degrade to inline (position/wrap/rotation/alt-text lost) | `import/body.rs:1070`, `:2912` | `DrawingAnchor` (position, wrap, extent, `descr` alt-text) | **Done** (P1F-28; position first cut — text wrap-around is a follow-up) |
| P1F-29 | Floating tables `w:tblpPr` snap to inline | model `table.rs:184-227` | `TableFloatPosition` on `TableProperties` | **Done** (P1F-29; model/import/export — float layout is a follow-up) |
| P1F-30 | Table style ref (`w:tblStyle`) + conditional formatting (`w:cnfStyle`) | `tests.rs:1275` | `style_ref` + `cnf` bits (pairs with P1F-9) | **Done** (P1F-30 model/import/export; cell-shading cascade at layout via P1F-TBL-CNF) |
| P1F-31 | Symbols `w:sym` vanish | catch-all `import/body.rs:1602` | `Symbol` inline node (font + code point) | **Done** (P1F-31 model + P1F-31b rendering via Unicode mapping) |
| P1F-32 | Legacy form fields `w:ffData` (name/type/default/maxLength/entries) | `import/body.rs:983-994` | `FormFieldData` on `Field` | **Done** (P1F-32) |
| P1F-33 | SDT data (binding, dropdown/combo entries, date/checkbox format) | `import/body.rs:1571-1575` | Extend `SdtProperties` (pairs with P1F-8) | **Done** — closed by **P1F-8** |
| P1F-34 | Tracked-change moves (`moveFrom`/`moveTo`) + `*PrChange` history | `import/body.rs:787-796` | `Revision` move kinds + `PropChange` prior props | **Done** (P1F-34 moves + range markers; P1F-34b `*PrChange` history) |
| P1F-35 | Comment range markers (`commentRangeStart`/`End`) → anchor becomes a point | `import/body.rs:925-932` | Model range markers like bookmarks | **Done** (P1F-35) |
| P1F-36 | Section long tail: `pgBorders`, `lnNumType`, per-section note props, `textDirection`/`bidi`, per-column widths | `import/body.rs:1094-1233` | Extend `SectionBoundary` | **Done** (P1F-36) |
| P1F-37 | Complex-script run props `bCs`/`iCs`/`szCs` | `properties.rs:21-136` | Add CS toggles + `size_cs` to `RunProperties` | **Done** (P1F-37) |
| P1F-38 | Underline style/color flattens to bool without report | `properties.rs:24` | Typed underline style + color | **Done** — closed typed model, DOCX import/export, cascade/layout, independent color, single/double/thick/dotted/dashed/dot-dash/wavy paint, command-routed editor authoring, mixed-state reflection, armed typing, clipboard preservation, exact undo, and reopen tests. `words` remains a disclosed paint-only limitation: it is preserved but the renderer does not yet suppress the line under spaces. Suggesting-mode authoring rejects explicitly until tracked-format deltas carry style/color. |
| P1F-39 | `w:altChunk`, NB/soft hyphen, `w:ptab` | catch-all | Model chunk ref + hyphen glyphs + abs tab | **Done** (P1F-39 model/import/export; P1F-39b layout placeholder — embedded-chunk *content* flow still open) |

### Tier 4 — Long tail / low priority / policy
`latentStyles`, `stylesWithEffects.xml`, `webSettings.xml` (opaque preserve via P1F-2);
glossary/AutoText **semantic** modeling (preserve opaque first — currently the only truly
silent whole-part drop, `import/lib.rs`); ruby annotation text; ink (`w:contentPart`);
generic non-drop `w:framePr` layout; `w:background`; digital signatures (edit invalidates them anyway —
drop-and-report deliberately); vertical text, distribute alignment, kashida justification.
**Policy decision needed:** `.docm` macro files are rejected at open
(`ooxml/package.rs:196-198`) — a hard capability gap; decide strip-and-open vs.
explicit-unsupported and document it.

**Status (2026-08-01): still open.** These remain the unresolved long tail. Partial coverage
via P1F-2: `stylesWithEffects.xml`, `webSettings.xml`, glossary/AutoText, and digital
signatures are now **preserved opaque** (survive a semantic edit as bytes) or, for signatures,
deliberately dropped-and-reported — but none of the following are **semantically modeled**:
`latentStyles`, glossary/AutoText semantics, ruby annotation, ink (`w:contentPart`),
`w:framePr` layout, `w:background`, vertical text, distribute alignment, and kashida
justification. The `.docm` macro-policy decision is **still undecided** — files are still
rejected at open; strip-and-open vs. explicit-unsupported has not been chosen or documented.

## What IS complete (verified — no gap)

So the register is not read as "nothing works": paragraphs/runs with rich direct properties;
tables (vMerge modeled, gridSpan, margins/borders/shading, row height/header/cantSplit,
layout, width solving, cross-page split); fields (simple + complex, as opaque instructions);
hyperlinks; bookmarks; footnotes/endnotes; comments (content); headers/footers (recursive
blocks); inline images + VML pictures; content-control structure (12 kinds); tracked
ins/del; text boxes; styles + basedOn inheritance; section geometry; font table (incl.
embedded faces) + theme font scheme; document defaults *structure*. And the whole
layout→paginate→render→hit-test spine: shaping, break control, line splitting, widow/orphan,
**bounded incremental re-pagination**, tables across pages, and byte-accurate hit-testing.

> **Reconciliation note (2026-08-01):** since this section was written, the rendered/preserved
> surface has grown substantially (Tier 0–3 above). For the current, verified picture of what
> renders vs. what is preserved-but-degraded vs. what is still invisible, read
> `docs/55-CURRENT-DOCX-FIDELITY-GAP-AUDIT.md` and `docs/60-FIDELITY-CORPUS-RENDERING-AUDIT.md`
> rather than relying on this snapshot.

## Cross-cutting recommendation

Do **F2's package-manifest disposition pass (P1F-1) first** — it is the single change that
brings the semantic path into compliance with "no silent data loss" for *every* Tier-1/Tier-3
part at once (turning silent drops into reported ones), independent of when each part is
actually modeled. Then the opaque side-table (P1F-2) makes the most common of those parts
edit-survivable. Tier-0 correctness bugs are small and should land immediately regardless.

**Update (2026-08-01):** this sequencing was followed — P1F-1 and P1F-2 are both Done, the
Tier-0 bugs (P1F-C1/C2/C3) landed, and the individual part/construct families were then modeled
on top. The remaining work is the residual list above: P1F-7 (docProps writer, in progress),
P1F-20 (line rule, needs verification), P1F-22 (footnote placement, in progress),
and the Tier-4 long tail incl. the `.docm` policy decision.
