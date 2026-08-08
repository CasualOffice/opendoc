// Word's two running-content toggles: "Different First Page" and "Different Odd
// & Even Pages" (docs/85 Q6, decided in #462 — all three benchmark editors offer
// both, so there was no divergence to weigh).
//
// Each flag only says a variant APPLIES; the variant's content is a separate
// header/footer reference. So turning one on for a document that has no such
// variant shows an empty band until one is created — which is what Word does
// too, and why the tests assert the FLAG rather than any painted output.
import { test, expect, gotoEditor, clickIntoFirstPage, setReviewMode, MOD } from "./fixtures.mjs";

async function runCommand(page, label) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill(label);
  await page.locator("#cmdList .cmd-item", { hasText: label }).first().click();
  await expect(page.locator("#cmdPalette")).toBeHidden();
}

async function commandLabelExists(page, label) {
  await page.keyboard.press(`${MOD}+Shift+P`);
  await expect(page.locator("#cmdPalette")).toBeVisible();
  await page.locator("#cmdInput").fill(label);
  const found = await page.locator("#cmdList .cmd-item", { hasText: label }).count();
  await page.keyboard.press("Escape");
  return found > 0;
}

test("Different first page toggles, reports its state, and undoes", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  // The label reports the state, read from the engine rather than a local flag,
  // so it can never drift from the document.
  expect(await commandLabelExists(page, "Different first page: off")).toBe(true);

  await runCommand(page, "Different first page: off");
  await expect(page.locator("#status")).toContainText("Different first page on");
  expect(await commandLabelExists(page, "Different first page: on")).toBe(true);

  // One undoable action.
  await page.keyboard.press(`${MOD}+z`);
  await expect.poll(() => commandLabelExists(page, "Different first page: off")).toBe(true);

  expect(consoleErrors).toEqual([]);
});

test("Different odd & even pages toggles independently of the first-page flag", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await runCommand(page, "Different odd & even pages: off");
  await expect(page.locator("#status")).toContainText("Different odd & even pages on");

  // The two are separate settings — Word keeps them independent, and OOXML puts
  // one on the section and the other in document settings.
  expect(await commandLabelExists(page, "Different first page: off")).toBe(true);
  expect(await commandLabelExists(page, "Different odd & even pages: on")).toBe(true);

  expect(consoleErrors).toEqual([]);
});

test("the toggles fail closed in Viewing", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);
  await setReviewMode(page, "viewing");

  await runCommand(page, "Different first page: off");
  await expect(page.locator("#status")).toContainText("read-only");
  // Nothing changed and nothing entered history.
  expect(await commandLabelExists(page, "Different first page: off")).toBe(true);
  await expect(page.locator("#undoBtn")).toBeDisabled();

  expect(consoleErrors).toEqual([]);
});
