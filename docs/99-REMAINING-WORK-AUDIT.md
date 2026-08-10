# 99 — Remaining Work Audit

**Status:** Living record. **Date:** 2026-08-09.
**Scope:** everything known to be unfinished in this repository, ranked, with the
evidence for each claim.

## Why this document exists

Asked "what is remaining", the honest answer had to be assembled by reading the
tracker, the gap audits, the support matrix and the code — and parts of the
tracker were **wrong**, still reporting shipped work as "Not started". A question
that important should not need an archaeology session, and should not be
answered from a document that misleads.

This is that answer, written down. It is deliberately pessimistic: anything not
demonstrated by a test is listed as unfinished, and capability that ships is
graded at the level it actually reaches, not the level it aims at.

Sources of truth this consolidates, and which stay authoritative in their own
areas: `14-EXECUTION-TRACKER.md` (per-slice status), `18-SUPPORT-MATRIX.md`
(public support claims), `44-COVERAGE-GAP-AUDIT.md` (model/round-trip coverage),
`46`/`55`/`60` (rendering fidelity), `67-EDITOR-UX-GAP-ANALYSIS.md` (editor UX),
`85` §5.7/§5.9 (object-editing scope), `98` (PDF).

## Owner priority decision — editor first

As of 2026-08-11, execution is ordered around document editing. This supersedes
the apparent priority implied by older phase numbering; it does not erase or
overstate the deferred work.

| Order | Active outcome | Exit gate |
| --- | --- | --- |
| 1 | Document safety and DOCX fidelity | No silent loss; unsupported content is preserved or reported; the oracle, visual, table, float/wrap, and font-fallback gates are materially closed |
| 2 | Editing experience and interaction reliability | The editing-surface continuity contract in doc 58 passes across body, running content, notes, text boxes/groups, and table cells |
| 3 | Authoring-model completeness | Sections, styles, note options, fields, drawing geometry, picture content/effects, text-box body properties, and typed run properties are represented deterministically and mutate only through commands/transactions |
| 4 | Missing editing parity | The high-value authoring controls in §2 ship against the completed model rather than as host-only state |
| 5 | Stable public SDK | The editor-proven commands and errors become a consolidated, versioned embedding boundary |
| 6 | OT/CRDT | Collaboration is designed over stable commands, transactions, identities, and selection semantics |

PDF, GPU rendering, Tauri, worker threading, plugin ABI, canonical CBOR, and
collaboration are not active editor-milestone work. The current SDK receives
only compatibility maintenance needed to keep editor development testable.

The quality bar for every active outcome is production/enterprise, not MVP:
deterministic model-owned behavior, bounded resource use, no silent data loss or
cross-surface mutation, explicit unsupported results, a regression for every
fixed defect, and the complete relevant gate set before a tracker row becomes
`Done`.

---

## 1. The open defect class — context and interaction

