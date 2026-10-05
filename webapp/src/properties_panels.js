// SPDX-License-Identifier: Apache-2.0
// The editor's second entry script: it binds the collapsible sections of the
// properties panels whose behaviour lives in `main.js` (`105` UX-027).
//
// ---- WHY THIS IS A SECOND ENTRY SCRIPT AND NOT A CALL IN `main.js` --------
//
// The paragraph and table properties panels are declared in `editor.html` and
// driven from `main.js`: its reflect loops write their values and its ribbon
// rows focus their controls. The natural place to bind their sections is
// therefore beside that code — and there is no room. `main.js` is 16,162 lines
// against a ratchet of 16,162 (`tests/module_seams.test.mjs`), it has zero
// exports and no mount seam, and it is owned by another lane. A line added
// there fails the build.
//
// So the binding arrives the way the site's pages already get their behaviour:
// as an entry script the markup loads, next to the markup it binds. `main.js`
// is untouched, the panels keep their single owner, and the three properties
// panels end up sharing one section mechanism rather than two.
//
// That is also why this is `.js` and not `.mjs`: the convention
// `module_seams.test.mjs` encodes is that `.mjs` is a module something imports
// and `.js` is a page's entry script, run for its side effects
// (`main.js`, `fidelity.js`, `home-embed.js`). This is the fourth.
//
// ---- WHAT IT ADDS OVER PLAIN `<details>` ---------------------------------
//
// Native `<details>` already collapses, takes Enter and Space, and announces
// itself — so the separation works with no script at all. Three things do need
// one:
//
//   1. THE FOCUS REVEAL. Layout ▸ Indent focuses the left-indent field and
//      Layout ▸ Spacing focuses the line-spacing select, both from `main.js`.
//      `focus()` on an element inside a closed `<details>` does nothing, so
//      folding those sections by default would silently break both pointers —
//      the exact defect ADR-062's corollary C1 exists to forbid.
//      `property_sections.mjs` wraps `focus` on the controls so the reveal
//      happens whoever calls it, which is what makes folding those sections
//      safe to ship at all.
//   2. PERSISTENCE. A reader who folds Pagination means it, and meaning it once
//      should be enough.
//   3. THE DIGESTS. A folded section says what it holds. `main.js` writes these
//      panels' values programmatically, and a programmatic `value =` fires
//      neither `input` nor `change` — so the digests are also repainted when a
//      panel is shown or hidden, which is when a stale one would be read.
import { bindPropertySections } from "./property_sections.mjs";

/** The panels this script owns, by element id and by the name their sections
 *  are stored under. The object properties panel is NOT here: its own module
 *  (`object_inspector.mjs`) binds it, because it also repaints the digests from
 *  its reflect loop and binding it twice would wrap `focus` twice. */
const PANELS = Object.freeze([
  ["paragraphPropertiesPanel", "paragraph"],
  ["tablePropertiesPanel", "table"],
]);

/**
 * Binds one panel and keeps its digests honest across open/close.
 *
 * The observer watches ONE attribute on ONE element and fires only when the
 * panel is shown or hidden, which is a handful of times in a session. It is not
 * a substitute for a reflect hook: it is the cheapest correct answer available
 * to a script that must not edit the module doing the reflecting.
 */
function bindPanel(element, name) {
  const sections = bindPropertySections(element, name);
  new MutationObserver(() => {
    if (!element.hidden) sections.refresh();
  }).observe(element, { attributes: true, attributeFilter: ["hidden"] });
}

for (const [id, name] of PANELS) {
  const element = document.getElementById(id);
  // A host page that embeds the editor without a panel is not an error.
  if (element) bindPanel(element, name);
}
