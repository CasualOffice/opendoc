import {
  test,
  expect,
  gotoEditor,
  clickIntoFirstPage,
  reflectedParagraphStyle,
} from "./fixtures.mjs";

test("style-driven text populates effective font, size, and format controls", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await expect.poll(() => reflectedParagraphStyle(page)).toBe("Heading 1");
  // The Styles trigger names it, so the user can see which style they are in
  // without opening anything (docs/115 §5.3).
  await expect(page.locator("#stylesTriggerLabel")).toHaveText("Heading 1");
  // The font family is now a dropdown trigger whose label reflects the effective
  // face; a populated label (not the "Font" placeholder) proves reflection.
  await expect(page.locator("#fontFamilyLabel")).not.toHaveText("Font");
  await expect(page.locator("#fontSize")).not.toHaveValue("");
  await expect(page.locator("#bold")).toHaveAttribute("aria-pressed", "true");
  expect(consoleErrors).toEqual([]);
});
