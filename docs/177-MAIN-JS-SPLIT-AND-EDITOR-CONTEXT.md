# 177 — Splitting `main.js`: feature modules over one editor context

**Status:** In progress, 2026-10-24. **Row:** `docs/109` HF-380 (progresses HF-085 and
HF-109). **Guard:** `webapp/tests/module_seams.test.mjs`.

---

## 1. The problem

`webapp/src/main.js` was 16,030 lines with zero exports. Every parallel UI change met
every other one in that file, and the line ratchet in `module_seams.test.mjs` only stopped
it growing; it did not take anything out. SKILL §10 asks for a recorded exception above
about 2,000 lines, and SKILL §11 records the consequence for embedding: the file binds
its DOM at import, so there is no mount seam (HF-109).

## 2. The pattern: a composition root with an explicit context

The named pattern is **dependency injection through a composition root**. `main.js`
keeps the bindings it still owns and builds one object, `ed` (the *editor context*),
whose entries are live getters (and, where a module assigns, setters) over those
bindings. Each feature domain is a module exporting one factory, `createX(ed)`:

```js
// main.js
const { openFind, findSession, … } = createFind(ed);
// editor_find.mjs
export function createFind(ed) { …the moved code… return { openFind, findSession, … }; }
```

Rules the split keeps:

- **Behaviour-preserving.** Code is moved, not rewritten. The only textual change in
  moved code is that a reference to a binding `main.js` still owns, `foo`, reads
  `ed.foo`. Imports the moved code used are imported by the module itself.
- **Same evaluation order.** The factory call sits where the first moved line sat, so
  the DOM lookups, listener registrations and state the domain sets up run at the same
  point of boot as before.
- **Live, lazy context.** `ed`'s entries are getters, so a module reads a value at call
  time exactly as the code did when it shared the file. A binding still in its temporal
  dead zone throws at the same moment it would have.
- **Generated, not hand-kept.** The `ed` block between `// editor-context:begin` and
  `// editor-context:end` lists exactly the names the modules reach. A test asserts that
  every `ed.name` a module reads has a getter and every one it writes has a setter.
- **One way.** No module imports `main.js`; modules reach each other only through `ed`.
- **Checks read all of it.** A check that reads the editor page's code to say what it
  does (the capabilities the chrome consults, `build-embed-docs.mjs`) reads
  `webapp/tools/editor_sources.mjs`'s `editorSource()`: `main.js` plus every
  `editor_*.mjs`. A domain leaving `main.js` must not shrink what such a check covers.
- **String debt moves, it is not forgiven.** `no_unrouted_strings.test.mjs` gives each
  domain module its measured count, and lowers `main.js` by the same amount.

## 3. Module map

Sizes are `wc -l` at the commit that last touched the row.

