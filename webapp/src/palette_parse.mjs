// The CSS half of `tools/palette_source.mjs`: how a palette is read out of
// stylesheet TEXT, with nothing that knows where the text came from.
//
// It was one module, and it had to split for one reason: the SDK configuration
// playground (`docs/126`) audits a host's palette **in the browser**, live, while
// somebody is choosing a colour — and it must reach the same answer the
// white-label generator reaches at build time, letter for letter, or the page is
// teaching a message the CLI does not emit. `tools/palette_source.mjs` imports
// `node:fs` at module scope, so a browser cannot import it at all; the parsing it
// does is pure string work a browser runs perfectly well.
//
// So the split is along the only line that matters: this file parses text, the
// tool reads files. Nothing is duplicated — `palette_source.mjs` re-exports every
// name below, so its existing callers did not move — and the alternative (a
// committed JSON copy of the palette for the browser to read) is exactly the
// second source of truth that module's own header argues against.
//
// No DOM, no network, no file system. `module_seams.test.mjs` holds that.

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

/** Drops CSS comments, so prose about colours is not read as colour. */
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
 * Complexity: O(stylesheet), once per call.
 *
 * @param {string} css the stylesheet's text.
 * @returns {{independent: Map<string,string>, themes: Map<string, Record<string,string>>}}
 */
export function parsePalettes(css) {
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
export function parseThemeBlocks(css) {
  return new Map(
    THEME_BLOCKS.map((block) => [block.name, declaredTokens(ruleBody(css, block.selector))]),
  );
}
