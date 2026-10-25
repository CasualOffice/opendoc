# Completed editor interaction slices

These are the original short implementation notes formerly numbered 77–79.
They record the scope of those completed slices; the current code and tests
remain the authority for present behavior.

## Link chip editing (former 77)

The link chip exposes Edit and Remove for external links. Edit reuses the
same-paragraph `setHyperlink` command with the authored target and tooltip;
Remove uses the exact resolved range and preserves linked text and formatting.
Internal bookmark links were Jump-only until a bookmark manager existed. Both
mutations are command-backed and undoable.

## Bookmark navigation (former 78)

The first bookmark-management slice was read-only and navigation-focused. The
runtime listed authored bookmark names and resolved each to a model caret; the
command palette presented the names and jumped to the selected bookmark. This
made internal links discoverable without inventing a new mutation format.
Creating, renaming, and deleting bookmarks were deferred until a range-aware
bookmark editor could be exposed.

## Save shortcut (former 79)

`Ctrl+S` on Windows/Linux and `Cmd+S` on macOS invoked the existing
user-initiated DOCX export/download path. The browser's default save-page
behavior was prevented only with an open document; without one, the shortcut
was left untouched. Export errors remained visible in the status bar.
