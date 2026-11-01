// An AI edit turn that writes nothing changes nothing (`docs/183` §23.6).
//
// The owner's report: "Make more confident" on one sentence left the panel
// showing only `Deleted: <the sentence>`, then "Accepted." — the sentence was
// gone. The selection was struck when the turn OPENED and the model's answer
// never arrived (an empty answer, a timeout, a dropped connection, a guard
// violation and a Stop all ended the turn the same way). These drive the REAL
// co-editor (`ai_coeditor.mjs`) over a small fake engine that applies each
// `aiTurn*` call to a paragraph map, so the assertion is on the document text,
// byte for byte, never on a message.
import assert from "node:assert/strict";
import test from "node:test";

import { createCoEditor } from "../src/ai_coeditor.mjs";

const ORIGINAL = "Our team believes the new process might help.";

/** A DOM just big enough for the turn bar (the pointer finds no viewport). */
function fakeView() {
  const element = () => {
    const el = {
      children: [],
      dataset: {},
      style: { setProperty() {} },
      classList: { toggle() {}, add() {} },
      hidden: false,
      textContent: "",
      setAttribute() {},
      addEventListener() {},
      append(...kids) {
        el.children.push(...kids);
      },
      getBoundingClientRect: () => ({ left: 0, top: 0, width: 0, height: 0, bottom: 0 }),
    };
    return el;
  };
  const document = { getElementById: () => null, createElement: element, createElementNS: element, body: element() };
  return { document, setTimeout, clearTimeout, addEventListener() {} };
}

/** The engine's turn calls over `{node: text}`, as direct edits, with a
 *  snapshot per turn that `aiTurnRevert` restores. */
function fakeEditor(paragraphs) {
  const text = { ...paragraphs };
  const calls = [];
  const statuses = [];
  let snapshot = null;
  let anchor = null;
  const result = () => ({ free() {} });
  const doc = {
    aiTurnBegin(mode) {
      calls.push(["begin", mode]);
      snapshot = { ...text };
      return 7;
    },
    aiTurnSetAnchor(_turn, node, offset) {
      anchor = { node, offset };
      return result();
    },
    aiTurnAnchor: () => JSON.stringify(anchor),
    aiTurnReplace(_turn, node, start, end) {
      calls.push(["replace", node, start, end]);
      text[node] = text[node].slice(0, start) + text[node].slice(end);
      anchor = { node, offset: start };
      return result();
    },
    aiTurnWrite(_turn, piece) {
      calls.push(["write", piece]);
      const { node, offset } = anchor;
      text[node] = text[node].slice(0, offset) + piece + text[node].slice(offset);
      anchor = { node, offset: offset + piece.length };
      return result();
    },
    aiTurnEnd: () => "{}",
    aiTurnRevert(_turn, retire) {
      calls.push(["revert", retire]);
      // The real engine undoes only the turn's own commits; with none, nothing.
      if (calls.some(([c]) => c === "replace" || c === "write")) Object.assign(text, snapshot);
      return result();
    },
    aiTurnAccept: () => result(),
    copyText: (node, start, _node, end) => (text[node] ?? "").slice(start, end),
    caretRect: () => [],
    selectionRects: () => [],
  };
  const ed = {
    doc,
    pages: [],
    selection: null,
    composingText: false,
    applyEditResult: async () => {},
    paintOverlayLayer() {},
    setStatus: (message, kind) => statuses.push([message, kind ?? "info"]),
  };
  /** The person types `piece` at `offset` of `node`; the engine maps the AI's
   *  anchor past it when it is at or after the edit, as `aiTurnAnchor` does. */
  const personTypes = (node, offset, piece) => {
    text[node] = text[node].slice(0, offset) + piece + text[node].slice(offset);
    if (anchor && anchor.node === node && anchor.offset >= offset) anchor = { node, offset: anchor.offset + piece.length };
  };
  return { ed, text, calls, statuses, personTypes };
}

function coeditor(paragraphs, mode = "direct") {
  const fake = fakeEditor(paragraphs);
  const states = [];
  const view = fakeView();
  const co = createCoEditor(fake.ed, { t: (key) => key, effective: () => ({ mode }), view });
  /** The turn bar's mode label, as the person reads it. */
  const barMode = () => view.document.body.children[0]?.children[0]?.children[2]?.textContent ?? null;
  return { ...fake, co, states, barMode, notify: (state) => states.push(state) };
}

const replaceAll = (node, exact) => ({ kind: "replace", node, start: 0, end: exact.length, exact });

test("opening a replace turn changes nothing: the range is only marked", async () => {
  const h = coeditor({ p1: ORIGINAL });
  const opened = await h.co.begin(replaceAll("p1", ORIGINAL), { notify: h.notify });
  assert.equal(opened.ok, true);
  assert.equal(h.text.p1, ORIGINAL);
  assert.equal(h.co.current().deferred, true);
  assert.deepEqual(h.calls.filter(([c]) => c === "replace"), []);
});

test("an empty answer leaves the selection exactly as it was, and says so", async () => {
  const h = coeditor({ p1: ORIGINAL });
  const opened = await h.co.begin(replaceAll("p1", ORIGINAL), { notify: h.notify });
  const ended = await h.co.end(opened.turn);
  assert.equal(ended.ok, false);
  assert.equal(ended.code, "empty");
  assert.equal(h.text.p1, ORIGINAL);
  assert.equal(h.states.at(-1).state, "rolledBack");
  assert.equal(h.states.at(-1).code, "empty");
  assert.deepEqual(h.statuses.at(-1), ["ai.turn.empty", "error"]);
  // Nothing is left to Accept: a decision on the turn is refused.
  assert.equal((await h.co.decide(opened.turn, true)).ok, false);
  assert.equal(h.text.p1, ORIGINAL);
});

