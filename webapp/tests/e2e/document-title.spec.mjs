// The browser tab must name the open document.
//
// `document.title` was assigned NOWHERE in `webapp/src`, so every editor tab
// read the same static string from `editor.html` and a user with three
// documents open had three identical tabs.
//
// A title assertion is easy to write so that it cannot fail — `toHaveTitle(/./)`
// passes on the static fallback, and so does any regex loose enough to match
// "OpenDoc". So every test here asserts the EXACT title, and the first one
// pins the before-and-after: the title must have CHANGED from what an editor
// with no document shows.
import { test, expect, gotoEditor, clickIntoFirstPage, moveCaretToDocStart } from "./fixtures.mjs";

/** What the tab reads with no document open — the static fallback in the HTML,
 *  read from the product rather than copied into the spec. */
async function noDocumentTitle(page) {
  await page.goto("/editor.html?blank=1");
  await page.waitForFunction(() => document.getElementById("status")?.textContent !== "Loading engine…", null, {
    timeout: 45_000,
  });
  return page.title();
}

test("the tab names the document, and says so only once one is open", async ({ page }) => {
  const fallback = await noDocumentTitle(page);
  expect(fallback).toContain("OpenDoc Editor");
  expect(fallback).not.toContain("opendoc-demo.docx");
  // No document is open — the header's own name field is empty — so a filename
  // in the tab would be naming something that does not exist.
  await expect(page.locator("#docTitle")).toHaveValue("");

  await gotoEditor(page);
  await expect(page).toHaveTitle("opendoc-demo.docx — OpenDoc");
  expect(await page.title()).not.toBe(fallback);
});

test("unsaved changes are marked in the tab, and saving clears the mark", async ({ page }) => {
  await gotoEditor(page);
  await expect(page).toHaveTitle("opendoc-demo.docx — OpenDoc");

  await clickIntoFirstPage(page);
  await moveCaretToDocStart(page);
  await page.keyboard.type("TITLEDIRTYMARKER");
  await expect(page.locator("#a11yDocument")).toContainText("TITLEDIRTYMARKER");
  await expect(page).toHaveTitle("• opendoc-demo.docx — OpenDoc");

  const download = page.waitForEvent("download");
  await page.locator('.app-menu-button[data-menu="file"]').click();
  await page.locator('#appMenuPopover .app-menu-item[data-command="file.save"]').click();
  await download;
  await expect(page).toHaveTitle("opendoc-demo.docx — OpenDoc");
});

test("renaming the document renames the tab", async ({ page }) => {
  await gotoEditor(page);
  const title = page.locator("#docTitle");
  await title.click();
  await title.fill("quarterly-report");
  await page.keyboard.press("Enter");

  // Rename changes what Save produces, which is unsaved work of its own — so
  // the mark belongs here too.
  await expect(page).toHaveTitle("• quarterly-report.docx — OpenDoc");
});

test("a new blank document names itself in the tab", async ({ page }) => {
  await gotoEditor(page);
  await page.locator('.app-menu-button[data-menu="file"]').click();
  await page.locator('#appMenuPopover .app-menu-item[data-command="file.new"]').click();
  await expect(page.locator("#docTitle")).toHaveValue("Untitled document.docx");
  await expect(page).toHaveTitle("Untitled document.docx — OpenDoc");
});
