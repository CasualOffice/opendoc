// Add-in proofing (`docs/183` §19): what the host believes of an add-in's
// report, the one-underline rule, the cache that brings marks back on undo,
// Next/Previous order, and that nothing an add-in can send applies a fix.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import { CATEGORIES, LIMITS, batches, checkReport, createAddinProofing, stepIssue } from "../src/addin_proofing.mjs";
import { checkManifest } from "../src/plugin_manifest.mjs";

const sent = (map) => (node) => map[node];

test("a report about text the paragraph no longer holds is dropped whole", () => {
  const out = checkReport({ results: [{ node: "a", text: "Teh cat", matches: [{ start: 0, end: 3, category: "spelling" }] }] }, sent({ a: "The cat" }));
  assert.deepEqual(out.results, []);
  assert.equal(out.dropped, 1);
});

test("a match must quote the text at its offsets, name a known category and stay inside the paragraph", () => {
  const text = "Teh cat sat.";
  const out = checkReport(
    {
      results: [{
        node: "a",
        text,
        matches: [
          { start: 0, end: 3, exact: "Teh", category: "spelling", message: "Possible typo", replacements: ["The", "Teh", "Ten"] },
          { start: 4, end: 7, exact: "dog", category: "grammar" },
          { start: 4, end: 7, category: "made-up" },
          { start: 8, end: 99, category: "grammar" },
          { start: 5, end: 5, category: "grammar" },
        ],
      }],
    },
    sent({ a: text }),
  );
  assert.equal(out.dropped, 4);
  const [mark] = out.results[0].marks;
  assert.deepEqual([mark.exact, mark.category, mark.byteStart, mark.byteEnd], ["Teh", "spelling", 0, 3]);
  assert.deepEqual(mark.replacements, ["The", "Ten"], "a replacement equal to the text is not a fix");
});

test("offsets are UTF-16 in, UTF-8 bytes out (the engine's coordinates)", () => {
  const text = "Café teh";
  const out = checkReport({ results: [{ node: "a", text, matches: [{ start: 5, end: 8, category: "spelling" }] }] }, sent({ a: text }));
  const [mark] = out.results[0].marks;
  assert.equal(mark.exact, "teh");
  assert.deepEqual([mark.byteStart, mark.byteEnd], [6, 9]);
});

test("bounds hold: matches per paragraph, message length, replacements", () => {
  const text = "x".repeat(400);
  const matches = [...Array(LIMITS.matchesPerParagraph + 7)].map((_, i) => ({ start: i, end: i + 1, category: "style", message: "m".repeat(1000), replacements: ["a", "b", "c", "d", "e", "f", "g"] }));
  const out = checkReport({ results: [{ node: "a", text, matches }] }, sent({ a: text }));
  assert.equal(out.results[0].marks.length, LIMITS.matchesPerParagraph);
  assert.equal(out.dropped, 7);
  assert.equal([...out.results[0].marks[0].message].length, LIMITS.messageChars);
  assert.equal(out.results[0].marks[0].replacements.length, LIMITS.replacements);
});

test("checks are batched within the paragraph and character bounds", () => {
  const many = [...Array(LIMITS.paragraphsPerCheck * 2 + 1)].map((_, i) => ({ node: `p${i}`, text: "ab" }));
  assert.deepEqual(batches(many).map((b) => b.length), [LIMITS.paragraphsPerCheck, LIMITS.paragraphsPerCheck, 1]);
  const big = [{ node: "a", text: "x".repeat(LIMITS.charsPerCheck - 10) }, { node: "b", text: "x".repeat(20) }];
  assert.equal(batches(big).length, 2);
});

test("Next and Previous walk the issues in document order and wrap", () => {
  const issue = (ordinal, byteStart) => ({ ordinal, mark: { byteStart } });
  const list = [issue(1, 0), issue(1, 10), issue(3, 4)];
  assert.equal(stepIssue(list, { ordinal: 1, offset: 0 }, 1), list[1]);
  assert.equal(stepIssue(list, { ordinal: 1, offset: 10 }, 1), list[2]);
  assert.equal(stepIssue(list, { ordinal: 3, offset: 4 }, 1), list[0], "wraps to the first");
  assert.equal(stepIssue(list, { ordinal: 1, offset: 0 }, -1), list[2], "wraps to the last");
  assert.equal(stepIssue(list, { ordinal: 3, offset: 4 }, -1), list[1]);
  assert.equal(stepIssue([], null, 1), null);
});

test("every category draws with a paper token; underlines reuse the built-in squiggle classes", () => {
  for (const [name, look] of Object.entries(CATEGORIES)) {
    assert.match(look.token, /^--paper-/, name);
    if (look.look === "underline") assert.match(look.className, /^(spell-error|grammar-error)\b/, name);
  }
});

// ---- The layer, with a stand-in editor ------------------------------------------

