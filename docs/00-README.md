# Casual Docs Runtime & SDK — Architecture Blueprint

**Status:** Draft v0.1
**Audience:** CasualOffice maintainers, SDK contributors, platform integrators
**Primary implementation:** Rust
**Primary hosts:** Tauri desktop, WebAssembly/web, headless/server
**License:** Apache-2.0

## Purpose

This document set defines the initial product and engineering blueprint for a reusable document editing runtime that can power Casual Docs and can also be embedded by third parties.

The runtime is not a UI toolkit and is not a DOCX-only editor. It is a deterministic document engine with:

- a stable document model;
- editing and transaction semantics;
- layout and pagination;
- rendering;
- import/export;
- collaboration hooks;
- extension APIs;
- native and WebAssembly bindings.

## Document set

- `01-ORD.md` — outcome and product requirements.
- `02-ARCHITECTURE.md` — target architecture and design principles.
- `03-HLD.md` — major components, data flow, deployment, and interfaces.
- `04-LLD.md` — detailed modules, traits, data structures, algorithms, and error model.
- `05-SDK-API-SPEC.md` — public SDK surface and embedding contract.
- `06-ROADMAP-AND-DELIVERY.md` — implementation phases, milestones, staffing, and acceptance gates.
- `07-QUALITY-SECURITY-AND-COMPATIBILITY.md` — testing, performance, security, compatibility, and release criteria.
- `08-ADR-REGISTER.md` — initial architecture decisions.
- `09-REPOSITORY-AND-CONTRIBUTION.md` — proposed repository structure and engineering workflow.
- `10-PROJECT-GOAL-AND-STANDARDS.md` — production goal and non-negotiable standards.
- `11-DESIGN-FIRST-PROCESS.md` — required research, design, tracking, and delivery flow.
- `12-COMPETITIVE-ANALYSIS.md` — current product and SDK comparison.
- `13-UX-AND-BUG-HUNTING.md` — UX review areas and defect policy.
- `14-EXECUTION-TRACKER.md` — current project execution state (terse; detail in git/PRs/design docs).
- `15-CI-AND-RELEASE-GATES.md` — automated quality and release gates.
- `16-DOCUMENTATION-MAINTENANCE.md` — documentation ownership and freshness.
- `17-GLOSSARY.md` — canonical project terminology.
- `18-SUPPORT-MATRIX.md` — platform, host, format, and feature targets.
- `19-WORKSPACE-SCAFFOLD-DESIGN.md` — accepted initial Rust workspace.
- `20-ERROR-CODE-REGISTRY.md` — stable public error taxonomy.
- `21-PARSER-LIMITS.md` — bounded parsing and resource policy.
- `22-NORMALIZED-SCHEMA-V0.md` — first normalized-model contract.
- `23-DOCX-FIXTURE-CORPUS.md` — fixture rights, metadata, and comparison policy.
- `24-TRANSACTION-SEMANTICS.md` — edit, mapping, inverse, and history semantics.
- `25-NORMALIZED-SNAPSHOT-IO.md` — strict bounded JSON load/export contract.
- `26-SELECTION-FOUNDATION.md` — caret/range state and transaction mapping.
- `27-RUNTIME-EVENT-FOUNDATION.md` — ordered bounded session event delivery.
- `28-DOCX-PACKAGE-READER.md` — bounded ZIP package admission and part reads.
- `29-BENCHMARK-AND-BASELINE-HARNESS.md` — reproducible timing and report contract.
- `30-PHASE-0-CLOSURE-DESIGN.md` — corpus, fuzzing, and exit-evidence closure.
- `31-PHASE-0-EXIT-REPORT.md` — accepted Phase 0 evidence and deferrals.
- `32` *(retired — merged into 38)* the Phase 1A semantic-import design is now the
  "Import architecture" section of `38-SCHEMA-V1-DESIGN-REFERENCE.md`.
- `33-DOCX-ENGINE-COMPETITOR-RESEARCH.md` — source-architecture study of DOCX import, editing, preservation, and export.
- `34-OOXML-FIDELITY-ARCHITECTURE.md` — proposed dual-representation fidelity and future save-planning contract.
- `35-DISPOSITION-TAXONOMY.md` — dual-axis (model/retention) disposition taxonomy for the compatibility report.
- `36-ADR-027-ACCEPTANCE-RECORD.md` — ADR-027 acceptance record (decisions D1–D11, reconciliations R1–R4).
- `37-PHASE-1A-DECISION-RESEARCH.md` — cited Word/ONLYOFFICE/LibreOffice research behind the Phase 1A decisions.
- `38-SCHEMA-V1-DESIGN-REFERENCE.md` — **consolidated** schema-v1 design record:
  semantic-import architecture, base schema v1, and one section per modeled
  construct (tables, fields, text boxes, notes, headers/footers, VML, extra-part
  media, ruby, comments, tracked changes, run/paragraph/table properties,
  bookmarks, content controls). Merges the former docs 32 and 38–53; the tracker
  references it by anchor. See its header for the old-number → section map.
