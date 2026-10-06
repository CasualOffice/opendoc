// A disabled control says WHY, in the sentence the palette already uses for the
// same precondition (`SKILL` §10: never a dead control).
//
// Four controls greyed out with nothing to say: Restart and Continue numbering on
// Home, the contextual Table tab, and Insert ▸ Link — while the palette row for
// each of the same commands already carried a `disabledReason`. A reader who
// meets the button first cannot tell "not here" from "broken". `title` is the one
// channel a disabled button has (it takes no focus and fires no events), and it
// is the accessible DESCRIPTION of a control that has a name, so a screen reader
// hears it too. The Table band and the Layout/References/Review sweeps already
// work this way; this is the same rule for the controls those sweeps do not own.
//
// One mechanism, so the sentence a button shows and the sentence its palette row
// shows are one catalogue entry each rather than two strings that drift.
import { authoredTitle } from "./localize.mjs";
import { t } from "./i18n.mjs";

/** Why an Insert-surface control is unavailable, by the precondition it
 *  declares (`INSERT_SURFACE`'s `requires`). Link is the one that needs a range. */
export const INSERT_REASON_KEYS = Object.freeze({
  range: "insert.reason.linkNeedsText",
  doc: "command.needsDocument",
});

/**
 * Restart / Continue numbering: whether each can run at the caret, and if not,
 * which sentence says why. Pure; `canContinue` is asked only when the caret IS
 * in a numbered list, because it is an engine call and the answer is moot
 * otherwise.
 *
 * @param {{hasCaret: boolean, listKind: string, canContinue: () => boolean}} state
 */
export function listNumberingStates({ hasCaret, listKind, canContinue }) {
  const numbered = !!hasCaret && listKind === "numbered";
  const continuable = numbered && !!canContinue();
  return {
    restart: { enabled: numbered, reasonKey: "list.reason.notNumbered" },
    continue: {
      enabled: continuable,
      // Two different reasons, and the reader needs the right one: not in a
      // numbered list at all, or in one with nothing earlier to resume.
      reasonKey: numbered ? "list.reason.nothingToContinue" : "list.reason.notNumbered",
    },
  };
}

/**
 * Enables or disables `control`, and makes its tooltip the REASON while it is
 * disabled and its own title again when it is not.
 *
 * A control with no authored title (the Table tab) gets its tooltip removed when
 * enabled rather than keeping a stale reason: the snapshot `authoredTitle` falls
 * back to is taken the first time this sees the control, before any reason has
 * been written into `title`.
 *
 * Complexity: O(1).
 */
export function reflectEnablement(control, { enabled, reasonKey }, platform) {
  if (!control) return;
  if (control.dataset.enabledTitle === undefined) control.dataset.enabledTitle = control.getAttribute("title") ?? "";
  control.disabled = !enabled;
  const title = enabled ? authoredTitle(control, platform) : t(reasonKey);
  if (title) control.setAttribute("title", title);
  else control.removeAttribute("title");
}
