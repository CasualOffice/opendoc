// Reads the editor's palette out of `src/style.css`, so everything that needs to
// know what a token is worth asks the stylesheet rather than carrying a copy.
//
// `docs/126` phase 3 lets a host override tokens, which means two new things need
// the palette: the white-label generator (to validate the host's palette AS IT
// WILL RENDER, not in isolation) and the guard over the generated stylesheet. The
// existing `tests/style_tokens.test.mjs` already parsed it with four local
// helpers. One parser, three callers — a second one would be the two-tables
// defect (`109` UX-006) applied to colour.
//
// WHY PARSE CSS INSTEAD OF DECLARING THE PALETTE IN JS. Because `style.css` is
// the source of truth for token VALUES and `docs/63` says so in as many words
// ("`style.css` is the source of truth for token values; this document owns their
// roles"). A palette declared in a module beside it would be a second source that
// drifts silently, and the drift would be invisible: the editor would render from
// the CSS while every check passed against the module.
//
// No DOM, no network. Plain file reading and string work.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

/** The stylesheet the editor's chrome is painted from. */
export const STYLE_PATH = join(WEBAPP, "src", "style.css");

/** The three theme entry points a token must be declared in, by the name this
 *  module reports them under, with the selector that opens each block.
 *
 *  Three rather than two because plain CSS cannot express one palette for both
 *  dark entry points: a reader who picked Dark on a light OS gets only
 *  `:root[data-theme="dark"]`, and a reader on a dark OS who picked nothing gets
 *  only the media-query block. Three patches shipped in one of them and not the
 *  other (`docs/104` HF-092), which is why both are named and compared. */
export const THEME_BLOCKS = Object.freeze([
  Object.freeze({ name: "light", selector: /:root,\s*\n:root\[data-theme="light"\]/ }),
  Object.freeze({ name: "system dark", selector: /:root:not\(\[data-theme\]\)/ }),
  Object.freeze({ name: "explicit dark", selector: /:root\[data-theme="dark"\]/ }),
]);

/** The theme-INDEPENDENT block: geometry, z-order, the accent fill, and the
 *  paper layer. Matched by the bare `:root {` that opens the file, which is the
 *  only `:root` with no further selector. */
const INDEPENDENT = /:root\s*\{/;

/** Drops `/* … *\/` comments so prose about colours is not read as colour. */
export function stripComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** The declaration body of the first rule whose selector matches. */
export function ruleBody(css, selectorPattern) {
  const source = stripComments(css);
  const match = source.match(selectorPattern);
  if (!match) throw new Error(`no rule matched ${selectorPattern}`);
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

/** Custom-property declarations in a rule body, as `name → value`. */
export function declaredTokens(body) {
  const tokens = new Map();
  for (const [, name, value] of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) {
    tokens.set(name, value.trim().replace(/\s+/g, " "));
  }
  return tokens;
}

/**
 * The editor's palette, as the three themes a host's overrides land on.
 *
 * Each theme is the theme-INDEPENDENT block merged with that theme's block,
 * because that is what the cascade produces and therefore what a contrast check
 * has to measure. Checking a theme block alone would miss every pair whose two
 * halves live in different blocks — which is all of the accent pairs, since
 * `--accent` is theme-independent and `--accent-text` is not.
 *
 * Complexity: O(stylesheet), once per process.
 *
 * @param {string} [css]
 * @returns {{independent: Map<string,string>, themes: Map<string, Record<string,string>>}}
 */
export function readPalettes(css = readFileSync(STYLE_PATH, "utf8")) {
  const independent = declaredTokens(ruleBody(css, INDEPENDENT));
  const themes = new Map();
  for (const block of THEME_BLOCKS) {
    const own = declaredTokens(ruleBody(css, block.selector));
    themes.set(block.name, { ...Object.fromEntries(independent), ...Object.fromEntries(own) });
  }
  return { independent, themes };
}

/** Just the per-theme blocks, unmerged — for the guard that the three declare
 *  exactly the same names and that the two dark ones agree value for value. */
export function readThemeBlocks(css = readFileSync(STYLE_PATH, "utf8")) {
  return new Map(THEME_BLOCKS.map((block) => [block.name, declaredTokens(ruleBody(css, block.selector))]));
}