- `39-PHASE-1B-SEMANTIC-DOCX-WRITER-DESIGN.md` — the semantic writer design (model → valid editable `.docx`).
- `40-FONT-MANAGEMENT-DESIGN.md` — accepted full-scope font resolution/substitution/metrics design.
- `41-PHASE-1B-EXIT-REPORT.md` — accepted Phase 1B evidence, coverage, and the no-silent-loss guarantee.
- `42-RENDERING-ARCHITECTURE-RESEARCH.md` — prior-art + ecosystem + algorithm research for the Phase 1C layout/pagination/rendering engine.
- `43-PHASE-1C-LAYOUT-RENDERING-DESIGN.md` — proposed production layout/pagination/rendering engine design (Phases 1C–1E).
- `44-COVERAGE-GAP-AUDIT.md` — evidence-backed inventory of unhandled/partial/lossy constructs (WML content, package parts, layout/render), tiered by severity; source of the P1F-* tracker rows.
- `45-EXTENSIBILITY-AND-COLLABORATION-SEAMS.md` — the four invariants that keep collaboration (OT/CRDT), MCP/agentic, and RAG/vector layers additive (ADR-030); review checklist for model/mutation PRs.
- `46-RENDERING-FIDELITY-GAP-ANALYSIS.md` — evidence-based diagnosis of the layout/render fidelity gaps (measured against LibreOffice as the layout oracle) and the prioritized fix roadmap (F1 style cascade through F7 appearance details).
- `47-TEXT-BOX-APPEARANCE-DESIGN.md` — text-box extent, appearance, anchor, and semantic fixed-point design.
- `48-NESTED-FLOAT-ANCHOR-DESIGN.md` — recursive float discovery and placement through body and running-content tables.
- `49-TABLE-VERTICAL-MERGE-DESIGN.md` — vertical-merge layout, pagination, paint, and malformed fallback rules.
- `50-STYLED-SEGMENTED-TABLE-BORDER-DESIGN.md` — common styled border paint, segmented span topology, and bounded fallbacks.
- `51-FLOAT-TEXT-REFLOW-DESIGN.md` — local `topAndBottom` float exclusion across body, running content, and nested cells.
- `52-TEXT-BOX-BODY-PROPERTIES-DESIGN.md` — DrawingML insets, vertical anchoring, overflow, and autofit behavior.
- `53-PER-SECTION-ANCHOR-GEOMETRY-DESIGN.md` — section-aware page/margin/column reference boxes for floating objects.
- `54-VML-TEXT-BOX-POSITIONING-DESIGN.md` — bounded VML position, wrap-distance, text-body, and safe body-float bridge.
- `55-CURRENT-DOCX-FIDELITY-GAP-AUDIT.md` — current-main reconciliation of remaining render, layout, nested-content, running-content, and semantic fidelity gaps, with PR order and acceptance gates.
- `56-EDITOR-SHELL-AND-RENDER-ARCHITECTURE.md` — fat-client editor/viewer shell and rendering architecture.
- `57-PHASE-1G-IMPLEMENTATION-PLAN.md` — build-level plan for the WASM bridge, viewer, text overlay, workers, and editing path.
- `58-INTERACTION-SELECTION-EDITING-ARCHITECTURE.md` — interaction, selection, and editing architecture over stable layout anchors.
- `59-V1-EDITING-OP-SET.md` — closed text-first editing operations over the rendered `v1::Document`.
- `60-FIDELITY-CORPUS-RENDERING-AUDIT.md` — fixture-backed rendering and pagination audit covering mixed page sizes, TOC leaders, section furniture, inline images, CJK overflow, and collision gates.
- `61-UNEQUAL-COLUMN-AND-CJK-CONTAINMENT-DESIGN.md` — post-PR-170 design for physical-width unequal-column flow, page/column break identity, authored dense CJK spacing, and bounded fallback-wrap correction.
- `62-FOOTNOTE-ENDNOTE-PAGINATION-DESIGN.md` — bounded fixed-point note reservation and endnote-flow design.
- `63-EDITOR-UI-UX-DESIGN-SYSTEM.md` — editor shell visual language, component states, accessibility, and responsive behavior.
- `64-EDITOR-TOOLBAR-RIBBON-DESIGN.md` — editor command/ribbon information architecture and interaction design.
- `65-VISUAL-CONTAINMENT-AND-LINK-INTERACTION-DESIGN.md` — accepted design for line/table containment, cross-paragraph floats, framed drop caps, hyperlinks/TOC navigation, and visual regression gates.
- `66-DEVELOPER-SITE-AND-DEMO-DISCOVERY-DESIGN.md` — developer-first GitHub Pages landing page, live sample/editor routing, and search/LLM discovery contract.
- `67-EDITOR-UX-GAP-ANALYSIS.md` — evidence-based editor UX gaps and prioritized delivery gates.
- `68-COMMENTS-AND-SUGGESTIONS-DESIGN.md` — review-mode comments, suggestions, and host-policy design.
- `69-MS-WORD-2026-COMPETITIVE-DESIGN-GUIDE.md` — current Word/Vellum interaction and visual reference guidance.
- `70-TABLE-EDITING-UX-FOUNDATION-DESIGN.md` — contextual table ribbon, right-side properties inspector, and atomic Apply contract.
- `71-HISTORY-FORMATTING-PARAGRAPH-INSPECTOR-DESIGN.md` — undo-history labels, the formatting-state model, and the paragraph inspector.
- `72-TABLE-DISTRIBUTION-DESIGN.md` — distributing table rows and columns evenly.
- `73-TABLE-SORT-DESIGN.md` — sorting a table's rows.
- `74-TABLE-METADATA-DESIGN.md` — a table's caption and accessibility description.
- `75-TABLE-FORMULA-DESIGN.md` — table formula calculation.
- `76-LIST-LEVEL-DESIGN.md` — promoting and demoting list levels.
- `77-LINK-CHIP-DESIGN.md` — the link chip: viewing and editing a hyperlink in place.
- `78-BOOKMARK-MANAGER-DESIGN.md` — the bookmark manager surface.
- `79-SAVE-SHORTCUT-DESIGN.md` — the save shortcut.
- `80-CURSOR-SELECTION-HIT-AUDIT.md` — audit of cursor placement, selection, and hit testing.
- `81-COMMENTS-SUGGESTIONS-COMPLETENESS-AUDIT.md` — gap inventory for comments and suggestions.
- `82-REVIEW-IDENTITY-AND-HISTORY-DESIGN.md` — review identity, scoped review operations, and history safety.
- `83-SDK-PACKAGING-EMBEDDING-AND-EXTENSIBILITY-ARCHITECTURE.md` — SDK packaging, embedding, preview mode, collaboration, MCP AI tools, and plugin extensibility architecture.
- `84-CONTEXT-MENU-AND-COMMAND-REGISTRY-DESIGN.md` — context menus and the shared command descriptors behind every menu, ribbon, and shortcut.
- `85-DRAWING-OBJECT-AND-HEADER-FOOTER-EDITING-DESIGN.md` — editing drawing objects, headers, and footers.
- `86-REVISION-AWARE-EDITING-DESIGN.md` — revision-aware range splitting/normalization so typing, deleting, and formatting work inside pending suggestions and across Revision/Hyperlink/SDT wrappers (REVIEW-GAP-007).
- `87-PRESET-SHAPE-MODEL-AND-PRIMITIVE-RENDERING-DESIGN.md` — the preset-shape model and primitive rendering (first shape-fidelity slice).
- `88-ANGULAR-PRESET-SHAPE-RENDERING-DESIGN.md` — rendering the angular preset shapes.
- `89-CONDITIONAL-TABLE-STYLE-TEXT-AND-BORDER-DESIGN.md` — text formatting and borders from conditional table-style regions.
- `90-INTRINSIC-INLINE-BOX-TABLE-SIZING-DESIGN.md` — table column sizing from the intrinsic size of inline boxes.
- `91-TABLE-ALIGNMENT-AND-BIDI-VISUAL-DESIGN.md` — table alignment and right-to-left (bidi-visual) table layout.
- `92-TABLE-CELL-SPACING-DESIGN.md` — laying out tables with cell spacing.
- `93-REVIEW-MARKUP-RENDER-VIEW-POLICY-DESIGN.md` — a read-only `ReviewView::Markup` galley policy so the native/PNG viewer shows struck deletions, author-colored/underlined insertions, and highlighted comment ranges without double-marking the webapp editor (docs/55 §11).
- `94-ORACLE-VISUAL-FIDELITY-HARNESS-DESIGN.md` — the LibreOffice-oracle visual fidelity harness (`tools/opendoc-fidelity`).
- `95-ODT-IMPORT-PROFILE.md` — bounded ODF package admission and staged ODT-to-schema-v1 semantic mapping, preservation, compatibility, and security rules.
- `96-ODT-EXPORT-PROFILE.md` — deterministic bounded ODF 1.4 package writing, partial semantic mapping, exact-unchanged recovery, loss reporting, and export gates.
- `97-ODT-EDIT-TOLERANT-PRESERVATION.md` — tolerant preservation and export behavior after ODT edits.
- `98-PDF-EXPORT-AND-PRINT-DESIGN.md` — PDF writer, font-subsetting, print, accessibility, and release-gate design.
- `99-REMAINING-WORK-AUDIT.md` — current prioritized repository state, incomplete editing/model/fidelity areas, and deferred SDK/collaboration work.
- `100-DOCUMENT-GRID-LINE-PITCH-DESIGN.md` — document-grid line pitch (`w:docGrid`).
- `101-EDITOR-OBJECT-AND-INSERT-PANEL-AUDIT.md` — deep object-editing correctness/UX audit, competitive analysis, contextual Properties/Insert panel design, equation-authoring gate, and prioritized delivery matrix.
- `102-COLOR-GLYPH-RENDERING-DESIGN.md` — rendering colour glyphs and emoji.
- `103-GLYPH-PANEL-UX-DESIGN.md` — the symbol and glyph picker panel.
- `104-HOTFIX-TRACKER.md` — ranked queue of confirmed UX, UI, and correctness defects in shipped code, with cross-cutting root-cause themes and the owner decisions that block several rows.
- `105-AUDIT-2026-09-TRACKER.md` — the September 2026 audit (an archive, closed to new rows; open work lives in `109`).
- `106-ONLYOFFICE-ALTERNATIVE-ROADMAP.md` — roadmap for the Apache-2.0 alternative to ONLYOFFICE Docs for documents.
- `107-COLLABORATION-OT-SNAPSHOT-REPLAY-DESIGN.md` — collaboration as operational transformation over transactions, with snapshot/replay versioning.
- `108-PARAGRAPH-LEVEL-TRACKED-CHANGES-DESIGN.md` — tracked changes at paragraph level (inserted and deleted paragraphs, paragraph-mark changes).
- `109-BACKLOG.md` — **the single working queue**: every piece of open work, in the order it will be worked.
- `110-RTF-IMPORT-PROFILE.md` — the RTF import profile.
- `111-LARGE-DOCUMENT-MEMORY-DESIGN.md` — memory for very large documents: what a 1.3M-paragraph file costs, and how it is admitted.
- `112-AUTOSAVE-AND-CRASH-RECOVERY-DESIGN.md` — autosave, drafts, and crash recovery.
- `113-WINDOWED-LAYOUT-DESIGN.md` — windowed layout: laying out only the pages someone is looking at.
- `114-SPELL-CHECK-DESIGN.md` — client-side spelling and grammar, the product glossary, and the personal dictionary.
- `115-STYLES-CONTROL-AND-DROPDOWN-CONSISTENCY.md` — the Styles control, and one dropdown model across the chrome.
- `116-MAIN-THREAD-BUDGET-AUDIT.md` — audit of every operation whose cost scales with the document.
- `117-GROUPED-OBJECT-SELECTION.md` — selecting a shape inside a group.
- `118-DOCUMENT-AUDIT-2026-09-22.md` — measured audit of what was still wrong with the owner's own documents.
- `119-CUSTOM-SHAPE-GEOMETRY.md` — custom shape geometry (`a:custGeom`).
- `120-ACCESSIBILITY-GROUPED-TEXT-AND-FORM-CHECKBOXES.md` — grouped text and form checkboxes in the accessibility mirror.
- `122-ONE-AXIS-NAVIGATION-DESIGN.md` — one navigation axis per chrome.
- `123-RIBBON-AND-FILE-PAGE-VISUAL-DESIGN.md` — the visual contract for the ribbon and the File page.
- `124-LOCALISATION-DESIGN.md` — localisation: the string seam, the pipeline, and the first eighteen locales.
- `125-EMBED-AND-HOST-CONTRACT-DESIGN.md` — the embed and host contract.
- `126-EMBEDDABILITY-AND-SDK-THREE-PHASE-PLAN.md` — embeddability and the SDK, in three phases.
- `127-FIELD-REFERENCES-DESIGN.md` — captions, cross-references, and the field-result contract.
- `128-PARAGRAPH-SPANNING-FIELD-DESIGN.md` — complex fields that span paragraphs.
- `129-LINK-TO-PREVIOUS-DESIGN.md` — header/footer Link to Previous, and why it is not shipped yet.
- `130-ONLYOFFICE-TOOLBAR-GAP-ANALYSIS.md` — gap analysis against the ONLYOFFICE toolbar.
- `131-PDF-SEMANTIC-RECONSTRUCTION-AND-BROWSER-OCR-ARCHITECTURE.md` — experimental future-feature architecture for local browser PDF evidence extraction, selective OCR, staged Rust/WASM reconstruction, and semantic export; not implemented or supported.
- `132-EXPERIMENTAL-DOCUMENT-ASSISTANCE-SEMANTIC-SEARCH-AND-MCP-ARCHITECTURE.md` — proposed experimental, use-case-first architecture for document assistance, browser-local semantic retrieval and summarization, reviewable change proposals, and an optional MCP adapter; not implemented or supported.
- `133-REVIEW-PROJECTION-AND-FORMATTING-DESIGN.md` — projecting review markup, and tracked formatting changes.
- `134-TYPED-OMML-MATH-MODEL-AND-RENDERING-DESIGN.md` — the typed OMML math model and its rendering.
- `135-MULTI-FORMAT-IMPORT-EXPORT-ARCHITECTURE.md` — accepted format-neutral detection, adapter registry, preservation sidecar, package substrate, and SDK/WASM migration for DOCX, ODT, normalized JSON, plain text, and later trusted format adapters.
- `136-MODEL-COMPLETENESS-LAYER1-TRACKER.md` — model-completeness (layer 1) tracker.
- `137-DUAL-CHROME-DESIGN.md` — dual chrome: the Ribbon and Toolbar layouts.
- `138-PATHOLOGICAL-COMPLEXITY-AUDIT.md` — audit of pathological (super-linear) complexity.
- `139-VERSION-HISTORY-EDIT-MANAGEMENT-AND-RESTORE-PRD.md` — proposed product requirements for Google-Docs-style durable versions, edit attribution, named history, safe append-only restore, copy/download, and structural version diff.
- `140-VERSION-HISTORY-RESTORE-AND-DIFF-ARCHITECTURE.md` — proposed local-first snapshot/replay, fidelity-complete checkpoint, atomic restore, host storage, retention, and typed version-diff architecture; not implemented.
- `141-GOOGLE-DOCS-TABLE-EXPERIENCE-GAP-AND-DESIGN.md` — table editing measured against Google Docs: gaps and design.
- `142-LIST-AND-NUMBERING-EXPERIENCE-GAP-AND-DESIGN.md` — lists and numbering: gaps and design.
- `143-EMBEDDED-COEDITING-RESEARCH-AND-INTEGRATION-ARCHITECTURE.md` — research and integration architecture for embedded co-editing.
- `144-COEDITING-PHASED-IMPLEMENTATION-PLAN-AND-CHECKLIST.md` — phased plan and checklist for co-editing.
- `145-EXPERIMENTAL-CAPABILITY-PORTFOLIO-ROADMAP-AND-CHECKLIST.md` — roadmap and checklist for the experimental capability portfolio.
- `146-CLIENT-PROOFING-ARCHITECTURE.md` — client-side proofing architecture.
- `147-TRANSACTIONS-AS-THE-ONE-MUTATION-PATH.md` — transactions as the one mutation path: revision chain, derived undo, and the choke-point guard.
- `148-PHONE-LAYOUT-COMPETITIVE-ANALYSIS-AND-DESIGN.md` — the phone layout: competitive analysis and design.
- `149-ONLYOFFICE-HOST-CONFIGURATION-PARITY.md` — ONLYOFFICE host configuration parity, option by option (generated).
- `150-OPERATIONAL-TRANSFORM-OVER-THE-CLOSED-OP-SET.md` — `transform`: rebasing one operation over a concurrent one.
- `151-REFLOW-PAGELESS-LAYOUT-DESIGN.md` — reflow (pageless) layout: the engine half and the shell half.
- `152-COLLABORATION-PROTOCOL-SESSION-AND-IDENTITY.md` — the collaboration protocol, the session state machines, and identity.
- `153-ONLYOFFICE-CAPABILITY-PARITY-MATRIX.md` — ONLYOFFICE capability parity matrix (generated by `tools/opendoc-parity`).
- `154-READING-VIEW-MEASURE-AND-DOCUMENT-FOLDING-COMPETITIVE-ANALYSIS.md` — the reading view's measure, and document folding: competitive analysis.
- `155-DRAWINGML-CHART-MODEL-RENDERING-AND-AUTHORING-DESIGN.md` — DrawingML charts: model, rendering, and authoring (ADR-050).
- `156-PRESENTATION-SUPPORT-AND-SHARED-DRAWING-CORE-DESIGN.md` — presentations (PPTX) and a shared drawing core; design only.
- `157-ONLYOFFICE-LARGE-DOCUMENT-AND-FOLDING-SOURCE-FINDINGS.md` — source findings on ONLYOFFICE's handling of large documents and folding.
- `158-DOCUMENT-COMPARISON-ON-CANVAS-COMPETITIVE-ANALYSIS.md` — how Word, ONLYOFFICE, and Google Docs present a document comparison (feeds ADR-061).
- `159-CHROME-SURFACE-PATTERN-AUDIT.md` — chrome surface patterns: where a control lives (ADR-062).
- `160-IMPORT-LOSS-MEASURED-OVER-A-REAL-CORPUS.md` — import loss measured over a real corpus, on both axes.
- `161-IMPORT-LOSS-QUEUE-CLOSING-THE-MEASURED-ATTRIBUTE-LOSSES.md` — closing the measured attribute losses, and what each fix recovers.
- `162-DEPLOYMENT-CONTAINERS-AND-CONFIGURATION.md` — deployment: container images, compose, and configuration.
- `163-INTEROPERABLE-DOCUMENT-ENCRYPTION-RESEARCH-AND-DESIGN.md` — interoperable document encryption: research and design; not implemented.
- `164-VERSIONS-AND-COMPARISON-WHAT-THE-CATEGORY-SHIPS.md` — versions and comparison: what the category ships, and where ours diverges.
- `165-RESTRICT-EDITING-DEFINITION-AND-PRESENTATION-STUDY.md` — Restrict Editing: what it means, where it lives, and whether it is worth having.
- `166-PAGELESS-SURFACE-AND-MODE-SWITCH-COMPETITIVE-STUDY.md` — the pageless surface and the mode switch, compared with Google Docs' pageless view.
- `167-HTML-EXPORT-FIDELITY.md` — HTML export resolved the way the page is: styles, tables, lists, headers and footers, contents lines, drawings, and charts (ADR-066).
- `168-SLIDE-FIDELITY-TO-DOCUMENT-LEVEL.md` — the presentation work measured against the document engine: what a real deck still loses, what the slide path reuses, and the lanes that close the gap.

