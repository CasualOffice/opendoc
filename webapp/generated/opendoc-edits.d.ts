// SPDX-License-Identifier: Apache-2.0
// GENERATED from the capability registry (crates/casual-doc-command, docs/181 §6). Do not edit.
// Regenerate: cargo run -p casual-doc-command --bin opendoc-command-gen

/** The edit vocabulary version (docs/181 §4.1): a minor bump adds commands; a major bump changes or removes one. */
export declare const VOCABULARY_VERSION: "1.3";

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
 * Link the quoted text to an http(s), mailto, ftp or tel URL, or to a bookmark as `#name`.
 *
 * Family `links`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_hyperlink","target":{"node":"1:2","exact":"notice period"},"url":"https://example.com/terms"}
 */
export interface SetHyperlinkEdit {
  op: "set_hyperlink";
  /** The text to link (a non-empty quote). */
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
  /** A tooltip shown on hover. */
  tooltip?: string | null;
  /**
   * An `http`, `https`, `mailto`, `ftp` or `tel` URL, or `#` and a
   * bookmark name.
   */
  url: string;
}

/**
 * Remove the hyperlink whose text is exactly the quote; the text and its formatting stay.
 *
 * Family `links`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"remove_hyperlink","target":{"node":"1:3","exact":"First"}}
 */
export interface RemoveHyperlinkEdit {
  op: "remove_hyperlink";
  /** The linked text, exactly. */
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
 * Bookmark the quoted text (a letter, then letters, digits or _, at most 40).
 *
 * Family `links`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"create_bookmark","target":{"node":"1:2","exact":"thirty (30) days"},"name":"NoticePeriod"}
 */
export interface CreateBookmarkEdit {
  op: "create_bookmark";
  /**
   * Its name: a letter first, then letters, digits or `_`, at most 40
   * characters, as Word requires.
   */
  name: string;
  /** What the bookmark spans. */
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
 * Rename a bookmark.
 *
 * Family `links`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"rename_bookmark","name":"Notice","new_name":"NoticeClause"}
 */
export interface RenameBookmarkEdit {
  op: "rename_bookmark";
  /** Its current name. */
  name: string;
  /** Its new name. */
  new_name: string;
}

/**
 * Delete a bookmark; the text it spans stays.
 *
 * Family `links`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"delete_bookmark","name":"Notice"}
 */
export interface DeleteBookmarkEdit {
  op: "delete_bookmark";
  /** Its name. */
  name: string;
}

/**
 * Insert a page-number, page-count, date, time, file-name or author field at an anchor.
 *
 * Family `fields`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_field","at":{"node":"1:2","side":"after"},"kind":"page"}
 */
export interface InsertFieldEdit {
  op: "insert_field";
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
  /** Which field. */
  kind: "page" | "numpages" | "date" | "time" | "filename" | "author";
  /**
   * The displayed value of a date, time, file-name or author field
   * (the engine reads no clock or file system); ignored for page
   * numbers, which are computed.
   */
  result?: string | null;
}

/**
 * Insert a picture (base64 PNG, JPEG, GIF, BMP, TIFF or WebP, at most 8 MiB) at an anchor.
 *
 * Family `objects`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_image","at":{"node":"1:2","side":"after"},"data":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==","content_type":"image/png","width_pt":36,"alt_text":"Company logo"}
 */
export interface InsertImageEdit {
  op: "insert_image";
  /** Alt text for readers who cannot see it. */
  alt_text?: string | null;
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
  /**
   * Its MIME type: image/png, image/jpeg, image/gif, image/bmp,
   * image/tiff or image/webp; the bytes must be that format.
   */
  content_type: string;
  /** The image bytes, standard base64, at most 8 MiB decoded. */
  data: string;
  /** Height in points; omitted, it keeps the picture's proportions. */
  height_pt?: number | null;
  /**
   * Width in points; with `height` omitted too, the picture's own
   * size at 96 DPI (PNG, GIF, JPEG and BMP state it).
   */
  width_pt?: number | null;
}

/**
 * Set or clear an object's alt text.
 *
 * Family `objects`; trackable: no. Object size, wrap, crop, alt text, fill, outline and deletion cannot be suggested: no tracked-change record for them is modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_alt_text","object":"1:20","text":"Signature of the lessor"}
 */
export interface SetAltTextEdit {
  op: "set_alt_text";
  /** The object id `read_blocks` lists. */
  object: string;
  /** The alt text; omitted or empty clears it. */
  text?: string | null;
}

/**
 * Resize a picture, shape or text box, in points.
 *
 * Family `objects`; trackable: no. Object size, wrap, crop, alt text, fill, outline and deletion cannot be suggested: no tracked-change record for them is modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"resize_object","object":"1:20","width_pt":72,"height_pt":72}
 */
