// The editor's feedback channel: one place that decides how a status message
// reaches a person, and the DOM half of `status_policy.mjs`.
//
// `109` UX-017. Before this there was exactly one channel — painted text in
// `.foot-left` — and both live regions were INSIDE it. `.foot-left` is
// `display: none` below 620px, and a `display: none` subtree is not in the
// accessibility tree, so below that width a refusal was invisible *and*
// unspeakable: the editor did nothing and said nothing. Measured through
// Chromium's own AX tree (CDP `Accessibility.getFullAXTree`), the string
// "Viewing mode is read-only; switch to Editing to change the document" appeared
// in 4 nodes at 1280px and in **0 nodes** at 390px.
//
// The competitive shape, read from source rather than from docs:
//
//   * ONLYOFFICE keeps its announcement region at BODY level and nowhere else —
//     `common/main/lib/util/ScreenReaderHelper.js:43-46` appends
//     `<div aria-live="assertive" class="sr-only">` to `$('body')`, so no layout
//     rule can prune it. It also clears the node and writes the text back on a
//     later tick (`:61-64`) so a repeated identical message is still a change
//     worth announcing. Both details are adopted here; the second one this
//     editor already had.
//   * A refusal itself is answered with a modal: 31 `Common.UI.warning` calls in
//     the document editor (`Common.UI.warning` is defined at
//     `common/main/lib/component/Window.js:666`), e.g. a rejected font size at
//     `documenteditor/main/app/controller/Toolbar.js:1521-1528`, which also puts
//     focus back in the field. Word does the same.
//   * Google Docs answers with a snackbar pinned to the viewport, not to the
//     chrome, and therefore present at every width.
//
// What is built here is the Docs shape rather than the modal: 124 `setStatus`
// call sites cannot each become a dialog that has to be dismissed, and a modal
// per refused keystroke would be worse than the silence it replaced. The toast
// is viewport-anchored, never focusable, `pointer-events: none`, and
// `aria-hidden` — the live region is the single thing that speaks, so a screen
// reader hears the message once, not twice.

import {
  announcementRegion,
  backgroundProgressMayPaint,
  needsToast,
  statusClassName,
  toastDuration,
} from "./status_policy.mjs";

/** The marker a background-progress line carries on the status element, so the
 *  channel can tell its own writing from the reader's without keeping a second
 *  copy of the string. A `data-` attribute rather than a module variable
 *  because the DOM is where the line actually is: a host that clears the footer,
 *  or a reload, cannot leave the flag disagreeing with the text. */
const PROGRESS_FLAG = "statusProgress";

/**
 * Wires the three feedback surfaces together.
 *
 * `statusLine` is the visible footer line; the channel asks whether it is
 * actually rendered rather than assuming a breakpoint, so a host that hides the
 * footer for its own reasons gets the toast too and nothing has to know the
 * media query exists.
 *
 * All O(1) per message, and messages arrive at human rate.
 */
