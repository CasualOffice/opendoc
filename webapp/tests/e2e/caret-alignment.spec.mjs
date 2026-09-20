// Clicking must put the caret on the line you clicked — including in a document
// that carries a `w:docGrid`.
//
// This guard exists because that was broken for five weeks and nothing caught
// it. PR #479 (2026-08-12) read an *absent* `w:docGrid/@w:type` as an active
// line grid, so every line was rounded up to the section's pitch. The page
// count went wrong, which is what got reported — but the click-to-line mapping
// went wrong with it, and on a real customer document a click put the caret
// nearly 200px away, on a different line.
//
// Nothing saw it because **every** caret spec loads `?fixture=rich` through
// `gotoEditor`, and that fixture has no `docGrid`. A whole class of document
// was untested at the interaction users perform most often.
//
// `fixtures/generated/pagination-fidelity.docx` carries
// `<w:docGrid w:linePitch="299"/>` with no type — the exact shape that broke.
//
// The oracle is deliberately NOT hand-computed geometry. A first version of
// this spec predicted each line's y from `w:pgMar` and a 240-twip line height
// and failed by one line on the table rows, whose cell margins it did not
// model — a guard that reports a defect the product does not have is worse
// than no guard. Instead the caret walks the document with Down and records
// where the engine *paints* it on each line; clicking back at those exact
// points must return to the same lines. That makes the assertion "painting and
// hit-testing agree", which is precisely what a user means by aligned, and it
// stays true however the document is laid out.
import { test, expect } from "./fixtures.mjs";

const FIXTURE = "../fixtures/generated/pagination-fidelity.docx";

/** The a11y mirror's block texts in flow order — where the caret really is. */
const blocks = (page) =>
  page.locator("#a11yDocument").evaluate((el) =>
    [...el.querySelectorAll("p,h1,h2,h3,h4,h5,h6,li,td,th")].map((n) => n.textContent.trim()),
  );

const caretBox = async (page) => {
  const boxes = [];
  for (const c of await page.locator(".overlay .caret").all()) {
    const b = await c.boundingBox().catch(() => null);
    if (b && b.height > 0) boxes.push(b);
  }
  return boxes[0] ?? null;
};

test("clicking returns the caret to the line the engine painted it on", async ({
  page,
  consoleErrors,
}) => {
  await page.goto("/editor.html");
  await page.waitForFunction(
    () => document.getElementById("file") && !document.getElementById("file").disabled,
    null,
    { timeout: 45_000 },
  );
  await page.locator("#file").setInputFiles(FIXTURE);
  await expect.poll(() => page.locator(".page-wrap").count(), { timeout: 45_000 }).toBeGreaterThan(0);
  await page.waitForTimeout(800);

  const first = await page.locator(".page-wrap").first().boundingBox();
  await page.mouse.click(first.x + 40, first.y + first.height * 0.06);
  await page.waitForTimeout(200);

  // Walk down the document, recording where the engine paints the caret on
  // each line. These are the ground-truth points: the engine's own answer to
  // "where is line N".
  const painted = [];
  for (let i = 0; i < 12; i += 1) {
    const box = await caretBox(page);
    if (box) painted.push({ y: box.y + box.height / 2, x: box.x });
    await page.keyboard.press("ArrowDown");
    await page.waitForTimeout(90);
  }
  expect(painted.length, "the caret must be painted while walking the document").toBeGreaterThan(8);

  // Now click back at those exact points. Each must return to the line whose
  // caret was painted there — anything else is a click the user aimed at one
  // line and landed on another.
  for (const index of [3, 6, 9]) {
    const target = painted[index];

    await page.mouse.click(target.x + 1, target.y);
    await page.waitForTimeout(150);
    await page.keyboard.insertText("@");
    await page.waitForTimeout(300);
    const marked = await blocks(page);
    const landed = marked.findIndex((t) => t.includes("@"));
    expect(landed, "typing after a click must insert somewhere").toBeGreaterThanOrEqual(0);
    await page.keyboard.press("Backspace");
    await page.waitForTimeout(200);

    const back = await caretBox(page);
    expect(back, "the caret must still be painted after clicking").not.toBeNull();
    const drift = Math.abs(back.y + back.height / 2 - target.y);
    expect(
      drift,
      `clicking at the point where line ${index}'s caret was painted moved it ` +
        `${drift.toFixed(0)}px away — painting and hit-testing disagree, which is ` +
        `what a misaligned cursor is`,
    ).toBeLessThan(back.height);
  }

  expect(consoleErrors).toEqual([]);
});
