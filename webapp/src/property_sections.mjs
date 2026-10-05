// SPDX-License-Identifier: Apache-2.0
// Grouped, collapsible sections for the properties panels — the one mechanism
// all three of them share (`105` UX-027, ADR-062's "selection property" row).
//
// ---- THE COMPLAINT THIS ANSWERS -------------------------------------------
//
// The owner, verbatim: "all properties panel is pathetic who designed it.. i
// dont makes any sense ..a nd alot of cognative burden". Measured before this
// module existed, at the default state each panel opens in, in a 320px column:
//
//   paragraph properties   5 sections, 0 collapsible, 22 controls on screen
//   table properties       5 sections, 0 collapsible, 17 controls on screen
//   object properties      7 sections, 0 collapsible, 12 for an image and 23
//                          for a text box
//
// Nothing anywhere in the editor folded except one `<details>` in the Document
// properties dialog. A reader opening a panel to change one number had to read
// past every other number to find it, and the panel's own scroll height did the
// rest.
//
// ---- THE ESTABLISHED PATTERN, NAMED BEFORE THE CODE -----------------------
//
// Progressive disclosure, and specifically the **property sheet with
// collapsible sections per concern** that Word's Format Picture pane and
// Google Docs' image sidebar use. This repository already had the right
// primitive in the tree and used it in exactly one place — `<details>` +
// `<summary>`, `editor.html`'s `.metadata-disclosure` — so this is that
// primitive applied to the family, not a new component. Native `<details>` is
// why there is no `aria-expanded` bookkeeping here: the element already
// announces itself as a disclosure, is operable from the keyboard, and reports
// its own state.
//
// NOT ONLYOFFICE's answer, and the difference is worth stating because the
// owner asked about theirs specifically. Read at `reference/web-apps`
// 9c0ca538c3b211052347df09d2a4d6781f023403: they have no collapsible section
// anywhere in the right panel — the panes are mutually exclusive, switched by a
// vertical icon rail (`app/view/RightMenu.js:397-398`), and inside a pane the
// sections are bold labels with a rule between them and everything is open
// (`resources/less/common.less:444-446`). Their image pane carries 12 controls
// and their table pane 30, all at once. Their separation is a SECOND SURFACE:
// an "Advanced settings" link at the foot of each pane
// (`app/template/ImageSettings.template:118`) opening a dialog with its own
// category list, and what goes behind it is everything numeric — their image
// pane cannot edit width or height at all (`app/view/ImageSettings.js:113-120`).
//
// A fold is preferred to a second surface here because the properties this
// panel carries ARE the numeric ones, and sending a reader to a modal to type a
// width is the round trip the panel exists to avoid.
//
// ---- FOLDED MUST NOT MEAN HIDDEN -----------------------------------------
//
// A fold that loses information is worse than the wall it replaced, because a
// reader then has to open every section to find out which one holds the value
// they are after. So a closed section carries a DIGEST of its own controls'
// current values in its summary — derived from the controls themselves, so no
// panel has to maintain a second description of its own state, and a panel
// whose reflect loop lives in `main.js` gets it without a line of wiring there.
//
// ---- WHY `focus` IS WRAPPED ----------------------------------------------
//
// Four ribbon rows and two palette commands reveal a property by FOCUSING its
// control: Layout ▸ Wrap text focuses the wrap select, Layout ▸ Indent focuses
// the left-indent field, and so on (ADR-062 corollary C1, "a pointer must
// point"). `HTMLElement.focus()` on an element inside a closed `<details>` does
// nothing at all — so collapsing a section silently breaks every one of those
// pointers, which is the exact defect C1 exists to forbid.
//
// The reveal therefore lives on the CONTROL, not in each caller: `focus` is
// wrapped per element so opening the ancestor sections happens first, whoever
// calls it. The alternative was to edit every call site, most of which are in
// `main.js` — a file at its line ratchet and owned by another lane — and a
// rule that depends on each caller remembering it is a rule that breaks on the
// next caller. One job, one mechanism (ADR-062 C3).
//
// Pure of browser globals: every entry point takes its root element and its
// storage host, so `property_sections.test.mjs` drives the whole policy in node
// against a hand-built DOM stub.
import { loadPrefObject, savePrefObject } from "./prefs.mjs";

/** Where a reader's folded/unfolded choices are kept. One key, one object, so
 *  three panels' worth of sections cost one storage read at open. */
export const SECTIONS_PREF_KEY = "opendoc.propertySections";

/** The controls a digest can read a value from, and that `focus` is wrapped on.
 *  `button` is included for the segmented groups (alignment, borders), whose
 *  state is `aria-pressed` rather than a value. */
const FOCUSABLE = "input, select, textarea, button";

/** How long a digest may be before it is cut. A 320px summary row holds roughly
 *  this many characters beside a title and a chevron; past that the title
 *  starts to ellipsise instead, which loses the more important half. */
const DIGEST_LIMIT = 30;

