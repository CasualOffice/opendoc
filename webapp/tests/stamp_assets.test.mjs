// The deploy must never pair a new page with an old script.
//
// GitHub Pages caches .html for 10 minutes and .js/.css/.wasm for 4 hours. With
// fixed asset URLs, a browser ran the new `editor.html` against the old
// `main.js` for hours after every deploy — which is how the Table menu from #546
// opened EMPTY in production while every test passed. `stamp-assets.py`
// versions every asset; this proves it covers all three ways the site fetches
// code, and that the Pages workflow actually runs it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, readdirSync, writeFileSync, mkdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const webapp = fileURLToPath(new URL("..", import.meta.url));
const BUILD = "0123abcd4567";

/** A throwaway copy of the deployable pages and modules, stamped. */
function stampedCopy() {
  const dir = mkdtempSync(join(tmpdir(), "stamp-"));
  for (const name of readdirSync(webapp)) {
    if (name.endsWith(".html")) cpSync(join(webapp, name), join(dir, name));
  }
  cpSync(join(webapp, "src"), join(dir, "src"), { recursive: true });
  // `pkg/` is a build output and may be absent in a unit-test checkout, so the
  // glue is a stand-in carrying the exact line wasm-bindgen generates.
  mkdirSync(join(dir, "pkg"));
  writeFileSync(
    join(dir, "pkg", "casual_doc_wasm.js"),
    "module_or_path = new URL('casual_doc_wasm_bg.wasm', import.meta.url);\n",
  );
  const run = () => execFileSync("python3", [join(webapp, "stamp-assets.py"), dir, BUILD]);
  run();
  return { dir, run };
}

test("every local script, stylesheet, module preload and wasm preload is versioned", () => {
  const { dir } = stampedCopy();
  for (const page of readdirSync(dir).filter((n) => n.endsWith(".html"))) {
    const html = readFileSync(join(dir, page), "utf8");
    const refs = [...html.matchAll(/(?:src|href)="(\.\/(?:src|pkg)\/[^"]+\.(?:m?js|css|wasm)[^"]*)"/g)];
    const stale = refs.map((m) => m[1]).filter((url) => !url.endsWith(`?v=${BUILD}`));
    assert.deepEqual(stale, [], `${page} still loads unversioned assets`);
    const scripted = [...html.matchAll(/["']\.\/pkg\/casual_doc_wasm_bg\.wasm[^"']*["']/g)].map((m) => m[0]);
    for (const literal of scripted) {
      assert.ok(literal.includes(`?v=${BUILD}`), `${page}: wasm preload ${literal} is unversioned`);
    }
  }
});

test("the import map covers every module one module imports from another", () => {
  // Relative imports resolve against the importing module and DROP its query
  // string, so a versioned main.js still fetched stale children until mapped.
  const { dir } = stampedCopy();
  const html = readFileSync(join(dir, "editor.html"), "utf8");
  const map = html.match(/<script type="importmap" data-stamp>(.*?)<\/script>/s);
  assert.ok(map, "editor.html must carry the stamped import map");
  const imports = JSON.parse(map[1]).imports;

  const sources = readdirSync(join(webapp, "src")).filter((n) => /\.m?js$/.test(n));
  for (const file of sources) {
    const code = readFileSync(join(webapp, "src", file), "utf8");
    for (const [, spec] of code.matchAll(/(?:import|export)\s[^"'`]*?from\s+["'](\.{1,2}\/[^"']+)["']/g)) {
      const key = spec.startsWith("../") ? `./${spec.slice(3)}` : `./src/${spec.slice(2)}`;
      assert.equal(imports[key], `${key}?v=${BUILD}`, `${file} imports ${spec}, which the map does not version`);
    }
  }
});

test("the wasm binary the glue fetches is versioned, and re-stamping is idempotent", () => {
  const { dir, run } = stampedCopy();
  run(); // a second pass must not produce ?v=x?v=x or a second import map
  const glue = readFileSync(join(dir, "pkg", "casual_doc_wasm.js"), "utf8");
  assert.match(glue, new RegExp(`casual_doc_wasm_bg\\.wasm\\?v=${BUILD}'`));
  assert.ok(!glue.includes("?v=" + BUILD + "?v="), "the glue was stamped twice");
  const html = readFileSync(join(dir, "editor.html"), "utf8");
  assert.equal((html.match(/type="importmap"/g) ?? []).length, 1, "one import map, not one per run");
  assert.ok(!html.includes(`?v=${BUILD}?v=`), "a page asset was stamped twice");
});

test("the Pages workflow stamps the artifact after building and before uploading it", () => {
  // A stamper nothing runs is the "CI-enforced gate that never executed" defect.
  const workflow = readFileSync(join(webapp, "..", ".github", "workflows", "pages.yml"), "utf8");
  const build = workflow.indexOf("./webapp/build.sh");
  const stamp = workflow.indexOf("./webapp/stamp-assets.py webapp");
  const upload = workflow.indexOf("upload-pages-artifact");
  assert.ok(build > 0 && stamp > 0 && upload > 0, "pages.yml must build, stamp and upload");
  assert.ok(build < stamp && stamp < upload, "the stamp must run after the build and before the upload");
});
