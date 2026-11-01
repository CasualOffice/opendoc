// The first-party add-ins' logic (`docs/183` §18, §19): LanguageTool's mapping of a
// server's matches back to paragraphs, and Readability's findings. Both run in
// a VM, as the add-ins' classic scripts run in their sandboxes.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

import { checkManifest } from "../src/plugin_manifest.mjs";
import { checkCatalogue } from "../src/plugin_catalogue.mjs";

function load(path, name) {
  const sandbox = { globalThis: {}, URLSearchParams };
  sandbox.self = sandbox.globalThis;
  vm.runInNewContext(readFileSync(new URL(path, import.meta.url), "utf8"), sandbox);
  const raw = sandbox.globalThis[name];
  const realm = (v) => (v === undefined || v === null ? v : JSON.parse(JSON.stringify(v)));
  return Object.fromEntries(Object.entries(raw).map(([k, v]) => [k, typeof v === "function" ? (...a) => realm(v(...a)) : v]));
}
const LT = load("../plugins/languagetool/languagetool_core.js", "LanguageToolCore");
const RD = load("../plugins/readability/readability_core.js", "ReadabilityCore");
const P = (id, text, extra = {}) => ({ kind: "paragraph", id, text, ...extra });

// ---- The manifests and the catalogue -------------------------------------------

