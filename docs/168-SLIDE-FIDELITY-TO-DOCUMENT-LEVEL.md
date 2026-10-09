# 168 — Slide fidelity to document level: the measured gap, the reuse map, and the lanes

**Status:** Programme tracker. **Opened:** 2026-10-06.
**Scope:** Bringing `casual-pres-*` to the fidelity bar `casual-doc-*` already holds, and
the webapp surface to the bar `editor.html` already holds. Design and sequencing live
here; the ranked rows live in `109` (PRES-01 and the rows this programme opens).
Tracker rows: `109` PRES-01. Design of the engine itself: `156`.

This document exists because the same mistake was made four times in one week: building a
second copy of something the tree already had. A parallel `.slides-*` CSS vocabulary with
no stylesheet behind it; a second popover manager; a second zoom ladder that disagreed
with the first about what 150% means; and a second line-stacking loop that was wrong in a
way the first one had a guard against. Every lane below therefore names, before it starts,
**what it consumes** — and a lane whose "consumes" column is empty has to justify that in
its own write-up rather than in review.

## 1. The bar, measured per layer

The document engine is the standard. "Shared" means the slide path calls the document
crate itself rather than carrying a copy.

| Layer | Document crate | lines | Slide path | State |
| --- | --- | ---: | --- | --- |
| Model | `casual-doc-model` | 22,764 | `casual-pres-model` plus the whole `v1` drawing vocabulary, shared | at bar |
| Import | `casual-doc-import` | 36,977 | `casual-pres-import`; own OPC reader, shared colour/theme/text readers | partial |
| Layout | `casual-doc-layout` | 63,440 | `casual-pres-layout` is 4,416 lines **because** it consumes the shared walk, shaper, display list and table path | partial |
| Raster / PDF | `casual-doc-render`, `-pdf` | 4,899 | shared outright — a slide composes into the same `DisplayList` | at bar |
| Export | `casual-doc-export` | 19,035 | `casual-pres-export` plus verbatim retention of unmodelled parts | partial |
| Transactions | `casual-doc-transaction` | 15,679 | nothing | ADR-067 proposed (`169`) |
| Editing ops | `casual-doc-edit` | 20,697 | nothing | blocked on the above |
| Diff | `casual-doc-diff` | 5,109 | nothing | not started |
| Selection | `casual-doc-selection` | — | nothing | not started |

Slide-specific code is **28,618 lines, 8.6%** of a 332,740-line engine. ONLYOFFICE's
equivalent split, measured in the local reference tree rather than quoted from their
marketing, is **96,178 of 924,744 — 10.4%** across `slide/`, `common/` and `word/`. Two
independent implementations landing two points apart is the strongest available evidence
that `156` §1's bet was right, and it bounds the remaining work.

**Read, lay out and render are at or near document level. Everything that CHANGES a deck
is at zero**, and the first of those is a design decision rather than an implementation.

## 2. The gap, measured on a real deck

Every finding the engine reports on a real 21-slide, 11-layout deck exported from Google
Slides (`fixtures/local/`, untracked — see its README for why a real file is never
committed). Ordered by how much of the file each touches.

| Construct | count | What is lost on screen | Consumes | Lane |
| --- | ---: | --- | --- | --- |
| `a:sym` | 229 | A run's symbol font; dingbats resolve to the wrong face | nothing — the document model has no symbol-font field either, so **both classes gain one** | E3 |
| `a:buSzPts` | 194 | Bullet size; bullets render at the paragraph's size | `text_bullet.rs` models the bullet already; only the size is absent | E1 |
| `cNvPr/@id` | 170 | The producer's own shape id | — | later |
| `grpSp/@name`, `grpSp` fill/outline | 33×3 | Group identity and suppressed paint inside a group | the `SlideNode` wrapper pattern; a `GroupChild` has no wrapper | E3 |
| `a:round` / cap / cmpd | 30 | Every outline draws as a plain round-capped single line | **`v1::StrokeDetail` already models all of it** and its own doc says nothing paints it **on either class** | E2 |
| `a:srcRect` | 24 | A cropped picture shows its whole self, wrongly scaled | **`GroupPicture.crop` exists and `AnchorContent::Image` already paints it** | E1 |
| `p:cxnSp` | 22 | Connector arrows and lines are not drawn | `AnchorContent::Line`, the 187-preset table | E1 |
| `grpSp/txBody` | 11 | Text inside a group reaches no model: invisible and unreadable | the wrapper the top level has | E3 |
| `a:hlinkClick` | 5 | Hyperlinks on shapes and pictures | **`GroupShape.hyperlink` and `GroupPicture.hyperlink` both exist** | E1 |
| notes parts | 22 | A deck with speaker notes reopens without them | the three-tier cascade already built for slides | E4 |
| `embeddedFontLst` | 8 | The deck ships Lato and Raleway inside it; neither is used | **`registerFonts` exists on the facade**; the bytes are in the package | E1 |
| table style parts | 11 | A styled table paints its cells' own formatting and nothing the style adds | the GUID already joins | later |
| `scene3d`, `sp3d` | 2 | 3-D bevels | nothing, on either class | declined |