Also present (not in the numbered sequence): `PHASE-1A-SEMANTIC-MODELING-TRACKER.md`.

### Consolidation note

The former per-construct schema-v1 design docs (32 and the earlier 38–53 set, sixteen files) were
consolidated into the single `38-SCHEMA-V1-DESIGN-REFERENCE.md`; the originals were
deleted with no information loss. Number 32 is intentionally retired (its content
lives in the "Import architecture" section of doc 38). Numbers 44–66 are now in
use by the later architecture, audit, and rendering-fidelity series listed above.

## Recommended project names

- Product/runtime: **Casual Document Runtime**
- Rust workspace repository: `opendoc`
- Core crate: `casual_doc`
- Public SDK facade: `casual_doc_sdk`
- Native renderer: `casual_doc_renderer`
- Published npm package: `@casualoffice/opendoc-embed` (this read
  `@casualoffice/document-runtime` until `docs/126` phase 3; that name has never
  existed)
- Tauri integration crate: `casual_doc_tauri`

## Core boundary

The engine owns document state, transactions, selection, layout, pagination, hit testing, and serialization.

Host applications own windows, menus, dialogs, authentication, storage policy, network policy, telemetry, and product-specific UI.

## Non-negotiable constraints

- Native-first, WebAssembly-compatible architecture.
- No browser DOM as the source of truth.
- Deterministic behavior for the same document, fonts, viewport, and engine version.
- No mandatory server dependency.
- No mandatory React dependency.
- No mandatory collaboration provider.
- Loss-aware DOCX round-trip behavior.
- Stable, versioned public API.
