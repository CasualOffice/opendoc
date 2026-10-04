import assert from "node:assert/strict";
import test from "node:test";
import { groupsToOverflow } from "../src/ribbon_overflow.mjs";

// The arithmetic the ribbon could not test while it was welded to the DOM.
// `docs/63`'s width budget is the reason this matters: the band has finite room
// and the wrong eviction order exiles a group a reader reaches for constantly.
const band = (widths, { pinned = [], clipboard = "clipboard" } = {}) => ({
  groups: Object.keys(widths),
  widthOf: (g) => widths[g],
  isPinned: (g) => pinned.includes(g),
  isLastResort: (g) => g === clipboard,
});

test("nothing moves when the band already fits", () => {
  const moved = groupsToOverflow({ ...band({ clipboard: 100, font: 100 }), avail: 300, reserve: 44 });
  assert.deepEqual(moved, []);
});

test("unpinned groups leave from the RIGHT, because the left is what a reader reaches for", () => {
  const moved = groupsToOverflow({
    ...band({ clipboard: 100, font: 100, paragraph: 100, styles: 100 }),
    avail: 300,
    reserve: 44,
  });
  // 400 total, 256 usable: the two rightmost suffice.
  assert.deepEqual(moved, ["styles", "paragraph"]);
});

test("a pinned group stays while an unpinned one can still be moved", () => {
  const moved = groupsToOverflow({
    ...band({ clipboard: 100, editing: 100, font: 100 }, { pinned: ["clipboard", "editing"] }),
    avail: 250,
    reserve: 44,
  });
  assert.deepEqual(moved, ["font"]);
});

test("when even the pinned composition cannot fit, pinned groups go too — but Clipboard never does", () => {
  // The width is chosen so the walk REACHES Clipboard: with 56px usable and
  // three 100px groups, moving the other two still leaves 100px, so the loop
  // runs on to Clipboard and is refused there. An earlier fixture fitted after
  // two moves, so the protection was never exercised and the assertion could
  // not be driven red — the mutation exposed the guard rather than the code.
  const moved = groupsToOverflow({
    ...band({ clipboard: 100, editing: 100, mode: 100 }, { pinned: ["clipboard", "editing", "mode"] }),
    avail: 100,
    reserve: 44,
  });
  assert.ok(!moved.includes("clipboard"), `Clipboard must never leave the band; moved ${moved}`);
  assert.deepEqual(moved, ["mode", "editing"], "both other pinned groups go, and the band still overflows");
});

test("the overflow button's own room is reserved, so the last group that 'fits' does not hide it", () => {
  const exactly = groupsToOverflow({ ...band({ clipboard: 100, font: 100 }), avail: 200, reserve: 0 });
  const reserved = groupsToOverflow({ ...band({ clipboard: 100, font: 100 }), avail: 200, reserve: 44 });
  assert.deepEqual(exactly, [], "200 of 200 fits when nothing is reserved");
  assert.deepEqual(reserved, ["font"], "the same band does not fit once the button is reserved");
});
