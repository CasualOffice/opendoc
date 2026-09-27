// The DOM half of region composition: does each region actually name something?
//
// `regions.test.mjs` proves the DECISION is right — which regions a role gets, and
// that roles and explicit lists are one mechanism. This file proves the decision
// can be CARRIED OUT: that every declared region resolves to a real element in
// `editor.html`, and that the stylesheet has a rule for it.
//
// WHY THAT NEEDS ITS OWN GUARD. A region is a name, and a name that matches nothing
// takes nothing away. `docs/99` §9.4 records the class: "built" is not "reachable".
// A `REGIONS` entry with a typo, or a region whose element was later renamed, would
// leave a host believing they had withheld a surface that is still on screen —
// which is worse than not offering the option, because they would ship it. So the
// vocabulary is checked against the markup and the stylesheet in the unit lane,
// where it costs nothing, and `tests/e2e/white-label.spec.mjs` then proves a real
// browser paints the result.
//
// Buildless: `editor.html` and `style.css` are read as text, like
// `style_tokens.test.mjs` and `no_unrouted_strings.test.mjs` already do.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

import { REGIONS } from "../src/capabilities.mjs";
import { applyRegions, bandElements, regionClass } from "../src/chrome_regions.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const editor = readFileSync(join(WEBAPP, "editor.html"), "utf8");
const css = readFileSync(join(WEBAPP, "src", "style.css"), "utf8");

/** What each region is expected to reach in the markup, as the selector the
 *  stylesheet uses. Declared here rather than in the module because the module
 *  applies a CLASS and the stylesheet owns the mapping; this is the test asserting
 *  that the two halves meet. */
const EXPECTED = Object.freeze({
  brand: ['class="brand"'],
  title: ['class="document-title-row"'],
  menu: ['id="appMenuBar"'],
  ribbon: ['class="ribbon"', 'class="ribbon-nav"'],
  rail: ['class="rail"', 'id="outlinePanel"', 'id="pagesPanel"'],
  // Both durable entry points, not just the View band's: the rail's Versions
  // button is the second face of the same command, and a region that took one
  // away and left the other would be a host believing they had withheld a
  // surface that is still on screen.
  history: [
    'id="versionPanel"',
    'id="viewVersionsBtn"',
    'id="railVersions"',
    'id="versionPreviewBanner"',
  ],
  status: ['class="footer"'],
  zoom: ['class="zoom"'],
  find: ['id="findPanel"'],
  selection: ['id="selToolbar"'],
  settings: ['id="settingsBtn"', 'id="settingsPanel"'],
});

test("every region names something that exists in editor.html", () => {
  const missing = [];
  for (const id of REGIONS) {
    if (id.startsWith("band.")) {
      const { tab, panel } = bandElements(id);
      if (!editor.includes(`id="${tab}"`)) missing.push(`${id} -> #${tab}`);
      if (!editor.includes(`id="${panel}"`)) missing.push(`${id} -> #${panel}`);
      continue;
    }
    const expected = EXPECTED[id];
    assert.ok(expected, `${id} has no expected selector, so this guard cannot check it`);
    for (const needle of expected) {
      if (!editor.includes(needle)) missing.push(`${id} -> ${needle}`);
    }
  }
  assert.deepEqual(
    missing,
    [],
    "a region that names nothing takes nothing away: a host would believe they had " +
      "withheld a surface that is still on screen, and would ship it",
  );
  // The half that fails when the guard breaks rather than when the tree does.
  assert.equal(Object.keys(EXPECTED).length + REGIONS.filter((r) => r.startsWith("band.")).length, REGIONS.length);
});

test("every region has a rule that actually removes it", () => {
  // A class nothing styles is a class that hides nothing. Checked against the
  // stylesheet rather than trusted, because the JS and the CSS are two files and
  // the only thing joining them is the spelling of one class name.
  const missing = [];
  for (const id of REGIONS) {
    if (!css.includes(`body.${regionClass(id)} `)) missing.push(regionClass(id));
  }
  assert.deepEqual(missing, [], "these region classes are set by JS and styled by nothing");
  // And the eight band ids the CSS names really are the eight the vocabulary has,
  // in both directions — a stale rule for a renamed band is a rule that fires for
  // nothing.
  const styled = [...css.matchAll(/body\.chrome-no-band-(\w+) /g)].map((m) => `band.${m[1]}`);
  assert.deepEqual([...new Set(styled)].sort(), REGIONS.filter((r) => r.startsWith("band.")).sort());
});

