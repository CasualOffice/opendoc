// Who may DECIDE a tracked change: one answer for the chrome and the API (HF-199).
//
// Accepting a suggestion writes it into the document outright; rejecting one
// discards somebody's text. Both are editing, not commenting. Word's reviewer
// and Google Docs' Commenter cannot do either, and the host contract has said so
// since SDK phase 2: every `review.accept*` / `review.reject*` row requires
// `edit` (`host_contract.mjs`). The chrome let a `commentor` container accept and
// reject anyway, so one capability had two answers depending on whether a host
// asked through the API or a person clicked a button. ONLYOFFICE ties the same
// buttons to its review permission, not to commenting: a change's popover is a
// bare hint without Accept/Reject when `!canReview`
// (`web-apps/apps/common/main/lib/controller/ReviewChanges.js:539`).
//
// THE CONTRACT IS THE AUTHORITY, and this module only reads it: the requirement
// comes from `commandContract(id).requires`, resolved through
// `grantsRequirement`, so a later change to what deciding needs is one row in
// `host_contract.mjs` and both doors follow it.
//
// Scope, deliberately narrow: the decision commands only. Every other command's
// container gate already reaches the chrome through the review MODE (a container
// without `edit` or `comment` is in Viewing), and narrowing the whole registry by
// the contract here would be a second enforcement of that rule.
//
// A room's participant grant is a different authority (`session_access.mjs`)
// and is applied before this one, as `reflectReviewSurface` documents.
import { t } from "./i18n.mjs";
import { commandContract, grantsRequirement } from "./host_contract.mjs";

/** The id the decision requirement is read from when a surface names a
 *  decision by a non-contract id (the context menu's `review.accept`). */
const DECISION_ANCHOR = "review.acceptAll";

/** True for an id that decides a tracked change: `review.accept…` or
 *  `review.reject…` on a camel or dotted boundary, so `review.acceptance` would
 *  not be captured. */
export function isReviewDecision(id) {
  return /^review\.(?:accept|reject)(?:$|[A-Z.])/.test(String(id ?? ""));
}

/**
 * The sentence a decision control carries while the container withholds what
 * deciding needs, or `""` when it may decide (and for any other command).
 *
 * A STRING, not a boolean, for the reason `session_access.mjs` gives: a caller
 * cannot gate on it and forget to say why.
 *
 * Complexity: O(1).
 *
 * @param {string} id the command id
 * @param {{has: (name: string) => boolean}} capabilities the container's grant
 * @returns {string}
 */
export function reviewDecisionRefusal(id, capabilities) {
  if (!isReviewDecision(id)) return "";
  const requires = (commandContract(id) ?? commandContract(DECISION_ANCHOR))?.requires ?? "edit";
  return grantsRequirement(requires, capabilities) ? "" : t("review.decision.needsEdit");
}

/**
 * `commands` with every decision row the container withholds disabled, with
 * its reason. A NEW array; rows only ever move from enabled to disabled.
 *
 * Complexity: O(commands).
 */
export function narrowDecisionsToContainer(commands, capabilities) {
  return commands.map((row) => {
    const refusal = reviewDecisionRefusal(row?.id, capabilities);
    return refusal ? { ...row, enabled: false, disabledReason: refusal } : row;
  });
}

/**
 * The choke point every decision path passes before it reaches the engine:
 * `false`, with the reason said, when the container withholds deciding.
 *
 * Every surface above also disables itself, but a disabled control is a
 * courtesy and this is the rule — a stale card, a chord, a host script calling
 * the chrome's own function all end here.
 *
 * @param {{has: (name: string) => boolean}} capabilities
 * @param {(text: string, kind?: string) => void} setStatus
 * @returns {boolean} whether the decision may go ahead
 */
export function mayDecideReview(capabilities, setStatus) {
  const refusal = reviewDecisionRefusal(DECISION_ANCHOR, capabilities);
  if (!refusal) return true;
  setStatus(refusal, "error");
  return false;
}
