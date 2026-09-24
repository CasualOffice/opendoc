// Guards the chrome's ONE dropdown model (docs/115 §6).
//
// The owner reported the same defect four times: "styles dropdown and font still
// have two different ones, OS and product dropdown still". Two separate mistakes
// were behind it, and both are the kind that no feature test notices:
//
//   1. `#paragraphStyle` was a native `<select>` sitting IN the ribbon band,
//      directly above a custom gallery doing the same job — two chromes and two
//      controls for one choice. The spec suite had a test called "the Styles
//      selector exposes every style", which passed the whole time.
//   2. Among the `<select>`s that remained, only `.ctl select` was restyled;
//      `.dialog-select`, `.menu-select`, `.save-format`, `#linkPlaceSelect` and
//      five selects the object inspector builds from a template string rendered
//      raw OS chrome. So the native ones were not even consistent with each other.
//
// Neither is visible to a test that asks whether a control works. They are visible
// here, in the markup and the stylesheet, which is where the rules are stated.
//
// The companion e2e (`tests/e2e/styles-control.spec.mjs`) re-checks the second rule
// from COMPUTED style, because a CSS rule can be present and still be overridden.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const html = await readFile(new URL("../editor.html", import.meta.url), "utf8");
const css = await readFile(new URL("../src/style.css", import.meta.url), "utf8");

/** Strips HTML comments so prose about `<select>` is not mistaken for one. */
function stripHtmlComments(source) {
  return source.replace(/<!--[\s\S]*?-->/g, "");
}

/** Every `<select ...>` opening tag in editor.html, as its raw attribute text. */
function selectTags(source) {
  return [...stripHtmlComments(source).matchAll(/<select\b([^>]*)>/g)].map((m) => m[1]);
}

function attr(tag, name) {
  return tag.match(new RegExp(`${name}="([^"]*)"`))?.[1] ?? "";
}

// A `<select>` is legitimate in a dialog or a menu row, where a platform popup over
// a long list is the right affordance. It is NOT legitimate in the ribbon band or
// the compact bar, where every picker is a product popover. Nothing is exempt today;
// an entry here must name the select and say why the rule does not apply to it.
const RIBBON_SELECT_EXEMPTIONS = [];

test("the ribbon band contains no native <select>", () => {
  // The ribbon panels, from `<div id="panelHome" class="ribbon-panel"` to the end
  // of the ribbon body. Taking the whole `.ribbon` region rather than parsing the
  // tree keeps this honest about nesting we cannot see with a regex.
  const source = stripHtmlComments(html);
  const start = source.indexOf('<div class="ribbon-body"');
  assert.ok(start > 0, "could not find the ribbon body in editor.html");
  const end = source.indexOf('id="ribbonOverflowMenu"', start);
  assert.ok(end > start, "could not find the end of the ribbon body");
  const band = source.slice(start, end);

  const found = [...band.matchAll(/<select\b([^>]*)>/g)]
    .map((m) => attr(m[1], "id") || m[1].trim())
    .filter((id) => !RIBBON_SELECT_EXEMPTIONS.includes(id));

  assert.deepEqual(
    found,
    [],
    "a native <select> is in the ribbon band. The band's pickers are product " +
      "popovers (the Styles gallery, the font menu); an OS dropdown beside them is " +
      "the inconsistency docs/115 closes. Put the control in a popover, or add it " +
      "to RIBBON_SELECT_EXEMPTIONS with a reason.",
  );
});

