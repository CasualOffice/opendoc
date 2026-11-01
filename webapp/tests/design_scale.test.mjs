// The chrome uses ONE scale per property (design system v2, docs/63 §v2,
// `docs/109` UX-052).
//
// The 2026-11-01 UI audit measured the stylesheet: 30 distinct font sizes, 9
// font weights, 31 border radii and 42 box shadows. The tokens for a scale
// already existed; nothing made a rule use them, so every surface was tuned
// locally and the product read as many small designs instead of one. The sweep
// that put every chrome rule on the v2 scale is worthless without a guard,
// because the next rule written with `font-size: 0.6875rem` or
// `border-radius: 5px` starts the drift again and nobody sees it in review.
//
// So in every CHROME rule of the editor's stylesheets:
//
//   * `font-size` is a type token (`--fs-xs|sm|md|lg|xl`: 11/12/13/15/20) or an
//     icon token (`--icon-sm|md|lg`: 16/20/24), or a relative keyword;
//   * `font-weight` is 400, 500 or 600 (or a `--fw-*` token), in the property
//     and in the `font` shorthand;
//   * every `border-radius` component is `0`, `50%` or a `--r-*` token
//     (4/6/8/12/pill);
//   * a `box-shadow` with any BLUR is an elevation token (`--e1|e2|e3`, or the
//     sheet's `--shadow-page`). Spread-only rings (focus rings, inset hairlines)
//     are outlines drawn with a shadow, not elevation, and stay free.
//
// The DOCUMENT region (the sheet, the overlay, the rulers — the same split
// `chrome_units.test.mjs` uses, from `css_regions.mjs`) is out of scope: it is
// drawn at the document's scale. So are the CONTENT PREVIEWS below, which show
// the document's typography rather than the chrome's; each says why.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { declarations, splitTop } from "./css_regions.mjs";

const read = (name) => readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8");
const SHEETS = ["style.css", "font_advanced.css"];

/** Rules that render a PREVIEW of document content, or a glyph composition,
 *  where the size or weight is the subject rather than chrome type. A selector
 *  matching one of these is exempt from the type and weight rules only. */
