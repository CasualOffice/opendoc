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
// 3.16:1 in dark. Those floors are stated so the palette cannot drift back
// under them — a colour picked by eye is how they got there.
//
// THE TABLES AND THE ARITHMETIC NOW LIVE IN `src/contrast.mjs`, and this file
// reads them. They used to be local, and `docs/126` phase 3 needed the same two
// questions asked of a HOST's palette — which token is text, and what must it
// clear. Two tables saying which token is text would disagree eventually, the way
// the `shortcut:` labels and the key bindings did (`109` UX-006). So there is one
// statement of the roles, one of the floors, and one resolver for `var()` and
// `color-mix()`, shared by this guard and by `tools/build-brand.mjs`.
//
// WHAT THAT CHANGED HERE, deliberately: the palette is now measured MERGED —
// the theme-independent block plus the theme's own — because that is what the
// cascade produces. Measuring a theme block alone could not see any pair whose
// halves live in different blocks, which is every accent pair, because `--accent`
// is theme-independent and `--accent-text` is not. Three pairs are therefore
// checked that nothing checked before: `--accent-ink` on `--accent`,
// `--primary-ink` on `--primary-bg`, and `--accent-text` on `--surface`. All
// three pass today; the mutation proving they can fail is in the PR.
import { TEXT_ROLES, UI_ROLES, PAIRED_ROLES, auditPalette } from "../src/contrast.mjs";
import { readPalettes } from "../tools/palette_source.mjs";

test("every colour role clears its WCAG floor in all three palettes", () => {
  const { themes } = readPalettes(css);
  const failures = [];
  for (const [theme, palette] of themes) {
    const { failures: bad, unreadable } = auditPalette(palette);
    for (const value of unreadable) {
      failures.push(`${theme} ${value} cannot be read as a colour, so nothing measured it`);
    }
    for (const f of bad) {
      failures.push(
        `${theme} ${f.role} ${f.value} on ${f.on} ${f.ground} = ${f.ratio.toFixed(2)}:1 ` +
          `(needs ${f.floor}:1; ${f.suggestion ?? "no value on this hue"} would pass)`,
      );
    }
  }
  assert.deepEqual(failures, []);
});

test("the role tables really cover the palette, so a pass is not a pass by omission", () => {
  // THE HALF THAT FAILS WHEN THE GUARD BREAKS RATHER THAN WHEN THE TREE DOES.
  // `auditPalette` reports nothing for a role it cannot find, so a typo in the
  // table — or a token renamed in the stylesheet — would turn this whole section
  // into a no-op silently. That is the "green because it checked nothing" defect
  // `docs/105` CQ-003 records, so the roles are asserted to EXIST.
  const { themes } = readPalettes(css);
  const missing = [];
  for (const [theme, palette] of themes) {
    for (const role of [...TEXT_ROLES, ...UI_ROLES]) {
      if (palette[role] === undefined) missing.push(`${theme} has no ${role}`);
    }
    for (const pair of PAIRED_ROLES) {
      if (palette[pair.ink] === undefined) missing.push(`${theme} has no ${pair.ink}`);
      if (palette[pair.on] === undefined) missing.push(`${theme} has no ${pair.on}`);
    }
  }
  assert.deepEqual(missing, []);
  assert.ok(TEXT_ROLES.length >= 8, "the text-role table has shrunk; a smaller table checks less");
  assert.ok(PAIRED_ROLES.length >= 3, "the paired-role table has shrunk");
});

// ---- The frame does not move (the owner's "frame of webapp is not fixed") --

test("the app shell refuses to rubber-band, and every chrome scroller contains its own", () => {
  // Nothing in the shell overflows — header, band, work area and status bar
  // are a flex column that fits, and `#viewport` does the scrolling. But with
  // `overscroll-behavior: auto` the browser still bounces the ROOT when a
  // gesture starts somewhere that cannot scroll, so dragging on the header or
  // the toolbar moved the whole frame and sprang back on release.
  const shell = css.match(/html,\s*\nbody\s*\{[^}]*\}/);
  assert.ok(shell, "the html/body rule should still exist");
  assert.match(
    shell[0],
    /overscroll-behavior:\s*none/,
    "the shell must refuse the root's rubber-band",
  );

  // And a scroller inside the chrome must not hand the wheel to the document
  // behind it when it reaches its end: that reads as the dialog stuttering and
  // then the page lurching.
  const chained = [];
  // No `(^|\})` anchor: `matchAll` resumes after the brace it consumed, so an
  // anchored pattern matches only every OTHER rule — which is why the first
  // version of this guard stayed green with `.dialog-body`'s containment
  // deleted.
  for (const [, selector, body] of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (!/overflow(-y)?\s*:\s*(auto|scroll)/.test(body)) continue;
    if (/overscroll-behavior/.test(body)) continue;
    const name = selector.trim().split("\n").pop().trim();
    // `#viewport` IS the document, and chaining from it is the browser's
    // ordinary behaviour rather than a defect.
    if (/viewport|\.page/.test(name)) continue;
    chained.push(name);
  }
  assert.deepEqual(chained, [], "these chrome scrollers chain their overscroll to the document");
});
