import assert from "node:assert/strict";
import test from "node:test";
import {
  cropToFill,
  cropToFit,
  isCropped,
  pictureToolCommands,
  visibleNaturalSize,
} from "../src/picture_tools.mjs";

// RM-29: Crop to Fill / Fit, Reset crop and Reset size (ONLYOFFICE's Image
// settings: `asc_cropFill`, `asc_cropFit`, `btnResetCrop`, `setOriginalSize`).
//
// Mutations that drive these red: swapping the two branches of `cropToFill`
// (cropping the short sides) fails the first test; dropping the minus sign in
// `cropToFit` (cropping instead of padding) fails the second; ignoring the crop
// in `visibleNaturalSize` fails the third; enabling Reset crop on an uncropped
// picture fails the gate test.

const near = (actual, expected) =>
  assert.ok(
    actual.every((value, i) => Math.abs(value - expected[i]) < 1e-9),
    `${JSON.stringify(actual)} != ${JSON.stringify(expected)}`,
  );

test("Crop to Fill covers the frame: a 2:1 frame over a 1:1 image hides a quarter top and bottom", () => {
  near(cropToFill([2, 1], [100, 100]), [0, 0.25, 0, 0.25]);
  near(cropToFill([1, 2], [100, 100]), [0.25, 0, 0.25, 0]);
  near(cropToFill([4, 3], [400, 300]), [0, 0, 0, 0]);
});

test("Crop to Fit pads the frame: a 2:1 frame around a 1:1 image outsets half on each side", () => {
  near(cropToFit([2, 1], [100, 100]), [-0.5, 0, -0.5, 0]);
  near(cropToFit([1, 2], [100, 100]), [0, -0.5, 0, -0.5]);
  // An extreme frame is clamped at the 100% outset the model stores.
  near(cropToFit([10, 1], [100, 100]), [-1, 0, -1, 0]);
});

test("the actual size of a cropped picture is the size of what the crop leaves", () => {
  assert.deepEqual(visibleNaturalSize([1000, 500], [0.1, 0.2, 0.1, 0.2]), [800, 300]);
  assert.deepEqual(visibleNaturalSize([1000, 500], null), [1000, 500]);
  // A padded (Fit) crop shows MORE than the image: its actual size is larger.
  assert.deepEqual(visibleNaturalSize([1000, 500], [-0.5, 0, -0.5, 0]), [2000, 500]);
  // A crop that leaves nothing falls back to the whole image, never to zero.
  assert.deepEqual(visibleNaturalSize([1000, 500], [0.6, 0, 0.6, 0]), [1000, 500]);
  assert.equal(isCropped([0, 0, 0, 0]), false);
  assert.equal(isCropped([0, -0.1, 0, 0]), true);
});

function engine({ natural = [800, 400], crop = [0, 0, 0, 0], extent = [400, 400] } = {}) {
  const calls = [];
  return {
    calls,
    pictureNaturalSize: () => natural,
    objectCrop: () => crop,
    objectExtent: () => extent,
    setImageCrop: (node, insets) => calls.push(["crop", node, insets]),
    setObjectExtent: (node, w, h) => calls.push(["extent", node, w, h]),
  };
}

function rows(doc, over = {}) {
  const announced = [];
  const list = pictureToolCommands({
    doc,
    node: "p1",
    mutationEnabled: true,
    mutationReason: "",
    t: (key) => key,
    runEdit: (thunk) => {
      thunk();
      return Promise.resolve(true);
    },
    announce: (text) => announced.push(text),
    ...over,
  });
  return { list, announced, byId: (id) => list.find((row) => row.id === id) };
}

test("each row writes through the engine as one edit, and says what it did", async () => {
  const doc = engine({ crop: [0.1, 0, 0.1, 0] });
  const { byId, announced } = rows(doc);
  byId("object.cropFill").run();
  byId("object.cropFit").run();
  byId("object.resetCrop").run();
  byId("object.resetSize").run();
  await Promise.resolve();
  await Promise.resolve();
  near(doc.calls[0][2], [0.25, 0, 0.25, 0]);
  near(doc.calls[1][2], [0, -0.5, 0, -0.5]);
  assert.deepEqual(doc.calls[2], ["crop", "p1", null]);
  assert.deepEqual(doc.calls[3], ["extent", "p1", 640, 400], "the visible 80% of 800 x 400");
  assert.deepEqual(announced, [
    "object.picture.cropFilled",
    "object.picture.cropFitted",
    "object.picture.cropReset",
    "object.picture.sizeReset",
  ]);
});

test("a row that cannot act is disabled WITH the reason, and its run writes nothing", () => {
  const doc = engine({ natural: [], crop: [0, 0, 0, 0] });
  const { list, byId } = rows(doc);
  assert.equal(byId("object.resetCrop").enabled, false);
  assert.equal(byId("object.resetCrop").disabledReason, "object.picture.reason.notCropped");
  for (const id of ["object.cropFill", "object.cropFit", "object.resetSize"]) {
    assert.equal(byId(id).enabled, false, id);
    assert.equal(byId(id).disabledReason, "object.picture.reason.noNaturalSize", id);
  }
  for (const row of list) row.run();
  assert.deepEqual(doc.calls, []);
});

test("the review-mode gate closes every row with the mode's own reason", () => {
  const { list } = rows(engine({ crop: [0.1, 0.1, 0.1, 0.1] }), {
    mutationEnabled: false,
    mutationReason: "Turn on Editing to change this object",
  });
  assert.equal(list.length, 4);
  for (const row of list) {
    assert.equal(row.enabled, false, row.id);
    assert.equal(row.disabledReason, "Turn on Editing to change this object", row.id);
  }
});
