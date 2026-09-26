// `109` UX-021. Two things are guarded here, and neither is "the helper was
// called":
//
//   1. The arrow arithmetic a radio group owns, which differs from the toolbar's
//      in exactly one way — a radio group is navigable on BOTH axes.
//   2. The markup invariant: a container declared `role="radiogroup"` must not
//      ship options that declare `aria-pressed`. That is the contradiction the
//      row names — "radio group, three items" followed by "toggle button, not
//      pressed" — and it is checkable without a browser, so a new segmented
//      control cannot reintroduce it and wait for the e2e suite to notice.
//
// The live DOM half — role, checked state and exactly one Tab stop on every
// group, at load and after a surface reflects — is
// `tests/e2e/radio-group-contract.spec.mjs`, because it is a runtime guarantee
// and reading it off the markup would prove nothing.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import { radioIndex } from "../src/radio_group.mjs";

const EDITOR_HTML = readFileSync(new URL("../editor.html", import.meta.url), "utf8");

test("both arrow axes move within a radio group, and both wrap", () => {
  for (const forward of ["ArrowRight", "ArrowDown"]) {
    assert.equal(radioIndex(forward, 0, 3), 1, forward);
    assert.equal(radioIndex(forward, 2, 3), 0, `${forward} wraps`);
  }
  for (const back of ["ArrowLeft", "ArrowUp"]) {
    assert.equal(radioIndex(back, 2, 3), 1, back);
    assert.equal(radioIndex(back, 0, 3), 2, `${back} wraps`);
  }
});

// The difference from a toolbar, and the reason this is not just `rovingIndex`:
// a toolbar owns one axis, a radio group owns both regardless of how it is laid
// out. A segmented control that stacks on a narrow viewport must still answer
// Down, and one laid out in a row must still answer Right.
test("the vertical pair is not left to the caller the way a toolbar leaves it", () => {
  assert.notEqual(radioIndex("ArrowDown", 0, 3), null);
  assert.notEqual(radioIndex("ArrowUp", 0, 3), null);
});

test("Home and End jump to the ends", () => {
  assert.equal(radioIndex("Home", 2, 4), 0);
  assert.equal(radioIndex("End", 1, 4), 3);
});

test("entering with focus on no option lands on the matching edge", () => {
  assert.equal(radioIndex("ArrowRight", -1, 3), 0);
  assert.equal(radioIndex("ArrowLeft", -1, 3), 2);
});

test("keys a radio group does not own are left alone, so nothing is swallowed", () => {
  for (const key of ["Tab", "Enter", " ", "Escape", "a", "PageDown"]) {
    assert.equal(radioIndex(key, 1, 3), null, key);
  }
});

test("an empty group claims nothing", () => {
  for (const key of ["ArrowRight", "ArrowDown", "Home", "End"]) {
    assert.equal(radioIndex(key, -1, 0), null, key);
  }
});

/** Every `role="radiogroup"` container in the editor markup, with its inner HTML.
 *  Deliberately a scan of the shipped page rather than a list kept here: a list
 *  would have to be remembered, and the five groups this row is about were added
 *  one at a time by people who had never read UX-021. */
function declaredRadioGroups() {
  const groups = [];
  const open = /<(\w+)([^>]*\brole="radiogroup"[^>]*)>/g;
  for (let m; (m = open.exec(EDITOR_HTML)); ) {
    const [tag, attrs] = [m[1], m[2]];
    // Walk to the matching close tag, counting nesting of the same tag name.
    const scan = new RegExp(`</?${tag}\\b[^>]*>`, "g");
    scan.lastIndex = open.lastIndex;
    let depth = 1;
    let end = EDITOR_HTML.length;
    for (let t; depth > 0 && (t = scan.exec(EDITOR_HTML)); ) {
      depth += t[0].startsWith("</") ? -1 : 1;
      if (depth === 0) end = t.index;
    }
    groups.push({
      id: /\bid="([^"]+)"/.exec(attrs)?.[1] ?? /\bclass="([^"]+)"/.exec(attrs)?.[1] ?? tag,
      inner: EDITOR_HTML.slice(open.lastIndex, end),
    });
  }
  return groups;
}

test("the editor declares radio groups at all, or this file proves nothing", () => {
  assert.ok(
    declaredRadioGroups().length >= 5,
    "the scan found fewer radio groups than the editor ships — it has stopped " +
      "matching the markup, so the invariant below is vacuous",
  );
});

test("no option inside a declared radio group claims to be a toggle button", () => {
  const contradictory = declaredRadioGroups()
    .filter((g) => /\baria-pressed=/.test(g.inner))
    .map((g) => g.id);
  assert.deepEqual(
    contradictory,
    [],
    "a `role=\"radiogroup\"` whose options carry `aria-pressed` is announced as " +
      '"radio group, N items" and then "toggle button, not pressed" — a direct ' +
      "contradiction, and `aria-pressed` is not an attribute `role=\"radio\"` " +
      "allows. Either bind the group with `bindRadioGroup` from " +
      "`src/radio_group.mjs` (exactly-one-of, always), or make the container " +
      '`role="group"` and keep the toggle buttons (109 UX-021).',
  );
});
