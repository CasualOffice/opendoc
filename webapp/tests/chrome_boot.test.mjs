// The chrome decision is taken twice and must be one rule (docs/109 UX-064,
// PROPOSAL D-16).
//
// `editor.html`'s <head> decides `html[data-chrome]` before the first frame, so
// only one navigation axis ever paints; `setChromeMode` decides it again once the
// modules load. Nothing mechanical joins the inline script to `chrome_boot.mjs`,
// so this file reads the script and RUNS it against every preference and width
// the module resolves, and fails the build the moment the two disagree: a
// renamed preference key, a moved phone rung, a third mode one side knows.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { CHROME_MODE_PREF, PHONE_QUERY, resolveChrome } from "../src/chrome_boot.mjs";

const html = readFileSync(new URL("../editor.html", import.meta.url), "utf8");
const headEnd = html.indexOf("</head>");
const bodyStart = html.indexOf("<body>");

/** The inline (src-less) scripts of a region of the page, in order. */
function inlineScripts(from, to) {
  const region = html.slice(from, to);
  return [...region.matchAll(/<script>([\s\S]*?)<\/script>/g)].map((m) => m[1]);
}

const headScript = inlineScripts(0, headEnd).find((s) => s.includes("data-chrome"));
const bodyScript = inlineScripts(bodyStart, bodyStart + 4000).find((s) => s.includes("data-chrome"));

/** Runs the head and body scripts in a stub page, as the parser would. */
function boot({ pref, width, search = "", storageThrows = false }) {
  const attrs = {};
  const htmlClasses = new Set();
  const bodyClasses = new Set();
  const classList = (set) => ({ add: (...c) => c.forEach((x) => set.add(x)), contains: (c) => set.has(c) });
  const documentElement = {
    setAttribute: (k, v) => (attrs[k] = String(v)),
    getAttribute: (k) => attrs[k] ?? null,
    classList: classList(htmlClasses),
  };
  const body = { classList: classList(bodyClasses) };
  // The one media query a browser would answer: `max-width: Nem` at 16px/em.
  const matchMedia = (query) => {
    const m = /\(max-width:\s*([\d.]+)em\)/.exec(query);
    assert.ok(m, `the head script asks an unexpected media query: ${query}`);
    return { matches: width <= Number(m[1]) * 16 };
  };
  const localStorage = {
    getItem: (key) => {
      if (storageThrows) throw new Error("SecurityError");
      return key === CHROME_MODE_PREF ? pref : null;
    },
  };
  const context = { document: { documentElement, body }, location: { search }, localStorage, matchMedia };
  context.window = context;
  vm.runInNewContext(headScript, context);
  vm.runInNewContext(bodyScript, context);
  return { chrome: attrs["data-chrome"], htmlClasses, bodyClasses };
}

test("the head script exists, runs before the stylesheet, and the body script before any chrome", () => {
  assert.ok(headScript, "editor.html <head> must decide data-chrome inline");
  assert.ok(bodyScript, "the first lines of <body> must apply the decision to the body classes");
  const scriptAt = html.indexOf(headScript);
  assert.ok(scriptAt < html.indexOf('href="./src/style.css"'), "the decision must precede style.css");
  assert.ok(html.indexOf(bodyScript) < html.indexOf('<header class="bar">'), "the body classes precede the header");
  assert.ok(headScript.includes(JSON.stringify(CHROME_MODE_PREF)), `the head script must read ${CHROME_MODE_PREF}`);
  assert.ok(headScript.includes(PHONE_QUERY), `the head script must ask ${PHONE_QUERY}`);
});

test("the head script and resolveChrome agree on every preference and every width", () => {
  for (const pref of [null, "ribbon", "compact", "something-else"]) {
    for (const width of [320, 390, 619, 620, 621, 800, 1280, 1920]) {
      const phone = width <= 620;
      const expected = resolveChrome({ pref, phone });
      const { chrome, bodyClasses } = boot({ pref, width });
      assert.equal(chrome, expected, `pref=${pref} width=${width}`);
      // The body classes every style.css rule keys on, from the same answer.
      assert.equal(bodyClasses.has("ribbon-mode"), expected === "ribbon", `ribbon-mode at ${pref}/${width}`);
      assert.equal(bodyClasses.has("compact-mode"), expected !== "ribbon", `compact-mode at ${pref}/${width}`);
      assert.equal(bodyClasses.has("phone-mode"), expected === "phone", `phone-mode at ${pref}/${width}`);
    }
  }
});

test("blocked storage falls back to the ribbon instead of throwing before the first frame", () => {
  assert.equal(boot({ pref: "compact", width: 1280, storageThrows: true }).chrome, "ribbon");
  assert.equal(boot({ pref: "compact", width: 390, storageThrows: true }).chrome, "phone");
});

test("the engine is pending from the first frame, and a document is expected unless ?blank=1", () => {
  const opening = boot({ pref: null, width: 1280, search: "?fixture=rich" });
  assert.ok(opening.htmlClasses.has("engine-pending"));
  assert.ok(opening.htmlClasses.has("doc-expected"));
  assert.ok(!boot({ pref: null, width: 1280, search: "?blank=1" }).htmlClasses.has("doc-expected"));
});

test("every chrome region is inert in markup until the engine lifts it", () => {
  // `engine_boot.mjs` lifts exactly `[data-boot-inert]`; a region that lost the
  // attribute would be clickable before anything behind it can run (FILE-03).
  for (const region of [
    /<div id="documentChrome"[^>]*>/,
    /<div class="controls"[^>]*>/,
    /<div[^>]*id="compactToolbar"[^>]*>/,
    /<div class="ribbon"[^>]*>/,
    /<nav[^>]*class="rail"[^>]*>/,
    /<footer class="footer"[^>]*>/,
  ]) {
    const tag = html.match(region);
    assert.ok(tag, `editor.html has ${region}`);
    assert.match(tag[0], /\binert\b/, `${tag[0]} must be inert until the engine is ready`);
    assert.match(tag[0], /\bdata-boot-inert\b/, `${tag[0]} must carry data-boot-inert so the boot lifts it`);
  }
});