export interface ResizeObjectEdit {
  op: "resize_object";
  /** Height in points. */
  height_pt: number;
  /** The object id. */
  object: string;
  /** Width in points. */
  width_pt: number;
}

/**
 * Set how text wraps around a floating object.
 *
 * Family `objects`; trackable: no. Object size, wrap, crop, alt text, fill, outline and deletion cannot be suggested: no tracked-change record for them is modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_wrap","object":"1:20","wrap":"square"}
 */
export interface SetWrapEdit {
  op: "set_wrap";
  /** The object id. */
  object: string;
  /** The wrap. */
  wrap: "square" | "tight" | "through" | "top_and_bottom" | "behind" | "front";
}

/**
 * Crop a picture by the fraction of each edge to hide; all zero removes the crop.
 *
 * Family `objects`; trackable: no. Object size, wrap, crop, alt text, fill, outline and deletion cannot be suggested: no tracked-change record for them is modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"crop_image","object":"1:20","left":0.1,"top":0,"right":0.1,"bottom":0}
 */
export interface CropImageEdit {
  op: "crop_image";
  /** Bottom edge. */
  bottom: number;
  /** Left edge. */
  left: number;
  /** The picture id. */
  object: string;
  /** Right edge. */
  right: number;
  /** Top edge. */
  top: number;
}

/**
 * Delete a picture, shape, text box or group.
 *
 * Family `objects`; trackable: no. Object size, wrap, crop, alt text, fill, outline and deletion cannot be suggested: no tracked-change record for them is modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"delete_object","object":"1:20"}
 */
export interface DeleteObjectEdit {
  op: "delete_object";
  /** The object id. */
  object: string;
}

/**
 * Set a shape's fill to a colour, or remove it.
 *
 * Family `objects`; trackable: no. Object size, wrap, crop, alt text, fill, outline and deletion cannot be suggested: no tracked-change record for them is modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_shape_fill","shape":"1:20","color":"#FFF2CC"}
 */
export interface SetShapeFillEdit {
  op: "set_shape_fill";
  /** `#RRGGBB`, or `none` to remove the fill. */
  color: string;
  /** The shape id. */
  shape: string;
}

/**
 * Set a shape's or a picture's outline colour and/or weight, or remove it.
 *
 * Family `objects`; trackable: no. Object size, wrap, crop, alt text, fill, outline and deletion cannot be suggested: no tracked-change record for them is modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_shape_outline","shape":"1:20","color":"#1F3864","width_pt":1.5}
 */
export interface SetShapeOutlineEdit {
  op: "set_shape_outline";
  /** `#RRGGBB`, or `none` to remove the outline. */
  color?: string | null;
  /** The shape or picture id. */
  shape: string;
  /** Weight in points. */
  width_pt?: number | null;
}

/**
 * Insert a footnote with its text; the reference mark goes at the anchor.
 *
 * Family `notes`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_footnote","at":{"node":"1:2","exact":"days","side":"after"},"text":"Calendar days, not business days."}
 */
export interface InsertFootnoteEdit {
  op: "insert_footnote";
  /** Where the reference mark goes. */
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
  /** The note's text. */
  text: string;
}

/**
 * Insert an endnote with its text; the reference mark goes at the anchor.
 *
 * Family `notes`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_endnote","at":{"node":"1:2","exact":"days","side":"after"},"text":"See the schedule of notice periods."}
 */
export interface InsertEndnoteEdit {
  op: "insert_endnote";
  /** Where the reference mark goes. */
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
  /** The note's text. */
  text: string;
}

/**
 * Insert a line, page or column break, or a blank page, at an anchor.
 *
 * Family `sections`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_break","at":{"node":"1:2","side":"after"},"kind":"page"}
 */
export interface InsertBreakEdit {
  op: "insert_break";
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
  /** Which break. */
  kind: "line" | "page" | "column" | "blank_page";
}

/**
 * Start a new section at an anchor; it inherits the page setup, headers and footers.
 *
 * Family `sections`; trackable: no. Page setup, section breaks, headers, footers and the watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_section_break","at":{"node":"1:2","side":"after"},"start":"next_page"}
 */
export interface InsertSectionBreakEdit {
  op: "insert_section_break";
  /** Where; a top-level body paragraph. */
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
  /** Where the new section starts. */
  start: "next_page" | "continuous" | "even_page" | "odd_page";
}

