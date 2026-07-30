# Editor UX Gap Analysis

Status: Active Phase 1G shell/editor plan; merged-main audit refreshed
Date: 2026-07-30
Scope: web editor shell, canvas editing, selection, command discoverability, and Word-like editing behavior. Table-editing implementation is tracked separately in `P1G-TABLE-COMPLETE-001`.

## Reference Baseline

The target is a serious document editor, closer to Word desktop / Microsoft 365 than to a lightweight text box. Microsoft’s current public Word guidance confirms the baseline behaviors we should match where the OpenDoc model supports them: table selection and resizing, table properties, header rows, AutoFit/fixed-width behavior, borders/shading, and row/column/cell operations. Google Docs remains a useful interaction baseline for browser expectations: lightweight contextual toolbars, low-latency selection, non-blocking menus, and predictable keyboard handling.

Reference pages:

- Microsoft Support, "Resize a table, column, or row"
- Microsoft Support, "Set or change table properties"
- Microsoft Learn notes that non-uniform Word tables require selection APIs for rows/columns, which matches our decision to avoid pretending merged tables are regular grids.

## Priority Rules

P0 means an editing session can feel broken, unsafe, or lossy. P1 means a common Word/Docs workflow is missing or too slow for daily use. P2 means important parity, but not a blocker for basic document editing. P3 means polish, depth, or long-tail power behavior.

## Gap Table

