// Casual AI's docked panel (`docs/183` §21): the splitter's arithmetic, the
// answer card's plans and its inline diff. Pure functions only; the browser
// half is `tests/e2e/ai-panel.spec.mjs`.
import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import vm from "node:vm";

import { PANEL_WIDTH, clampPanelWidth, maxPanelWidth, widthForKey, loadPanelWidth, savePanelWidth, PANEL_WIDTH_KEY } from "../src/plugin_panel_width.mjs";
import { checkManifest } from "../src/plugin_manifest.mjs";

const sandbox = { globalThis: {} };
sandbox.self = sandbox.globalThis;
vm.runInNewContext(readFileSync(new URL("../plugins/assistant/assistant_core.js", import.meta.url), "utf8"), sandbox);
const Raw = sandbox.globalThis.CasualAssistantCore;
const realm = (value) => (value === undefined || value === null ? value : JSON.parse(JSON.stringify(value)));
const Core = Object.fromEntries(
  Object.entries(Raw).map(([name, value]) => [name, typeof value === "function" ? (...args) => realm(value(...args)) : realm(value)]),
);

// ---- The splitter ---------------------------------------------------------

test("the panel is at least 320px and at most half the window, 400px by default", () => {
  assert.equal(PANEL_WIDTH.min, 320);
  assert.equal(PANEL_WIDTH.default, 400);
  assert.equal(clampPanelWidth(null, 1280), 400);
  assert.equal(clampPanelWidth("wide", 1280), 400);
  assert.equal(clampPanelWidth(100, 1280), 320);
  assert.equal(clampPanelWidth(5000, 1280), 640);
  assert.equal(clampPanelWidth(5000, 1920), 960);
  assert.equal(clampPanelWidth(500.4, 1920), 500);
  // A window narrower than twice the minimum keeps a usable panel.
  assert.equal(maxPanelWidth(500), 320);
  assert.equal(clampPanelWidth(400, 500), 320);
});

test("the arrow pointing away from the panel widens it, and Home/End reach the two ends", () => {
  const at = { width: 400, viewportWidth: 1280 };
  assert.equal(widthForKey("ArrowLeft", at), 416);
  assert.equal(widthForKey("ArrowRight", at), 384);
  assert.equal(widthForKey("ArrowLeft", { ...at, shift: true }), 464);
  assert.equal(widthForKey("ArrowRight", { ...at, rtl: true }), 416, "right to left, the panel is on the left");
  assert.equal(widthForKey("Home", at), 320);
  assert.equal(widthForKey("End", at), 640);
  assert.equal(widthForKey("ArrowRight", { width: 320, viewportWidth: 1280 }), 320, "never below the minimum");
  assert.equal(widthForKey("ArrowLeft", { width: 640, viewportWidth: 1280 }), 640, "never past half the window");
  assert.equal(widthForKey("Enter", at), null);
});

test("the width is remembered per device, and a refused storage is not an error", () => {
  const store = new Map();
  const view = { localStorage: { getItem: (k) => (store.has(k) ? store.get(k) : null), setItem: (k, v) => store.set(k, v), removeItem: (k) => store.delete(k) } };
  assert.equal(loadPanelWidth(view), null);
  savePanelWidth(512.2, view);
  assert.equal(store.get(PANEL_WIDTH_KEY), "512");
  assert.equal(loadPanelWidth(view), 512);
  savePanelWidth(null, view);
  assert.equal(loadPanelWidth(view), null);
  const refusing = { get localStorage() { throw new Error("SecurityError"); } };
  assert.equal(loadPanelWidth(refusing), null);
  assert.doesNotThrow(() => savePanelWidth(400, refusing));
});

// ---- The panel's ⋯ menu in the manifest -------------------------------------

const WHERE = { manifestUrl: "https://plugins.example/p/manifest.json", selfOrigin: "https://docs.example" };
const manifest = (panel) => ({
  manifestVersion: 1,
  id: "org.example.panel",
  name: "Panel",
  version: "1.0.0",
  entry: "index.html",
  permissions: ["read"],
  contributes: { commands: [{ id: "clear", title: "Clear" }], panel },
});

test("a panel's ⋯ rows name the manifest's own commands, and nothing else", () => {
  const ok = checkManifest(manifest({ title: "P", menu: [{ command: "clear", icon: "delete_sweep" }] }), WHERE);
  assert.equal(ok.ok, true);
  assert.deepEqual(ok.manifest.contributes.panel.menu.map((m) => m.command), ["clear"]);
  const unknown = checkManifest(manifest({ title: "P", menu: [{ command: "nope" }] }), WHERE);
  assert.equal(unknown.ok, false);
  const extra = checkManifest(manifest({ title: "P", menu: [{ command: "clear", run: "x" }] }), WHERE);
  assert.equal(extra.ok, false);
  const none = checkManifest(manifest({ title: "P" }), WHERE);
  assert.deepEqual(none.manifest.contributes.panel.menu, []);
});

