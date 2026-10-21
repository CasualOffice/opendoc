// SPDX-License-Identifier: Apache-2.0
// The parts of a comment card that say WHAT was commented on and let its author
// change what they said: the anchored-text quote (RC-12) and the in-place comment
// editor (RC-11), shared by a thread's root and its replies.
//
// ---- WHAT THE COMPETITORS DO (SKILL §8) -------------------------------------
//
// Editing. Word, Google Docs and ONLYOFFICE all let the author edit the ROOT of a
// thread, not only its replies: Word's comment card offers Edit comment for the
// author's own comment, Docs puts Edit on the thread's ⋮ menu, and ONLYOFFICE
// shows an edit control on the root comment beside Resolve. (The ONLYOFFICE
// source checkout was not present in this lane, so no file:line is cited; this
// is the shipped behaviour.) This card had Edit on replies and only Resolve
// and Delete on the root, so a typo in the comment that started a thread could
// be fixed only by deleting the thread and every reply under it. The engine has
// always taken the root through the same `updateComment` a reply uses.
//
// The edit control is ONE editor for both, and it is the composer's own key
// contract (`attachComposerKeys`): Enter or Ctrl/Cmd+Enter saves, Shift+Enter is
// a newline, Escape cancels. The reply editor used to be a single-line `<input>`
// with its own Enter handler, so a reply that contained a line break was
// silently flattened by editing it — a second, weaker path for one action.
//
// The quote. ONLYOFFICE stores the quoted text with the comment and keeps
// showing it after the commented text is deleted; Google Docs keeps the thread and marks its original text as deleted;
// Word deletes a comment whose whole range is deleted. A card that keeps the
// thread but shows nothing of what it referred to is the worst of the three — the
// reader has a conversation about nothing. So the card quotes the anchored text
// while it exists and keeps the last text it saw once it does not, marked
// deleted.
//
// ---- THE PATTERN, NAMED -----------------------------------------------------
//
// The quote memory is a last-known-value cache (a "stale-while-gone" read): the
// live value is always preferred, and the remembered one is shown only — and
// visibly — when the live one has gone. It is bounded by the number of comments
// in the open document and cleared when the document is replaced.
//
// It lives in memory, not in the file. DOCX has no field for a comment's quoted
// text (`w:comment` carries only the comment's own paragraphs), and keeping one
// in the model is an engine change outside this module; a document reopened
// after its commented text was deleted therefore has no quote to show. That is
// recorded as a limit in `109` rather than papered over here.
//
// Every string is a catalogue key; nothing here calls `t()` before the card it
// builds is being shown (SKILL §11).
import { attachComposerKeys, autoGrowTextarea, reviewCardButton, reviewIconButton } from "./review_chrome.mjs";

/** How much of the anchored text a card quotes. A quote identifies the passage;
 *  it is not a second copy of it, and an unbounded one would make a comment on a
 *  whole section as tall as the section. */
export const QUOTE_LIMIT = 160;

/**
 * The anchored text as a card quotes it: whitespace collapsed to single spaces,
 * trimmed, and cut at [`QUOTE_LIMIT`] code points with an ellipsis.
 *
 * Code points and not UTF-16 units, so the cut never splits a surrogate pair and
 * leaves a lone half of an emoji on the card.
 *
 * Complexity: O(text).
 *
 * @param {unknown} raw
 * @param {number} [limit]
 * @returns {string}
 */
export function commentQuoteText(raw, limit = QUOTE_LIMIT) {
  const text = String(raw ?? "").replace(/\s+/g, " ").trim();
  const points = Array.from(text);
  return points.length > limit ? `${points.slice(0, limit).join("").trimEnd()}…` : text;
}

/** Whether a comment is a reply. A reply has no anchor of its own worth quoting:
 *  it shares its root's, and repeating the quote on every reply is noise. */
const isReply = (comment) => comment?.parentParaId != null && comment.parentParaId !== "";

/**
 * The quote memory for one open document.
 *
 * `read(doc, comment)` answers `{text, deleted}` or `null`:
 *   * the live anchored text when the anchor still covers some (`deleted: false`),
 *     remembering it;
 *   * the last text it saw, `deleted: true`, once the anchor covers none;
 *   * `null` for a reply, or for a comment whose anchor has never covered text in
 *     this session (a point comment, or one opened already collapsed).
 *
 * `prime(doc, comments)` reads every root once, so a comment whose text is
 * deleted before its card is ever drawn still has a quote to show. The caller
 * runs it when a document opens.
 *
 * Complexity: `read` is O(anchored text) — one `copyText` over the anchor's own
 * range, never a document walk. `prime` is O(comments × their anchored text),
 * once per open. Memory is bounded by the document's comment count.
 *
 * @param {{limit?: number}} [options]
 */
export function createCommentQuotes({ limit = QUOTE_LIMIT } = {}) {
  const known = new Map();
  function live(doc, comment) {
    const anchor = comment?.anchor;
    if (!doc || !anchor?.node) return "";
    const start = Number(anchor.start) || 0;
    const end = Number(anchor.end) || start;
    if (end <= start) return "";
    try {
      return commentQuoteText(doc.copyText(anchor.node, start, anchor.node, end), limit);
    } catch {
      return "";
    }
  }
  function read(doc, comment) {
    if (!comment?.id || isReply(comment)) return null;
    const text = live(doc, comment);
    if (text) {
      known.set(comment.id, text);
      return Object.freeze({ text, deleted: false });
    }
    const kept = known.get(comment.id);
    return kept ? Object.freeze({ text: kept, deleted: true }) : null;
  }
  return Object.freeze({
    read,
    prime(doc, comments) {
      for (const comment of comments ?? []) read(doc, comment);
    },
    /** Forgets everything: the next document's comment ids are not this one's. */
    clear() {
      known.clear();
    },
    /** How many quotes are remembered — for the bound's guard. */
    size: () => known.size,
  });
}

