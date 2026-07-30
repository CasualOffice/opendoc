# Bookmark manager surface

The first bookmark-management slice is read-only and navigation-focused. The
runtime lists authored bookmark names and resolves each to a model caret; the
command palette presents the names and jumps to the selected bookmark. This
avoids inventing a new bookmark mutation format while making internal links
discoverable. Creating, renaming, and deleting bookmarks remain deferred until
the host can expose a range-aware bookmark editor.
