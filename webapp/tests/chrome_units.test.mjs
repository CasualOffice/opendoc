// The chrome is measured in rem; the document is measured in px (`docs/109`
// HF-413).
//
// The owner, 2026-10-30: "Switch the chrome from px to rem, so it can be
// adjusted based on resolution changes" — "and adjust rem based on resolution,
// and zoom." Every chrome length in the editor's stylesheets is written in rem
// against a fluid root, so the header, ribbon, menus, dialogs, panels and status
// bar scale from ONE number. A px length written into a chrome rule opts that
// one rule out: it stays the same size while everything around it grows or
// shrinks, which is a misaligned ribbon at 1920px and a clipped label at 1024px,
// and nobody sees it at the 1280px the screenshots are taken at — because the
// curve is flat at 16px exactly there, by design.
//
// So this guard fails on any px length in a chrome rule outside a short list of
// things that are px ON PURPOSE:
//
//   * hairlines — a value of at most 1px anywhere, and the border/outline
//     family (`border*`, `outline*`, `column-rule`, `text-decoration*`,
//     `text-underline-offset`): a line is crisp at a whole device pixel and a
//     rem border would be 1.25px at a 20px root;
//   * shadow components of at most 2px (the contact shadow and 2px rings);
//   * the DOCUMENT region — the sheet, the page stack and its gap, the rulers,
//     the caret, and everything painted onto the raster under `.overlay`.
//     The layout engine and `scaleOf(page)` measure those in px, the editor's
//     zoom control scales them, and they must not move with the chrome's root;
//   * `--shadow-page`, the sheet's own shadow, declared as a token, and the
//     `var(--page-width, 816px)` fallback, which is the sheet's width.
//
// And the breakpoints: `@media` in em (media queries resolve em against the
// INITIAL font size, so the rungs follow zoom and the reader's own font setting
// but not the fluid root), `@container` in rem (a container query resolves em
// against the CONTAINER's font size, which is not the 16px the rung was
// measured at).
//
// Plus the curve itself, evaluated rather than pattern-matched: 16px across
// 1280-1440 (the ribbon's budget width), 14px at 1024 and 20px at 2560, never
// decreasing, and a coarse pointer never below 100%.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const read = (name) => readFileSync(new URL(`../src/${name}`, import.meta.url), "utf8");

/** The editor's chrome stylesheets. `marketing.css` is the website, not the
 *  editor, and `fonts.css`/`brand.css` declare faces and a mark, no lengths. */
const SHEETS = ["style.css", "font_advanced.css", "fonts.css", "brand.css"];

/** A selector's SUBJECT (its last compound) naming one of these is document
 *  region; so is any selector under `.overlay`. Keep this list in step with
 *  the prose at the top of the file and with `style.css`'s HF-413 block. */
const DOCUMENT_SUBJECT = new RegExp(
  [
    String.raw`\.viewport\b`,
    String.raw`#viewport\b`,
    String.raw`#pages\b`,
    String.raw`\.pages(?![\w-])`,
    String.raw`\.page(?![\w-])`,
    String.raw`\.page-wrap\b`,
    String.raw`\.page-band\b`,
    String.raw`\.ruler`,
    String.raw`\.tab-corner\b`,
    String.raw`\.tab-glyph\b`,
    String.raw`\.caret(?![\w-])`,
    String.raw`\.remote-(?:caret|selection)\b`,
    String.raw`\.ime-preedit\b`,
    String.raw`\.editor-text-input\b`,
    String.raw`\.running-(?:band|marker)`,
    String.raw`\.fold-body-chevron\b`,
    String.raw`\.touch-(?:handle|magnifier)\b`,
    String.raw`\.reflow-table-`,
    String.raw`\.is-reflow\b`,
    String.raw`\.table-(?:gutter|col-|row-|column-strip|insert-target|move-indicator|cell-selection)`,
    String.raw`\.cell-outline\b`,
    String.raw`\.object-(?:align-guide|crop-|drop-caret|handle|move-pad|outline|resize-preview|rotate-handle|rotate-preview|rotate-tether)`,
    String.raw`\.review-margin-add\b`,
    String.raw`\.review-[\w-]*-marker\b`,
    String.raw`\.checklist-marker\b`,
  ].join("|"),
);

const HAIRLINE_PROPERTY =
  /^(?:border(?:-(?:top|right|bottom|left|inline|block)(?:-(?:start|end))?)?(?:-width)?|outline(?:-width|-offset)?|column-rule(?:-width)?|text-decoration(?:-thickness)?|text-underline-offset|-webkit-text-stroke(?:-width)?|stroke-width)$/;
const DOCUMENT_TOKENS = new Set(["--shadow-page"]);
/** The root curve's anchors (`100vw - 1280px`) are viewport WIDTHS, not chrome
 *  lengths: they say where the curve bends, and the last test evaluates them. */
