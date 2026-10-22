// docs/174 TA-03 and TA-04, driven through the real find box with real
// keystrokes.
//
// TA-03: typing the query used to step forward from the previous match on every
// keystroke, so "q", "qz", "qzx" ended on 3 or 4 of 4 and Replace changed a match
// the user never navigated to. Word, Docs and ONLYOFFICE keep the first match
// after where Find was opened, and advance only on Enter or Next.
//
// TA-04: only the current match was marked. Every match is now highlighted, the
// current one distinct.
import { MOD, clickIntoFirstPage, expect, gotoEditor, moveCaretToDocStart, test } from "./fixtures.mjs";

async function seed(page, text) {
  await page.evaluate((value) => {
    const data = new DataTransfer();
    data.setData("text/plain", value);
    document.dispatchEvent(new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }));
  }, text);
}

const mirrorText = (page) => page.locator("#a11yDocument").innerText();

test("typing the query stays on the first match after the caret, and Replace changes that one", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await seed(page, "qzx1 qzx2 qzx3 qzx4 ");
  await moveCaretToDocStart(page);

  await page.keyboard.press(`${MOD}+f`);
  const findInput = page.locator("#findInput");
  await expect(findInput).toBeFocused();
  await findInput.pressSequentially("qzx");
  await expect(page.locator("#findStatus")).toHaveText("1 of 4");

  // Deleting a character refines too; it does not move either.
  await findInput.press("Backspace");
  await expect(page.locator("#findStatus")).toHaveText("1 of 4");
  await findInput.pressSequentially("x");
  await expect(page.locator("#findStatus")).toHaveText("1 of 4");

  // Enter is the step.
  await findInput.press("Enter");
  await expect(page.locator("#findStatus")).toHaveText("2 of 4");

  await page.locator("#replaceInput").fill("CAT");
  await page.locator("#replaceOne").click();
  await expect.poll(() => mirrorText(page)).toContain("qzx1 CAT2 qzx3 qzx4");
  // Replace moves on to the match AFTER the replacement, not back to the top.
  await expect(page.locator("#findStatus")).toHaveText("2 of 3");
  await page.locator("#replaceOne").click();
  await expect.poll(() => mirrorText(page)).toContain("qzx1 CAT2 CAT3 qzx4");

  await page.keyboard.press("Escape");
  expect(consoleErrors).toEqual([]);
});

test("every match is highlighted, the current one distinct, and the marks go when Find closes", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await seed(page, "qzx1 qzx2 qzx3 qzx4 ");
  await moveCaretToDocStart(page);

  await page.keyboard.press(`${MOD}+f`);
  await page.locator("#findInput").pressSequentially("qzx");
  await expect(page.locator("#findStatus")).toHaveText("1 of 4");

  const marks = page.locator(".overlay .find-match");
  const current = page.locator(".overlay .find-match-current");
  await expect(marks).toHaveCount(4);
  await expect(current).toHaveCount(1);
  // The current mark is the selected match, not merely one of the four.
  const overlaps = async () =>
    page.evaluate(() => {
      const a = document.querySelector(".overlay .find-match-current").getBoundingClientRect();
      const b = document.querySelector(".overlay .highlight").getBoundingClientRect();
      return Math.abs(a.left - b.left) < 1 && Math.abs(a.top - b.top) < 1;
    });
  expect(await overlaps()).toBe(true);
  const firstLeft = await current.evaluate((el) => el.getBoundingClientRect().left);

  await page.locator("#findNext").click();
  await expect(page.locator("#findStatus")).toHaveText("2 of 4");
  await expect(current).toHaveCount(1);
  expect(await current.evaluate((el) => el.getBoundingClientRect().left)).toBeGreaterThan(firstLeft);
  expect(await overlaps()).toBe(true);

  // A query with no match clears every mark.
  await page.locator("#findInput").fill("zz_no_such_text_zz");
  await expect(page.locator("#findStatus")).toHaveText("No match");
  await expect(marks).toHaveCount(0);

  await page.locator("#findInput").fill("qzx");
  await expect(marks).toHaveCount(4);
  await page.keyboard.press("Escape");
  await expect(page.locator("#findPanel")).toBeHidden();
  await expect(marks).toHaveCount(0);
  expect(consoleErrors).toEqual([]);
});