| Module | Factory | Domain | Lines |
|---|---|---|---|
| `editor_caret_nav.mjs` | `createCaretNav` | Caret navigation: the vertical goal column and `navCaret` | 90 |
| `editor_clipboard.mjs` | `createClipboard` | Cut, paste (plain, rich, tracked, HTML), Paste as text, Paste options | 525 |
| `editor_context_menu.mjs` | `createContextMenu` | The context-aware editor menu: what a right-click offers | 643 |
| `editor_dialogs.mjs` | `createDialogs` | Shortcuts reference, About, bookmarks, Insert field / picture, captions and cross-references, Symbol / Emoji | 470 |
| `editor_dirty.mjs` | `createDirtyTracking` | Unsaved-work tracking: saved and current revisions, the dirty answer | 134 |
| `editor_drafts.mjs` | `createDrafts` | Autosave drafts, crash recovery and the recovery offer | 642 |
| `editor_edit_pipeline.mjs` | `createEditPipeline` | The edit choke point: `runEdit`, `applyEditResult`, typing sessions, mode gates | 202 |
| `editor_file_open.mjs` | `createFileOpen` | Boot, demo presets, a blank document, `openBytes`, and the header rename | 420 |
| `editor_file_pick.mjs` | `createFilePick` | Opening a picked or dropped file: size ceiling and handleFile | 67 |
| `editor_find.mjs` | `createFind` | Find and replace panel, match scan, Replace / Replace all, the Find session | 318 |
| `editor_format_painter.mjs` | `createFormatPainter` | Format painter, font size, indents, line break, checklist toggle | 348 |
| `editor_formatting.mjs` | `createFormatting` | Formatting toolbar state: run format, font and underline faces, toggles, table styles | 299 |
| `editor_header_footer.mjs` | `createHeaderFooter` | Header and footer editing: running bands, markers, notes | 478 |
| `editor_keyboard.mjs` | `createKeyboard` | The editor keydown dispatch | 538 |
| `editor_link_dialog.mjs` | `createLinkDialog` | Insert / edit link dialog | 218 |
| `editor_menu_render.mjs` | `createMenuRender` | Menu rendering: the root context menu and submenu flyouts | 335 |
| `editor_object_moves.mjs` | `createObjectMoves` | Alt text, moving and nudging objects, snap guides, object text | 377 |
| `editor_objects.mjs` | `createObjects` | Objects: selection, crop, resize, rotate, arrange, inspector, groups | 564 |
| `editor_outline.mjs` | `createOutline` | The Outline panel and the flanking-panel toggles | 81 |
| `editor_overlay_paint.mjs` | `createOverlayPaint` | Checklist markers, selection highlight, IME pre-edit (place stays in main.js) | 124 |
| `editor_paint.mjs` | `createPaint` | Page virtualization and painting (renderAll stays in main.js) | 232 |
| `editor_palette.mjs` | `createPalette` | Command palette and `editorCommands()`, the registry of editor actions | 652 |
| `editor_palette_ui.mjs` | `createPaletteUi` | The palette's surface: filtered list, row selection, run by row or id, its keyboard | 222 |
| `editor_pointer.mjs` | `createPointer` | Pointer input, links and the link chip, table chrome, hover, touch, drag selection, copy | 948 |
| `editor_proofing.mjs` | `createProofing` | Proofing chrome, the overlay layer and drawSelection | 191 |
| `editor_review_decisions.mjs` | `createReviewDecisions` | Review decisions: inline accept/reject card, Next/Previous over comments and changes, decisions at the caret | 539 |
| `editor_save.mjs` | `createSave` | Save and export | 53 |
| `editor_selection.mjs` | `createSelection` | Selection overlays, anchors, review markers, point-to-twip (scaleOf stays in main.js) | 341 |
| `editor_selection_commands.mjs` | `createSelectionCommands` | Caret to a position, Page Up / Page Down, Select all | 97 |
| `editor_selection_toolbar.mjs` | `createSelectionToolbar` | The floating selection toolbar | 73 |
| `editor_settings.mjs` | `createSettings` | Settings dialog: theme, accent, reviewer identity, autosave and proofing switches; localisation start | 205 |
| `editor_text_input.mjs` | `createTextInput` | Soft keyboards and dictation; AutoCorrect as you type | 110 |
| `editor_versions.mjs` | `createVersions` | Version history: name prompt, read-only preview, timeline wiring | 146 |
| `editor_zoom.mjs` | `createZoom` | Zoom: editable %, Fit width / Fit page, Ctrl+scroll (setZoom stays in main.js) | 120 |

`main.js`: 16,030 lines when the split began, 6,311 now. The domain modules hold 10,802.

## 4. What is left for HF-109

The factories make the seam, not the mount. Each one still looks its DOM up by fixed id
inside `document`, and `main.js` still runs at import. The next step is to give `ed` a
`root` and have the factories look up `ed.root.querySelector(…)`, then wrap the
composition root in `mountEditor(root, options)`. Recorded as open in HF-380.