/**
 * The quote element for a card, or `null` when there is nothing to quote.
 *
 * A `<blockquote>` because it IS a quotation of the document, which is also what
 * a screen reader announces it as. A deleted quote says so in words as well as in
 * strikethrough: strikethrough alone is a visual convention a screen reader does
 * not voice and a high-contrast theme may not draw.
 *
 * @param {{text: string, deleted: boolean}|null} quote
 * @param {(key: string) => string} t
 * @returns {HTMLElement|null}
 */
export function commentQuoteElement(quote, t) {
  if (!quote?.text) return null;
  const block = document.createElement("blockquote");
  block.className = `review-margin-quote${quote.deleted ? " deleted" : ""}`;
  block.dataset.testid = "review-comment-quote";
  const text = document.createElement("span");
  text.className = "review-margin-quote-text";
  text.textContent = quote.text;
  block.appendChild(text);
  if (quote.deleted) {
    const state = document.createElement("small");
    state.className = "review-margin-quote-state";
    state.textContent = t("reviewCard.quoteDeleted");
    block.appendChild(state);
  }
  return block;
}

/**
 * The in-place editor for a comment or a reply: a growing textarea holding the
 * current text, with Cancel and Save.
 *
 * `onSave(text)` receives the trimmed text and is not called for an empty one —
 * emptying a comment is deleting it, which has its own control and its own
 * confirmation. `onCancel()` puts the card back. Both are the CALLER's, because
 * applying the edit goes through `runEdit` and the card's re-render, which live
 * in the host. `onInput(text)` is told of every keystroke, so the caller can keep
 * the draft where a re-render does not destroy it ([`createCommentEditState`]).
 *
 * @param {{text: string, kind: "comment"|"reply", t: (key: string) => string,
 *          onSave: (text: string) => unknown, onCancel: () => unknown,
 *          onInput?: (text: string) => void}} options
 * @returns {{root: HTMLElement, field: HTMLTextAreaElement, focus: () => void}}
 */
export function commentEditor({ text, kind, t, onSave, onCancel, onInput }) {
  const root = document.createElement("div");
  root.className = "review-comment-editor";
  const field = document.createElement("textarea");
  field.rows = 1;
  field.maxLength = 4096;
  field.value = String(text ?? "");
  field.dataset.testid = "review-comment-edit";
  field.setAttribute("aria-label", t(kind === "reply" ? "reviewCard.replyText" : "reviewCard.commentText"));
  autoGrowTextarea(field);
  const save = reviewCardButton(
    t("reviewCard.save"),
    async () => {
      const next = field.value.trim();
      if (!next) return;
      await onSave(next);
    },
    false,
    t(kind === "reply" ? "reviewCard.saveReply" : "reviewCard.saveComment"),
  );
  save.dataset.testid = "review-comment-edit-save";
  const cancel = reviewCardButton(t("reviewCard.cancel"), () => onCancel());
  cancel.dataset.testid = "review-comment-edit-cancel";
  const actions = document.createElement("div");
  actions.className = "review-composer-actions";
  actions.append(cancel, save);
  // The card toggles on a click and a pointerdown outside a control; the editor
  // is a control.
  field.addEventListener("pointerdown", (event) => event.stopPropagation());
  field.addEventListener("click", (event) => event.stopPropagation());
  field.addEventListener("input", () => onInput?.(field.value));
  attachComposerKeys(field, { onSubmit: () => save.click(), onCancel: () => cancel.click() });
  root.append(field, actions);
  return {
    root,
    field,
    focus() {
      field.focus({ preventScroll: true });
      const end = field.value.length;
      field.setSelectionRange(end, end);
    },
  };
}

/**
 * The root comment's Edit control — the pencil ONLYOFFICE puts beside Resolve.
 *
 * An icon in the card head, beside Resolve, rather than a text button in the
 * action row: the root's head already holds its two thread-level actions as
 * icons, and Edit is the third.
 *
 * @param {(key: string) => string} t
 * @param {() => unknown} action
 * @returns {HTMLButtonElement}
 */
export function commentEditButton(t, action) {
  const button = reviewIconButton("edit", t("reviewCard.editComment"), action);
  button.dataset.testid = "review-comment-edit-open";
  return button;
}

/**
 * Which comment is being edited, kept OUTSIDE the card.
 *
 * An expanded card is rebuilt on every review render (`renderReviewMarginItems`),
 * so an editor held only in the card's DOM disappears the moment anything else
 * repaints the column — taking the half-typed text with it. The id and the draft
 * live here instead, and the rebuilt card asks [`editing`] whether to draw the
 * editor and with what text.
 *
 * Complexity: O(1).
 */
export function createCommentEditState() {
  let current = null;
  return Object.freeze({
    /** Starts editing `id` from `text`. */
    begin(id, text) {
      current = { id: String(id), draft: String(text ?? "") };
    },
    /** The draft for `id`, or `null` when `id` is not being edited. */
    draftFor(id) {
      return current && current.id === String(id) ? current.draft : null;
    },
    /** Records what the author has typed so far. */
    update(id, text) {
      if (current && current.id === String(id)) current.draft = String(text ?? "");
    },
    /** Ends editing, saved or not. */
    end() {
      current = null;
    },
    /** The id being edited, or `null` — `reviewCardSignature`'s `editingId`, so
     *  a cached card is rebuilt into, and out of, its editor. */
    id: () => current?.id ?? null,
  });
}