test("a blank answer (spaces and newlines only) is an empty answer", async () => {
  const h = coeditor({ p1: ORIGINAL });
  const opened = await h.co.begin(replaceAll("p1", ORIGINAL), { notify: h.notify });
  h.co.write(opened.turn, "  ");
  // Past a batch and the highlight: a blank batch must not have struck anything.
  await new Promise((resolve) => setTimeout(resolve, 700));
  assert.equal(h.text.p1, ORIGINAL);
  h.co.write(opened.turn, "\n \n");
  const ended = await h.co.end(opened.turn);
  assert.equal(ended.code, "empty");
  assert.equal(h.text.p1, ORIGINAL);
});

test("a timeout, a dropped connection or a Stop before the first word leaves the text unchanged", async () => {
  const h = coeditor({ p1: ORIGINAL });
  const opened = await h.co.begin(replaceAll("p1", ORIGINAL), { notify: h.notify });
  // The add-in's error path and the person's Stop both reach the editor as stop.
  assert.equal(h.co.stop(opened.turn, "person"), true);
  assert.equal((await h.co.decide(opened.turn, true)).ok, false);
  assert.equal(h.text.p1, ORIGINAL);
  assert.equal(h.states.at(-1).state, "rolledBack");
});

test("a guard violation before the first word leaves the text unchanged", async () => {
  const h = coeditor({ p1: ORIGINAL });
  const opened = await h.co.begin(replaceAll("p1", ORIGINAL), { notify: h.notify });
  await h.co.abort(opened.turn, "injection");
  assert.equal(h.text.p1, ORIGINAL);
});

test("the first visible text replaces the range, once, in the same step", async () => {
  const h = coeditor({ p1: ORIGINAL });
  const opened = await h.co.begin(replaceAll("p1", ORIGINAL), { notify: h.notify });
  h.co.write(opened.turn, "We are ");
  h.co.write(opened.turn, "confident.");
  const ended = await h.co.end(opened.turn);
  assert.equal(ended.ok, true);
  assert.equal(h.text.p1, "We are confident.");
  assert.equal(h.calls.filter(([c]) => c === "replace").length, 1);
});

test("the person types before the marked range while the model thinks: the range is found again", async () => {
  const h = coeditor({ p1: `Intro. ${ORIGINAL}` });
  const opened = await h.co.begin({ kind: "replace", node: "p1", start: 7, end: 7 + ORIGINAL.length, exact: ORIGINAL }, { notify: h.notify });
  h.personTypes("p1", 0, "NEW ");
  h.co.write(opened.turn, "Better.");
  await h.co.end(opened.turn);
  assert.equal(h.text.p1, "NEW Intro. Better.");
});

test("the person changed the marked text meanwhile: stale, rolled back, nothing lost", async () => {
  const h = coeditor({ p1: ORIGINAL });
  const opened = await h.co.begin(replaceAll("p1", ORIGINAL), { notify: h.notify });
  h.personTypes("p1", 4, "whole ");
  h.co.write(opened.turn, "Better.");
  await h.co.end(opened.turn);
  assert.equal(h.text.p1, `Our whole ${ORIGINAL.slice(4)}`);
  assert.equal(h.states.at(-1).code, "stale");
});

test("across paragraphs, a paragraph whose rewrite came back empty is left as it was", async () => {
  const h = coeditor({ p1: "First one.", p2: "Second one." });
  const opened = await h.co.begin({ kind: "at", at: { node: "p1", offset: 0 } }, { notify: h.notify });
  assert.equal((await h.co.replace(opened.turn, replaceAll("p1", "First one."))).ok, true);
  h.co.write(opened.turn, "Uno.");
  assert.equal((await h.co.replace(opened.turn, replaceAll("p2", "Second one."))).ok, true);
  const ended = await h.co.end(opened.turn);
  assert.equal(ended.ok, true);
  assert.deepEqual({ ...h.text }, { p1: "Uno.", p2: "Second one." });
});

test("the turn bar names the mode the engine was asked to write in, never another", async () => {
  // The owner saw "Direct edits" beside a plugin grant of `direct: false`. The
  // grant is the private preview's; the live turn follows the effective access
  // (the person's "AI edits as", capped by mode, protection and role). What the
  // bar says and what the engine does must be the same value.
  for (const mode of ["direct", "suggest"]) {
    const h = coeditor({ p1: ORIGINAL }, mode);
    await h.co.begin(replaceAll("p1", ORIGINAL), { notify: h.notify });
    assert.deepEqual(h.calls[0], ["begin", mode]);
    assert.equal(h.barMode(), `ai.turn.mode.${mode}`);
  }
});

test("a mode that admits no edit opens no turn at all", async () => {
  for (const mode of ["comment", "chat"]) {
    const h = coeditor({ p1: ORIGINAL }, mode);
    const opened = await h.co.begin(replaceAll("p1", ORIGINAL), { notify: h.notify });
    assert.equal(opened.ok, false);
    assert.equal(opened.code, "access");
    assert.deepEqual(h.calls, []);
  }
});
