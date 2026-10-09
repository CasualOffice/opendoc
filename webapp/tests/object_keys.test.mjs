// The arrow keys on a selected object and in crop mode (`object_keys.mjs`).
//
// Two defects, both of which a user meets in the first minute: Ctrl+Arrow fell
// through to caret navigation and dropped the object instead of nudging it by a
// pixel as Word does, and in crop mode an arrow reached the object nudge and
// MOVED THE PICTURE out from under the crop (`docs/104` HF-106).
import assert from "node:assert/strict";
import test from "node:test";

const {
  CROP_PAN,
  CROP_PAN_LARGE,
  NUDGE_TWIP,
  NUDGE_TWIP_FINE,
  NUDGE_TWIP_LARGE,
  handleCropKey,
  objectNudge,
  panCrop,
} = await import("../src/object_keys.mjs");

const key = (name, mods = {}) => ({
  key: name,
  shiftKey: false,
  ctrlKey: false,
  altKey: false,
  metaKey: false,
  ...mods,
});

test("an arrow nudges by the plain step, Shift by the large one", () => {
  assert.deepEqual(objectNudge(key("ArrowRight")), { dx: 1, dy: 0, step: NUDGE_TWIP });
  assert.deepEqual(objectNudge(key("ArrowUp", { shiftKey: true })), { dx: 0, dy: -1, step: NUDGE_TWIP_LARGE });
});

test("Ctrl (Windows) and Option (Mac) take Word's one-pixel step", () => {
  assert.equal(objectNudge(key("ArrowLeft", { ctrlKey: true })).step, NUDGE_TWIP_FINE);
  assert.equal(objectNudge(key("ArrowDown", { altKey: true })).step, NUDGE_TWIP_FINE);
  assert.ok(NUDGE_TWIP_FINE < NUDGE_TWIP && NUDGE_TWIP < NUDGE_TWIP_LARGE);
});

test("Cmd and non-arrows are not nudges, so line navigation still works on a Mac", () => {
  assert.equal(objectNudge(key("ArrowRight", { metaKey: true })), null);
  assert.equal(objectNudge(key("a")), null);
  assert.equal(objectNudge(key("Tab")), null);
});

test("panning moves the kept area and preserves its size exactly", () => {
  const crop = { l: 0.2, t: 0.1, r: 0.3, b: 0.1 };
  const right = panCrop(crop, "ArrowRight", false);
  assert.equal(right.moved, true);
  assert.ok(Math.abs(right.crop.l - (0.2 + CROP_PAN)) < 1e-12);
  assert.ok(Math.abs(right.crop.l + right.crop.r - (crop.l + crop.r)) < 1e-12, "kept width unchanged");
  const up = panCrop(crop, "ArrowUp", true);
  assert.ok(Math.abs(up.crop.t - Math.max(0, 0.1 - CROP_PAN_LARGE)) < 1e-12);
  assert.ok(Math.abs(up.crop.t + up.crop.b - 0.2) < 1e-12, "kept height unchanged");
});

test("panning stops at the picture's edge, and reports when there was nothing to move", () => {
  const crop = { l: 0.004, t: 0, r: 0.5, b: 0 };
  const left = panCrop(crop, "ArrowLeft", true);
  assert.equal(left.crop.l, 0, "never past the left edge");
  assert.ok(Math.abs(left.crop.r - 0.504) < 1e-12);
  const none = panCrop({ l: 0, t: 0, r: 0, b: 0 }, "ArrowDown", false);
  assert.equal(none.moved, false, "an uncropped picture has nothing to move");
  assert.equal(panCrop(crop, "Enter", false), null);
});

test("the crop session owns every arrow — a modified one too — and never lets it reach the nudge", () => {
  const calls = [];
  const io = {
    session: { crop: { l: 0.1, t: 0, r: 0.1, b: 0 } },
    commit: () => calls.push("commit"),
    cancel: () => calls.push("cancel"),
    redraw: () => calls.push("redraw"),
    nothingToMove: () => calls.push("nothing"),
  };
  const press = (name, mods) => {
    let prevented = false;
    const event = { ...key(name, mods), preventDefault: () => (prevented = true) };
    return { handled: handleCropKey(event, io), prevented };
  };
  assert.deepEqual(press("ArrowRight"), { handled: true, prevented: true });
  assert.deepEqual(press("ArrowRight", { ctrlKey: true }), { handled: true, prevented: true });
  assert.deepEqual(press("Enter"), { handled: true, prevented: true });
  assert.deepEqual(press("Escape"), { handled: true, prevented: true });
  assert.deepEqual(calls, ["redraw", "redraw", "commit", "cancel"]);
  io.session.crop = { l: 0, t: 0, r: 0, b: 0 };
  assert.deepEqual(press("ArrowUp"), { handled: true, prevented: true });
  assert.equal(calls.at(-1), "nothing", "an arrow with nothing to move says so");
  assert.deepEqual(press("a"), { handled: false, prevented: false });
});