**Five of thirteen are pure read-side work**: the machinery exists and already paints on
the document path, and the slide importer does not call it. Those five — crop, hyperlinks,
connectors, bullet size, embedded fonts — are 253 of the occurrences above.

**One is a shared render gap the document editor also has.** Line cap, join and compound
are modelled on both classes and painted on neither. That makes it Tier 0 work in `156`'s
sense: it improves DOCX output, so it is not slide work at all.

## 3. Competitive, and what it says about sequencing

ONLYOFFICE is a **behavioural reference only** — AGPL-3.0, so no code, string, identifier
list or data table from it enters this tree. What is used below is structural fact: which
modules exist, and how large.

| Capability | ONLYOFFICE | Google Slides | opendoc |
| --- | --- | --- | --- |
| Open a real `.pptx` offline | no — format I/O is native `x2t`, with no WebAssembly build | no — converts on upload | **yes** |
| Loss reporting | none surfaced | none surfaced | **73 findings, per construct and part** |
| Licence | AGPL-3.0, commercial tiers, per-tab Developer Edition | proprietary SaaS | **Apache-2.0** |
| Slide model | Presentation, Slide, Layout, Master, **Notes, NotesMaster, Comments, Timing, ViewPr**, ChartSpace, TextBody, Shape, Group, Image, Styles | equivalent | ours lacks Notes, Comments, Timing, ViewPr |
| Editing | full | full | none — needs an ADR first |
| Transitions / animation | tabs for both; `Timing` in the model | both | retained byte-for-byte, not modelled |
| Speaker notes | first-class | notes pane plus presenter view | **reported as lost** |
| Slide show | preview plus pen annotation while presenting | present, presenter view, Q&A, captions | **none** |
| Accessibility of slide text | not surfaced as a structural mirror | their own DOM | **off-screen structural mirror** |

Three capabilities neither competitor can match without changing its architecture or its
business model — offline open, loss reporting, permissive licence — and everything else in
the table is catch-up. **The two gaps that would end an evaluation are speaker notes and
slide show**, and neither is blocked on the editing decision, which is what makes them the
right product work to run beside the engine lanes.

## 4. The lanes

Parallel, scoped by file domain so they cannot collide (`SKILL` §7).

| Lane | Files | Work | Consumes |
| --- | --- | --- | --- |
| **E1** read-side | `casual-pres-import/**`, `casual-pres-layout/**` | `a:srcRect`, `a:hlinkClick`, `p:cxnSp`, `a:buSzPts`, `embeddedFontLst` → the font registry | `GroupPicture.crop`, `.hyperlink`, `AnchorContent::Line`, the preset table, `registerFonts` — all existing, all painted on the document path |
| **E2** shared render | `casual-doc-layout/**`, `casual-doc-render/**` | Paint `StrokeDetail`: cap, join, compound. **Fixes the document editor too** | `v1::StrokeDetail`, the `tiny-skia` backend |
| **E3** model | `casual-doc-model/**`, `casual-pres-model/**` | Grouped-shape text and paint; `a:sym` | the `SlideNode` wrapper pattern |
| **E4** notes | `casual-pres-*/**` | `p:notesSlide`, `p:notesMaster` — model, reader, writer | the three-tier cascade already built |
| **W1** webapp | `webapp/**` | Notes pane under the stage | `.side-panel`, `.panel-head`, the rail |
| **W2** webapp | `webapp/**` | Slide show / present mode | `modal.mjs`, the existing full-surface render path |
| **D1** design | `docs/**` | An ADR for the slide operation set inside `casual-doc-transaction` — **proposed as ADR-067, designed in `169`** | ADR-005, ADR-043, the document editor's closed operation set as the shape |

