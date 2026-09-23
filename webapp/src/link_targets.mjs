// Which link targets this host is willing to hand to the browser.
//
// Extracted from `main.js` (`109` HF-085) because it is a security predicate and
// a pure function: link targets arrive from imported documents, pasted HTML and
// the link dialog, and are followed from three surfaces. Having it in a module
// is what lets the allowlist be tested directly rather than through a click.

/** The schemes this host is willing to hand to the browser. Anything else in a
 * link target — `javascript:`, `data:`, `file:`, a custom app scheme — is a
 * capability an imported document must not be able to reach. */
export const EXTERNAL_LINK_SCHEMES = new Set(["http:", "https:", "mailto:"]);

/** Resolves a document-supplied URL to something safe to navigate to, or null.
 *
 * Link targets arrive from imported files, pasted HTML and the link dialog, and
 * are followed from three surfaces (the chip's Open, a click on the text, the
 * context menu). The allowlist used to live inside `activateLink` only, so the
 * context menu opened `javascript:` unchecked — the same capability, guarded on
 * one surface. Every follow path resolves here now, so there is one predicate
 * to change when the policy does. */
export function resolveExternalTarget(url) {
  if (typeof url !== "string" || !url.trim()) return null;
  let target;
  try {
    target = new URL(url, window.location.href);
  } catch {
    return null;
  }
  return EXTERNAL_LINK_SCHEMES.has(target.protocol) ? target : null;
}

/** What to tell the reader when a target is refused.
 *
 *  Policy, so it lives with the allowlist it belongs to: a blocked link has to
 *  SAY it was blocked — a link that silently does nothing is indistinguishable
 *  from a broken editor — and naming the scheme is what makes the refusal
 *  checkable rather than mysterious.
 */
export function blockedTargetMessage(url) {
  let scheme = "";
  try {
    scheme = new URL(url, window.location.href).protocol;
  } catch {
    scheme = "";
  }
  return scheme ? `Blocked ${scheme} link scheme` : "Blocked an invalid link target";
}

/** Follows an allowlisted target and reports the refusal when there is not one.
 * A blocked link says so — a link that silently does nothing is indistinguishable
 * from a broken editor. `report` is the host's status reporter, injected so this
 * whole policy — allowlist, refusal wording and navigation — is one testable
 * unit rather than three places that have to agree. `noreferrer` accompanies `noopener` everywhere: the
 * context-menu path used to leak the referrer that the click path withheld. */
export function followExternalTarget(url, report) {
  const target = resolveExternalTarget(url);
  if (!target) {
    report(blockedTargetMessage(url), "error");
    return false;
  }
  if (target.protocol === "mailto:") window.location.assign(target.href);
  else window.open(target.href, "_blank", "noopener,noreferrer");
  return true;
}
