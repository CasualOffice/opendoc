// Never a modal over a modal (`docs/184` D1a; the owner, 2026-11-01: "popup
// over a popup — who designed it"). A dialog opened from a dialog takes its
// place: the first is hidden, keeping its state, and comes back with focus on
// the control that opened the second when the second closes. `modal.mjs` does
// it once for every registered dialog; this holds it through two real routes,
// Settings ▸ AutoCorrect options… and Settings ▸ an add-in that asks for
// permission.
import { expect, gotoEditor, test } from "./fixtures.mjs";

const visibleOverlays = (page) =>
  page.locator(".dialog-overlay").evaluateAll((els) => els.filter((el) => !el.hidden && el.getClientRects().length > 0).map((el) => el.id));

test("a dialog opened from Settings takes its place, and Settings comes back", async ({ page, consoleErrors }) => {
  await gotoEditor(page);
  await page.locator("#settingsBtn").click();
  await page.locator('#settingsPanel [data-settings-tab="autocorrect"]').click();
  await page.locator("#settingsAutoCorrectOptions").click();
  await expect(page.locator("#autoCorrectDialog")).toBeVisible();
  expect(await visibleOverlays(page)).toEqual(["autoCorrectDialog"]);

  await page.keyboard.press("Escape");
  await expect(page.locator("#autoCorrectDialog")).toBeHidden();
  expect(await visibleOverlays(page)).toEqual(["settingsPanel"]);
  // Its page and its place are as they were.
  await expect(page.locator('#settingsPanel [data-settings-tab="autocorrect"]')).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#settingsAutoCorrectOptions")).toBeFocused();

  await page.keyboard.press("Escape");
  expect(await visibleOverlays(page)).toEqual([]);
  expect(consoleErrors).toEqual([]);
});

test("turning on an add-in in Settings asks for permission in Settings' place", async ({ page }) => {
  await gotoEditor(page);
  await page.locator("#settingsBtn").click();
  await page.locator('#settingsPanel [data-settings-tab="ai"]').click();
  const toggle = page.locator('#settingsAddinsList [data-plugin-switch="org.casualoffice.languagetool"]');
  await expect(toggle).toBeVisible();
  await expect(toggle).not.toBeChecked();
  await toggle.click();
  await expect(page.locator("#pluginGrantDialog")).toBeVisible();
  expect(await visibleOverlays(page)).toEqual(["pluginGrantDialog"]);
  await page.locator("#pluginGrantAllow").click();
  await expect(page.locator("#pluginGrantDialog")).toBeHidden();
  expect(await visibleOverlays(page)).toEqual(["settingsPanel"]);
  await expect(toggle).toBeChecked();
});