E2 touches the shipped DOCX editor, so it carries the editor's full browser suite as a
gate. E1 touches neither shared crate nor `webapp/`, which is why it goes first.

## 4a. The shell: one application, not two

The owner's rule (2026-10-09): "use whatever we can from our document shell and
engine, so the UX does not differ from slides to the document editor." Measured
against `editor.html` at 1440×900 before this section's work, `slides.html` had a
text menu bar where the editor has a ribbon tab strip, no Compact/Ribbon switch, a
black "Open a presentation" button and a truncated fidelity sentence where the
editor has its state and findings chips, a status bar reading "Slide 1 of 10
100%" where the editor has a language control and a zoom control, number-free
thumbnail captions in a monospace face, and a slide fitted by width alone, so a
16:9 deck opened with its bottom below the fold.

**Shared now** — each the editor's own module or markup, not a copy:

| Piece | From the editor | Notes |
| --- | --- | --- |
| Ribbon, tab strip | `.ribbon-tabs`/`.ribbon-panel`/`.rgroup` markup, `ribbon_nav.mjs` roving | File, Home and View: the three tabs a viewer has commands for. Captions are the editor's keys. Built by `slides_chrome.mjs` from the same registry the compact bar and the menus read |
| Compact / Ribbon switch | `.chrome-mode` markup, `radio_group.mjs`, `prefs.mjs` | On the editor's `opendoc.chromeMode` preference, so the choice holds on both pages; a phone forces compact as the editor does |
| State and findings chips | `#documentState`, `.compatibility-status`, `compat_findings.mjs` | The chip opens the editor's findings dialog; `deckReportJson` restates the deck report in the editor's report shape, including the outcome spellings (`not-retained` → `not_retained`) |
| Language control | `locale_boot.mjs`'s `startLocalisation`, the footer menu | On the editor's `opendoc.settings`, so one language setting across both pages (`124` §5) |
| Theme | `appearance.mjs` | Also on `opendoc.settings` |
| Zoom control | the footer's `.zoom` markup, `view_zoom.mjs`'s ladder and parser | The percentage is relative to the fit, which is what "100%" means on a slide in both reference products |
| Fit on open | — | Whole slide, measured on both axes, as the editor opens a page |
| Slide navigator | `.pages-panel`/`.page-thumb`, `pages_panel.mjs` | Captions are numbers, as in the editor's navigator; the slide's name is the card's accessible name |

Every new string is an existing editor key with identical English;
`slides_chrome.test.mjs` fails the build if a borrowed key's English drifts,
because `build-locale.mjs` merges both pages' markup into one catalogue.

**Not shared yet, and why.** The editor's settings dialog, document-properties
dialog and editing-mode menu are bound by id inside `main.js`, which has no mount
seam (`109` HF-109); the ribbon's own controller is in `slides_chrome.mjs` for the
same reason. They move to a shared chrome module when HF-109 lifts them out of
`main.js`, and `slides_chrome.mjs` is written to be that module's first consumer.
The editing-mode control is absent rather than disabled: a deck has no mode to
choose until the slide operation set (lane D1 above) exists.

## 5. What this programme will not do

**Editing.** No slide operation set, no transaction envelope. A surface that mutated a deck
without routing through `casual-doc-transaction` would repeat the defect `105` CQ-002
records on the document path, where ADR-005 came to be honoured nowhere. D1 produces the
ADR — ADR-067, designed in `169` — and implementation is a separate programme that starts
only when the owner accepts it.

**Master and layout editing.** Out of v1 scope on the competitive evidence: neither web
client edits masters, and a layout *picker* is the table-stakes item.

**3-D.** `scene3d`/`sp3d` are modelled by neither document class and are reported.

**Chart and SmartArt payloads.** `155`/ADR-050 owns them. A `p:graphicFrame` holding one is
placed as a positioned empty box and the payload is reported by name, which is a narrower
loss than discarding the frame.