test("the menu bar survives the ribbon being withheld, or reading chrome has no axis", () => {
  // `body.ribbon-mode #appMenuBar { display: none }` enforces the one-navigation-axis
  // invariant (`109` UX-014, docs/122) for the default chrome. Withholding the
  // ribbon therefore has to REVEAL the menu bar, with higher specificity, or a
  // reading container has neither axis and File > Print — `readonly`'s only grant —
  // is unreachable.
  assert.match(css, /body\.chrome-no-ribbon\.ribbon-mode #appMenuBar \{\s*display: flex;/);
  // And `preview`, which withholds the menu too, gets neither. `!important` because
  // the rule above would otherwise win by specificity.
  assert.match(css, /body\.chrome-no-menu\.ribbon-mode #appMenuBar,/);
  assert.match(css, /body\.chrome-no-menu #appMenuPopover \{\s*display: none !important;/);
});

test("applying a region set toggles a class for every region, both ways", () => {
  // Idempotent, and symmetric. A function that only ever ADDS classes leaves a
  // container in the union of every set it was ever given, which is how a host
  // narrowing a role live would end up with chrome that never comes back.
  const classes = new Set();
  const body = {
    classList: { toggle: (name, on) => (on ? classes.add(name) : classes.delete(name)) },
    dataset: {},
  };
  const tabs = new Map(
    REGIONS.filter((r) => r.startsWith("band.")).map((id) => [bandElements(id).tab, { hidden: false, getAttribute: () => null }]),
  );
  const root = { getElementById: (id) => tabs.get(id) ?? null };

  const withheld = applyRegions({ body, root, regions: new Set(["status"]), selectBand: () => {} });
  assert.equal(classes.has(regionClass("status")), false, "a SHOWN region must carry no class");
  assert.equal(classes.has(regionClass("ribbon")), true);
  assert.equal(withheld.includes("ribbon"), true);
  assert.equal(withheld.includes("status"), false);
  assert.equal(body.dataset.chromeWithheld.includes("ribbon"), true);
  // Every band's tab is marked hidden, which is the one thing the stylesheet cannot
  // do: a tab at `display: none` is still in the roving-tabindex array, so without
  // this the arrow keys walk onto an invisible tab and navigation appears to stop.
  for (const tab of tabs.values()) assert.equal(tab.hidden, true);

  // Now widen, and every class must come back off.
  applyRegions({ body, root, regions: new Set(REGIONS), selectBand: () => {} });
  assert.deepEqual([...classes], [], "applying the full set left classes behind");
  assert.equal(body.dataset.chromeWithheld, "");
  for (const tab of tabs.values()) assert.equal(tab.hidden, false);
});

test("a container whose selected band was withheld is shown a surviving one", () => {
  // Home carries `aria-selected="true"` in the markup, so the default band is
  // exactly the one a host is most likely to withhold. Without this,
  // `?chrome=-band.home` opens onto nothing at all.
  const selected = new Map([["tabHome", "true"]]);
  const root = {
    getElementById: (id) => ({ hidden: false, getAttribute: () => selected.get(id) ?? null }),
  };
  const body = { classList: { toggle: () => {} }, dataset: {} };
  const chosen = [];
  const shown = new Set(REGIONS.filter((r) => r !== "band.home"));
  applyRegions({ body, root, regions: shown, selectBand: (band) => chosen.push(band) });
  assert.deepEqual(chosen, ["insert"], "the first surviving band should have been selected");

  // And when the selected band survives, nothing is switched — a container must not
  // be yanked off the band its host chose.
  chosen.length = 0;
  applyRegions({ body, root, regions: new Set(REGIONS), selectBand: (band) => chosen.push(band) });
  assert.deepEqual(chosen, []);

  // `band.file` is never the survivor: it is the File PAGE, which covers the work
  // area, so opening a container into it would hide the document it exists to show.
  chosen.length = 0;
  applyRegions({
    body,
    root,
    regions: new Set(["ribbon", "band.file"]),
    selectBand: (band) => chosen.push(band),
  });
  assert.deepEqual(chosen, [], "a container must not open into the File page");
});

test("band ids are derived from region ids, not listed a second time", () => {
  // `ribbon_faces.mjs` versus `one-axis-navigation.spec.mjs` is this repository's
  // standing example of what a second table of ids costs.
  assert.deepEqual(bandElements("band.home"), { tab: "tabHome", panel: "panelHome" });
  assert.deepEqual(bandElements("band.references"), { tab: "tabReferences", panel: "panelReferences" });
  assert.equal(regionClass("band.table"), "chrome-no-band-table");
  assert.equal(regionClass("status"), "chrome-no-status");
});
