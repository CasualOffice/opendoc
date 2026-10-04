// About (HF: "which build am I on?").
//
// Extracted from `main.js` (`109` HF-085). There was no About anywhere in the
// product: no version, no licence, no way for someone reporting a bug to say
// which build they were on. The version comes from the engine's own
// `engineVersion()`, compiled from its crate manifest, so it cannot drift from
// what actually shipped.

import { registerModal } from "./modal.mjs";
import { BRAND } from "./brand.mjs";

/** The customer block's fields, in the order they are shown, and how each one is
 *  rendered.
 *
 *  ONLYOFFICE's `customization.customer` (`api.js:151-160`) put here for the same
 *  reason theirs is in their About box: this is the one surface in an editor
 *  whose job is saying who made and who runs this thing, and a white-label whose
 *  deployment cannot say "this is run by Northwind, here is how to reach them"
 *  is a white-label in name only.
 *
 *  NOT LOCALISED, and deliberately. Every value is host text in the host's own
 *  language — a company name, a street address, a sentence about themselves — so
 *  there is no key to route and no twentieth catalogue entry. `docs/124`'s rule
 *  is about strings WE write; `frame-title` on the embed element is the same
 *  decision, and it is the reason the `feedback` and `help` links carry their own
 *  `text`. The only English here would be labels, and they are not written:
 *  a postal address, a phone number and a website announce themselves. */
const CUSTOMER_FIELDS = Object.freeze([
  Object.freeze({ key: "name", as: "text", className: "about-customer-name" }),
  Object.freeze({ key: "info", as: "text", className: "about-customer-info" }),
  Object.freeze({ key: "address", as: "text", className: "about-customer-line" }),
  Object.freeze({ key: "www", as: "link" }),
  Object.freeze({ key: "mail", as: "link" }),
  Object.freeze({ key: "phone", as: "link" }),
]);

/** An external link, with the rel/target the three existing About links already
 *  carry. `textContent`, never `innerHTML`: the label came out of somebody's
 *  `brand.json`. */
function link(href, label) {
  const anchor = document.createElement("a");
  anchor.href = href;
  anchor.rel = "noreferrer noopener";
  anchor.target = "_blank";
  anchor.textContent = label;
  return anchor;
}

/**
 * Renders the deployment's own identity into About, or nothing at all.
 *
 * Nothing at all is the shipped case: `brand.json` is all-null, so the default
 * build is the product and a white-label is visibly a decision somebody made
 * (`docs/126` phase 3). An empty section with a heading would be chrome paid for
 * by every deployment that never configured one.
 *
 * Idempotent — it replaces its own container — because About is stamped on every
 * open and a second call must not double the block.
 *
 * Complexity: O(fields). Nothing here reads the document.
 */
export function renderBrandIdentity(root = document) {
  const body = root.getElementById?.("aboutDialog")?.querySelector(".about-body");
  if (!body) return null;
  const existing = root.getElementById?.("aboutCustomer");
  existing?.remove();
  const { customer, feedback, help } = BRAND;
  if (!customer && !feedback && !help) return null;

  const section = document.createElement("section");
  section.id = "aboutCustomer";
  section.className = "about-customer";

  if (customer?.logo) {
    // ONE element for both themes, painted from a token, rather than two `<img>`
    // elements one of which is always wrong: the dark rule falls back to the
    // light value, so a host who supplied only `logo` gets it in both themes and
    // a host who supplied both gets the right one in all three theme entry
    // points. Two elements is the shape of `docs/104` HF-092 — a fix that
    // shipped in one block and not the other.
    const mark = document.createElement("span");
    mark.className = "about-customer-mark";
    mark.setAttribute("aria-hidden", "true");
    mark.style.setProperty("--about-customer-mark", `url("${customer.logo}")`);
    if (customer.logoDark) mark.style.setProperty("--about-customer-mark-dark", `url("${customer.logoDark}")`);
    section.append(mark);
  }
  for (const field of CUSTOMER_FIELDS) {
    const value = customer?.[field.key];
    if (!value) continue;
    if (field.as === "link") {
      const row = document.createElement("p");
      row.className = "about-customer-line";
      row.append(link(value, value));
      section.append(row);
      continue;
    }
    const row = document.createElement("p");
    row.className = field.className;
    row.textContent = value;
    section.append(row);
  }
  if (feedback || help) {
    const row = document.createElement("p");
    row.className = "about-links";
    // The host's own label, because the destination is theirs: ONLYOFFICE's
    // `feedback` has no text at all and shows their English, which is exactly the
    // thing a white-label cannot afford.
    if (help) row.append(link(help.url, help.text ?? help.url));
    if (feedback) row.append(link(feedback.url, feedback.text ?? feedback.url));
    section.append(row);
  }
  body.append(section);
  return section;
}

/**
 * Wires the About dialog and returns its toggle.
 *
 * @param {() => string} engineVersion the engine's own version getter.
 * @param {() => Element} fallbackFocus where focus goes when the dialog closes
 *        and the thing that opened it is gone.
 * @returns {(open: boolean) => void}
 */
export function createAboutDialog(engineVersion, fallbackFocus) {
  const dialog = document.getElementById("aboutDialog");
  const close = document.getElementById("aboutClose");
  const modal = dialog
    ? registerModal(dialog, { initialFocus: () => close, fallbackFocus })
    : null;

  /** Stamps the engine version into the panel. Shared with the File page's
   *  About PANE, which shows the same element without opening the dialog. */
  const stampVersion = () => {
    const slot = document.getElementById("aboutVersion");
    if (slot) {
      // The engine may not have booted yet — About is a `noDoc` command, so it
      // is reachable from the very first frame. Say so rather than printing a
      // placeholder that reads like a version.
      let version = "";
      try {
        version = engineVersion();
      } catch {
        version = "";
      }
      slot.textContent = version || "not loaded yet";
    }
    // The deployment's own identity, stamped beside the version for the same
    // reason and at the same moment: the File page's About PANE shows this
    // element without opening the dialog, so anything only the dialog rendered
    // would be missing there.
    renderBrandIdentity(document);
  };

  const toggle = (open) => {
    if (!modal) return;
    if (!open) {
      modal.close();
      return;
    }
    stampVersion();
    modal.open();
  };

  // Both dismissals, not just the X: About has a body and now has an action row
  // under it too (HF-233 D3), and `[data-about-dismiss]` is how one handler
  // serves both so neither can be the one that is wired and neither forgotten.
  for (const button of dialog?.querySelectorAll("[data-about-dismiss]") ?? []) {
    button.addEventListener("click", () => toggle(false));
  }
  // The stamp comes back too: the File page's About PANE shows this same
  // element without opening the dialog, and an About that says "not loaded
  // yet" forever would be worse than no pane.
  toggle.stampVersion = stampVersion;
  return toggle;
}