/** The separator between a digest's parts. A middle dot rather than a comma,
 *  because several of the values ARE comma-or-point decimals depending on the
 *  reader's locale (`measurement_units.mjs`) and two meanings for one glyph in
 *  one string is unreadable. */
const DIGEST_JOIN = " · ";

/** `section` -> its stable id, which is what the preference is keyed on. */
function sectionId(section) {
  return section.getAttribute("data-property-section") ?? "";
}

/** Every collapsible section under `root`, in document order. */
export function propertySections(root) {
  return root ? [...root.querySelectorAll("[data-property-section]")] : [];
}

/**
 * One control's contribution to its section's digest, or `""` for a control
 * that has nothing a reader would want to see folded.
 *
 * Deliberately NOT every control. A text field (a caption, an alt-text
 * description, a formula) can be a sentence, and a sentence in a 30-character
 * summary is a truncation that tells a reader less than the field's label
 * already did. A colour swatch has no text at all. What is worth folding is
 * the SHORT answers: numbers with their unit, a chosen option, a switch that is
 * on, a pressed button in a segmented group.
 */
function controlDigest(control) {
  if (control.disabled) return "";
  const tag = control.tagName.toLowerCase();
  if (tag === "select") {
    const option = control.options?.[control.selectedIndex];
    return option && option.value !== "" ? (option.textContent ?? "").trim() : "";
  }
  if (tag === "button") {
    // A segmented group reports the pressed member's name. `aria-pressed` and
    // not a class, because the group's state is already published there for a
    // screen reader and reading it twice from two places is how the two drift.
    if (control.getAttribute("aria-pressed") !== "true") return "";
    return (control.getAttribute("aria-label") ?? control.textContent ?? "").trim();
  }
  if (tag !== "input") return "";
  if (control.type === "checkbox") {
    if (!control.checked) return "";
    // The `<strong>` is the switch's own name; its `<small>` is the sentence
    // explaining it, which belongs in the open section and not in a summary.
    const name = control.parentElement?.querySelector("strong");
    return (name?.textContent ?? "").trim();
  }
  if (control.type !== "number") return "";
  const value = control.value.trim();
  if (value === "") return "";
  // The unit travels with the number. A measurement with no unit in a product
  // whose unit is a preference is a number a reader cannot act on.
  const unit = control.parentElement?.querySelector("span:not([data-property-digest])");
  const suffix = (unit?.textContent ?? "").trim();
  return suffix ? `${value} ${suffix}` : value;
}

/**
 * The digest text for one section, derived from its own controls.
 *
 * Complexity: O(controls in the section) — a dozen at most, recomputed when one
 * of them changes or when the panel re-reads the model. Never over the document.
 *
 * @param {Element} section
 * @returns {string}
 */
export function digestFor(section) {
  const parts = [];
  for (const control of section.querySelectorAll(FOCUSABLE)) {
    if (control.closest("[data-property-digest-skip]")) continue;
    const part = controlDigest(control);
    if (part) parts.push(part);
  }
  const text = parts.join(DIGEST_JOIN);
  if (text.length <= DIGEST_LIMIT) return text;
  // Cut on a part boundary rather than mid-number: "3.2 in · 2.4 i" reads as a
  // different measurement, which is worse than saying less.
  let kept = "";
  for (const part of parts) {
    const next = kept ? kept + DIGEST_JOIN + part : part;
    if (next.length > DIGEST_LIMIT) break;
    kept = next;
  }
  return kept ? `${kept}…` : `${text.slice(0, DIGEST_LIMIT)}…`;
}

/** Writes every section's digest under `root`. Safe to call on every repaint. */
export function paintPropertyDigests(root) {
  for (const section of propertySections(root)) {
    const slot = section.querySelector("[data-property-digest]");
    if (!slot) continue;
    // Only a CLOSED section's digest is shown. Open, the values are right
    // there, and a summary repeating them is noise competing with the title.
    slot.textContent = section.open ? "" : digestFor(section);
  }
}

/**
 * Opens every collapsible section `control` sits inside, outermost first.
 *
 * Returns the sections it had to open, so a caller — and a guard — can tell a
 * reveal that did something from one that found the section already open.
 *
 * @param {Element|null} control
 * @returns {Element[]}
 */
export function revealPropertyControl(control) {
  const opened = [];
  for (let node = control; node; node = node.parentElement) {
    if (node.matches?.("[data-property-section]") && !node.open) {
      node.open = true;
      opened.push(node);
    }
  }
  // Outermost first: a nested section's own ancestors must already be open for
  // its content to be laid out at all.
  return opened.reverse();
}

/**
 * Wraps `focus` on every control inside a collapsible section so that focusing
 * it opens the section first. See the header for why this is not done at the
 * call sites.
 *
 * Idempotent: a control already wrapped is left alone, because `bind` runs
 * again whenever a panel rebuilds its fields and double-wrapping would open the
 * ancestors twice and lose the native method.
 */
