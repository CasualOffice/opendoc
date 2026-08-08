// Smart quotes, as Word and Docs do them.
//
// Both products replace the typewriter quotes as you type, so documents from
// either arrive full of curly quotes. Typing into one used to introduce straight
// quotes beside them — visibly different glyphs in the same sentence — because
// the editor had no autocorrection of any kind.
//
// The whole feature is a decision about the character BEFORE the caret, so that
// is what these tests exercise: opening after nothing/whitespace/brackets,
// closing otherwise, and the apostrophe case ("don't") that makes closing the
// right default for the ambiguous position.
import { test, expect, gotoEditor, clickIntoFirstPage, setReviewMode, MOD } from "./fixtures.mjs";

// Types into an empty paragraph of its own, so each case is judged only on what
// this test typed rather than on whatever the fixture's paragraph already held.
async function typeInFreshParagraph(page, text) {
  await clickIntoFirstPage(page);
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type(text);
}

const paragraphText = (page, contains) =>
  page.evaluate((needle) => {
    const el = [...document.querySelectorAll("#a11yDocument p, #a11yDocument h1, #a11yDocument h2")]
      .find((node) => (node.textContent || "").includes(needle));
    return el ? el.textContent : null;
  }, contains);

test("a quote opens at the start of a paragraph and closes after a word", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await typeInFreshParagraph(page, '"Hello"');

  // Opening at paragraph start, closing after "Hello" — the two ends differ.
  await expect.poll(() => paragraphText(page, "Hello")).toBe("“Hello”");

  expect(consoleErrors).toEqual([]);
});

test("an apostrophe inside a word closes, so contractions are correct", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await typeInFreshParagraph(page, "don't");

  // The ambiguous position defaults to closing precisely for this case, which is
  // far more common in prose than a leading elision.
  await expect.poll(() => paragraphText(page, "don")).toBe("don’t");

  expect(consoleErrors).toEqual([]);
});

test("a quote after whitespace or a bracket opens", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await typeInFreshParagraph(page, 'said "yes" and ("no")');

  await expect
    .poll(() => paragraphText(page, "said"))
    .toBe("said “yes” and (“no”)");

  expect(consoleErrors).toEqual([]);
});

test("smart quotes can be turned off, and then a straight quote stays straight", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);

  // Word and Docs both let this be switched off; typing code is the reason.
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Smart quotes");
  const row = page.locator("#cmdList .cmd-item", { hasText: "Smart quotes: on" }).first();
  await expect(row).toBeVisible();
  await row.click();
  await expect(page.locator("#status")).toContainText("Smart quotes off");

  await typeInFreshParagraph(page, 'const x = "y"');
  await expect.poll(() => paragraphText(page, "const x")).toBe('const x = "y"');

  // The preference survives a reload — a setting that resets is not a setting.
  await page.reload();
  await page.waitForFunction(() => document.body.dataset.fontsReady === "true", null, {
    timeout: 45_000,
  });
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Smart quotes");
  await expect(
    page.locator("#cmdList .cmd-item", { hasText: "Smart quotes: off" }).first(),
  ).toBeVisible();

  expect(consoleErrors).toEqual([]);
});

test("the substitution rides the ordinary typing path: one undo, and read-only still refuses", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await typeInFreshParagraph(page, '"quoted"');
  await expect.poll(() => paragraphText(page, "quoted")).toBe("“quoted”");

  // Typing coalesces into one session, so the burst undoes as one action — the
  // substitution is an ordinary character insert, not a second edit on top.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => paragraphText(page, "quoted")).toBe(null);

  // And because it is the ordinary typing path, Viewing still fails closed: the
  // substitution cannot become a way to write into a read-only document.
  await setReviewMode(page, "viewing");
  await page.keyboard.type('"blocked"');
  await expect(page.locator("#status")).toContainText("read-only");
  await expect.poll(() => paragraphText(page, "blocked")).toBe(null);

  expect(consoleErrors).toEqual([]);
});
