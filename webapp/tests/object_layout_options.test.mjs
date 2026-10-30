import { test } from "node:test";
import assert from "node:assert/strict";

import {
  EMU_PER_PT,
  layoutOptionsCommand,
  layoutOptionsPayload,
  readLayoutOptions,
} from "../src/object_layout_options.mjs";
import { EN_STRINGS } from "../src/en_strings.mjs";

const floating = JSON.stringify({
  floating: true,
  horizontal: { relativeFrom: "column", offsetEmu: 0 },
  vertical: { relativeFrom: "paragraph", offsetEmu: 0 },
  wrap: "square",
  behindDoc: false,
  wrapDistances: { topEmu: 0, bottomEmu: 12700, startEmu: 114300, endEmu: 114300 },
  allowOverlap: true,
  moveWithText: true,
});

test("the engine's position reads into the dialog in points", () => {
  assert.deepEqual(readLayoutOptions(floating), {
    available: true,
    top: 0,
    bottom: 1,
    left: 9,
    right: 9,
    moveWithText: true,
    allowOverlap: true,
  });
});

test("an inline object, a group member and no object are refused with a reason", () => {
  assert.deepEqual(readLayoutOptions(JSON.stringify({ floating: false })), {
    available: false,
    reasonKey: "object.layoutOptions.inline",
  });
  assert.equal(readLayoutOptions(JSON.stringify({ floating: false, groupChild: true })).reasonKey, "object.wrap.groupChild");
  assert.equal(readLayoutOptions("").reasonKey, "object.wrap.notAnObject");
  for (const read of [readLayoutOptions(""), readLayoutOptions('{"floating":false}')]) {
    assert.ok(read.reasonKey in EN_STRINGS, read.reasonKey);
  }
});

test("only what changed is sent, and an untouched dialog sends nothing", () => {
  const before = readLayoutOptions(floating);
  const same = { top: "0", bottom: "1", left: "9", right: "9", moveWithText: true, allowOverlap: true };
  assert.equal(layoutOptionsPayload(before, same), null);

  assert.deepEqual(layoutOptionsPayload(before, { ...same, allowOverlap: false }), { allowOverlap: false });
  assert.deepEqual(layoutOptionsPayload(before, { ...same, moveWithText: false }), { moveWithText: false });
  assert.deepEqual(layoutOptionsPayload(before, { ...same, top: "4.5" }), {
    wrapDistances: { topEmu: 4.5 * EMU_PER_PT, bottomEmu: EMU_PER_PT, startEmu: 9 * EMU_PER_PT, endEmu: 9 * EMU_PER_PT },
  });
});

test("a distance outside Word's range names the field instead of being clamped", () => {
  const before = readLayoutOptions(floating);
  const form = { top: "0", bottom: "1", left: "9", right: "9", moveWithText: true, allowOverlap: true };
  assert.deepEqual(layoutOptionsPayload(before, { ...form, left: "-1" }), { error: "left" });
  assert.deepEqual(layoutOptionsPayload(before, { ...form, right: "1585" }), { error: "right" });
  assert.deepEqual(layoutOptionsPayload(before, { ...form, top: "abc" }), { error: "top" });
});

test("the menu row is disabled with the review-mode reason first, then the object's", () => {
  const t = (key) => `<${key}>`;
  let opened = 0;
  const live = layoutOptionsCommand(readLayoutOptions(floating), { mutationEnabled: true, mutationReason: "", t, open: () => opened++ });
  assert.equal(live.id, "object.layoutOptions");
  assert.equal(live.enabled, true);
  live.run();
  assert.equal(opened, 1);

  const viewing = layoutOptionsCommand(readLayoutOptions(floating), { mutationEnabled: false, mutationReason: "why", t, open() {} });
  assert.deepEqual([viewing.enabled, viewing.disabledReason], [false, "why"]);

  const inline = layoutOptionsCommand(readLayoutOptions('{"floating":false}'), { mutationEnabled: true, mutationReason: "", t, open() {} });
  assert.deepEqual([inline.enabled, inline.disabledReason], [false, "<object.layoutOptions.inline>"]);
  assert.ok("object.layoutOptions" in EN_STRINGS);
});