/**
 * Change a section's (or every section's) page size, orientation, margins or columns, in twips.
 *
 * Family `sections`; trackable: no. Page setup, section breaks, headers, footers and the watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_page_setup","section":0,"orientation":"landscape","margins":{"left":1080,"right":1080}}
 */
export interface SetPageSetupEdit {
  op: "set_page_setup";
  /** Space between columns, twips. */
  column_spacing?: number | null;
  /** Number of text columns, 1 to 45. */
  columns?: number | null;
  /** Page height, twips (Letter is 15840, A4 16838). */
  height?: number | null;
  /** Margins, twips. */
  margins?: {
    /** Bottom margin, twips. */
    bottom?: number | null;
    /** Left (start) margin, twips. */
    left?: number | null;
    /** Right (end) margin, twips. */
    right?: number | null;
    /** Top margin, twips. */
    top?: number | null;
  } | null;
  /** Orientation; a change swaps width and height unless both are given. */
  orientation?: "portrait" | "landscape" | null;
  /**
   * The 0-based section (`get_outline` lists them); omitted, every
   * section.
   */
  section?: number | null;
  /** Page width, twips (Letter is 12240, A4 11906). */
  width?: number | null;
}

/**
 * Number a section's lines in the margin every N lines, or turn it off with 0.
 *
 * Family `sections`; trackable: no. Page setup, section breaks, headers, footers and the watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_line_numbering","section":0,"count_by":1,"restart":"each_page"}
 */
export interface SetLineNumberingEdit {
  op: "set_line_numbering";
  /** Number every Nth line; 0 turns numbering off. */
  count_by: number;
  /** When numbering restarts (default each page). */
  restart?: "each_page" | "each_section" | "continuous" | null;
  /** The 0-based section; omitted, every section. */
  section?: number | null;
  /** The first number (default 1). */
  start?: number | null;
}

/**
 * Set a section's page-number format, and restart it at a number or continue.
 *
 * Family `sections`; trackable: no. Page setup, section breaks, headers, footers and the watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_page_numbering","section":0,"format":"lower_roman","start":1}
 */
export interface SetPageNumberingEdit {
  op: "set_page_numbering";
  /** The format; omitted, decimal. */
  format?: "decimal" | "lower_roman" | "upper_roman" | "lower_letter" | "upper_letter" | null;
  /** The 0-based section; omitted, every section. */
  section?: number | null;
  /**
   * Restart at this number; omitted, continue from the previous
   * section.
   */
  start?: number | null;
}

/**
 * Stamp a text watermark on every page, or remove it.
 *
 * Family `sections`; trackable: no. Page setup, section breaks, headers, footers and the watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_watermark","text":"DRAFT"}
 */
export interface SetWatermarkEdit {
  op: "set_watermark";
  /** `#RRGGBB` (default light grey). */
  color?: string | null;
  /** The font family (default the document's). */
  font?: string | null;
  /** The angle (default diagonal). */
  layout?: "diagonal" | "horizontal" | null;
  /** The words; omitted or empty removes the watermark. */
  text?: string | null;
}

/**
 * Add an empty header to a section; read_blocks on its surface gives the paragraph to write into.
 *
 * Family `headers_footers`; trackable: no. Page setup, section breaks, headers, footers and the watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_header","section":0}
 */
export interface InsertHeaderEdit {
  op: "insert_header";
  /** Which pages it is for (default every page). */
  pages?: "default" | "first" | "even";
  /** The 0-based section (default 0). */
  section?: number;
}

/**
 * Add an empty footer to a section; read_blocks on its surface gives the paragraph to write into.
 *
 * Family `headers_footers`; trackable: no. Page setup, section breaks, headers, footers and the watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_footer","section":0}
 */
export interface InsertFooterEdit {
  op: "insert_footer";
  /** Which pages it is for (default every page). */
  pages?: "default" | "first" | "even";
  /** The 0-based section (default 0). */
  section?: number;
}

/**
 * Give a section's first page its own header and footer, or not.
 *
 * Family `headers_footers`; trackable: no. Page setup, section breaks, headers, footers and the watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_different_first_page","section":0,"enabled":true}
 */
export interface SetDifferentFirstPageEdit {
  op: "set_different_first_page";
  /** On or off. */
  enabled: boolean;
  /** The 0-based section. */
  section: number;
}

/**
 * Give even and odd pages their own headers and footers, or not (document-wide).
 *
 * Family `headers_footers`; trackable: no. Page setup, section breaks, headers, footers and the watermark cannot be suggested yet: tracked section properties (`w:sectPrChange`) are not modelled. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"set_different_odd_even","enabled":true}
 */
export interface SetDifferentOddEvenEdit {
  op: "set_different_odd_even";
  /** On or off. */
  enabled: boolean;
}

