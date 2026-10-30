// The accessibility checker (`docs/109` HF-064): Word's Review ▸ Check
// Accessibility, from the band, the Review menu and the palette, over what a
// screen reader is actually handed.
//
// Each test reads FINDINGS, not chrome: a dialog that opened empty on a
// document with a header-less table would pass a spec that only looked for the
// dialog, which is the false green `SKILL` §4 names.
import { test, expect, clickIntoFirstPage, runAppMenuCommand, openCommandPalette } from "./fixtures.mjs";

async function blankDocumentWithText(page, text) {
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(
    () => document.getElementById("status")?.textContent.startsWith("Ready — open"),
    null,
    { timeout: 45_000 },
  );
  await runAppMenuCommand(page, "file", "file.new");
  await expect(page.locator(".page-wrap")).not.toHaveCount(0, { timeout: 45_000 });
  await clickIntoFirstPage(page);
  await page.locator("#pages").pressSequentially(text);
}

const dialog = (page) => page.locator("#a11yCheckDialog");

test("a clean document says so, from the Review band", async ({ page, consoleErrors }) => {
  await blankDocumentWithText(page, "Plain prose.");
  await page.locator('[data-tab="review"]').click();
  await page.locator("#reviewA11yCheckBtn").click();
  await expect(dialog(page)).toBeVisible();
  await expect(page.locator("#a11yCheckSummary")).toHaveText("No accessibility issues found.");
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toBeHidden();
  // Focus came back to the band button that opened it.
  await expect(page.locator("#reviewA11yCheckBtn")).toBeFocused();
  expect(consoleErrors).toEqual([]);
});

test("a table without a header row is an error with a Go to, from the menu and the palette", async ({
  page,
  consoleErrors,
}) => {
  await blankDocumentWithText(page, "Results");
  await page.keyboard.press("Enter");
  await page.locator('[data-tab="insert"]').click();
  await page.locator("#insertTableBtn").click();
  await page.locator('.gc[data-r="2"][data-c="2"]').click();

  await runAppMenuCommand(page, "review", "tools.checkAccessibility");
  await expect(dialog(page)).toBeVisible();
  await expect(page.locator("#a11yCheckSummary")).toHaveText(/Errors: 1 · Warnings: \d+/);
  const header = page.locator(".a11y-check-item.is-error", { hasText: "Missing table header" });
  await expect(header).toHaveCount(1);
  await expect(page.locator(".a11y-check-item", { hasText: "Blank table row" })).toHaveCount(1);
  // A table finding has a Go to: the caret goes to its first cell.
  await header.getByRole("button").click();
  await expect(dialog(page)).toBeHidden();
  await expect(page.locator("#tabTable")).toBeEnabled();

  await openCommandPalette(page);
  await page.locator("#cmdInput").fill("accessibility");
  await page.locator('.cmd-item[data-command-id="tools.checkAccessibility"]').click();
  await expect(dialog(page)).toBeVisible();
  await expect(header).toHaveCount(1);
  expect(consoleErrors).toEqual([]);
});
