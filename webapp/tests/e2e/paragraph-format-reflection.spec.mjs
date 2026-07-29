import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

test("style-driven text populates effective font, size, and format controls", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await expect(page.locator("#paragraphStyle")).toHaveValue("Heading 1");
  await expect(page.locator("#fontFamily")).not.toHaveValue("");
  await expect(page.locator("#fontSize")).not.toHaveValue("");
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "true");
  expect(consoleErrors).toEqual([]);
});
