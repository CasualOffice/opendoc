// Scrolling and page-canvas virtualization are geometry changes, not editing-
// context transitions. A repeated header/footer remains the same model story,
// while its visible caret projection follows the page the user is viewing
// (docs/58, P1G-CONTEXT-05).
import { test, expect, documentPageCount, gotoEditor, pageSheet } from "./fixtures.mjs";
import { makeLargeDocx } from "./large-docx.mjs";

async function openVirtualizedDocument(page, pageCount = 12) {
  await page.setViewportSize({ width: 1280, height: 900 });
  await gotoEditor(page);
  await page.setInputFiles("#file", {
    name: "running-virtualization.docx",
    mimeType: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    buffer: Buffer.from(makeLargeDocx(pageCount)),
  });
  await page.waitForFunction(
    () => {
      const status = document.getElementById("status");
      return (
        status?.textContent === "" &&
        !status.classList.contains("error") &&
        document.querySelectorAll(".page-wrap").length > 0 &&
        document.querySelectorAll("canvas.page").length > 0
      );
    },
    null,
    { timeout: 45_000 },
  );
  // Every page of the document exists for the HOST — the page count is the
  // document's — while only the pages near the viewport have sheets.
  await expect.poll(() => documentPageCount(page)).toBe(pageCount);
}

async function enterRunningBand(page, band) {
  await page.locator('[data-tab="insert"]').click();
  await page.locator(band === "header" ? "#insertHeaderBtn" : "#insertFooterBtn").click();
  await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", band);
  await expect(page.locator(".overlay .caret")).toHaveCount(1);
}

for (const band of ["header", "footer"]) {
  test(`a ${band} caret follows a distant virtualized page without leaving the story`, async ({
    page,
    consoleErrors,
  }) => {
    await openVirtualizedDocument(page);
    const bodyBefore = await page.locator("#a11yDocument").textContent();
    await enterRunningBand(page, band);
    await page.keyboard.type("BEFORE_SCROLL");
    expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);

    const first = page.locator('.page-wrap[data-page-number="1"]');
    const target = page.locator('.page-wrap[data-page-number="10"]');
    await expect.poll(() => first.locator("canvas.page").count()).toBe(1);
    // Page 10 has no sheet at all yet — the stronger form of "no raster".
    await expect.poll(() => target.count()).toBe(0);
    await pageSheet(page, 10);

    // Prove the exercise crossed the actual virtualization boundary: the first
    // page's sheet and raster are released and the distant page's are built.
    await expect.poll(() => first.locator("canvas.page").count()).toBe(0);
    await expect.poll(() => target.locator("canvas.page").count()).toBe(1);

    await expect(page.locator("#pages")).toHaveAttribute("data-running-edit", band);
    await expect(page.locator("body")).toHaveClass(/running-edit/);
    await expect(target.locator(".running-band")).toBeVisible();
    await expect(target.locator(".overlay .caret")).toBeVisible();
    await expect(first.locator(".overlay .caret")).toHaveCount(0);

    const beforeTyping = await page.locator("#viewport").evaluate((element) => element.scrollTop);
    const targetHeight = await target.evaluate((element) => element.getBoundingClientRect().height);
    await page.keyboard.type("AFTER_SCROLL");
    await expect(page.locator("#undoBtn")).toHaveAttribute("aria-label", "Undo Typing");
    await page.waitForTimeout(250);
    const afterTyping = await page.locator("#viewport").evaluate((element) => element.scrollTop);

    expect(
      Math.abs(afterTyping - beforeTyping),
      "typing must not jump back to the old page",
    ).toBeLessThan(targetHeight);
    await expect.poll(() => first.locator("canvas.page").count()).toBe(0);
    await expect.poll(() => target.locator("canvas.page").count()).toBe(1);
    expect(await page.locator("#a11yDocument").textContent()).toBe(bodyBefore);
    expect(consoleErrors).toEqual([]);
  });
}