Every editing defect reported by the owner in August 2026 was one class: **a rule
the host inferred instead of a rule stated once**. Inferred from a hit-test miss,
from `pages[0]`, from the host's own copy of the page setup. Closing it for
headers (PR #475) and for text boxes and the ribbon (PR #476) each uncovered more
of it, which is the signature of a class rather than a bug.

The class is **not exhausted**. Untested, in the order the next defect is most
likely to be found. The status is maintained as each bounded test slice lands:

| # | Context / gesture | Status | Why it is suspect / evidence |
| --- | --- | --- | --- |
| 1 | Grouped box and nested objects — click inside, click away, Escape | Covered 2026-08-11 | `nested-object-editing.spec.mjs` derives the grouped child's bounds from its selection outline and proves inside-empty-space retention, body click-away, and the two-step Escape grammar |
| 2 | Drag-selection ACROSS a context boundary (header→body, box→body) | Fixed 2026-08-11 | Both reproductions initially failed: pointer-move reused click-away resolution and exited the starting story. Pointer-down now retains the owning running band/text box and clips later moves at that boundary; `surface-boundary-selection.spec.mjs` proves context and subsequent typing stay in the starting story |
| 3 | Zoom change while inside a context | Open | Caret geometry is now page-scoped; nothing proves it survives a zoom or a re-render |
| 4 | Table cells as a context | Open | The one editing context never examined in this sweep |
| 5 | Scroll while a band is open | Open | The band chrome is drawn per page; scrolling to a page whose band is not mounted is untested |

**Recommendation:** finish this sweep before adding capability. It is the same
class that produced every symptom the owner hit.

### Standing lesson

A matrix proves nothing about the case it does not exercise. The
operation × surface matrix was green on a **one-page** sample while multi-page
clicking was broken; the reproductions in
`webapp/tests/e2e/reported-editing-defects.spec.mjs` now build a genuinely
multi-page document for that reason. Four failures during that work were harness
errors, not product defects — clicks landing outside the window, points guessed
instead of derived from what the editor draws. Derive test points from the
product's own geometry.

---

## 2. Editing capability not yet authorable

These render and round-trip; the editor cannot create or change them. Graded from
the fidelity matrix (`webapp/src/fidelity.js`), which the frontend unit tests pin.

| Capability | State |
| --- | --- |
| Shape rotation and flip | Modeled, rendered, round-trips. No handle or control |
| Custom shape geometry (`custGeom`) | Retained verbatim, not typed; not authorable |
| Replace a picture's bytes in place | Insert/crop/alt-text/geometry ship; replace does not |
| Picture borders and effects | Rendered, not authorable |
| Text-box body properties (insets, vertical anchor, autofit) | `SetTextBoxBody`, doc 52 — designed, not built (`P1G-OBJ-TEXTBOX`) |
| Footnote↔endnote conversion; note number formats and restart | Insert and body editing ship; Word's note options do not |
| Insert or split a section | Page setup is editable; section structure is not |
| Create or update a named style | Apply-from-gallery ships; authoring styles does not |
| Fields as fields | Insert ships; editing a field's definition does not |
| Math (OMML) | Fully typed, common constructs typeset. No equation editor |

---

## 3. Engine and rendering

| Area | State |
| --- | --- |
| **PDF export** | **Not built.** Designed end to end in `98-PDF-EXPORT-AND-PRINT-DESIGN.md`; blocked on ADR-031 (writer/subsetter build-vs-buy) and a Phase-2 scope gate. Printing currently goes through the browser from engine-rendered pages |
| Charts, SmartArt | Modeled as first-class references, preserved byte-for-byte, **not drawn** — an embedded preview if the file has one, else a placeholder |
| Colour fonts / colour emoji | Not rasterized. Emoji render through a monochrome face; no COLR/CBDT/sbix path |
| Text wrap around floats | Top-and-bottom and square reserve shared flow, including in cells, headers and footers. **Tight and through contour wrapping, and page-coupled reflow, remain partial** |
| Oracle page parity | 3 of 5 corpus documents exact; SDS +1 (final-page column balancing), Medical −1 (document-grid row heights) |
| Typed underline style/colour | `P1F-38` — the boolean underline draws; the typed style/colour is not modeled. Still open |
| `.docm` macro files | **Rejected at open.** Strip-and-open versus explicit non-support is an undecided policy question, not an oversight |
| Long tail (docs 44 Tier 4) | `latentStyles`, glossary/AutoText **semantics**, ruby annotation, ink (`w:contentPart`), generic `w:framePr` layout, `w:background`, vertical text, distribute alignment, kashida justification. Preserved opaque where possible; not semantically modeled |

---

## 4. Not started

| Area | Note |
| --- | --- |
| GPU render backend | CPU raster is the reference backend; Phase 1E's GPU half is not begun |
| Tauri desktop shell + host fonts | `P1G-004`. Deliberately after the browser-first work |
| Worker threading (SAB + OffscreenCanvas) | `P1G-005` |
| Stable public SDK surfaces | Internal crates are unpublished by design while contracts move |
| Collaboration adapters | Phase 5. Seams reserved by ADR-030 / doc 45; the OT-vs-CRDT decision is still open |

### Pending decisions (docs 08)

Shaping stack (HarfBuzz vs platform), native renderer choice, internal text
storage (rope / piece tree / chunked), collaboration operation model, canonical
CBOR profile and golden vectors, plugin ABI stability, fixed-point layout units,
ADR-031 (PDF), and the `.docm` policy.

---

## 5. Process debt

- **The execution tracker is stale.** Rows including `P1G-HF-CONTEXT`,
  `P1G-HF-CONTENT`, `P1G-HF-VARIANTS`, `P1G-HF-LINK` and `P1G-OBJ-STRUCTURE` read
  "Not started" for work that has shipped and is under test. It is the document
  someone would use to answer "what is remaining", so a stale row is worse than a
  missing one. Corrected alongside this audit; keep it current per PR.
- **The public fidelity page carried fabricated evidence** until 2026-08-09 —
  five fixture rows (`tables-nested.docx`, `toc-leaders.docx`, `sections-mixed.docx`,
  `cjk-mixed.docx`, `floats-wrap.docx`) for files that do not exist in this
  repository, with invented page counts and a "Δ 0.4% pixels" figure. Replaced
  with the real oracle corpus and its real numbers. Nothing on a page whose
  premise is "measured, not claimed" may be illustrative.
- **`node --test webapp/tests/*.test.mjs` is easy to skip.** The browser suite
  passing is not sufficient; the frontend unit tests pin the public support
  matrix and caught an overstatement CI had to reject.
