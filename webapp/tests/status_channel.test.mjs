// A background task's progress is not the reader's feedback.
//
// `status_channel.mjs` is the one place that decides how a status message
// reaches a person, and until now it offered exactly two ways in: `publish`,
// which paints the footer line, writes a live region a screen reader speaks, and
// raises a toast when the line is hidden or the message is a refusal; and
// `announce`, which only speaks. A background task had neither, so it used
// `publish`, and on the default editor page the font-provisioning pass therefore
// announced "Fetching fonts for sample.docx…" to a screen reader and owned the
// status line for as long as the download took — which is 31.40 MB of
// coverage-driven CJK and colour-emoji faces, measured 2026-10-06. Whatever the
// reader had just been told was gone.
//
// This file drives the third surface. The DOM it needs is three fields of one
// element, so a stand-in is enough and no browser is involved — which is the
// point: "does progress take the line off the reader" is a question about a
// rule, and a rule that only Playwright can ask is a rule nothing asks.
import assert from "node:assert/strict";
import test from "node:test";

import { createStatusChannel } from "../src/status_channel.mjs";

// `publish` reaches two browser globals of its own: `requestAnimationFrame`, to
// write a live region on the frame AFTER clearing it (so a repeated identical
// refusal still counts as a change worth announcing), and `window.setTimeout`
// for the toast's dwell. They are stubbed rather than avoided — a test that
// routed around `publish` could not check that progress and the reader's
// channel interact correctly, which is the whole subject.
//
// `requestAnimationFrame` runs its callback immediately here: the frame is not
// what is under test, and deferring it would make every assertion about an
// announcement a wait.
globalThis.requestAnimationFrame ??= (fn) => {
  fn();
  return 0;
};
globalThis.window ??= { setTimeout: () => 0, clearTimeout: () => {} };
globalThis.clearTimeout ??= () => {};

/** The status line, as much of it as the channel touches. `getClientRects`
 *  answers "not painted", which is the honest state for a node with no layout —
 *  and it means `publish` takes the toast path, so the toast rules are exercised
 *  here rather than assumed away. */
function fakeStatusLine() {
  return {
    textContent: "",
    className: "status",
    dataset: {},
    getClientRects: () => [],
  };
}

function fakeRegion() {
  return { textContent: "" };
}

function fakeToast() {
  return {
    textContent: "",
    hidden: true,
    dataset: {},
    classList: {
      names: new Set(),
      add(n) {
        this.names.add(n);
      },
      remove(n) {
        this.names.delete(n);
      },
    },
    removeAttribute() {
      delete this.dataset.kind;
    },
    offsetWidth: 0,
  };
}

function channelOver(statusLine) {
  const live = fakeRegion();
  const alert = fakeRegion();
  const toast = fakeToast();
  return {
    live,
    alert,
    toast,
    channel: createStatusChannel({ live, alert, toast, statusLine }),
  };
}

test("progress paints the status line when nobody else wants it", () => {
  const statusLine = fakeStatusLine();
  const { channel } = channelOver(statusLine);
  assert.equal(channel.progress("Fetching fonts for sample.docx…"), true);
  assert.equal(statusLine.textContent, "Fetching fonts for sample.docx…");
});

test("progress says nothing to a screen reader and raises no toast", () => {
  // The status line reports itself as unpainted, so `publish` WOULD toast here —
  // `needsToast` escalates anything the line cannot show. Progress must not,
  // because a card thrown over the document is for must-notice messages and a
  // font download is not one.
  const statusLine = fakeStatusLine();
  const { channel, live, alert, toast } = channelOver(statusLine);
  channel.progress("Fetching fonts for sample.docx…");
  assert.equal(live.textContent, "");
  assert.equal(alert.textContent, "");
  assert.equal(toast.hidden, true);
  assert.equal(toast.textContent, "");
});

test("progress never takes the line off a reader's message", () => {
  const statusLine = fakeStatusLine();
  const { channel } = channelOver(statusLine);
  // The reader's own feedback arrives the way every command's does.
  statusLine.textContent = "Place the caret inside a tracked change to accept it";
  channel.publish(statusLine.textContent, "error");

  assert.equal(channel.progress("Fetching fonts for sample.docx…"), false);
  assert.equal(statusLine.textContent, "Place the caret inside a tracked change to accept it");
  // And a progress CLEAR must not wipe it either — that is the half a
  // "don't overwrite" rule alone would miss, and it is how a refusal vanished
  // seconds after the reader earned it.
  assert.equal(channel.progress(""), false);
  assert.equal(statusLine.textContent, "Place the caret inside a tracked change to accept it");
});

test("the reader's next message takes the line back from progress", () => {
  const statusLine = fakeStatusLine();
  const { channel } = channelOver(statusLine);
  channel.progress("Fetching fonts for sample.docx…");
  // A command reports: `main.js` writes the text, then publishes it.
  statusLine.textContent = "Footnote added — type the note text";
  channel.publish(statusLine.textContent);
  // From here the line is the reader's, and the next progress line is refused
  // rather than inheriting a flag the previous pass left behind.
  assert.equal(channel.progress("Fetching fonts for Japanese…"), false);
  assert.equal(statusLine.textContent, "Footnote added — type the note text");
});

test("one progress pass may hand the line to the next", () => {
  const statusLine = fakeStatusLine();
  const { channel } = channelOver(statusLine);
  channel.progress("Fetching fonts for sample.docx…");
  assert.equal(channel.progress("Fetching fonts for sample.docx (Japanese, Korean)…"), true);
  assert.equal(statusLine.textContent, "Fetching fonts for sample.docx (Japanese, Korean)…");
  // And retires its own line when it is finished.
  assert.equal(channel.progress(""), true);
  assert.equal(statusLine.textContent, "");
  // A retired line is nobody's, so progress may have it again.
  assert.equal(channel.progress("Fetching fonts for emoji…"), true);
});

test("a channel with no status line refuses progress rather than throwing", () => {
  // A host that hides the footer, or an embed that never built one. The toast
  // and the live regions are still wired, and progress deliberately uses
  // neither, so the honest answer is "not shown".
  const { channel } = channelOver(null);
  assert.equal(channel.progress("Fetching fonts…"), false);
});

test("progress clears an error class the previous message left on the line", () => {
  const statusLine = fakeStatusLine();
  const { channel } = channelOver(statusLine);
  statusLine.textContent = "Could not load the Japanese font";
  statusLine.className = "status error";
  channel.publish(statusLine.textContent, "error");
  // The reader owns the line, so progress is refused and the class stands.
  channel.progress("Fetching fonts…");
  assert.equal(statusLine.className, "status error");
  // Once the line is free, progress is never a refusal and must not inherit
  // red: a font download is not an error.
  statusLine.textContent = "";
  channel.publish("");
  channel.progress("Fetching fonts…");
  assert.equal(statusLine.className, "status ");
});
