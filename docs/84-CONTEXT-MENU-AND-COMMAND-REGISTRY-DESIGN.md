# 84 — Context Menu and Shared Command Descriptors

Status: accepted implementation design

Date: 2026-07-31
Depends on: docs 58, 63, 67, 68, 70, 81, and 83

## Problem

The web editor has a table-only right-click menu assembled from a private array.
Prose, selections, links, lists, comments, and suggestions fall through to the
browser menu. The command palette separately assembles another action list, so
labels, availability, and invocation behavior can drift. The table menu also
moves the caret on every invocation, has no keyboard entry point, and implements
only pointer hover rather than an accessible menu interaction.

## Decisions

1. `editorCommands(context)` is the web host's shared descriptor source for the
   command palette and context menu. A descriptor has a stable id, label, group,
   search keywords, optional shortcut, visibility, availability, disabled
   reason, and one invocation callback. Existing ribbon handlers may migrate to
   the same source incrementally; this slice does not rewrite working ribbon
   controls solely for architectural symmetry.
2. The context menu is one reusable `role="menu"` surface. Its contents are
   derived from the model context at invocation time:
   - selection/caret: clipboard, Select all, link/comment, paragraph/list;
   - link: edit and remove the exact resolved link range;
   - comment or tracked change: open the comment, or accept/reject the exact
     revision, editor group, or move pair;
   - table cell: selection, structure, distribute/sort, cell/table properties,
     merge/split, and destructive actions.
3. A pointer invocation inside the current painted selection preserves that
   selection. A pointer invocation outside it collapses to the hit-tested anchor
   before building context. `Shift+F10` and the Menu key open at the current
   caret/selection geometry and never change the model selection.
4. Commands that are unsafe for the current state remain visible only when that
   visibility teaches useful context, and carry an explicit disabled reason.
   Examples are Copy without a range and column operations on a merged table.
   Mutating table/list/paragraph commands are unavailable in Suggesting mode
   until those structures have a tracked representation.
5. Review UI remains in the dedicated sibling sidebar. The context menu only
   invokes review actions or focuses the corresponding sidebar card; it does not
   create a canvas card, popover, superscript, or second review panel.
6. Menu focus uses roving `tabindex`: Arrow Up/Down, Home/End, Enter/Space, and
   Escape. Escape and command execution restore focus to the editor surface.
   Pointer dismissal respects the newly clicked target. Placement is clamped to
   an 8px viewport inset and recomputed after the menu is measured.
7. All mutations continue through existing WASM command/transaction entry
   points. The menu owns no document state and performs no direct model edits.

## Verification

- Pure unit tests cover menu grouping, separators, enabled-index navigation, and
  viewport placement.
- Browser tests cover prose and table invocation, selection preservation,
  keyboard invocation/focus return, disabled reasons, viewport collision, and
  light/dark theme surfaces.
- Existing clipboard, list, link, table, review, and focus-recovery suites remain
  green.

## Deferred

- Migrating every ribbon and shortcut listener to descriptor lookup is the next
  command-routing increment, together with structured command error codes.
- Native spelling/grammar and browser extension entries cannot be reproduced in
  a custom canvas menu. A future host policy may expose a deliberate native-menu
  escape gesture if required.
