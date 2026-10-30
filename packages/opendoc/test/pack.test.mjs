// The package is packable and ships what it needs: the engine, its glue, the
// types, the embedded fonts' licences, the examples. `npm pack --dry-run` is
// what `npm publish` would upload; nothing is published here.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import test from "node:test";
import { fileURLToPath } from "node:url";

const here = fileURLToPath(new URL("..", import.meta.url));
const manifest = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));

test("npm pack includes the engine, the types, the font licences and no stray build output", () => {
  const [result] = JSON.parse(execFileSync("npm", ["pack", "--dry-run", "--json", "--ignore-scripts"], { cwd: here, encoding: "utf8" }));
  const files = new Set(result.files.map((f) => f.path));
  for (const wanted of [
    "pkg/opendoc_bg.wasm",
    "pkg/opendoc.js",
    "src/node.js",
    "src/docx.js",
    "types/index.d.ts",
    "types/docx.d.ts",
    "pkg/LICENSES/OFL-1.1-Liberation.txt",
    "README.md",
    "LICENSE",
    "NOTICE",
  ]) {
    assert.ok(files.has(wanted), `the tarball lacks ${wanted}`);
  }
  assert.ok(![...files].some((f) => f.endsWith(".tgz") || f.startsWith("test/")), "tests or tarballs would ship");
});

test("every export path resolves to a file the tarball ships", () => {
  const [result] = JSON.parse(execFileSync("npm", ["pack", "--dry-run", "--json", "--ignore-scripts"], { cwd: here, encoding: "utf8" }));
  const files = new Set(result.files.map((f) => f.path));
  for (const [path, target] of Object.entries(manifest.exports)) {
    const targets = typeof target === "string" ? [target] : Object.values(target);
    for (const t of targets) assert.ok(files.has(t.replace(/^\.\//, "")), `export ${path} -> ${t} is not packed`);
  }
});
