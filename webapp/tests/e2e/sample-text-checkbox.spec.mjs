import { test, expect, MOD, stableBox } from "./fixtures.mjs";

test("the default sample's text checkboxes toggle by click and Space, undo and save", async ({ page }) => {
  // The checkbox face is self-hosted. Unavailable remote CJK/colour faces
  // must not prevent completing the sample's text checklist.
  await page.route("https://cdn.jsdelivr.net/**", route => route.abort());
  await page.goto("/editor.html");
  await page.waitForFunction(() => document.body.dataset.fontsReady === "true", null, { timeout: 45000 });
  await expect(page.locator("#a11yDocument")).toContainText("☐ Layout matches reference");
  await page.locator("#findBtn").click();
  await page.locator("#findInput").fill("☐ Layout matches reference");
  await expect(page.locator("#findStatus")).toContainText("1");
  await page.keyboard.press("Escape");
  await page.keyboard.press("ArrowLeft");
  const caret = page.locator(".overlay .caret").first();
  await expect(caret).toBeVisible();
  const rect = await stableBox(caret);
  await page.mouse.click(rect.x + 3, rect.y + rect.height / 2);
  await expect(page.locator("#a11yDocument")).toContainText("☒ Layout matches reference");
  await page.locator("#undoBtn").click();
  await expect(page.locator("#a11yDocument")).toContainText("☐ Layout matches reference");
  await page.locator("#pages").focus();
  await page.keyboard.press("Home");
  await page.keyboard.press("Space");
  await expect(page.locator("#a11yDocument")).toContainText("☒ Layout matches reference");
  const download = page.waitForEvent("download");
  await page.keyboard.press(`${MOD}+s`);
  const saved = await download;
  await page.locator("#file").setInputFiles(await saved.path());
  await expect(page.locator("#a11yDocument")).toContainText("☒ Layout matches reference");
});