test("every first-party manifest is valid, and no first-party add-in holds edit", () => {
  const catalogue = JSON.parse(readFileSync(new URL("../plugins/catalogue.json", import.meta.url), "utf8"));
  const checked = checkCatalogue(catalogue, "https://docs.example/plugins/catalogue.json", { source: "first-party" });
  assert.equal(checked.ok, true);
  for (const entry of checked.entries) {
    const path = new URL(entry.manifestUrl).pathname.replace(/^\/plugins\//, "../plugins/");
    const raw = JSON.parse(readFileSync(new URL(path, import.meta.url), "utf8"));
    const out = checkManifest(raw, { manifestUrl: entry.manifestUrl, selfOrigin: "https://docs.example" });
    assert.equal(out.ok, true, `${entry.manifestUrl}: ${out.reason}`);
    assert.equal(out.manifest.permissions.includes("edit"), false, `${out.manifest.id} must only propose`);
  }
});

test("LanguageTool is the one first-party add-in that sends text out, so it is the one that asks", () => {
  const catalogue = JSON.parse(readFileSync(new URL("../plugins/catalogue.json", import.meta.url), "utf8"));
  const lt = catalogue.plugins.find((p) => p.manifest.startsWith("languagetool/"));
  assert.equal(lt.grant, undefined, "no installation grant: turning it on shows the prompt naming the server");
  assert.equal(lt.enabled, false);
  const rd = catalogue.plugins.find((p) => p.manifest.startsWith("readability/"));
  assert.equal(rd.enabled, false, "off until a person turns it on");
});

test("an administrator may point a plugin at its own server; a person's install may not", () => {
  const host = checkCatalogue({ catalogueVersion: 1, plugins: [{ manifest: "lt/manifest.json", network: ["https://lt.corp.example"], config: { server: "https://lt.corp.example" } }] }, "https://h.example/c.json", { source: "host" });
  assert.deepEqual(host.entries[0].network, ["https://lt.corp.example"]);
  assert.deepEqual(host.entries[0].config, { server: "https://lt.corp.example" });
  const mine = checkCatalogue({ catalogueVersion: 1, plugins: [{ manifest: "lt/manifest.json", network: ["https://evil.example"] }] }, "https://h.example/c.json", { source: "installed" });
  assert.equal(mine.entries[0].network, null);
});

// ---- LanguageTool --------------------------------------------------------------

const DOC = [P("a", "Their is a problem here."), P("b", "  "), P("c", "This are fine. This are fine.")];

test("chunks join paragraphs and remember where each one starts", () => {
  const [chunk] = LT.chunks(DOC, 6000);
  assert.equal(chunk.text, "Their is a problem here.\n\nThis are fine. This are fine.");
  assert.deepEqual(chunk.map.map((m) => [m.node, m.start]), [["a", 0], ["c", 26]]);
});

test("a long document splits into chunks under the limit, never mid-paragraph", () => {
  const long = [...Array(50)].map((_, i) => P(`p${i}`, "x".repeat(200)));
  const chunks = LT.chunks(long, 1000);
  assert.ok(chunks.length > 1);
  assert.ok(chunks.every((c) => c.text.length <= 1000));
  assert.equal(chunks.reduce((n, c) => n + c.map.length, 0), 50);
});

test("matches map back to their paragraph and the exact occurrence", () => {
  const [chunk] = LT.chunks(DOC, 6000);
  const response = {
    matches: [
      { offset: 0, length: 5, message: "Did you mean “There”?", replacements: [{ value: "There" }], rule: { id: "THEIR_IS", issueType: "grammar", category: { name: "Grammar" } } },
      // The second "This are" in paragraph c (offset 26 + 15).
      { offset: 41, length: 8, message: "Agreement", replacements: [{ value: "This is" }], rule: { id: "AGR" } },
      // Spans the paragraph boundary: dropped.
      { offset: 20, length: 10, message: "x", replacements: [] },
    ],
  };
  const out = LT.issuesFrom(response, chunk);
  assert.equal(out.dropped, 1);
  assert.deepEqual(out.issues.map((i) => [i.node, i.exact, i.occurrence, i.replacements[0]]), [["a", "Their", 1, "There"], ["c", "This are", 2, "This is"]]);
});

test("each match gets the editor's category, from the documented issueType and category id", () => {
  assert.equal(LT.categoryOf("misspelling", "TYPOS"), "spelling");
  assert.equal(LT.categoryOf("", "TYPOS"), "spelling");
  assert.equal(LT.categoryOf("typographical", "PUNCTUATION"), "punctuation");
  assert.equal(LT.categoryOf("whitespace", ""), "punctuation");
  assert.equal(LT.categoryOf("style", "STYLE"), "style");
  assert.equal(LT.categoryOf("uncategorized", "REDUNDANCY"), "style");
  assert.equal(LT.categoryOf("grammar", "GRAMMAR"), "grammar");
  assert.equal(LT.categoryOf("", ""), "grammar");
});

test("the report has one result per paragraph sent, so a clean paragraph clears its marks", () => {
  const paragraphs = [{ node: "a", text: "Their is a problem here." }, { node: "c", text: "Fine." }];
  const out = LT.resultsFor(paragraphs, [
    { node: "a", offset: 0, length: 5, exact: "Their", message: "Did you mean “There”?", short: "", replacements: ["There", "They’re", "Theirs", "x"], rule: "THEIR_IS", issueType: "grammar", categoryId: "GRAMMAR" },
    { node: "zzz", offset: 0, length: 1, exact: "x", message: "", short: "", replacements: [], rule: "", issueType: "", categoryId: "" },
  ]);
  assert.deepEqual(out, [
    { node: "a", text: "Their is a problem here.", matches: [{ start: 0, end: 5, exact: "Their", category: "grammar", message: "Did you mean “There”?", replacements: ["There", "They’re", "Theirs"], rule: "THEIR_IS" }] },
    { node: "c", text: "Fine.", matches: [] },
  ]);
  assert.equal(LT.languageFor("en-US"), "en-US");
  assert.equal(LT.languageFor(""), "auto");
  assert.equal(LT.languageFor("x-weird tag"), "auto");
});

test("the request is the documented form body; the URL is the server's /v2/check", () => {
  assert.equal(LT.requestBody("a b&c", "auto"), "text=a+b%26c&language=auto");
  assert.equal(LT.checkUrl("https://lt.example/"), "https://lt.example/v2/check");
  assert.equal(LT.checkUrl(""), "https://api.languagetool.org/v2/check");
});

// ---- Readability ----------------------------------------------------------------

test("long sentences, passive voice and filler words are found and anchored", () => {
  const long = "This sentence keeps going and going with clause after clause so that nobody reading it could possibly hold all of its many parts in mind at once.";
  const out = RD.findings([P("p", `${long} The report was written by the team. It is very clear.`), P("h", long, { headingLevel: 2 })], { english: true });
  const kinds = out.map((f) => f.kind).sort();
  assert.deepEqual(kinds, ["filler", "long", "passive"]);
  const longOne = out.find((f) => f.kind === "long");
  assert.ok(longOne.words > 25);
  assert.ok(longOne.exact.startsWith("This sentence keeps"));
  assert.equal(out.find((f) => f.kind === "passive").exact, "was written");
  assert.ok(out.every((f) => f.node === "p"), "headings are not reviewed for style");
});

test("English-only heuristics stay off for other languages, and CJK text is not split into fake words", () => {
  const out = RD.findings([P("p", "Der Bericht wurde vom Team sehr schnell geschrieben."), P("j", "これはとても長い文です。".repeat(10))], { english: false });
  assert.deepEqual(out, []);
  // The switch itself: the same English text, flagged only when the reader's
  // language is English.
  const text = [P("e", "The report was written very quickly.")];
  assert.equal(RD.findings(text, { english: true }).length, 2);
  assert.deepEqual(RD.findings(text, { english: false }), []);
});

test("the reading-ease score follows the published formula", () => {
  const sc = RD.scores(["The cat sat on the mat."]);
  // 6 words, 1 sentence, 6 syllables: 206.835 - 1.015*6 - 84.6*1 = 116.145
  assert.equal(sc.ease, 116);
  assert.equal(sc.sentences, 1);
});

test("in the text: a long sentence is marked whole, passive voice and fillers where they are", () => {
  const long = "This sentence keeps going and going with clause after clause so that nobody reading it could possibly hold all of its many parts in mind at once.";
  const text = `${long} The report was written by the team. It is very clear.`;
  const out = RD.matchesIn(text, { english: true });
  const at = (kind) => out.find((m) => m.kind === kind);
  assert.deepEqual([at("long").start, at("long").end], [0, long.length]);
  assert.equal(text.slice(at("passive").start, at("passive").end), "was written");
  assert.equal(text.slice(at("filler").start, at("filler").end), "very");
  assert.deepEqual(RD.matchesIn("The report was written very quickly.", { english: false }), []);
  assert.deepEqual(RD.matchesIn("これはとても長い文です。".repeat(10), { english: false }), []);
});

test("findings become comments only", () => {
  const out = RD.planForComments([{ node: "p", kind: "filler", exact: "very", occurrence: 1 }], 3, () => "note", 50);
  assert.deepEqual(out.plan.edits, [{ op: "comment", target: { node: "p", exact: "very", occurrence: 1 }, text: "note" }]);
});
