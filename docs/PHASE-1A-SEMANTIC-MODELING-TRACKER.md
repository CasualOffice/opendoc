# Phase 1A — Semantic Modeling: Active Tracker

> **Purpose.** A single, forward-looking view of the *current* phase: what is
> done, what is in progress, and **what is next, in order**. This is the
> sequencing/visibility board. The permanent, append-only history lives in
> [`14-EXECUTION-TRACKER.md`](14-EXECUTION-TRACKER.md); this file is deliberately
> lightweight and **is deleted when Phase 1A closes**.
>
> **Phase 1A goal.** Model every WordprocessingML construct as a first-class,
> editable schema-v1 value — no silent data loss, every addition additive
> (existing snapshots + migration golden byte-identical), each slice behind an
> adversarial multi-agent review.
>
> **Legend:** ✅ done & merged · 🔧 in progress · ⏭️ next (ordered) · 💤 queued
> · 🧊 deferred sub-item. Last updated: 2026-07-25.

---

## Done (merged to `main`)

| # | Construct | PR |
|---|-----------|-----|
| ✅ | Comments (definitions + reference + metadata) | #17 |
| ✅ | Tracked changes (`w:ins`/`w:del`/`w:delText`) + wrapper-stack fix | #18/#19 |
| ✅ | Run-property tail: toggles, fonts (`w:rFonts`), vocabularies + rFonts fix | #20/#21 |
| ✅ | Table/row/cell properties — attribute-based (wave 1) | #22 |
| ✅ | Paragraph flags + outline level | #23 |
| ✅ | Model-coverage audit + 5 parallel designs (multi-agent) | — |

_Earlier Phase-1A families (styles, numbering, sections, media, tables, hyperlinks,
fields, drawings, VML, text boxes, footnotes/endnotes, headers/footers, ruby) are
in `14-EXECUTION-TRACKER.md` (P1A-021…P1A-030)._

---

## In progress

| # | Item | Branch / PR | Notes |
|---|------|-------------|-------|
| 🔧 | **Fix: property-change revisions + theme shading** | `fix/property-change-revisions` | Unified `pr_change_depth` guard (`pPr`/`rPr`/`tblPr`/`trPr`/`tcPr`) so `w:*PrChange` historical values never overwrite current ones; `w:themeFill`/`themeColor` reported. Fixes 3 confirmed table-props review bugs. |

---

## Next — in sequence (this phase)

Ordered by the coverage audit (real-world frequency × edit value × effort).
Each is designed + adversarially reviewed; docs 49–53. Implement top-down.

| Seq | Item | Tracker | Design | Status |
|----:|------|---------|--------|--------|
| ⏭️ 1 | Table **borders + margins** (nested edge-child capture, collision-safe) | P1A-035b | `51` (wave 2) | 💤 queued |
| ⏭️ 2 | Paragraph **shading + borders (`pBdr`) + tabs** | P1A-034b | `50` (wave 2) | 💤 queued |
| ⏭️ 3 | **Bookmarks** (`bookmarkStart/End` + internal-anchor validation) | P1A-036 | `52` | 💤 queued (review-fixes listed) |
| ⏭️ 4 | **Content controls** (`w:sdt` block + inline wrapper) | P1A-037 | `53` | 💤 queued (review-fixes listed) |
| ⏭️ 5 | Run-property **metrics** (`w:spacing`/`kern`/`position`) + `w:lang` | P1A-033b | `49` (slices C/E) | 💤 queued |

### Low-tail (fold into the slices above or a final sweep)
- Row `w:jc` / `w:wBefore` / `w:wAfter` / `w:gridBefore` / `w:gridAfter`
- Table `w:tblInd` / `w:tblOverlap` / `w:bidiVisual` / floating `w:tblpPr`
- Run `w:caps`/`smallCaps` done; remaining `w:outline`/`emboss`/`imprint`/`effect`, `w:bdr`, color theme tint/shade

---

## Explicitly deferred out of Phase 1A (reported, byte-preserved)
- Property-change revision *content* (the historical values themselves) — only the current values are modeled; the change is dispositioned.
- Move revisions (`w:moveFrom`/`moveTo`), custom-XML/cell revisions.
- Non-embedded drawings (charts, SmartArt, linked blips).
- `commentsExtended`/`Ids`/`Extensible`, comment range markers.
- Ruby annotation text (base text preserved).

---

## Phase-exit checklist (delete this file when all ✅)
- [ ] Sequence items 1–5 above merged.
- [ ] Low-tail swept or explicitly deferred with a tracked reason.
- [ ] A coverage re-audit confirms every remaining reported item is intentional (not silent).
- [ ] `14-EXECUTION-TRACKER.md` updated with the final P1A summary row.
- [ ] Fidelity harness re-run on the corpus; no content regressions.