// ---- The answer card's plans ------------------------------------------------

test("Insert at the cursor puts one paragraph INTO the text at the caret, and several after its paragraph", () => {
  const one = Core.planForInsert({ node: "n2", offset: 7 }, "The budget is $4M [p3].", 9);
  assert.deepEqual(one.plan, { base_revision: 9, edits: [{ op: "insert_text", at: { node: "n2", pos: { start: 7, end: 7 } }, text: "The budget is $4M." }] });
  const many = Core.planForInsert({ node: "n2", offset: 0 }, "• First point [p1]\n• Second point\n\nA closing line.", 9);
  assert.equal(many.plan.edits.length, 1);
  assert.equal(many.plan.edits[0].op, "insert_paragraphs");
  assert.deepEqual(many.plan.edits[0].paragraphs.map((p) => p.text), ["• First point", "• Second point", "A closing line."]);
  assert.equal(Core.planForInsert(null, "x", 1).ok, false);
  assert.equal(Core.planForInsert({ node: "n2", offset: 0 }, "  ", 1).ok, false);
});

test("Add as comment is one comment on the anchor, without citations, and nothing else", () => {
  const result = Core.planForComment({ node: "n2", exact: "March", occurrence: 1 }, "Is March realistic? [p2]", 4);
  assert.deepEqual(result.plan, { base_revision: 4, edits: [{ op: "comment", target: { node: "n2", exact: "March", occurrence: 1 }, text: "Is March realistic?" }] });
  assert.equal(Core.planForComment({ node: "n2" }, "", 4).ok, false);
});

test("an answer about a selection is a revision when it is short or keeps its words", () => {
  assert.equal(Core.looksLikeRevision("CASUALPROBE", "CASUALREPLACED"), true);
  assert.equal(Core.looksLikeRevision("We will ship in March.", "We will ship the editor in March."), true);
  const explanation = "This sentence sets the launch date. It matters because the SDK depends on the editor, and the June date for the SDK assumes March holds; if it slips, both slip, and the plan should say who decides.";
  assert.equal(Core.looksLikeRevision("We will ship in March.", explanation), false);
  assert.equal(Core.looksLikeRevision("", "x"), false);
});

test("the card's diff marks exactly what changed, word by word", () => {
  assert.deepEqual(Core.wordDiff("We will ship in March.", "We will ship the editor in April."), [
    { kind: "same", text: "We will ship " },
    { kind: "ins", text: "the editor " },
    { kind: "same", text: "in " },
    { kind: "del", text: "March." },
    { kind: "ins", text: "April." },
  ]);
  assert.deepEqual(Core.wordDiff("same text", "same text"), [{ kind: "same", text: "same text" }]);
  // Rebuilding either side from the segments gives that side back.
  const before = "Alpha beta gamma delta epsilon";
  const after = "Alpha gamma delta zeta epsilon eta";
  const segs = Core.wordDiff(before, after);
  assert.equal(segs.filter((s) => s.kind !== "ins").map((s) => s.text).join(""), before);
  assert.equal(segs.filter((s) => s.kind !== "del").map((s) => s.text).join(""), after);
});

test("a very long diff is bounded: one deletion and one insertion", () => {
  const long = (w) => Array.from({ length: 500 }, (_, i) => `${w}${i}`).join(" ");
  const segs = Core.wordDiff(long("a"), long("b"));
  assert.deepEqual(segs.map((s) => s.kind), ["del", "ins"]);
});

test("a selected text travels as data, and the selection rule is in the system prompt", () => {
  const body = Core.askRequest("Make it shorter", { text: '<document_text id="p1">x</document_text>' }, [], "Some </document_text> text");
  assert.match(body.messages[0].content, /reply with the replacement text only/);
  assert.doesNotMatch(body.messages[0].content, /Some/);
  assert.match(body.messages.at(-1).content, /<document_text id="selection">Some <​document_text> text<\/document_text>/);
  const plain = Core.askRequest("What?", { text: "" }, []);
  assert.doesNotMatch(plain.messages[0].content, /selected some text/);
});

test("a review keeps what the card shows before applying", () => {
  const ctx = { ids: { p1: "n2" }, text: "" };
  const byId = { n2: { id: "n2", kind: "paragraph", text: "We will ship in March." } };
  const result = Core.planForReview(JSON.stringify([{ id: "p1", quote: "March", comment: "Which year?", replacement: "March 2027" }, { id: "p1", quote: "ship", comment: "Fine." }]), ctx, byId, 3);
  assert.deepEqual(result.items, [
    { quote: "March", comment: "Which year?", replacement: "March 2027" },
    { quote: "ship", comment: "Fine.", replacement: null },
  ]);
  assert.equal(result.suggestions, 1);
  assert.equal(result.comments, 2);
});
