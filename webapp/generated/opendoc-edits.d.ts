// SPDX-License-Identifier: Apache-2.0
// GENERATED from the capability registry (crates/casual-doc-command, docs/181 §6). Do not edit.
// Regenerate: cargo run -p casual-doc-command --bin opendoc-command-gen

/** The edit vocabulary version (docs/181 §4.1): a minor bump adds commands; a major bump changes or removes one. */
export declare const VOCABULARY_VERSION: "1.1";

/**
 * Insert text at an anchor.
 *
 * Family `text`; trackable: yes.
 *
 * @example {"op":"insert_text","at":{"node":"1:2","exact":"days","side":"after"},"text":" (calendar)"}
 */
export interface InsertTextEdit {
  op: "insert_text";
  /** Where. */
  at: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
  /** What. */
  text: string;
}

/**
 * Replace the quoted text with new text.
 *
 * Family `text`; trackable: yes.
 *
 * @example {"op":"replace_text","target":{"node":"1:2","exact":"thirty (30) days"},"text":"sixty (60) days"}
 */
export interface ReplaceTextEdit {
  op: "replace_text";
  /** The text to replace. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
  /** Its replacement. */
  text: string;
}

/**
 * Delete the quoted text.
 *
 * Family `text`; trackable: yes.
 *
 * @example {"op":"delete_text","target":{"node":"1:2","exact":" without notice"}}
 */
