// The column a run of Up/Down presses aims at must survive a short line.
//
// Reported as part of "arrow causes cursor to flick in multiple places", and
// the half of it that #556 did not fix. Measured on the owner's document: a
// caret at x = 1456 on a long line, ArrowDown onto a short line (392), then an
// empty paragraph (346), and the next long line was entered at 346 — not at
// 1456. Walking back up it stayed at 346. Word and Google Docs both restore
// the original column, and this is one of the most-used interactions there is.
//
// What the guard asserts is the whole rule rather than the one reported
// number, because the reported number is a property of the owner's file:
//
//   * every line the walk lands on that is LONG ENOUGH must be entered at the
//     goal column;
//   * every line too short to offer it clamps the caret to its own end, and
//     that clamp must not become the new goal.
//
// "Long enough" is measured, not assumed: the same walk is made twice down the
// same lines — once pressing Home+End at every step, which records how far
// right each line can actually reach and (being horizontal moves) deliberately
// destroys the column, and once with nothing but ArrowDown. The second walk is
// then read against the first. Nothing here depends on font metrics, zoom, or
// the fixture's exact wrapping.
//
// Why the walk starts on page 2 and runs past the end of it: every browser
// spec in this repo works inside the first viewport near the document start,
// which is why several caret bugs survived weeks. This one scrolls first and
// asserts it crossed a page boundary with the column still held.
import { test, expect, gotoEditor, pageSheet, stableBox } from "./fixtures.mjs";
import { makeGoalColumnDocx } from "./large-docx.mjs";

const GOAL_COLUMN_DOCX = {
  name: "goal-column.docx",
  mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  buffer: Buffer.from(makeGoalColumnDocx()),
};

/** How many ArrowDown presses each walk makes. Enough, on this fixture, to
 *  pass several short lines and run off the bottom of page 2. */
const STEPS = 20;

/** Where the caret IS — geometry, not identity.
 *
 *  A model position cannot answer this question: `moveCaret` can return a
 *  different node/offset on the same visual line, so "did the position change"
 *  says yes while the caret slides sideways. The editor paints to canvas, so
 *  the caret is the only DOM element that carries the answer, and it is the
 *  engine's own `caretRect` that put it there. */
function caretAt(page) {
  return page.evaluate(() => {
    const caret = document.querySelector(".overlay .caret");
    if (!caret) return null;
    const view = document.getElementById("viewport");
    const rect = caret.getBoundingClientRect();
    const viewRect = view.getBoundingClientRect();
    return {
      x: Math.round(rect.left),
      docY: Math.round(view.scrollTop + rect.top - viewRect.top),
      page: Number(caret.closest(".page-wrap")?.dataset.pageNumber ?? 0),
    };
  });
}

/** The bottom-most rectangle of an extended selection, as `{x, bottom}` in
 *  client coordinates — where a downward extension's focus is.
 *
 *  There is no caret element while a selection exists: the overlay paints the
 *  engine's `selectionRects` instead, and they carry no padding or border, so
 *  the last one's right edge is the same number `caretRect` would give.
 *
 *  With one exception the caller has to handle: a focus sitting at the START of
 *  a line covers nothing, the engine emits no rectangle for it, and the
 *  bottom-most rectangle is then the line ABOVE — whose right edge is that
 *  line's full width and says nothing about the focus. `bottom` is how the
 *  caller tells the two apart. */
function selectionFocus(page) {
  return page.evaluate(() => {
    const rects = [...document.querySelectorAll(".overlay .highlight")].map((el) =>
      el.getBoundingClientRect(),
    );
    if (rects.length === 0) return null;
    const last = rects.reduce((a, b) => (b.bottom > a.bottom ? b : a));
    const view = document.getElementById("viewport");
    const viewRect = view.getBoundingClientRect();
    return {
      x: Math.round(last.right),
      bottom: Math.round(view.scrollTop + last.bottom - viewRect.top),
    };
  });
}

async function openGoalColumnFixture(page) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.setInputFiles("#file", GOAL_COLUMN_DOCX);
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status?.textContent === "" &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length >= 2
      );
    },
    null,
    { timeout: 45_000 },
  );
}

/**
 * Puts the caret at the end of a long line partway down page 2, the same way
 * every time, and answers with the sheet's box and that caret.
 *
 * Deliberately a click plus Home/End rather than a count of key presses: the
 * click is a document coordinate, so it lands in the same paragraph whatever
 * the font does, and Home/End pin the caret to a line edge rather than to a
 * character. The short search afterwards is what makes "a long line" true by
 * measurement instead of by assumption — and it is deterministic, so both
 * walks start in exactly the same place.
 */
async function startAtTheEndOfALongLine(page) {
  const sheet = await pageSheet(page, 2);
  const paper = sheet.locator(".page");
  const box = await stableBox(paper);
  // Deliberately a point in the MIDDLE of a line rather than in the left
  // margin: the column a click establishes has to be distinguishable from both
  // a line's start and its end, or a test cannot tell a stale column from a
  // fresh one.
  await paper.click({
    position: { x: Math.round(box.width * 0.4), y: Math.round(box.height * 0.55) },
  });
  await page.keyboard.press("Home");
  await page.keyboard.press("End");
  let at = await caretAt(page);
  for (let i = 0; i < 8 && at && at.x - box.x < box.width * 0.5; i++) {
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Home");
    await page.keyboard.press("End");
    at = await caretAt(page);
  }
  expect(at, "no caret after clicking into page 2").not.toBeNull();
  expect(
    at.x - box.x,
    "the walk must start at the end of a genuinely long line",
  ).toBeGreaterThan(box.width * 0.5);
  return { box, start: at };
}

