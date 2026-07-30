# 71 — History Labels, Formatting State, and Paragraph Inspector

Status: accepted implementation design
Date: 2026-07-30
Related: docs 58, 59, 63, 67, and 70

## 1. Scope

This increment closes the first three items in the merged-main editor UX plan:

1. engine-owned history metadata and user-facing Undo/Redo labels;
2. correct run-format toggle/value reflection, including mixed selections;
3. a live paragraph-properties inspector using the same shell pattern as tables.

The model remains the source of truth. The browser does not infer history,
formatting, or paragraph state from toolbar values or rendered DOM.

## 2. History metadata

Each undo/redo stack item is a `HistoryEntry` containing:

- the inverse operation group for one user action; and
- a bounded, engine-owned action label.

Labels describe semantic user actions, not internal operation variants. The
initial stable vocabulary includes Typing, Paste, Delete, Replace, Paragraph break,
Formatting, Paragraph formatting, Link change, Table resize, Table formatting,
Table structure, Document properties, and Page setup. A host may choose a
closed semantic kind for a command that shares a lower-level operation group
(plain paste versus paragraph break), but it cannot inject arbitrary UI text.

Undo moves the entry label to redo with the generated forward operation group;
redo moves it back to undo. Typing coalescing merges only operation groups and
keeps the original Typing label. Public `undoLabel` / `redoLabel` getters return
the next action label or an empty string. The editor renders `Undo <label>` and
`Redo <label>` in `title` and `aria-label`, while disabled empty stacks retain
plain Undo/Redo labels.

## 3. Formatting state

Boolean character formatting uses an explicit three-state query:

- `off`: every covered effective run is off;
- `on`: every covered effective run is on;
- `mixed`: the covered effective runs disagree.

Toolbar toggle buttons expose this through `aria-pressed=false|true|mixed`.
Activating an off or mixed toggle applies it to the full selection; activating
an on toggle clears it.

Value formatting returns both a value and an explicit mixed flag:

- font family;
- font size;
- text color;
- highlight;
- vertical alignment.

An absent authored highlight reflects as `none`; disagreement reflects as
mixed. Superscript and subscript are mutually exclusive and activating the
currently uniform value applies `baseline`. At a collapsed caret the pending
typing format overrides the inherited effective value.

Font size is an editable combobox with common values as suggestions. It accepts
finite half-point values from 1 through 1638 points, rejects invalid input
without mutating the document, and normalizes the displayed value after commit.

## 4. Paragraph inspector

The paragraph-options toolbar button opens a right-side `Panel`, not a popover.
It is mutually exclusive with the table inspector and uses the shared inset,
rounded, bordered surface on desktop and the shared viewport-inset drawer on
narrow screens.

The inspector reflects every paragraph touched by the model selection:

- uniform numeric values are shown normally;
- mixed numeric/select values are blank with a Mixed placeholder;
- mixed booleans use the native checkbox indeterminate state;
- mixed shading and border values are explicitly identified and no value is
  invented.

Changes apply live:

- selects, checkboxes, color choices, and border presets commit on activation;
- numeric values commit on `change` (blur or Enter);
- each completed control interaction is one undoable Paragraph formatting
  action;
- no Apply or Reset controls are present; Undo is recovery.

Opening the panel focuses its first control. Escape/Close returns focus to the
trigger. Editing inside the panel preserves the model selection. The existing
line-and-paragraph-spacing control remains a compact ribbon popover.

## 5. Verification

Required gates:

- engine tests for label movement/coalescing and mixed formatting/paragraph
  state;
- browser tests for dynamic Undo/Redo labels, mixed/toggle formatting,
  arbitrary font sizes, live paragraph commits, focus restoration, and
  narrow-screen panel bounds;
- visual inspection in light and dark editor modes;
- formatting, strict Clippy, workspace tests, web unit tests, WASM build,
  Playwright suite, and `git diff --check`.