| Area | Priority | Current state | Expected Word/Docs-class behavior | Required work | Verification gate |
| --- | --- | --- | --- | --- | --- |
| Focus ownership and stale gestures | P0 | Recent fixes reset pointer-cancel/lost-capture and keep the page surface focusable. | Canvas never "freezes" the caret after canceled drags, toolbar clicks, modal/chip interactions, or page-gap movement. | Add a permanent Playwright smoke suite for pointercancel, blur, hidden-tab, toolbar click-return, and canvas click/type. | Browser smoke in CI with no console errors and caret visible after each recovery path. |
| Edit hot loop | P0 | Chrome refresh is coalesced; color/shading controls commit on `change`; table column drag previews locally and commits once. | Typing, color picking, shading, table drag, and toolbar formatting stay responsive without repeated full stats/outline/reflow work. | Add frame-budget instrumentation around typing, shading, and table drag; fail on repeated sync chrome refresh in hot paths. | Perf smoke with max edit latency and repaint count thresholds. |
| Undo/redo user model | P0 | Typing bursts, multiline paste, IME commits, replace-with-Enter, formatting, and table drag/property interactions now use user-action history boundaries; buttons reflect real stack availability. | Every visible command is undoable as one user action; drag preview produces one undo entry; command names should be explainable. | Audit the remaining compound commands and add command metadata for user-facing Undo/Redo labels. | Unit tests for transaction count plus browser smoke for undo after drag, paste, shading, merge/split. |
| Native clipboard fidelity | P0 | Plain-text native event fallback exists. | Copy/cut/paste preserves rich runs, paragraphs, lists, tables, links, and HTML where safe; plain text remains fallback. | Add internal rich clipboard payload and sanitized HTML import/export bridge. | Browser clipboard-event tests for formatted text, table cells, links, lists, and plain fallback. |
| IME live preedit | P0 | Final composition commit exists; live preedit is not painted. | CJK/Indic IME shows active composition text/caret and does not commit intermediate keystrokes. | Add host preedit overlay tied to caret rect and compositionupdate. | Browser composition smoke for interim display, cancel, commit, undo. |
| Cross-structure delete/selection | P0 | Same-paragraph and some range edits exist; broad structural delete is incomplete. | Delete/backspace across paragraphs, tables, lists, links, fields, and sections follows Word-like safe structural rules with no silent data loss. | Define structural deletion policy and implement transactional range delete across block boundaries. | Corpus unit tests for paragraph join, table boundary, list join, hyperlink partial selection, protected unsupported nodes. |
| Selection ergonomics | P1 | Drag selection, page gaps, word/paragraph select, table range overlays exist. | Shift-click, drag, double/triple click, keyboard selection, table handles, and scroll extension behave predictably. | Add keyboard selection expansion by word/paragraph/document; add visible page-edge auto-scroll feedback; add table row/column gutter affordances. | Browser smokes for keyboard selection and multi-page drag. |
| Command discoverability | P1 | Ribbon, contextual popovers, command palette, and one model-aware right-click/Shift+F10 menu exist for prose, links, lists, tables, comments, and suggestions. Palette/menu common actions share descriptors and disabled reasons. | Word-like commands are discoverable via ribbon, context menu, keyboard shortcuts, command search, and selection toolbar. | Migrate the remaining ribbon and shortcut bindings to the shared descriptors and fill the remaining palette coverage. | Snapshot test of command registry and browser smoke for palette/context invocation. |
| Keyboard shortcuts | P1 | Basic formatting and find shortcuts exist; table Tab handling exists. | Common Word shortcuts work: save/export, undo/redo, bold/italic/underline, find/replace, select all, copy/cut/paste, links, headings, lists, indent, page break, non-breaking space, table navigation. | Centralize shortcut registry with platform-specific labels and conflict handling. | Unit test registry plus browser shortcut smoke. |
| Paragraph formatting parity | P1 | Alignment, spacing, indent, tabs, shading, borders, keep flags exist. | Ruler and dialogs cover left/right/hanging/first indent, tabs, spacing, borders/shading, page break controls with reflectable state. | Add richer tab leader UI, style-linked defaults display, and reset-to-style controls. | WASM reflection tests and browser menu state tests. |
| Lists and numbering UX | P1 | Toggle list basics exist. | Bullets/numbering/multilevel lists expose gallery, indent levels, restart/continue, and style-aware numbering. | Build list gallery and level controls on top of numbering definitions. | DOCX round-trip tests and browser list workflow smoke. |
| Styles UX | P1 | Font family/paragraph style primitives exist. | Style gallery, apply style, update style from selection, clear formatting, and style preview. | Add style catalog reflection and command surface; defer style editing if model support is incomplete. | Style apply/undo/export/reopen tests. |
| Page/layout controls | P1 | Rendering honors sections; editor shell exposes limited layout controls. | Margins, orientation, page size, columns, breaks, headers/footers entry points are discoverable and transactional. | Add layout property command set and shell controls after section edit policy is locked. | Section property round-trip and pagination regression tests. |
| Link/bookmark UX | P1 | Link authoring/activation and TOC navigation exist. | Insert/edit/remove links, bookmarks, cross references, and safe activation are discoverable without selection loss. | Add bookmark manager and edit-link affordance in link chip. | Browser tests for edit/remove/bookmark jump. |
| Comments/revisions | P1 | Not a current editor workflow. | Word-like review workflows: comments, tracked changes display, accept/reject, author metadata policy owned by host. | Design review data model and host policy first; do not fake tracked-changes support. | Dedicated ADR/design doc before implementation. |
| Find/replace depth | P1 | Text find/replace panel exists. | Replace all, formatting-aware search, whole word, wildcards/basic regex, selection-only scope. | Extend search query options and transaction grouping. | Unit tests over body/table/header/footer scopes. |
| Tables | P1 | Dedicated table PR now covers context, selection overlays, merge/split, sizing, properties, header rows, margins/spacing. | Word-like table operations: select row/column/table, insert/delete, merge/split, resize, AutoFit/fixed, header row, borders/shading, margins/spacing. | Remaining long-tail after table PR: distribute rows/columns, split-cell dialog with arbitrary counts, table style gallery, sort, formulas, caption/alt text UI. | Table PR gates plus future long-tail issue list. |
| Floating/contextual toolbar | P2 | Selection toolbar exists for basic formatting. | Toolbar appears near selection, stays bounded, avoids covering caret/content, and exposes high-frequency actions. | Add collision-aware placement and command registry integration. | Browser screenshots at desktop/mobile and edge selections. |
| Accessibility | P2 | Canvas shell has limited ARIA affordance. | Keyboard users can reach commands; screen readers get document structure/status even though source of truth is model, not DOM. | Add accessible command surface, status announcements, and model-derived outline/navigation semantics. | Axe/static checks plus manual keyboard smoke. |
| Mobile/tablet editing | P2 | Desktop-first. | Touch selection handles, keyboard-safe viewport, touch table handles, toolbar compaction. | Separate touch interaction design and viewport tests. | Mobile Playwright screenshots and touch event smokes. |
| Collaboration/editor presence | P2 | Runtime is host-owned; no mandatory provider. | If enabled by host, remote cursors/selections and conflict-safe commands sit above transactions. | Design host extension seam; keep runtime provider-neutral. | Contract tests with fake host provider. |
| Error/reporting UX | P2 | Some unsupported edits are ignored with console warnings. | Unsupported commands show bounded user-facing reasons and never silently do nothing. | Add structured edit errors and disabled-command explanations. | Unit tests for error codes; browser smoke for disabled reason. |
| Performance observability | P2 | Manual browser smokes catch regressions. | Editor exposes debug/perf counters for repaint, pagination, command time, and cache hits. | Add internal telemetry hooks owned by host; no network dependency. | Perf snapshots in CI artifacts. |

## 2026-07-30 merged-main editing audit

This pass inspected and exercised the merged `main` rather than relying on the
earlier planning table. Focus recovery, rich run/paragraph/link clipboard,
IME preedit, structural deletion across a table boundary, the contextual table
ribbon, and the right-side table inspector now have permanent browser coverage.
They are no longer accurately described as wholly missing above; the remaining
scope in those rows is their documented long tail.

The following daily-driver defects remain the highest-priority work:

| Order | Gap | Observed behavior | Required behavior |
| --- | --- | --- | --- |
| 1 | Undo transaction grouping | Fixed by `P1G-EDIT-CORRECTNESS-005`: rapid adjacent plain/styled typing coalesces with guarded host sessions; multiline paste, composition commits, and Enter over a selection are atomic. | Audit remaining compound commands and add user-facing history labels. |
| 2 | Selection collapse and platform navigation | Plain Left/Right now collapses forward/backward ranges to their engine-ordered edge. Ctrl and Command still share one mapping, producing incorrect Windows word/paragraph movement. | Centralize macOS/Windows keymaps; add word deletion and Page Up/Down. |
| 3 | History reflection | Fixed by `P1G-EDIT-CORRECTNESS-005`: `canUndo`/`canRedo` are engine truth and drive the buttons. | Add a user-facing command label when history metadata lands. |
| 4 | Formatting semantics | Superscript/subscript set a value but do not toggle back to baseline; highlight has no reflected value; mixed paragraph/run selections are incompletely represented. | Add tri-state/mixed reflection, baseline toggles, and uniform state across all selected paragraphs. |
| 5 | Lists | Enter cannot terminate a list, Tab changes paragraph indent rather than list level, and restart/continue/multilevel/checklist authoring are absent. | Implement complete list lifecycle before adding more gallery chrome. |
| 6 | Paragraph property surface | The tall anchored form obscures most of a narrow document and uses menu semantics around form controls. | Move low-frequency paragraph properties into the shared live right inspector; retain only a compact spacing popover. |
| 7 | Clipboard structure | Rich clipboard preserves paragraphs, runs, and links; tables, list definitions, images, and most external paragraph formatting flatten. | Add bounded structured fragments and richer sanitized HTML mapping. |
| 8 | Command/shortcut coverage | The palette and context menu now share stable common descriptors and disabled reasons; many ribbon/shortcut bindings are still direct listeners, and Command/Ctrl+K conflicts with conventional link authoring. | Complete the migration to one registry for ribbon, palette, shortcuts, disabled reasons, and context menus. |
| 9 | Accessibility and touch | The canvas has limited model-derived accessibility semantics; touch selection handles and keyboard-safe mobile editing are absent. | Design a model-backed accessibility surface and a separate touch interaction layer. |
| 10 | Editing scope | Body/table-cell paragraph text is editable; headers/footers, notes, text boxes, and objects are not general editing surfaces. | Add each surface through commands/transactions without making the browser DOM authoritative. |

Effective formatting reflection was fixed during this audit: font, size, and
run toggles now resolve document defaults, paragraph and character styles, and
direct formatting. Theme font references resolve to their authored family;
undeclared text reflects the engine default (Roboto); imported families can
appear in the font control. Physical font substitution and glyph coverage
fallback remain renderer diagnostics and are not written back as authored DOCX
formatting.

The table-properties inspector was also changed from a draft dialog model to a
live inspector. Toggle/segmented/select controls commit on change, numeric fields
commit on blur/Enter, and each completed control interaction is one undo action.
Apply and Reset are removed; Undo is the recovery mechanism.

Editing correctness was then tightened at the same command boundary. Plain-text
paste/composition/replace-with-break is one atomic engine group; rapid adjacent
typing coalesces only with both a matching host gesture id and exact caret
continuity; plain Left/Right collapses a range to an engine-ordered edge; and
Undo/Redo availability comes from the actual history stacks.

The keyboard follow-up now uses an explicit platform map: Option performs word
movement/deletion on macOS, Ctrl does so on Windows/Linux, Command/Ctrl plus
Up/Down moves by paragraph, Command/Ctrl plus Home/End moves to document bounds,
and Page Up/Down resolves one viewport-height away through model hit-testing.
It also makes both side inspectors inset rounded surfaces using the
shared border/radius/elevation tokens; responsive layout may change their
docking, but not their visual component grammar.

## Revised execution plan

1. Add history metadata and user-facing undo/redo labels. Transaction grouping,
   platform navigation/deletion, Page Up/Down, horizontal range collapse, and
   real history availability are complete.
2. Formatting semantics: super/sub baseline toggle, highlight reflection, mixed
   selection state, and arbitrary font-size entry.
3. Contextual properties: move paragraph properties into the same live inspector
   system as tables.
4. Lists/styles: complete list lifecycle, style gallery/reset-to-style, clear
   formatting, and copy/paste formatting.
5. Command registry: unify palette, shortcuts, menus, contextual commands, and
   disabled reasons. Palette/context descriptors and the complete accessible
   editor context menu are complete in doc 84; ribbon/shortcut migration remains.
6. Structured clipboard, accessibility/touch, and additional editing surfaces.

## Open Risks

- Word parity is large. The project should define "Word-like" as high-confidence behavior for common editing paths, not every long-tail Word feature in one phase.
- Non-uniform tables must remain conservative. Some row/column operations are invalid unless the selection maps to a real rectangular grid.
- Browser clipboard and IME behavior varies by OS/browser; the CI smoke suite must include synthetic event coverage and at least one real-browser local/manual checklist.
- The runtime must keep the model as source of truth. Accessibility and rich clipboard bridges cannot become hidden DOM editors.
