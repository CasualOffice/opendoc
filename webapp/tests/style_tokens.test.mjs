// Guards the editor's colour contract (docs/104 theme T-08 / T-17).
//
// Three separate defects in the hotfix tracker were the same mistake made three
// times, and each was invisible until someone switched theme:
//
//   HF-021  the whole review surface was written in light-mode literals, so
//           tracked-change text sat at ~2.4:1 on the dark surface;
//   HF-092  three dark patches were written as bare `prefers-color-scheme`
//           blocks with no explicit-dark twin, so picking Dark on a light OS
//           got none of them;
//   HF-086  three rules referenced `--bg-1`, which nothing ever defined, so the
//           header-band label inherited --ink onto an accent fill and the
//           "Add header" chip lost its background entirely.
//
// None of the three could be caught by a test that renders the light theme, and
// the editor's e2e suite renders the light theme. So they are caught here, from
// the stylesheet text, where the invariants are actually stated:
//
//   1. colour literals live in the palette blocks and nowhere else;
//   2. every semantic token is declared in the light block AND in both dark
//      entry points, with the two dark blocks in exact agreement;
//   3. every token the stylesheet reads without a fallback is one it defines.
//
// Each of these has already been violated in shipped code. They are not
// hypothetical.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const STYLE_PATH = new URL("../src/style.css", import.meta.url);

/** Drops /* … *\/ comments so prose about colours is not mistaken for colours. */
function stripComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** The declaration body of the first rule whose selector text matches. */
function ruleBody(css, selectorPattern) {
  const source = stripComments(css);
  const match = source.match(selectorPattern);
  assert.ok(match, `no rule matched ${selectorPattern}`);
  const start = source.indexOf("{", match.index);
  let depth = 0;
  for (let i = start; i < source.length; i += 1) {
    if (source[i] === "{") depth += 1;
    else if (source[i] === "}") {
      depth -= 1;
      if (depth === 0) return source.slice(start + 1, i);
    }
  }
  throw new Error(`unterminated rule for ${selectorPattern}`);
}

/** Custom-property declarations in a rule body, as name → value. */
function declaredTokens(body) {
  const tokens = new Map();
  for (const [, name, value] of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    tokens.set(name, value.trim().replace(/\s+/g, " "));
  }
  return tokens;
}

const css = await readFile(STYLE_PATH, "utf8");

// The palette region: everything up to and including the explicit-dark block.
// Literals are legal here and only here.
const paletteEnd = css.indexOf("}", css.indexOf('--review-move: #', css.indexOf(':root[data-theme="dark"]')));
assert.ok(paletteEnd > 0, "could not locate the end of the explicit-dark palette block");
const paletteRegion = css.slice(0, paletteEnd + 1);
const featureRegion = css.slice(paletteEnd + 1);