const CONTENT_PREVIEWS = [
  // Format ▸ Drop cap shows the cap at its document size.
  [/\.drop-cap-sample/, "drop-cap preview"],
  // Insert ▸ Symbol: the glyph grid previews characters, at a size you can read.
  [/\.glyph-cell|\.glyph-grid-emoji/, "symbol preview"],
  // The Styles gallery shows each style in its own size and weight (Word's
  // gallery does the same).
  [/\.style-option-[\w-]+ \.style-option-name|\.style-option-name\b/, "style preview"],
  // The bullet/numbering gallery draws list markers, not chrome text.
  [/\.list-gallery-(?:glyph|sample)/, "list-marker preview"],
  // The font menu lists faces, previewed in their own face.
  [/\.font-menu-item\b/, "font preview"],
  // The Bold button's B is drawn bold: that is what the button means.
  [/\.bold-glyph\b/, "bold glyph"],
  // The product mark is a logotype.
  [/\.brand \.mark\b/, "logotype"],
  // A+/A- and the delete-row/column/table badges compose a glyph out of two
  // parts; the small part is an icon detail, not text.
  [/\.grow-sign\b|\.table-command-badge\b/, "icon composition"],
  // The empty-state "drop a file" illustration.
  [/\.drop-icon\b/, "illustration"],
  // A coarse pointer's inputs are 16px so iOS does not zoom the page on focus.
  [/^(?:input|select|textarea):not\(/, "iOS focus-zoom floor"],
  // The root font size IS the scale's unit.
  [/^html$/, "root unit"],
];

const TYPE = /^var\(--(?:fs-(?:xs|sm|md|lg|xl)|icon-(?:sm|md|lg))\)$/;
const RELATIVE = /^(?:inherit|initial|unset|[\d.]+em|[\d.]+%|smaller|larger)$/;
const WEIGHT = /^(?:400|500|600|normal|inherit|var\(--fw-(?:regular|medium|semibold)\))$/;
const RADIUS = /^(?:0|50%|inherit|var\(--r-(?:xs|sm|md|lg|pill)\))$/;
const ELEVATION = /var\(--(?:e1|e2|e3|shadow-page)\)/g;

const previewReason = (selector) =>
  splitTop(selector, ",").every((part) => CONTENT_PREVIEWS.some(([re]) => re.test(part.trim())));

/** Offenders in one sheet's source, as readable lines. Exported shape is local:
 *  the mutation test below calls it on hand-written CSS. */
function offenders(source, sheet) {
  const out = [];
  for (const d of declarations(source)) {
    if (d.document || d.property.startsWith("--")) continue;
    const where = `${sheet}:${d.line} ${d.selector.slice(0, 90)} { ${d.property}: ${d.value} }`;
    const preview = previewReason(d.selector);
    if (d.property === "font-size" && !preview && !TYPE.test(d.value) && !RELATIVE.test(d.value)) {
      out.push(`${where} — a font size is --fs-xs/sm/md/lg/xl or --icon-sm/md/lg`);
    }
    if (d.property === "font-weight" && !preview && !WEIGHT.test(d.value)) {
      out.push(`${where} — weights are 400, 500 and 600`);
    }
    if (d.property === "font" && !preview) {
      const weight = d.value.match(/^(\d{3})\b/);
      if (weight && !WEIGHT.test(weight[1])) out.push(`${where} — weights are 400, 500 and 600`);
      const size = d.value.match(/(?:^|\s)([\d.]+(?:rem|px))\s*\//);
      if (size) out.push(`${where} — the shorthand's size must be a --fs-* token`);
    }
    if (d.property === "border-radius") {
      const parts = splitTop(d.value, " /").filter((p) => p.trim());
      if (parts.some((p) => !RADIUS.test(p.trim()))) {
        out.push(`${where} — a radius is 0, 50% or --r-xs/sm/md/lg/pill`);
      }
    }
    if (d.property === "box-shadow") {
      for (const layer of splitTop(d.value.replace(ELEVATION, ""), ",")) {
        const lengths = [...layer.matchAll(/(?<![\w.-])-?(\d*\.?\d+)(?:px|rem|em)?(?![\w.%])/g)]
          .map((m) => Number(m[1]))
          .filter((n) => !Number.isNaN(n));
        // x, y, blur, spread: a non-zero BLUR is elevation.
        if (lengths.length >= 3 && lengths[2] !== 0 && !/rgba?\(|color-mix/.test(layer.split(/\s/)[0])) {
          out.push(`${where} — a blurred shadow is elevation: use --e1, --e2 or --e3`);
          break;
        }
      }
    }
  }
  return out;
}

test("every chrome rule is on the v2 type, weight, radius and elevation scale", () => {
  const bad = [];
  let read_ = 0;
  for (const sheet of SHEETS) {
    const source = read(sheet);
    read_ += declarations(source).length;
    bad.push(...offenders(source, sheet));
  }
  assert.ok(read_ > 5000, `expected to read the stylesheet's declarations, read ${read_}`);
  assert.deepEqual(
    bad,
    [],
    "the chrome has ONE scale per property (docs/63 §v2). Use the token; if the rule draws " +
      "document content rather than chrome, add it to CONTENT_PREVIEWS with the reason:\n  " +
      bad.join("\n  "),
  );
});

test("the scale tokens are declared once, at the values the design states", () => {
  const css = read("style.css").replace(/\/\*[\s\S]*?\*\//g, "");
  const value = (name) => css.match(new RegExp(`\\n\\s*${name}:\\s*([^;]+);`))?.[1]?.trim();
  const STATED = {
    "--fs-xs": "0.6875rem",
    "--fs-sm": "0.75rem",
    "--fs-md": "0.8125rem",
    "--fs-lg": "0.9375rem",
    "--fs-xl": "1.25rem",
    "--fw-regular": "400",
    "--fw-medium": "500",
    "--fw-semibold": "600",
    "--icon-sm": "1rem",
    "--icon-md": "1.25rem",
    "--icon-lg": "1.5rem",
    "--r-xs": "0.25rem",
    "--r-sm": "0.375rem",
    "--r-md": "0.5rem",
    "--r-lg": "0.75rem",
    "--h-dense": "1.5rem",
    "--h-tool": "1.75rem",
    "--h-ctl": "2rem",
    "--h-touch": "2.75rem",
  };
  for (const [name, expected] of Object.entries(STATED)) {
    assert.equal(value(name), expected, `${name} is the design's ${expected}`);
  }
});

test("the guard catches each kind of drift, and spares the document and previews", () => {
  // The mutation, run every time: one offender per rule, none for the rules
  // that are exempt for a stated reason.
  const mutated = [
    ".ribbon-tab { font-size: 0.84375rem; }",
    ".btn { font-weight: 550; }",
    ".menu { border-radius: 0.3125rem; }",
    ".card { box-shadow: 0 0.25rem 0.875rem rgba(0, 0, 0, 0.24); }",
    ".tip { font: 700 var(--fs-xs)/1 inherit; }",
    ".ok { font-size: var(--fs-sm); font-weight: 600; border-radius: var(--r-sm) 0 0 var(--r-sm); }",
    ".ring { box-shadow: 0 0 0 2px var(--accent-soft), inset 0 0 0 1px var(--line); }",
    ".lifted { box-shadow: var(--e2), 0 0 0 2px var(--accent-text); }",
    ".overlay .object-handle { border-radius: 2px; box-shadow: 0 1px 2px #000; }",
    ".drop-cap-sample b { font-size: 2.9375rem; font-weight: 700; }",
  ].join("\n");
  const found = offenders(mutated, "mutation.css");
  assert.equal(found.length, 5, found.join("\n"));
  assert.match(found[0], /\.ribbon-tab/);
  assert.match(found[1], /\.btn/);
  assert.match(found[2], /\.menu/);
  assert.match(found[3], /\.card/);
  assert.match(found[4], /\.tip/);
});

// ---- Spacing, control heights, and the plugin contract's old names ----------
//
// Phase 1 also puts spacing on the grid and the heights that already equal a
// rung on the rung's token. The grid is 4px with a 2px half-step for the inside
// of dense controls (docs/63 §7.1); an odd pixel is never on it. A 1px length
// is a hairline (a border's width, an `.sr-only` clip), not spacing, and stays.
// Lengths inside a function (`calc`, `clamp`, a `var` fallback) are arithmetic
// on tokens and are not read.
const SPACING = /^(?:padding|margin|gap|row-gap|column-gap)(?:-[a-z-]+)?$/;
const HEIGHT = /^(?:height|min-height|block-size|min-block-size)$/;
const HEIGHT_RUNGS = { "1.5rem": "--h-dense", "1.75rem": "--h-tool", "2rem": "--h-ctl", "2.75rem": "--h-touch" };
/** A height equal to a rung that is NOT a control's height, each with why. */
const NOT_CONTROLS = [
  // The About dialog's customer logo is a 28px image, sized as artwork.
  [/^\.about-customer-mark$/, "logo artwork"],
];
/** The pre-v2 names `style.css` still declares, ONLY as aliases for the plugin
 *  contract (`PLUGIN_TOKENS`, `docs/183`). No editor rule may read one. */
const PLUGIN_ALIASES = [
  "--radius",
  "--radius-sm",
  "--radius-md",
  "--radius-popover",
  "--fs-body",
  "--fs-small",
  "--fs-caption",
  "--shadow-popover",
];
const ALIAS_READ = new RegExp(`var\\(\\s*(?:${PLUGIN_ALIASES.join("|")})\\s*[,)]`);

function gridOffenders(source, sheet) {
  const out = [];
  for (const d of declarations(source)) {
    if (d.document || d.property.startsWith("--")) continue;
    const where = `${sheet}:${d.line} ${d.selector.slice(0, 90)} { ${d.property}: ${d.value} }`;
    if (ALIAS_READ.test(d.value)) {
      out.push(`${where} — a pre-v2 name is a plugin-contract alias; use its v2 token`);
    }
    if (SPACING.test(d.property)) {
      let depth = 0;
      for (const token of d.value.split(/(\(|\)|\s+)/)) {
        if (token === "(") depth += 1;
        else if (token === ")") depth -= 1;
        const m = depth === 0 && token.match(/^-?(\d*\.?\d+)(rem|px)$/);
        if (!m) continue;
        const px = m[2] === "rem" ? Number(m[1]) * 16 : Number(m[1]);
        if (px !== 1 && Math.abs(px / 2 - Math.round(px / 2)) > 1e-9) {
          out.push(`${where} — spacing is on the 4px grid (2px half-step): ${px}px is not`);
          break;
        }
      }
    }
    if (HEIGHT.test(d.property)) {
      const v = d.value.replace(/\s*!important$/, "");
      const named = splitTop(d.selector, ",").every((part) => NOT_CONTROLS.some(([re]) => re.test(part.trim())));
      if (HEIGHT_RUNGS[v] && !named) {
        out.push(`${where} — ${v} is a height rung: write var(${HEIGHT_RUNGS[v]})`);
      }
    }
  }
  return out;
}

test("chrome spacing is on the grid, rung heights use their token, no rule reads an alias", () => {
  const bad = [];
  for (const sheet of SHEETS) bad.push(...gridOffenders(read(sheet), sheet));
  assert.deepEqual(bad, [], `docs/63 §7.1:\n  ${bad.join("\n  ")}`);
});

test("every token the plugin platform publishes is declared, aliases included", () => {
  // A token a plugin is promised but the editor never declares reaches the
  // plugin as nothing at all: `tokens()` drops an empty value, and the plugin's
  // `var(--fs-body)` falls back to the UA default. That is exactly what merging
  // v2 (which deleted the pre-v2 names) with the plugin platform (which
  // publishes them) produced, with every other test green.
  const platform = readFileSync(new URL("../src/plugin_platform.mjs", import.meta.url), "utf8");
  const list = platform.match(/PLUGIN_TOKENS = Object\.freeze\(\[([\s\S]*?)\]\)/)?.[1] ?? "";
  const names = [...list.matchAll(/"(--[\w-]+)"/g)].map((m) => m[1]);
  assert.ok(names.length > 30, `read PLUGIN_TOKENS, got ${names.length}`);
  const css = read("style.css").replace(/\/\*[\s\S]*?\*\//g, "");
  // `--brand-font` is a white-label seam a HOST sets (docs/126); every reader
  // carries a fallback, so it is unset by design until a host names a face.
  const HOST_SET = ["--brand-font"];
  const missing = names.filter((n) => !HOST_SET.includes(n) && !new RegExp(`(?:^|[\\s;{])${n}\\s*:`).test(css));
  assert.deepEqual(missing, [], "PLUGIN_TOKENS names a token style.css never declares");
  for (const alias of PLUGIN_ALIASES) assert.ok(names.includes(alias), `${alias} is kept only for PLUGIN_TOKENS`);
});

test("the grid guard catches each kind of drift", () => {
  const mutated = [
    ".a { padding: 0.3125rem 0.5rem; }",
    ".b { gap: 0.4375rem; }",
    ".c { margin: 0 0 0.5625rem; }",
    ".d { min-height: 2rem; }",
    ".e { font-size: var(--fs-body); }",
    ".ok { padding: 0.375rem 0.5rem; margin: -1px; gap: calc(0.3125rem + 1px); height: var(--h-ctl); }",
    ".about-customer-mark { height: 1.75rem; }",
    ".overlay .x { padding: 0.3125rem; }",
  ].join("\n");
  const found = gridOffenders(mutated, "mutation.css");
  assert.equal(found.length, 5, found.join("\n"));
  for (const [i, sel] of ["\\.a", "\\.b", "\\.c", "\\.d", "\\.e"].entries()) {
    assert.match(found[i], new RegExp(sel));
  }
});