test("a short line clamps the caret without taking the goal column", async ({
  page,
  consoleErrors,
}) => {
  await openGoalColumnFixture(page);

  // Walk one: how far right each line the walk visits can reach. Home+End are
  // horizontal moves, so this walk also proves they clear the column — if they
  // did not, this walk would carry one and its own measurements would be wrong.
  const { box, start } = await startAtTheEndOfALongLine(page);
  const reach = [];
  for (let i = 0; i < STEPS; i++) {
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("Home");
    await page.keyboard.press("End");
    reach.push(await caretAt(page));
  }

  // Walk two: the same lines, with nothing but ArrowDown.
  const second = await startAtTheEndOfALongLine(page);
  const goal = second.start.x;
  expect(second.start.docY, "the two walks must start on the same line").toBe(start.docY);
  const walked = [];
  for (let i = 0; i < STEPS; i++) {
    await page.keyboard.press("ArrowDown");
    walked.push(await caretAt(page));
  }

  // Walk three: the same path with Shift held. Extending is a vertical move
  // too, so the focus has to travel exactly where the collapsed caret did.
  await startAtTheEndOfALongLine(page);
  const extended = [];
  for (let i = 0; i < STEPS; i++) {
    await page.keyboard.press("Shift+ArrowDown");
    extended.push(await selectionFocus(page));
  }
  expect(await page.locator(".overlay .highlight").count()).toBeGreaterThan(0);

  // A glyph's worth of slack: landing "at the goal column" means the nearest
  // character boundary to it, which is up to one character away.
  const slack = Math.max(8, Math.round(box.width * 0.02));
  let clamped = 0;
  let restored = 0;
  let restoredAfterThePageTurned = 0;
  for (let i = 0; i < STEPS; i++) {
    expect(walked[i], `no caret after ArrowDown ${i + 1}`).not.toBeNull();
    expect(
      walked[i].docY,
      `the two walks diverged at step ${i + 1}: ${walked[i].docY} vs ${reach[i].docY}`,
    ).toBe(reach[i].docY);
    if (reach[i].x >= goal + slack) {
      expect(
        Math.abs(walked[i].x - goal),
        `step ${i + 1} landed at x=${walked[i].x} on a line that reaches ${reach[i].x}; ` +
          `the goal column ${goal} was lost`,
      ).toBeLessThanOrEqual(slack);
      restored++;
      if (walked[i].page > start.page) restoredAfterThePageTurned++;
      // The extension's focus is on the same line and at the same column. Its
      // bottom-most rectangle has to belong to the focus's OWN line to say
      // anything — a focus parked at a line's left edge covers nothing, emits
      // no rectangle, and leaves the line above as the bottom-most one, which
      // is exactly what losing the column looks like here.
      expect(
        extended[i].bottom,
        `step ${i + 1}: extending stopped a line short of the caret walk — ` +
          `the focus is at a line start, not at the goal column ${goal}`,
      ).toBeGreaterThan(walked[i].docY + 2);
      expect(
        Math.abs(extended[i].x - goal),
        `step ${i + 1}: Shift+ArrowDown put the focus at x=${extended[i].x}, not at ${goal}`,
      ).toBeLessThanOrEqual(slack);
    } else if (reach[i].x <= goal - slack) {
      expect(
        Math.abs(walked[i].x - reach[i].x),
        `step ${i + 1} landed at x=${walked[i].x} on a line that ends at ${reach[i].x}`,
      ).toBeLessThanOrEqual(slack);
      clamped++;
    }
  }

  // The walk has to have actually met the situation, or it proves nothing: a
  // run of equal-length lines keeps its column with or without this fix.
  expect(clamped, "the walk never passed a line shorter than the goal column").toBeGreaterThan(1);
  expect(restored, "the walk never reached a line long enough to test").toBeGreaterThan(1);
  expect(
    restoredAfterThePageTurned,
    "the walk never crossed a page boundary with the column held",
  ).toBeGreaterThan(0);

  expect(consoleErrors).toEqual([]);
});

test("a horizontal move ends the run and drops the column", async ({ page, consoleErrors }) => {
  await openGoalColumnFixture(page);
  const { box, start } = await startAtTheEndOfALongLine(page);
  const slack = Math.max(8, Math.round(box.width * 0.02));

  // Home is a horizontal move: it ends the run. The next ArrowDown must aim at
  // where the caret now IS, not at the column the run before it held.
  await page.keyboard.press("ArrowDown"); // sets the column
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Home"); // …and a horizontal move clears it
  const afterHome = await caretAt(page);
  await page.keyboard.press("ArrowDown");
  const next = await caretAt(page);
  expect(
    Math.abs(next.x - afterHome.x),
    `after Home the caret was at ${afterHome.x}; ArrowDown then jumped to ${next.x}, ` +
      `which is the old column (${start.x}), not the new one`,
  ).toBeLessThanOrEqual(slack);

  expect(consoleErrors).toEqual([]);
});