export interface DeleteTextEdit {
  op: "delete_text";
  /** The text to delete. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Insert new paragraphs after an anchor's paragraph.
 *
 * Family `paragraph`; trackable: yes.
 *
 * @example {"op":"insert_paragraphs","after":{"node":"1:2"},"paragraphs":[{"text":"Termination","style":"Heading 2"}]}
 */
export interface InsertParagraphsEdit {
  op: "insert_paragraphs";
  /** The paragraph they follow. */
  after: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
  /** The new paragraphs, in order. */
  paragraphs: Array<{
    /** A paragraph style, by name (for example `Heading 2`) or id. */
    style?: string | null;
    /** Its text. */
    text?: string;
  }>;
}

/**
 * Apply a paragraph style, by name or id.
 *
 * Family `styles`; trackable: yes.
 *
 * @example {"op":"apply_paragraph_style","target":{"node":"1:2"},"style":"Heading 1"}
 */
export interface ApplyParagraphStyleEdit {
  op: "apply_paragraph_style";
  /** The style, by name or id. */
  style: string;
  /** The paragraph. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Set bold, italic, underline, strike, size, colour, highlight or font on the quoted text.
 *
 * Family `text`; trackable: yes.
 *
 * @example {"op":"set_text_format","target":{"node":"1:2","exact":"Notice"},"format":{"bold":true}}
 */
export interface SetTextFormatEdit {
  op: "set_text_format";
  /** What to set. */
  format: {
    /** Bold on or off. */
    bold?: boolean | null;
    /** Colour, `#RRGGBB`. */
    color?: string | null;
    /** Font family. */
    font?: string | null;
    /** Highlight name (`yellow`, `green`, `cyan`, `magenta`, `blue`, `red`, …, `none`). */
    highlight?: string | null;
    /** Italic on or off. */
    italic?: boolean | null;
    /** Size in points (half-point precision), 1 to 1638. */
    size?: number | null;
    /** Strikethrough on or off. */
    strike?: boolean | null;
    /** Underline on or off. */
    underline?: boolean | null;
  };
  /** The text. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Remove direct character formatting from the quoted text; the paragraph style stays.
 *
 * Family `text`; trackable: no. Clearing formatting cannot be suggested yet: a suggested format change is recorded as the properties it sets, and clearing sets none. Use set_text_format with explicit values, or mode `direct`.
 *
 * @example {"op":"clear_formatting","target":{"node":"1:2","exact":"notice period"}}
 */
export interface ClearFormattingEdit {
  op: "clear_formatting";
  /** The text. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Rewrite one paragraph from read_blocks Markdown; only the words that differ change.
 *
 * Family `paragraph`; trackable: yes.
 *
 * @example {"op":"rewrite_block","target":{"node":"1:2"},"markdown":"Notice: the notice period is sixty (60) days without notice."}
 */
export interface RewriteBlockEdit {
  op: "rewrite_block";
  /**
   * The paragraph's new content: an optional `{#id}` prefix (it must
   * name `target`), an optional `#`..`######` heading marker, then the
   * text, on one line.
   */
  markdown: string;
  /** The paragraph (its `node`; a quote is not used). */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Align a paragraph: start, center, end or justify.
 *
 * Family `paragraph`; trackable: yes.
 *
 * @example {"op":"set_alignment","target":{"node":"1:2"},"alignment":"center"}
 */
export interface SetAlignmentEdit {
  op: "set_alignment";
  /** The alignment. */
  alignment: "start" | "center" | "end" | "justify";
  /** The paragraph. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Set a paragraph's line spacing: a multiple of single, or an exact or minimum height in twips.
 *
 * Family `paragraph`; trackable: yes.
 *
 * @example {"op":"set_line_spacing","target":{"node":"1:2"},"rule":"multiple","value":150}
 */
export interface SetLineSpacingEdit {
  op: "set_line_spacing";
  /** How `value` is read. */
  rule: "multiple" | "exact" | "at_least";
  /** The paragraph. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
  /** Percent of single for `multiple`; twips for `exact` and `at_least`. */
  value: number;
}

/**
 * Set the space before and/or after a paragraph, in twips.
 *
 * Family `paragraph`; trackable: yes.
 *
 * @example {"op":"set_paragraph_spacing","target":{"node":"1:2"},"before":240,"after":120}
 */
export interface SetParagraphSpacingEdit {
  op: "set_paragraph_spacing";
  /** Twips after; negative clears it back to the style's. */
  after?: number | null;
  /** Twips before; negative clears it back to the style's. */
  before?: number | null;
  /** The paragraph. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Set a paragraph's start, end or first-line indent, in twips (a negative first line hangs).
 *
 * Family `paragraph`; trackable: yes.
 *
 * @example {"op":"set_indent","target":{"node":"1:2"},"start":720,"first_line":-360}
 */
export interface SetIndentEdit {
  op: "set_indent";
  /** End (right) indent in twips, at least 0. */
  end?: number | null;
  /**
   * First-line indent in twips: positive indents, negative hangs, 0
   * clears both.
   */
  first_line?: number | null;
  /** Start (left) indent in twips, at least 0. */
  start?: number | null;
  /** The paragraph. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Set the text of one table cell.
 *
 * Family `tables`; trackable: yes.
 *
 * @example {"op":"set_cell_text","table":"1:9","row":1,"column":0,"text":"Q3"}
 */
export interface SetCellTextEdit {
  op: "set_cell_text";
  /** 0-based cell in that row. */
  column: number;
  /** 0-based row. */
  row: number;
  /** The table id. */
  table: string;
  /** The new text. */
  text: string;
}

/**
 * Insert an empty row at an index; a vertical merge it lands inside grows to cover it.
 *
 * Family `tables`; trackable: no. Table rows cannot be suggested yet: tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"table_insert_row","table":"1:9","index":1}
 */
export interface TableInsertRowEdit {
  op: "table_insert_row";
  /** 0 = above the first row, the row count = below the last. */
  index: number;
  /** The table id. */
  table: string;
}

/**
 * Delete one row; a merged cell anchored in it keeps its content on the next row.
 *
 * Family `tables`; trackable: no. Table rows cannot be suggested yet: tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"table_delete_row","table":"1:9","index":1}
 */
export interface TableDeleteRowEdit {
  op: "table_delete_row";
  /** 0-based row. */
  index: number;
  /** The table id. */
  table: string;
}

/**
 * Insert an empty table (rows x columns, with a thin grid) after an anchor's top-level block.
 *
 * Family `tables`; trackable: no. Table structure and cell formatting cannot be suggested yet: tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_table","after":{"node":"1:2"},"rows":2,"columns":3}
 */
export interface InsertTableEdit {
  op: "insert_table";
  /** A paragraph; the table goes after the body block holding it. */
  after: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
  /** Columns, 1 to 20. */
  columns: number;
  /** Rows, 1 to 50. */
  rows: number;
}

/**
 * Insert an empty column at a grid index; a merged cell it lands inside grows to cover it.
 *
 * Family `tables`; trackable: no. Table structure and cell formatting cannot be suggested yet: tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"table_insert_column","table":"1:9","index":1}
 */
export interface TableInsertColumnEdit {
  op: "table_insert_column";
  /** 0 = left of the first column, the column count = right of the last. */
  index: number;
  /** The table id. */
  table: string;
}

/**
 * Delete one grid column; a merged cell crossing it shrinks.
 *
 * Family `tables`; trackable: no. Table structure and cell formatting cannot be suggested yet: tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"table_delete_column","table":"1:9","index":1}
 */
export interface TableDeleteColumnEdit {
  op: "table_delete_column";
  /** 0-based grid column. */
  index: number;
  /** The table id. */
  table: string;
}

/**
 * Merge the rectangle between two cells into one; every cell's text moves into it in reading order.
 *
 * Family `tables`; trackable: no. Table structure and cell formatting cannot be suggested yet: tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"table_merge_cells","table":"1:9","from":{"row":0,"column":0},"to":{"row":0,"column":1}}
 */
export interface TableMergeCellsEdit {
  op: "table_merge_cells";
  /** One corner. */
  from: {
    /** 0-based cell in that row. */
    column: number;
    /** 0-based row. */
    row: number;
  };
  /** The table id. */
  table: string;
  /** The opposite corner. */
  to: {
    /** 0-based cell in that row. */
    column: number;
    /** 0-based row. */
    row: number;
  };
}

/**
 * Unmerge the merged cell a cell belongs to; its text stays in the top-left cell.
 *
 * Family `tables`; trackable: no. Table structure and cell formatting cannot be suggested yet: tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"table_split_cell","table":"1:9","cell":{"row":0,"column":0}}
 */
export interface TableSplitCellEdit {
  op: "table_split_cell";
  /** A cell of the merged cell. */
  cell: {
    /** 0-based cell in that row. */
    column: number;
    /** 0-based row. */
    row: number;
  };
  /** The table id. */
  table: string;
}

/**
 * Delete a whole table.
 *
 * Family `tables`; trackable: no. Table structure and cell formatting cannot be suggested yet: tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"table_delete","table":"1:9"}
 */
export interface TableDeleteEdit {
  op: "table_delete";
  /** The table id. */
  table: string;
}

/**
 * Set the shading and/or vertical alignment of one cell or a rectangle of cells.
 *
 * Family `tables`; trackable: no. Table structure and cell formatting cannot be suggested yet: tracked table structure is not modelled (docs/178 Q11). Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"table_set_cell_format","table":"1:9","from":{"row":0,"column":0},"to":{"row":0,"column":1},"shading":"#FFF2CC","vertical_alignment":"center"}
 */
export interface TableSetCellFormatEdit {
  op: "table_set_cell_format";
  /** The cell, or one corner of the rectangle. */
  from: {
    /** 0-based cell in that row. */
    column: number;
    /** 0-based row. */
    row: number;
  };
  /** Fill `#RRGGBB`, or `none` to clear. */
  shading?: string | null;
  /** The table id. */
  table: string;
  /** The opposite corner; omitted for one cell. */
  to?: {
    /** 0-based cell in that row. */
    column: number;
    /** 0-based row. */
    row: number;
  } | null;
  /** Vertical alignment of the text. */
  vertical_alignment?: "top" | "center" | "bottom" | null;
}

/**
 * Make a paragraph a bulleted or numbered list item, or take it out of its list.
 *
 * Family `lists`; trackable: yes.
 *
 * @example {"op":"set_list","target":{"node":"1:2"},"kind":"bullet"}
 */
export interface SetListEdit {
  op: "set_list";
  /** Which list. */
  kind: "bullet" | "numbered" | "none";
  /** The paragraph. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Move a list item down (positive) or up (negative) a number of levels.
 *
 * Family `lists`; trackable: yes.
 *
 * @example {"op":"change_list_level","target":{"node":"1:3"},"delta":1}
 */
export interface ChangeListLevelEdit {
  op: "change_list_level";
  /** Levels to move, clamped to levels 0..=8. */
  delta: number;
  /** The list item. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Restart numbering at a numbered list item, at 1 or a given value.
 *
 * Family `lists`; trackable: no. Restarting or continuing numbering cannot be suggested: it moves items to another numbering instance, which Word has no tracked-change record for. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"restart_numbering","target":{"node":"1:3"},"value":1}
 */
export interface RestartNumberingEdit {
  op: "restart_numbering";
  /** The list item. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
  /** The number it starts at, 1 to 32767 (default 1). */
  value?: number | null;
}

/**
 * Continue numbering from the nearest earlier numbered list at the item's level.
 *
 * Family `lists`; trackable: no. Restarting or continuing numbering cannot be suggested: it moves items to another numbering instance, which Word has no tracked-change record for. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"continue_numbering","target":{"node":"1:3"}}
 */
export interface ContinueNumberingEdit {
  op: "continue_numbering";
  /** The list item. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
}

/**
 * Comment on the quoted text, or reply to a comment.
 *
 * Family `review`; trackable: not-applicable.
 *
 * @example {"op":"comment","target":{"node":"1:2","exact":"notice period"},"text":"Aligned with the MSA, clause 9."}
 */
export interface CommentEdit {
  op: "comment";
  /** A comment id to reply to. */
  reply_to?: string | null;
  /** The text commented on. */
  target: {
    /** The quoted text, matched inside `node` only. */
    exact?: string | null;
    /** A paragraph id returned by `get_outline`, `read_blocks` or `search`. */
    node: string;
    /** 1-based occurrence of `exact` among the matches that remain. */
    occurrence?: number | null;
    /** Offsets for programs only; never computed by a model. */
    pos?: {
      /** End, exclusive. */
      end: number;
      /** Start, UTF-8 bytes into the node's text. */
      start: number;
    } | null;
    /** Text immediately before `exact`, to disambiguate. */
    prefix?: string | null;
    /** For an insertion point: before or after the target. */
    side?: "before" | "after" | null;
    /** Text immediately after `exact`, to disambiguate. */
    suffix?: string | null;
  };
  /** The comment. */
  text: string;
}

/**
 * Resolve or reopen a comment thread.
 *
 * Family `review`; trackable: not-applicable.
 *
 * @example {"op":"resolve_comment","id":"1:40","resolved":true}
 */
export interface ResolveCommentEdit {
  op: "resolve_comment";
  /** The comment id. */
  id: string;
  /** `true` (the default) resolves, `false` reopens. */
  resolved?: boolean | null;
}

/**
 * Accept or reject named tracked changes, or all of them.
 *
 * Family `review`; trackable: not-applicable.
 *
 * @example {"op":"review","decision":"accept","revisions":["1:41"]}
 */
export interface ReviewEdit {
  op: "review";
  /** Every revision in the document. */
  all?: boolean;
  /** Accept or reject. */
  decision: "accept" | "reject";
  /** Revision ids. */
  revisions?: string[];
}

/** Every `op` the vocabulary admits. */
export type EditOp =
  | "insert_text"
  | "replace_text"
  | "delete_text"
  | "insert_paragraphs"
  | "apply_paragraph_style"
  | "set_text_format"
  | "clear_formatting"
  | "rewrite_block"
  | "set_alignment"
  | "set_line_spacing"
  | "set_paragraph_spacing"
  | "set_indent"
  | "set_cell_text"
  | "table_insert_row"
  | "table_delete_row"
  | "insert_table"
  | "table_insert_column"
  | "table_delete_column"
  | "table_merge_cells"
  | "table_split_cell"
  | "table_delete"
  | "table_set_cell_format"
  | "set_list"
  | "change_list_level"
  | "restart_numbering"
  | "continue_numbering"
  | "comment"
  | "resolve_comment"
  | "review";

/** One edit, as an edit plan's `edits[]` carries it. */
export type Edit =
  | InsertTextEdit
  | ReplaceTextEdit
  | DeleteTextEdit
  | InsertParagraphsEdit
  | ApplyParagraphStyleEdit
  | SetTextFormatEdit
  | ClearFormattingEdit
  | RewriteBlockEdit
  | SetAlignmentEdit
  | SetLineSpacingEdit
  | SetParagraphSpacingEdit
  | SetIndentEdit
  | SetCellTextEdit
  | TableInsertRowEdit
  | TableDeleteRowEdit
  | InsertTableEdit
  | TableInsertColumnEdit
  | TableDeleteColumnEdit
  | TableMergeCellsEdit
  | TableSplitCellEdit
  | TableDeleteEdit
  | TableSetCellFormatEdit
  | SetListEdit
  | ChangeListLevelEdit
  | RestartNumberingEdit
  | ContinueNumberingEdit
  | CommentEdit
  | ResolveCommentEdit
  | ReviewEdit;

/** The toolset families (docs/181 §5.2). */
export type EditFamily = "text" | "paragraph" | "styles" | "tables" | "lists" | "review";

/** The edits of each family. */
export interface EditsByFamily {
  text: InsertTextEdit | ReplaceTextEdit | DeleteTextEdit | SetTextFormatEdit | ClearFormattingEdit;
  paragraph: InsertParagraphsEdit | RewriteBlockEdit | SetAlignmentEdit | SetLineSpacingEdit | SetParagraphSpacingEdit | SetIndentEdit;
  styles: ApplyParagraphStyleEdit;
  tables: SetCellTextEdit | TableInsertRowEdit | TableDeleteRowEdit | InsertTableEdit | TableInsertColumnEdit | TableDeleteColumnEdit | TableMergeCellsEdit | TableSplitCellEdit | TableDeleteEdit | TableSetCellFormatEdit;
  lists: SetListEdit | ChangeListLevelEdit | RestartNumberingEdit | ContinueNumberingEdit;
  review: CommentEdit | ResolveCommentEdit | ReviewEdit;
}
