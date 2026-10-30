// Paste as picture (`paste_picture.mjs`): what reaches the engine's rasteriser,
// and what reaches the insert. The engine half (the picture itself) is held by
// `casual-doc-wasm`'s `picture_insert` tests.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createPasteAsPicture, markerRuns, pictureDpi, textToRuns } from "../src/paste_picture.mjs";

function fakeEditor({ mode = "editing", raster, rasterError } = {}) {
  const log = [];
  const ed = {
    doc: {
      rasterizeClipboard(node, runsJson, dpi) {
        log.push({ rasterize: { node, runs: JSON.parse(runsJson), dpi } });
        if (rasterError) throw rasterError;
        return raster ?? { png: new Uint8Array([1, 2, 3]), widthPx: 400, heightPx: 100, dpi, free() {} };
      },
    },
    selection: { focus: { node: "p1", offset: 0 } },
    reviewMode: mode,
    blockMutationInViewing: () => mode === "viewing",
    blockUntrackedInSuggesting: () => mode === "suggesting",
    insertImageFromBlob: async (blob, size) => log.push({ insert: { type: blob.type, size } }),
    setStatus: (text, kind) => log.push({ status: text, kind }),
  };
  return { ed, log };
}

function clipboardOf(items, text = "") {
  return {
    read: async () => items,
    readText: async () => text,
  };
}

test("plain text becomes one run per line, with paragraph breaks between", () => {
  assert.deepEqual(textToRuns("a\r\nb\n\nc"), [
    { text: "a" },
    { paragraphBreak: true },
    { text: "b" },
    { paragraphBreak: true },
    { paragraphBreak: true },
    { text: "c" },
  ]);
});

test("an internal copy's marker yields its runs, in either marker shape", () => {
  assert.deepEqual(markerRuns('[{"text":"x"}]'), [{ text: "x" }]);
  assert.deepEqual(markerRuns('{"blocks":[],"runs":[{"text":"y"}]}'), [{ text: "y" }]);
  assert.equal(markerRuns("not json"), null);
});

test("the picture is painted at least twice screen resolution, capped", () => {
  assert.equal(pictureDpi(1), 192);
  assert.equal(pictureDpi(2.5), 240);
  assert.equal(pictureDpi(4), 288);
  assert.equal(pictureDpi(Number.NaN), 192);
});

test("text on the clipboard is rasterised at the caret and inserted at its true size", async () => {
  const { ed, log } = fakeEditor();
  const paste = createPasteAsPicture(ed, { clipboard: clipboardOf([], "Hello\nWorld"), devicePixelRatio: () => 1 });
  await paste.pasteAsPicture();
  const raster = log.find((entry) => entry.rasterize).rasterize;
  assert.equal(raster.node, "p1");
  assert.equal(raster.dpi, 192);
  assert.deepEqual(raster.runs, textToRuns("Hello\nWorld"));
  const insert = log.find((entry) => entry.insert).insert;
  assert.equal(insert.type, "image/png");
  // 400 x 100 device px at 192 dpi is 200 x 50 CSS px.
  assert.deepEqual(insert.size, { widthPx: 200, heightPx: 50 });
});

test("a picture already on the clipboard is pasted as itself, not rasterised", async () => {
  const { ed, log } = fakeEditor();
  const image = new Blob([new Uint8Array([9])], { type: "image/png" });
  const items = [{ types: ["image/png"], getType: async () => image }];
  await createPasteAsPicture(ed, { clipboard: clipboardOf(items, "ignored") }).pasteAsPicture();
  assert.equal(log.some((entry) => entry.rasterize), false);
  assert.deepEqual(log.filter((entry) => entry.insert).map((entry) => entry.insert), [{ type: "image/png", size: undefined }]);
});

test("Viewing and Suggesting refuse before reading the clipboard", async () => {
  for (const mode of ["viewing", "suggesting"]) {
    const { ed, log } = fakeEditor({ mode });
    let read = false;
    const clipboard = { read: async () => ((read = true), []), readText: async () => "x" };
    await createPasteAsPicture(ed, { clipboard }).pasteAsPicture();
    assert.equal(read, false, mode);
    assert.equal(log.length, 0, mode);
  }
});

test("an engine refusal is said, and Paste Options' undo does not run before a picture exists", async () => {
  const error = Object.assign(new Error("too large"), { code: "paste.picture-too-large" });
  const { ed, log } = fakeEditor({ rasterError: error });
  let undone = false;
  const ok = await createPasteAsPicture(ed).pasteRunsAsPicture([{ text: "x" }], { beforeInsert: async () => (undone = true) });
  assert.equal(ok, false);
  assert.equal(undone, false, "the text paste is kept when no picture could be made");
  assert.deepEqual(log.at(-1), { status: "pastePicture.tooLarge", kind: "error" });
});

test("Paste Options' undo runs after painting and before inserting", async () => {
  const { ed, log } = fakeEditor();
  await createPasteAsPicture(ed).pasteRunsAsPicture([{ text: "x" }], { beforeInsert: async () => log.push("undo") });
  const order = log.map((entry) => (entry === "undo" ? "undo" : Object.keys(entry)[0]));
  assert.deepEqual(order, ["rasterize", "undo", "insert"]);
});

test("a clipboard the browser will not read says so, and an empty one says that", async () => {
  const blockedBoard = { read: async () => Promise.reject(new Error("denied")), readText: async () => Promise.reject(new Error("denied")) };
  const blocked = fakeEditor();
  await createPasteAsPicture(blocked.ed, { clipboard: blockedBoard }).pasteAsPicture();
  assert.equal(blocked.log.at(-1).status, "pastePicture.blocked");
  const empty = fakeEditor();
  await createPasteAsPicture(empty.ed, { clipboard: clipboardOf([], "") }).pasteAsPicture();
  assert.equal(empty.log.at(-1).status, "pastePicture.nothing");
  const blank = fakeEditor();
  await createPasteAsPicture(blank.ed, { clipboard: clipboardOf([], "   ") }).pasteAsPicture();
  assert.equal(blank.log.at(-1).status, "pastePicture.empty");
});
