// SPDX-License-Identifier: Apache-2.0
// GENERATED from the capability registry (crates/casual-doc-command, docs/181 §6). Do not edit.
// Regenerate: cargo run -p casual-doc-command --bin opendoc-command-gen

/** The edit vocabulary version (docs/181 §4.1): a minor bump adds commands; a major bump changes or removes one. */
export declare const VOCABULARY_VERSION: "1.0";

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
 * Comment on the quoted text, or reply to a comment.
 *
 * Family `review`; trackable: not-applicable.
 *
 * @example {"op":"comment","target":{"node":"1:2","exact":"termination"},"text":"Aligned with the MSA, clause 9."}
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
  | "set_cell_text"
  | "table_insert_row"
  | "table_delete_row"
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
  | SetCellTextEdit
  | TableInsertRowEdit
  | TableDeleteRowEdit
  | CommentEdit
  | ResolveCommentEdit
  | ReviewEdit;

/** The toolset families (docs/181 §5.2). */
export type EditFamily = "text" | "paragraph" | "styles" | "tables" | "review";

/** The edits of each family. */
export interface EditsByFamily {
  text: InsertTextEdit | ReplaceTextEdit | DeleteTextEdit | SetTextFormatEdit;
  paragraph: InsertParagraphsEdit;
  styles: ApplyParagraphStyleEdit;
  tables: SetCellTextEdit | TableInsertRowEdit | TableDeleteRowEdit;
  review: CommentEdit | ResolveCommentEdit | ReviewEdit;
}
