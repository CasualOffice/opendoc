import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const WEBAPP = new URL("../", import.meta.url);

async function read(relativePath) {
  return readFile(new URL(relativePath, WEBAPP));
}

test("editor chrome fonts are self-hosted and linked by both routes", async () => {
  const [fontsCss, editorHtml, siteHtml] = await Promise.all([
    read("src/fonts.css").then(String),
    read("editor.html").then(String),
    read("index.html").then(String),
  ]);

  assert.match(fontsCss, /font-family: "Inter"/);
  assert.match(fontsCss, /font-family: "Material Symbols Outlined"/);
  assert.match(fontsCss, /\.\.\/assets\/fonts\/inter-latin-400-700\.woff2/);
  assert.match(fontsCss, /\.\.\/assets\/fonts\/material-symbols-outlined\.woff2/);
  assert.doesNotMatch(fontsCss, /fonts\.(?:googleapis|gstatic)\.com/);
  assert.match(editorHtml, /href="\.\/src\/fonts\.css"/);
  assert.match(siteHtml, /href="\.\/src\/fonts\.css"/);
});

test("chrome font binaries and their licenses are checked in", async () => {
  const [inter, symbols, interLicense, symbolsLicense] = await Promise.all([
    read("assets/fonts/inter-latin-400-700.woff2"),
    read("assets/fonts/material-symbols-outlined.woff2"),
    read("assets/fonts/LICENSE-Inter.txt").then(String),
    read("assets/fonts/LICENSE-Material-Symbols.txt").then(String),
  ]);

  assert.equal(inter.subarray(0, 4).toString("ascii"), "wOF2");
  assert.equal(symbols.subarray(0, 4).toString("ascii"), "wOF2");
  assert.match(interLicense, /SIL OPEN FONT LICENSE Version 1\.1/);
  assert.match(symbolsLicense, /Apache License\s+Version 2\.0/);
});