export function createStatusChannel({ live, alert, toast, statusLine }) {
  let hideTimer = 0;

  /** Whether the status line is on screen. `checkVisibility` answers the real
   *  question — is this painted — where a width check would only answer "is the
   *  breakpoint the one I remembered". */
  function statusLineVisible() {
    if (!statusLine) return false;
    if (typeof statusLine.checkVisibility === "function") {
      return statusLine.checkVisibility({ contentVisibilityAuto: true, opacityProperty: true });
    }
    return statusLine.getClientRects().length > 0;
  }

  function hideToast() {
    clearTimeout(hideTimer);
    hideTimer = 0;
    if (!toast) return;
    toast.classList.remove("is-shown");
    toast.hidden = true;
    toast.textContent = "";
    toast.removeAttribute("data-kind");
  }

  function showToast(text, kind) {
    if (!toast) return;
    clearTimeout(hideTimer);
    toast.textContent = text;
    if (kind) toast.dataset.kind = kind;
    else toast.removeAttribute("data-kind");
    toast.hidden = false;
    // Re-run the entrance for a repeated identical message: the same edit
    // refused twice must read as twice, and a card that is already on screen
    // would otherwise look like nothing happened.
    toast.classList.remove("is-shown");
    void toast.offsetWidth;
    toast.classList.add("is-shown");
    hideTimer = window.setTimeout(hideToast, toastDuration(kind));
  }

  /** Speaks `text`, and nothing else. `announcementRegion` picks the region. */
  function announce(text, kind = "") {
    const region = announcementRegion(kind) === "assertive" ? alert : live;
    // Only one of the two regions may hold text, or a screen reader browsing
    // them meets the last error long after it stopped being true.
    if (live) live.textContent = "";
    if (alert) alert.textContent = "";
    if (!text || !region) return;
    // Clear-then-write-next-frame, as ONLYOFFICE does on a later tick: a live
    // region only announces a CHANGE, and the same refusal twice is the same
    // text.
    requestAnimationFrame(() => {
      region.textContent = text;
    });
  }

  return {
    /**
     * Publishes `text` to every channel that should carry it — the regions, and
     * the toast when the status line cannot be seen or the message is a refusal.
     * This is the entry point for the status line's own messages.
     *
     * An empty `text` is a clear, not a message: both regions empty, so a stale
     * refusal cannot be found in the accessibility tree long after it stopped
     * being true. A clear does NOT take down a toast that is still showing — see
     * the rule below.
     */
    publish(text, kind = "") {
      // The reader's message owns the line from here. Clearing the flag is what
      // stops a later background pass deciding the line is still its own and
      // painting over an answer the reader is reading.
      if (statusLine) delete statusLine.dataset[PROGRESS_FLAG];
      announce(text, kind);
      if (text && needsToast(kind, statusLineVisible())) showToast(text, kind);
      // Otherwise the toast is LEFT ALONE to finish its dwell, and that is the
      // rule rather than an oversight: a message that does not itself need the
      // toast has been judged not-must-notice, so it must not evict one that is.
      //
      // Without this a refusal was destroyed by the informational line that
      // followed it inside the same command. Review ▸ "Accept change and move to
      // next" with the caret outside a change does exactly that: it refuses
      // ("Place the caret inside a tracked change…") and then advances, which
      // reports "This document has no comments or tracked changes". The user saw
      // the reason for the refusal appear and vanish in one tick, and the message
      // that survived was the one that did not matter.
    },
    /**
     * Speaks `text` without touching the on-screen channels.
     *
     * For the callers that already own a visible surface of their own — the
     * recovered-work bar is on screen at every width — so a toast would be a
     * second copy of something the user is already looking at.
     */
    announce,
    /**
     * A BACKGROUND task's progress — a font download, not something the reader
     * asked for.
     *
     * It is deliberately the smallest of the three surfaces: the status line
     * and nothing else. No live region, because a screen reader being told
     * "Fetching fonts…" is being interrupted by a detail it cannot act on; no
     * toast, because a card over the document is reserved for must-notice
     * messages and this is not one; and the caller does not route it to the host
     * either, because a host listening for editor status would re-issue a font
     * fetch as if the editor had refused something.
     *
     * Paints only when `backgroundProgressMayPaint` allows it, so a refusal the
     * reader just earned is never replaced by progress on work they did not
     * start. An empty `text` retires the line, but only if it is still the
     * line this channel wrote.
     *
     * @returns whether the line was painted, so a caller can tell "shown" from
     *   "the reader owns the line" rather than assuming.
     */
    progress(text) {
      if (!statusLine) return false;
      const mine = statusLine.dataset[PROGRESS_FLAG] === "1";
      if (!backgroundProgressMayPaint(statusLine.textContent, mine)) return false;
      statusLine.textContent = text;
      statusLine.className = statusClassName("");
      if (text) statusLine.dataset[PROGRESS_FLAG] = "1";
      else delete statusLine.dataset[PROGRESS_FLAG];
      return true;
    },
  };
}
