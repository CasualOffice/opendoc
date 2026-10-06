// The Home / View / Table bands and their command ids must agree with the markup
// (`109` UX-005).
//
// Two declarations, as always: the controls live in `editor.html` and the command
// each one stands for lives in `ribbon_faces.mjs`. The failure mode of any such
// pair is that one gains a row and the other does not — a button with no command
// id is the defect UX-005 names, and a face with no button is a selector that
// matches nothing at boot. Both directions are asserted here against the real
// markup, in node, so the next hand-bound button is caught when it is added.
//
// What this file deliberately does NOT check: that an id is live in the command
// registry, and that activating the control reaches it. Neither is answerable
// without the app — `main.js` has no exports and builds the registry from
// document state — so both are `tests/e2e/ribbon-command-faces.spec.mjs`, driven
// in a browser. A table-only check is exactly what let `⌘⌥M` stay bound to a
// command id the registry does not return (#623).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  HOME_FACES,
  VIEW_FACES,
  TABLE_FACES,
  RIBBON_FACES,
  stampRibbonFaces,
  declaredCommandIds,
  declaredFamilies,
} from "../src/ribbon_faces.mjs";

const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");

/** One ribbon panel's markup. Scoped, so a control elsewhere in the file cannot
 *  silently satisfy a check about this band. */
function panel(id, nextId) {
  const start = html.indexOf(`id="${id}"`);
  assert.ok(start > 0, `${id} must exist in editor.html`);
  const end = html.indexOf(`id="${nextId}"`, start);
  assert.ok(end > start, `${nextId} must follow ${id}`);
  return html.slice(start, end);
}

const PANELS = {
  panelHome: () => panel("panelHome", "panelInsert"),
  panelView: () => panel("panelView", "panelReview"),
  panelTable: () => panel("panelTable", "panelView"),
};

/** Every control in a band's markup, as the selector a face would name it by.
 *  `<option>` inside the font-size datalist is not a control; `button`, `input`
 *  and `select` are. */
function controlsIn(rawMarkup) {
  // Comments first: `editor.html` explains the Styles group by quoting the
  // `<select>` it replaced, and a scan that reads prose as markup reports a
  // control that does not exist.
  const markup = rawMarkup.replace(/<!--[\s\S]*?-->/g, "");
  const selectors = [];
  for (const match of markup.matchAll(/<(?:button|input|select)\b([^>]*)>/g)) {
    const attrs = match[1];
    const id = attrs.match(/\bid="([^"]+)"/);
    const keyed = attrs.match(/\bdata-table-(select|action|distribute|sort)="([^"]+)"/);
    if (id) selectors.push(`#${id[1]}`);
    else if (keyed) selectors.push(`[data-table-${keyed[1]}="${keyed[2]}"]`);
    else selectors.push(`UNNAMED: ${attrs.trim().slice(0, 80)}`);
  }
  return selectors;
}

test("every control in the three bands names the command it stands for", () => {
  for (const [panelId, entries] of Object.entries(RIBBON_FACES)) {
    const declared = new Set(entries.map((entry) => entry.select));
    const unnamed = controlsIn(PANELS[panelId]()).filter((selector) => !declared.has(selector));
    assert.deepEqual(
      unnamed,
      [],
      `${panelId} holds controls with no entry in ribbon_faces.mjs. Each one is a ` +
        "ribbon-only capability: add a `face(selector, id)` for the command it runs, " +
        "or a `chooser(selector, prefix)` if it opens a picker over a family",
    );
  }
});

test("every face has a control in the markup", () => {
  for (const [panelId, entries] of Object.entries(RIBBON_FACES)) {
    const present = new Set(controlsIn(PANELS[panelId]()));
    const missing = entries.map((e) => e.select).filter((selector) => !present.has(selector));
    assert.deepEqual(
      missing,
      [],
      `${panelId} declares faces whose control is not in the markup — they would ` +
        "stamp nothing at boot and the control they were renamed from would lose its id",
    );
  }
});

