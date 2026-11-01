// The PDF layout switch (`docs/109` UX-051, owner 2026-11-01): a status-bar
// chip, shown only for a PDF session, names the layout and switches it, and
// View ▸ Page display carries the same two radio rows. The switch is symmetric:
// either direction reopens the PDF the session kept, so a reader can go back.
import {
  closeAppMenu,
  expect,
  gotoEditor,
  menuCommandRow,
  runAppMenuCommand,
  test,
  useCompactChrome,
} from "./fixtures.mjs";
import { PDF_FIXTURE } from "./pdf-import-helpers.mjs";

const NAME = "synthetic-prose.pdf";
const chip = (page) => page.locator("#pdfLayoutStatus");
const findings = (page) => page.locator("#compatibilityFindingsDialog");

/** Waits for a (re)open of the PDF to finish and dismisses its summary. */
async function settled(page) {
  await expect(findings(page)).toBeVisible({ timeout: 60_000 });
  await page.locator("#compatibilityFindingsDialogDone").click();
  await expect(findings(page)).toBeHidden();
}

async function openPdf(page) {
  await page.locator("#file").setInputFiles(PDF_FIXTURE);
  await expect.poll(() => page.locator("#docTitle").inputValue(), { timeout: 60_000 }).toBe(NAME);
  await settled(page);
}

test.describe("the PDF layout switch", () => {
  test.setTimeout(150_000);

  test("is absent for a document that is not a PDF, and View says why its rows are off", async ({
    page,
    consoleErrors,
  }) => {
    await gotoEditor(page);
    await expect(chip(page)).toBeHidden();
    await useCompactChrome(page);
    for (const id of ["view.pdfLayout.flowing", "view.pdfLayout.exact"]) {
      const row = await menuCommandRow(page, "view", id);
      await expect(row).toBeDisabled();
      await expect(row).toHaveAttribute("role", "menuitemradio");
      await expect(row).toHaveAttribute("aria-checked", "false");
      await expect(row).toHaveAttribute("title", /opened from a PDF/);
      await closeAppMenu(page);
    }
    expect(consoleErrors).toEqual([]);
  });

  // Mutation: make the chip's reflect() leave `hidden` set and the first
  // assertion times out; make the Flowing row run "Convert to flowing
  // document" and the last chip assertion fails, because a converted document
  // keeps no PDF and the chip disappears.
  test("the chip names the layout, switches it from the keyboard, and switches back", async ({
    page,
    consoleErrors,
  }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await gotoEditor(page);
    await openPdf(page);

    // A PDF opens flowing, and the chip says so — to the eye and to a reader.
    await expect(chip(page)).toBeVisible();
    await expect(chip(page)).toContainText("Flowing");
    await expect(chip(page)).toHaveAttribute("aria-label", "PDF layout: Flowing. Choose a layout");
    await expect(chip(page)).toHaveAttribute("aria-haspopup", "menu");

    // Keyboard: Enter opens the menu ON the checked row; ArrowDown walks to the
    // other; Enter switches.
    await chip(page).focus();
    await page.keyboard.press("Enter");
    const menu = page.locator("#pdfLayoutMenu");
    await expect(menu).toBeVisible();
    await expect(chip(page)).toHaveAttribute("aria-expanded", "true");
    const flowingRow = menu.locator('[data-command="view.pdfLayout.flowing"]');
    const exactRow = menu.locator('[data-command="view.pdfLayout.exact"]');
    await expect(flowingRow).toHaveAttribute("aria-checked", "true");
    await expect(exactRow).toHaveAttribute("aria-checked", "false");
    await expect(flowingRow).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(exactRow).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(menu).toBeHidden();
    await settled(page);
    await expect(chip(page)).toContainText("Exact layout");
    await expect.poll(() => page.locator(".page-wrap").count()).toBe(2);

    // View ▸ Page display reflects it, checked.
    await useCompactChrome(page);
    const viewExact = await menuCommandRow(page, "view", "view.pdfLayout.exact");
    await expect(viewExact).toHaveAttribute("aria-checked", "true");
    await expect(viewExact).toBeEnabled();
    await closeAppMenu(page);

    // And back, from View this time: the session is still a PDF, so the chip
    // stays and says Flowing.
    await runAppMenuCommand(page, "view", "view.pdfLayout.flowing");
    await settled(page);
    await expect(chip(page)).toBeVisible();
    await expect(chip(page)).toContainText("Flowing");
    await expect(page.locator("#docTitle")).toHaveValue(NAME);
    expect(consoleErrors).toEqual([]);
  });
});
