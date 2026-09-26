// What the editor says about itself: which live region hears a status message,
// what the document-state pill reads, and what the browser tab is called.
//
// The same shape as `edit_errors.mjs`, and extracted for the same reason (`109`
// HF-085): these are pure decisions that were only reachable through a DOM
// write inside an 18k-line module, so the one thing worth testing about them —
// the DECISION, not the assignment — could not be tested at all. There are ~117
// `setStatus` call sites and every one of them routes through the two rules at
// the top of this file.
//
// It is also the first string table the editor owns as data rather than as
// literals buried in call sites, which is the shape `109` HF-081 (the i18n
// seam) needs before it can begin.

/** Failures go to the assertive region because a refusal the user cannot hear
 *  reads as the editor doing nothing; everything else is polite and waits its
 *  turn. `kind` is the status class the caller passed — `""`, `"ok"`,
 *  `"error"`. */
export function announcementRegion(kind) {
  return kind === "error" ? "assertive" : "polite";
}

/** The status bar's class attribute for a message of this kind. */
export function statusClassName(kind) {
  return `status ${kind}`;
}

/** The document-state pill: icon and wording per state.
 *
 *  The pill is display only. Whether work would be LOST is answered by the
 *  engine revision watermark, deliberately not by this string — which is what
 *  let an older boolean flag sit here with no readers. */
export const DOCUMENT_STATE_BADGES = {
  opened: { icon: "check_circle", text: "Opened", key: "status.state.opened" },
  edited: { icon: "edit", text: "Edited", key: "status.state.edited" },
  downloaded: { icon: "download_done", text: "Downloaded", key: "status.state.downloaded" },
};

/**
 * The pill for `state`, falling back to `opened` for anything unknown.
 *
 * @returns {{state: string, icon: string, text: string}} `state` is the name to
 *   stamp on the element — the FALLBACK's name when the input was unknown, so
 *   the attribute and the text can never disagree.
 */
export function documentStateBadge(state) {
  const known = Object.hasOwn(DOCUMENT_STATE_BADGES, state);
  const badge = known ? DOCUMENT_STATE_BADGES[state] : DOCUMENT_STATE_BADGES.opened;
  return { state: known ? state : "opened", ...badge };
}

/**
 * The browser tab's title for the open document.
 *
 * The convention is the platform's, not an invention: the document name FIRST
 * (a tab strip truncates from the right, so anything before the name is what
 * survives and the name is what the user is scanning for), then the app, then a
 * leading `•` when there is unsaved work — the web's equivalent of Word's
 * title-bar asterisk and the dot Docs shows while saving.
 *
 * With no document open there is no name to show, so the page's own static
 * title stands; that is `fallback`.
 */
export function documentTabTitle({ name, dirty = false, fallback = "OpenDoc" }) {
  if (!name) return fallback;
  return `${dirty ? "• " : ""}${name} — OpenDoc`;
}

/** Whether a message needs the transient on-screen channel — the toast — as
 *  well as the status line.
 *
 *  Two independent reasons, and `109` UX-017 is the first of them:
 *
 *  1. **The status line is not on screen.** Below 620px `.foot-left` is
 *     `display: none`, so the line that carries every refusal paints nothing and
 *     is pruned from the accessibility tree with it. A channel that disappears
 *     is not a channel.
 *  2. **The message is a refusal.** The user's action did not happen, and a 12px
 *     line in the footer is somewhere the answer is *available* rather than
 *     somewhere it is *noticed*. Word and ONLYOFFICE both answer a refusal with
 *     a modal (ONLYOFFICE: 31 `Common.UI.warning` sites in the document editor
 *     alone — `documenteditor/main/app/controller/Toolbar.js:1521-1528` is a
 *     rejected font size, which even restores focus to the field), and Google
 *     Docs with a viewport-anchored snackbar. Escalating errors to a transient
 *     card is the cheaper half of that agreement; a modal per refused keystroke
 *     would be worse than the defect.
 *
 *  Confirmations do not escalate while the line is showing them, or every
 *  toolbar press would throw a card over the document. */
export function needsToast(kind, statusLineVisible) {
  return kind === "error" || !statusLineVisible;
}

/** How long a toast stays, in milliseconds.
 *
 *  A refusal outlives a confirmation because it has to be *read* — it names a
 *  reason and usually a way out ("switch to Editing"). Both values sit in the
 *  range Google Docs' snackbar uses; neither is long enough to be litter. */
export function toastDuration(kind) {
  return kind === "error" ? 6000 : 3500;
}

/** Whether a status line is one an object selection put there.
 *
 *  Clearing the status bar when the selection goes away must not wipe a message
 *  somebody else wrote — a save confirmation, or the reason an edit was
 *  refused — so the clear is scoped to the lines object selection itself
 *  produces. */
export function isObjectSelectionStatus(text) {
  return /^(Image|Shape|Text box|Object) selected/.test(text ?? "");
}
