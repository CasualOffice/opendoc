// Covers three previously-missing gaps the owner flagged: no way to rename a
// document, no document-properties (docProps/core.xml) view/edit, and no
// page-setup (size/margins/orientation) dialog. New engine ops:
// `Operation::SetCoreProperties` and `Operation::SetSectionGeometry`
// (casual-doc-edit), exposed as `documentProperties`/`setDocumentProperties`
// and `pageSetup`/`setPageSetup` (casual-doc-wasm), both JSON-bridge payloads
// mirroring the existing copyRichRuns/pasteRichRuns convention.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

test("the header title renames the document and Save reflects the new name", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  const title = page.locator("#docTitle");
  await expect(title).toHaveValue("opendoc-demo.docx");

  await title.click();
  await page.keyboard.press(`${MOD}+a`);
  await page.keyboard.type("Quarterly Review");
  await page.keyboard.press("Enter");
  await expect(title).toHaveValue("Quarterly Review.docx");

  // A name with no extension gets .docx appended; blank reverts to the
  // current name rather than leaving an empty title.
  await title.click();
  await page.keyboard.press(`${MOD}+a`);
  await page.keyboard.type("Board Notes.docx");
  await page.keyboard.press("Enter");
  await expect(title).toHaveValue("Board Notes.docx");

  await title.click();
  await page.keyboard.press(`${MOD}+a`);
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Enter");
  await expect(title).toHaveValue("Board Notes.docx");

  expect(consoleErrors).toEqual([]);
});

test("document properties open, apply as one undoable action, and reflect back", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#propertiesBtn").click();
  await expect(page.locator("#propertiesPanel")).toBeVisible();
  // A freshly-imported fixture carries no core properties yet.
  await expect(page.locator("#propTitle")).toHaveValue("");

  await page.locator("#propTitle").fill("Q3 Board Report");
  await page.locator("#propCreator").fill("Ada Lovelace");
  await page.locator("#propSubject").fill("Quarterly financials");
  await page.locator("#propertiesApply").click();
  await expect(page.locator("#propertiesPanel")).toBeHidden();

  await page.locator("#propertiesBtn").click();
  await expect(page.locator("#propTitle")).toHaveValue("Q3 Board Report");
  await expect(page.locator("#propCreator")).toHaveValue("Ada Lovelace");
  await expect(page.locator("#propSubject")).toHaveValue("Quarterly financials");
  await page.keyboard.press("Escape");

  await page.locator("#undoBtn").click();
  await page.locator("#propertiesBtn").click();
  await expect(page.locator("#propTitle")).toHaveValue("");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("cancel discards edited property fields without applying them", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#propertiesBtn").click();
  await page.locator("#propTitle").fill("Draft — do not ship");
  await page.locator("#propertiesCancel").click();
  await expect(page.locator("#propertiesPanel")).toBeHidden();

  await page.locator("#propertiesBtn").click();
  await expect(page.locator("#propTitle")).toHaveValue("");
});

test("page setup reflects real geometry, applies margins/size, and undoes", async ({
  page,
  consoleErrors,
}) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#tabView").click();
  await page.locator("#pageSetupBtn").click();
  await expect(page.locator("#pageSetupMenu")).toBeVisible();
  // The fixture is A4 with ~1cm (0.39in) margins.
  await expect(page.locator("#pageWidth")).toHaveValue("8.27");
  await expect(page.locator("#pageHeight")).toHaveValue("11.69");
  await expect(page.locator("#pageMarginTop")).toHaveValue("0.39");

  await page.locator("#pageMarginTop").fill("2");
  await page.locator("#pageMarginBottom").fill("2");
  await page.locator("#pageSetupApply").click();
  await expect(page.locator("#pageSetupMenu")).toBeHidden();

  await page.locator("#pageSetupBtn").click();
  await expect(page.locator("#pageMarginTop")).toHaveValue("2");
  await expect(page.locator("#pageMarginBottom")).toHaveValue("2");
  await page.keyboard.press("Escape");

  await page.locator("#tabHome").click();
  await page.locator("#undoBtn").click();
  await page.locator("#tabView").click();
  await page.locator("#pageSetupBtn").click();
  await expect(page.locator("#pageMarginTop")).toHaveValue("0.39");
  await page.keyboard.press("Escape");

  expect(consoleErrors).toEqual([]);
});

test("the orientation toggle swaps width and height without applying yet", async ({ page }) => {
  await gotoEditor(page);
  await clickIntoFirstPage(page);

  await page.locator("#tabView").click();
  await page.locator("#pageSetupBtn").click();
  await expect(page.locator("#pageWidth")).toHaveValue("8.27");

  await page.locator('[data-orientation="landscape"]').click();
  await expect(page.locator("#pageWidth")).toHaveValue("11.69");
  await expect(page.locator("#pageHeight")).toHaveValue("8.27");

  // Cancel: nothing was applied, so reopening shows the original portrait size.
  await page.locator("#pageSetupCancel").click();
  await page.locator("#pageSetupBtn").click();
  await expect(page.locator("#pageWidth")).toHaveValue("8.27");
  await page.keyboard.press("Escape");
});
