import { test, expect, gotoEditor, clickIntoFirstPage } from "./fixtures.mjs";

test("style-driven text populates effective font, size, and format controls", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await expect(page.locator("#paragraphStyle")).toHaveValue("Heading 1");
  // The font family is now a dropdown trigger whose label reflects the effective
  // face; a populated label (not the "Font" placeholder) proves reflection.
  await expect(page.locator("#fontFamilyLabel")).not.toHaveText("Font");
  await expect(page.locator("#fontSize")).not.toHaveValue("");
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "true");
  expect(consoleErrors).toEqual([]);
});