test("no control is claimed twice, and no face is both a command and a chooser", () => {
  for (const [panelId, entries] of Object.entries(RIBBON_FACES)) {
    const selectors = entries.map((entry) => entry.select);
    assert.equal(
      new Set(selectors).size,
      selectors.length,
      `two faces in ${panelId} claim the same control`,
    );
    for (const entry of entries) {
      assert.equal(
        Boolean(entry.command) !== Boolean(entry.family),
        true,
        `${panelId} ${entry.select} must declare exactly one of command / family`,
      );
    }
  }
});

test("a command id looks like one, and a family prefix is a prefix", () => {
  for (const id of declaredCommandIds()) {
    assert.match(id, /^[a-z]+(\.[A-Za-z0-9]+)+$/, `"${id}" is not a command id`);
  }
  for (const family of declaredFamilies()) {
    // A family must be able to have members. `format.color` is a legitimate
    // family without a trailing dot because the colour VALUES are not commands —
    // the apply half repeats the picker's choice — so the rule is only that the
    // prefix names a namespace, not the whole of one id.
    assert.match(family, /^[a-z]+\.[A-Za-z0-9.]*$/, `"${family}" is not a family prefix`);
  }
});

test("Find and Replace are two commands with a face each, so Replace can carry ⌘H", () => {
  // They were declared as two faces of `edit.find`, which made Ctrl+H impossible:
  // a chord binds to a command id, and the Replace button's own chord had no id to
  // be. `review.toggle`'s second face is on the Review band, outside these tables,
  // so no command is declared twice here any more.
  const ids = declaredCommandIds();
  const twice = ids.filter((id, i) => ids.indexOf(id) !== i);
  assert.deepEqual([...new Set(twice)], []);
  assert.ok(ids.includes("edit.find") && ids.includes("edit.replace"));
});

test("stamping puts the ids on the controls, and reports the ones it could not find", () => {
  // A minimal stand-in DOM: enough to prove the stamping contract without a
  // browser, which is what keeps `ribbon_faces.mjs` pure.
  const made = new Map();
  const element = (select) => {
    const el = { dataset: {} };
    made.set(select, el);
    return el;
  };
  const root = {
    querySelector: (selector) => ({
      querySelector: (inner) => (inner === "#bold" || inner === "#undoBtn" ? element(inner) : null),
    }),
  };
  const faces = {
    panelHome: [
      { select: "#bold", command: "format.bold" },
      { select: "#undoBtn", command: "edit.undo" },
      { select: "#gone", command: "format.italic" },
      { select: "#alsoGone", family: "style." },
    ],
  };
  const unresolved = stampRibbonFaces(root, faces);
  assert.equal(made.get("#bold").dataset.command, "format.bold");
  assert.equal(made.get("#undoBtn").dataset.command, "edit.undo");
  assert.deepEqual(unresolved, ["panelHome #gone", "panelHome #alsoGone"]);
});

test("a chooser stamps the family, not the command", () => {
  const stamped = { dataset: {} };
  const root = { querySelector: () => ({ querySelector: () => stamped }) };
  stampRibbonFaces(root, { panelHome: [{ select: "#changeCaseBtn", family: "format.case." }] });
  assert.equal(stamped.dataset.commandFamily, "format.case.");
  assert.equal(stamped.dataset.command, undefined);
});

test("the bands are not empty, so a table emptied by accident is not silently fine", () => {
  // Every assertion above is vacuously true for an empty table. These three
  // numbers are lower bounds, not pinned counts: adding a control must not turn
  // this red (SKILL.md — a guard pinned to a measured size reddens main on
  // changes that remove nothing).
  assert.ok(HOME_FACES.length >= 40, `Home declares ${HOME_FACES.length} faces`);
  assert.ok(VIEW_FACES.length >= 8, `View declares ${VIEW_FACES.length} faces`);
  assert.ok(TABLE_FACES.length >= 19, `Table declares ${TABLE_FACES.length} faces`);
});
