# Table sorting

Table sorting is a deterministic, undoable row reorder over the active table.
It sorts by the first cell's plain text using Unicode case-folded lexical order;
`ascending` and `descending` are the only modes. A repeated-header row remains
at the top and is not moved. Sorting is refused for merged/spanned tables or
cells whose first block is not a paragraph, avoiding ambiguous structural
reordering. The complete table is replaced in one `ReplaceTable` transaction,
so content, formatting, and row properties move together and undo restores the
original order exactly.