function wrapFocusReveal(root) {
  for (const control of root.querySelectorAll(FOCUSABLE)) {
    if (control.dataset.propertyFocusReveal === "1") continue;
    if (!control.closest("[data-property-section]")) continue;
    const native = control.focus.bind(control);
    control.focus = (options) => {
      revealPropertyControl(control);
      native(options);
    };
    control.dataset.propertyFocusReveal = "1";
  }
}

/**
 * Binds `root`'s collapsible sections: restores the reader's folded choices,
 * persists each toggle, keeps the digests painted, and installs the focus
 * reveal.
 *
 * A section with no stored choice keeps the `open` attribute its markup
 * declares. That is what makes the DEFAULT state a design decision stated in
 * the markup — one primary section open, the rest folded — rather than
 * something this module imposes on every panel.
 *
 * @param {Element} root the panel body (or the panel)
 * @param {string} panel the panel's id, so three panels share one stored object
 *        without colliding on a section name they happen to share ("Table")
 * @param {object} [view] the storage host, for a test with no `localStorage`
 * @returns {{refresh: () => void, sections: () => Element[]}}
 */
export function bindPropertySections(root, panel, view = undefined) {
  const stored = loadPrefObject(SECTIONS_PREF_KEY, {}, view);
  const keyOf = (section) => `${panel}:${sectionId(section)}`;

  for (const section of propertySections(root)) {
    const key = keyOf(section);
    if (typeof stored[key] === "boolean") section.open = stored[key];
    if (section.dataset.propertySectionBound === "1") continue;
    section.dataset.propertySectionBound = "1";
    section.addEventListener("toggle", () => {
      const next = loadPrefObject(SECTIONS_PREF_KEY, {}, view);
      next[keyOf(section)] = section.open;
      savePrefObject(SECTIONS_PREF_KEY, next, view);
      paintPropertyDigests(root);
    });
  }

  if (root.dataset.propertyDigestsBound !== "1") {
    root.dataset.propertyDigestsBound = "1";
    // One listener on the body rather than one per control: a panel rebuilds
    // its fields and a per-control listener would have to be rebound each time.
    root.addEventListener("change", () => paintPropertyDigests(root));
    root.addEventListener("input", () => paintPropertyDigests(root));
  }

  wrapFocusReveal(root);
  paintPropertyDigests(root);

  return {
    refresh: () => {
      wrapFocusReveal(root);
      paintPropertyDigests(root);
    },
    sections: () => propertySections(root),
  };
}

/**
 * The controls a reader can actually operate right now under `root`: not inside
 * a closed section, not inside a hidden container, not itself hidden.
 *
 * This exists so the cognitive-burden claim is a MEASUREMENT rather than a
 * feeling. `SKILL` §9 rule 1 — a published number is derived from a committed
 * artifact — and the before/after counts in this module's header and in
 * `e2e/properties-panel-structure.spec.mjs` are read from here.
 *
 * Complexity: O(controls in the panel).
 *
 * @param {Element} root
 * @returns {Element[]}
 */
export function operableControls(root) {
  if (!root) return [];
  const out = [];
  for (const control of root.querySelectorAll(FOCUSABLE)) {
    let reachable = true;
    for (let node = control; node && node !== root.parentElement; node = node.parentElement) {
      if (node.hidden) {
        reachable = false;
        break;
      }
      if (node.matches?.("[data-property-section]") && !node.open && node !== control) {
        // The summary is the section's own control and stays reachable closed;
        // everything in the content is not.
        if (!control.closest("summary")) {
          reachable = false;
          break;
        }
      }
    }
    if (reachable) out.push(control);
  }
  return out;
}

/**
 * Withholds `control` with a stated reason. Never a dead control (`SKILL` §10).
 *
 * The reason rides on `title` AND on the accessible name, which is the shape
 * `object_arrange_chrome.mjs` already uses for a withheld menu row: `title`
 * alone is invisible to a screen reader and unreachable by touch, so a control
 * that cannot say why it is off is the dead-control defect with extra steps.
 *
 * @param {Element|null} control
 * @param {string} reason a localised sentence, already resolved
 * @param {string} [label] the control's own name, for the composed announcement
 */
export function withholdControl(control, reason, label = "") {
  if (!control) return;
  control.disabled = true;
  if (!reason) return;
  control.title = reason;
  const name = label || control.getAttribute("aria-label") || "";
  control.setAttribute("aria-label", name ? `${name} — ${reason}` : reason);
  control.dataset.withheld = "1";
}

/**
 * Restores `control` after a `withholdControl`, including the accessible name
 * the reason was composed into — which is the half a first implementation
 * forgets, leaving a working control announcing why it used to be off.
 *
 * @param {Element|null} control
 * @param {string} [label] the control's own name
 */
export function allowControl(control, label = "") {
  if (!control) return;
  control.disabled = false;
  if (control.dataset.withheld !== "1") return;
  delete control.dataset.withheld;
  control.removeAttribute("title");
  if (label) control.setAttribute("aria-label", label);
  else control.removeAttribute("aria-label");
}
