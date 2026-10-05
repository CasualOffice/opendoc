// The density ratchet: a chrome surface may not quietly grow.
//
// `docs/63`, `64`, `67`, `101`, `105`, `123` and `148` all discuss control
// density and none of them publishes a number that regenerates, so the only
// density figures this repository had were point measurements in prose — "~30 of
// Word's 38 controls", "269px of slack" — written once and never re-derived. The
// consequence is the one the owner reported: burden accumulates one justified
// control at a time, and nothing says stop.
//
// `webapp/chrome-census.json` is the artifact, `tools/build-chrome-census.mjs`
// generates it, and `build.sh` fails when it drifts. This file is the other half:
// it asserts the PROPERTIES the census is supposed to have, so a regeneration
// that merely records a worse chrome still fails the build.
//
// It is a ratchet and not a cage. Every budget here is the measured value plus a
// stated allowance, and raising one is a one-line change with a reason — which
// is exactly the conversation the previous arrangement did not force anyone to
// have.
import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { build } from "../tools/build-chrome-census.mjs";

const census = JSON.parse(readFileSync(new URL("../chrome-census.json", import.meta.url), "utf8"));

test("the committed chrome census is what the generator produces", () => {
  assert.equal(
    build(true),
    0,
    "webapp/chrome-census.json is stale — run ./tools/build-chrome-census.mjs and commit it",
  );
});

test("the parser the census depends on does not nest void elements", () => {
  // A scanner that pushes a frame for `<input>` puts every later control inside
  // the first form field, and every per-surface count after that is wrong while
  // the total stays right. That failure is invisible in the output, so it is
  // asserted on a fixture rather than hoped for.
  const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
  const firstInput = html.indexOf("<input");
  assert.ok(firstInput > 0, "expected editor.html to contain an <input>");
  // `#docTitle` is the FIRST input in the file and it sits in the header, two
  // levels deep. A nesting bug would put the ribbon inside it.
  const docTitleAt = html.indexOf('id="docTitle"');
  const ribbonAt = html.indexOf('class="ribbon-tabs"');
  assert.ok(docTitleAt > 0 && ribbonAt > docTitleAt, "fixture assumption moved");
  assert.ok(
    census.markup.persistent.ribbonTabsRibbonOnly > 0,
    "the ribbon tab strip counted zero controls, which is what a void-element " +
      "nesting bug looks like: the strip ends up inside #docTitle and therefore " +
      "inside the header's subtree rather than being measured on its own",
  );
});

test("exactly one ribbon band is the landing band", () => {
  // Two unhidden bands is two bands of controls on screen at once — the
  // "one navigation axis" rule (docs/123 §3) applied inside the ribbon.
  const landing = Object.entries(census.markup.ribbonPanels).filter(([, p]) => p.landing);
  assert.deepEqual(
    landing.map(([name]) => name),
    ["home"],
    "exactly one ribbon panel may ship without `hidden`, and it is Home",
  );
});

// The budgets. Each is the value measured on this tree plus headroom, and the
// headroom is deliberately small: a band that needs more than a couple of new
// controls is a band that needs a decision, not a bigger number here.
const BUDGET = {
  // 43 today. The Home band is the surface a reader meets first and the one
  // `docs/123` §4.6 measures against Word's 38, so this is the number that
  // matters most and the one with the least slack.
  home: 46,
  insert: 24,
  layout: 20,
  references: 14,
  review: 22,
  view: 14,
  table: 22,
};

for (const [band, budget] of Object.entries(BUDGET)) {
  test(`the ${band} band stays within its control budget`, () => {
    const panel = census.markup.ribbonPanels[band];
    assert.ok(panel, `no census entry for the ${band} band`);
    assert.ok(
      panel.controls <= budget,
      `the ${band} band now shows ${panel.controls} controls against a budget of ` +
        `${budget}. This is the burden ratchet, not an error: a band that needs ` +
        `more controls needs a decision about what leaves, or a raised budget ` +
        `with the reason recorded here. See docs/167.`,
    );
  });
}

test("the chrome a reader meets at rest stays within its budget", () => {
  // 65 on the ribbon chrome, 22 on the compact one. The owner's brief is "little
  // to zero cognitive burden"; this is the single number that expresses it, and
  // the two chromes are budgeted separately because they are two products in
  // one and the compact one is the low-burden rung.
  assert.ok(
    census.markup.persistentRibbonChrome <= 70,
    `the ribbon chrome now shows ${census.markup.persistentRibbonChrome} controls ` +
      `before a reader opens anything (budget 70)`,
  );
  assert.ok(
    census.markup.persistentCompactChrome <= 26,
    `the compact chrome now shows ${census.markup.persistentCompactChrome} controls ` +
      `before a reader opens anything (budget 26). The compact chrome is the ` +
      `low-burden rung — if it is growing toward the ribbon's figure, the rung ` +
      `has stopped being a rung.`,
  );
});

test("the two navigation axes are never summed", () => {
  // The guard against the mistake the first run of this census made: counting
  // the menu bar and the ribbon tab strip together describes a chrome that does
  // not exist, because style.css hides each one in the other's mode.
  const p = census.markup.persistent;
  assert.ok(
    census.markup.persistentRibbonChrome < p.header + p.appMenuBarCompactOnly + p.ribbonTabsRibbonOnly + p.rail + p.statusBar + census.markup.ribbonPanels.home.controls,
    "persistentRibbonChrome must exclude the compact-only menu bar",
  );
  assert.ok(
    !Object.prototype.hasOwnProperty.call(census.markup, "persistentTotal"),
    "`persistentTotal` summed both navigation axes and described a chrome this " +
      "editor never shows; it must not come back",
  );
});
