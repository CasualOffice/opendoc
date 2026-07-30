# Link chip editing

The link chip now exposes Edit and Remove for external links. Edit reuses the
existing same-paragraph `setHyperlink` command with the authored target and
tooltip; Remove uses the exact resolved range and preserves linked text and
formatting. Internal bookmark links remain Jump-only until a bookmark manager
exists. Both mutations remain command-backed and undoable.