test("no colour literal escapes the palette blocks", () => {
  const stray = [];
  const source = stripComments(featureRegion);
  const offsetOfLine = (index) => featureRegion.slice(0, index).split("\n").length;
  for (const match of source.matchAll(/#[0-9a-fA-F]{3,8}\b/g)) {
    stray.push(`${match[0]} (near feature-CSS line ${offsetOfLine(match.index)})`);
  }
  assert.deepEqual(
    stray,
    [],
    "colour literals must be promoted to a token defined in all three palette " +
      "blocks; a literal in feature CSS has no dark value by construction",
  );
});

test("every semantic token is declared in the light block and both dark blocks", () => {
  const light = declaredTokens(ruleBody(paletteRegion, /:root,\s*\n:root\[data-theme="light"\]/));
  const systemDark = declaredTokens(
    ruleBody(paletteRegion, /:root:not\(\[data-theme\]\)/),
  );
  const explicitDark = declaredTokens(ruleBody(paletteRegion, /:root\[data-theme="dark"\]/));

  assert.ok(light.size > 20, "the light palette block should carry the whole token set");

  const names = (map) => [...map.keys()].sort();
  assert.deepEqual(
    names(systemDark),
    names(light),
    "the system-dark block must redefine exactly the tokens the light block defines",
  );
  assert.deepEqual(
    names(explicitDark),
    names(light),
    "the explicit-dark block must redefine exactly the tokens the light block " +
      "defines — a user who picks Dark on a light OS gets only this block",
  );
  assert.deepEqual(
    Object.fromEntries(explicitDark),
    Object.fromEntries(systemDark),
    "the two dark entry points must agree value-for-value; they are one palette " +
      "that plain CSS cannot express as one rule",
  );
});

test("no rule reads a token the stylesheet never defines", () => {
  const source = stripComments(css);
  const defined = new Set([...source.matchAll(/(--[\w-]+)\s*:/g)].map((m) => m[1]));
  const missing = new Set();
  for (const [, name, next] of source.matchAll(/var\(\s*(--[\w-]+)\s*([,)])/g)) {
    // A var() with a fallback is a deliberate default for a property some other
    // layer sets (JS writes --sw, --review-author-color, --page-ratio inline).
    if (next === ",") continue;
    if (!defined.has(name)) missing.add(name);
  }
  assert.deepEqual(
    [...missing].sort(),
    [],
    "an undefined custom property makes `color` inherit and `background` " +
      "transparent, silently — exactly how --bg-1 shipped (HF-086)",
  );
});

// ---- Measured contrast -------------------------------------------------------
// The tokens above are only as good as their numbers, and the numbers are what
// the tracker rows were actually about: review deletion text measured 2.38:1 on
// the dark surface, the mode pill 2.62:1, and --faint 3.29:1 in light and
// 3.16:1 in dark. Those floors are stated here so the palette cannot drift back
// under them — a colour picked by eye is how they got there.

/** WCAG 2.1 relative luminance / contrast, on plain sRGB hex. */
function channels(hex) {
  let value = hex.replace("#", "");
  if (value.length === 3) value = [...value].map((c) => c + c).join("");
  return [0, 2, 4].map((i) => Number.parseInt(value.slice(i, i + 2), 16));
}

function luminance(hex) {
  const [r, g, b] = channels(hex).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// Roles used directly as text on --surface: the 4.5:1 body-text floor.
const TEXT_ROLES = [
  "--ink",
  "--muted",
  "--faint",
  "--success",
  "--warning",
  "--danger",
  "--info",
  "--review-move",
];

// Roles used directly as a border, rule or edge: the 3:1 non-text floor. Only
// the ones a rule sets straight onto the surface are listed; a -line token that
// is always mixed into another colour first (--warning-line, via color-mix) is
// read at its mixed value, not at this one.
const UI_ROLES = ["--success-line", "--danger-line", "--info-line", "--review-format-line"];

test("every colour role clears its WCAG floor in both themes", () => {
  const themes = {
    light: declaredTokens(ruleBody(paletteRegion, /:root,\s*\n:root\[data-theme="light"\]/)),
    "system dark": declaredTokens(ruleBody(paletteRegion, /:root:not\(\[data-theme\]\)/)),
    "explicit dark": declaredTokens(ruleBody(paletteRegion, /:root\[data-theme="dark"\]/)),
  };
  const failures = [];
  for (const [theme, tokens] of Object.entries(themes)) {
    const surface = tokens.get("--surface");
    for (const [roles, floor] of [
      [TEXT_ROLES, 4.5],
      [UI_ROLES, 3],
    ]) {
      for (const role of roles) {
        const value = tokens.get(role);
        assert.ok(value, `${role} is not declared in the ${theme} palette`);
        assert.match(value, /^#[0-9a-f]{3,6}$/i, `${role} must be a literal in the palette`);
        const ratio = contrast(value, surface);
        if (ratio < floor) {
          failures.push(
            `${theme} ${role} ${value} on ${surface} = ${ratio.toFixed(2)}:1 (needs ${floor}:1)`,
          );
        }
      }
    }
  }
  assert.deepEqual(failures, []);
});
