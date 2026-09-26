// Editing an object must not scroll the reader to the text caret.
//
// Owner report: editing sends the view back to page 1, and it is not one
// command — "img readjust and resize.. find all cases".
//
// The cause is one line, not thirteen. `applyEditResult` ends every edit with
// `scrollCaretIntoView()` unless the caller passes `keepView: true`. Of 85
// `runEdit` call sites only 9 passed it, and NONE of the object ones did:
// resize by drag and by the inspector, crop, move, anchor position, wrap (twice),
// text-box body properties, alt text, shape fill, shape outline, and the two
// review `movePair` edits. So every one of them scrolled the viewport to the
// TEXT caret, which during an object gesture is wherever the user last typed —
// frequently page 1.
//
// Verified before fixing, by instrumenting `scrollCaretIntoView` and applying a
// wrap mode to a selected float:
//
//     scrollCaretIntoView calls after object wrap edit: 1
//        <- at applyEditResult (src/main.js:8901)
//
// and 0 after the fix, while plain typing still reports 1 — the caret is still
// followed when the caret is what was edited.
//
// The fix is therefore in `applyEditResult` and not `keepView: true` written
// thirteen times: the fourteenth object command would have forgotten it. This
// spec is written against that rule rather than against any one command, so a
// new object operation is covered on the day it lands.
import { test, expect, stableBox } from "./fixtures.mjs";

/** Selecting the float in `float.docx`. The position is the one
 *  `object-command-reach.spec.mjs` uses, for the same object. */
async function selectFloat(page) {
  await page.goto("/editor.html?fixture=float");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * 0.14, y: box.height * 0.11 } });
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");
}

const scrollTop = (page) =>
  page.evaluate(() => document.getElementById("viewport").scrollTop);

/** Scroll far from the caret — which is up at the object — so that "follow the
 *  caret" and "stay where the reader is" are different answers. Without this the
 *  caret is already on screen, `scrollCaretIntoView` is a no-op, and the guard
 *  would pass whether or not the bug is present. */
async function scrollAwayFromCaret(page) {
  const top = await page.evaluate(() => {
    const v = document.getElementById("viewport");
    v.scrollTop = v.scrollHeight - v.clientHeight;
    return v.scrollTop;
  });
  expect(top, "the fixture must have somewhere to scroll TO, or this proves nothing").toBeGreaterThan(80);
  return top;
}

/** Runs an object command from the palette, so the object need not be on screen
 *  for the gesture — which is the whole point of the scenario. */
async function runObjectCommand(page, query, match) {
  await page.keyboard.press("Meta+Shift+KeyP");
  await expect(page.locator("#cmdInput")).toBeVisible();
  await page.locator("#cmdInput").fill(query);
  const options = page.locator("#cmdList [role=option]");
  await expect(options.first()).toBeVisible();
  const labels = await options.allTextContents();
  const index = labels.findIndex((t) => match.test(t));
  expect(index, `no palette option matched ${match} in: ${labels.join(" | ")}`).toBeGreaterThan(-1);
  await options.nth(index).click();
}

test("changing an object's wrap leaves the reader where they were", async ({
  page,
  consoleErrors,
}) => {
  await selectFloat(page);
  const before = await scrollAwayFromCaret(page);

  await runObjectCommand(page, "wrap", /Wrap text (Square|Tight|Through)/i);
  await expect(page.locator("#pages")).toHaveAttribute("data-object-mode", "selected");

  const after = await scrollTop(page);
  expect(
    Math.abs(after - before),
    `editing the object scrolled the view from ${before} to ${after} — the reader was thrown off the object they were editing`,
  ).toBeLessThanOrEqual(2);
  expect(consoleErrors).toEqual([]);
});

test("the caret is still followed when the CARET is what was edited", async ({
  page,
  consoleErrors,
}) => {
  // The fix must not become "never scroll". Typing far from the viewport still
  // has to bring the caret back, or the cure is worse than the disease.
  await page.goto("/editor.html?fixture=demo");
  await page.waitForFunction(() => document.querySelectorAll(".page-wrap").length > 0, null, {
    timeout: 45_000,
  });
  const canvas = page.locator(".page-wrap .page").first();
  const box = await stableBox(canvas);
  await canvas.click({ position: { x: box.width * 0.3, y: box.height * 0.2 } });
  await expect(page.locator("#pages")).not.toHaveAttribute("data-object-mode", "selected");

  const parked = await page.evaluate(() => {
    const v = document.getElementById("viewport");
    v.scrollTop = v.scrollHeight - v.clientHeight;
    return v.scrollTop;
  });
  expect(parked, "the fixture must be scrollable for this to mean anything").toBeGreaterThan(80);

  await page.keyboard.insertText("X");
  await expect
    .poll(async () => await scrollTop(page), {
      message: "typing must bring the caret back into view",
    })
    .toBeLessThan(parked);
  expect(consoleErrors).toEqual([]);
});