/**
 * Insert a chart (column, bar, line, area, scatter, pie or doughnut, plain, stacked or percent) with sample data at an anchor.
 *
 * Family `charts`; trackable: no. Links, bookmarks, fields, pictures, notes and breaks cannot be suggested yet: the review lowering tracks text, paragraph marks and properties, not the insertion or removal of inline structure. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_chart","at":{"node":"1:2","side":"after"},"kind":"column"}
 */
export interface InsertChartEdit {
  op: "insert_chart";
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
  /** The chart family. */
  kind: "column" | "column-stacked" | "column-percent" | "bar" | "bar-stacked" | "bar-percent" | "line" | "line-markers" | "line-stacked" | "line-percent" | "area" | "area-stacked" | "area-percent" | "scatter" | "scatter-smooth" | "pie" | "doughnut";
}

/**
 * Insert a table of contents (each heading with its page) before a body paragraph.
 *
 * Family `references`; trackable: no. Tables of contents, captions and cross-references cannot be suggested yet: they insert fields and whole paragraphs and rewrite field results, which the review lowering does not track. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_table_of_contents","before":{"node":"1:2"},"levels":2}
 */
export interface InsertTableOfContentsEdit {
  op: "insert_table_of_contents";
  /**
   * The body paragraph the table goes before (its id; a quote is not
   * needed).
   */
  before: {
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
  /** The deepest heading level listed, 1 to 9; 3 when omitted. */
  levels?: number | null;
}

/**
 * Update a table of contents: its page numbers only, or the entire table from the headings.
 *
 * Family `references`; trackable: no. Tables of contents, captions and cross-references cannot be suggested yet: they insert fields and whole paragraphs and rewrite field results, which the review lowering does not track. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"update_table_of_contents","mode":"entire"}
 */
export interface UpdateTableOfContentsEdit {
  op: "update_table_of_contents";
  /**
   * The table's field id, as `read_blocks` lists it; omitted, the
   * document's only table of contents.
   */
  field?: string | null;
  /**
   * `page_numbers` keeps the entries and re-reads their pages;
   * `entire` rebuilds the entries from the headings.
   */
  mode: "page_numbers" | "entire";
}

/**
 * Insert a numbered caption (Figure, Table, Equation or the document's own label) above or below a table, picture or paragraph.
 *
 * Family `references`; trackable: no. Tables of contents, captions and cross-references cannot be suggested yet: they insert fields and whole paragraphs and rewrite field results, which the review lowering does not track. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_caption","target":"1:9","label":"Table","text":": Payment schedule"}
 */
export interface InsertCaptionEdit {
  op: "insert_caption";
  /**
   * Include the chapter number, counted from headings of this level
   * (1 to 9).
   */
  chapter_level?: number | null;
  /** Leave the label out and show only the number. */
  exclude_label?: boolean;
  /**
   * The label and numbering sequence: `Figure`, `Table`, `Equation` or
   * any label the document already uses.
   */
  label: string;
  /** How the number is written; arabic when omitted. */
  number_format?: "arabic" | "upper_roman" | "lower_roman" | "upper_letter" | "lower_letter";
  /** Above or below the target; below when omitted. */
  position?: "above" | "below";
  /** The separator after the chapter number; a hyphen when omitted. */
  separator?: "hyphen" | "period" | "colon" | "em_dash" | "en_dash";
  /** The id of the table, picture or paragraph the caption is for. */
  target: string;
  /** The caption's own text after the number. */
  text?: string;
}

/**
 * Renumber every caption whose shown number is stale.
 *
 * Family `references`; trackable: no. Tables of contents, captions and cross-references cannot be suggested yet: they insert fields and whole paragraphs and rewrite field results, which the review lowering does not track. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"update_caption_numbers"}
 */
export interface UpdateCaptionNumbersEdit {
  op: "update_caption_numbers";
}

/**
 * Insert a cross-reference to a heading, caption, bookmarked paragraph or note: its text, label and number, page, or above/below.
 *
 * Family `references`; trackable: no. Tables of contents, captions and cross-references cannot be suggested yet: they insert fields and whole paragraphs and rewrite field results, which the review lowering does not track. Use mode `direct` under a profile that grants `edit`.
 *
 * @example {"op":"insert_cross_reference","at":{"node":"1:2","side":"after"},"target":"1:3","show":"page_number"}
 */
export interface InsertCrossReferenceEdit {
  op: "insert_cross_reference";
  /** Where the reference goes. */
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
  /** Make the reference a link to its target. */
  hyperlink?: boolean;
  /** Follow it with `above` or `below`, as Word's checkbox does. */
  include_above_below?: boolean;
  /** What the reference shows. */
  show: "entire_caption" | "label_and_number" | "caption_text" | "page_number" | "above_below" | "paragraph_text";
  /**
   * The paragraph referred to: a heading, a caption, a bookmarked
   * paragraph or a note's first paragraph.
   */
  target: string;
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
  | "set_hyperlink"
  | "remove_hyperlink"
  | "create_bookmark"
  | "rename_bookmark"
  | "delete_bookmark"
  | "insert_field"
  | "insert_image"
  | "set_alt_text"
  | "resize_object"
  | "set_wrap"
  | "crop_image"
  | "delete_object"
  | "set_shape_fill"
  | "set_shape_outline"
  | "insert_footnote"
  | "insert_endnote"
  | "insert_break"
  | "insert_section_break"
  | "set_page_setup"
  | "set_line_numbering"
  | "set_page_numbering"
  | "set_watermark"
  | "insert_header"
  | "insert_footer"
  | "set_different_first_page"
  | "set_different_odd_even"
  | "insert_chart"
  | "insert_table_of_contents"
  | "update_table_of_contents"
  | "insert_caption"
  | "update_caption_numbers"
  | "insert_cross_reference"
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
  | SetHyperlinkEdit
  | RemoveHyperlinkEdit
  | CreateBookmarkEdit
  | RenameBookmarkEdit
  | DeleteBookmarkEdit
  | InsertFieldEdit
  | InsertImageEdit
  | SetAltTextEdit
  | ResizeObjectEdit
  | SetWrapEdit
  | CropImageEdit
  | DeleteObjectEdit
  | SetShapeFillEdit
  | SetShapeOutlineEdit
  | InsertFootnoteEdit
  | InsertEndnoteEdit
  | InsertBreakEdit
  | InsertSectionBreakEdit
  | SetPageSetupEdit
  | SetLineNumberingEdit
  | SetPageNumberingEdit
  | SetWatermarkEdit
  | InsertHeaderEdit
  | InsertFooterEdit
  | SetDifferentFirstPageEdit
  | SetDifferentOddEvenEdit
  | InsertChartEdit
  | InsertTableOfContentsEdit
  | UpdateTableOfContentsEdit
  | InsertCaptionEdit
  | UpdateCaptionNumbersEdit
  | InsertCrossReferenceEdit
  | CommentEdit
  | ResolveCommentEdit
  | ReviewEdit;

/** The toolset families (docs/181 §5.2). */
export type EditFamily = "text" | "paragraph" | "styles" | "tables" | "lists" | "links" | "fields" | "objects" | "notes" | "sections" | "headers_footers" | "charts" | "references" | "review";

/** The edits of each family. */
export interface EditsByFamily {
  text: InsertTextEdit | ReplaceTextEdit | DeleteTextEdit | SetTextFormatEdit | ClearFormattingEdit;
  paragraph: InsertParagraphsEdit | RewriteBlockEdit | SetAlignmentEdit | SetLineSpacingEdit | SetParagraphSpacingEdit | SetIndentEdit;
  styles: ApplyParagraphStyleEdit;
  tables: SetCellTextEdit | TableInsertRowEdit | TableDeleteRowEdit | InsertTableEdit | TableInsertColumnEdit | TableDeleteColumnEdit | TableMergeCellsEdit | TableSplitCellEdit | TableDeleteEdit | TableSetCellFormatEdit;
  lists: SetListEdit | ChangeListLevelEdit | RestartNumberingEdit | ContinueNumberingEdit;
  links: SetHyperlinkEdit | RemoveHyperlinkEdit | CreateBookmarkEdit | RenameBookmarkEdit | DeleteBookmarkEdit;
  fields: InsertFieldEdit;
  objects: InsertImageEdit | SetAltTextEdit | ResizeObjectEdit | SetWrapEdit | CropImageEdit | DeleteObjectEdit | SetShapeFillEdit | SetShapeOutlineEdit;
  notes: InsertFootnoteEdit | InsertEndnoteEdit;
  sections: InsertBreakEdit | InsertSectionBreakEdit | SetPageSetupEdit | SetLineNumberingEdit | SetPageNumberingEdit | SetWatermarkEdit;
  headers_footers: InsertHeaderEdit | InsertFooterEdit | SetDifferentFirstPageEdit | SetDifferentOddEvenEdit;
  charts: InsertChartEdit;
  references: InsertTableOfContentsEdit | UpdateTableOfContentsEdit | InsertCaptionEdit | UpdateCaptionNumbersEdit | InsertCrossReferenceEdit;
  review: CommentEdit | ResolveCommentEdit | ReviewEdit;
}
