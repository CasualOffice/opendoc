// The deck viewer's page has a stylesheet, and every class in it is styled.
//
// # Why this file exists
//
// `slides.html` shipped with a complete class vocabulary and NOT ONE RULE for any
// of it, and every guard the page had passed anyway. The unit tests drive
// `slides.mjs` with stubs and never touch the DOM; the browser specs query
// elements by id and read pixels out of a canvas, and a canvas with a backing
// store paints correctly whether or not anything around it has a layout. So the
// page rendered as bare HTML — a plain file input, an invisible brand mark, a
// zero-height sorter — and the first person to find out was the person who opened
// it. That is `SKILL` §9.4 exactly: built is not reachable.
//
// The lesson generalises past this one page: a guard that asserts on STRUCTURE
// cannot see presentation, so the thing to assert is that presentation EXISTS for
// every name the structure uses. That is cheap, it is mechanical, and it is the
// assertion that fails on an unstyled page.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");
const PAGE = readFileSync(join(WEBAPP, "slides.html"), "utf8");
const SLIDES_CSS = readFileSync(join(WEBAPP, "src/slides.css"), "utf8");
const STYLE_CSS = readFileSync(join(WEBAPP, "src/style.css"), "utf8");

/** Drops comments, so prose mentioning a class or a colour is not read as code. */
function withoutComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** Every class token the markup uses, including the ones script adds. */
function classesIn(markup) {
  const found = new Set();
  for (const match of markup.matchAll(/\bclass="([^"]+)"/g)) {
    for (const token of match[1].split(/\s+/)) if (token) found.add(token);
  }
  return found;
}

/** Every class token that appears in a SELECTOR position in `css`. */
function styledClasses(css) {
  const found = new Set();
  // Selector text only: everything up to the first `{` of each rule. A class
  // named inside a declaration value (a `content:` string, a url) is not a rule
  // for it, and counting one would let this test pass on a page whose classes are
  // merely mentioned.
  for (const rule of withoutComments(css).split("}")) {
    const selector = rule.split("{")[0] ?? "";
    for (const match of selector.matchAll(/\.([A-Za-z_][\w-]*)/g)) found.add(match[1]);
  }
  return found;
}

test("every class the slides page uses is actually styled", () => {
  const styled = new Set([...styledClasses(SLIDES_CSS), ...styledClasses(STYLE_CSS)]);
  const unstyled = [...classesIn(PAGE)].filter((name) => !styled.has(name)).sort();
  assert.deepEqual(
    unstyled,
    [],
    "a class in the markup with no rule anywhere is an element with no layout — " +
      "which is how this page shipped looking blank",
  );
});

test("every class the viewer's SCRIPT creates is styled too", () => {
  // The sorter is built in script, so its classes never appear in the markup and
  // the test above cannot see them. They are the ones a reader actually looks at.
  const script = readFileSync(join(WEBAPP, "src/slides_page.mjs"), "utf8");
  const created = new Set();
  for (const match of script.matchAll(/\.className = "([^"]+)"/g)) {
    for (const token of match[1].split(/\s+/)) if (token) created.add(token);
  }
  assert.ok(created.size >= 4, `the sorter builds several classed elements: ${[...created]}`);
  const styled = new Set([...styledClasses(SLIDES_CSS), ...styledClasses(STYLE_CSS)]);
  const unstyled = [...created].filter((name) => !styled.has(name)).sort();
  assert.deepEqual(unstyled, [], "a thumbnail with no rules is an unstyled list item");
});

test("the page LOADS the stylesheet it depends on", () => {
  // The previous two tests pass on a page that never links the file. This is the
  // other half, and it is not hypothetical: `src/slides.css` did not exist at all
  // and nothing noticed.
  assert.match(
    PAGE,
    /<link rel="stylesheet" href="\.\/src\/slides\.css" \/>/,
    "slides.html must link src/slides.css",
  );
  // After style.css, so the page-scoped overrides of the editor's `.bar` grid win
  // by ordinary cascade order rather than by `!important`.
  assert.ok(
    PAGE.indexOf("src/slides.css") > PAGE.indexOf("src/style.css"),
    "slides.css must come after style.css",
  );
  // A MODE class from the first paint: `style.css` shows exactly one of the
  // compact toolbar and the ribbon under `compact-mode`/`ribbon-mode`, so a page
  // with neither class gets neither bar until script runs. Ribbon, because it is
  // the editor's default too; `slides_chrome.mjs` then applies the reader's
  // saved choice from the editor's own preference.
  assert.match(PAGE, /<body class="(ribbon|compact)-mode">/);
});

test("the slides stylesheet writes no bare colour", () => {
  // The same colour contract `style.css` states for itself and
  // `style_tokens.test.mjs` enforces there: a literal is a colour with no dark
  // value, and this page gets a dark mode for free only because every colour in
  // it is a token both theme blocks declare.
  const literals = [
    ...withoutComments(SLIDES_CSS).matchAll(/#[0-9a-fA-F]{3,8}\b|\brgba?\(|\bhsla?\(/g),
  ].map((match) => match[0]);
  assert.deepEqual(
    literals,
    [],
    "colour comes from a token, never from a literal — otherwise dark mode has no value for it",
  );
});

test("the viewport the slide is sized from has no HORIZONTAL padding", () => {
  // `slides.mjs` reads `#slideStage.getBoundingClientRect().width` as the width a
  // slide may occupy, and `getBoundingClientRect` INCLUDES padding. The stage is
  // the editor's `.viewport`, whose padding is `12px 0 48px` — vertical only —
  // so the measurement is the desk's true width today. That is a cross-file
  // assumption this page depends on and does not own, which is exactly the kind
  // worth pinning: a left/right padding added to `.viewport` for the document
  // editor would silently push every slide past the right edge of the desk.
  const rule = withoutComments(STYLE_CSS)
    .split("}")
    .find((block) => /(^|\s)\.viewport\s*\{/.test(block));
  assert.ok(rule, "the editor's viewport has a rule");
  const padding = rule.match(/padding:\s*([^;]+);/)?.[1]?.trim();
  assert.ok(padding, `.viewport declares a padding: ${rule}`);
  const sides = padding.split(/\s+/);
  // One value applies to all four sides; two or three put the second on the
  // horizontal axis; four list top/right/bottom/left.
  const horizontal =
    sides.length === 1 ? [sides[0], sides[0]] : [sides[1], sides[3] ?? sides[1]];
  assert.deepEqual(
    horizontal.map((value) => value.replace(/0[a-z%]*/, "0")),
    ["0", "0"],
    "a horizontal padding on .viewport is counted as available slide width",
  );
});

test("the slide is inset from the desk edges by the page, not by padding", () => {
  // The other half: the breathing room either side of the sheet comes from
  // `DESK_MARGIN_PX` in the page module. Without it the slide runs edge to edge
  // and loses the shadow that makes it read as a sheet at all — and because the
  // viewport has no horizontal padding, nothing else would supply it.
  const page = readFileSync(join(WEBAPP, "src/slides_page.mjs"), "utf8");
  assert.match(page, /const DESK_MARGIN_PX = \d+;/, "the desk margin is a named constant");
  assert.match(
    page,
    /getBoundingClientRect\?\.\(\);?[\s\S]{0,160}DESK_MARGIN_PX/,
    "and it is subtracted from the measured stage width",
  );
});
