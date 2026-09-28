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
// THE PARSING ITSELF MOVED to `src/palette_parse.mjs`, and only the parsing: this
// file is still where "which stylesheet, and read it from disk" lives, and every
// name it used to export it still exports. The reason for the split is that the
// SDK configuration playground audits a host's palette in the BROWSER, live, and
// must reach the same answer this generator reaches — which it can only do by
// importing the same code, and it cannot import a module that opens with
// `node:fs`.
//
// No DOM, no network. Plain file reading and string work.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { parsePalettes, parseThemeBlocks } from "../src/palette_parse.mjs";

export {
  THEME_BLOCKS,
  declaredTokens,
  ruleBody,
  stripComments,
} from "../src/palette_parse.mjs";

const WEBAPP = join(dirname(fileURLToPath(import.meta.url)), "..");

/** The stylesheet the editor's chrome is painted from. */
export const STYLE_PATH = join(WEBAPP, "src", "style.css");

/** The editor's palette, as the three themes a host's overrides land on.
 *  Defaults to the committed stylesheet; see `parsePalettes` for the shape. */
export function readPalettes(css = readFileSync(STYLE_PATH, "utf8")) {
  return parsePalettes(css);
}

/** Just the per-theme blocks, unmerged — for the guard that the three declare
 *  exactly the same names and that the two dark ones agree value for value. */
export function readThemeBlocks(css = readFileSync(STYLE_PATH, "utf8")) {
  return parseThemeBlocks(css);
}
