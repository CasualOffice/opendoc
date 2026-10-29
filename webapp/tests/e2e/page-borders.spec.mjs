// Page borders (HF-193): add, change and remove one, from two surfaces.
//
// Every assertion reads the ENGINE back through the dialog: Apply writes the
// section's `w:pgBorders`, and reopening paints the fields from `pageBorders`,
// so a dialog that wrote nothing — or wrote the wrong field — reopens on the
// defaults and fails here.
import { test, expect, clickIntoFirstPage, runAppMenuCommand, MOD } from "./fixtures.mjs";

async function blankDocument(page) {
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(
    () => document.getElementById("status")?.textContent.startsWith("Ready — open"),
    null,
    { timeout: 45_000 },
  );
  await runAppMenuCommand(page, "file", "file.new");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
  await clickIntoFirstPage(page);
  await page.locator("#pages").pressSequentially("Bordered");
}

const dialog = (page) => page.locator("#pageBordersDialog");

async function openFromBand(page) {
  await page.locator('[data-tab="layout"]').click();
  await expect(page.locator("#pageBordersBtn")).toBeEnabled();
  await page.locator("#pageBordersBtn").click();
  await expect(dialog(page)).toBeVisible();
}

test("a page border is added, changed and removed, and reads back from the engine", async ({
  page,
  consoleErrors,
}) => {
  await blankDocument(page);

  // ADD: a section without a border opens on a box, all four edges on.
  await openFromBand(page);
  await expect(page.locator("#pageBordersTop")).toBeFocused();
  for (const id of ["#pageBordersTop", "#pageBordersBottom", "#pageBordersLeft", "#pageBordersRight"]) {
    await expect(page.locator(id)).toBeChecked();
  }
  await page.locator("#pageBordersStyle").selectOption("dotted");
  await page.locator("#pageBordersWidth").selectOption("24");
  await page.locator("#pageBordersBottom").uncheck();
  await page.locator("#pageBordersApply").click();
  await expect(dialog(page)).toBeHidden();
  await expect(page.locator("#status")).toContainText("Page borders applied");

  // CHANGE: reopening paints the engine's answer, not the last form.
  await openFromBand(page);
  await expect(page.locator("#pageBordersStyle")).toHaveValue("dotted");
  await expect(page.locator("#pageBordersWidth")).toHaveValue("24");
  await expect(page.locator("#pageBordersBottom")).not.toBeChecked();
  await expect(page.locator("#pageBordersTop")).toBeChecked();

  // An out-of-range spacing is refused at the field and nothing is written.
  await page.locator("#pageBordersSpacing").fill("99");
  await page.locator("#pageBordersApply").click();
  await expect(dialog(page)).toBeVisible();
  await page.locator("#pageBordersSpacing").fill("12");

  // REMOVE: no edge checked is the empty value.
  for (const id of ["#pageBordersTop", "#pageBordersLeft", "#pageBordersRight"]) {
    await page.locator(id).uncheck();
  }
  await page.locator("#pageBordersApply").click();
  await expect(page.locator("#status")).toContainText("Page borders removed");
  await openFromBand(page);
  // Back to the no-border opening state: a box on all four edges, single line.
  await expect(page.locator("#pageBordersStyle")).toHaveValue("single");
  await expect(page.locator("#pageBordersBottom")).toBeChecked();
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("the palette opens the same dialog, and Undo takes the border off", async ({
  page,
  consoleErrors,
}) => {
  await blankDocument(page);
  await page.keyboard.press(`${MOD}+Shift+P`);
  await page.locator("#cmdInput").fill("Page borders");
  await page.locator('#cmdList [data-command-id="layout.pageBorders"]').click();
  await expect(dialog(page)).toBeVisible();
  await page.locator("#pageBordersStyle").selectOption("double");
  await page.locator("#pageBordersApply").click();
  await expect(page.locator("#status")).toContainText("Page borders applied");

  await clickIntoFirstPage(page);
  await page.keyboard.press(`${MOD}+z`);
  await openFromBand(page);
  await expect(page.locator("#pageBordersStyle")).toHaveValue("single");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});
