import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

const WEBAPP = new URL("../", import.meta.url);
const TAG = /<script src="\.{1,2}\/src\/analytics\.js"><\/script>/;

// The pages a visitor lands on: the editor, every generated site page, and
// every reference page. `embed.html` is deliberately absent — it is the page a
// host frames, and the host owns telemetry (docs/00).
async function servedPages() {
  const root = (await readdir(WEBAPP)).filter(
    (name) => name.endsWith(".html") && name !== "embed.html",
  );
  const reference = (await readdir(new URL("reference/", WEBAPP)))
    .filter((name) => name.endsWith(".html"))
    .map((name) => `reference/${name}`);
  return [...root, ...reference];
}

test("every served page loads the analytics tag in its head", async () => {
  const missing = [];
  for (const page of await servedPages()) {
    const html = await readFile(new URL(page, WEBAPP), "utf8");
    const head = html.slice(0, html.indexOf("</head>"));
    if (!TAG.test(head)) missing.push(page);
  }
  assert.deepEqual(missing, [], "these pages do not load src/analytics.js in <head>");
});

// Runs the real script against a stub document on a given host and returns the
// scripts it injected and what it queued for gtag.
async function runOn(hostname) {
  const source = await readFile(new URL("src/analytics.js", WEBAPP), "utf8");
  const injected = [];
  const window = { location: { hostname } };
  const document = {
    createElement: () => ({}),
    head: { appendChild: (node) => injected.push(node) },
  };
  vm.runInNewContext(source, { window, document, Date });
  return { injected, dataLayer: window.dataLayer };
}

test("the tag reports from the canonical host", async () => {
  const { injected, dataLayer } = await runOn("opendoc.casualoffice.org");
  assert.equal(injected.length, 1);
  assert.equal(injected[0].src, "https://www.googletagmanager.com/gtag/js?id=G-4DEDXRTCF4");
  assert.equal(injected[0].async, true);
  assert.deepEqual(Array.from(dataLayer.at(-1)), ["config", "G-4DEDXRTCF4"]);
});

// The self-host image (`Dockerfile.editor`) ships these same pages, and the
// test suites serve them from localhost: neither may report to this property.
for (const hostname of ["localhost", "127.0.0.1", "docs.example.com", "casualoffice.org"]) {
  test(`the tag stays silent on ${hostname}`, async () => {
    const { injected, dataLayer } = await runOn(hostname);
    assert.equal(injected.length, 0);
    assert.equal(dataLayer, undefined);
  });
}