test("the Styles group offers exactly one control", () => {
  const source = stripHtmlComments(html);
  const start = source.indexOf('data-group="styles"');
  assert.ok(start > 0, "the Home band should still have a Styles group");
  // Ends at the group's own caption, found without assuming ATTRIBUTE ORDER:
  // `class` is no longer the first attribute on that span (the i18n pass put a
  // `data-i18n` key before it), and matching the literal string silently ran
  // the slice to the end of the file and swept in eight unrelated selects.
  const captionAt = source.slice(start).search(/<span\b[^>]*\brgroup-label\b[^>]*>\s*Styles/);
  assert.ok(captionAt > 0, "the Styles group should still carry its caption");
  const group = source.slice(start, start + captionAt);

  // One trigger, no select, and no second entry point. The group held three
  // controls for one job — a select, a card gallery and a "▾" popover that listed
  // every style over again — and every one of them called `setParagraphStyle`.
  assert.equal(
    (group.match(/<select\b/g) ?? []).length,
    0,
    "the Styles group must not carry a <select>",
  );
  assert.equal(
    (group.match(/aria-haspopup=/g) ?? []).length,
    1,
    "the Styles group must expose exactly one dropdown trigger. Two is the defect " +
      "the owner reported four times; zero means the gallery is back on the band " +
      "instead of behind a trigger (docs/115 §5)",
  );
  assert.ok(
    group.includes('id="stylesTrigger"'),
    "the one control is `#stylesTrigger`, which the compact chrome adopts by id",
  );
  // The LIST lives in the popover layer, not in the band. A gallery rendered
  // inline satisfies every other assertion here.
  assert.equal(
    (group.match(/role="listbox"/g) ?? []).length,
    0,
    "the option list belongs in `#stylesMenu`, off the band, not inside the group",
  );
  // The popover is a DIALOG holding a filter field and a listbox — the shape
  // `#fontMenu` already uses, and the correct ARIA once a list has a search
  // box above it. Asserting `role="listbox"` on the popover itself was right
  // only while the menu was a bare list.
  assert.ok(
    /id="stylesMenu"[^>]*role="dialog"/.test(source),
    "`#stylesMenu` must be the dialog the trigger opens",
  );
  assert.ok(
    /id="stylesMenuList"[^>]*role="listbox"/.test(source),
    "`#stylesMenuList` must be the listbox inside it",
  );
  assert.ok(
    source.includes('id="stylesMenuInput"'),
    "the full style list is reachable by typing, not only by scrolling",
  );
});

// The selector list that carries `appearance: none` plus the product's caret. Read
// from the stylesheet rather than restated, so the two cannot drift.
function sharedSelectSelectors() {
  const marker = css.indexOf("ONE select treatment for the whole chrome");
  assert.ok(marker > 0, "the shared select-treatment block is gone from style.css");
  const ruleStart = css.indexOf("appearance: none;", marker);
  assert.ok(ruleStart > 0, "the shared select-treatment block no longer sets appearance");
  const braceAt = css.lastIndexOf("{", ruleStart);
  const commentEnd = css.lastIndexOf("*/", braceAt);
  return css
    .slice(commentEnd + 2, braceAt)
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

test("the shared select treatment covers every <select> in editor.html", () => {
  const selectors = sharedSelectSelectors();
  assert.ok(selectors.includes(".ctl select"), "the ribbon's own combos must stay covered");

  // Only SINGLE-CLASS selectors are usable as proof here. A descendant selector
  // (`.ctl select`, `.dialog-field select`) depends on an ancestor a regex over flat
  // HTML cannot confirm — and the first version of this test treated any selector
  // ending in `select` as covering everything, so deleting `.dialog-select` from the
  // shared list left it green. That is exactly the failure §4 exists to catch, found
  // by mutating the stylesheet, so the rule is now the narrow, checkable one: a
  // `<select>` in editor.html must carry a class the shared list names outright. The
  // e2e reads COMPUTED style and so covers the ancestor-scoped cases as well.
  const classNames = selectors
    .filter((selector) => /^\.[\w-]+$/.test(selector))
    .map((selector) => selector.slice(1));
  assert.ok(classNames.length > 0, "the shared list should name at least one class");

  const uncovered = [];
  for (const tag of selectTags(html)) {
    const classes = attr(tag, "class").split(/\s+/).filter(Boolean);
    if (!classes.some((name) => classNames.includes(name))) {
      uncovered.push(attr(tag, "id") || tag.trim());
    }
  }

  assert.deepEqual(
    uncovered,
    [],
    "every <select> in the chrome must carry a class the shared treatment names " +
      `(one of: ${classNames.join(", ")}), or it renders raw OS chrome next to ` +
      "controls that do not — which is what `#linkPlaceSelect` did. Add the class, " +
      "or extend the shared selector list.",
  );
});

test("the shared treatment is the LAST word on select backgrounds", () => {
  // `.ctl select` sets the `background` shorthand, which resets `background-image`.
  // If the shared block were moved above it, the caret would silently vanish from
  // the ribbon's own combos with every rule still present in the file.
  const shared = css.indexOf("ONE select treatment for the whole chrome");
  const shorthand = css.indexOf(".ctl select,\n.ctl > input[type=\"number\"]");
  assert.ok(shorthand > 0, "the .ctl select background shorthand rule moved or changed");
  assert.ok(
    shared > shorthand,
    "the shared select treatment must come AFTER the `.ctl select` background " +
      "shorthand, or the shorthand wins and the caret disappears",
  );
});