function fakeEditor() {
  const listeners = {};
  const target = { addEventListener: (type, fn) => ((listeners[type] ??= []).push(fn)) };
  const placed = [];
  const replaced = [];
  const ed = {
    spellChecker: { setExternal() {}, personalWords: () => [] },
    pagesEl: target,
    viewportEl: target,
    selection: null,
    hasRange: () => false,
    paintOverlayLayer() {},
    place: (rect, className) => {
      const el = { dataset: {}, classList: { add() {} }, className };
      placed.push(el);
      return el;
    },
    replaceMisspelling: (span, text) => replaced.push([span, text]),
    doc: { copyText: () => "Teh" },
  };
  const view = { document: { addEventListener: () => {}, body: { append() {} } }, innerWidth: 1280, innerHeight: 800 };
  return { ed, view, placed, replaced };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

test("an add-in underline replaces the built-in squiggle on the same span, and only there", async () => {
  const { ed, view } = fakeEditor();
  const proofing = createAddinProofing(ed, { view, announce: () => {} });
  const asked = [];
  proofing.register({ id: "lt", name: "LanguageTool", canReplace: true, send: (paragraphs) => asked.push(...paragraphs) });
  proofing.layer.window([{ node: "a", text: "Teh cat sat", language: "en" }], [{ pageNumber: 1 }], null);
  assert.deepEqual(asked.map((p) => p.node), ["a"]);
  proofing.report("lt", { results: [{ node: "a", text: "Teh cat sat", matches: [{ start: 0, end: 3, category: "spelling", replacements: ["The"] }] }] });
  assert.equal(proofing.layer.covers("a", 0, 3), true);
  assert.equal(proofing.layer.covers("a", 4, 7), false);
  assert.equal(proofing.count(), 1);
  await tick();
});

test("only a changed paragraph is sent again, and an undo back to checked text needs no new check", () => {
  const { ed, view } = fakeEditor();
  const proofing = createAddinProofing(ed, { view, announce: () => {} });
  const asked = [];
  proofing.register({ id: "lt", name: "LanguageTool", canReplace: true, send: (paragraphs) => asked.push(...paragraphs.map((p) => p.text)) });
  const page = [{ pageNumber: 1 }];
  proofing.layer.window([{ node: "a", text: "Teh cat" }, { node: "b", text: "Fine." }], page, null);
  proofing.report("lt", { results: [{ node: "a", text: "Teh cat", matches: [{ start: 0, end: 3, category: "spelling", replacements: ["The"] }] }, { node: "b", text: "Fine.", matches: [] }] });
  asked.length = 0;
  // The fix lands: only paragraph a changed.
  proofing.layer.window([{ node: "a", text: "The cat" }, { node: "b", text: "Fine." }], page, null);
  assert.deepEqual(asked, ["The cat"]);
  assert.equal(proofing.count(), 0, "the mark went with the text it was about");
  // Undo: the old text is answered from the cache, at once, with no request.
  asked.length = 0;
  proofing.layer.window([{ node: "a", text: "Teh cat" }, { node: "b", text: "Fine." }], page, null);
  assert.deepEqual(asked, []);
  assert.equal(proofing.count(), 1, "undo brings the mark back");
});

test("nothing an add-in sends applies a fix: the plugin host has no such method, and the card needs a trusted event", () => {
  const host = readFileSync(new URL("../src/plugin_host.mjs", import.meta.url), "utf8");
  const methods = [...host.matchAll(/^ {4}(?:async )?"?([\w/]+)"?\(/gm)].map((m) => m[1]);
  assert.ok(methods.includes("proofing/report"));
  // The live co-editor's `edit/*` methods (`docs/183` §23) DO change the
  // document, and are the product's own assistant's only: each one goes through
  // `coedit()`, which refuses any add-in the platform did not mark first-party
  // and AI-gated. Every other method still applies nothing.
  const liveOnly = methods.filter((m) => m.startsWith("edit/"));
  for (const m of liveOnly) {
    const body = host.slice(host.indexOf(`"${m}"(`), host.indexOf("\n    },", host.indexOf(`"${m}"(`)));
    assert.match(body, /coedit\(\)/, `${m} is not gated to the first-party assistant`);
  }
  assert.match(host, /if \(!env\.coedit\) throw new RpcRefusal\("permission-denied"/, "coedit() refuses an add-in without the co-editor");
  assert.equal(methods.filter((m) => !m.startsWith("edit/")).some((m) => /apply|replace|accept/i.test(m)), false, `a plugin method could apply a fix: ${methods}`);
  const source = readFileSync(new URL("../src/addin_proofing.mjs", import.meta.url), "utf8");
  assert.match(source, /if \(!event\?\.isTrusted\) return;/, "apply() no longer requires the person's own click");
});

test("a proofing contribution needs read", () => {
  const base = { manifestVersion: 1, id: "org.example.lint", name: "Lint", version: "1.0.0", entry: "index.html", contributes: { proofing: {} } };
  const opts = { manifestUrl: "https://p.example/lint/manifest.json", selfOrigin: "https://docs.example" };
  assert.equal(checkManifest({ ...base, permissions: [] }, opts).ok, false);
  assert.equal(checkManifest({ ...base, permissions: ["read"] }, opts).ok, true);
  assert.equal(checkManifest({ ...base, permissions: ["read"], contributes: { proofing: { anything: 1 } } }, opts).ok, false);
});
