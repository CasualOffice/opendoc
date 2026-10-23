// The editor page's own source, wherever it now lives (docs/177).
//
// `main.js` is being split into `editor_*.mjs` domain modules, each a
// `create*(ed)` factory over one editor context. A check that reads the editor
// page's code to ask what it DOES ("which capabilities does the chrome
// consult", "which strings does it show") must read all of it, or a domain
// moving out of `main.js` would silently turn the check into a statement about
// less code. This is that one reader, so no check keeps its own list.
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

/** The editor's domain modules, in a stable order. */
export function editorModuleFiles(srcDir) {
  return readdirSync(srcDir)
    .filter((name) => /^editor_[a-z_]+\.mjs$/.test(name))
    .sort();
}

/** `main.js` followed by every domain module, as one text. */
export function editorSource(srcDir) {
  return [readFileSync(join(srcDir, "main.js"), "utf8")]
    .concat(editorModuleFiles(srcDir).map((name) => readFileSync(join(srcDir, name), "utf8")))
    .join("\n");
}
