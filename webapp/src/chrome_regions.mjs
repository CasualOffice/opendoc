// The DOM half of region composition: which chrome a container actually paints.
//
// `capabilities.mjs` decides WHICH regions a host gets — it is the one authority
// and it stays free of the DOM. This file knows where each region lives and how
// to take it away, the same split `i18n.mjs` and `localize.mjs` already use for
// localisation: the decision is answerable in node, the application needs a page.
//
// WHY BODY CLASSES AND CSS, not `element.hidden`. Because `hidden` is not
// durable here. The editor toggles `hidden` on exactly these elements as part of
// working — `outlinePanel.hidden = !outlinePanel.hidden` when the rail is
// clicked, `panel.hidden = panel.dataset.panel !== name` on every tab change — so
// a region taken away by setting `hidden` comes back the first time the feature
// that owns it runs. A `display: none` rule keyed off a class on `<body>` wins
// against that, survives every re-render, and needs no cooperation from the 300
// call sites that already toggle these nodes. It is also the shape the editor
// already uses for exactly this job: `body:not(.doc-loaded)` hides the ribbon, the
// rail and the status bar together, and `body.compact-mode` hides the ribbon in
// favour of the menu bar.
//
// One thing still needs JavaScript, and it is a real bug if it is left out: a
// ribbon TAB that is `display: none` is still in the roving-tabindex array
// `main.js` builds, so the arrow keys would walk onto an invisible tab and appear
// to stop. So a withheld band's tab is also marked `hidden`, which the roving
// filter reads. Nothing in the editor un-hides a tab (only panels), so that
// marking is durable.
//
// No English in this file. A region that is not there says nothing — that is the
// distinction `docs/126` draws: "never, for you" is composition and is silent,
// "not right now" is state and explains itself. So there is no string to route.
import { REGIONS } from "./capabilities.mjs";

/** The class a withheld region puts on `<body>`. One vocabulary, so a stylesheet
 *  rule and a test name the same thing. */
export const regionClass = (id) => `chrome-no-${id.replace(/\./g, "-")}`;

/** Ribbon band → the tab button and the panel it controls.
 *
 *  Derived from the region id rather than listed: `band.home` → `#tabHome` /
 *  `#panelHome`, which is the naming `editor.html` already uses for all eight.
 *  A second table of ids is a second thing to keep in step, and
 *  `ribbon_faces.mjs`'s relationship with `one-axis-navigation.spec.mjs` is the
 *  repository's standing example of what that costs. */
export function bandElements(id) {
  const name = id.slice("band.".length);
  const suffix = name.charAt(0).toUpperCase() + name.slice(1);
  return { tab: `tab${suffix}`, panel: `panel${suffix}` };
}

/**
 * Applies a region set to a page.
 *
 * Idempotent: every region is either added or removed on every call, so calling
 * it twice — or calling it again after a host narrows the set — leaves the page
 * in exactly the state the set describes rather than accumulating.
 *
 * `selectBand` is called only when the band that was on screen is one the host
 * withheld, with the first band that survives. Without it a `?chrome=-band.home`
 * container would open showing nothing, because Home is the default tab.
 *
 * Complexity: O(regions) — eighteen — once at boot.
 *
 * @param {object} io
 * @param {{classList: {toggle: Function}, dataset: object}} io.body
 * @param {{getElementById: Function}} io.root
 * @param {{has: (id: string) => boolean}} io.regions
 * @param {(band: string) => void} [io.selectBand]
 * @returns {readonly string[]} the regions withheld, for a caller that wants to
 *   report them (the embed demo lists them) rather than re-derive them.
 */
export function applyRegions({ body, root, regions, selectBand }) {
  const withheld = [];
  for (const id of REGIONS) {
    const shown = regions?.has?.(id) === true;
    if (!shown) withheld.push(id);
    body.classList.toggle(regionClass(id), !shown);
    if (!id.startsWith("band.")) continue;
    // The one piece the stylesheet cannot do: keep a hidden tab out of the
    // keyboard's way.
    const tab = root.getElementById(bandElements(id).tab);
    if (tab) tab.hidden = !shown;
  }
  // A container that withheld the band that was on screen must be shown another
  // one, or `?chrome=-band.home` opens onto nothing: Home carries
  // `aria-selected="true"` in the markup, so the default tab is precisely the one
  // a host is most likely to withhold.
  //
  // `band.file` is skipped as a survivor: it is the File PAGE, which covers the
  // work area rather than sitting beside the document, so opening a container
  // into it would hide the document the container exists to show.
  const bands = REGIONS.filter((id) => id.startsWith("band."));
  const survivor = bands.find((id) => id !== "band.file" && regions?.has?.(id) === true);
  const current = bands.find((id) => {
    const tab = root.getElementById(bandElements(id).tab);
    return tab?.getAttribute("aria-selected") === "true";
  });
  const stranded = !current || regions?.has?.(current) !== true;
  if (stranded && survivor && selectBand) selectBand(survivor.slice("band.".length));
  // Published on the element so a host page, a screenshot or a spec can read
  // what was composed away without re-deriving it from eighteen classes.
  body.dataset.chromeWithheld = withheld.join(" ");
  return Object.freeze(withheld);
}
