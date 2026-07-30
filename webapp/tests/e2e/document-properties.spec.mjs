// Covers three previously-missing gaps the owner flagged: no way to rename a
// document, no document-properties (docProps/core.xml) view/edit, and no
// page-setup (size/margins/orientation) dialog. New engine ops:
// `Operation::SetCoreProperties` and `Operation::SetSectionGeometry`
// (casual-doc-edit), exposed as `documentProperties`/`setDocumentProperties`
// and `pageSetup`/`setPageSetup` (casual-doc-wasm), both JSON-bridge payloads
// mirroring the existing copyRichRuns/pasteRichRuns convention.
import { test, expect, gotoEditor, clickIntoFirstPage, MOD } from "./fixtures.mjs";

test("the public demo opens sample.docx and exposes its real saved metadata", async ({
  page,
  consoleErrors,
}) => {
  const docxRequests = [];
  page.on("request", (request) => {
    if (request.url().endsWith(".docx")) docxRequests.push(request.url());
  });

  await page.goto("/editor.html?demo=1");
  await page.waitForFunction(
    () =>
      document.body.classList.contains("doc-loaded") &&
      document.getElementById("status")?.textContent === "",
    null,
    { timeout: 45_000 },
  );

  await expect(page.locator("#docTitle")).toHaveValue("sample.docx");
  expect(docxRequests.some((url) => url.endsWith("/sample.docx"))).toBe(true);
  expect(docxRequests.some((url) => url.endsWith("/demo.docx"))).toBe(false);

  await page.locator("#propertiesBtn").click();
  await expect(page.locator("#propTitle")).toHaveValue("OpenDoc Feature Test Document");
  await expect(page.locator("#propCreator")).toHaveValue("CasualOffice");
  await expect(page.locator("#metaCreated")).toHaveAttribute(
    "title",
    "2013-12-23T23:15:00Z",
  );
  await expect(page.locator("#metaModified")).toHaveAttribute(
    "title",
    "2013-12-23T23:15:00Z",
  );
  await expect(page.locator("#metaApplication")).toHaveText("Microsoft Macintosh Word");
  await expect(page.locator("#metaAppVersion")).toHaveText("14.0000");
  expect(consoleErrors).toEqual([]);
});

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

  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  await expect((await download).suggestedFilename()).toBe("Board Notes.docx");

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

test("properties and page setup are keyboard-safe, mobile-bounded modal dialogs", async ({
  page,
  consoleErrors,
}) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoEditor(page);

  const propertiesBtn = page.locator("#propertiesBtn");
  await propertiesBtn.click();
  const propertiesDialog = page.locator("#propertiesPanel");
  await expect(propertiesDialog).toHaveAttribute("aria-modal", "true");
  await expect(page.locator("#propTitle")).toBeFocused();
  await page.locator("#propertiesClose").click();
  await expect(propertiesBtn).toBeFocused();

  await page.locator("#tabView").click();
  const pageSetupBtn = page.locator("#pageSetupBtn");
  await pageSetupBtn.click();
  const setupDialog = page.locator("#pageSetupMenu");
  await expect(setupDialog).toHaveAttribute("aria-modal", "true");
  await expect(page.locator('button[data-orientation="portrait"]')).toBeFocused();

  const cardBounds = await page.locator(".page-setup-dialog").evaluate((card) => {
    const rect = card.getBoundingClientRect();
    return {
      left: rect.left,
      top: rect.top,
      right: rect.right,
      bottom: rect.bottom,
      viewportWidth: window.innerWidth,
      viewportHeight: window.innerHeight,
    };
  });
  expect(cardBounds.left).toBeGreaterThanOrEqual(0);
  expect(cardBounds.top).toBeGreaterThanOrEqual(0);
  expect(cardBounds.right).toBeLessThanOrEqual(cardBounds.viewportWidth);
  expect(cardBounds.bottom).toBeLessThanOrEqual(cardBounds.viewportHeight);
  await expect(page.locator("#pageSetupApply")).toBeVisible();

  await page.locator("#pageWidth").fill("10");
  await expect(page.locator("#pagePreviewLabel")).toContainText("10 × 11.69 in");
  await page.keyboard.press("Escape");
  await expect(setupDialog).toBeHidden();
  await expect(pageSetupBtn).toBeFocused();
  expect(consoleErrors).toEqual([]);
});