const CURVE_TOKENS = new Set(["--chrome-root"]);
const PX = /(?<![\w.#-])-?(\d*\.?\d+)px\b/g;

/** Blank out comments and quoted strings, keeping offsets, so prose and
 *  `content:` text are never read as lengths. */
function mask(css) {
  return css.replace(/\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'/g, (m) =>
    m.replace(/[^\n]/g, " "),
  );
}

/** Split on `seps` at bracket depth zero. */
function splitTop(text, seps) {
  const out = [];
  let depth = 0;
  let cur = "";
  for (const ch of text) {
    if (ch === "(" || ch === "[") depth += 1;
    else if (ch === ")" || ch === "]") depth -= 1;
    if (depth === 0 && seps.includes(ch)) {
      out.push(cur);
      cur = "";
    } else cur += ch;
  }
  out.push(cur);
  return out;
}

function isDocumentSelector(prelude) {
  return splitTop(prelude, ",").some((selector) => {
    if (selector.includes(".overlay")) return true;
    const parts = splitTop(selector.trim(), " >+~\n\t").filter((p) => p.trim());
    return parts.length > 0 && DOCUMENT_SUBJECT.test(parts[parts.length - 1]);
  });
}

/** Every px length a chrome rule may not carry, as readable strings. */
function chromePxOffenders(source, sheet = "style.css") {
  const css = mask(source);
  const offenders = [];
  const stack = [];
  let checked = 0;
  let segment = 0;
  const lineOf = (at) => css.slice(0, at).split("\n").length;

  const declarations = (start, end) => {
    const top = stack[stack.length - 1];
    if (!top || top.prelude.startsWith("@") || top.document) return;
    let offset = start;
    for (const decl of css.slice(start, end).split(";")) {
      const at = offset;
      offset += decl.length + 1;
      const colon = decl.indexOf(":");
      if (colon < 0) continue;
      const property = decl.slice(0, colon).trim().toLowerCase();
      const value = decl.slice(colon + 1);
      // `--page-width` is the SHEET's width, published by the renderer in
      // document px; its Letter fallback is a document length even inside a
      // chrome rule that subtracts it (the comment column's width).
      const chrome = value.replace(/var\(--page-width,\s*[\d.]+px\)/g, "var(--page-width)");
      const lengths = [...chrome.matchAll(PX)].map((m) => Number(m[1]));
      if (lengths.length === 0) continue;
      checked += 1;
      if (HAIRLINE_PROPERTY.test(property) || DOCUMENT_TOKENS.has(property)) continue;
      if (CURVE_TOKENS.has(property)) continue;
      const limit = property.includes("shadow") ? 2 : 1;
      const bad = lengths.filter((n) => n !== 0 && n > limit);
      if (bad.length === 0) continue;
      offenders.push(
        `${sheet}:${lineOf(at)} ${top.prelude.split("\n")[0].trim()} { ${property}:${value.trim()} }`,
      );
    }
  };

  for (let i = 0; i < css.length; i++) {
    const ch = css[i];
    if (ch === "{") {
      const raw = css.slice(segment, i);
      const prelude = raw.split(";").pop().trim();
      if (prelude.startsWith("@media") && /\d+(?:\.\d+)?px\b/.test(prelude)) {
        offenders.push(`${sheet}:${lineOf(i)} ${prelude} — a breakpoint goes in em`);
      }
      if (prelude.startsWith("@container") && /\d+(?:\.\d+)?(?:px|em)\b/.test(prelude.replace(/rem\b/g, ""))) {
        offenders.push(`${sheet}:${lineOf(i)} ${prelude} — a container query goes in rem`);
      }
      const inherited = stack.some((s) => s.document);
      stack.push({
        prelude,
        document: inherited || (!prelude.startsWith("@") && isDocumentSelector(prelude)),
      });
      segment = i + 1;
    } else if (ch === "}") {
      declarations(segment, i);
      stack.pop();
      segment = i + 1;
    }
  }
  return { offenders, checked };
}

test("no chrome rule in the editor's stylesheets carries a px length", () => {
  let total = 0;
  const offenders = [];
  for (const sheet of SHEETS) {
    const result = chromePxOffenders(read(sheet), sheet);
    total += result.checked;
    offenders.push(...result.offenders);
  }
  // Non-vacuous: the sheet has hundreds of hairline declarations, so a parser
  // that stopped reading rules would report a suspiciously small number here.
  assert.ok(total > 200, `expected to read the chrome's px declarations, read ${total}`);
  assert.deepEqual(
    offenders,
    [],
    "the chrome is rem (docs/109 HF-413): write the length as n/16 rem — 12px is 0.75rem — " +
      "or, if the rule draws on the DOCUMENT, add its subject to DOCUMENT_SUBJECT with a reason:\n  " +
      offenders.join("\n  "),
  );
});

test("the guard can tell a chrome padding from a hairline and from the document", () => {
  // The mutation the guard exists for, run every time rather than once: a
  // ribbon rule with a px padding is caught, while the same length on a border,
  // on the page and under the overlay is not.
  const mutated = [
    ".ribbon-panel { padding: 12px; border-bottom: 1px solid red; }",
    ".rgroup { gap: 0.25rem; margin: -1px; box-shadow: 0 1px 2px #000; }",
    ".page-wrap { margin: 22px auto; }",
    ".overlay .object-handle { width: 9px; }",
    "@media (max-width: 620px) { .bar { padding: 0 0.5rem; } }",
  ].join("\n");
  const { offenders } = chromePxOffenders(mutated, "mutation.css");
  assert.equal(offenders.length, 2, offenders.join("\n"));
  assert.match(offenders[0], /\.ribbon-panel \{ padding:12px/);
  assert.match(offenders[1], /@media \(max-width: 620px\)/);
});

// ---- The root curve ---------------------------------------------------------

/** The `--chrome-root` declared in each `html` rule, with its media prelude. */
function rootRules(css) {
  const code = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rungs = [];
  const re = /(@media[^{]*)?\{\s*html\s*\{([^}]*)\}\s*\}|(?:^|\n)html\s*\{([^}]*)\}/g;
  for (const m of code.matchAll(re)) {
    const body = m[2] ?? m[3];
    const value = body.match(/--chrome-root:\s*([^;]+);/)?.[1]?.trim();
    if (value) rungs.push({ media: (m[1] ?? "").trim(), value, body });
  }
  return rungs;
}

/** Evaluate `100%`, or `clamp(a%, calc(100% + (100vw - Npx) / D), b%)`, in px
 *  for a 16px initial font size and a viewport `width` px wide. */
function evaluate(value, width) {
  const base = 16;
  if (/^100%$/.test(value)) return base;
  const m = value.match(
    /^clamp\(\s*([\d.]+)%\s*,\s*calc\(\s*100%\s*\+\s*\(\s*100vw\s*-\s*([\d.]+)px\s*\)\s*\/\s*([\d.]+)\s*\)\s*,\s*([\d.]+)%\s*\)$/,
  );
  assert.ok(m, `the root curve must stay in the form this guard evaluates, found ${value}`);
  const [lo, at, slope, hi] = [m[1], m[2], m[3], m[4]].map(Number);
  const fluid = base + (width - at) / slope;
  return Math.min(Math.max(fluid, (lo / 100) * base), (hi / 100) * base);
}

/** Which rung applies at `width` for a fine or coarse pointer. Only the forms
 *  the sheet uses are understood; anything else fails loudly. */
function applies(media, width, pointer) {
  if (!media) return true;
  let ok = true;
  const range = media.match(/\(\s*([\d.]+)em\s*<=\s*width\s*<\s*([\d.]+)em\s*\)/);
  if (range) ok &&= width >= Number(range[1]) * 16 && width < Number(range[2]) * 16;
  const above = media.match(/\(\s*width\s*>\s*([\d.]+)em\s*\)/);
  if (above) ok &&= width > Number(above[1]) * 16;
  const wantPointer = media.match(/\(pointer:\s*(fine|coarse)\)/);
  if (wantPointer) ok &&= wantPointer[1] === pointer;
  assert.ok(range || above, `unrecognised root rung ${media}`);
  return ok;
}

function rootAt(css, width, pointer = "fine") {
  let px = null;
  for (const rung of rootRules(css)) {
    if (applies(rung.media, width, pointer)) px = evaluate(rung.value, width);
  }
  return px;
}

test("the chrome's root is 16px at the ribbon's budget width and follows the screen", () => {
  const css = read("style.css");
  const plain = rootRules(css).find((r) => !r.media);
  assert.ok(plain, "an unconditional `html { --chrome-root }` must exist");
  // The root is the curve TIMES the host/user multiplier, and is never a px
  // literal, so the reader's browser font setting still counts.
  assert.match(plain.body, /font-size:\s*calc\(\s*var\(--chrome-root\)\s*\*\s*var\(--chrome-scale,\s*1\)\s*\)/);
  assert.doesNotMatch(plain.body, /font-size:[^;]*\dpx/);

  for (const width of [1280, 1300, 1366, 1440]) {
    assert.equal(rootAt(css, width), 16, `the root must be exactly 16px at ${width}px`);
  }
  assert.equal(rootAt(css, 1024), 14, "a small laptop gets 14px");
  assert.equal(rootAt(css, 2560), 20, "a 1440p desktop gets 20px");
  assert.equal(rootAt(css, 3840), 20, "and it stops at 125%");
  assert.equal(rootAt(css, 390), 16, "the phone rung keeps 100%");

  // Never decreasing with width, so making a window wider never shrinks it.
  let previous = 0;
  for (let width = 1024; width <= 3000; width += 8) {
    const px = rootAt(css, width);
    assert.ok(px >= previous, `the root shrinks from ${previous} to ${px} at ${width}px`);
    previous = px;
  }
  // A finger never gets less than 100%: the 2.75rem targets stay 44px.
  for (const width of [1024, 1100, 1200, 1279]) {
    assert.equal(rootAt(css, width, "coarse"), 16, `coarse pointer at ${width}px`);
  }
});
